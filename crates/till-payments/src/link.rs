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
/// Everything but the keys is here, so the wallet backend (till-wallet)
/// opens the wallet and sets it up the same way each time; this is also
/// what Settings changes.
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
    /// Paid at this many confirmations: Fob's Settled Confirmations, 10 to
    /// 720 (docs/view-key-mode.md §5.6). Till's own setting, never Fob's.
    #[serde(default = "default_confirmations")]
    pub settled_confirmations: u32,
    /// How long a payment code lasts, in minutes.
    #[serde(default = "default_expiry")]
    pub expiry_minutes: u32,
    /// Whose prices convert the sale's currency to XMR ("Kraken"), when
    /// chosen or handed over by Fob; else the first provider that prices
    /// the currency (docs/view-key-mode.md §8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_provider: Option<String>,
    /// The currency Fob's prices were in when it handed them over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_currency: Option<String>,
    /// The wallet in Fob this till was connected to, when it was connected
    /// through Fob (docs/view-key-mode.md §5.9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fob: Option<FobWallet>,
}

/// The wallet in Fob a till was connected to: what Till says of it
/// ("Connected to Fob · Corner Café · Till (Corner Café)"), and how Fob
/// knows it on a reconnect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FobWallet {
    pub wallet_id: String,
    pub wallet_name: String,
    /// The account's label: "Till (Corner Café)".
    pub label: String,
    /// What Fob handed over besides the keys.
    #[serde(default)]
    pub node_from_fob: bool,
    #[serde(default)]
    pub prices_from_fob: bool,
}

/// Fob's default Settled Confirmations.
pub const DEFAULT_CONFIRMATIONS: u32 = 10;
/// A code lasts 30 minutes unless Settings says otherwise.
pub const DEFAULT_EXPIRY_MINUTES: u32 = 30;

fn default_confirmations() -> u32 {
    DEFAULT_CONFIRMATIONS
}

fn default_expiry() -> u32 {
    DEFAULT_EXPIRY_MINUTES
}

impl WalletLink {
    /// A link with Till's defaults for everything but the network, the
    /// till's number and the file's key.
    pub fn new(network: &str, till_number: u32, file_key: Secret) -> WalletLink {
        WalletLink {
            network: network.to_owned(),
            till_number,
            file_key,
            node: None,
            light_server: None,
            settled_confirmations: DEFAULT_CONFIRMATIONS,
            expiry_minutes: DEFAULT_EXPIRY_MINUTES,
            price_provider: None,
            price_currency: None,
            fob: None,
        }
    }
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
            node: Some("https://node.example:18089".into()),
            ..WalletLink::new("mainnet", 2, Secret::from("k_1"))
        });
        let saved = link.save();
        assert_eq!(
            saved,
            r#"{"wallet":{"network":"mainnet","till_number":2,"file_key":"k_1","node":"https://node.example:18089","settled_confirmations":10,"expiry_minutes":30}}"#
        );
        // What the first wallet links were written as reads with the
        // defaults.
        assert_eq!(
            Link::restore(
                r#"{"wallet":{"network":"mainnet","till_number":2,"file_key":"k_1","node":"https://node.example:18089"}}"#
            ),
            Some(link.clone())
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
