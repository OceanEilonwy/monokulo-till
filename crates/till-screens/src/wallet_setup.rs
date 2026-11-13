//! Setting up a till that watches the shop's own wallet (docs/view-key-mode.md
//! §11.1, boards Vk* on the canvas's "View-key mode" page): how Till sees
//! payments (V1), the wallet (V2), Connect to Fob (C1, and B4 or C4 once
//! connected), or the wallet entered by hand (D1 a wallet code, D2 the
//! address and view key, D3 a seed phrase), then This till (D4).
//!
//! The words are the designs', in sentence case where a design sets a
//! label in capitals (the platforms draw section labels in capitals, as
//! they do setup's others). The pages are drawn in setup's one column
//! (round 4), not the boards' older step rail.

use crate::view::{Choice, Device, Fact, Press};

/// How Till sees payments (V1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PaymentsMode {
    /// The store's Monokulo server: today's connect code.
    Monokulo,
    /// The shop's own wallet, through a Monero node.
    Wallet,
    /// The shop's own light wallet server.
    LightServer,
}

/// The Monero network a wallet entered by hand is on (D2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum WalletNetwork {
    Mainnet,
    Stagenet,
}

impl WalletNetwork {
    pub fn name(self) -> &'static str {
        match self {
            WalletNetwork::Mainnet => "Mainnet",
            WalletNetwork::Stagenet => "Stagenet",
        }
    }

    /// What a primary address starts with on it.
    fn primary_starts(self) -> char {
        match self {
            WalletNetwork::Mainnet => '4',
            WalletNetwork::Stagenet => '5',
        }
    }
}

/// One of V2's ways to enter the wallet yourself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum WalletWay {
    /// D1: scan a view-only wallet code.
    ScanCode,
    /// D2: type the address and view key.
    Keys,
    /// D3: a seed phrase, wiped after.
    Seed,
}

/// What's wrong with D2's address, as the engine found it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressIssue {
    NotAnAddress,
    OtherNetwork(WalletNetwork),
    NotPrimary,
}

/// What's wrong with D2's view key, as the engine found it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewKeyIssue {
    NotAKey,
    DoesntMatch,
}

/// A check's mark beside its line: D4's node checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CheckMark {
    Checking,
    Passed,
    Failed,
}

/// What the wallet steps are drawn from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletStep {
    /// V1: the way chosen so far.
    Mode { chosen: Option<PaymentsMode> },
    /// V2.
    Entry,
    /// C1: the request's code and pattern.
    FobRequest {
        qr: String,
        cells: Vec<bool>,
        ink: String,
        paper: String,
        problem: Option<String>,
        busy: bool,
    },
    /// B4 (phone) or C4 (iPad): what Fob handed over, and Till's own
    /// payment settings.
    FobConnected(FobConnectedSetup),
    /// D1.
    Code { problem: Option<String> },
    /// D2, as typed so far.
    Keys(KeysSetup),
    /// D3: how many words, and why the last words weren't taken.
    Seed { words: u8, problem: Option<String> },
    /// D4.
    ThisTill(ThisTillSetup),
}

/// What Fob handed over (B4, C4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FobConnectedSetup {
    pub wallet_name: String,
    /// "Till (Corner Café)".
    pub label: String,
    pub account: u32,
    /// Fob's node, when it shared it.
    pub node: Option<String>,
    /// Fob's price provider and currency, when it shared them.
    pub prices: Option<(String, String)>,
    pub confirmations: u32,
    pub expiry_minutes: u32,
    pub busy: bool,
    pub problem: Option<String>,
    /// Drawn as C4, the tablet's page, on any device: an iPad, or an
    /// Android tablet (till-app's `EngineConfig::large_screen`). B4 on a
    /// phone.
    pub tablet: bool,
}

/// D2 as typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeysSetup {
    pub network: WalletNetwork,
    pub address: String,
    /// Something is typed in the view key's field.
    pub view_key_typed: bool,
    /// `None` while the field is empty or right.
    pub address_issue: Option<AddressIssue>,
    pub view_key_issue: Option<ViewKeyIssue>,
    /// Both are right.
    pub ok: bool,
    pub busy: bool,
}

/// D4's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThisTillSetup {
    /// The till's number; `None` when Fob chose the account.
    pub number: Option<u32>,
    /// The node, unless Fob handed one over.
    pub node: Option<String>,
    pub checks: Vec<(String, CheckMark)>,
    /// Fob's default nodes, for Change.
    pub node_choices: Vec<String>,
    pub confirmations: u32,
    pub expiry_minutes: u32,
    pub busy: bool,
    pub problem: Option<String>,
}

/// V1.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ModeStep {
    pub choices: Vec<ModeChoice>,
    /// "With your own wallet or light wallet server, Till hears payments
    /// while it's open on screen."
    pub footnote: String,
    pub next: Press,
}

/// One of V1's three ways.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ModeChoice {
    pub mode: PaymentsMode,
    pub title: String,
    pub text: String,
    pub selected: bool,
    pub enabled: bool,
    /// Under the text when it can't be chosen yet: "Coming soon".
    pub note: Option<String>,
}

/// V2.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WalletEntryStep {
    /// The Fob card: "Connect to Fob" and what it does.
    pub fob_title: String,
    pub fob_text: String,
    pub connect: Press,
    /// "Or enter it yourself".
    pub section: String,
    pub ways: Vec<WalletWayRow>,
    pub back: Press,
    /// "Don't have Fob? Get it for Android".
    pub get_fob: Press,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WalletWayRow {
    pub way: WalletWay,
    pub title: String,
    pub text: String,
}

/// The pattern both screens show (board VkIdenticon): 25 cells row by row,
/// mirrored left to right, `true` in `ink` on `paper` (`#rrggbb`, the same
/// in light and dark).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct FobPattern {
    pub cells: Vec<bool>,
    pub ink: String,
    pub paper: String,
}

/// C1.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct FobRequestStep {
    /// What the QR code holds: the request, `fob://connect?v=1&…`.
    pub qr: String,
    pub pattern: FobPattern,
    /// Under the pattern: "Fob shows this same pattern".
    pub pattern_caption: String,
    /// Numbered from 1, `**bold**` for what to tap.
    pub steps: Vec<String>,
    pub back: Press,
    /// "Scan Fob's code": the camera opens for Fob's answer
    /// (`Action::FobAnswer`).
    pub scan: Press,
    /// Over the camera while it looks.
    pub looking: String,
    pub problem: Option<String>,
}

/// A value with − and + (D4, B4, C4).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SetupStepper {
    /// "Paid after".
    pub label: String,
    /// "10 confirmations".
    pub value: String,
    /// "About 20 minutes".
    pub detail: Option<String>,
    pub minus: Press,
    pub plus: Press,
}

/// B4 (phone) and C4 (iPad).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct FobConnectedStep {
    /// Phone, over the heading: "Set up Till · Your wallet".
    pub eyebrow: Option<String>,
    /// Wallet, Account, and Node and Prices when Fob shared them.
    pub facts: Vec<Fact>,
    /// "Payments · set in Till".
    pub section: String,
    pub paid_after: SetupStepper,
    pub expiry: SetupStepper,
    pub footnote: String,
    /// iPad: "Use another wallet".
    pub other_wallet: Option<Press>,
    pub next: Press,
    pub problem: Option<String>,
}

/// D1.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WalletCodeStep {
    /// Over the camera: "Hold the code inside the frame".
    pub looking: String,
    pub paste: Press,
    pub footnote: String,
    /// In place of the camera until it's allowed, and its button.
    pub camera_ask: String,
    pub allow_camera: String,
    pub back: Press,
    pub problem: Option<String>,
}

/// One of D2's fields: its label, and what's said under it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct KeyField {
    pub label: String,
    /// What's in the field as the engine last heard it: the address as
    /// typed; never the view key, which is only ever `filled`.
    pub value: String,
    /// Something is typed in it.
    pub filled: bool,
    /// Under the field once it's judged: "A Mainnet address" with a tick,
    /// or what's wrong.
    pub note: Option<String>,
    /// `note` says it's right.
    pub good: bool,
}

/// D2.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WalletKeysStep {
    /// "Network".
    pub network_label: String,
    /// Mainnet, then Stagenet: `Action::EditWalletKeys`' network.
    pub networks: Vec<Choice>,
    pub address: KeyField,
    pub view_key: KeyField,
    /// "Where do I find these?" and its answer.
    pub help_title: String,
    pub help: String,
    pub back: Press,
    pub next: Press,
}

/// D3.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WalletSeedStep {
    /// The warning, ending where its link begins.
    pub warning: String,
    /// "scan one instead": to D1.
    pub scan_instead: Press,
    /// "25 words", "16 words (Polyseed)": `Action::ChooseSeedLength`.
    pub lengths: Vec<Choice>,
    /// How many fields: 25 or 16.
    pub words: u8,
    pub footnote: String,
    pub back: Press,
    pub next: Press,
    pub problem: Option<String>,
}

/// One of D4's node checks.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NodeCheckRow {
    pub text: String,
    pub mark: CheckMark,
}

/// D4's node: where it is, Change, and Fob's checks.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NodeRow {
    pub label: String,
    /// "node.example.com:18089".
    pub value: String,
    /// The whole URL, for Change's field.
    pub url: String,
    pub change: Press,
    pub checks: Vec<NodeCheckRow>,
    /// Fob's default nodes, offered under Change.
    pub choices: Vec<String>,
}

/// D4.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ThisTillStep {
    /// "Till number", unless Fob chose the account.
    pub number: Option<SetupStepper>,
    /// The node, unless Fob handed one over.
    pub node: Option<NodeRow>,
    pub paid_after: SetupStepper,
    pub expiry: SetupStepper,
    pub back: Press,
    pub next: Press,
    pub problem: Option<String>,
}

/// Paid after's steps: Settled Confirmations, 10 to 720 (fob-screens'
/// `SETTLED_MIN` and `SETTLED_MAX`).
pub const CONFIRMATION_STEPS: [u32; 11] = [10, 15, 20, 30, 45, 60, 90, 120, 180, 360, 720];

/// How long a code lasts: its steps, in minutes.
pub const EXPIRY_STEPS: [u32; 7] = [10, 15, 20, 30, 45, 60, 120];

/// The step after `at` in `steps`, up or down; `at` itself at either end.
/// A value between two steps moves to the nearer one in that direction.
pub fn step(steps: &[u32], at: u32, up: bool) -> u32 {
    if up {
        steps.iter().copied().find(|s| *s > at).unwrap_or(at)
    } else {
        steps.iter().rev().copied().find(|s| *s < at).unwrap_or(at)
    }
}

/// What V1 shows.
pub fn mode_step(chosen: Option<PaymentsMode>) -> ModeStep {
    let choice = |mode, title: &str, text: &str, enabled: bool| ModeChoice {
        mode,
        title: title.into(),
        text: text.into(),
        selected: chosen == Some(mode),
        enabled,
        note: (!enabled).then(|| "Coming soon".into()),
    };
    ModeStep {
        choices: vec![
            choice(
                PaymentsMode::Monokulo,
                "Monokulo",
                "Your store's Monokulo server tells Till. Alerts arrive even while Till is closed.",
                true,
            ),
            choice(
                PaymentsMode::Wallet,
                "Your own wallet",
                "Till watches your wallet itself, through a Monero node. No server needed.",
                true,
            ),
            // The light wallet server's backend isn't built yet
            // (docs/view-key-mode.md §5.7): shown, not chosen.
            choice(
                PaymentsMode::LightServer,
                "Your light wallet server",
                "A light wallet server you run watches your wallet for Till.",
                false,
            ),
        ],
        footnote: "With your own wallet or light wallet server, Till hears payments while it's open on screen.".into(),
        next: Press::new("Continue").enabled(chosen.is_some()),
    }
}

/// What V2 shows.
pub fn entry_step() -> WalletEntryStep {
    let way = |way, title: &str, text: &str| WalletWayRow {
        way,
        title: title.into(),
        text: text.into(),
    };
    WalletEntryStep {
        fob_title: "Connect to Fob".into(),
        fob_text: "Choose a wallet in Fob, or make one for the shop, and allow Till. Fob sets up the rest.".into(),
        connect: Press::new("Connect to Fob"),
        section: "Or enter it yourself".into(),
        ways: vec![
            way(
                WalletWay::ScanCode,
                "Scan a wallet code",
                "A view-only wallet code from Feather, Cake or Fob",
            ),
            way(
                WalletWay::Keys,
                "Type the address and view key",
                "Both are in your wallet's keys",
            ),
            way(
                WalletWay::Seed,
                "Enter a seed phrase",
                "Till keeps only the view key and wipes the words",
            ),
        ],
        back: Press::new("Back"),
        get_fob: Press::new("Don't have Fob? Get it for Android"),
    }
}

/// "this iPad", "this iPhone", "this phone".
fn this(device: Device) -> &'static str {
    match device {
        Device::Ipad => "this iPad",
        Device::Iphone => "this iPhone",
        Device::Android => "this device",
    }
}

/// What C1 shows.
pub fn fob_request_step(
    device: Device,
    qr: &str,
    cells: &[bool],
    ink: &str,
    paper: &str,
    problem: Option<&str>,
    busy: bool,
) -> FobRequestStep {
    FobRequestStep {
        qr: qr.into(),
        pattern: FobPattern {
            cells: cells.to_vec(),
            ink: ink.into(),
            paper: paper.into(),
        },
        pattern_caption: "Fob shows this same pattern".into(),
        steps: vec![
            "Open Fob, tap **Scan** and scan this code.".into(),
            "Check the pattern matches, then choose a wallet and allow Till.".into(),
            format!(
                "Fob shows a code back. Tap **Scan Fob's code** and hold the phone up to {}.",
                this(device)
            ),
        ],
        back: Press::new("Back"),
        scan: Press::new("Scan Fob's code").busy(busy).enabled(!busy),
        looking: "Looking for Fob's code…".into(),
        problem: problem.map(str::to_owned),
    }
}

/// "About 20 minutes" for `confirmations` (a block every two minutes).
pub fn confirmations_time(confirmations: u32) -> String {
    let minutes = confirmations.saturating_mul(2);
    if minutes < 120 {
        format!("About {minutes} minutes")
    } else {
        format!("About {} hours", minutes.div_ceil(60))
    }
}

/// "30 minutes", "1 hour", "2 hours".
pub fn expiry_text(minutes: u32) -> String {
    match minutes {
        60 => "1 hour".into(),
        m if m > 60 && m % 60 == 0 => format!("{} hours", m / 60),
        m => format!("{m} minutes"),
    }
}

fn paid_after(confirmations: u32, merged: bool) -> SetupStepper {
    let count = format!("{confirmations} confirmations");
    let time = confirmations_time(confirmations);
    let (value, detail) = if merged {
        // B4's one line, in Settings' words: "10 confirmations · about 20
        // minutes".
        (format!("{count} · {}", time.replacen("About", "about", 1)), None)
    } else {
        (count, Some(time))
    };
    SetupStepper {
        label: "Paid after".into(),
        value,
        detail,
        minus: Press::new("−").enabled(confirmations > CONFIRMATION_STEPS[0]),
        plus: Press::new("+")
            .enabled(confirmations < CONFIRMATION_STEPS[CONFIRMATION_STEPS.len() - 1]),
    }
}

/// A stepper at `at` on `steps`, its − and + off at either end.
pub fn step_stepper(label: &str, value: String, at: u32, steps: &[u32]) -> SetupStepper {
    SetupStepper {
        label: label.into(),
        value,
        detail: None,
        minus: Press::new("−").enabled(steps.first().is_some_and(|s| at > *s)),
        plus: Press::new("+").enabled(steps.last().is_some_and(|s| at < *s)),
    }
}

fn expiry(minutes: u32) -> SetupStepper {
    SetupStepper {
        label: "A code runs out after".into(),
        value: expiry_text(minutes),
        detail: None,
        minus: Press::new("−").enabled(minutes > EXPIRY_STEPS[0]),
        plus: Press::new("+").enabled(minutes < EXPIRY_STEPS[EXPIRY_STEPS.len() - 1]),
    }
}

/// "node.example.com:18089" from "https://node.example.com:18089/".
pub fn node_address(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.trim_end_matches('/').to_owned()
}

/// "node.example.com", without a port.
fn node_host(url: &str) -> String {
    let address = node_address(url);
    match address.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => host.to_owned(),
        _ => address,
    }
}

/// What B4 (phone) or C4 (iPad, Android tablet) shows.
pub fn fob_connected_step(c: &FobConnectedSetup, device: Device) -> FobConnectedStep {
    let pad = c.tablet || device == Device::Ipad;
    let mut facts = vec![
        Fact::new("Wallet", &c.wallet_name),
        Fact::new(
            "Account",
            if pad {
                format!("{} · account {}", c.label, c.account)
            } else {
                format!("{} · {}", c.label, c.account)
            },
        ),
    ];
    if let Some(node) = &c.node {
        facts.push(Fact::new("Node", format!("{} · from Fob", node_host(node))));
    }
    if let Some((provider, currency)) = &c.prices {
        facts.push(Fact::new(
            "Prices",
            format!("{provider} · {currency} · from Fob"),
        ));
    }
    FobConnectedStep {
        eyebrow: (!pad).then(|| "Set up Till · Your wallet".into()),
        facts,
        section: "Payments · set in Till".into(),
        paid_after: paid_after(c.confirmations, !pad),
        expiry: expiry(c.expiry_minutes),
        footnote: if pad {
            "Till's own settings, for your wallet or light wallet server. Change any of these later in Settings."
        } else {
            "Till's own settings, for your wallet or light wallet server. Till hears payments while it's open on screen."
        }
        .into(),
        other_wallet: pad.then(|| Press::new("Use another wallet").enabled(!c.busy)),
        next: Press::new("Continue").busy(c.busy).enabled(!c.busy),
        problem: c.problem.clone(),
    }
}

/// B4's and C4's lead: what Fob set up besides the keys.
pub fn fob_connected_lead(c: &FobConnectedSetup, device: Device) -> String {
    let both = c.node.is_some() && c.prices.is_some();
    let set_up = match (c.node.is_some(), c.prices.is_some()) {
        (true, true) => " Fob set up the node and prices too",
        (true, false) => " Fob set up the node too",
        (false, true) => " Fob set up the prices too",
        (false, false) => "",
    };
    let skipped = if (c.tablet || device == Device::Ipad) && both {
        ", so the next steps are skipped."
    } else if set_up.is_empty() {
        ""
    } else {
        "."
    };
    format!("Till can see payments to its own account.{set_up}{skipped}")
}

/// What D1 shows.
pub fn code_step(problem: Option<&str>) -> WalletCodeStep {
    WalletCodeStep {
        looking: "Hold the code inside the frame".into(),
        paste: Press::new("Paste a code instead"),
        footnote: "A code with seed words works too. Till takes only the view key from it and wipes the rest.".into(),
        camera_ask: "Allow the camera to scan the code, or paste it below.".into(),
        allow_camera: "Allow the camera".into(),
        back: Press::new("Back"),
        problem: problem.map(str::to_owned),
    }
}

/// D2's lead: which address to use, on the network chosen.
pub fn keys_lead(network: WalletNetwork) -> String {
    format!(
        "Both are in your wallet's keys. Use the wallet's main address, the one starting with {}.",
        network.primary_starts()
    )
}

/// What D2 shows.
pub fn keys_step(k: &KeysSetup) -> WalletKeysStep {
    let network = |n: WalletNetwork| Choice {
        label: n.name().into(),
        detail: None,
        selected: k.network == n,
        enabled: true,
    };
    let address_note = match k.address_issue {
        None if k.address.trim().is_empty() => None,
        None => Some((format!("A {} address", k.network.name()), true)),
        Some(AddressIssue::NotAnAddress) => {
            Some((format!("This isn't a {} address.", k.network.name()), false))
        }
        Some(AddressIssue::OtherNetwork(other)) => Some((
            format!(
                "This is a {} address. Choose {} above, or use a {} address.",
                other.name(),
                other.name(),
                k.network.name()
            ),
            false,
        )),
        Some(AddressIssue::NotPrimary) => Some((
            format!(
                "This is a subaddress. Use the wallet's main address, the one starting with {}.",
                k.network.primary_starts()
            ),
            false,
        )),
    };
    let view_note = match k.view_key_issue {
        None => None,
        Some(ViewKeyIssue::NotAKey) => Some(
            "A private view key is 64 letters and numbers, 0 to 9 and a to f.".to_owned(),
        ),
        Some(ViewKeyIssue::DoesntMatch) => Some(
            "This view key doesn't match the address. Check you copied the private view key, not the public one."
                .to_owned(),
        ),
    };
    WalletKeysStep {
        network_label: "Network".into(),
        networks: vec![network(WalletNetwork::Mainnet), network(WalletNetwork::Stagenet)],
        address: KeyField {
            label: "Primary address".into(),
            value: k.address.clone(),
            filled: !k.address.trim().is_empty(),
            good: address_note.as_ref().is_some_and(|(_, good)| *good),
            note: address_note.map(|(note, _)| note),
        },
        view_key: KeyField {
            label: "Private view key".into(),
            value: String::new(),
            filled: k.view_key_typed,
            good: false,
            note: view_note,
        },
        help_title: "Where do I find these?".into(),
        help: "Most wallets show them under Keys in the wallet's settings. Till never asks for the spend key.".into(),
        back: Press::new("Back"),
        next: Press::new("Continue").enabled(k.ok && !k.busy).busy(k.busy),
    }
}

/// What D3 shows.
pub fn seed_step(words: u8, problem: Option<&str>) -> WalletSeedStep {
    let length = |n: u8, label: &str| Choice {
        label: label.into(),
        detail: None,
        selected: words == n,
        enabled: true,
    };
    WalletSeedStep {
        warning: "A till is shared and often left alone. Till takes the view key from these words, then wipes them. It never keeps them and can't spend. A view-only code is safer:".into(),
        scan_instead: Press::new("scan one instead"),
        lengths: vec![length(25, "25 words"), length(16, "16 words (Polyseed)")],
        words,
        footnote: "The keyboard won't suggest or learn these words. Till clears them from the screen as soon as you continue.".into(),
        back: Press::new("Back"),
        next: Press::new("Continue"),
        problem: problem.map(str::to_owned),
    }
}

/// What D4 shows.
pub fn this_till_step(t: &ThisTillSetup) -> ThisTillStep {
    ThisTillStep {
        number: t.number.map(|n| SetupStepper {
            label: "Till number".into(),
            value: n.to_string(),
            detail: Some(format!("Account {n} in your wallet")),
            minus: Press::new("−").enabled(n > 1),
            plus: Press::new("+"),
        }),
        node: t.node.as_ref().map(|url| NodeRow {
            label: "Node".into(),
            value: node_address(url),
            url: url.clone(),
            change: Press::new("Change"),
            checks: t
                .checks
                .iter()
                .map(|(text, mark)| NodeCheckRow {
                    text: text.clone(),
                    mark: *mark,
                })
                .collect(),
            choices: t.node_choices.clone(),
        }),
        paid_after: paid_after(t.confirmations, false),
        expiry: expiry(t.expiry_minutes),
        back: Press::new("Back"),
        next: Press::new("Continue")
            .enabled(!t.busy && !t.checks.iter().any(|(_, m)| *m == CheckMark::Failed))
            .busy(t.busy),
        problem: t.problem.clone(),
    }
}

/// D4's lead: with the till's number, or (connected through Fob, which
/// chose the account) without it.
pub fn this_till_lead(numbered: bool) -> &'static str {
    if numbered {
        "Give each till its own number. Its payments go to that account in your wallet."
    } else {
        "Fob chose this till's account. Pick the node Till watches it through."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> FobConnectedSetup {
        FobConnectedSetup {
            wallet_name: "Corner Café".into(),
            label: "Till (Corner Café)".into(),
            account: 1,
            node: Some("https://node.example.com:18089".into()),
            prices: Some(("Kraken".into(), "AUD".into())),
            confirmations: 10,
            expiry_minutes: 30,
            busy: false,
            problem: None,
            tablet: false,
        }
    }

    #[test]
    fn v1_offers_three_ways_and_continue_waits_for_one() {
        let view = mode_step(None);
        let titles: Vec<&str> = view.choices.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Monokulo", "Your own wallet", "Your light wallet server"]
        );
        assert!(!view.next.enabled);
        assert!(!view.choices[2].enabled);
        assert_eq!(view.choices[2].note.as_deref(), Some("Coming soon"));
        let view = mode_step(Some(PaymentsMode::Wallet));
        assert!(view.next.enabled && view.choices[1].selected);
    }

    #[test]
    fn connected_says_what_fob_set_up_on_ipad_and_on_a_phone() {
        let c = connected();
        let pad = fob_connected_step(&c, Device::Ipad);
        let facts: Vec<(&str, &str)> = pad
            .facts
            .iter()
            .map(|f| (f.label.as_str(), f.value.as_str()))
            .collect();
        assert_eq!(
            facts,
            [
                ("Wallet", "Corner Café"),
                ("Account", "Till (Corner Café) · account 1"),
                ("Node", "node.example.com · from Fob"),
                ("Prices", "Kraken · AUD · from Fob"),
            ]
        );
        assert_eq!(pad.paid_after.value, "10 confirmations");
        assert_eq!(pad.paid_after.detail.as_deref(), Some("About 20 minutes"));
        assert!(!pad.paid_after.minus.enabled && pad.paid_after.plus.enabled);
        assert_eq!(pad.expiry.value, "30 minutes");
        assert_eq!(pad.other_wallet.unwrap().label, "Use another wallet");
        assert_eq!(
            fob_connected_lead(&c, Device::Ipad),
            "Till can see payments to its own account. Fob set up the node and prices too, so the next steps are skipped."
        );

        let phone = fob_connected_step(&c, Device::Android);
        assert_eq!(phone.eyebrow.as_deref(), Some("Set up Till · Your wallet"));
        assert_eq!(phone.facts[1].value, "Till (Corner Café) · 1");
        assert_eq!(phone.paid_after.value, "10 confirmations · about 20 minutes");
        assert!(phone.other_wallet.is_none());
        assert_eq!(
            fob_connected_lead(&c, Device::Iphone),
            "Till can see payments to its own account. Fob set up the node and prices too."
        );
        let bare = FobConnectedSetup {
            node: None,
            prices: None,
            ..connected()
        };
        assert_eq!(
            fob_connected_lead(&bare, Device::Ipad),
            "Till can see payments to its own account."
        );
    }

    #[test]
    fn an_android_tablet_gets_the_ipads_c4() {
        let c = FobConnectedSetup {
            tablet: true,
            ..connected()
        };
        let tablet = fob_connected_step(&c, Device::Android);
        assert_eq!(tablet, fob_connected_step(&c, Device::Ipad));
        assert!(tablet.eyebrow.is_none());
        assert_eq!(tablet.facts[1].value, "Till (Corner Café) · account 1");
        assert_eq!(tablet.paid_after.value, "10 confirmations");
        assert_eq!(
            tablet.paid_after.detail.as_deref(),
            Some("About 20 minutes")
        );
        assert_eq!(tablet.other_wallet.unwrap().label, "Use another wallet");
        assert_eq!(
            fob_connected_lead(&c, Device::Android),
            fob_connected_lead(&c, Device::Ipad)
        );
    }

    #[test]
    fn paid_after_and_expiry_step_through_their_ladders() {
        assert_eq!(step(&CONFIRMATION_STEPS, 10, true), 15);
        assert_eq!(step(&CONFIRMATION_STEPS, 10, false), 10);
        assert_eq!(step(&CONFIRMATION_STEPS, 720, true), 720);
        assert_eq!(step(&CONFIRMATION_STEPS, 25, false), 20);
        assert_eq!(confirmations_time(720), "About 24 hours");
        assert_eq!(confirmations_time(45), "About 90 minutes");
        assert_eq!(expiry_text(60), "1 hour");
        assert_eq!(expiry_text(120), "2 hours");
        assert_eq!(step(&EXPIRY_STEPS, 30, true), 45);
    }

    #[test]
    fn d2_says_what_is_wrong_with_each_field() {
        let typed = |address_issue, view_key_issue| KeysSetup {
            network: WalletNetwork::Mainnet,
            address: "48Efg".into(),
            view_key_typed: true,
            address_issue,
            view_key_issue,
            ok: false,
            busy: false,
        };
        let view = keys_step(&typed(None, Some(ViewKeyIssue::DoesntMatch)));
        assert_eq!(view.address.note.as_deref(), Some("A Mainnet address"));
        assert!(view.address.good);
        assert_eq!(
            view.view_key.note.as_deref(),
            Some(
                "This view key doesn't match the address. Check you copied the private view key, not the public one."
            )
        );
        assert!(!view.next.enabled);
        let view = keys_step(&typed(
            Some(AddressIssue::OtherNetwork(WalletNetwork::Stagenet)),
            None,
        ));
        assert!(!view.address.good);
        assert!(
            view.address
                .note
                .unwrap()
                .starts_with("This is a Stagenet address.")
        );
        assert_eq!(
            keys_lead(WalletNetwork::Stagenet),
            "Both are in your wallet's keys. Use the wallet's main address, the one starting with 5."
        );
    }

    #[test]
    fn d4_shows_the_number_unless_fob_chose_it_and_waits_on_a_failed_check() {
        let mut t = ThisTillSetup {
            number: Some(1),
            node: Some("https://node.example.com:18089".into()),
            checks: vec![
                ("Answers in 0.4 s".into(), CheckMark::Passed),
                ("Mainnet".into(), CheckMark::Passed),
                ("Up to date".into(), CheckMark::Passed),
            ],
            node_choices: vec![],
            confirmations: 10,
            expiry_minutes: 30,
            busy: false,
            problem: None,
        };
        let view = this_till_step(&t);
        let number = view.number.unwrap();
        assert_eq!(
            (number.value.as_str(), number.detail.as_deref()),
            ("1", Some("Account 1 in your wallet"))
        );
        assert!(!number.minus.enabled);
        assert_eq!(view.node.unwrap().value, "node.example.com:18089");
        assert!(view.next.enabled);
        t.checks[1].1 = CheckMark::Failed;
        t.number = None;
        let view = this_till_step(&t);
        assert!(view.number.is_none());
        assert!(!view.next.enabled);
    }
}
