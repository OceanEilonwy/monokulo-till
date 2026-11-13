//! Setting up a till that watches the shop's own wallet
//! (docs/view-key-mode.md §11.1, core.md decision 232): how Till sees
//! payments (V1), for a light wallet server its address (checked), then
//! the wallet, by Connect to Fob (C1, then B4 or C4) or entered by hand
//! (D1, D2, D3), then This till (D4), after which the wallet file is made
//! and the till goes on to the point of sale as a Monokulo till does.
//!
//! What's typed is held here only until the wallet is made: the view key
//! in a buffer wiped when it goes, seed words never at all (they're turned
//! into the view key the moment they arrive, and wiped).

use std::path::PathBuf;
use std::sync::Arc;

use till_payments::{FobWallet, WalletLink};
use till_screens::onboarding::Step;
use till_screens::prices::{PricesInput, ProviderCheck};
use till_screens::wallet_settings::WalletSettingsInput;
use till_screens::wallet_setup::{
    AddressIssue, CONFIRMATION_STEPS, CheckMark, EXPIRY_STEPS, FobConnectedSetup, KeysSetup,
    LightServerSetup, PaymentsMode, ThisTillSetup, ViewKeyIssue, WalletNetwork, WalletStep,
    WalletWay, step,
};
use till_wallet::keys::{AddressProblem, CodeProblem, SeedProblem, ViewKeyProblem};
use till_wallet::setup::{self, CheckState};
use till_wallet::{
    AnswerProblem, FobRequest, Granted, Network, SharedNode, ViewKeys, WalletBackend, WalletSetup,
};
use zeroize::Zeroizing;

use till_sales::RunnerSide;

use super::{State, apply};
use crate::store::{Model, keys};
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
    /// The price providers' answers on Prices and Settings (fob-price's
    /// check); `None` while they're asked.
    price_checks: Option<Vec<ProviderCheck>>,
    /// Bumped with each check of the providers.
    price_checking: u64,
    /// The light wallet server as typed, and whether its own certificate
    /// is accepted.
    light_url: String,
    light_self_signed: bool,
    light_checks: Vec<(String, CheckMark)>,
    /// The address and setting the checks passed for: Continue goes on
    /// without checking again while they're what's typed.
    light_passed: Option<(String, bool)>,
    /// Bumped with each check of the server.
    light_checking: u64,
    light_busy: bool,
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
            price_checks: None,
            price_checking: 0,
            light_url: String::new(),
            light_self_signed: false,
            light_checks: Vec::new(),
            light_passed: None,
            light_checking: 0,
            light_busy: false,
        }
    }

    /// Set up through a light wallet server.
    fn light(&self) -> bool {
        self.mode == Some(PaymentsMode::LightServer)
    }

    fn network(&self) -> Network {
        network_of(self.network)
    }

    /// `setup` through the light wallet server typed, when that's the way
    /// chosen. The server typed is used even when Fob shared its own: it
    /// was chosen, and checked, first.
    fn with_light_server(&self, setup: WalletSetup) -> WalletSetup {
        if !self.light() {
            return setup;
        }
        WalletSetup {
            light_server: Some(self.light_url.trim().to_owned()),
            light_self_signed: self.light_self_signed,
            ..setup
        }
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

fn check_mark(state: CheckState) -> CheckMark {
    match state {
        CheckState::Checking => CheckMark::Checking,
        CheckState::Passed => CheckMark::Passed,
        CheckState::Failed => CheckMark::Failed,
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

    /// V1's choice.
    pub(super) fn choose_payments_mode(&mut self, mode: PaymentsMode) {
        self.form().mode = Some(mode);
    }

    /// V1's Continue.
    pub(super) fn payments_mode_next(&mut self) {
        match self.wallet.mode {
            Some(PaymentsMode::Monokulo) => self.go(Screen::Scan),
            Some(PaymentsMode::Wallet) => self.go(Screen::WalletEntry),
            Some(PaymentsMode::LightServer) => self.go(Screen::LightServer),
            None => {}
        }
    }

    /// V2's Back: to the light wallet server's step when there is one.
    pub(super) fn wallet_entry_back(&mut self) {
        let to = if self.wallet.light() {
            Screen::LightServer
        } else {
            Screen::PaymentsMode
        };
        self.go(to);
    }

    /// The light wallet server's address and certificate setting as typed.
    pub(super) fn edit_light_server(&mut self, url: String, self_signed: bool) {
        let form = self.form();
        if form.light_url == url && form.light_self_signed == self_signed {
            return;
        }
        form.light_url = url;
        form.light_self_signed = self_signed;
        form.problem = None;
        // What was checked isn't what's typed now.
        form.light_checks.clear();
        form.light_passed = None;
        form.light_checking += 1;
        form.light_busy = false;
    }

    /// The light wallet server's Continue: the server checked (once for
    /// what's typed), then on to the wallet when it passes. The wallet's
    /// network becomes the server's (core.md decision 245).
    pub(super) fn light_server_next(&mut self) {
        let form = &self.wallet;
        let typed = (form.light_url.trim().to_owned(), form.light_self_signed);
        if typed.0.is_empty() || form.light_busy {
            return;
        }
        if form.light_passed.as_ref() == Some(&typed) {
            self.go(Screen::WalletEntry);
            return;
        }
        self.run_light_check(typed.0, typed.1, true);
    }

    /// The light wallet server at `url` checked as Fob's Test checks one.
    /// In setup the lines go under its field, and with `then` setup goes on
    /// when it passes; in Settings they go under its row.
    fn run_light_check(&mut self, url: String, self_signed: bool, then: bool) {
        let network = self.wallet.network();
        let settings = self.in_wallet_settings();
        let form = self.form();
        let checking = vec![("Checking the server…".to_owned(), CheckMark::Checking)];
        let token = if settings {
            form.checks = checking;
            form.checking += 1;
            form.checking
        } else {
            form.light_checks = checking;
            form.light_busy = true;
            form.light_checking += 1;
            form.light_checking
        };
        self.task(async move {
            let found = setup::check_light_server(network, &url, self_signed).await;
            apply(move |s| {
                let lines: Vec<(String, CheckMark)> = found
                    .checks
                    .iter()
                    .map(|c| (c.text.clone(), check_mark(c.state)))
                    .collect();
                if settings {
                    if s.wallet.checking == token {
                        s.wallet.checks = lines;
                    }
                    return;
                }
                if s.wallet.light_checking != token {
                    return;
                }
                let form = &mut s.wallet;
                form.light_busy = false;
                form.light_checks = lines;
                if let Some(network) = found.network {
                    form.network = wallet_network(network);
                }
                if found.passed() {
                    form.light_passed = Some((url, self_signed));
                    if then && s.screen == Screen::LightServer {
                        s.go(Screen::WalletEntry);
                    }
                }
            })
        });
    }

    /// V2's "Connect to Fob": a new request, shown on C1.
    pub(super) fn connect_fob(&mut self) {
        let network = self.wallet.network();
        match FobRequest::new("", &self.config.device_name, network, None) {
            Ok(mut request) => {
                // Kept with the request, so a restart goes on through it.
                request.light_server = self.wallet.light().then(|| SharedNode {
                    url: self.wallet.light_url.trim().to_owned(),
                    self_signed: self.wallet.light_self_signed,
                });
                self.keep_fob_request(Some(&request));
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
        if self.wallet.request.is_none() {
            self.restore_fob_request();
        }
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
                self.keep_fob_request(None);
                self.go(Screen::FobConnected);
            }
            Err(problem) => self.form().problem = Some(answer_problem(problem).into()),
        }
    }

    /// C4's "Use another wallet".
    pub(super) fn use_another_wallet(&mut self) {
        self.keep_fob_request(None);
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
        match till_wallet::keys::keys_from_code(text, self.wallet.network()) {
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
        if let Some(keys) =
            till_wallet::keys::keys_from_typed(form.network(), &form.address, &form.view_key)
        {
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
        match till_wallet::keys::keys_from_seed(self.wallet.network(), words) {
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
        if self.in_wallet_settings() {
            return self.change_wallet(|link| {
                link.till_number = if up {
                    link.till_number.saturating_add(1)
                } else {
                    link.till_number.saturating_sub(1).max(1)
                };
            });
        }
        let form = self.form();
        form.till_number = if up {
            form.till_number.saturating_add(1)
        } else {
            form.till_number.saturating_sub(1).max(1)
        };
    }

    pub(super) fn step_paid_after(&mut self, up: bool) {
        if self.in_wallet_settings() {
            return self.change_wallet(|link| {
                link.settled_confirmations =
                    step(&CONFIRMATION_STEPS, link.settled_confirmations, up);
            });
        }
        let form = self.form();
        form.confirmations = step(&CONFIRMATION_STEPS, form.confirmations, up);
    }

    pub(super) fn step_code_expiry(&mut self, up: bool) {
        if self.in_wallet_settings() {
            return self.change_wallet(|link| {
                link.expiry_minutes = step(&EXPIRY_STEPS, link.expiry_minutes, up);
            });
        }
        let form = self.form();
        form.expiry_minutes = step(&EXPIRY_STEPS, form.expiry_minutes, up);
    }

    /// D4's node, checked now as Fob checks one.
    pub(super) fn set_node(&mut self, url: String) {
        let url = url.trim().to_owned();
        if url.is_empty() {
            return;
        }
        if self.in_wallet_settings() {
            // Through a light wallet server, the row is the server's: a new
            // one is checked and taken (Q3: never a switch to a node).
            if let Some(self_signed) = self.light_self_signed() {
                return self.set_light_server(url, self_signed);
            }
            let node = url.clone();
            self.change_wallet(move |link| {
                link.node = Some(node);
                if let Some(fob) = &mut link.fob {
                    fob.node_from_fob = false;
                }
            });
        }
        if !self.in_wallet_settings() {
            self.form().node = Some(url.clone());
        }
        self.run_node_check(url);
    }

    /// Whether the light wallet server's own certificate is accepted, on a
    /// till through one.
    fn light_self_signed(&self) -> Option<bool> {
        self.parts
            .as_ref()
            .and_then(|p| p.side.model().wallet)
            .and_then(|l| l.light_server.is_some().then_some(l.light_self_signed))
    }

    /// Settings' light wallet server: its address and whether its own
    /// certificate is accepted (core.md decision 253). The wallet checks
    /// the server first, and the link keeps it only if that passes
    /// (`change_wallet`); the lines under the row are checked again too.
    /// A new address or setting gets a new trusted key: fob-wallet's
    /// `set_light_server` pins the one its check saw, or none without the
    /// switch.
    pub(super) fn set_light_server(&mut self, url: String, self_signed: bool) {
        let url = url.trim().to_owned();
        if url.is_empty() || !self.in_wallet_settings() || self.light_self_signed().is_none() {
            return;
        }
        let server = url.clone();
        self.change_wallet(move |link| {
            link.light_server = Some(server);
            link.light_self_signed = self_signed;
        });
        self.run_light_check(url, self_signed, false);
    }

    /// The node at `url` checked as Fob checks one, its lines shown as
    /// they come.
    fn run_node_check(&mut self, url: String) {
        let network = self.wallet.network();
        let form = self.form();
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
                    .map(|c| (c.text, check_mark(c.state)))
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
                let setup = form.with_light_server(setup);
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
                (keys, form.with_light_server(setup))
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
        let link = backend.link();
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
        self.keep_fob_request(None);
        self.new_runner();
        self.go(Screen::PointOfSale);
    }

    /// Whether setup asks where prices come from: on the shop's own
    /// wallet, with a fiat currency some provider prices (§8, §11.1). Fob's
    /// choice, when it shared one, is picked, but the step still shows.
    pub(super) fn prices_due(&self) -> bool {
        let Some(p) = &self.parts else { return false };
        let model = p.side.model();
        model.wallet.is_some()
            && model.connection.is_none()
            && price_currency(&model)
                .is_some_and(|c| !till_wallet::prices::provider_names(Some(&c)).is_empty())
    }

    /// After the point of sale: Prices when due, else alerts.
    pub(super) fn after_pos(&mut self) {
        if self.prices_due() {
            self.go(Screen::Prices);
            self.check_prices();
        } else {
            self.go(self.alerts_step());
        }
    }

    /// Setup's alerts step, or on a keypad till, which shows no
    /// notifications, the end of setup (core.md decision 260).
    pub(super) fn alerts_step(&self) -> Screen {
        if self.on_keypad() {
            Screen::Ready
        } else {
            Screen::Alerts
        }
    }

    /// Prices' Continue: the currency and provider kept in the wallet's
    /// link, so the wallet keeps that price fresh, then alerts.
    pub(super) fn prices_next(&mut self) {
        let Some(model) = self.model() else { return };
        let Some(currency) = price_currency(&model) else {
            return self.go(self.alerts_step());
        };
        let provider = model.wallet.as_ref().map(|link| {
            till_wallet::prices::provider_name(link.price_provider.as_deref(), Some(&currency))
        });
        self.change_wallet_then(
            move |link| {
                link.price_currency = Some(currency);
                link.price_provider = provider;
            },
            |s| s.go(s.alerts_step()),
        );
    }

    /// Each provider of the currency asked for its price now, as Fob's
    /// Price from does as it opens: on Prices, and as Settings opens.
    pub(super) fn check_prices(&mut self) {
        let Some(model) = self.model() else { return };
        let Some(link) = model.wallet.as_ref() else {
            return;
        };
        let Some(currency) = price_currency(&model) else {
            return;
        };
        let chosen =
            till_wallet::prices::provider_name(link.price_provider.as_deref(), Some(&currency));
        let form = self.form();
        form.price_checks = None;
        form.price_checking += 1;
        let token = form.price_checking;
        self.task(async move {
            let answers = till_wallet::prices::check_providers(&currency, &chosen).await;
            apply(move |s| {
                if s.wallet.price_checking == token {
                    s.wallet.price_checks = Some(
                        answers
                            .into_iter()
                            .map(|a| ProviderCheck {
                                name: a.name,
                                rate: a.rate,
                                failure: a.failure.map(str::to_owned),
                            })
                            .collect(),
                    );
                }
            })
        });
    }

    /// What Prices shows.
    pub(super) fn prices_step(&self, model: &Model) -> Option<Step> {
        let link = model.wallet.as_ref()?;
        let currency = price_currency(model)?;
        let plural = till_screens::prices::plural_or_code(
            &till_wallet::prices::currency_plural(&currency),
            &currency,
        );
        Some(Step::Prices(PricesInput {
            provider: till_wallet::prices::provider_name(
                link.price_provider.as_deref(),
                Some(&currency),
            ),
            providers: till_wallet::prices::provider_names(Some(&currency)),
            plural,
            currency,
            checks: self.wallet.price_checks.clone(),
            from_fob: link.fob.as_ref().is_some_and(|f| f.prices_from_fob),
            busy: self.busy,
            problem: self.problem.clone(),
        }))
    }

    /// Settings is up on a till linked to its own wallet: the steppers and
    /// the node change the wallet's link, not setup's form.
    fn in_wallet_settings(&self) -> bool {
        self.screen == Screen::Settings && self.parts.as_ref().is_some_and(|p| p.side.wallet_mode())
    }

    /// The price source, on Prices or in Settings.
    pub(super) fn choose_price_provider(&mut self, name: String) {
        if !self.in_wallet_settings() && self.screen != Screen::Prices {
            return;
        }
        let currency = self.model().as_ref().and_then(price_currency);
        if !till_wallet::prices::provider_names(currency.as_deref()).contains(&name) {
            return;
        }
        self.change_wallet(move |link| {
            link.price_provider = Some(name);
            if currency.is_some() {
                link.price_currency = currency;
            }
            if let Some(fob) = &mut link.fob {
                fob.prices_from_fob = false;
            }
        });
    }

    /// The keypad's currency chosen: the wallet prices it from now on.
    pub(super) fn change_wallet_currency(&mut self, code: String) {
        let Some(model) = self.model() else { return };
        if model.wallet.is_none() || model.connection.is_some() {
            return;
        }
        self.change_wallet(move |link| {
            if link.price_currency.as_deref() != Some(code.as_str()) {
                link.price_currency = Some(code);
                if let Some(fob) = &mut link.fob {
                    fob.prices_from_fob = false;
                }
            }
        });
    }

    /// Changes the wallet's link as `change` says: the wallet is set up
    /// that way first (a new node checked by it), then the link kept.
    /// Refused, nothing changes and Settings says why.
    fn change_wallet(&mut self, change: impl FnOnce(&mut WalletLink)) {
        self.change_wallet_then(change, |_| {});
    }

    /// [`State::change_wallet`], then `then` once the link is kept (or at
    /// once, when nothing changed).
    fn change_wallet_then(
        &mut self,
        change: impl FnOnce(&mut WalletLink),
        then: impl FnOnce(&mut State) + Send + 'static,
    ) {
        let Some(p) = &self.parts else { return };
        let Some(old) = p.side.model().wallet else {
            return;
        };
        let mut link = old.clone();
        change(&mut link);
        if link == old {
            return then(self);
        }
        let Some(backend) = p.side.wallet_backend(&old) else {
            self.problem = Some("Couldn't open the wallet to change it.".into());
            return;
        };
        let side = p.side.clone();
        self.work(async move {
            match backend.update(link.clone()).await {
                Ok(()) => apply(move |s| {
                    side.rekey_wallet(&link);
                    if s.keep(|m| m.wallet = Some(link)) {
                        then(s);
                    }
                    if let Some(p) = &s.parts {
                        p.live.read(p.side.backend().as_ref());
                    }
                }),
                Err(e) => {
                    let said = e.say();
                    apply(move |s| s.problem = Some(format!("Couldn't change that: {said}")))
                }
            }
        });
    }

    /// Settings' node checked, and the price providers asked, as Settings
    /// opens.
    pub(super) fn check_wallet_node(&mut self) {
        if !self.in_wallet_settings() {
            return;
        }
        self.check_prices();
        let Some(link) = self.parts.as_ref().and_then(|p| p.side.model().wallet) else {
            return;
        };
        let network = till_wallet::backend::network_of(&link);
        self.wallet.network = wallet_network(network);
        if let Some(server) = link.light_server.clone() {
            self.run_light_check(server, link.light_self_signed, false);
            return;
        }
        let Some(node) = link.node.clone() else {
            return;
        };
        self.run_node_check(node);
    }

    /// The wallet section of Settings, for a till on its own wallet.
    pub(super) fn wallet_settings(
        &self,
        p: &super::Parts,
        model: &crate::store::Model,
    ) -> Option<WalletSettingsInput> {
        let link = model
            .wallet
            .as_ref()
            .filter(|_| model.connection.is_none())?;
        let now = p.live.now();
        let address = now
            .backend
            .as_ref()
            .map(|b| b.address.clone())
            .filter(|a| !a.is_empty())
            .or_else(|| p.side.wallet_backend(link).map(|b| b.address()))
            .unwrap_or_default();
        let currency = price_currency(model);
        let provider =
            till_wallet::prices::provider_name(link.price_provider.as_deref(), currency.as_deref());
        let network = till_wallet::backend::network_of(link);
        Some(WalletSettingsInput {
            address,
            network: wallet_network(network),
            // Through a light server, the node its proofs are checked with:
            // the link's, else Fob's default for the network.
            node: link.node.clone().or_else(|| {
                link.light_server
                    .is_some()
                    .then(|| setup::default_node(network))
            }),
            light_server: link.light_server.clone(),
            light_self_signed: link.light_self_signed,
            till_number: link.till_number,
            fob: link
                .fob
                .as_ref()
                .map(|f| (f.wallet_name.clone(), f.label.clone())),
            confirmations: link.settled_confirmations,
            expiry_minutes: link.expiry_minutes,
            checks: self.wallet.checks.clone(),
            node_choices: setup::default_nodes(network),
            providers: till_wallet::prices::provider_names(currency.as_deref()),
            price_checks: self.wallet.price_checks.clone(),
            xmr_only: model.point_of_sale == Some(monokulo_client::PointOfSale::Till)
                && model.keypad_currency.as_deref() == Some("XMR"),
            provider,
            currency,
            warnings: now.backend.map(|b| b.warnings).unwrap_or_default(),
            busy: self.busy,
            problem: self.problem.clone(),
        })
    }

    /// Keeps C1's request (with its one-time key) as a secret, or forgets
    /// it, so Fob's answer still opens after a restart.
    fn keep_fob_request(&self, request: Option<&FobRequest>) {
        let Some(p) = &self.parts else { return };
        let kept = request.map(FobRequest::keep);
        if let Err(e) = p
            .side
            .store()
            .put(keys::FOB_REQUEST, kept.as_deref().map(|k| k.as_str()))
        {
            self.log(format!("till-core: couldn't keep the request for Fob: {e}"));
        }
    }

    /// C1's request as kept before a restart, back in the form; whether
    /// there was one.
    pub(super) fn restore_fob_request(&mut self) -> bool {
        let kept = self
            .parts
            .as_ref()
            .and_then(|p| p.side.store().get(keys::FOB_REQUEST).ok().flatten())
            .map(Zeroizing::new);
        let Some(request) = kept.as_deref().and_then(|k| FobRequest::restore(k)) else {
            return false;
        };
        let form = self.form();
        form.network = wallet_network(request.network());
        // Through the light wallet server typed (and checked) before the
        // request, as if there had been no restart (core.md decision 254).
        match &request.light_server {
            Some(server) => {
                form.mode = Some(PaymentsMode::LightServer);
                form.light_url = server.url.clone();
                form.light_self_signed = server.self_signed;
                form.light_passed = Some((server.url.clone(), server.self_signed));
            }
            None => form.mode = Some(PaymentsMode::Wallet),
        }
        form.request = Some(Arc::new(request));
        true
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
                    tablet: self.config.tablet(),
                })
            }
            Screen::WalletCode => WalletStep::Code {
                problem: form.problem.clone(),
            },
            Screen::WalletKeys => {
                let check = till_wallet::keys::check_typed_keys(
                    form.network(),
                    &form.address,
                    &form.view_key,
                );
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
                light: form.light(),
            }),
            Screen::LightServer => WalletStep::LightServer(LightServerSetup {
                url: form.light_url.clone(),
                self_signed: form.light_self_signed,
                checks: form.light_checks.clone(),
                busy: form.light_busy,
                problem: form.problem.clone(),
            }),
            _ => return None,
        };
        Some(Step::Wallet(step))
    }
}

/// The currency a till on its own wallet prices sales in: its point of
/// sale's (the Square location's, the Clover merchant's), else the one Fob
/// shared.
pub(super) fn price_currency(model: &Model) -> Option<String> {
    use monokulo_client::PointOfSale;
    let pos = match model.point_of_sale {
        Some(PointOfSale::Square) => model.location.as_ref().map(|l| l.currency.clone()),
        Some(PointOfSale::Clover) => model
            .clover_merchant
            .as_ref()
            .map(|m| m.currency.to_string()),
        // XMR only: no price is ever asked for.
        Some(PointOfSale::Till) => return model.keypad_currency.clone().filter(|c| c != "XMR"),
        None => None,
    };
    pos.or_else(|| model.wallet.as_ref()?.price_currency.clone())
}
