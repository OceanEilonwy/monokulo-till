//! Making the till's wallet at the end of setup (docs/view-key-mode.md
//! §5.1, §11.1), and the node checks setup shows (§9).

use std::path::Path;

use fob_wallet::Wallet;
use fob_wallet::light;
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
    /// The node: the chain's, or with a light server the one its proofs
    /// are checked with.
    pub node: Option<String>,
    pub light_server: Option<String>,
    /// The light server's own certificate is accepted.
    pub light_self_signed: bool,
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
            light_self_signed: false,
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
        light_self_signed: setup.light_server.is_some() && setup.light_self_signed,
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

/// What checking a light wallet server found: its lines, as D4's node
/// checks are drawn, and the network it's on when Till knows it, so setup
/// can take the wallet's network from the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightCheck {
    pub checks: Vec<NodeCheck>,
    pub network: Option<Network>,
}

impl LightCheck {
    /// Every line passed: the server can be used.
    pub fn passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|c| c.state == CheckState::Passed)
    }
}

/// Checks the light wallet server at `url` as Fob's Test does
/// (`/get_version`, which sends nothing of the wallet's): it answers within
/// 10 s, on a network Till knows, watching subaddresses. A server on the
/// other network Till knows is checked as on that one, and says so, since
/// setup takes the wallet's network from it (core.md decision 245).
pub async fn check_light_server(preferred: Network, url: &str, self_signed: bool) -> LightCheck {
    let started = std::time::Instant::now();
    let report = light::check(preferred, url, self_signed, None).await;
    let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    light_checks_of(&report, ms)
}

/// The lines for a server's report, `ms` the time it took to answer.
pub fn light_checks_of(report: &light::Report, ms: u64) -> LightCheck {
    let failed = |text: String| NodeCheck {
        text,
        state: CheckState::Failed,
    };
    let passed = |text: String| NodeCheck {
        text,
        state: CheckState::Passed,
    };
    let version = match &report.outcome {
        Err(problem) => {
            return LightCheck {
                checks: vec![failed(light_problem(problem))],
                network: None,
            };
        }
        Ok(version) => version,
    };
    let network = match version.network.as_str() {
        "main" => Some(Network::Mainnet),
        "stage" => Some(Network::Stagenet),
        _ => None,
    };
    let mut checks = vec![passed(format!("Answers in {}", seconds(ms)))];
    checks.push(match network {
        Some(n) => passed(capitalised(n.name())),
        None if version.network.is_empty() => failed("Doesn't say its network".into()),
        None => failed(format!("On {}, which Till doesn't use", version.network)),
    });
    checks.push(if version.max_subaddresses == 0 {
        failed("Doesn't watch subaddresses".into())
    } else {
        passed("Watches subaddresses".into())
    });
    if let Some(pin) = report.pin {
        // The start of its key, to compare with the server's own.
        let key = fob_wallet::words::light::key(&pin);
        let start: Vec<&str> = key.split(' ').take(4).collect();
        checks.push(passed(format!("Key {}…", start.join(" "))));
    }
    LightCheck { checks, network }
}

/// What's wrong with a light wallet server, in Till's words.
pub fn light_problem(problem: &light::Problem) -> String {
    use light::Problem;
    match problem {
        Problem::Unreachable(_) => "Doesn't answer".into(),
        Problem::Certificate(_) => {
            "Its certificate isn't trusted. Accept its own certificate if it's yours".into()
        }
        Problem::Unreadable | Problem::Status(_) => "Isn't a light wallet server".into(),
        Problem::Onion => "Till can't reach .onion or .i2p servers".into(),
        Problem::PlainHttp => {
            "Needs https: the view key goes with every request. Plain http is only for this device"
                .into()
        }
        Problem::BadAddress => "That isn't a server's address".into(),
        Problem::Unavailable => "Its own node isn't answering it".into(),
        other => fob_wallet::words::light::problem(other),
    }
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

    fn version(network: &str, subaddresses: u32) -> light::lws::Version {
        light::lws::Version {
            server_type: "monokulo-lws".into(),
            last_git_commit_hash: None,
            last_commit_date: None,
            monero_version_full: String::new(),
            blockchain_height: 2_250_000,
            api: 2,
            max_subaddresses: subaddresses,
            network: network.into(),
            testnet: network != "main",
        }
    }

    fn report(
        outcome: Result<light::lws::Version, light::Problem>,
        pin: Option<[u8; 32]>,
    ) -> light::Report {
        light::Report {
            url: "https://lws.example:8443".into(),
            self_signed: pin.is_some(),
            address: "lws.example:8443".into(),
            ours: Network::Mainnet,
            outcome,
            pin,
        }
    }

    #[test]
    fn a_light_servers_lines_say_it_answers_its_network_and_subaddresses() {
        let good = light_checks_of(&report(Ok(version("stage", 10_000)), None), 412);
        let lines: Vec<(&str, CheckState)> = good
            .checks
            .iter()
            .map(|c| (c.text.as_str(), c.state))
            .collect();
        assert_eq!(
            lines,
            [
                ("Answers in 0.4 s", CheckState::Passed),
                ("Stagenet", CheckState::Passed),
                ("Watches subaddresses", CheckState::Passed),
            ]
        );
        assert!(good.passed());
        assert_eq!(
            good.network,
            Some(Network::Stagenet),
            "the wallet takes the server's"
        );

        let blind = light_checks_of(&report(Ok(version("main", 0)), Some([0xab; 32])), 90);
        assert!(!blind.passed());
        assert_eq!(blind.checks[2].text, "Doesn't watch subaddresses");
        assert_eq!(blind.checks[3].text, "Key abab abab abab abab…");

        let testnet = light_checks_of(&report(Ok(version("test", 10)), None), 90);
        assert_eq!(testnet.checks[1].text, "On test, which Till doesn't use");
        assert_eq!(testnet.network, None);

        let gone = light_checks_of(
            &report(Err(light::Problem::Unreachable("x".into())), None),
            0,
        );
        assert_eq!(gone.checks.len(), 1);
        assert_eq!(
            (gone.checks[0].text.as_str(), gone.checks[0].state),
            ("Doesn't answer", CheckState::Failed)
        );
        assert!(light_problem(&light::Problem::PlainHttp).starts_with("Needs https"));
    }

    #[tokio::test]
    async fn plain_http_to_another_machine_is_refused_before_asking() {
        let check = check_light_server(Network::Mainnet, "http://192.0.2.1:8443", false).await;
        assert!(!check.passed());
        assert!(check.checks[0].text.starts_with("Needs https"), "{check:?}");
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
