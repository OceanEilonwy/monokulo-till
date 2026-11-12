//! Setting up a till that watches the shop's own wallet
//! (docs/view-key-mode.md §11.1, core.md decision 232): how Till sees
//! payments (V1), then the wallet, by Connect to Fob (C1, then B4 or C4)
//! or entered by hand (D1, D2, D3), then This till (D4), after which the
//! wallet file is made and the till goes on to the point of sale as a
//! Monokulo till does.
//!
//! What's typed is held here only until the wallet is made: the view key
//! in a buffer wiped when it goes, seed words never at all (they're turned
//! into the view key the moment they arrive, and wiped).

use std::path::PathBuf;
use std::sync::Arc;

use till_payments::FobWallet;
use till_screens::onboarding::Step;
use till_screens::wallet_setup::{
    AddressIssue, CONFIRMATION_STEPS, CheckMark, EXPIRY_STEPS, FobConnectedSetup, KeysSetup,
    PaymentsMode, ThisTillSetup, ViewKeyIssue, WalletNetwork, WalletStep, WalletWay, step,
};
use till_wallet::keys::{self, AddressProblem, CodeProblem, SeedProblem, ViewKeyProblem};
use till_wallet::setup::{self, CheckState};
use till_wallet::{
    AnswerProblem, FobRequest, Granted, Network, ViewKeys, WalletBackend, WalletSetup,
};
use zeroize::Zeroizing;

use till_sales::RunnerSide;

use super::{State, apply};
use crate::{Effect, Screen};

/// Where "Don't have Fob? Get it for Android" goes: Fob's releases.
pub(crate) const FOB_URL: &str = "https://github.com/OceanEilonwy/monokulo-fob/releases";

/// The wallet as setup has it so far.
pub(crate) struct WalletForm {
    /// V1's choice.
    pub mode: Option<PaymentsMode>,
    /// C1's request, holding its one-time key until Fob answers.
    request: Option<Arc<FobRequest>>,
    /// What Fob handed over.
    granted: Option<Granted>,
    /// The keys entered by hand, once taken (D1, D2, D3).
    keys: Option<ViewKeys>,
    /// The screen the keys were entered on, for D4's Back.
    entered_on: Screen,
    /// D2 as typed.
    network: WalletNetwork,
    address: String,
    view_key: Zeroizing<String>,
    /// D3's length.
    seed_words: u8,
    /// Why the last code, answer or seed wasn't taken.
    problem: Option<String>,
    /// D4.
    till_number: u32,
    node: Option<String>,
    checks: Vec<(String, CheckMark)>,
    /// Bumped with each check, so an older one's answer is dropped.
    checking: u64,
    confirmations: u32,
    expiry_minutes: u32,
}

impl WalletForm {
    /// A fresh form: a debug build sets up on stagenet, where Fob starts
    /// too; a release build on mainnet.
    pub(crate) fn new(debug: bool) -> WalletForm {
        WalletForm {
            mode: None,
            request: None,
            granted: None,
            keys: None,
            entered_on: Screen::WalletEntry,
            network: if debug {
                WalletNetwork::Stagenet
            } else {
                WalletNetwork::Mainnet
            },
            address: String::new(),
            view_key: Zeroizing::new(String::new()),
            seed_words: 25,
            problem: None,
            till_number: 1,
            node: None,
            checks: Vec::new(),
            checking: 0,
            confirmations: till_payments::DEFAULT_CONFIRMATIONS,
            expiry_minutes: till_payments::DEFAULT_EXPIRY_MINUTES,
        }
    }

    fn network(&self) -> Network {
        network_of(self.network)
    }
}

fn network_of(network: WalletNetwork) -> Network {
    match network {
        WalletNetwork::Mainnet => Network::Mainnet,
        WalletNetwork::Stagenet => Network::Stagenet,
    }
}

fn wallet_network(network: Network) -> WalletNetwork {
    match network {
        Network::Mainnet => WalletNetwork::Mainnet,
        Network::Stagenet => WalletNetwork::Stagenet,
    }
}

fn answer_problem(problem: AnswerProblem) -> &'static str {
    match problem {
        AnswerProblem::NotAnAnswer => {
            "That isn't Fob's code. In Fob, allow Till, then scan the code Fob shows."
        }
        AnswerProblem::NotForThisRequest => {
            "That code is for another request. Scan the code Fob shows for this one."
        }
        AnswerProblem::Declined => "Fob didn't give access. Connect again, or enter it yourself.",
        AnswerProblem::BadGrant => "Fob's answer holds no wallet Till can watch.",
    }
}

fn code_problem(problem: CodeProblem) -> &'static str {
    match problem {
        CodeProblem::NotACode => "That isn't a wallet code.",
        CodeProblem::BadKeys => {
            "That code's keys don't match each other. Show the wallet's view-only code again."
        }
    }
}

fn seed_problem(problem: SeedProblem, wanted: u8) -> String {
    match problem {
        SeedProblem::WordCount(count) => format!("Enter all {wanted} words: there are {count}."),
        SeedProblem::NotASeed => {
            "These words aren't a seed phrase. Check each word and their order.".into()
        }
    }
}

impl State {
    fn form(&mut self) -> &mut WalletForm {
        &mut self.wallet
    }

    /// V1's choice. The light wallet server isn't offered yet.
    pub(super) fn choose_payments_mode(&mut self, mode: PaymentsMode) {
        if mode != PaymentsMode::LightServer {
            self.form().mode = Some(mode);
        }
    }

    /// V1's Continue.
    pub(super) fn payments_mode_next(&mut self) {
        match self.wallet.mode {
            Some(PaymentsMode::Monokulo) => self.go(Screen::Scan),
            Some(PaymentsMode::Wallet) => self.go(Screen::WalletEntry),
            _ => {}
        }
    }

    /// V2's "Connect to Fob": a new request, shown on C1.
    pub(super) fn connect_fob(&mut self) {
        let network = self.wallet.network();
        match FobRequest::new("", &self.config.device_name, network, None) {
            Ok(request) => {
                let form = self.form();
                form.request = Some(Arc::new(request));
                form.granted = None;
                form.problem = None;
                self.go(Screen::FobRequest);
            }
            Err(e) => self.problem = Some(format!("Couldn't make a request for Fob: {e}")),
        }
    }

    pub(super) fn get_fob(&mut self) {
        self.effect(Effect::OpenUrl {
            url: FOB_URL.into(),
        });
    }

    pub(super) fn choose_wallet_way(&mut self, way: WalletWay) {
        self.form().problem = None;
        self.go(match way {
            WalletWay::ScanCode => Screen::WalletCode,
            WalletWay::Keys => Screen::WalletKeys,
            WalletWay::Seed => Screen::WalletSeed,
        });
    }

    /// Fob's answer: connected, on to B4 or C4; else C1 says why.
    pub(super) fn fob_answer(&mut self, text: String) {
        let text = Zeroizing::new(text);
        let Some(request) = self.wallet.request.clone() else {
            return;
        };
        match request.open(&text) {
            Ok(granted) => {
                let form = self.form();
                form.granted = Some(granted);
                form.problem = None;
                // The request's key is spent: dropped, and wiped with it.
                form.request = None;
                self.go(Screen::FobConnected);
            }
            Err(problem) => self.form().problem = Some(answer_problem(problem).into()),
        }
    }

    /// C4's "Use another wallet".
    pub(super) fn use_another_wallet(&mut self) {
        let form = self.form();
        form.granted = None;
        form.request = None;
        self.go(Screen::WalletEntry);
    }

    /// D1: a code scanned or pasted. `None`: nothing to paste.
    pub(super) fn wallet_code(&mut self, text: Option<String>) {
        let text = text.map(Zeroizing::new);
        let Some(text) = text.as_ref().filter(|t| !t.trim().is_empty()) else {
            self.form().problem =
                Some("There's nothing to paste. Copy the wallet code first.".into());
            return;
        };
        match keys::keys_from_code(text, self.wallet.network()) {
            Ok(keys) => self.keys_taken(keys, Screen::WalletCode),
            Err(problem) => self.form().problem = Some(code_problem(problem).into()),
        }
    }

    /// D2 as typed.
    pub(super) fn edit_wallet_keys(
        &mut self,
        network: WalletNetwork,
        address: String,
        view_key: String,
    ) {
        let form = self.form();
        form.network = network;
        form.address = address;
        form.view_key = Zeroizing::new(view_key);
    }

    /// D2's Continue, once both are right.
    pub(super) fn wallet_keys_next(&mut self) {
        let form = &self.wallet;
        if let Some(keys) = keys::keys_from_typed(form.network(), &form.address, &form.view_key) {
            self.keys_taken(keys, Screen::WalletKeys);
        }
    }

    pub(super) fn choose_seed_length(&mut self, words: u8) {
        if words == 25 || words == 16 {
            let form = self.form();
            form.seed_words = words;
            form.problem = None;
        }
    }

    /// D3's Continue: the view key taken from the words, which are wiped.
    pub(super) fn enter_seed(&mut self, words: String) {
        let words = Zeroizing::new(words);
        let wanted = self.wallet.seed_words;
        match keys::keys_from_seed(self.wallet.network(), words) {
            Ok(keys) => self.keys_taken(keys, Screen::WalletSeed),
            Err(problem) => self.form().problem = Some(seed_problem(problem, wanted)),
        }
    }

    /// The keys entered by hand: on to D4.
    fn keys_taken(&mut self, keys: ViewKeys, on: Screen) {
        let form = self.form();
        form.network = wallet_network(keys.network);
        form.keys = Some(keys);
        form.granted = None;
        form.entered_on = on;
        form.problem = None;
        // What was typed is no longer needed.
        form.view_key = Zeroizing::new(String::new());
        self.enter_this_till();
    }

    /// D4, its node checked as it opens.
    fn enter_this_till(&mut self) {
        if self.wallet.node.is_none() {
            let node = setup::default_node(self.wallet.network());
            self.set_node(node);
        } else if self.wallet.checks.is_empty() {
            let node = self.wallet.node.clone().unwrap_or_default();
            self.set_node(node);
        }
        self.go(Screen::ThisTill);
    }

    pub(super) fn step_till_number(&mut self, up: bool) {
        let form = self.form();
        form.till_number = if up {
            form.till_number.saturating_add(1)
        } else {
            form.till_number.saturating_sub(1).max(1)
        };
    }

    pub(super) fn step_paid_after(&mut self, up: bool) {
        let form = self.form();
        form.confirmations = step(&CONFIRMATION_STEPS, form.confirmations, up);
    }

    pub(super) fn step_code_expiry(&mut self, up: bool) {
        let form = self.form();
        form.expiry_minutes = step(&EXPIRY_STEPS, form.expiry_minutes, up);
    }

    /// D4's node, checked now as Fob checks one.
    pub(super) fn set_node(&mut self, url: String) {
        let url = url.trim().to_owned();
        if url.is_empty() {
            return;
        }
        let network = self.wallet.network();
        let form = self.form();
        form.node = Some(url.clone());
        form.checks = vec![("Checking the node…".into(), CheckMark::Checking)];
        form.checking += 1;
        let token = form.checking;
        self.task(async move {
            let checks = setup::check_node(network, &url).await;
            apply(move |s| {
                if s.wallet.checking != token {
                    return;
                }
                s.wallet.checks = checks
                    .into_iter()
                    .map(|c| {
                        let mark = match c.state {
                            CheckState::Checking => CheckMark::Checking,
                            CheckState::Passed => CheckMark::Passed,
                            CheckState::Failed => CheckMark::Failed,
                        };
                        (c.text, mark)
                    })
                    .collect();
            })
        });
    }

    /// B4's and C4's Continue: D4 for the node when Fob shared none, else
    /// the wallet made.
    pub(super) fn fob_connected_next(&mut self) {
        let Some(granted) = &self.wallet.granted else {
            return;
        };
        if granted.node.is_none() {
            self.enter_this_till();
        } else {
            self.finish_wallet();
        }
    }

    /// D4's Back: to wherever the wallet came from.
    pub(super) fn this_till_back(&mut self) {
        let to = if self.wallet.granted.is_some() {
            Screen::FobConnected
        } else {
            self.wallet.entered_on.clone()
        };
        self.go(to);
    }

    /// The wallet made from what setup gathered, and the till linked to it.
    pub(super) fn finish_wallet(&mut self) {
        let Some(p) = &self.parts else { return };
        let form = &self.wallet;
        let (keys, wallet_setup) = match &form.granted {
            Some(granted) => {
                let (provider, currency) = granted.prices.clone().unzip();
                let setup = WalletSetup {
                    node: granted
                        .node
                        .as_ref()
                        .map(|n| n.url.clone())
                        .or(form.node.clone()),
                    settled_confirmations: form.confirmations,
                    expiry_minutes: form.expiry_minutes,
                    price_provider: provider,
                    price_currency: currency,
                    fob: Some(FobWallet {
                        wallet_id: granted.wallet_id.clone(),
                        wallet_name: granted.wallet_name.clone(),
                        label: granted.label.clone(),
                        node_from_fob: granted.node.is_some(),
                        prices_from_fob: granted.prices.is_some(),
                    }),
                    restore_height: Some(granted.restore_height),
                    ..WalletSetup::new(granted.account)
                };
                (granted.keys.clone(), setup)
            }
            None => {
                let Some(keys) = form.keys.clone() else {
                    return;
                };
                let setup = WalletSetup {
                    node: form.node.clone(),
                    settled_confirmations: form.confirmations,
                    expiry_minutes: form.expiry_minutes,
                    ..WalletSetup::new(form.till_number)
                };
                (keys, setup)
            }
        };
        let data_dir = PathBuf::from(&self.config.data_dir);
        let wake_ups = p.wake_ups.clone();
        let clock = self.clock.clone();
        self.work(async move {
            match till_wallet::make_wallet(&data_dir, &keys, &wallet_setup).await {
                Ok(made) => {
                    drop(keys);
                    // A Bell registration left from a Monokulo store goes:
                    // a till on its own wallet registers none.
                    wake_ups.unregister(true).await;
                    let backend = WalletBackend::of_made(made, clock);
                    apply(move |s| s.wallet_linked(backend))
                }
                Err(e) => {
                    apply(move |s| s.problem = Some(format!("Couldn't set up the wallet: {e}")))
                }
            }
        });
    }

    /// The till linked to its wallet: everything about an earlier store
    /// forgotten, the link kept, and on to the point of sale.
    fn wallet_linked(&mut self, backend: WalletBackend) {
        let Some(p) = &self.parts else { return };
        let link = backend.link().clone();
        let kept = p.side.kept.clone();
        let kept_it = kept.forget_all().and_then(|()| {
            kept.update(|m| {
                m.connection = None;
                m.wallet = Some(link.clone());
            })
        });
        if let Err(e) = kept_it {
            self.problem = Some(format!("Couldn't keep the wallet: {e}"));
            return;
        }
        p.side.adopt_wallet(backend);
        p.live.forget();
        p.live.read(p.side.backend().as_ref());
        self.generation += 1;
        self.tender = None;
        self.pos_confirmed = false;
        self.finding = None;
        self.counter = None;
        self.left.clear();
        self.wallet = WalletForm::new(self.config.debug);
        self.new_runner();
        self.go(Screen::PointOfSale);
    }

    /// The step a wallet screen shows, while one is up.
    pub(super) fn wallet_step(&self) -> Option<Step> {
        let form = &self.wallet;
        let step = match &self.screen {
            Screen::PaymentsMode => WalletStep::Mode { chosen: form.mode },
            Screen::WalletEntry => WalletStep::Entry,
            Screen::FobRequest => {
                let request = form.request.as_ref()?;
                let pattern = request.pattern();
                WalletStep::FobRequest {
                    qr: request.uri(),
                    cells: pattern.cells.iter().flatten().copied().collect(),
                    ink: pattern.ink,
                    paper: pattern.paper,
                    problem: form.problem.clone(),
                    busy: self.busy,
                }
            }
            Screen::FobConnected => {
                let granted = form.granted.as_ref()?;
                WalletStep::FobConnected(FobConnectedSetup {
                    wallet_name: granted.wallet_name.clone(),
                    label: granted.label.clone(),
                    account: granted.account,
                    node: granted.node.as_ref().map(|n| n.url.clone()),
                    prices: granted.prices.clone(),
                    confirmations: form.confirmations,
                    expiry_minutes: form.expiry_minutes,
                    busy: self.busy,
                    problem: self.problem.clone(),
                })
            }
            Screen::WalletCode => WalletStep::Code {
                problem: form.problem.clone(),
            },
            Screen::WalletKeys => {
                let check = keys::check_typed_keys(form.network(), &form.address, &form.view_key);
                WalletStep::Keys(KeysSetup {
                    network: form.network,
                    address: form.address.clone(),
                    view_key_typed: !form.view_key.trim().is_empty(),
                    address_issue: match check.address {
                        Ok(()) | Err(AddressProblem::Empty) => None,
                        Err(AddressProblem::NotAnAddress) => Some(AddressIssue::NotAnAddress),
                        Err(AddressProblem::OtherNetwork(n)) => {
                            Some(AddressIssue::OtherNetwork(wallet_network(n)))
                        }
                        Err(AddressProblem::NotPrimary) => Some(AddressIssue::NotPrimary),
                    },
                    view_key_issue: match check.view_key {
                        Ok(()) | Err(ViewKeyProblem::Empty) => None,
                        Err(ViewKeyProblem::NotAKey) => Some(ViewKeyIssue::NotAKey),
                        Err(ViewKeyProblem::DoesntMatch) => Some(ViewKeyIssue::DoesntMatch),
                    },
                    ok: check.ok(),
                    busy: self.busy,
                })
            }
            Screen::WalletSeed => WalletStep::Seed {
                words: form.seed_words,
                problem: form.problem.clone(),
            },
            Screen::ThisTill => WalletStep::ThisTill(ThisTillSetup {
                number: form.granted.is_none().then_some(form.till_number),
                node: form
                    .granted
                    .as_ref()
                    .and_then(|g| g.node.as_ref())
                    .is_none()
                    .then(|| form.node.clone())
                    .flatten(),
                checks: form.checks.clone(),
                node_choices: setup::default_nodes(form.network()),
                confirmations: form.confirmations,
                expiry_minutes: form.expiry_minutes,
                busy: self.busy,
                problem: self.problem.clone(),
            }),
            _ => return None,
        };
        Some(Step::Wallet(step))
    }
}
