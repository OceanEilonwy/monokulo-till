//! What a till keeps of how it is paid (docs/view-key-mode.md §4.1, core.md
//! decision 238).

use monokulo_client::Connection;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use till_types::Secret;

/// How this till is paid, kept (encrypted) under the `"connection"` secret
/// until it's disconnected.
///
/// A Monokulo link is kept as the bare [`Connection`] JSON every build
/// before it wrote (`{"app_key":…,"api":…}`), so a till connected before
/// payment backends reads as [`Link::Monokulo`] and nobody connects again,
/// and an older build put back on the device still reads it. A wallet link
/// is kept as `{"wallet":{…}}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Monokulo(Connection),
    Wallet(WalletLink),
}

/// The device's own wallet, through a node or the shop's light wallet
/// server (docs/view-key-mode.md §5). The keys themselves are in the
/// wallet's encrypted file; this keeps only the key that opens it.
///
/// A first shape, for the saved link to carry: the wallet backend fills it
/// in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletLink {
    /// `mainnet`, `stagenet` or `testnet`.
    pub network: String,
    /// This till's number among the shop's tills: which account's
    /// addresses it uses (docs/view-key-mode.md §5.3).
    pub till_number: u32,
    /// The key that opens the wallet file.
    pub file_key: Secret,
    /// The Monero node it reads the chain from, when it uses one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// The light wallet server it asks, when it uses one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light_server: Option<String>,
}

/// The JSON a link is kept as.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Kept {
    Wallet { wallet: WalletLink },
    Monokulo(Connection),
}

impl Serialize for Link {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Link::Monokulo(connection) => connection.serialize(serializer),
            Link::Wallet(wallet) => Kept::Wallet {
                wallet: wallet.clone(),
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Link {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Link, D::Error> {
        Ok(match Kept::deserialize(deserializer)? {
            Kept::Wallet { wallet } => Link::Wallet(wallet),
            Kept::Monokulo(connection) => Link::Monokulo(connection),
        })
    }
}

impl Link {
    /// What the app writes to keep it.
    pub fn save(&self) -> String {
        serde_json::to_string(self).expect("a link is always JSON")
    }

    /// The link `saved` holds, or `None` for none.
    pub fn restore(saved: &str) -> Option<Link> {
        serde_json::from_str(saved).ok()
    }

    /// The Monokulo connection, for a Monokulo link.
    pub fn monokulo(&self) -> Option<&Connection> {
        match self {
            Link::Monokulo(connection) => Some(connection),
            Link::Wallet(_) => None,
        }
    }
}

impl From<Connection> for Link {
    fn from(connection: Connection) -> Link {
        Link::Monokulo(connection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a till connected before payment backends keeps, as the app's
    /// store test has it: key, entry, onion entry and Tor.
    const SAVED: &str = r#"{"app_key":"ak_1","api":"https://pay.example/api/v1","onion_api":"http://abcdef.onion/api/v1","tor":true}"#;

    #[test]
    fn a_connection_kept_before_payment_backends_reads_as_a_monokulo_link_and_is_written_back_the_same()
     {
        let link = Link::restore(SAVED).unwrap();
        let Link::Monokulo(connection) = &link else {
            panic!("a Monokulo link: {link:?}");
        };
        assert_eq!(connection.app_key.expose_str(), "ak_1");
        assert_eq!(connection.api.as_str(), "https://pay.example/api/v1");
        assert_eq!(
            connection.onion_api.as_deref(),
            Some("http://abcdef.onion/api/v1")
        );
        assert!(connection.tor);
        assert_eq!(link.save(), SAVED, "an older build still reads it");
        assert_eq!(
            Link::restore(r#"{"app_key":"ak_x","api":"https://pay.example/api/v1"}"#),
            Some(Link::Monokulo(Connection::new(
                "ak_x",
                "https://pay.example/api/v1"
            )))
        );
    }

    #[test]
    fn a_wallet_link_comes_back_as_it_went() {
        let link = Link::Wallet(WalletLink {
            network: "mainnet".into(),
            till_number: 2,
            file_key: Secret::from("k_1"),
            node: Some("https://node.example:18089".into()),
            light_server: None,
        });
        let saved = link.save();
        assert_eq!(
            saved,
            r#"{"wallet":{"network":"mainnet","till_number":2,"file_key":"k_1","node":"https://node.example:18089"}}"#
        );
        assert_eq!(Link::restore(&saved), Some(link.clone()));
        assert_eq!(link.monokulo(), None);
        assert!(!format!("{link:?}").contains("k_1"), "the key never prints");
    }

    #[test]
    fn nothing_usable_restores_nothing() {
        assert_eq!(Link::restore("not json"), None);
        assert_eq!(Link::restore(r#"{"app_key":"ak_x"}"#), None);
        assert_eq!(Link::restore(r#"{"wallet":{"network":"mainnet"}}"#), None);
    }
}
