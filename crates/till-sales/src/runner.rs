//! Running the till while a shift is on (Android: `TillRunner.kt`).

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use monokulo_client::OrderId;
use monokulo_client::{Clock, PointOfSale, SystemClock};
use till_payments::{BackendEvent, PaymentBackend, Sharing};
use till_pos::{FoundSale, PosError, RefundByTill, Refunds, SaleOrder, SaleSource};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::{Books, Shift, Standing, Till, TillOptions, TillState, WakeCause};

/// The till as the screens and the service see it.
#[derive(Clone, Debug, PartialEq)]
pub struct RunnerState {
    pub running: bool,
    /// The store's event stream is open.
    pub live: bool,
    /// The store disconnected this device: its key no longer works.
    pub disconnected: bool,
    /// What the event stream's last failure said, while it's closed:
    /// `too-many-streams`, say.
    pub stream_problem: Option<String>,
    /// The point of sale the till's source watches
    /// ([`SaleSource::point_of_sale`]): with a sale's own, whether its "No"
    /// is refunded by the till or by the cashier there (decision 130 in
    /// docs/decisions/android.md).
    pub source: PointOfSale,
    pub till: TillState,
}

impl Default for RunnerState {
    fn default() -> RunnerState {
        RunnerState {
            running: false,
            live: false,
            disconnected: false,
            stream_problem: None,
            source: PointOfSale::Square,
            till: TillState::default(),
        }
    }
}

/// A source for a till that runs without a point of sale signed in: it
/// shows other tills' sales only. A Square sale's refund fails rather than
/// marking the sale refunded with nothing sent to Square (decision 143 in
/// docs/decisions/android.md); any other point of sale's is marked to
/// refund there, as [`Till`] does for a sale its source didn't take.
pub struct NoSource;

#[async_trait]
impl SaleSource for NoSource {
    fn point_of_sale(&self) -> PointOfSale {
        PointOfSale::Square
    }

    async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError> {
        Ok(Vec::new())
    }

    async fn order(&self, _: &FoundSale) -> Result<SaleOrder, PosError> {
        Ok(SaleOrder::default())
    }

    fn refunds(&self) -> Refunds<'_> {
        Refunds::ByTill(self)
    }

    fn handled(&self, _: &FoundSale) {}
}

#[async_trait]
impl RefundByTill for NoSource {
    async fn refund(&self, _: &FoundSale) -> Result<Option<till_pos::ListedRefund>, PosError> {
        Err(PosError::NotAsked("Sign in to Square first".into()))
    }
}

/// What the runner asks of the app around it (Android: the `Graph` and
/// its stores, the Bell's wake-ups and the live store). The engine
/// implements it.
pub trait RunnerSide: Send + Sync {
    /// The payment backend this till is linked to, made from the link as
    /// it's kept now, while it has one (core.md decision 238).
    fn backend(&self) -> Option<Arc<dyn PaymentBackend>>;

    /// Reads the store live again ([`crate::LiveStore::read`]): a shift is
    /// starting.
    fn read_store(&self, backend: &Arc<dyn PaymentBackend>);

    /// Notes now as when the shift started, unless one has started already:
    /// the first run, in setup (or after an update from before shifts were
    /// saved), starts the first shift.
    fn start_shift(&self);

    /// The source for the point of sale the cashier chose, while signed in
    /// to it (with its location or merchant); `None` makes a till that only
    /// shows other tills' sales ([`NoSource`]). At Clover too the phone or
    /// iPad isn't the point-of-sale device but sits beside one, whose
    /// sales are this counter's once a test sale names it
    /// ([`RunnerSide::till_options`], core.md decision 142).
    fn source(&self) -> Option<Arc<dyn SaleSource>>;

    /// The ledger the till keeps its facts in (docs/ledger.md), the same
    /// for every till the runner makes.
    fn books(&self) -> Arc<Books>;

    /// How often to check without a wake-up: 30 s while the Bell is
    /// connected, else 2 s (decision 94). Read on every tick.
    fn interval(&self) -> Duration;

    /// The timer's check found `found` sales no check had found before:
    /// only the timer's finds tell whether the Bell missed a sale, a ding's
    /// or the app's don't.
    fn timer_found(&self, found: usize);

    /// Registers with the Bell again, so a registration the Bell dropped
    /// comes back (decision 95): a shift is starting.
    fn recheck_wake_ups(&self);

    /// How the cashier set the till up: this counter's device, and cash
    /// (core.md decisions 113 and 116). Read as each till is made.
    fn till_options(&self) -> TillOptions {
        TillOptions::default()
    }
}

/// What the runner holds while it has a till.
#[derive(Default)]
struct Parts {
    /// The shift's tasks: its checks, the event stream, the codes' clock
    /// and the confirmations.
    job: Option<JoinHandle<()>>,
    till: Option<Arc<Till>>,
    shift: Option<Arc<Shift>>,
    /// Copies the till's state into the runner's: it outlives `job`, so a
    /// stopped till's shift still shows and changes.
    showing: Option<JoinHandle<()>>,
    /// The app isn't in the foreground ([`TillRunner::foreground`]).
    behind: bool,
    /// The shift was stopped because the app left the foreground, and
    /// starts again when it's back.
    stopped_behind: bool,
}

struct Inner {
    side: Arc<dyn RunnerSide>,
    clock: Arc<dyn Clock>,
    runtime: Handle,
    state: watch::Sender<RunnerState>,
    parts: Mutex<Parts>,
}

/// Runs the till while a shift is on (docs/design.md §3): the POS checked
/// on a timer and whenever woken, and the store's event stream followed,
/// reconnecting with a growing wait. Held by the engine; the platform keeps
/// the process alive while it runs.
pub struct TillRunner {
    inner: Arc<Inner>,
}

impl TillRunner {
    /// The longest wait before the event stream is opened again.
    const MOST_STREAM_WAIT: Duration = Duration::from_secs(30);

    /// A runner over `side`, its tills making orders through the side's
    /// payment backend ([`RunnerSide::backend`]) and stamping times from
    /// `clock`, its tasks run on the tokio runtime it is made in.
    ///
    /// # Panics
    ///
    /// When made outside a tokio runtime.
    pub fn new(side: Arc<dyn RunnerSide>, clock: Arc<dyn Clock>) -> TillRunner {
        TillRunner {
            inner: Arc::new(Inner {
                side,
                clock,
                runtime: Handle::current(),
                state: watch::Sender::new(RunnerState::default()),
                parts: Mutex::new(Parts::default()),
            }),
        }
    }

    /// The runner's state as it changes.
    pub fn subscribe(&self) -> watch::Receiver<RunnerState> {
        self.inner.state.subscribe()
    }

    /// The runner's state now, with the till's as the ledger holds it this
    /// moment (the copy [`TillRunner::subscribe`] follows may lag behind).
    pub fn state(&self) -> RunnerState {
        let mut state = self.inner.state.borrow().clone();
        if let Some(till) = self.till() {
            state.till = till.state();
        }
        state
    }

    /// Starts a shift, unless one is running or there's no connection: the
    /// live store read, the shift's start noted, then the POS checked at
    /// once and on the timer, the event stream followed, live codes ended
    /// on time and confirmations followed, and the Bell registered again.
    pub fn start(&self) {
        let inner = &self.inner;
        if inner
            .parts()
            .job
            .as_ref()
            .is_some_and(|job| !job.is_finished())
        {
            return;
        }
        let Some(backend) = inner.side.backend() else {
            return;
        };
        if backend.sharing() == Sharing::ThisTill && inner.parts().behind {
            // The device's own wallet looks only while the app is in the
            // foreground (docs/view-key-mode.md §10).
            inner.parts().stopped_behind = true;
            return;
        }
        inner.side.read_store(&backend);
        inner.side.start_shift();
        let till = inner.new_till(backend);
        inner.show(till.clone());
        let interval = {
            let side = inner.side.clone();
            Arc::new(move || side.interval())
        };
        let on_checked = {
            let side = inner.side.clone();
            Arc::new(move |cause, found| {
                if cause == WakeCause::Timer {
                    side.timer_found(found);
                }
            })
        };
        let shift = Arc::new(Shift::new(till.clone(), interval, on_checked));
        inner.state.send_modify(|s| {
            s.running = true;
            s.live = false;
            s.disconnected = false;
            s.stream_problem = None;
        });
        let job = {
            let inner = inner.clone();
            let shift = shift.clone();
            inner.runtime.clone().spawn(async move {
                tokio::join!(
                    shift.run(),
                    inner.follow(&till),
                    till.lapse_on_time(|| SystemClock.now_ms()),
                );
            })
        };
        {
            let mut parts = inner.parts();
            parts.shift = Some(shift);
            parts.job = Some(job);
        }
        inner.side.recheck_wake_ups();
    }

    /// While the till isn't watching: the saved shift on screen, so its
    /// sales can still be opened and refunded (decision 86). The engine
    /// calls it as it starts with watching off.
    pub fn show_saved(&self) {
        if self.inner.parts().till.is_some() {
            return;
        }
        let Some(backend) = self.inner.side.backend() else {
            return;
        };
        let till = self.inner.new_till(backend);
        self.inner.show(till);
    }

    /// Stops the shift's tasks; the shift stays on screen.
    pub fn stop(&self) {
        if let Some(job) = self.inner.parts().job.take() {
            job.abort();
        }
        self.inner.state.send_modify(|s| {
            s.running = false;
            s.live = false;
        });
    }

    /// The app came to the foreground (`true`) or left it. With a backend
    /// that knows only this till's payments (the device's own wallet), the
    /// shift's tasks stop as the app leaves, so its event stream (the loop
    /// that looks at the chain) is no longer polled, and start again as it
    /// comes back, the stream's `Ready` catching up on what was mined
    /// meanwhile (docs/view-key-mode.md §10). Monokulo's shift runs on
    /// behind, as it always has.
    pub fn foreground(&self, front: bool) {
        let this_till = self.till().is_some_and(|till| till.this_till_only());
        let restart = {
            let mut parts = self.inner.parts();
            parts.behind = !front;
            front && std::mem::take(&mut parts.stopped_behind)
        };
        if !front && this_till {
            let running = self
                .inner
                .parts()
                .job
                .as_ref()
                .is_some_and(|job| !job.is_finished());
            if running {
                self.stop();
                self.inner.parts().stopped_behind = true;
            }
        } else if restart {
            self.start();
        }
    }

    /// The cashier stopped watching: every live sale's code comes down
    /// ([`Till::close_live`]), here and now, so the closes are saved before
    /// the service stops and before the next shift reads what the last one
    /// left to do.
    pub fn close_live(&self) {
        if let Some(till) = self.till() {
            till.close_live();
        }
    }

    /// The store disconnected this device (it answered 401): stop asking.
    pub fn disconnected(&self) {
        self.inner.disconnected();
    }

    /// Check the POS now (the app came to the front).
    pub fn wake(&self) {
        if let Some(shift) = self.inner.parts().shift.clone() {
            shift.wake(WakeCause::Opened);
        }
    }

    /// A ding from the Monokulo Bell: check the POS now, while a shift
    /// runs. Outside one it goes nowhere (decision 93).
    pub fn ding(&self) {
        let parts = self.inner.parts();
        if parts.job.as_ref().is_some_and(|job| !job.is_finished())
            && let Some(shift) = &parts.shift
        {
            shift.wake(WakeCause::Ding);
        }
    }

    /// Refunds the sale's POS sale ([`Till::refund`]), closing it first when
    /// `close` says (the close sheet's "No"). The task ends when the POS
    /// has answered; `None` with no till.
    pub fn refund(&self, order_id: &OrderId, close: bool) -> Option<JoinHandle<()>> {
        let till = self.till()?;
        let order_id = order_id.to_owned();
        Some(self.inner.runtime.spawn(async move {
            if close {
                till.close(&order_id);
            }
            till.refund(&order_id).await;
        }))
    }

    /// Where `order_id` stands in the till itself, ahead of the runner's
    /// copy.
    pub fn standing(&self, order_id: &OrderId) -> Option<Standing> {
        self.till()?
            .state()
            .sale(order_id)
            .map(|sale| sale.standing())
    }

    /// The refund sheet's "I've refunded it": whether the point of sale now
    /// shows the sale refunded (decision 130).
    pub async fn confirm_refund(&self, order_id: &OrderId) -> bool {
        match self.till() {
            Some(till) => till.confirm_refund(order_id).await,
            None => false,
        }
    }

    /// Closes the sale for now (the close sheet's "Not sure"): its code
    /// comes down, it stays in the shift.
    pub fn close(&self, order_id: &OrderId) {
        if let Some(till) = self.till() {
            till.close(order_id);
        }
    }

    /// "Show the code again" on a closed sale.
    pub fn reopen(&self, order_id: &OrderId) {
        if let Some(till) = self.till() {
            till.reopen(order_id);
        }
    }

    /// "Keep as cash" on an offered cash sale.
    pub fn keep_as_cash(&self, order_id: &OrderId) {
        if let Some(till) = self.till() {
            till.keep_as_cash(order_id);
        }
    }

    /// "Paid in cash" on a cash sale with a code.
    pub fn paid_in_cash(&self, order_id: &OrderId) {
        if let Some(till) = self.till() {
            till.paid_in_cash(order_id);
        }
    }

    /// "Mark as refunded" on an overpaid sale.
    pub fn extra_refunded(&self, order_id: &OrderId) {
        if let Some(till) = self.till() {
            till.extra_refunded(order_id);
        }
    }

    /// The till on screen, once there is one.
    pub fn till(&self) -> Option<Arc<Till>> {
        self.inner.parts().till.clone()
    }
}

impl Drop for TillRunner {
    fn drop(&mut self) {
        let mut parts = self.inner.parts();
        for task in [parts.job.take(), parts.showing.take()]
            .into_iter()
            .flatten()
        {
            task.abort();
        }
    }
}

impl Inner {
    fn parts(&self) -> MutexGuard<'_, Parts> {
        self.parts.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The till for the point of sale the cashier chose and signed in to,
    /// or one that only shows other tills' sales.
    fn new_till(&self, backend: Arc<dyn PaymentBackend>) -> Arc<Till> {
        let source = self.side.source();
        let with_pos = source.is_some();
        let source = source.unwrap_or_else(|| Arc::new(NoSource));
        let pos = source.point_of_sale();
        self.state.send_modify(|s| s.source = pos);
        let till = Till::new(source, backend, self.side.books())
            .with_clock(self.clock.clone())
            .with_options(&self.side.till_options());
        till.books().refresh();
        Arc::new(if with_pos { till } else { till.without_pos() })
    }

    fn show(&self, till: Arc<Till>) {
        let mut rx = till.subscribe();
        let state = self.state.clone();
        let showing = self.runtime.spawn(async move {
            loop {
                let now = rx.borrow_and_update().clone();
                state.send_modify(|s| s.till = now);
                if rx.changed().await.is_err() {
                    return;
                }
            }
        });
        let mut parts = self.parts();
        parts.till = Some(till);
        if let Some(old) = parts.showing.replace(showing) {
            old.abort();
        }
    }

    fn disconnected(&self) {
        self.state.send_modify(|s| {
            s.live = false;
            s.disconnected = true;
        });
        if let Some(job) = self.parts().job.take() {
            job.abort();
        }
    }

    /// Follows the store's event stream, opening it again a second after it
    /// fails, the wait doubling to 30 s, and back to a second once it's
    /// open. A 401 is the store having disconnected this device.
    async fn follow(&self, till: &Till) {
        let Some(backend) = self.side.backend() else {
            return;
        };
        let mut wait = Duration::from_secs(1);
        loop {
            let mut events = backend.events();
            while let Some(event) = events.next().await {
                match event {
                    Ok(BackendEvent::Ready | BackendEvent::Lagged) => {
                        wait = Duration::from_secs(1);
                        self.state.send_modify(|s| {
                            s.live = true;
                            s.stream_problem = None;
                        });
                        self.resync(till).await;
                    }
                    Ok(BackendEvent::Changed(payment)) => till.on_change(&payment),
                    Err(e) => {
                        if e.disconnected() {
                            self.disconnected();
                            return;
                        }
                        if e.problem().is_some() {
                            let said = e.say();
                            self.state.send_modify(|s| s.stream_problem = Some(said));
                        }
                        break;
                    }
                }
            }
            self.state.send_modify(|s| s.live = false);
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(TillRunner::MOST_STREAM_WAIT);
        }
    }

    /// Refreshes the sales on screen, after the stream (re)opens: an
    /// expired sale too, until the backend stops watching it for a late
    /// payment. Through the link as it's kept now.
    async fn resync(&self, till: &Till) {
        let Some(backend) = self.side.backend() else {
            return;
        };
        let refs = till.open_refs();
        if refs.is_empty() {
            return;
        }
        if let Ok(payments) = backend.refresh(&refs).await {
            for payment in &payments {
                till.on_payment(payment);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use monokulo_client::{
        AppIdentity, Connection, FixedClock, MonokuloApi, SalePhase, UnixSeconds,
    };

    use till_ledger::{Fact, OrderCreated, OrderId, SaleKey};
    use till_payments::{BackendError, FakeBackend, MonokuloBackend, Payment, PaymentRef, Sharing};

    use super::*;
    use crate::books::testing::TestHost;

    /// The engine's side, counting what the runner asked of it. Not part of
    /// the Kotlin oracle: `TillRunner` had no unit tests.
    struct Side {
        backend: Option<Arc<dyn PaymentBackend>>,
        books: Arc<Books>,
        _dir: tempfile::TempDir,
        reads: AtomicUsize,
        starts: AtomicUsize,
        rechecks: AtomicUsize,
    }

    impl RunnerSide for Arc<Side> {
        fn backend(&self) -> Option<Arc<dyn PaymentBackend>> {
            self.backend.clone()
        }
        fn read_store(&self, _: &Arc<dyn PaymentBackend>) {
            self.reads.fetch_add(1, Ordering::SeqCst);
        }
        fn start_shift(&self) {
            self.starts.fetch_add(1, Ordering::SeqCst);
        }
        fn source(&self) -> Option<Arc<dyn SaleSource>> {
            None
        }
        fn books(&self) -> Arc<Books> {
            self.books.clone()
        }
        fn interval(&self) -> Duration {
            Duration::from_secs(2)
        }
        fn timer_found(&self, _: usize) {}
        fn recheck_wake_ups(&self) {
            self.rechecks.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// The store's Monokulo at `api`, as the engine makes it.
    fn monokulo(api: String) -> Arc<dyn PaymentBackend> {
        let monokulo = MonokuloApi::new(
            reqwest::Client::new(),
            AppIdentity {
                slug: "till".into(),
                name: "Till".into(),
                version: "0.1.0".into(),
                platform: "android".into(),
                device_name: "Pixel".into(),
            },
            Arc::new(FixedClock::at(1_761_000_000)),
        );
        Arc::new(MonokuloBackend::new(monokulo, Connection::new("ak_1", api)))
    }

    /// The url the saved sale `ord_2` is read again at.
    const ORD_2: &str = "fake:ord_2";

    fn runner(backend: Option<Arc<dyn PaymentBackend>>) -> (TillRunner, Arc<Side>) {
        let dir = tempfile::tempdir().unwrap();
        let books = Arc::new(Books::new(TestHost::new(dir.path()), None));
        books.set_clock(Arc::new(FixedClock::at(60)));
        // Another till's sale, waiting, known from Monokulo alone.
        books.record(Fact::OrderCreated(OrderCreated {
            sale: SaleKey::Order(OrderId::new("ord_2")),
            order_id: OrderId::new("ord_2"),
            url: Some(ORD_2.into()),
            payment_uri: "monero:8Cy?tx_amount=0.03".into(),
            address: "8Cy".into(),
            xmr: "0.03".into(),
            price: till_types::Price::new("5.00", "USD"),
            reference: None,
            expires_at: UnixSeconds(0),
            confirmations_required: None,
            device_name: None,
            renew: None,
            price_used: None,
        }));
        let side = Arc::new(Side {
            backend,
            books,
            _dir: dir,
            reads: AtomicUsize::new(0),
            starts: AtomicUsize::new(0),
            rechecks: AtomicUsize::new(0),
        });
        let runner = TillRunner::new(Arc::new(side.clone()), Arc::new(FixedClock::at(70)));
        (runner, side)
    }

    /// `ord_2` as a backend says it stands.
    fn ord_2(phase: SalePhase) -> Payment {
        Payment {
            reference: PaymentRef::new(ORD_2),
            order_id: "ord_2".into(),
            address: "8Cy".into(),
            xmr_piconero: 30_000_000_000,
            expires_at: UnixSeconds(0),
            phase,
            confirmations: 0,
            required: 10,
            is_final: false,
            received: 0,
            overpaid: false,
            price: None,
            amount: Some("5.00".into()),
            currency: Some("USD".into()),
            receipt: None,
            device_name: None,
            renew: None,
        }
    }

    async fn until(runner: &TillRunner, what: impl FnMut(&RunnerState) -> bool) -> RunnerState {
        let mut rx = runner.subscribe();
        tokio::time::timeout(Duration::from_secs(5), rx.wait_for(what))
            .await
            .expect("within 5 s")
            .unwrap()
            .clone()
    }

    #[tokio::test]
    async fn without_a_connection_nothing_starts() {
        let (runner, side) = runner(None);
        runner.start();
        runner.show_saved();
        assert!(!runner.state().running);
        assert!(runner.till().is_none());
        assert_eq!(side.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_shift_shows_the_saved_sales_and_watching_stopped_closes_the_live_ones() {
        // A Monokulo that answers nothing: the event stream fails and waits to try again.
        let server = wiremock::MockServer::builder().start().await;
        let (runner, side) = runner(Some(monokulo(format!("{}/api/v1", server.uri()))));
        runner.start();
        assert!(runner.state().running);
        assert_eq!(runner.state().source, PointOfSale::Square, "NoSource's");
        assert_eq!(side.reads.load(Ordering::SeqCst), 1);
        assert_eq!(side.starts.load(Ordering::SeqCst), 1);
        assert_eq!(side.rechecks.load(Ordering::SeqCst), 1);
        runner.start();
        assert_eq!(side.reads.load(Ordering::SeqCst), 1, "one shift at a time");
        until(&runner, |s| s.till.sales.len() == 1).await;
        assert_eq!(
            runner.standing(&"ord_2".into()),
            Some(Standing::Live(monokulo_client::SalePhase::Waiting))
        );
        runner.close_live();
        until(&runner, |s| {
            s.till.sales[0].closed_at == Some(UnixSeconds(70))
        })
        .await;
        assert_eq!(
            side.books
                .view(&SaleKey::Order(OrderId::new("ord_2")))
                .unwrap()
                .closed_at,
            Some(UnixSeconds(70)),
            "and kept"
        );
        runner.stop();
        assert!(!runner.state().running);
        runner.reopen(&"ord_2".into());
        until(&runner, |s| s.till.sales[0].closed_at.is_none()).await;
    }

    /// Any backend drives the shift as Monokulo's stream did (core.md
    /// decision 238): its stream opening refreshes the open sales, a
    /// change reaches the sale, and a disconnection stops the shift.
    #[tokio::test]
    async fn a_backends_events_and_refreshes_move_the_shift_on_and_its_disconnection_stops_it() {
        let backend = Arc::new(FakeBackend::new(Sharing::ThisTill));
        backend.set(ord_2(SalePhase::Seen));
        let (runner, _side) = runner(Some(backend.clone()));
        runner.start();
        let seen = until(&runner, |s| {
            s.live && s.till.sales.first().map(|sale| sale.phase) == Some(SalePhase::Seen)
        })
        .await;
        assert!(!seen.disconnected);
        assert!(backend.refreshes() >= 1, "the stream's Ready refreshed");
        backend.emit(BackendEvent::Changed(Payment {
            is_final: true,
            ..ord_2(SalePhase::Paid)
        }));
        until(&runner, |s| {
            s.till.sales.first().map(|sale| sale.phase) == Some(SalePhase::Paid)
        })
        .await;
        backend.fail_streams(|| {
            BackendError::Monokulo(monokulo_client::MonokuloError::Refused {
                status: 401,
                detail: "This key was not accepted.".into(),
                problem: None,
                retry_after: None,
            })
        });
        let stopped = until(&runner, |s| s.disconnected).await;
        assert!(!stopped.live);
    }

    /// Ported from the Android app's `NoSourceTest`: the source of a till
    /// not signed in to a point of sale (decision 143).
    #[tokio::test]
    async fn a_square_sales_refund_fails_with_nothing_sent_so_the_till_never_marks_it_refunded() {
        let sale = FoundSale {
            id: "p_1".into(),
            amount: till_types::Money::new(840, "USD".parse().unwrap()),
            reference: Some("bP9m".into()),
            order_id: Some("o_1".into()),
            created_at: "2026-10-22T22:50:00Z".parse().unwrap(),
            updated_at: None,
            status: None,
            device_id: None,
            device_name: None,
            cash: false,
            title: None,
            point_of_sale: PointOfSale::Square,
        };
        let Refunds::ByTill(refunds) = NoSource.refunds() else {
            panic!("NoSource refunds by till");
        };
        let refused = refunds.refund(&sale).await.unwrap_err();
        assert_eq!(refused.to_string(), "Sign in to Square first");
        assert!(matches!(refused, PosError::NotAsked(_)), "{refused:?}");
        assert_eq!(
            refused.pos(),
            None,
            "a bare PosException, no SquareException"
        );
        assert_eq!(
            NoSource.point_of_sale(),
            PointOfSale::Square,
            "a Clover sale isn't its own, so Till marks it to refund in Clover"
        );
    }
}
