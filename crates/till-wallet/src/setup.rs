//! Making the till's wallet at the end of setup (docs/view-key-mode.md
//! §5.1, §11.1), and the node checks setup shows (§9).

use std::path::Path;

use fob_wallet::Wallet;
use fob_wallet::node::{self, Mark, Outcome, Report, Setting, Step};
use till_payments::{FobWallet, WalletLink};
use till_types::Secret;

use crate::backend::{apply, wallet_path};
use crate::keys::{Network, ViewKeys};

/// What setup chose for the till, besides its keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletSetup {
    /// The till's number: the account its addresses are in (§5.3).
    pub till_number: u32,
    pub node: Option<String>,
    pub light_server: Option<String>,
    pub settled_confirmations: u32,
    pub expiry_minutes: u32,
    pub price_provider: Option<String>,
    pub price_currency: Option<String>,
    pub fob: Option<FobWallet>,
    /// Where the wallet begins: Fob's tip when connected through Fob, else
    /// the chain's tip now, estimated from the clock (§5.4).
    pub restore_height: Option<u64>,
}

impl WalletSetup {
    /// Till's defaults, for till `till_number`.
    pub fn new(till_number: u32) -> WalletSetup {
        WalletSetup {
            till_number,
            node: None,
            light_server: None,
            settled_confirmations: till_payments::DEFAULT_CONFIRMATIONS,
            expiry_minutes: till_payments::DEFAULT_EXPIRY_MINUTES,
            price_provider: None,
            price_currency: None,
            fob: None,
            restore_height: None,
        }
    }
}

/// The wallet made, and the link that opens it again.
pub struct Made {
    pub link: WalletLink,
    pub wallet: Wallet,
}

/// The node a network starts on: Fob's first default.
pub fn default_node(network: Network) -> String {
    network.default_nodes()[0].to_owned()
}

/// Fob's default nodes for `network`, for Change.
pub fn default_nodes(network: Network) -> Vec<String> {
    network
        .default_nodes()
        .iter()
        .map(|n| (*n).to_owned())
        .collect()
}

/// A day of blocks: how far before the clock's estimate of the tip a new
/// wallet starts, so an estimate a little ahead never skips a payment.
const MARGIN: u64 = 720;

/// Makes the till's wallet file in `data_dir` (any earlier one goes first),
/// view-only from `keys`, in Manual Sync and set up as `setup` says, under
/// a new random file key the link keeps.
pub async fn make_wallet(
    data_dir: &Path,
    keys: &ViewKeys,
    setup: &WalletSetup,
) -> Result<Made, String> {
    let mut key = [0u8; 32];
    getrandom(&mut key)?;
    let file_key: String = key.iter().map(|b| format!("{b:02x}")).collect();
    let path = wallet_path(data_dir);
    remove_wallet(data_dir);
    let height = setup.restore_height.unwrap_or_else(|| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        keys.network.height_at(now).saturating_sub(MARGIN)
    });
    let (network, address, view) = (keys.network, keys.address.clone(), keys.view_key.clone());
    let wallet = tokio::task::spawn_blocking(move || {
        Wallet::restore_view_only(&path, key, network, &address, &view, Some(height))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    let link = WalletLink {
        node: setup
            .node
            .clone()
            .or_else(|| setup.light_server.is_none().then(|| default_node(network))),
        light_server: setup.light_server.clone(),
        settled_confirmations: setup.settled_confirmations,
        expiry_minutes: setup.expiry_minutes,
        price_provider: setup.price_provider.clone(),
        price_currency: setup.price_currency.clone(),
        fob: setup.fob.clone(),
        ..WalletLink::new(
            network.name(),
            setup.till_number.max(1),
            Secret::from(file_key),
        )
    };
    apply(&wallet, &link).await.map_err(|e| e.say())?;
    Ok(Made { link, wallet })
}

/// Takes the wallet file in `data_dir` away, if there is one.
pub fn remove_wallet(data_dir: &Path) {
    let path = wallet_path(data_dir);
    let _ = fob_wallet::wallets::remove_files(&path);
    let _ = std::fs::remove_file(&path);
}

fn getrandom(bytes: &mut [u8]) -> Result<(), String> {
    use rand_core::{OsRng, RngCore};
    OsRng
        .try_fill_bytes(bytes)
        .map_err(|e| format!("Couldn't make the wallet's key: {e}"))
}

/// One of a node's checks as setup shows it (D4): "Answers in 0.4 s",
/// "Mainnet", "Up to date".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeCheck {
    pub text: String,
    pub state: CheckState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Checking,
    Passed,
    Failed,
}

/// Checks the node at `url` on `network` as Fob does (certificate,
/// answers within 5 s, network, up to date, blocks a second), for the
/// three lines D4 shows.
pub async fn check_node(network: Network, url: &str) -> Vec<NodeCheck> {
    if let Err(problem) = node::reachable(&node::normalize(url)) {
        return vec![NodeCheck {
            text: fob_wallet::FobError::Endpoint(problem).to_string(),
            state: CheckState::Failed,
        }];
    }
    let setting = Setting {
        url: url.to_owned(),
        self_signed: false,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let report = node::check(network, &setting, None, 0, now, &|_| {}).await;
    checks_of(network, &report)
}

/// D4's lines from a report: answering, the network, up to date.
pub fn checks_of(network: Network, report: &Report) -> Vec<NodeCheck> {
    let state = |mark: Mark| match mark {
        Mark::Pass => CheckState::Passed,
        Mark::Fail => CheckState::Failed,
        _ => CheckState::Checking,
    };
    let mut checks = Vec::new();
    for row in &report.rows {
        let text = match (row.step, &row.outcome) {
            (Step::Answers, Outcome::Latency(ms)) => format!("Answers in {}", seconds(*ms)),
            (Step::Answers, Outcome::Failed { .. }) => "Doesn't answer".to_owned(),
            (Step::Answers, _) => "Answers".to_owned(),
            (Step::Network, Outcome::WrongNetwork { theirs, .. }) if !theirs.is_empty() => {
                format!(
                    "On {}, not {}",
                    capitalised(theirs),
                    capitalised(network.name())
                )
            }
            (Step::Network, _) => capitalised(network.name()),
            (Step::UpToDate, Outcome::Behind { .. }) => "Behind".to_owned(),
            (Step::UpToDate, _) => "Up to date".to_owned(),
            (Step::Certificate, Outcome::Certificate(Some(_))) => {
                "Its certificate isn't trusted".to_owned()
            }
            _ => continue,
        };
        checks.push(NodeCheck {
            text,
            state: state(row.mark),
        });
    }
    checks
}

/// 412 ms as "0.4 s".
fn seconds(ms: u64) -> String {
    let tenths = ms.saturating_add(50).div_euclid(100).max(1);
    format!("{}.{} s", tenths / 10, tenths % 10)
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nodes_answer_time_reads_in_tenths() {
        assert_eq!(seconds(412), "0.4 s");
        assert_eq!(seconds(1_960), "2.0 s");
        assert_eq!(seconds(400), "0.4 s");
        assert_eq!(seconds(40), "0.1 s");
    }

    #[tokio::test]
    async fn the_wallet_is_made_view_only_in_manual_sync_with_this_tills_account() {
        let dir = tempfile::tempdir().unwrap();
        let keys = fob_keys::Keys::generate(Network::Stagenet).keys;
        let view = crate::keys::keys_from_typed(
            Network::Stagenet,
            &keys.primary_address(),
            &keys.view_key_hex(),
        )
        .unwrap();
        let setup = WalletSetup {
            settled_confirmations: 30,
            ..WalletSetup::new(4)
        };
        // Nothing here asks the node: the network's default is kept.
        let Made { link, wallet } = make_wallet(dir.path(), &view, &setup).await.unwrap();
        assert_eq!((link.network.as_str(), link.till_number), ("stagenet", 4));
        assert_eq!(link.file_key.expose_str().len(), 64);
        assert_eq!(
            link.node.as_deref(),
            Some(Network::Stagenet.default_nodes()[0])
        );
        assert_eq!(wallet.receive_account().await, 4);
        let manual = wallet.manual_settings().await;
        assert!(manual.on);
        assert_eq!(manual.settled_confirmations, 30);
        drop(wallet);
        // Made again, the first one goes.
        let again = make_wallet(dir.path(), &view, &WalletSetup::new(4))
            .await
            .unwrap();
        assert_ne!(again.link.file_key, link.file_key);
    }
}
