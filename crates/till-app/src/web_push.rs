//! This device's Web Push keys, and opening what a sender encrypted to them
//! (RFC 8291, `aes128gcm`): on iOS the relay carries a ding as an APNs
//! alert with the ciphertext beside `aps` (design.md §5, "The relay API"),
//! and the core opens it, in the app and in the Notification Service
//! Extension alike, so neither has crypto of its own.
//!
//! The keys are kept through [`Platform`]'s secrets under
//! [`keys::PUSH_KEYS`], in the Keychain group the extension shares. Only
//! the public half and the auth secret ever leave the device, in the
//! subscription a sender is given; the private key never does.

use std::sync::Arc;

use aws_lc_rs::aead::{AES_128_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use aws_lc_rs::agreement::{self, ECDH_P256, PrivateKey, UnparsedPublicKey};
use aws_lc_rs::encoding::{AsBigEndian, EcPrivateKeyBin};
use aws_lc_rs::hkdf::{HKDF_SHA256, KeyType, Salt};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use crate::store::{Store, keys};
use crate::{Platform, SecretError};

/// A P-256 public key, uncompressed (RFC 8291 §3.1).
const PUBLIC_KEY_LEN: usize = 65;
const AUTH_LEN: usize = 16;
const SALT_LEN: usize = 16;
/// The record's AES-GCM tag.
const TAG_LEN: usize = 16;
/// Padding ends with this byte in the last record (RFC 8188 §2).
const LAST_RECORD: u8 = 2;

/// The device's half of a Web Push subscription: its ECDH key and the auth
/// secret a sender mixes in.
pub(crate) struct PushKeys {
    private: PrivateKey,
    auth: [u8; AUTH_LEN],
}

/// How the keys are kept: base64url, the private key as its 32 big-endian
/// bytes.
#[derive(Serialize, Deserialize)]
struct KeptKeys {
    private: String,
    auth: String,
}

impl PushKeys {
    /// New keys, from the system's random source.
    pub fn generate() -> Option<PushKeys> {
        let private = PrivateKey::generate(&ECDH_P256).ok()?;
        let mut auth = [0; AUTH_LEN];
        aws_lc_rs::rand::fill(&mut auth).ok()?;
        Some(PushKeys { private, auth })
    }

    fn from_parts(private: &[u8], auth: &[u8]) -> Option<PushKeys> {
        Some(PushKeys {
            private: PrivateKey::from_private_key(&ECDH_P256, private).ok()?,
            auth: auth.try_into().ok()?,
        })
    }

    /// The keys kept on this device; `None` when none are, or what is kept
    /// doesn't read as keys.
    pub fn load(store: &Store) -> Result<Option<PushKeys>, SecretError> {
        let Some(json) = store.get(keys::PUSH_KEYS)? else {
            return Ok(None);
        };
        let Ok(kept) = serde_json::from_str::<KeptKeys>(&json) else {
            return Ok(None);
        };
        let (Ok(private), Ok(auth)) = (
            URL_SAFE_NO_PAD.decode(kept.private),
            URL_SAFE_NO_PAD.decode(kept.auth),
        ) else {
            return Ok(None);
        };
        Ok(PushKeys::from_parts(&private, &auth))
    }

    /// The keys kept on this device, made and kept first if there are none.
    /// `None` when new keys couldn't be made.
    pub fn load_or_make(store: &Store) -> Result<Option<PushKeys>, SecretError> {
        if let Some(keys) = PushKeys::load(store)? {
            return Ok(Some(keys));
        }
        let Some(keys) = PushKeys::generate() else {
            return Ok(None);
        };
        let Ok(private) = AsBigEndian::<EcPrivateKeyBin>::as_be_bytes(&keys.private) else {
            return Ok(None);
        };
        let kept = KeptKeys {
            private: URL_SAFE_NO_PAD.encode(private.as_ref()),
            auth: URL_SAFE_NO_PAD.encode(keys.auth),
        };
        let json = serde_json::to_string(&kept).expect("always JSON");
        store.put(keys::PUSH_KEYS, Some(&json))?;
        Ok(Some(keys))
    }

    /// The public key, uncompressed, base64url: a subscription's `p256dh`.
    pub fn p256dh(&self) -> Option<String> {
        let public = self.private.compute_public_key().ok()?;
        Some(URL_SAFE_NO_PAD.encode(public.as_ref()))
    }

    /// The auth secret, base64url: a subscription's `auth`.
    pub fn auth(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.auth)
    }

    /// The plaintext of `body`, an `aes128gcm` message of one record
    /// encrypted to these keys (RFC 8291 §3.4, RFC 8188 §2); `None` when it
    /// isn't one, or wasn't encrypted to these keys.
    pub fn open(&self, body: &[u8]) -> Option<Vec<u8>> {
        // salt (16) | record size (4) | key id length (1) | key id | record
        let salt = body.get(..SALT_LEN)?;
        let id_len = usize::from(*body.get(SALT_LEN + 4)?);
        if id_len != PUBLIC_KEY_LEN {
            return None;
        }
        let sender = body.get(SALT_LEN + 5..SALT_LEN + 5 + id_len)?;
        let record = body.get(SALT_LEN + 5 + id_len..)?;
        if record.len() <= TAG_LEN {
            return None;
        }
        let receiver = self.private.compute_public_key().ok()?;
        let ecdh = agreement::agree(
            &self.private,
            UnparsedPublicKey::new(&ECDH_P256, sender),
            (),
            |secret| Ok(secret.to_vec()),
        )
        .ok()?;
        let (key, nonce) = content_key(&ecdh, &self.auth, receiver.as_ref(), sender, salt)?;
        let key = LessSafeKey::new(UnboundKey::new(&AES_128_GCM, &key).ok()?);
        let mut in_out = record.to_vec();
        let plain = key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::empty(),
                &mut in_out,
            )
            .ok()?;
        // Padding is zeros after the delimiter.
        let end = plain.iter().rposition(|&byte| byte != 0)?;
        if plain[end] != LAST_RECORD {
            return None;
        }
        Some(plain[..end].to_vec())
    }
}

/// An HKDF output length.
struct Len(usize);

impl KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn hkdf(salt: &[u8], secret: &[u8], info: &[u8], out: &mut [u8]) -> Option<()> {
    Salt::new(HKDF_SHA256, salt)
        .extract(secret)
        .expand(&[info], Len(out.len()))
        .ok()?
        .fill(out)
        .ok()
}

/// The content encryption key and nonce (RFC 8291 §3.4).
fn content_key(
    ecdh: &[u8],
    auth: &[u8],
    receiver: &[u8],
    sender: &[u8],
    salt: &[u8],
) -> Option<([u8; 16], [u8; 12])> {
    let mut info = b"WebPush: info\0".to_vec();
    info.extend_from_slice(receiver);
    info.extend_from_slice(sender);
    let mut ikm = [0; 32];
    hkdf(auth, ecdh, &info, &mut ikm)?;
    let mut key = [0; 16];
    hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0", &mut key)?;
    let mut nonce = [0; 12];
    hkdf(salt, &ikm, b"Content-Encoding: nonce\0", &mut nonce)?;
    Some((key, nonce))
}

/// What a push the relay carried says, opened with the keys kept on this
/// device; `None` when there are none or it wasn't encrypted to them.
pub(crate) fn open(platform: Arc<dyn Platform>, body: &[u8]) -> Option<Vec<u8>> {
    PushKeys::load(&Store::new(platform)).ok()??.open(body)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testing::FakePlatform;

    /// RFC 8291 Appendix A, as the RFC prints it.
    const UA_PRIVATE: &str = "q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94";
    const UA_PUBLIC: &str =
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    const AUTH_SECRET: &str = "BTBZMqHH6r4Tts7J_aSIgg";
    const BODY: &str = "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN";

    fn bytes(text: &str) -> Vec<u8> {
        URL_SAFE_NO_PAD.decode(text).unwrap()
    }

    fn rfc_keys() -> PushKeys {
        PushKeys::from_parts(&bytes(UA_PRIVATE), &bytes(AUTH_SECRET)).unwrap()
    }

    /// What a sender does (RFC 8291 §3.4): a fresh key and salt, one record.
    pub fn encrypt(p256dh: &str, auth: &str, plaintext: &[u8]) -> Vec<u8> {
        let receiver = bytes(p256dh);
        let sender = PrivateKey::generate(&ECDH_P256).unwrap();
        let sender_public = sender.compute_public_key().unwrap();
        let ecdh = agreement::agree(
            &sender,
            UnparsedPublicKey::new(&ECDH_P256, &receiver),
            (),
            |secret| Ok(secret.to_vec()),
        )
        .unwrap();
        let mut salt = [0; SALT_LEN];
        aws_lc_rs::rand::fill(&mut salt).unwrap();
        let (key, nonce) = content_key(
            &ecdh,
            &bytes(auth),
            &receiver,
            sender_public.as_ref(),
            &salt,
        )
        .unwrap();
        let mut record = plaintext.to_vec();
        record.push(LAST_RECORD);
        let key = LessSafeKey::new(UnboundKey::new(&AES_128_GCM, &key).unwrap());
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::empty(),
            &mut record,
        )
        .unwrap();
        let mut body = salt.to_vec();
        body.extend_from_slice(&4096u32.to_be_bytes());
        body.push(PUBLIC_KEY_LEN as u8);
        body.extend_from_slice(sender_public.as_ref());
        body.extend_from_slice(&record);
        body
    }

    #[test]
    fn the_rfc_8291_example_opens() {
        let keys = rfc_keys();
        assert_eq!(keys.p256dh().as_deref(), Some(UA_PUBLIC));
        assert_eq!(keys.auth(), AUTH_SECRET);
        assert_eq!(
            keys.open(&bytes(BODY)).as_deref(),
            Some(&b"When I grow up, I want to be a watermelon"[..])
        );
    }

    #[test]
    fn keys_are_made_once_and_kept() {
        let platform = FakePlatform::with(&[]);
        let store = Store::new(platform.clone());
        assert!(PushKeys::load(&store).unwrap().is_none());
        let made = PushKeys::load_or_make(&store).unwrap().unwrap();
        let kept = PushKeys::load_or_make(&store).unwrap().unwrap();
        assert_eq!(made.p256dh(), kept.p256dh());
        assert_eq!(made.auth(), kept.auth());
        assert!(
            platform
                .get("push.keys")
                .unwrap()
                .starts_with("{\"private\":\"")
        );
        assert_eq!(bytes(&made.p256dh().unwrap()).len(), PUBLIC_KEY_LEN);
        assert_eq!(bytes(&made.auth()).len(), AUTH_LEN);
    }

    #[test]
    fn a_push_opens_with_the_kept_keys_and_no_others() {
        let platform = FakePlatform::with(&[]);
        let keys = PushKeys::load_or_make(&Store::new(platform.clone()))
            .unwrap()
            .unwrap();
        let body = encrypt(
            &keys.p256dh().unwrap(),
            &keys.auth(),
            br#"{"wake":"square"}"#,
        );
        assert_eq!(
            open(platform.clone(), &body).as_deref(),
            Some(&br#"{"wake":"square"}"#[..])
        );
        assert_eq!(open(FakePlatform::with(&[]), &body), None, "no keys");
        assert_eq!(rfc_keys().open(&body), None, "another device's keys");
        let mut tampered = body.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert_eq!(keys.open(&tampered), None);
        assert_eq!(keys.open(&body[..40]), None, "cut short");
        assert_eq!(keys.open(b""), None);
    }

    #[test]
    fn kept_keys_that_dont_read_are_none() {
        let platform = FakePlatform::with(&[("push.keys", "{\"private\":\"AA\",\"auth\":\"AA\"}")]);
        assert!(PushKeys::load(&Store::new(platform)).unwrap().is_none());
        let platform = FakePlatform::with(&[("push.keys", "not json")]);
        assert!(PushKeys::load(&Store::new(platform)).unwrap().is_none());
    }
}
