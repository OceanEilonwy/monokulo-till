//! The engine's task and the state it owns (Android: `AppViewModel`, the
//! `Graph` and `TillService`'s notifications), on Tern's model: actions
//! and the results of background work arrive as messages, work started
//! for one store is dropped once the till has connected afresh or been
//! forgotten (the generation), and after each batch of messages every
//! [`Update`] slice is published if it changed.

use monokulo_client::OrderId;
use std::collections::BTreeSet;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use monokulo_client::connect_code;
use monokulo_client::{
    Clock, ConnectCode, Connection, MonokuloApi, PointOfSale, SystemClock, problems, tor,
};
use till_bell::{
    BellAddress, BellApi, BellWakeUps, SubscriptionKeys, WakeUpChoice, WakeUpStatus, is_ding,
};
use till_ledger::{AlertKind, Claim, ClaimPolicy, Fact, Process, Scope, ShiftMark, UnixMillis};
use till_pos::clover::TenderState;
use till_pos::square::SquareLocation;
use till_pos::{OAuthRedirect, Pkce, PosError};
use till_sales::{Books, LiveStore, PosSwitch, Standing, TillRunner, TillSale};
use till_screens::bubble::PaidMoment;
use till_screens::close::{self, CloseThen};
use till_screens::onboarding::{
    CloverTender, CounterSetup, OnboardingInput, PosSetup, ScanProblem, Step,
};
use till_screens::settings::{PosState, SaleChoices, SettingsInput, StoreNow};
use till_screens::shift::ShiftFilter;
use till_screens::{
    Device, DeviceClock, OnboardingView, SettingsView, WakeUpsInput, WakeUpsView, onboarding,
    settings, wake_ups,
};
use till_screens::{notification, pos_text};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::watch;

mod till;

use crate::phases::Money;
use crate::push::PlatformPush;
use crate::relay::{DeviceToken, RelayPush};
use crate::side::Side;
use crate::store::{Kept, Log, PendingSignIn, Store, WakeUpsKept};
use crate::{
    Action, CORE_VERSION, CloseChoice, CopyKind, CounterChoice, Effect, EngineConfig,
    EngineListener, Haptic, Platform, PushPlatform, Screen, Update, WakeUps,
};

pub(crate) enum Msg {
    Action(Action),
    /// What work started under a generation does once it's done.
    Done(u64, Apply),
    /// Something the engine watches changed: publish.
    Changed,
    /// The store answered 401: it disconnected this device.
    Disconnected,
    /// The engine has been dropped.
    Stop,
    /// Tests look at the state.
    #[cfg(test)]
    Peek(Box<dyn FnOnce(&State) + Send>),
    /// Tests change the state, as a timer's end would.
    #[cfg(test)]
    Poke(Apply),
    /// Alerts this process claimed in the ledger, to deliver.
    Claims(Vec<Claim>),
    /// Another process wrote the ledger, or the app is in front again:
    /// read it again.
    LedgerChanged,
    /// The platform closed the ledger: the app may be suspended.
    Suspended,
}

/// The engine's books, shared with [`crate::Engine`] so the platform's
/// `suspend` can close the ledger from its own thread.
pub(crate) type BooksSlot = Arc<Mutex<Option<Arc<Books>>>>;

/// How long a `Foreground` heartbeat says the app is in front for, and how
/// often it's written while it is (docs/ledger.md §6).
const FOREGROUND_FOR: Duration = Duration::from_secs(10);
const HEARTBEAT: Duration = Duration::from_secs(4);

pub(crate) type Apply = Box<dyn FnOnce(&mut State) + Send>;

fn apply(f: impl FnOnce(&mut State) + Send + 'static) -> Apply {
    Box::new(f)
}

/// What tests change.
#[derive(Default)]
pub(crate) struct Options {
    pub clock: Option<Arc<dyn Clock>>,
    pub http: Option<reqwest::Client>,
}

/// Starts the engine's task on `runtime`.
pub(crate) fn spawn(
    runtime: &Handle,
    config: EngineConfig,
    platform: Arc<dyn Platform>,
    push: Arc<dyn PushPlatform>,
    listener: Arc<dyn EngineListener>,
    options: Options,
    books: BooksSlot,
) -> UnboundedSender<Msg> {
    let (tx, mut rx) = unbounded_channel();
    let mut state = State::new(
        tx.clone(),
        runtime.clone(),
        config,
        platform,
        push,
        listener,
        options,
    );
    state.books_slot = books;
    runtime.spawn(async move {
        state.step(|s| s.start());
        state.after();
        while let Some(msg) = rx.recv().await {
            if matches!(msg, Msg::Stop) {
                return;
            }
            state.message(msg);
            // Everything already here at once is published as one change.
            while let Ok(msg) = rx.try_recv() {
                if matches!(msg, Msg::Stop) {
                    return;
                }
                state.message(msg);
            }
            state.after();
        }
    });
    tx
}

/// The refund sheet over a sale, at a point of sale the till can't refund
/// through (decision 130 of android.md).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RefundSheet {
    pub order_id: OrderId,
    pub pos: PointOfSale,
    /// The point of sale can be asked whether it's refunded yet.
    pub asks: bool,
    pub checking: bool,
    pub not_seen: bool,
}

/// The parts made from the persisted model.
struct Parts {
    side: Arc<Side>,
    monokulo: MonokuloApi,
    live: Arc<LiveStore>,
    wake_ups: Arc<BellWakeUps>,
    monokulo_bell: BellAddress,
    push: Arc<PlatformPush>,
    /// iOS: the push side over the relay, with a relay in the config.
    relay: Option<Arc<RelayPush>>,
    runner: TillRunner,
}

/// `Update::Nav`'s fields: by itself, the back target, Back enabled, back,
/// steps.
type NavSlice = (bool, Option<Screen>, bool, bool, bool);

/// `Update::Work`'s fields.
#[derive(Clone, PartialEq)]
struct WorkSlice {
    busy: bool,
    problem: Option<String>,
    shown_busy: bool,
    shown_problem: Option<String>,
}

/// What was last published, slice by slice.
#[derive(Default)]
struct Published {
    core: Option<String>,
    screen: Option<Screen>,
    work: Option<WorkSlice>,
    service: Option<bool>,
    onboarding: Option<OnboardingView>,
    settings: Option<SettingsView>,
    wake_ups: Option<WakeUpsView>,
    nav: Option<NavSlice>,
    shift: Option<till_screens::ShiftView>,
    sale: Option<Update>,
    close: Option<(Option<OrderId>, Option<till_screens::CloseView>)>,
    refund: Option<Option<crate::RefundSheetView>>,
    bubble: Option<crate::BubbleView>,
    notification: Option<till_screens::NotificationView>,
    live_activity: Option<Option<till_screens::LiveActivityState>>,
}

pub(crate) struct State {
    tx: UnboundedSender<Msg>,
    runtime: Handle,
    config: Arc<EngineConfig>,
    platform: Arc<dyn Platform>,
    push_platform: Arc<dyn PushPlatform>,
    /// iOS's APNs token as last heard, for each push side made.
    push_token: DeviceToken,
    listener: Arc<dyn EngineListener>,
    clock: Arc<dyn Clock>,
    http: Option<reqwest::Client>,
    /// Bumped when the till connects afresh or forgets its store: work
    /// started before then is dropped when it's done.
    generation: u64,
    parts: Option<Parts>,
    /// Why the kept state couldn't be read; tried again on `Opened`.
    load_problem: Option<String>,
    screen: Screen,
    confirm: Option<ConnectCode>,
    locations: Vec<SquareLocation>,
    scan_problem: Option<ScanProblem>,
    busy: bool,
    problem: Option<String>,
    before_settings: Option<Screen>,
    /// The sign-in under way went through a Bell's application.
    bell_sign_in: bool,
    /// Step 4 was opened from Settings.
    from_settings: bool,
    /// Clover's Monero payment type; `None` while not known.
    tender: Option<TenderState>,
    left: BTreeSet<OrderId>,
    refunding: BTreeSet<OrderId>,
    /// Offered cash sales whose order is being made ("Take it in Monero").
    taking: BTreeSet<OrderId>,
    /// Whose sales the shift's list shows, once the cashier chose
    /// (core.md decision 114).
    counter: Option<CounterChoice>,
    /// Step 4 is past choosing the point of sale (O1), on to signing in.
    pos_confirmed: bool,
    /// "Find this device again" waits for a sale that isn't one of these.
    finding: Option<BTreeSet<OrderId>>,
    /// The Clover devices whose names were asked for since the till
    /// started, named or not (core.md decision 142): each is asked once.
    devices_asked: BTreeSet<till_pos::PosDeviceId>,
    refund_sheet: Option<RefundSheet>,
    visible: bool,
    alerts_allowed: bool,
    bubble_allowed: bool,
    /// Orbot's proxy, as the platform's last probe found it.
    tor_running: Option<bool>,
    /// Wake-ups' "My own Bell" picked with no address saved, and its field
    /// as typed ([`Action::EditOwnBell`]).
    own_picked: bool,
    own_typed: Option<String>,
    /// The shift's chip, and whether its tray is folded.
    filter: ShiftFilter,
    folded: bool,
    /// The sale whose close sheet is up.
    close_sheet: Option<OrderId>,
    /// The sales that were live at the last look.
    live_before: BTreeSet<OrderId>,
    /// The screen the platform's stage rests on; `None` while it moves.
    resting_on: Option<Screen>,
    /// The screen up was put there by the sales, not the cashier.
    by_itself: bool,
    /// Bumped to call off a paid sale's step back.
    step_back: u64,
    /// The sale just paid, for [`till::PAID_DWELL`].
    just_paid: Option<OrderId>,
    just_paid_token: u64,
    /// The till's own sale whose code just ran out or was reversed, for
    /// [`till::ENDED_DWELL`]: the Live Activity shows it as expired.
    just_ended: Option<OrderId>,
    just_ended_token: u64,
    /// The sale that arrived while the app wasn't in front, alerted in the
    /// next notification update only.
    alert_for: Option<OrderId>,
    /// Where the bubble's paid moment is (decision 95).
    paid_moment: PaidMoment,
    /// The sale the bubble showed last.
    bubble_focus: Option<OrderId>,
    /// Android's bubble panel is up, as the platform last said.
    panel_up: bool,
    /// A redraw is due in a second.
    ticking: bool,
    utc_offset: i32,
    hours_24: bool,
    /// The sales listed at the last look: one gone from the list has its
    /// notification cleared.
    listed: Option<BTreeSet<OrderId>>,
    /// The ledger's scope as last seen: when the store or the point of
    /// sale's account changes, the old scope's rows go (docs/ledger.md §4).
    scope: Option<Scope>,
    /// Bumped to stop the `Foreground` heartbeat's loop.
    heartbeat: u64,
    books_slot: BooksSlot,
    published: Published,
}

impl State {
    fn new(
        tx: UnboundedSender<Msg>,
        runtime: Handle,
        config: EngineConfig,
        platform: Arc<dyn Platform>,
        push_platform: Arc<dyn PushPlatform>,
        listener: Arc<dyn EngineListener>,
        options: Options,
    ) -> State {
        State {
            tx,
            runtime,
            utc_offset: config.utc_offset,
            hours_24: config.hours_24,
            config: Arc::new(config),
            platform,
            push_platform,
            push_token: DeviceToken::NotYet,
            listener,
            clock: options.clock.unwrap_or_else(|| Arc::new(SystemClock)),
            http: options.http,
            generation: 0,
            parts: None,
            load_problem: None,
            screen: Screen::Welcome,
            confirm: None,
            locations: Vec::new(),
            scan_problem: None,
            busy: false,
            problem: None,
            before_settings: None,
            bell_sign_in: false,
            from_settings: false,
            tender: None,
            left: BTreeSet::new(),
            refunding: BTreeSet::new(),
            taking: BTreeSet::new(),
            counter: None,
            pos_confirmed: false,
            finding: None,
            devices_asked: BTreeSet::new(),
            refund_sheet: None,
            visible: true,
            alerts_allowed: false,
            bubble_allowed: false,
            tor_running: None,
            own_picked: false,
            own_typed: None,
            filter: ShiftFilter::All,
            folded: true,
            close_sheet: None,
            live_before: BTreeSet::new(),
            resting_on: None,
            by_itself: false,
            step_back: 0,
            just_paid: None,
            just_paid_token: 0,
            just_ended: None,
            just_ended_token: 0,
            alert_for: None,
            paid_moment: PaidMoment::None,
            bubble_focus: None,
            panel_up: false,
            ticking: false,
            listed: None,
            scope: None,
            heartbeat: 0,
            books_slot: BooksSlot::default(),
            published: Published::default(),
        }
    }

    fn log(&self, line: impl Into<String>) {
        self.listener.on_log(line.into());
    }

    fn effect(&self, effect: Effect) {
        self.listener.on_effect(effect);
    }

    /// Runs `f`, a panic logged rather than ending the task.
    fn step(&mut self, f: impl FnOnce(&mut State)) {
        if catch_unwind(AssertUnwindSafe(|| f(self))).is_err() {
            self.log("till-core: panicked");
        }
    }

    fn message(&mut self, msg: Msg) {
        match msg {
            Msg::Action(action) => {
                let label = action.name();
                if catch_unwind(AssertUnwindSafe(|| self.handle(action))).is_err() {
                    self.log(format!("till-core: panicked handling {label}"));
                }
            }
            Msg::Done(generation, done) => {
                if generation == self.generation {
                    self.step(done);
                }
            }
            Msg::Changed => {}
            #[cfg(test)]
            Msg::Peek(look) => look(self),
            #[cfg(test)]
            Msg::Poke(change) => self.step(change),
            Msg::Disconnected => {
                if let Some(p) = &self.parts {
                    p.runner.disconnected();
                }
            }
            Msg::Claims(claims) => self.step(|s| s.deliver(claims)),
            Msg::LedgerChanged => self.step(|s| s.ledger_changed()),
            Msg::Suspended => {}
            Msg::Stop => {}
        }
    }

    /// After each batch: what changed about the sales, then every slice.
    fn after(&mut self) {
        self.step(|s| {
            s.rescope();
            s.learn_found_device();
            s.name_clover_devices();
            s.follow();
            s.follow_sales();
            s.publish();
        });
    }

    /// Runs `work` off this task; what it returns is applied here, unless
    /// the generation has moved on.
    fn task(&self, work: impl Future<Output = Apply> + Send + 'static) {
        let (tx, generation) = (self.tx.clone(), self.generation);
        self.runtime.spawn(async move {
            let done = work.await;
            let _ = tx.send(Msg::Done(generation, done));
        });
    }

    /// [`State::task`] for a network call the cashier waits on: buttons
    /// wait meanwhile, and one at a time.
    fn work(&mut self, work: impl Future<Output = Apply> + Send + 'static) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.problem = None;
        self.task(async move {
            let done = work.await;
            apply(move |s| {
                s.busy = false;
                done(s);
            })
        });
    }

    /// Forwards each change of `rx` to the task as [`Msg::Changed`].
    fn watch<T: Send + Sync + 'static>(&self, mut rx: watch::Receiver<T>) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            while rx.changed().await.is_ok() {
                if tx.send(Msg::Changed).is_err() {
                    return;
                }
            }
        });
    }

    fn start(&mut self) {
        self.build();
    }

    /// Makes the parts from the persisted model; a store that can't answer
    /// leaves the engine waiting for the next `Opened`.
    fn build(&mut self) {
        match self.make_parts() {
            Ok(parts) => {
                self.load_problem = None;
                self.parts = Some(parts);
                self.begin();
            }
            Err(problem) => {
                self.log(format!("till-core: {problem}"));
                self.load_problem = Some(problem);
            }
        }
    }

    fn make_parts(&mut self) -> Result<Parts, String> {
        let log: Log = {
            let listener = self.listener.clone();
            Arc::new(move |line| listener.on_log(line))
        };
        let kept = Arc::new(
            Kept::load(Store::new(self.platform.clone()), log)
                .map_err(|e| format!("Couldn't read what this till keeps: {e}"))?,
        );
        let http = match &self.http {
            Some(http) => http.clone(),
            None => tor::http_client(tor::SOCKS).map_err(|e| e.to_string())?,
        };
        let side = Arc::new(Side::new(
            self.config.clone(),
            kept.clone(),
            self.platform.clone(),
            Process::App,
            http.clone(),
            self.clock.clone(),
        ));
        {
            let tx = self.tx.clone();
            side.ledger().deliver_to(Arc::new(move |claims| {
                let _ = tx.send(Msg::Claims(claims));
            }));
        }
        if let Ok(mut slot) = self.books_slot.lock() {
            *slot = Some(side.books().clone());
        }
        let monokulo = MonokuloApi::new(http.clone(), side.identity(), self.clock.clone());
        let monokulo_bell = BellAddress::parse(&self.config.bell_url, self.config.debug)
            .ok_or_else(|| {
                format!(
                    "The build's Bell isn't a Bell address: {}",
                    self.config.bell_url
                )
            })?;
        let live = {
            let tx = self.tx.clone();
            let weak: Weak<Side> = Arc::downgrade(&side);
            Arc::new(LiveStore::new(
                monokulo.clone(),
                Arc::new(move || {
                    let _ = tx.send(Msg::Disconnected);
                }),
                Arc::new(move |onion: Option<&str>| {
                    if let Some(side) = weak.upgrade() {
                        keep_onion_api(&side, onion);
                    }
                }),
            ))
        };
        let relay = self.config.relay_url.as_deref().map(|url| {
            RelayPush::new(
                url,
                self.platform.clone(),
                http.clone(),
                self.tx.clone(),
                self.runtime.clone(),
                self.push_token.clone(),
            )
        });
        let push_platform: Arc<dyn PushPlatform> = match &relay {
            Some(relay) => relay.clone(),
            None => self.push_platform.clone(),
        };
        let push = Arc::new(PlatformPush::new(push_platform));
        let bells = Arc::new(BellApi::new().map_err(|e| e.to_string())?);
        let wake_ups = Arc::new(BellWakeUps::new(
            push.clone(),
            push.clone(),
            bells,
            Arc::new(WakeUpsKept(kept)),
            monokulo_bell.clone(),
            side.clone(),
            self.clock.clone(),
        ));
        side.attach(&live, &wake_ups);
        let runner = TillRunner::new(side.clone(), monokulo.clone(), self.clock.clone());
        self.watch(live.subscribe());
        self.watch(wake_ups.status());
        self.watch(wake_ups.choice());
        self.watch(runner.subscribe());
        Ok(Parts {
            side,
            monokulo,
            live,
            wake_ups,
            monokulo_bell,
            push,
            relay,
            runner,
        })
    }

    /// What the app does as it starts (`AppViewModel`'s `init`, and the
    /// service started while a shift is on): the store read live, the
    /// shift running or its saved sales on show, and Clover's payment type
    /// found again.
    fn begin(&mut self) {
        let Some(p) = &self.parts else { return };
        let model = p.side.model();
        p.live.read(model.connection.as_ref());
        self.scope = Some(crate::ledger::scope_of(&model));
        self.keep_settings();
        if let Some(p) = &self.parts {
            p.side.books().refresh();
            // What the ledger held as the app started (an older build's
            // shift, imported) is learnt; what the first look finds is new
            // (docs/decisions/core.md 155).
            if let Err(e) = p.side.books().with(|l| l.set_learning(false)) {
                self.log(format!("till-core: ending the ledger's learning: {e}"));
            }
            if let Err(e) = p.side.books().with(|l| l.maintain()) {
                self.log(format!("till-core: the ledger's upkeep: {e}"));
            }
        }
        let Some(p) = &self.parts else { return };
        if model.shift_on && model.connection.is_some() {
            p.runner.start();
        } else {
            p.runner.show_saved();
        }
        if model.point_of_sale == Some(PointOfSale::Clover)
            && let Some(merchant) = model.clover_merchant
        {
            let source = p.side.clover_source(merchant);
            self.task(async move {
                let tender = source.ensure_tender().await.ok();
                apply(move |s| s.tender = tender)
            });
        }
        self.screen = self.home();
        // The sales already live as the app starts were told of then: none
        // rises out of the shift on its own.
        self.live_before = till_screens::till_nav::live_ids(&self.sales_now());
        // The first screen is at rest before anything moves.
        self.resting_on = Some(self.screen.clone());
    }

    /// A new runner, for a till connected afresh or forgotten: the old
    /// one's shift goes with it.
    fn new_runner(&mut self) {
        let Some(p) = &mut self.parts else { return };
        p.runner = TillRunner::new(p.side.clone(), p.monokulo.clone(), self.clock.clone());
        let rx = p.runner.subscribe();
        self.watch(rx);
        self.listed = None;
    }

    fn model(&self) -> Option<crate::store::Model> {
        Some(self.parts.as_ref()?.side.model())
    }

    fn setup_done(&self) -> bool {
        self.model().is_some_and(|m| m.setup_done)
    }

    /// Changes the kept model; a refused write is shown.
    fn keep(&mut self, change: impl FnOnce(&mut crate::store::Model)) -> bool {
        let Some(p) = &self.parts else { return false };
        match p.side.kept.update(change) {
            Ok(()) => true,
            Err(e) => {
                self.problem = Some(format!("Couldn't keep that: {e}"));
                false
            }
        }
    }

    fn home(&self) -> Screen {
        let Some(model) = self.model() else {
            return Screen::Welcome;
        };
        if model.connection.is_none() {
            Screen::Welcome
        } else if model.setup_done {
            Screen::Till
        } else if model.place_name().is_none() && !model.without_pos {
            // Not yet at a Square location or a Clover merchant.
            Screen::PointOfSale
        } else {
            Screen::Alerts
        }
    }

    fn go(&mut self, screen: Screen) {
        self.problem = None;
        self.by_itself = false;
        self.unfold_for(&screen);
        // Going to the screen already up changes nothing the stage would land from.
        if screen != self.screen {
            self.resting_on = None;
        }
        self.screen = screen;
    }

    /// Back on the shift from a sale whose row the folded tray hides: the
    /// tray opens first, so the row is there for the sale to shrink into on
    /// the change's first frame (android.md 269, 289).
    fn unfold_for(&mut self, to: &Screen) {
        if *to != Screen::Till || !self.folded {
            return;
        }
        let Some(order_id) = till::order_id(&self.screen) else {
            return;
        };
        let Some(p) = &self.parts else { return };
        let sales = p.runner.state().till.sales;
        if till_screens::shift::folds(&sales, self.filter, order_id) {
            self.folded = false;
        }
    }

    fn go_home(&mut self) {
        let home = self.home();
        self.go(home);
    }

    /// Starts (or starts again) the till when a shift is on.
    fn restart(&mut self) {
        let Some(p) = &self.parts else { return };
        p.runner.stop();
        let model = p.side.model();
        if model.shift_on && model.connection.is_some() {
            p.runner.start();
        }
    }

    fn handle(&mut self, action: Action) {
        match action {
            Action::Opened => self.opened(),
            Action::Backgrounded => {
                self.visible = false;
                self.heartbeat += 1;
                // Behind now: a payment is a notice, not the paid moment.
                let now = self.now_ms();
                self.record(Fact::Foreground { until: now });
            }
            Action::ClockChanged {
                utc_offset,
                hours_24,
            } => {
                self.utc_offset = utc_offset;
                self.hours_24 = hours_24;
            }
            Action::Permissions { alerts, bubble } => {
                self.alerts_allowed = alerts;
                self.bubble_allowed = bubble;
            }
            Action::TorProbed { running } => self.tor_running = Some(running),
            Action::PanelShown { up } => self.panel_shown(up),
            action if self.parts.is_none() => {
                self.log(format!(
                    "till-core: nothing kept could be read, so {} waits",
                    action.name()
                ));
            }
            Action::Back => self.back(),
            Action::Next => self.next(),
            Action::Skip => self.skip(),
            Action::ConnectLink { url } => {
                self.connect_link(Some(&url), ScanProblem::NotALink, ScanProblem::NotALink)
            }
            Action::PasteConnectLink { text } => self.connect_link(
                text.as_deref(),
                ScanProblem::NothingToPaste,
                ScanProblem::NotALink,
            ),
            Action::SharedText { text } => {
                if let Some(link) = connect_code::shared_link(&text, self.config.debug) {
                    let link = link.to_owned();
                    self.connect_link(Some(&link), ScanProblem::NotALink, ScanProblem::NotALink);
                }
            }
            Action::ScanConnectCode { text } => {
                self.connect_link(Some(&text), ScanProblem::NotACode, ScanProblem::NotACode)
            }
            Action::Connect => self.connect(),
            Action::ChoosePointOfSale { pos } => self.choose_point_of_sale(pos.into()),
            Action::SignIn { application_id } => self.sign_in(application_id),
            Action::SignInReturned { url } => self.sign_in_returned(&url),
            Action::ChooseLocation { location_id } => {
                if let Some(location) = self
                    .locations
                    .iter()
                    .find(|l| l.id.as_str() == location_id)
                    .cloned()
                {
                    self.choose_location(location);
                }
            }
            Action::ChangeLocation => self.change_location(),
            Action::LookAgain => self.look_again(),
            Action::WithoutPos => self.without_pos(),
            Action::SignOutOfPos => self.sign_out_of_pos(),
            Action::StartShift => {
                if self.setup_done() {
                    self.set_shift(true);
                } else if self.keep(|m| m.setup_done = true) {
                    self.set_shift(true);
                    self.go(Screen::Till);
                }
            }
            Action::StopShift => self.set_shift(false),
            Action::ChooseFilter { filter } => self.filter = filter,
            Action::FoldTray { folded } => self.folded = folded,
            Action::OpenShift => {
                let sales = till_sales_ids(self);
                self.left.extend(sales);
                if self.setup_done() {
                    self.go(Screen::Till);
                }
            }
            Action::Landed { screen } => {
                if screen == self.screen {
                    self.resting_on = Some(screen);
                }
            }
            Action::AskClose { order_id } => {
                if till_screens::close::applies(self.sale(&order_id).as_ref()) {
                    self.close_sheet = Some(order_id);
                }
            }
            Action::DismissClose => self.close_sheet = None,
            Action::OpenSale { order_id } => self.open(order_id),
            Action::Enlarge { order_id } => self.go(Screen::Code { order_id }),
            Action::ShowOrder { order_id } => self.go(Screen::Order { order_id }),
            Action::Close { order_id, choice } => {
                self.close_sheet = None;
                self.close(order_id, choice);
            }
            Action::Refund { order_id } => self.refund(order_id, false),
            Action::ConfirmRefund { order_id } => self.confirm_refund(order_id),
            Action::DismissRefund => self.refund_sheet = None,
            Action::Reopen { order_id } => {
                if let Some(p) = &self.parts {
                    p.runner.reopen(&order_id);
                }
                self.open(order_id);
            }
            Action::CopyAddress { order_id } => {
                if let Some(sale) = self.sale(&order_id) {
                    self.effect(Effect::CopyText {
                        text: sale.address,
                        kind: CopyKind::Address,
                    });
                }
            }
            Action::CopyAmount { order_id } => {
                if let Some(sale) = self.sale(&order_id) {
                    self.effect(Effect::CopyText {
                        text: sale.xmr,
                        kind: CopyKind::Amount,
                    });
                }
            }
            Action::CopyWalletAddress => {
                let address = self.parts.as_ref().and_then(|p| {
                    p.live
                        .now()
                        .store
                        .and_then(|store| store.wallet)
                        .map(|wallet| wallet.address)
                });
                if let Some(text) = address {
                    self.effect(Effect::CopyText {
                        text,
                        kind: CopyKind::Address,
                    });
                }
            }
            Action::OpenSettings => self.open_settings(),
            Action::OpenPointOfSale => {
                self.from_settings = true;
                self.go(Screen::PointOfSale);
            }
            Action::ReadStore => {
                if let Some(p) = &self.parts {
                    p.live.read(p.side.model().connection.as_ref());
                }
            }
            Action::SetBaseUrl { pos, url } => {
                if self.config.debug {
                    let url = Some(url).filter(|u| !u.is_empty());
                    self.keep(|m| match pos {
                        crate::Pos::Square => m.square_base_url = url,
                        crate::Pos::Clover => m.clover_base_url = url,
                    });
                }
            }
            Action::SetBubble { on } => {
                self.keep(|m| m.bubble_on = on);
            }
            Action::MoveBubble { left, y } => {
                self.keep(|m| {
                    m.bubble_left = left;
                    m.bubble_y = Some(y);
                });
            }
            Action::SetTor { on } => self.set_tor(on),
            // The island is the iPhone's alone: elsewhere they stay off.
            Action::SetLiveActivity { on } if self.config.device == Device::Iphone => {
                self.keep(|m| m.live_activity = on);
            }
            Action::SetCodeInIsland { on } if self.config.device == Device::Iphone => {
                self.keep(|m| m.code_in_island = on);
            }
            Action::SetLiveActivity { .. } | Action::SetCodeInIsland { .. } => {}
            Action::Disconnect => self.disconnect(),
            Action::Forget => self.forget(),
            Action::ChooseWakeUps { choice } => {
                self.own_picked = false;
                self.own_typed = None;
                self.choose_wake_ups(choice);
            }
            Action::EditOwnBell { picked, typed } => {
                // The row tapped with an address saved: that Bell, at once.
                let saved = self.model().and_then(|m| m.own_bell);
                match saved {
                    Some(bell) if picked && typed.is_none() => {
                        self.own_picked = false;
                        self.own_typed = None;
                        if let Some(p) = &self.parts {
                            p.wake_ups.choose(WakeUpChoice::OwnBell { bell });
                        }
                    }
                    _ => {
                        self.own_picked = picked;
                        self.own_typed = typed;
                    }
                }
            }
            Action::ChooseDistributor { name } => {
                if let Some(p) = &self.parts {
                    p.wake_ups.choose_distributor(&name);
                }
            }
            Action::PushEndpoint { url, p256dh, auth } => {
                if let Some(p) = &self.parts {
                    p.push
                        .feed
                        .endpoint(&url, SubscriptionKeys::new(p256dh, auth));
                }
            }
            Action::PushFailed { failure } => {
                if let Some(p) = &self.parts {
                    p.push.failed(failure);
                }
            }
            Action::PushUnregistered => {
                if let Some(p) = &self.parts {
                    p.push.feed.unregistered();
                }
            }
            Action::PushReceived { content, decrypted } => self.push_received(&content, decrypted),
            Action::PushToken { token } => {
                self.push_token = match token {
                    Some(token) => DeviceToken::Have(token.into()),
                    None => DeviceToken::Failed,
                };
                if let Some(relay) = self.parts.as_ref().and_then(|p| p.relay.as_ref()) {
                    relay.token(self.push_token.clone());
                }
            }
            Action::DistributorsChanged => {
                if let Some(p) = &self.parts {
                    p.push.distributors_changed();
                }
            }
            Action::SetAlertFor { every_device } => {
                if self.keep(|m| m.alert_every = every_device) {
                    self.keep_settings();
                }
            }
            Action::FindDevice => self.find_device(),
            Action::SetShowOtherCounters { on } => {
                if self.keep(|m| m.show_others = on) {
                    // The list opens as the setting now says.
                    self.counter = None;
                    self.keep_settings();
                }
            }
            Action::ChooseCounter { choice } => self.counter = Some(choice),
            Action::SetCashAsMonero { on } => {
                if self.keep(|m| m.cash_as_monero = on) {
                    self.keep_settings();
                    self.restart();
                }
            }
            Action::ChooseShowSales { mode } => {
                if self.keep(|m| m.show_sales = mode) {
                    self.keep_settings();
                }
            }
            Action::TakeInMonero { order_id } => self.take_in_monero(order_id),
            Action::NewCode { order_id } => self.new_code(order_id),
            Action::ExtraRefunded { order_id } => {
                if let Some(p) = &self.parts {
                    p.runner.extra_refunded(&order_id);
                }
            }
            Action::KeepAsCash { order_id } => {
                if let Some(p) = &self.parts {
                    p.runner.keep_as_cash(&order_id);
                }
            }
            Action::PaidInCash { order_id } => {
                if let Some(p) = &self.parts {
                    p.runner.paid_in_cash(&order_id);
                }
                // Its code is down: the sale's page shows how it ended.
                if till::order_id(&self.screen) == Some(&order_id)
                    && self.screen
                        != (Screen::Sale {
                            order_id: order_id.clone(),
                        })
                {
                    self.go(Screen::Sale { order_id });
                }
            }
        }
    }

    /// How this till channels sales by device, as kept (core.md decision
    /// 113).
    pub(crate) fn counters(&self) -> till_screens::counters::Counters {
        self.model().map(|m| m.counters()).unwrap_or_default()
    }

    /// The sales that stay quiet here: another counter's, while alerts are
    /// for this device alone (core.md decision 113).
    pub(crate) fn quiet_ids(&self) -> BTreeSet<OrderId> {
        let counters = self.counters();
        self.parts
            .as_ref()
            .map(|p| p.runner.state().till.sales)
            .unwrap_or_default()
            .into_iter()
            .filter(|sale| counters.quiet(sale))
            .map(|sale| sale.order_id)
            .collect()
    }

    /// "Find this device again with a test sale", or tapped again while it
    /// waits, stop waiting.
    fn find_device(&mut self) {
        if self.finding.take().is_none() {
            self.finding = Some(till_sales_ids(self).into_iter().collect());
        }
    }

    /// The first sale found since "Find this device again" that names its
    /// device names this counter's.
    pub(crate) fn learn_found_device(&mut self) {
        let Some(before) = &self.finding else { return };
        let Some(p) = &self.parts else { return };
        let found = p
            .runner
            .state()
            .till
            .sales
            .iter()
            .filter(|sale| !before.contains(&sale.order_id))
            .find_map(|sale| {
                sale.pos_device()
                    .map(|(id, name)| (id.to_owned(), name.map(str::to_owned)))
            });
        if let Some((id, name)) = found {
            self.finding = None;
            self.learn_device(id, name);
        }
    }

    /// Reads Clover's devices for their names when a sale (or this
    /// counter) names a device Clover's list hasn't named yet, so the
    /// counters menu says "Front counter" rather than "Device 3F2A" (core.md
    /// decision 142). Each device is asked about once a run; a failed read
    /// leaves it unnamed.
    fn name_clover_devices(&mut self) {
        let Some(p) = &self.parts else { return };
        let model = p.side.model();
        if model.point_of_sale != Some(PointOfSale::Clover) {
            return;
        }
        let Some(merchant) = model.clover_merchant.clone() else {
            return;
        };
        let sales = p.runner.state().till.sales;
        let unnamed: BTreeSet<till_pos::PosDeviceId> = sales
            .iter()
            .filter_map(|sale| sale.pos_device().map(|(id, _)| id.to_owned()))
            .chain(model.counter_device.iter().map(|d| d.id.clone()))
            .filter(|id| !model.clover_devices.contains_key(id) && !self.devices_asked.contains(id))
            .collect();
        if unnamed.is_empty() || !p.side.signed_in(PointOfSale::Clover) {
            return;
        }
        self.devices_asked.extend(unnamed);
        let api = p.side.clover_api();
        self.task(async move {
            let devices = api.devices(&merchant.id).await;
            apply(move |s| match devices {
                Ok(devices) => {
                    let named: Vec<(till_pos::PosDeviceId, String)> = devices
                        .iter()
                        .filter_map(|d| d.label().map(|label| (d.id.as_str().into(), label)))
                        .collect();
                    let _ = s.keep(|m| m.clover_devices.extend(named));
                }
                Err(e) => s.log(format!("till-core: couldn't read Clover's devices: {e}")),
            })
        });
    }

    /// Keeps `id` as this counter's device, and starts the till again
    /// with it.
    fn learn_device(&mut self, id: till_pos::PosDeviceId, name: Option<String>) {
        let learned_at = self.clock.now();
        if self.keep(|m| {
            m.counter_device = Some(crate::store::CounterKept {
                id,
                name,
                learned_at,
            })
        }) {
            self.counter = None;
            self.keep_settings();
            self.restart();
        }
    }

    /// The test sale step 4 found, while setting up: the first sale this
    /// till found.
    fn test_sale(&self) -> Option<TillSale> {
        self.parts
            .as_ref()?
            .runner
            .state()
            .till
            .sales
            .into_iter()
            .rev()
            .find(|sale| sale.found.is_some())
    }

    /// "Take it in Monero": the offered cash sale's order is made, and the
    /// sale opens on its code.
    fn take_in_monero(&mut self, order_id: OrderId) {
        if self.taking.contains(&order_id) {
            return;
        }
        let Some(till) = self.parts.as_ref().and_then(|p| p.runner.till()) else {
            return;
        };
        self.taking.insert(order_id.clone());
        self.task(async move {
            let taken = till.take_in_monero(&order_id).await;
            apply(move |s| {
                s.taking.remove(&order_id);
                if let Ok(new) = taken {
                    s.open(new);
                }
            })
        });
    }

    /// "New code": the expired sale's next order is made, and the sale opens
    /// on its code.
    fn new_code(&mut self, order_id: OrderId) {
        if self.taking.contains(&order_id) {
            return;
        }
        let Some(till) = self.parts.as_ref().and_then(|p| p.runner.till()) else {
            return;
        };
        self.taking.insert(order_id.clone());
        self.task(async move {
            let made = till.new_code(&order_id).await;
            apply(move |s| {
                s.taking.remove(&order_id);
                match made {
                    Ok(new) => s.open(new),
                    Err(e) => s.problem = Some(e),
                }
            })
        });
    }

    /// The app came forward: everything is published again, what couldn't
    /// be read is read again, the store read live, an unreachable Bell
    /// tried now (decision 123), the shift started if it should be running
    /// and the point of sale asked now. A shift another process (the iOS
    /// extension) added sales to meanwhile is read again.
    /// Asks Android to look for Orbot ([`Effect::ProbeTor`]); iOS has no
    /// Tor (docs/ios-plan.md §11).
    fn probe_tor(&self) {
        if self.config.device == Device::Android {
            self.effect(Effect::ProbeTor);
        }
    }

    fn opened(&mut self) {
        self.visible = true;
        self.published = Published::default();
        // Orbot's probe as the app comes forward (decision 54).
        self.probe_tor();
        if self.parts.is_none() {
            self.build();
            return;
        }
        // What the extension wrote while the app was behind.
        self.ledger_changed();
        self.heartbeat += 1;
        self.beat(self.heartbeat);
        let Some(p) = &self.parts else { return };
        if let Err(e) = p.side.books().with(|l| l.maintain()) {
            self.log(format!("till-core: the ledger's upkeep: {e}"));
        }
        let model = p.side.model();
        p.live.read(model.connection.as_ref());
        retry_wake_ups(&p.wake_ups);
        if !p.runner.state().running && model.shift_on && model.connection.is_some() {
            p.runner.start();
        }
        p.runner.wake();
    }

    /// Another process wrote the ledger (iOS's extension), or the app is in
    /// front again: its projections are read again and what it left this
    /// process to deliver is claimed.
    fn ledger_changed(&mut self) {
        let Some(p) = &self.parts else { return };
        let books = p.side.books().clone();
        books.refresh();
        let claims = books.claim_due(ClaimPolicy::All);
        self.deliver(claims);
    }

    /// While the app is in front, the ledger hears so every few seconds
    /// (`Foreground`), so a payment is the paid moment rather than a notice.
    fn beat(&mut self, token: u64) {
        if !self.visible || token != self.heartbeat {
            return;
        }
        let until = self.now_ms().plus(FOREGROUND_FOR.as_millis() as i64);
        self.record(Fact::Foreground { until });
        self.task(async move {
            tokio::time::sleep(HEARTBEAT).await;
            apply(move |s| s.beat(token))
        });
    }

    /// Now, as the ledger's facts keep a time (Unix ms).
    fn now_ms(&self) -> UnixMillis {
        self.clock.now_ms()
    }

    /// Records `fact` in the engine's books.
    fn record(&self, fact: Fact) {
        if let Some(p) = &self.parts {
            p.side.books().record(fact);
        }
    }

    /// Keeps the settings the ledger's views and alerts read (docs/ledger.md
    /// §2): this counter's device, every device, cash as Monero, and how
    /// sales show up and whether other counters' are listed.
    fn keep_settings(&self) {
        let Some(p) = &self.parts else { return };
        let model = p.side.model();
        let at = self.now_ms();
        p.side.books().keep_options(&model.till_options(), at);
        p.side.books().keep_settings(&model.ledger_settings(), at);
    }

    /// The store or the point of sale's account changed: the old scope's
    /// rows are deleted and the ledger reads the new one's (docs/ledger.md
    /// §4: signing out, forgetting the store, another point of sale).
    fn rescope(&mut self) {
        let Some(p) = &self.parts else { return };
        let scope = crate::ledger::scope_of(&p.side.model());
        if self.scope.as_ref() == Some(&scope) {
            return;
        }
        if self.scope.is_some() {
            p.side.books().rescope();
        }
        self.scope = Some(scope);
    }

    /// Delivers what this process claimed in the ledger (docs/ledger.md
    /// §6), then says so: a new sale's alert while the app is behind; a
    /// payment's paid moment and haptic, with a notice while behind; a
    /// code that ran out unpaid. Notices are posted on Android, on iPad
    /// and on an iPhone without the Live Activity (the owner's fix 2).
    fn deliver(&mut self, claims: Vec<Claim>) {
        if claims.is_empty() {
            return;
        }
        let Some(p) = &self.parts else { return };
        let books = p.side.books().clone();
        let model = p.side.model();
        let till = model.point_of_sale;
        let money = Money(self.platform.clone());
        let android = self.config.device == Device::Android;
        let notices = android || !self.live_activity_on(&model);
        let quiet = self.visible;
        for claim in claims {
            let sale = TillSale::of_view(&claim.view);
            let order_id = sale.order_id.clone();
            match claim.kind {
                AlertKind::NewSale => {
                    if !self.visible {
                        self.alert_for = Some(order_id);
                    }
                }
                AlertKind::Paid | AlertKind::PaidMoment => {
                    self.just_paid(order_id.clone());
                    if self.visible {
                        self.effect(Effect::Haptic {
                            haptic: Haptic::Paid,
                        });
                    }
                    if notices && (claim.kind == AlertKind::Paid || android) {
                        self.effect(Effect::Notify {
                            order_id,
                            notice: notification::paid_notice(&sale, &money),
                            paid: true,
                            timeout_ms: notification::notice_timeout(true),
                            quiet,
                        });
                    }
                }
                // Always a notice, even beside a Live Activity: money is owed.
                AlertKind::Overpaid => self.effect(Effect::Notify {
                    order_id,
                    notice: notification::overpaid_notice(&sale, &money),
                    paid: false,
                    timeout_ms: notification::notice_timeout(false),
                    quiet,
                }),
                AlertKind::Ended => {
                    if sale.found.is_some() {
                        self.just_ended(order_id.clone());
                    }
                    if notices {
                        let pos = pos_text::of(&sale, till);
                        self.effect(Effect::Notify {
                            order_id,
                            notice: notification::unpaid_notice(&sale, pos, &money),
                            paid: false,
                            timeout_ms: notification::notice_timeout(false),
                            quiet,
                        });
                    }
                }
            }
            books.posted(&claim);
        }
    }

    /// One step back: in setup, from a sale to the shift, from its code or
    /// order to the sale; home from elsewhere.
    fn back(&mut self) {
        let setup_done = self.setup_done();
        match self.screen.clone() {
            Screen::Sale { order_id } => self.leave(order_id),
            Screen::Code { order_id } | Screen::Order { order_id } => {
                self.go(Screen::Sale { order_id })
            }
            Screen::Settings => self.close_settings(),
            Screen::Scan => self.go(Screen::Welcome),
            Screen::Confirm => self.go(Screen::Scan),
            Screen::Locations => self.go(Screen::PointOfSale),
            Screen::PointOfSale if setup_done => {
                self.from_settings = false;
                self.go(Screen::Settings);
            }
            // O2 back to O1, the choice.
            Screen::PointOfSale if self.pos_confirmed => self.pos_confirmed = false,
            Screen::Counter => self.go(Screen::PointOfSale),
            Screen::Alerts => self.go(Screen::PointOfSale),
            Screen::Float => self.go(Screen::Alerts),
            Screen::ShowSales => self.go(Screen::Alerts),
            Screen::Ready if self.config.device == Device::Android => self.go(Screen::Float),
            Screen::Ready if self.config.device == Device::Ipad => self.go(Screen::ShowSales),
            Screen::Ready => self.go(Screen::Alerts),
            _ => self.go_home(),
        }
    }

    /// The point of sale chosen, and whether the till is signed in to it at
    /// a Square location or Clover merchant: step 4's card says connected.
    fn pos_connected(&self) -> (Option<PointOfSale>, bool) {
        let Some(p) = &self.parts else {
            return (None, false);
        };
        let model = p.side.model();
        let chosen = model.point_of_sale;
        let connected = settings::connected(
            chosen,
            chosen.is_some_and(|pos| p.side.signed_in(pos)),
            model.location.is_some(),
            model.clover_merchant.is_some(),
        );
        (chosen, connected)
    }

    /// The screen's way on. On step 4 it is the button under the card: the
    /// chosen point of sale's sign-in until the till is connected to it,
    /// then "Continue" (or "Done", opened from Settings).
    fn next(&mut self) {
        match self.screen {
            Screen::Welcome => self.go(Screen::Scan),
            Screen::Connected => self.go(Screen::PointOfSale),
            Screen::PointOfSale => match self.pos_connected() {
                (None, _) => {}
                // O1's "Continue with Square": on to signing in (decision 121).
                (Some(_), false) if !self.setup_done() && !self.pos_confirmed => {
                    self.pos_confirmed = true;
                }
                (Some(_), false) => self.sign_in(None),
                (Some(_), true) => self.pos_done(),
            },
            Screen::Counter => self.go(Screen::Alerts),
            Screen::Alerts if self.config.device == Device::Android => self.go(Screen::Float),
            Screen::Alerts if self.config.device == Device::Ipad => self.go(Screen::ShowSales),
            Screen::Alerts => self.go(Screen::Ready),
            Screen::ShowSales => self.go(Screen::Ready),
            Screen::Float => {
                self.keep(|m| m.bubble_on = true);
                self.go(Screen::Ready);
            }
            _ => {}
        }
    }

    /// The screen's way past, below its button: on step 4 "Not now: only
    /// show other tills' sales" until the till is connected, then "Skip the
    /// test"; step 5's "Not now"; Android's "Use notifications only".
    fn skip(&mut self) {
        match self.screen {
            Screen::PointOfSale => match self.pos_connected() {
                (_, false) => self.without_pos(),
                (_, true) => self.pos_done(),
            },
            Screen::Alerts => self.next(),
            Screen::Float => {
                self.keep(|m| m.bubble_on = false);
                self.go(Screen::Ready);
            }
            _ => {}
        }
    }

    /// "Not now": only other tills' sales.
    fn without_pos(&mut self) {
        if self.keep(|m| m.without_pos = true) {
            self.restart();
            let next = if self.setup_done() {
                Screen::Till
            } else {
                Screen::Alerts
            };
            self.go(next);
        }
    }

    /// Step 4 connected: on to alerts, or back to Settings.
    fn pos_done(&mut self) {
        if self.setup_done() {
            self.back();
        } else {
            // The test sale has done its job: the till opens on its idle
            // screen, the sale in its list.
            if let Some(p) = &self.parts {
                let sales = p.runner.state().till.sales;
                self.left.extend(sales.into_iter().map(|s| s.order_id));
            }
            // A test sale that names its device names this counter's (C4,
            // decision 113), on Square or Clover (decision 142); skipped,
            // every device's sales are this till's.
            let device = self.test_sale().and_then(|sale| {
                sale.pos_device()
                    .map(|(id, name)| (id.to_owned(), name.map(str::to_owned)))
            });
            match device {
                Some((id, name)) => {
                    self.learn_device(id, name);
                    self.go(Screen::Counter);
                }
                None => self.go(Screen::Alerts),
            }
        }
    }

    /// A connect link pasted, scanned or opened as an App Link: step 2 says
    /// `empty` when there was nothing, `problem` when it isn't a store's.
    fn connect_link(&mut self, text: Option<&str>, empty: ScanProblem, problem: ScanProblem) {
        let text = text.map(str::trim).filter(|t| !t.is_empty());
        let Some(text) = text else {
            self.scan_problem = Some(empty);
            return;
        };
        match ConnectCode::parse(text, self.config.debug) {
            Some(code) => {
                self.scan_problem = None;
                self.confirm = Some(code);
                self.go(Screen::Confirm);
            }
            None => self.scan_problem = Some(problem),
        }
    }

    /// Step 3: the code redeemed, a registration left from an earlier store
    /// taken back, and everything about it forgotten, before the new
    /// connection is kept.
    fn connect(&mut self) {
        let (Some(code), Some(p)) = (self.confirm.clone(), &self.parts) else {
            return;
        };
        let (monokulo, wake_ups) = (p.monokulo.clone(), p.wake_ups.clone());
        // Over Tor when the store has an onion address and the platform's
        // probe found Orbot running (decision 55); off it until a probe says so.
        let orbot = self.tor_running == Some(true);
        self.work(async move {
            match monokulo.connect(&code).await {
                Ok(connection) => {
                    let connection = Connection {
                        tor: connection.onion_api.is_some() && orbot,
                        ..connection
                    };
                    wake_ups.unregister(true).await;
                    apply(move |s| s.connected(connection))
                }
                Err(e) => {
                    let said = problems::say(&e);
                    apply(move |s| s.problem = Some(said))
                }
            }
        });
    }

    fn connected(&mut self, connection: Connection) {
        let Some(p) = &self.parts else { return };
        let kept = p.side.kept.clone();
        let kept_it = kept
            .forget_all()
            .and_then(|()| kept.update(|m| m.connection = Some(connection.clone())));
        if let Err(e) = kept_it {
            self.problem = Some(format!("Couldn't keep the connection: {e}"));
            return;
        }
        p.live.forget();
        p.live.read(Some(&connection));
        self.generation += 1;
        self.tender = None;
        self.pos_confirmed = false;
        self.finding = None;
        self.counter = None;
        self.left.clear();
        self.new_runner();
        self.go(Screen::Connected);
    }

    /// A point of sale tapped, in step 4 or Settings ([`PosSwitch`],
    /// decision 138).
    fn choose_point_of_sale(&mut self, to: PointOfSale) {
        let Some(p) = &self.parts else { return };
        let side = p.side.clone();
        let from = side.model().point_of_sale;
        let signed_in = from.is_some_and(|from| side.signed_in(from));
        self.work(async move {
            let changed = PosSwitch::choose(&*side, from, signed_in, to).await;
            apply(move |s| {
                if !changed {
                    return;
                }
                // Another point of sale's devices aren't this counter's.
                s.keep(|m| {
                    m.without_pos = false;
                    m.counter_device = None;
                    m.alert_every = false;
                });
                s.counter = None;
                s.tender = None;
                s.restart();
                // Settings' rows stay in Settings; step 4 shows the new card.
                if s.screen != Screen::Settings {
                    s.go(Screen::PointOfSale);
                }
            })
        });
    }

    /// The chosen point of sale's sign-in page for this attempt, kept
    /// pending first: a page whose attempt couldn't be kept isn't opened.
    fn sign_in(&mut self, application_id: Option<String>) {
        let Some(p) = &self.parts else { return };
        let side = p.side.clone();
        let Some(pos) = side.model().point_of_sale else {
            return;
        };
        let pkce = Pkce::create();
        let (through, url) = match pos {
            PointOfSale::Square => {
                let through =
                    application_id.filter(|id| *id != side.square_config(None).application_id);
                let url = side.square_auth(through.as_deref()).authorize_url(&pkce);
                (through, url)
            }
            PointOfSale::Clover => {
                let through =
                    application_id.filter(|id| *id != side.clover_config(None).application_id);
                let url = side.clover_auth(through.as_deref()).authorize_url(&pkce);
                (through, url)
            }
        };
        let pending = PendingSignIn {
            point_of_sale: pos,
            pkce,
            application_id: through,
        };
        if self.keep(|m| m.pending_sign_in = Some(pending)) {
            self.effect(Effect::OpenUrl { url });
        }
    }

    /// A point of sale's redirect, back in the app. One with no sign-in of
    /// that point of sale pending is dropped (decision 127).
    fn sign_in_returned(&mut self, url: &str) {
        let Some(p) = &self.parts else { return };
        let side = p.side.clone();
        let Ok(parsed) = reqwest::Url::parse(url) else {
            return;
        };
        let returned = ReturnLink::new(url);
        let (square, clover) = (&self.config.square, &self.config.clover);
        let pos = if returned.is_for(&square.redirect_uri, square.return_uri.as_deref()) {
            PointOfSale::Square
        } else if returned.is_for(&clover.redirect_uri, clover.return_uri.as_deref()) {
            PointOfSale::Clover
        } else {
            self.log(format!(
                "till-core: a redirect to no sign-in: {}",
                returned.as_str()
            ));
            return;
        };
        let Some(pending) = side
            .model()
            .pending_sign_in
            .filter(|p| p.point_of_sale == pos)
        else {
            self.problem = Some(format!(
                "That {} sign-in wasn't started here. Try again.",
                pos.display_name()
            ));
            return;
        };
        let redirect = OAuthRedirect::parse(&parsed);
        let setup_done = side.model().setup_done;
        self.work(async move {
            let through = pending.application_id.is_some();
            let finished = match pos {
                PointOfSale::Square => finish_square(&side, &pending, &redirect)
                    .await
                    .map(Finished::Square),
                PointOfSale::Clover => finish_clover(&side, &pending, &redirect)
                    .await
                    .map(Finished::Clover),
            };
            apply(move |s| match finished {
                Ok(finished) => {
                    s.bell_sign_in = through;
                    match finished {
                        Finished::Square(mut locations) if locations.len() == 1 => {
                            s.choose_location(locations.remove(0));
                        }
                        Finished::Square(locations) => {
                            // The running till still asks Square as the old
                            // application: it starts again as the new one.
                            if through {
                                s.restart();
                            }
                            s.locations = locations;
                            s.go(Screen::Locations);
                        }
                        Finished::Clover(tender) => {
                            s.tender = Some(tender);
                            s.signed_in();
                        }
                    }
                }
                Err(e) => {
                    s.bell_sign_in = false;
                    s.go(if through && setup_done {
                        Screen::Settings
                    } else {
                        Screen::PointOfSale
                    });
                    s.problem = Some(e.to_string());
                }
            })
        });
    }

    fn choose_location(&mut self, location: SquareLocation) {
        if self.keep(|m| {
            m.location = Some(location);
            m.without_pos = false;
        }) {
            self.signed_in();
        }
    }

    /// Signed in at a Square location or a Clover merchant: registered
    /// with the Bell again for it, the till started, and back where the
    /// sign-in started.
    fn signed_in(&mut self) {
        if let Some(p) = &self.parts {
            p.wake_ups.recheck();
        }
        self.restart();
        let back_to_settings = self.bell_sign_in || self.from_settings;
        self.bell_sign_in = false;
        self.from_settings = false;
        let setup_done = self.setup_done();
        self.go(if back_to_settings && setup_done {
            Screen::Settings
        } else if setup_done {
            Screen::Till
        } else {
            Screen::PointOfSale
        });
    }

    fn change_location(&mut self) {
        let Some(p) = &self.parts else { return };
        let api = p.side.square_api();
        self.work(async move {
            let locations = api.locations().await;
            apply(move |s| match locations {
                Ok(locations) => {
                    s.locations = locations;
                    s.go(Screen::Locations);
                }
                Err(e) => s.problem = Some(e.to_string()),
            })
        });
    }

    fn look_again(&mut self) {
        let Some(p) = &self.parts else { return };
        let Some(merchant) = p.side.model().clover_merchant else {
            return;
        };
        let source = p.side.clover_source(merchant);
        self.work(async move {
            let tender = source.ensure_tender().await;
            apply(move |s| match tender {
                Ok(tender) => {
                    s.tender = Some(tender);
                    // A sale rung up with the tender just found is the till's.
                    s.restart();
                }
                Err(e) => s.problem = Some(e.to_string()),
            })
        });
    }

    /// Settings' "Sign out of …": the Bell's registration taken back
    /// first; the choices stay, so signing in again registers again.
    fn sign_out_of_pos(&mut self) {
        let Some(p) = &self.parts else { return };
        let side = p.side.clone();
        let Some(pos) = side.model().point_of_sale else {
            return;
        };
        self.work(async move {
            PosSwitch::sign_out(&*side, pos).await;
            apply(move |s| {
                if pos == PointOfSale::Clover {
                    s.tender = None;
                }
                s.restart();
                s.go(Screen::PointOfSale);
            })
        });
    }

    /// Watching on or off. Off brings every live code down; on after off
    /// starts a new shift, keeping only the sales that still need the
    /// cashier (decisions 64, 86).
    fn set_shift(&mut self, on: bool) {
        let Some(p) = &self.parts else { return };
        let side = p.side.clone();
        let was = side.model().shift_on;
        if on && !was {
            // A new window (docs/ledger.md §9): the sales that still need
            // the cashier stay listed.
            let now = self.clock.now();
            self.keep(|m| m.shift_started_at = Some(now));
            self.record(Fact::ShiftMarked {
                mark: ShiftMark::Started,
                at: now.to_millis(),
            });
            if let Some(p) = &self.parts
                && let Err(e) = p.side.books().with(|l| l.maintain())
            {
                self.log(format!("till-core: the ledger's upkeep: {e}"));
            }
        }
        if !on
            && was
            && let Some(p) = &self.parts
        {
            p.runner.close_live();
        }
        if !self.keep(|m| m.shift_on = on) {
            return;
        }
        let Some(p) = &self.parts else { return };
        if on {
            if !p.runner.state().running && side.model().connection.is_some() {
                p.runner.start();
            }
        } else {
            p.runner.stop();
        }
    }

    fn sale(&self, order_id: &OrderId) -> Option<TillSale> {
        self.parts
            .as_ref()?
            .runner
            .state()
            .till
            .sale(order_id)
            .cloned()
    }

    /// Shows a sale full screen.
    fn open(&mut self, order_id: OrderId) {
        self.left.remove(&order_id);
        self.step_back += 1;
        if self.setup_done() {
            self.go(Screen::Sale { order_id });
        }
    }

    /// The cashier left a sale's full screen for the shift.
    fn leave(&mut self, order_id: OrderId) {
        // Left by the cashier during a paid moment, the step back has nothing left to do.
        self.step_back += 1;
        let on_it = matches!(
            &self.screen,
            Screen::Sale { order_id: o } | Screen::Code { order_id: o } | Screen::Order { order_id: o }
                if *o == order_id
        );
        self.left.insert(order_id);
        if on_it {
            self.go(Screen::Till);
        }
    }

    /// The close sheet's answer, while the sale's code is still up: what it
    /// does ([`close::outcome`]), and where the cashier lands, from the sale
    /// or from the shift's row alike (decision 96).
    fn close(&mut self, order_id: OrderId, choice: CloseChoice) {
        if !close::applies(self.sale(&order_id).as_ref()) {
            return;
        }
        let outcome = close::outcome(choice);
        if outcome.refund {
            self.refund(order_id.clone(), outcome.close);
        } else if outcome.close
            && let Some(p) = &self.parts
        {
            p.runner.close(&order_id);
        }
        match outcome.then {
            // The refund shows on the sale's page, which asks for the
            // cashier's word when the point of sale refunds it.
            CloseThen::Sale => self.open(order_id),
            CloseThen::Shift => self.leave(order_id),
            CloseThen::Stay => {}
        }
    }

    /// Refunds a sale's point-of-sale sale (closing it first: the close
    /// sheet's "No"). A sale the till leaves to the cashier opens the
    /// refund sheet once it stands so.
    fn refund(&mut self, order_id: OrderId, close: bool) {
        if self.refunding.contains(&order_id) {
            return;
        }
        let Some(p) = &self.parts else { return };
        let Some(sale) = self.sale(&order_id) else {
            return;
        };
        let Some(found) = &sale.found else { return };
        let pos = found.point_of_sale;
        // Rung up at a point of sale this till has changed from: refunded
        // there, and only a sale already told so opens the sheet.
        if Some(pos) != p.side.model().point_of_sale && sale.standing() != Standing::RefundInPos {
            return;
        }
        let Some(job) = p.runner.refund(&order_id, close) else {
            return;
        };
        self.refunding.insert(order_id.clone());
        self.task(async move {
            let _ = job.await;
            apply(move |s| {
                s.refunding.remove(&order_id);
                let Some(p) = &s.parts else { return };
                if p.runner.standing(&order_id) == Some(Standing::RefundInPos) {
                    let source = p.runner.state().source;
                    s.refund_sheet = Some(RefundSheet {
                        order_id,
                        pos,
                        asks: pos == source && refunds_there(pos, source),
                        checking: false,
                        not_seen: false,
                    });
                }
            })
        });
    }

    fn confirm_refund(&mut self, order_id: OrderId) {
        let Some(sheet) = self
            .refund_sheet
            .as_mut()
            .filter(|sheet| sheet.order_id == order_id && !sheet.checking)
        else {
            return;
        };
        sheet.checking = true;
        sheet.not_seen = false;
        let Some(till) = self.parts.as_ref().and_then(|p| p.runner.till()) else {
            return;
        };
        self.task(async move {
            let seen = till.confirm_refund(&order_id).await;
            apply(move |s| {
                if let Some(sheet) = &mut s.refund_sheet
                    && sheet.order_id == order_id
                {
                    if seen {
                        s.refund_sheet = None;
                    } else {
                        sheet.checking = false;
                        sheet.not_seen = true;
                    }
                }
            })
        });
    }

    /// Settings, with the store read again as it opens.
    fn open_settings(&mut self) {
        let Some(p) = &self.parts else { return };
        p.live.read(p.side.model().connection.as_ref());
        retry_wake_ups(&p.wake_ups);
        self.before_settings = matches!(
            self.screen,
            Screen::Till | Screen::Sale { .. } | Screen::Code { .. } | Screen::Order { .. }
        )
        .then(|| self.screen.clone());
        self.go(Screen::Settings);
    }

    /// Back from Settings: to the sale or the shift it was opened from.
    fn close_settings(&mut self) {
        self.settings_back();
    }

    /// Reach the store over Tor, at its onion address, or not (decision 55).
    fn set_tor(&mut self, on: bool) {
        let Some(p) = &self.parts else { return };
        let Some(connection) = p.side.model().connection else {
            return;
        };
        let Some(onion) = connection.onion_api.clone().or(p.live.now().onion_api) else {
            return;
        };
        let changed = Connection {
            onion_api: Some(onion),
            tor: on,
            ..connection
        };
        if self.keep(|m| m.connection = Some(changed.clone())) {
            if let Some(p) = &self.parts {
                p.live.read(Some(&changed));
            }
            self.restart();
            self.go(Screen::Settings);
        }
    }

    /// The Bell forgets this till before the store does (acceptance
    /// A10.3), then everything is forgotten here.
    fn disconnect(&mut self) {
        let Some(p) = &self.parts else { return };
        let (monokulo, wake_ups) = (p.monokulo.clone(), p.wake_ups.clone());
        let connection = p.side.model().connection;
        self.work(async move {
            wake_ups.unregister(true).await;
            if let Some(connection) = connection {
                // The store may already have disconnected it.
                let _ = monokulo.disconnect(&connection).await;
            }
            apply(|s| s.forget_here())
        });
    }

    /// The store disconnected this device: forgotten, the Bell's
    /// registration taken back first.
    fn forget(&mut self) {
        let Some(p) = &self.parts else { return };
        let wake_ups = p.wake_ups.clone();
        self.work(async move {
            wake_ups.unregister(true).await;
            apply(|s| s.forget_here())
        });
    }

    fn forget_here(&mut self) {
        let Some(p) = &self.parts else { return };
        p.runner.stop();
        if let Err(e) = p.side.kept.forget_all() {
            self.problem = Some(format!("Couldn't forget the store: {e}"));
            return;
        }
        p.live.forget();
        self.tender = None;
        self.pos_confirmed = false;
        self.finding = None;
        self.devices_asked.clear();
        self.counter = None;
        self.refund_sheet = None;
        self.left.clear();
        self.before_settings = None;
        self.bell_sign_in = false;
        self.generation += 1;
        self.new_runner();
        self.go(Screen::Welcome);
    }

    fn choose_wake_ups(&mut self, choice: WakeUps) {
        let Some(p) = &self.parts else { return };
        let choice = match choice {
            WakeUps::Timer => WakeUpChoice::TimerOnly,
            WakeUps::MonokuloBell => WakeUpChoice::MonokuloBell,
            WakeUps::OwnBell { address } => {
                let Some(bell) = BellAddress::parse(&address, self.config.debug) else {
                    return;
                };
                WakeUpChoice::OwnBell { bell }
            }
        };
        p.wake_ups.choose(choice);
    }

    /// A push: Wake-ups hears it, and a ding wakes the running shift
    /// (outside one it goes nowhere, decision 93).
    ///
    /// One the relay carried comes still encrypted, and is opened here. It
    /// reaches the app only after iOS's Notification Service Extension has
    /// looked for the sale and kept what it found, so the kept shift is read
    /// again before the look the ding wakes, which would find nothing new.
    fn push_received(&mut self, content: &[u8], decrypted: bool) {
        let Some(p) = &self.parts else { return };
        let opened = match &p.relay {
            Some(relay) if !decrypted => relay.open(content),
            _ => None,
        };
        let relayed = opened.is_some();
        let (content, decrypted) = match &opened {
            Some(plain) => (plain.as_slice(), true),
            None => (content, decrypted),
        };
        let pos = p.side.model().point_of_sale;
        p.push.feed.message(content, decrypted, pos);
        if !is_ding(content, decrypted, pos) {
            return;
        }
        if relayed {
            // The extension looked first and wrote what it found.
            self.ledger_changed();
        }
        if let Some(p) = &self.parts {
            p.runner.ding();
        }
    }

    /// What changed about the sales since the last look that the ledger
    /// doesn't say: a sale gone from the list has its notification cleared,
    /// and the ledger hears which sale is open on screen (it gets the paid
    /// moment even when it's another counter's: the owner's fix 3).
    fn follow(&mut self) {
        let Some(p) = &self.parts else { return };
        let books = p.side.books().clone();
        let open = till::order_id(&self.screen).and_then(|id| books.sale_key(id));
        books.set_open_sale(open);
        let now: BTreeSet<OrderId> = p
            .runner
            .state()
            .till
            .sales
            .into_iter()
            .map(|sale| sale.order_id)
            .collect();
        if let Some(before) = self.listed.replace(now.clone()) {
            for gone in before.difference(&now) {
                self.effect(Effect::ClearNotification {
                    id: gone.to_string(),
                });
            }
        }
    }

    fn device_clock(&self) -> DeviceClock {
        DeviceClock {
            now: self.clock.now().0,
            utc_offset: self.utc_offset,
            hours_24: self.hours_24,
        }
    }

    fn tender_for_screen(&self) -> CloverTender {
        match &self.tender {
            Some(TenderState::Ready(_)) => CloverTender::Ready,
            Some(TenderState::AddInClover(_)) => CloverTender::AddInClover,
            None => CloverTender::Looking,
        }
    }

    fn store_now(p: &Parts) -> StoreNow {
        let now = p.live.now();
        StoreNow {
            store: now.store,
            device: now.device,
            onion_api: now.onion_api,
            reading: now.reading,
            problem: now.problem,
        }
    }

    fn wake_ups_input(&self, p: &Parts) -> WakeUpsInput {
        let model = p.side.model();
        WakeUpsInput {
            choice: p.wake_ups.choice().borrow().clone(),
            status: p.wake_ups.status().borrow().clone(),
            monokulo_bell: p.monokulo_bell.clone(),
            own_bell: model.own_bell,
            own_picked: self.own_picked,
            typed: self.own_typed.clone(),
            pos: model.point_of_sale,
            distributor_names: Default::default(),
            allow_http: self.config.debug,
            alerts_allowed: self.alerts_allowed,
            problem: None,
        }
    }

    fn settings_input(&self, p: &Parts) -> SettingsInput {
        let model = p.side.model();
        let chosen = model.point_of_sale;
        let signed_in = chosen.is_some_and(|pos| p.side.signed_in(pos));
        let tender_name = match chosen {
            Some(PointOfSale::Clover) => p.side.clover_config(None).tender_label,
            _ => p.side.square_config(None).tender_name,
        };
        let mut base_urls = std::collections::BTreeMap::new();
        for (pos, url) in [
            (PointOfSale::Square, &model.square_base_url),
            (PointOfSale::Clover, &model.clover_base_url),
        ] {
            if let Some(url) = url {
                base_urls.insert(pos.wire(), url.clone());
            }
        }
        SettingsInput {
            connection: model.connection.clone(),
            store: State::store_now(p),
            live: p.runner.state().live,
            pos: PosState {
                chosen,
                signed_in,
                location: model.location.clone(),
                merchant: model.clover_merchant.as_ref().map(|m| m.name.clone()),
                tender_name,
                tender: self.tender_for_screen(),
            },
            shift_on: model.shift_on,
            alerts: self.alerts_allowed,
            bubble_wanted: model.bubble_on,
            bubble_allowed: self.bubble_allowed,
            debug: self.config.debug,
            base_urls,
            // Orbot's probe stays on the platform (decision 10).
            tor_running: self.tor_running,
            wake_ups: self.wake_ups_input(p),
            live_activity: self.live_activity_on(&model),
            code_in_island: self.code_in_island_on(&model),
            choices: self.sale_choices(p, &model),
        }
    }

    /// Counters, cash and how a sale shows up, for Settings.
    fn sale_choices(&self, p: &Parts, model: &crate::store::Model) -> SaleChoices {
        let counters = self.counters();
        let sales = p.runner.state().till.sales;
        let mut devices: Vec<String> = Vec::new();
        let ids = counters.device.iter().map(|d| d.id.clone()).chain(
            sales
                .iter()
                .filter_map(|s| s.pos_device().map(|(id, _)| id.to_owned())),
        );
        for id in ids {
            let name = till_screens::counters::device_name(&id, &sales, &counters);
            if !devices.contains(&name) {
                devices.push(name);
            }
        }
        SaleChoices {
            devices,
            learned_at: model.counter_device.as_ref().map(|d| d.learned_at.0),
            finding_device: self.finding.is_some(),
            cash_as_monero: model.cash_as_monero,
            show_sales: model.show_sales,
            counters,
        }
    }

    /// Settings › Dynamic Island's switches, as they hold on this device:
    /// off but on iPhone, whatever is kept.
    fn live_activity_on(&self, model: &crate::store::Model) -> bool {
        self.config.device == Device::Iphone && model.live_activity
    }

    fn code_in_island_on(&self, model: &crate::store::Model) -> bool {
        self.config.device == Device::Iphone && model.code_in_island
    }

    /// The iPad's Picture in Picture window (docs/ipad-redesign.md
    /// decision 2): drawn from the Live Activity's state, while a new sale
    /// shows up in a floating window.
    fn pip_on(&self, model: &crate::store::Model) -> bool {
        self.config.device == Device::Ipad && model.show_sales == crate::ShowSales::FloatingWindow
    }

    fn onboarding_input(&self, p: &Parts) -> Option<OnboardingInput> {
        let model = p.side.model();
        let store = State::store_now(p);
        let step = match &self.screen {
            Screen::Welcome => Step::Welcome,
            Screen::Scan => Step::Scan {
                problem: self.scan_problem,
            },
            Screen::Confirm => {
                let code = self.confirm.as_ref()?;
                Step::Confirm {
                    host: code.host(),
                    encrypted: code.encrypted(),
                    device_name: p.side.identity().device_name,
                    busy: self.busy,
                    problem: self.problem.clone(),
                }
            }
            Screen::Connected => {
                let connection = model.connection.as_ref()?;
                Step::Connected {
                    host: connection.host(),
                    encrypted: connection.encrypted(),
                    store,
                }
            }
            Screen::PointOfSale => {
                let runner = p.runner.state();
                let money = Money(self.platform.clone());
                let chosen = model.point_of_sale;
                Step::PointOfSale(PosSetup {
                    chosen,
                    location: model.location.clone(),
                    merchant: model.clover_merchant.as_ref().map(|m| m.name.clone()),
                    signed_in: chosen.is_some_and(|pos| p.side.signed_in(pos)),
                    tender: self.tender_for_screen(),
                    busy: self.busy,
                    problem: self.problem.clone().or(runner.till.problem.clone()),
                    test_sale: runner
                        .till
                        .sales
                        .iter()
                        .find(|sale| sale.found.is_some())
                        .map(|sale| monokulo_client::amounts::price(&sale.price, &money)),
                    can_change_location: self.locations.len() > 1,
                    setup: !model.setup_done,
                    confirmed: self.pos_confirmed,
                })
            }
            Screen::Counter => {
                let sale = self.test_sale()?;
                let device = model.counter_device.as_ref()?;
                let money = Money(self.platform.clone());
                let counters = self.counters();
                Step::Counter(CounterSetup {
                    amount: monokulo_client::amounts::price(&sale.price, &money),
                    number: till_screens::sale_text::label(&sale),
                    device: device.name.clone().unwrap_or_else(|| {
                        till_screens::counters::device_name(&device.id, &[], &counters)
                    }),
                    place: model.place_name(),
                    every_device: model.alert_every,
                })
            }
            Screen::ShowSales => Step::ShowSales {
                chosen: model.show_sales,
                currency: model
                    .location
                    .as_ref()
                    .map(|l| l.currency.clone())
                    .or(store.store.as_ref().map(|s| s.base_currency.clone()))
                    .unwrap_or_else(|| "USD".into()),
            },
            Screen::Locations => Step::Locations {
                locations: self.locations.clone(),
            },
            Screen::Alerts => Step::Alerts {
                place: model.place_name(),
                currency: model
                    .location
                    .as_ref()
                    .map(|l| l.currency.clone())
                    .or(store.store.as_ref().map(|s| s.base_currency.clone()))
                    .unwrap_or_else(|| "USD".into()),
                allowed: self.alerts_allowed,
            },
            Screen::Float => Step::Float {
                allowed: self.bubble_allowed,
            },
            Screen::Ready => Step::Ready {
                store: store
                    .store
                    .as_ref()
                    .map(|s| s.name.clone())
                    .unwrap_or_default(),
                location: model.place_name(),
                alerts: self.alerts_allowed,
                bubble: model.bubble_on && self.bubble_allowed,
            },
            _ => return None,
        };
        Some(OnboardingInput {
            step,
            pos: model.point_of_sale,
        })
    }

    /// Sends each slice that changed since it was last sent.
    fn publish(&mut self) {
        macro_rules! send {
            ($slot:ident, $view:expr, $update:expr) => {{
                let view = $view;
                if self.published.$slot.as_ref() != Some(&view) {
                    self.published.$slot = Some(view.clone());
                    #[allow(clippy::redundant_closure_call)]
                    self.listener.on_update(($update)(view));
                }
            }};
        }
        send!(core, CORE_VERSION.to_owned(), |version| Update::Core {
            version
        });
        // Which way the change goes, from the screen last published; with
        // the screen unchanged, the way the last change went stands.
        let (back, steps) = match &self.published.screen {
            Some(from) if *from != self.screen => till::direction(from, &self.screen),
            _ => self
                .published
                .nav
                .as_ref()
                .map_or((false, false), |nav| (nav.3, nav.4)),
        };
        let nav = (
            self.by_itself,
            self.back_target(),
            self.back_enabled(),
            back,
            steps,
        );
        send!(nav, nav, |nav: NavSlice| {
            let (by_itself, back_target, back_enabled, back, steps) = nav;
            Update::Nav {
                by_itself,
                back_target,
                back_enabled,
                back,
                steps,
            }
        });
        // Orbot's probe as step 3 and Settings open (decision 54), once
        // each time one of them comes up.
        let probes = |s: &Screen| matches!(s, Screen::Confirm | Screen::Settings);
        if probes(&self.screen) && self.published.screen.as_ref().is_some_and(|s| !probes(s)) {
            self.probe_tor();
        }
        send!(screen, self.screen.clone(), |screen| Update::Screen {
            screen
        });
        let problem = self.problem.clone().or(self.load_problem.clone());
        // Step 3 and step 4 draw the busy button and the problem in their
        // own views; elsewhere the platform shows them (decision 109).
        let in_view = matches!(self.screen, Screen::Confirm | Screen::PointOfSale);
        let work = WorkSlice {
            shown_busy: self.busy && !in_view,
            shown_problem: if in_view {
                self.load_problem.clone()
            } else {
                problem.clone()
            },
            busy: self.busy,
            problem,
        };
        send!(work, work, |w: WorkSlice| Update::Work {
            busy: w.busy,
            problem: w.problem,
            shown_busy: w.shown_busy,
            shown_problem: w.shown_problem,
        });
        let Some(p) = &self.parts else { return };
        let device = self.config.device;
        let clock = self.device_clock();
        let running = p.runner.state().running;
        let onboarding = self
            .onboarding_input(p)
            .map(|input| onboarding::onboarding(&input, device, &Money(self.platform.clone())));
        let settings = settings::settings(&self.settings_input(p), device, clock);
        let wake_ups = wake_ups::wake_ups(&self.wake_ups_input(p), device, clock);
        send!(service, running, |running| Update::Service { running });
        if let Some(view) = onboarding {
            send!(onboarding, view, |view| Update::Onboarding { view });
        }
        send!(settings, settings, |view| Update::Settings { view });
        send!(wake_ups, wake_ups, |view| Update::WakeUps { view });
        let mut ticks = false;
        if self.setup_done() {
            let shift = self.shift_view(p);
            ticks |= shift.ticks;
            send!(shift, shift, |view| Update::Shift { view });
        }
        if let Some(sale) = self.sale_update(p) {
            if let Update::Sale { view, code, .. } = &sale {
                ticks |= view.ticks || code.ticks;
            }
            send!(sale, sale, |update| update);
        }
        let close = self.close_view(p);
        if close.is_none() {
            // Paid, ended or closed meanwhile: the sheet goes.
            self.close_sheet = None;
        }
        ticks |= close.as_ref().is_some_and(|c| c.ticks);
        let close = (self.close_sheet.clone(), close);
        send!(close, close, |(order_id, view)| Update::CloseSheet {
            order_id,
            view
        });
        let refund = self.refund_sheet_view(p);
        send!(refund, refund, |view| Update::RefundSheet { view });
        let notification = self.notification_view(p);
        send!(notification, notification, |view| Update::Notification {
            view
        });
        // An alert is posted once, as its sale arrives.
        self.alert_for = None;
        if device.is_ios() {
            let state = self.live_activity_state(p);
            send!(live_activity, state, |state| Update::LiveActivity { state });
        }
        // The panel counts down over other apps, with Till in the back.
        let mut panel_ticks = false;
        if self.has_bubble()
            && let Some(bubble) = self.bubble_view()
        {
            panel_ticks = bubble.shown
                && self.panel_up
                && bubble.panel.as_ref().is_some_and(|panel| panel.ticks);
            send!(bubble, bubble, |view| Update::Bubble { view });
        }
        self.tick_if((ticks && self.visible) || panel_ticks);
    }
}

/// Whether "No" on a sale of `pos` leaves the refund to the cashier there
/// (Android: `Closing.refundsThere`).
fn refunds_there(pos: PointOfSale, source: PointOfSale) -> bool {
    pos != source || source == PointOfSale::Clover
}

/// A Bell that couldn't be reached is tried again now, as what was in the
/// way may have been seen to (decision 123).
fn retry_wake_ups(wake_ups: &BellWakeUps) {
    if matches!(
        *wake_ups.status().borrow(),
        WakeUpStatus::Unreachable { .. }
    ) {
        wake_ups.recheck();
    }
}

/// Keeps where the store's onion entry is with the connection; one the
/// store no longer has takes the till off Tor.
fn keep_onion_api(side: &Side, onion: Option<&str>) {
    let Some(connection) = side.model().connection else {
        return;
    };
    if connection.onion_api.as_deref() == onion {
        return;
    }
    side.update(|m| {
        m.connection = Some(Connection {
            onion_api: onion.map(str::to_owned),
            tor: connection.tor && onion.is_some(),
            ..connection
        });
    });
}

/// A link a sign-in came back on, without its query and fragment: what is
/// compared with the setups' redirects.
struct ReturnLink<'a>(&'a str);

impl<'a> ReturnLink<'a> {
    fn new(url: &'a str) -> ReturnLink<'a> {
        ReturnLink(url.split(['?', '#']).next().unwrap_or(url))
    }

    fn as_str(&self) -> &'a str {
        self.0
    }

    /// Back on `redirect` itself, or on the build's return link to the app,
    /// `back` (decision 57): either way the exchange names the redirect.
    fn is_for(&self, redirect: &str, back: Option<&str>) -> bool {
        let matches = |uri: &str| ReturnLink::new(uri).0 == self.0;
        matches(redirect) || back.is_some_and(matches)
    }
}

/// What a finished sign-in leaves to choose.
enum Finished {
    Square(Vec<SquareLocation>),
    Clover(TenderState),
}

/// Square's redirect: the tokens kept, the till a Square till signed in
/// through the attempt's application, then its locations.
async fn finish_square(
    side: &Side,
    pending: &PendingSignIn,
    redirect: &OAuthRedirect,
) -> Result<Vec<SquareLocation>, PosError> {
    side.square_auth(pending.application_id.as_deref())
        .finish(&pending.pkce, redirect)
        .await?;
    let build = side.config.square.application_id.clone();
    side.update(|m| {
        m.pending_sign_in = None;
        m.point_of_sale = Some(PointOfSale::Square);
        if let Some(through) = &pending.application_id {
            // Signed in through the Bell's application: the till's from now (decision 96).
            m.square_application_id = Some(through.clone()).filter(|id| *id != build);
        }
    });
    side.square_api().locations().await
}

/// Clover's redirect: the tokens for the redirect's merchant, the merchant
/// read, then the Monero payment type found or added (decision 129).
async fn finish_clover(
    side: &Side,
    pending: &PendingSignIn,
    redirect: &OAuthRedirect,
) -> Result<TenderState, PosError> {
    let tokens = side
        .clover_auth(pending.application_id.as_deref())
        .finish(&pending.pkce, redirect)
        .await?;
    let build = side.config.clover.application_id.clone();
    side.update(|m| {
        m.pending_sign_in = None;
        if let Some(through) = &pending.application_id {
            m.clover_application_id = Some(through.clone()).filter(|id| *id != build);
        }
    });
    // The sign-in checked it: the merchant's id as `finish` read it.
    let merchant_id =
        till_pos::clover::CloverMerchantId::new(&tokens.merchant_id).ok_or_else(|| {
            PosError::answered(
                PointOfSale::Clover,
                0,
                "Clover didn't say which merchant signed in. Try again.",
            )
        })?;
    let merchant = side.clover_api().merchant(&merchant_id).await?;
    side.update(|m| {
        m.clover_merchant = Some(merchant.clone());
        m.point_of_sale = Some(PointOfSale::Clover);
        m.location = None;
        m.without_pos = false;
    });
    side.clover_source(merchant).ensure_tender().await
}

#[cfg(test)]
pub(crate) mod tests;

/// Every sale the till shows.
fn till_sales_ids(s: &State) -> Vec<OrderId> {
    s.parts
        .as_ref()
        .map(|p| {
            p.runner
                .state()
                .till
                .sales
                .into_iter()
                .map(|sale| sale.order_id)
                .collect()
        })
        .unwrap_or_default()
}
