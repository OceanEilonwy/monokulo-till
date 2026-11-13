//! Settings for a till on the shop's own wallet (docs/view-key-mode.md
//! §5.3, §5.6, §8, §9, §11; core.md decision 234): how Till sees payments
//! and through what, the wallet (and Fob's wallet when connected through
//! Fob), this till's number and account with the lookahead warning, the
//! node with Fob's checks, Paid after and the code's expiry, the price
//! source, what the wallet warns of, and Disconnect.
//!
//! The steppers, node row and check marks are setup's own
//! ([`crate::wallet_setup`]), so D4 and Settings draw them the same way and
//! send the same actions.

use crate::view::{Fact, Press};
use crate::wallet_setup::{
    CheckMark, NodeCheckRow, NodeRow, SetupStepper, WalletNetwork, confirmations_time, expiry_text,
    node_address, self_signed_choice, step_stepper,
};

/// The accounts a wallet looks at past the last one used (wallet2, Feather
/// and Cake's default lookahead): a till numbered above it may go unseen.
pub const ACCOUNT_LOOKAHEAD: u32 = 50;

/// What the wallet section is drawn from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletSettingsInput {
    /// The primary address, whole; empty until the wallet has said.
    pub address: String,
    pub network: WalletNetwork,
    pub node: Option<String>,
    /// Through the shop's light wallet server: its address. The row shows
    /// it in the node's place, and `node` is the one its proofs are
    /// checked with.
    pub light_server: Option<String>,
    /// Whether the light wallet server's own certificate is accepted.
    pub light_self_signed: bool,
    pub till_number: u32,
    /// Connected through Fob: its wallet's name and the account's label.
    pub fob: Option<(String, String)>,
    pub confirmations: u32,
    pub expiry_minutes: u32,
    /// The node's checks, while there are any to show.
    pub checks: Vec<(String, CheckMark)>,
    pub node_choices: Vec<String>,
    /// The point of sale's currency, when known: prices are for it.
    pub currency: Option<String>,
    /// The provider prices come from now.
    pub provider: String,
    /// The providers that price `currency` (fob-price's `providers_for`),
    /// or every provider without one.
    pub providers: Vec<String>,
    /// Each provider asked for the price as Settings opened; `None` while
    /// they're asked.
    pub price_checks: Option<Vec<crate::prices::ProviderCheck>>,
    /// Till's keypad takes XMR only: no price is ever looked up.
    pub xmr_only: bool,
    /// What the wallet backend warns of (`BackendInfo::warnings`).
    pub warnings: Vec<String>,
    pub busy: bool,
    pub problem: Option<String>,
    /// A sale is waiting for payment: another wallet would never see it
    /// paid, so the wallet can't change yet.
    pub live_sales: bool,
}

/// The price source: whose prices convert the sale's currency.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PricesRow {
    /// "Prices from".
    pub label: String,
    /// "Kraken · AUD".
    pub value: String,
    /// One per provider that prices the currency, each with what it says
    /// XMR is worth, the one in use selected: `Action::ChoosePriceProvider`
    /// with its name (setup's Prices step draws the same rows).
    pub providers: Vec<crate::prices::ProviderRow>,
    /// "Asking each provider…", while they're asked.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub asking: Option<String>,
    pub footnote: String,
}

/// Settings › Your wallet.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WalletSection {
    /// "Payments".
    pub title: String,
    /// "How Till sees payments · Your own wallet".
    pub mode: Fact,
    /// Connected through Fob: "Connected to Fob" and "Corner Café · Till
    /// (Corner Café)".
    pub fob: Option<Fact>,
    /// Wallet address (short, monospaced) and Network.
    pub facts: Vec<Fact>,
    /// The whole address, for Copy (`Action::CopyWalletAddress`).
    pub wallet_address: String,
    /// "Till number", − and +, "In your wallet: account 2"; `None`
    /// connected through Fob, which chose the account ([`Self::account`]).
    pub till_number: Option<SetupStepper>,
    /// Connected through Fob: "In your wallet" and "Account 3".
    pub account: Option<Fact>,
    /// A till numbered past [`ACCOUNT_LOOKAHEAD`].
    pub lookahead: Option<String>,
    pub node: Option<NodeRow>,
    pub paid_after: SetupStepper,
    pub expiry: SetupStepper,
    pub prices: PricesRow,
    /// What the wallet warns of, each its own line in the warning colour.
    pub warnings: Vec<String>,
    /// "Till hears payments while it's open on screen."
    pub footnote: String,
    pub problem: Option<String>,
    /// "Disconnect this wallet", and the question before it.
    pub disconnect: Press,
    pub disconnect_footnote: String,
    pub disconnect_ask: String,
    pub disconnect_ask_detail: String,
    pub disconnect_confirm: String,
    /// "Change the wallet", as "Change the point of sale" is (core.md
    /// decision 261): setup's wallet step, back to Settings after. Off
    /// while a sale waits for payment.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub change: Option<Press>,
    /// Under it: what's kept, or why it's off.
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub change_footnote: String,
}

/// Under "Change the wallet": what changes and what's kept.
pub const CHANGE_FOOTNOTE: &str =
    "Connect another wallet. The point of sale, its sales and these settings stay.";
/// Under "Change the wallet" while a sale waits for payment.
pub const CHANGE_WAITING: &str = "A sale is waiting for payment. Another wallet wouldn't see it paid, so finish or close it first.";

/// The row of a till on the shop's light wallet server.
pub const LIGHT_SERVER_LABEL: &str = "Light wallet server";

/// Under the wallet's Disconnect.
pub const DISCONNECT_FOOTNOTE: &str = "Till forgets the wallet's keys and deletes its copy of the wallet from this device. Your money stays in your wallet.";
/// The question's detail: a view key can't be taken back.
pub const DISCONNECT_DETAIL: &str = "Payments already made aren't affected. Anyone who has the view key can still see payments: to stop that, move the money to a new wallet.";

/// "4A8xS…9fQ2": the start and end people check.
pub fn address_short(address: &str) -> String {
    let chars: Vec<char> = address.chars().collect();
    if chars.len() <= 12 {
        return address.to_owned();
    }
    let start: String = chars[..5].iter().collect();
    let end: String = chars[chars.len() - 4..].iter().collect();
    format!("{start}…{end}")
}

/// The wallet section, as `w` stands.
pub fn wallet_section(w: &WalletSettingsInput) -> WalletSection {
    let mut facts = Vec::new();
    if !w.address.is_empty() {
        facts.push(Fact::new("Wallet address", address_short(&w.address)).mono());
    }
    facts.push(Fact::new("Network", w.network.name()));
    if w.light_server.is_some()
        && let Some(node) = &w.node
    {
        facts.push(Fact::new("Proofs checked with", node_address(node)));
    }
    let numbered = w.fob.is_none();
    let prices = crate::prices::PricesInput {
        currency: w.currency.clone().unwrap_or_default(),
        plural: String::new(),
        provider: w.provider.clone(),
        providers: w.providers.clone(),
        checks: w.price_checks.clone(),
        from_fob: false,
        busy: w.busy,
        problem: None,
    };
    let light = w.light_server.is_some();
    // The row: the light wallet server in the node's place, with no
    // default servers to offer.
    let row = match &w.light_server {
        Some(server) => Some((LIGHT_SERVER_LABEL, server, Vec::new())),
        None => w.node.as_ref().map(|n| ("Node", n, w.node_choices.clone())),
    };
    WalletSection {
        title: "Payments".into(),
        mode: Fact::new(
            "How Till sees payments",
            if light {
                "Your light wallet server"
            } else {
                "Your own wallet"
            },
        ),
        fob: w
            .fob
            .as_ref()
            .map(|(name, label)| Fact::new("Connected to Fob", format!("{name} · {label}"))),
        facts,
        wallet_address: w.address.clone(),
        till_number: numbered.then(|| SetupStepper {
            label: "Till number".into(),
            value: w.till_number.to_string(),
            detail: Some(format!("In your wallet: account {}", w.till_number)),
            minus: Press::new("−").enabled(w.till_number > 1 && !w.busy),
            plus: Press::new("+").enabled(!w.busy),
            value_said: String::new(),
        }),
        account: (!numbered)
            .then(|| Fact::new("In your wallet", format!("Account {}", w.till_number))),
        lookahead: (w.till_number > ACCOUNT_LOOKAHEAD).then(|| {
            format!(
                "Till number {} is past the {ACCOUNT_LOOKAHEAD} accounts most wallets look at. Ask your wallet to look further, or it may not show this till's takings.",
                w.till_number
            )
        }),
        node: row.map(|(label, url, choices)| NodeRow {
            label: label.into(),
            value: node_address(url),
            url: url.clone(),
            change: Press::new("Change").enabled(!w.busy),
            checks: w
                .checks
                .iter()
                .map(|(text, mark)| NodeCheckRow {
                    text: text.clone(),
                    mark: *mark,
                })
                .collect(),
            choices,
            self_signed: light.then(|| self_signed_choice(w.light_self_signed, !w.busy)),
        }),
        // Shortened to fit beside its stepper (owner, 2026-11-13).
        paid_after: SetupStepper {
            detail: Some(confirmations_time(w.confirmations)),
            value_said: format!("{} confirmations", w.confirmations),
            ..step_stepper(
                "Paid after",
                format!("{} conf.", w.confirmations),
                w.confirmations,
                &crate::wallet_setup::CONFIRMATION_STEPS,
            )
        },
        expiry: step_stepper(
            "A code runs out after",
            expiry_text(w.expiry_minutes),
            w.expiry_minutes,
            &crate::wallet_setup::EXPIRY_STEPS,
        ),
        prices: PricesRow {
            label: "Prices from".into(),
            value: match &w.currency {
                _ if w.xmr_only => "None: XMR only".into(),
                Some(currency) => format!("{} · {currency}", w.provider),
                None => w.provider.clone(),
            },
            providers: if w.xmr_only {
                Vec::new()
            } else {
                crate::prices::provider_rows(&prices)
            },
            asking: (w.price_checks.is_none() && w.currency.is_some() && !w.xmr_only)
                .then(|| "Asking each provider…".into()),
            footnote: crate::prices::PRIVACY.into(),
        },
        warnings: w.warnings.clone(),
        footnote: if light {
            "Till hears payments while it's open on screen. Your light wallet server sees every payment to this till."
        } else {
            "Till hears payments while it's open on screen."
        }
        .into(),
        problem: w.problem.clone(),
        disconnect: Press::new("Disconnect this wallet").enabled(!w.busy),
        disconnect_footnote: DISCONNECT_FOOTNOTE.into(),
        disconnect_ask: "Disconnect this wallet?".into(),
        disconnect_ask_detail: DISCONNECT_DETAIL.into(),
        disconnect_confirm: "Disconnect".into(),
        change: Some(Press::new("Change the wallet").enabled(!w.busy && !w.live_sales)),
        change_footnote: if w.live_sales {
            CHANGE_WAITING
        } else {
            CHANGE_FOOTNOTE
        }
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> WalletSettingsInput {
        WalletSettingsInput {
            address: "4A8xSAMPLEADDRESS000000000000000000009fQ2".into(),
            network: WalletNetwork::Mainnet,
            node: Some("https://node.example.com:18089".into()),
            light_server: None,
            light_self_signed: false,
            till_number: 2,
            fob: None,
            confirmations: 10,
            expiry_minutes: 30,
            checks: vec![],
            node_choices: vec![],
            currency: Some("AUD".into()),
            provider: "Kraken".into(),
            providers: vec!["Kraken".into(), "CoinGecko".into()],
            price_checks: None,
            xmr_only: false,
            warnings: vec![],
            busy: false,
            problem: None,
            live_sales: false,
        }
    }

    #[test]
    fn a_wallet_typed_by_hand_shows_its_number_and_account() {
        let view = wallet_section(&input());
        assert_eq!(view.facts[0].value, "4A8xS…9fQ2");
        let number = view.till_number.unwrap();
        assert_eq!(number.detail.as_deref(), Some("In your wallet: account 2"));
        assert!(view.account.is_none() && view.fob.is_none() && view.lookahead.is_none());
        assert_eq!(view.paid_after.value, "10 conf.");
        assert_eq!(view.paid_after.value_said, "10 confirmations");
        assert!(!view.paid_after.minus.enabled);
        assert_eq!(view.prices.value, "Kraken · AUD");
        assert!(view.prices.providers[0].selected);
        assert_eq!(view.prices.asking.as_deref(), Some("Asking each provider…"));
        assert_eq!(view.node.unwrap().value, "node.example.com:18089");
    }

    #[test]
    fn through_a_light_server_the_row_is_the_servers_and_proofs_name_the_node() {
        let view = wallet_section(&WalletSettingsInput {
            light_server: Some("https://lws.example.com:8443".into()),
            node_choices: vec!["https://node.example.com:18089".into()],
            ..input()
        });
        assert_eq!(view.mode.value, "Your light wallet server");
        let row = view.node.unwrap();
        assert_eq!(
            (row.label.as_str(), row.value.as_str()),
            ("Light wallet server", "lws.example.com:8443")
        );
        assert!(row.choices.is_empty(), "no default servers");
        let switch = row.self_signed.unwrap();
        assert_eq!(switch.label, "Accept its own certificate", "setup's words");
        assert!(!switch.selected && switch.enabled);
        let proofs = view
            .facts
            .iter()
            .find(|f| f.label == "Proofs checked with")
            .unwrap();
        assert_eq!(proofs.value, "node.example.com:18089");
        assert!(view.footnote.contains("sees every payment"));
    }

    #[test]
    fn the_light_servers_certificate_switch_shows_the_link_and_waits_while_busy() {
        let light = WalletSettingsInput {
            light_server: Some("https://lws.example.com:8443".into()),
            light_self_signed: true,
            busy: true,
            ..input()
        };
        let switch = wallet_section(&light).node.unwrap().self_signed.unwrap();
        assert!(switch.selected && !switch.enabled);
        let node = wallet_section(&input()).node.unwrap();
        assert!(node.self_signed.is_none(), "a node has no switch");
    }

    #[test]
    fn through_fob_the_account_is_fobs_and_a_high_number_is_warned_of() {
        let view = wallet_section(&WalletSettingsInput {
            fob: Some(("Corner Café".into(), "Till (Corner Café)".into())),
            till_number: 51,
            warnings: vec!["Another till may be using till number 51.".into()],
            ..input()
        });
        assert!(view.till_number.is_none());
        assert_eq!(view.account.unwrap().value, "Account 51");
        assert_eq!(view.fob.unwrap().value, "Corner Café · Till (Corner Café)");
        assert!(
            view.lookahead
                .unwrap()
                .starts_with("Till number 51 is past the 50")
        );
        assert_eq!(view.warnings.len(), 1);
    }
}
