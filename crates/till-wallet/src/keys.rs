//! The wallet's keys as setup takes them (docs/view-key-mode.md §5.2):
//! typed, from a wallet code, or from a seed phrase. Each way ends with
//! [`ViewKeys`]: a primary address and its private view key, which is all
//! Till keeps. Seed words and spend keys are wiped as soon as the view key
//! is taken from them, and never leave this module.

use fob_codes::uri::{WalletCode, parse_wallet_code};
use fob_keys::{KeyError, Keys, Restored};
use fob_wallet::address::check_address;
use fob_wallet::error::MoneroError;
use zeroize::Zeroizing;

pub use fob_wallet::Network;

/// A wallet's view-only keys: what Till restores its wallet file from.
/// The view key is wiped when this goes, and never printed.
#[derive(Clone)]
pub struct ViewKeys {
    pub network: Network,
    /// The primary address (starting 4 on mainnet, 5 on stagenet).
    pub address: String,
    pub view_key: Zeroizing<String>,
    /// Where the wallet began, when the way in knew (a Polyseed's
    /// birthday, as a height); Till starts at the chain's tip regardless
    /// (§5.4), so this is only ever an earliest bound.
    pub birthday_height: Option<u64>,
}

impl std::fmt::Debug for ViewKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewKeys")
            .field("network", &self.network)
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl ViewKeys {
    fn of(keys: &Keys, birthday_height: Option<u64>) -> ViewKeys {
        ViewKeys {
            network: keys.network,
            address: keys.primary_address(),
            view_key: keys.view_key_hex(),
            birthday_height,
        }
    }
}

/// What's wrong with a typed address (D2), as it's typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressProblem {
    /// Nothing typed yet.
    Empty,
    /// Not an address at all.
    NotAnAddress,
    /// An address on the other network.
    OtherNetwork(Network),
    /// A subaddress (starting 8 or 7) or an integrated address: Till
    /// needs the wallet's primary address.
    NotPrimary,
}

/// What's wrong with the view key typed beside an address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewKeyProblem {
    Empty,
    /// Not 64 hex digits, or not a key.
    NotAKey,
    /// A key, but not this address's private view key (the public one,
    /// or another wallet's).
    DoesntMatch,
}

/// Both fields of D2 checked together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeysCheck {
    pub address: Result<(), AddressProblem>,
    /// `Ok` only once the address is good too.
    pub view_key: Result<(), ViewKeyProblem>,
}

impl KeysCheck {
    pub fn ok(&self) -> bool {
        self.address.is_ok() && self.view_key.is_ok()
    }
}

/// Checks a typed address on `network`.
pub fn check_typed_address(network: Network, address: &str) -> Result<(), AddressProblem> {
    let address = address.trim();
    if address.is_empty() {
        return Err(AddressProblem::Empty);
    }
    match check_address(network, address) {
        Ok(()) => {}
        Err(MoneroError::WrongNetwork { found, .. }) => {
            return Err(AddressProblem::OtherNetwork(found));
        }
        Err(_) => return Err(AddressProblem::NotAnAddress),
    }
    // A good address that view_only refuses as an address is a subaddress
    // or an integrated one. The key here is the base point's scalar 1: any
    // valid scalar does, since only the address is judged first.
    match Keys::view_only(network, address, ONE) {
        Err(KeyError::Monero(_)) => Err(AddressProblem::NotPrimary),
        _ => Ok(()),
    }
}

/// A canonical scalar, for asking whether an address is a primary one.
const ONE: &str = "0100000000000000000000000000000000000000000000000000000000000000";

/// Checks D2's address and view key together, as they're typed: the view
/// key is judged only against a good address.
pub fn check_typed_keys(network: Network, address: &str, view_key: &str) -> KeysCheck {
    let address_check = check_typed_address(network, address);
    let view_key = view_key.trim();
    let view_check = if view_key.is_empty() {
        Err(ViewKeyProblem::Empty)
    } else if view_key.len() != 64 || !view_key.bytes().all(|b| b.is_ascii_hexdigit()) {
        Err(ViewKeyProblem::NotAKey)
    } else if address_check.is_err() {
        // Can't be judged yet; nothing to say about it.
        Ok(())
    } else {
        match Keys::view_only(network, address.trim(), view_key) {
            Ok(_) => Ok(()),
            Err(KeyError::BadKey) => {
                // 64 hex digits that are a key, but this address's? A
                // non-canonical scalar is "not a key"; anything else
                // didn't match.
                Err(ViewKeyProblem::DoesntMatch)
            }
            Err(_) => Err(ViewKeyProblem::NotAKey),
        }
    };
    KeysCheck {
        address: address_check,
        view_key: view_check,
    }
}

/// The keys typed in D2, once both are right.
pub fn keys_from_typed(network: Network, address: &str, view_key: &str) -> Option<ViewKeys> {
    let keys = Keys::view_only(network, address.trim(), view_key.trim()).ok()?;
    Some(ViewKeys::of(&keys, None))
}

/// Why seed words gave no keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedProblem {
    /// Not 25 or 16 words: how many there were.
    WordCount(usize),
    /// The right count, but not a seed phrase (a word not on the list, or
    /// the checksum doesn't hold).
    NotASeed,
}

/// The view-only keys in a seed phrase: 25 words or a 16-word Polyseed.
/// The words are taken, and wiped with the spend key before this returns,
/// whatever it returns (fob-keys' `view_only_from_words`).
pub fn keys_from_seed(network: Network, words: Zeroizing<String>) -> Result<ViewKeys, SeedProblem> {
    let count = words.split_whitespace().count();
    if count != 25 && count != 16 {
        return Err(SeedProblem::WordCount(count));
    }
    let restored: Restored =
        Restored::view_only_from_words(network, words).map_err(|_| SeedProblem::NotASeed)?;
    let birthday = restored.birthday.map(|b| network.height_at(b));
    Ok(ViewKeys::of(&restored.keys, birthday))
}

/// Why a scanned or pasted wallet code gave no keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeProblem {
    /// Not a wallet code at all.
    NotACode,
    /// A wallet code, but its keys don't hold together.
    BadKeys,
}

/// The view-only keys in a wallet code (D1): `monero_wallet:` with a view
/// key (Feather, Cake, Fob), with a spend key, or with seed words, or bare
/// seed words. Whatever can spend is wiped once the view key is taken. The
/// network is the code's own when it holds an address, else `network`.
pub fn keys_from_code(text: &str, network: Network) -> Result<ViewKeys, CodeProblem> {
    let text = Zeroizing::new(text.trim().to_owned());
    let mut last = CodeProblem::NotACode;
    let other = match network {
        Network::Mainnet => Network::Stagenet,
        Network::Stagenet => Network::Mainnet,
    };
    for network in [network, other] {
        let code = match parse_wallet_code(network, &text) {
            Ok(code) => code,
            Err(_) => continue,
        };
        let keys = match code {
            WalletCode::ViewOnly { address, view, .. } => Keys::view_only(network, &address, &view)
                .map(|keys| ViewKeys::of(&keys, None))
                .map_err(|_| CodeProblem::BadKeys),
            WalletCode::SpendKey {
                address,
                spend,
                view,
                ..
            } => match Keys::from_spend_key(network, &spend, view.as_deref().map(|v| v.as_str())) {
                Ok(keys)
                    if address
                        .as_deref()
                        .is_none_or(|a| a == keys.primary_address()) =>
                {
                    // The spend key goes with `keys` here.
                    Ok(ViewKeys::of(&keys, None))
                }
                _ => Err(CodeProblem::BadKeys),
            },
            WalletCode::Words { words, .. } => {
                keys_from_seed(network, words).map_err(|_| CodeProblem::BadKeys)
            }
        };
        match keys {
            Ok(keys) => return Ok(keys),
            Err(problem) => last = problem,
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wallet made for the test, its keys as a wallet shows them.
    fn wallet(network: Network) -> (String, Zeroizing<String>, Keys) {
        let keys = Keys::generate(network).keys;
        (keys.primary_address(), keys.view_key_hex(), keys)
    }

    #[test]
    fn typed_keys_are_checked_as_they_are_typed() {
        let (address, view, keys) = wallet(Network::Mainnet);
        let check = check_typed_keys(Network::Mainnet, "", "");
        assert_eq!(check.address, Err(AddressProblem::Empty));
        assert_eq!(check.view_key, Err(ViewKeyProblem::Empty));

        let check = check_typed_keys(Network::Mainnet, "4notanaddress", "");
        assert_eq!(check.address, Err(AddressProblem::NotAnAddress));

        let check = check_typed_keys(Network::Stagenet, &address, &view);
        assert_eq!(
            check.address,
            Err(AddressProblem::OtherNetwork(Network::Mainnet))
        );

        let check = check_typed_keys(Network::Mainnet, &keys.address_at(0, 1), &view);
        assert_eq!(check.address, Err(AddressProblem::NotPrimary));

        let check = check_typed_keys(Network::Mainnet, &address, "abc");
        assert_eq!(check.view_key, Err(ViewKeyProblem::NotAKey));

        // The public view key, as people paste by mistake.
        let check = check_typed_keys(Network::Mainnet, &address, &keys.public_view_key_hex());
        assert_eq!(check.address, Ok(()));
        assert_eq!(check.view_key, Err(ViewKeyProblem::DoesntMatch));
        assert!(!check.ok());

        let check = check_typed_keys(Network::Mainnet, &format!(" {address}\n"), &view);
        assert!(check.ok(), "{check:?}");
        let typed = keys_from_typed(Network::Mainnet, &address, &view).unwrap();
        assert_eq!(typed.address, address);
        assert_eq!(*typed.view_key, *view);
        assert!(!format!("{typed:?}").contains(view.as_str()));
    }

    #[test]
    fn a_seed_gives_the_same_view_key_and_its_words_are_counted_first() {
        let restored = Keys::generate(Network::Stagenet);
        let (kind, words) = restored.keys.seed_words().unwrap();
        assert_eq!(kind.words(), 16);
        let keys = keys_from_seed(Network::Stagenet, words).unwrap();
        assert_eq!(keys.address, restored.keys.primary_address());
        assert_eq!(*keys.view_key, *restored.keys.view_key_hex());
        assert!(
            keys.birthday_height.is_some(),
            "a Polyseed knows its birthday"
        );

        assert_eq!(
            keys_from_seed(Network::Mainnet, Zeroizing::new("one two three".into())).unwrap_err(),
            SeedProblem::WordCount(3)
        );
        let sixteen = Zeroizing::new(vec!["notaword"; 16].join(" "));
        assert_eq!(
            keys_from_seed(Network::Mainnet, sixteen).unwrap_err(),
            SeedProblem::NotASeed
        );
    }

    #[test]
    fn a_wallet_code_gives_its_view_key_whatever_else_it_holds() {
        let (address, view, keys) = wallet(Network::Stagenet);
        let view_only = format!("monero_wallet:{address}?view_key={}&height=100", *view);
        let read = keys_from_code(&view_only, Network::Mainnet).unwrap();
        assert_eq!(read.network, Network::Stagenet);
        assert_eq!(read.address, address);

        let spend = keys.spend_key_hex().unwrap();
        let spend_code = format!("monero_wallet:{address}?spend_key={}", *spend);
        assert_eq!(
            keys_from_code(&spend_code, Network::Mainnet)
                .unwrap()
                .address,
            address
        );

        let (_, words) = keys.seed_words().unwrap();
        let seed_code = format!("monero_wallet:?seed={}", words.replace(' ', "%20"));
        assert_eq!(
            keys_from_code(&seed_code, Network::Stagenet)
                .unwrap()
                .address,
            address
        );
        assert_eq!(
            keys_from_code(&words, Network::Stagenet).unwrap().address,
            address
        );

        assert_eq!(
            keys_from_code("monero:4abc?tx_amount=1", Network::Mainnet).unwrap_err(),
            CodeProblem::NotACode
        );
        let (other, _, _) = wallet(Network::Stagenet);
        let mismatched = format!("monero_wallet:{other}?view_key={}", *view);
        assert_eq!(
            keys_from_code(&mismatched, Network::Stagenet).unwrap_err(),
            CodeProblem::BadKeys
        );
    }
}
