//! The till's books: the one ledger (till-ledger, docs/ledger.md) a process
//! holds, written one fact at a time, and the till's state read back from
//! its projections after each write.
//!
//! Nothing here keeps a list of sales of its own: [`TillState::sales`] is
//! always what [`Ledger::sales`] says, within the shift's window
//! ([`Ledger::shift_window`]), so the app, the iOS extension and a restart
//! all see the same thing. The process's platform opens the ledger
//! ([`LedgerHost`]); the books close it when the process is suspended
//! ([`Books::suspend`]) and open it again on the next write or read.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use till_ledger::{
    Claim, ClaimPolicy, Fact, Ledger, LedgerError, Opened, Recorded, SaleKey, SaleRow, SaleView,
    Scope, UnixMillis, UnixSeconds, ViewSettings, view::setting,
};
use tokio::sync::watch;

use crate::{TillOptions, TillSale, TillState};

/// What the process gives the books: how to open the ledger, how to keep
/// the process awake across a write, and where claimed alerts go.
pub trait LedgerHost: Send + Sync {
    /// The scope the ledger's rows are under now: the store and the point
    /// of sale's account (docs/ledger.md §4).
    fn scope(&self) -> Scope;

    /// Opens the ledger with the platform's key, for this process and
    /// [`LedgerHost::scope`], reading `clock`.
    fn open(&self, clock: Arc<dyn till_ledger::Clock>) -> Result<Opened, LedgerError>;

    /// Called once after each open, with the ledger: what the process does
    /// first (the app imports the old kept shift, docs/ledger.md §10).
    fn opened(&self, _ledger: &mut Ledger) {}

    /// Holds the process awake while the books write (iOS's background
    /// task assertion, docs/ledger.md §11); dropped when the write is done.
    fn hold(&self) -> Box<dyn Send> {
        Box::new(())
    }

    /// Alerts this process claimed and must deliver, then report with
    /// [`Books::posted`]. Called outside the books' lock.
    fn claimed(&self, _claims: Vec<Claim>) {}

    /// Something went wrong with the ledger, in words for the log.
    fn log(&self, _line: String) {}
}

/// The ledger one process holds, and the till's state read from it.
pub struct Books {
    host: Arc<dyn LedgerHost>,
    clock: TillClock,
    ledger: Mutex<Option<Ledger>>,
    open_sale: Mutex<Option<SaleKey>>,
    state: watch::Sender<TillState>,
    /// After each look, claim the alerts other processes left (the app).
    claims: Option<ClaimPolicy>,
    /// The first look in this process has been made: learning without
    /// alerting (docs/ledger.md §6) ends with it.
    looked: AtomicBool,
}

impl Books {
    /// The books over `host`'s ledger, opened on first use. `claims` is the
    /// app's [`ClaimPolicy::All`], which claims what the extension left
    /// after every look; the extension claims its one alert itself.
    pub fn new(host: Arc<dyn LedgerHost>, claims: Option<ClaimPolicy>) -> Books {
        Books {
            host,
            clock: TillClock(Arc::new(RwLock::new(Arc::new(
                monokulo_client::SystemClock,
            )))),
            ledger: Mutex::new(None),
            open_sale: Mutex::new(None),
            state: watch::Sender::new(TillState::default()),
            claims,
            looked: AtomicBool::new(false),
        }
    }

    /// The ledger reads `clock` from now on, as the till does.
    pub fn set_clock(&self, clock: Arc<dyn monokulo_client::Clock>) {
        *self.clock.0.write().unwrap_or_else(|e| e.into_inner()) = clock;
    }

    fn slot(&self) -> MutexGuard<'_, Option<Ledger>> {
        self.ledger.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs `f` on the ledger, opening it first if it's closed, with the
    /// process held awake meanwhile.
    pub fn with<R>(
        &self,
        f: impl FnOnce(&mut Ledger) -> Result<R, LedgerError>,
    ) -> Result<R, LedgerError> {
        let _hold = self.host.hold();
        let mut slot = self.slot();
        if slot.is_none() {
            let mut ledger = self.host.open(Arc::new(self.clock.clone()))?.ledger;
            ledger.set_open_sale(
                self.open_sale
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone(),
            );
            self.host.opened(&mut ledger);
            *slot = Some(ledger);
        }
        f(slot.as_mut().expect("opened above"))
    }

    /// Records `fact`; when it changed something, the till's state is read
    /// again and any alerts this process claimed go to the host. A ledger
    /// that can't be written is logged and kept as the till's problem.
    pub fn record(&self, fact: Fact) -> Option<Recorded> {
        let recorded = self.with(|ledger| {
            let recorded = ledger.record(fact)?;
            if recorded.changed {
                self.publish(ledger)?;
            }
            Ok(recorded)
        });
        match recorded {
            Ok(recorded) => {
                if !recorded.effects_claimed.is_empty() {
                    self.host.claimed(recorded.effects_claimed.clone());
                }
                Some(recorded)
            }
            Err(e) => {
                self.failed(&e);
                None
            }
        }
    }

    fn failed(&self, e: &LedgerError) {
        self.host.log(format!("till-core: the ledger: {e}"));
        let said = format!("Couldn't keep the sales on this device: {e}");
        self.state.send_if_modified(|s| {
            let changed = s.problem.as_deref() != Some(said.as_str());
            s.problem = Some(said.clone());
            changed
        });
    }

    /// Reads the till's state from the ledger again: after another process
    /// wrote it (the iOS extension), or as the app comes to the front.
    pub fn refresh(&self) {
        if let Err(e) = self.with(|ledger| self.publish(ledger)) {
            self.failed(&e);
        }
    }

    /// The sales as the shift shows them, into the state.
    fn publish(&self, ledger: &Ledger) -> Result<(), LedgerError> {
        let window = ledger.shift_window()?;
        let views = ledger.sales()?;
        let mut sales = Vec::new();
        let mut shown = Vec::new();
        for view in views {
            // A sale whose order is still being made has no code to show.
            if view.waiting_for_order {
                continue;
            }
            let sale = TillSale::of_view(&view);
            let in_window = window.contains(UnixSeconds(view.created_at).to_millis());
            // Another counter's cash sale waits for its own till's order,
            // which this till never hears of: it's listed with the shift it
            // was rung up in, not for ever.
            let keep = view.live
                || (sale.offered && sale.kept_as_cash_at.is_none() && !view.for_its_counter)
                || in_window
                || sale.needs_action();
            if keep {
                sales.push(sale);
                shown.push(view);
            }
        }
        sales.truncate(crate::Till::MAX_SALES);
        shown.truncate(crate::Till::MAX_SALES);
        let refund_problems: BTreeMap<String, String> = shown
            .iter()
            .filter_map(|v| Some((v.order_id.clone(), v.refund_problem.clone()?)))
            .collect();
        let from = Some(window.from.to_seconds());
        self.state.send_if_modified(|s| {
            let changed = s.sales != sales
                || s.views != shown
                || s.refund_problems != refund_problems
                || s.shift_from != from;
            if changed {
                s.sales = sales;
                s.views = shown;
                s.refund_problems = refund_problems;
                s.shift_from = from;
            }
            changed
        });
        Ok(())
    }

    /// The process may be suspended: any write under way finishes (the
    /// lock), then the ledger is closed. The next use opens it again.
    pub fn suspend(&self) {
        let mut slot = self.slot();
        if let Some(ledger) = slot.take()
            && let Err(e) = ledger.close()
        {
            self.host.log(format!("till-core: closing the ledger: {e}"));
        }
    }

    /// Whether the ledger is open now.
    pub fn is_open(&self) -> bool {
        self.slot().is_some()
    }

    /// The till's state as it changes.
    pub fn subscribe(&self) -> watch::Receiver<TillState> {
        self.state.subscribe()
    }

    /// The till's state now.
    pub fn state(&self) -> TillState {
        self.state.borrow().clone()
    }

    /// Changes the parts of the state the ledger doesn't hold (what went
    /// wrong, the sign-in).
    pub fn change(&self, f: impl FnOnce(&TillState) -> TillState) {
        self.state.send_if_modified(|s| {
            let new = f(s);
            let changed = new != *s;
            *s = new;
            changed
        });
    }

    /// The sale a screen calls `order_id`: its deciding order's id, one of
    /// an offered cash sale's `cash:` id, or its key.
    pub fn sale_key(&self, order_id: &str) -> Option<SaleKey> {
        let state = self.state.borrow();
        state
            .views
            .iter()
            .find(|v| v.order_id == order_id || v.sale.text() == order_id)
            .map(|v| v.sale.clone())
            .or_else(|| SaleKey::parse(order_id))
    }

    /// One sale's view, read from the ledger.
    pub fn view(&self, sale: &SaleKey) -> Option<SaleView> {
        self.with(|ledger| ledger.sale(sale)).ok().flatten()
    }

    /// One sale's row, read from the ledger.
    pub fn row(&self, sale: &SaleKey) -> Option<SaleRow> {
        self.with(|ledger| ledger.row(sale)).ok().flatten()
    }

    /// The settings views read, from the ledger.
    pub fn view_settings(&self) -> ViewSettings {
        self.with(|ledger| ledger.view_settings())
            .unwrap_or_default()
    }

    /// Claims what's waiting as `policy` says (docs/ledger.md §6).
    pub fn claim_due(&self, policy: ClaimPolicy) -> Vec<Claim> {
        match self.with(|ledger| ledger.claim_due(policy)) {
            Ok(claims) => claims,
            Err(e) => {
                self.host
                    .log(format!("till-core: claiming the ledger's alerts: {e}"));
                Vec::new()
            }
        }
    }

    /// A claimed alert was delivered.
    pub fn posted(&self, claim: &Claim) {
        if let Err(e) = self.with(|ledger| ledger.posted(claim)) {
            self.host
                .log(format!("till-core: noting an alert posted: {e}"));
        }
    }

    /// A look was made: the first in this process ends learning without
    /// alerting; and the app claims what other processes left.
    pub fn looked(&self) {
        if !self.looked.swap(true, Ordering::SeqCst)
            && let Err(e) = self.with(|ledger| ledger.set_learning(false))
        {
            self.host.log(format!("till-core: ending learning: {e}"));
        }
        if let Some(policy) = self.claims {
            let claims = self.claim_due(policy);
            if !claims.is_empty() {
                // Another process wrote these sales: show them as they are.
                self.refresh();
                self.host.claimed(claims);
            }
        }
    }

    /// The sale the cashier has open on screen (it gets the paid moment even
    /// when it's another counter's).
    pub fn set_open_sale(&self, sale: Option<SaleKey>) {
        {
            let mut open = self.open_sale.lock().unwrap_or_else(|e| e.into_inner());
            if *open == sale {
                return;
            }
            open.clone_from(&sale);
        }
        if let Some(ledger) = self.slot().as_mut() {
            ledger.set_open_sale(sale);
        }
    }

    /// Keeps `options` as the ledger's settings (`Fact::SettingChanged`),
    /// those that changed only.
    pub fn keep_options(&self, options: &TillOptions, at_ms: i64) {
        let wanted = [
            (
                setting::DEVICE,
                options.device_id.as_ref().map(|d| d.to_string()),
            ),
            (
                setting::EVERY_DEVICE,
                Some(options.every_device.to_string()),
            ),
            (
                setting::CASH_AS_MONERO,
                Some(options.cash_as_monero.to_string()),
            ),
        ];
        self.keep_settings(&wanted, at_ms);
    }

    /// Keeps each `(name, value)` as a setting, recording only those that
    /// differ from the ledger's.
    pub fn keep_settings(&self, wanted: &[(&str, Option<String>)], at_ms: i64) {
        let kept = match self.with(|ledger| ledger.settings()) {
            Ok(kept) => kept,
            Err(e) => {
                self.failed(&e);
                return;
            }
        };
        let mut changed = false;
        for (name, value) in wanted {
            if kept.get(*name) == value.as_ref() {
                continue;
            }
            changed = true;
            self.record(Fact::SettingChanged {
                name: (*name).to_owned(),
                value: value.clone(),
                at: at_ms,
            });
        }
        if changed {
            self.refresh();
        }
    }

    /// The scope changed (signed out, the store forgotten, another point of
    /// sale): the old scope's rows are deleted and the ledger reads the new
    /// one's (docs/ledger.md §4).
    pub fn rescope(&self) {
        let scope = self.host.scope();
        let done = self.with(|ledger| {
            if *ledger.scope() == scope {
                return Ok(false);
            }
            let old = ledger.scope().clone();
            ledger.delete_scope(&old)?;
            ledger.set_scope(scope);
            self.publish(ledger)?;
            Ok(true)
        });
        if let Err(e) = done {
            self.failed(&e);
        }
    }
}

/// The ledger's clock: the till's own, which [`Books::set_clock`] changes
/// for both.
#[derive(Clone)]
struct TillClock(Arc<RwLock<Arc<dyn monokulo_client::Clock>>>);

impl monokulo_client::Clock for TillClock {
    fn now_ms(&self) -> UnixMillis {
        self.0.read().unwrap_or_else(|e| e.into_inner()).now_ms()
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! A ledger in a test's directory, with a fixed key.

    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use till_ledger::{Claim, KeyMaterial, Ledger, LedgerError, Opened, Process, Scope};

    use super::LedgerHost;

    /// Opens `till.db` in a directory, as the app (or `process`) does,
    /// keeping what it was handed to deliver.
    pub(crate) struct TestHost {
        dir: PathBuf,
        process: Process,
        pub claimed: Mutex<Vec<Claim>>,
    }

    impl TestHost {
        pub(crate) fn new(dir: &Path) -> Arc<TestHost> {
            TestHost::of(dir, Process::App)
        }

        pub(crate) fn of(dir: &Path, process: Process) -> Arc<TestHost> {
            Arc::new(TestHost {
                dir: dir.to_path_buf(),
                process,
                claimed: Mutex::new(Vec::new()),
            })
        }
    }

    impl LedgerHost for TestHost {
        fn scope(&self) -> Scope {
            Scope::of("test", None)
        }

        fn open(&self, clock: Arc<dyn till_ledger::Clock>) -> Result<Opened, LedgerError> {
            let key = KeyMaterial::from_secret(&[7u8; 48]).expect("48 bytes");
            let path = self.dir.join("till.db");
            let created = !path.exists();
            let ledger = Ledger::open(&path, &key, self.scope(), self.process, clock)?;
            Ok(Opened {
                ledger,
                created,
                recovered: false,
            })
        }

        fn claimed(&self, claims: Vec<Claim>) {
            self.claimed.lock().unwrap().extend(claims);
        }
    }
}
