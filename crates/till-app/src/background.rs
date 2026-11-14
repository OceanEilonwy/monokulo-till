//! One look for new sales with no engine (docs/ios-plan.md §2, §5): the iOS
//! Notification Service Extension's entry when a ding arrives as an alert,
//! and Android's when a ding wakes a process with no shift on screen.
//!
//! It reads the persisted model, asks the point of sale for new sales once
//! and reads Monokulo's open orders again, recording each as a fact in the
//! ledger the app shares (docs/ledger.md §8), makes the orders that are this
//! till's to make, then claims the newest new-sale alert the round left and
//! answers what it should say (§6: the extension claims only what it shows;
//! the app's next look claims the rest). The ledger is closed before it
//! returns (§11). Nothing runs after it returns: no engine, no listener, no
//! event stream. The sign-in's refresh takes the same file lock as the
//! app's (decision 13), so the two never spend one refresh token twice.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use monokulo_client::{Clock, PointOfSale, SystemClock, tor};
use till_ledger::{ClaimPolicy, Process};
use till_pos::{FoundSale, ListedRefund, PosError, Refunds, Rfc3339Instant, SaleOrder, SaleSource};
use till_sales::{Till, TillSale};
use till_screens::notification::{self, NotificationInput, OngoingFace, SaleAlert};
use till_screens::{Device, pos_text};

use crate::phases::Money;
use crate::side::Side;
use crate::store::{Kept, Log, Store};
use crate::{EngineConfig, Platform};

/// What a background look found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct BackgroundCheck {
    /// iOS: one alert per sale this look made an order for whose code is
    /// up, newest first, as `till_screens::notification` words it: what the
    /// notification should say in place of the ding's own words.
    pub alerts: Vec<SaleAlert>,
    /// Android: the shift's ongoing notification after the look, alerting
    /// about the newest such sale; `None` when the look found none.
    pub ongoing: Option<OngoingFace>,
    /// What went wrong, in the till's words: the kept state couldn't be
    /// read, or the point of sale or Monokulo refused.
    pub problem: Option<String>,
    /// False when the deadline came first. Orders made by then are kept;
    /// a sale whose order wasn't made is found again by the next look, and
    /// gets the same order (its idempotency key).
    pub finished: bool,
    /// How long each step took, in the order they ended, for the
    /// extension's log (docs/decisions/core.md 203): where a slow alert's
    /// time went.
    pub steps: Vec<LookStep>,
}

/// One step of a look and how long it took. The steps:
/// - `state`: the kept state read (the Keychain);
/// - `ding`: the relay's ciphertext opened ([`background_check_ding`]);
/// - `ledger.open`: the ledger opened and the shift read from it;
/// - `pos.payments`, `pos.order`, `pos.refunds`: each request to the point
///   of sale (a sign-in's refresh, when due, is inside the first);
/// - `monokulo.create`: each Monokulo order made;
/// - `monokulo.reread`: the shift's open orders read again, together;
/// - `round`: the point of sale's look and the re-reads, which overlap;
/// - `ledger.close`: the alert claimed, the shift read again, the ledger
///   closed;
/// - `total`: from the call to its answer.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LookStep {
    pub step: String,
    pub ms: u64,
}

/// The steps a look has timed so far, shared with what it hands them to.
#[derive(Clone, Default)]
struct Steps(Arc<Mutex<Vec<LookStep>>>);

impl Steps {
    fn add(&self, step: &str, took: Duration) {
        let ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX);
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(LookStep {
                step: step.to_owned(),
                ms,
            });
    }

    fn time<R>(&self, step: &str, f: impl FnOnce() -> R) -> R {
        let started = Instant::now();
        let r = f();
        self.add(step, started.elapsed());
        r
    }

    async fn time_async<F: Future>(&self, step: &str, f: F) -> F::Output {
        let started = Instant::now();
        let r = f.await;
        self.add(step, started.elapsed());
        r
    }

    fn taken(&self) -> Vec<LookStep> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// The look's point of sale, each request timed.
struct TimedSource {
    inner: Arc<dyn SaleSource>,
    steps: Steps,
}

#[async_trait]
impl SaleSource for TimedSource {
    fn point_of_sale(&self) -> PointOfSale {
        self.inner.point_of_sale()
    }

    async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError> {
        self.steps
            .time_async("pos.payments", self.inner.new_sales())
            .await
    }

    async fn order(&self, sale: &FoundSale) -> Result<SaleOrder, PosError> {
        self.steps
            .time_async("pos.order", self.inner.order(sale))
            .await
    }

    fn refunds(&self) -> Refunds<'_> {
        self.inner.refunds()
    }

    async fn refunds_since(
        &self,
        begin_time: &Rfc3339Instant,
    ) -> Result<Option<Vec<ListedRefund>>, PosError> {
        self.steps
            .time_async("pos.refunds", self.inner.refunds_since(begin_time))
            .await
    }

    fn handled(&self, sale: &FoundSale) {
        self.inner.handled(sale)
    }
}

/// How many of the shift's open orders are read again at once.
const REREADS_AT_ONCE: usize = 4;

impl BackgroundCheck {
    fn nothing() -> BackgroundCheck {
        BackgroundCheck {
            finished: true,
            ..BackgroundCheck::default()
        }
    }

    fn failed(problem: impl Into<String>) -> BackgroundCheck {
        BackgroundCheck {
            problem: Some(problem.into()),
            finished: true,
            ..BackgroundCheck::default()
        }
    }

    fn timed(self, steps: &Steps, started: Instant) -> BackgroundCheck {
        steps.add("total", started.elapsed());
        BackgroundCheck {
            steps: steps.taken(),
            ..self
        }
    }
}

/// Looks for new sales once and makes their orders, within `deadline_ms`
/// milliseconds from now; blocks the calling thread until then at most.
/// Nothing is looked for without a connection, a shift on (a ding outside
/// one goes nowhere, decision 93 of android.md) or a point of sale signed
/// in. Must not be called on a thread running a Tokio runtime.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn background_check(
    config: EngineConfig,
    platform: Arc<dyn Platform>,
    deadline_ms: u64,
) -> BackgroundCheck {
    look(config, platform, None, deadline_ms).unwrap_or_else(BackgroundCheck::nothing)
}

/// [`is_relayed_ding`] and then, for a ding, [`background_check`], reading
/// the kept state once for both (the Keychain, read item by item, is a
/// look's slowest local step, docs/decisions/core.md 205): `None` when
/// `content` isn't a ding for this till, and nothing is looked for. What
/// iOS's Notification Service Extension calls.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn background_check_ding(
    config: EngineConfig,
    platform: Arc<dyn Platform>,
    content: Vec<u8>,
    deadline_ms: u64,
) -> Option<BackgroundCheck> {
    look(config, platform, Some(content), deadline_ms)
}

fn look(
    config: EngineConfig,
    platform: Arc<dyn Platform>,
    ding: Option<Vec<u8>>,
    deadline_ms: u64,
) -> Option<BackgroundCheck> {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            return Some(BackgroundCheck::failed(format!(
                "Couldn't start the core: {e}"
            )));
        }
    };
    let http = match tor::http_client(tor::SOCKS) {
        Ok(http) => http,
        Err(e) => {
            return Some(BackgroundCheck::failed(format!(
                "Couldn't start the core: {e}"
            )));
        }
    };
    runtime.block_on(check_ding(
        config,
        platform,
        ding,
        http,
        Arc::new(SystemClock),
        Duration::from_millis(deadline_ms),
    ))
}

/// Whether `content`, the `aes128gcm` ciphertext an APNs alert from the
/// relay carries (beside `aps`, base64url, under `aes128gcm`), is a ding
/// for this till: it opens with the Web Push keys the engine keeps and
/// reads as `{"wake":"<the till's point of sale>"}` (till-bell's
/// `is_ding`). The iOS Notification Service Extension asks this before
/// [`background_check`]. Reads the kept state; no network.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn is_relayed_ding(platform: Arc<dyn Platform>, content: Vec<u8>) -> bool {
    let Ok(model) = Store::new(platform.clone()).load() else {
        return false;
    };
    let Some(plain) = crate::web_push::open(platform, &content) else {
        return false;
    };
    till_bell::is_ding(&plain, true, model.point_of_sale)
}

/// [`background_check`] on the runtime it is awaited on.
#[cfg(test)]
pub(crate) async fn check(
    config: EngineConfig,
    platform: Arc<dyn Platform>,
    http: reqwest::Client,
    clock: Arc<dyn Clock>,
    budget: Duration,
) -> BackgroundCheck {
    check_ding(config, platform, None, http, clock, budget)
        .await
        .unwrap_or_else(BackgroundCheck::nothing)
}

/// [`background_check_ding`] on the runtime it is awaited on; with no
/// `ding`, [`background_check`] (never `None`).
pub(crate) async fn check_ding(
    config: EngineConfig,
    platform: Arc<dyn Platform>,
    ding: Option<Vec<u8>>,
    http: reqwest::Client,
    clock: Arc<dyn Clock>,
    budget: Duration,
) -> Option<BackgroundCheck> {
    let started = Instant::now();
    let steps = Steps::default();
    let deadline = tokio::time::Instant::now() + budget;
    // No listener to log to: what goes wrong is in the answer.
    let log: Log = Arc::new(|_| {});
    let kept = match steps.time("state", || Kept::load(Store::new(platform.clone()), log)) {
        Ok(kept) => Arc::new(kept),
        // Not knowing the keys, nobody can say it's a ding: as before, a
        // ding that can't be opened keeps the relay's words.
        Err(_) if ding.is_some() => return None,
        Err(e) => {
            return Some(
                BackgroundCheck::failed(format!("Couldn't read what this till keeps: {e}"))
                    .timed(&steps, started),
            );
        }
    };
    let model = kept.model();
    if let Some(content) = ding {
        let is_ding = steps.time("ding", || {
            crate::web_push::open(platform.clone(), &content)
                .is_some_and(|plain| till_bell::is_ding(&plain, true, model.point_of_sale))
        });
        if !is_ding {
            return None;
        }
    }
    Some(
        look_kept(config, platform, kept, http, clock, deadline, &steps)
            .await
            .timed(&steps, started),
    )
}

async fn look_kept(
    config: EngineConfig,
    platform: Arc<dyn Platform>,
    kept: Arc<Kept>,
    http: reqwest::Client,
    clock: Arc<dyn Clock>,
    deadline: tokio::time::Instant,
    steps: &Steps,
) -> BackgroundCheck {
    let model = kept.model();
    // A till on the shop's own wallet looks only while it's open in the
    // foreground (docs/view-key-mode.md §10): never here.
    let Some(link) = model.link().filter(|link| link.monokulo().is_some()) else {
        return BackgroundCheck::nothing();
    };
    if !model.shift_on {
        return BackgroundCheck::nothing();
    }
    let device = config.device;
    let side = Side::new(
        Arc::new(config),
        kept,
        platform.clone(),
        Process::Extension,
        http.clone(),
        clock.clone(),
    );
    let Some(source) = side.sale_source() else {
        return BackgroundCheck::nothing();
    };
    let source = Arc::new(TimedSource {
        inner: source,
        steps: steps.clone(),
    });
    let books = side.books().clone();
    let Some(backend) = side.backend_reading(&link, REREADS_AT_ONCE) else {
        return BackgroundCheck::nothing();
    };
    let timer_steps = steps.clone();
    let till = Till::new(source, backend.clone(), books.clone())
        .with_clock(clock)
        .with_timer(Arc::new(move |step, took| timer_steps.add(step, took)))
        .with_options(&model.till_options());
    let opened = steps.time("ledger.open", || {
        books.refresh();
        // Woken by a ding for a sale: what this round finds is new, even on
        // a ledger just made or imported, as the extension's look always
        // alerted (docs/decisions/core.md 155).
        books.with(|l| l.set_learning(false))
    });
    if let Err(e) = opened {
        return BackgroundCheck::failed(format!("Couldn't read the sales this till keeps: {e}"));
    }
    // The open orders, refreshed (docs/ledger.md §8): those the shift had
    // before this look, read while the point of sale is asked, a few at
    // once (decision 204). An order this look makes is known from the
    // backend's answer to making it.
    let refs = till.open_refs();
    let round = async {
        let reread = async {
            if refs.is_empty() {
                return Vec::new();
            }
            backend.refresh(&refs).await.unwrap_or_default()
        };
        let (_, payments) = tokio::join!(
            till.check_at_once(),
            steps.time_async("monokulo.reread", reread)
        );
        for payment in &payments {
            till.on_payment(payment);
        }
    };
    let finished = tokio::time::timeout_at(deadline, steps.time_async("round", round))
        .await
        .is_ok();
    let (claims, state) = steps.time("ledger.close", || {
        // The one alert this round shows: the newest new sale it left due.
        // The app (or the next push) claims the rest.
        let claims = books.claim_due(ClaimPolicy::NewestNewSale);
        // Another process may have written what this round found first.
        books.refresh();
        let state = till.state();
        for claim in &claims {
            books.posted(claim);
        }
        books.suspend();
        (claims, state)
    });
    let new: Vec<TillSale> = claims
        .iter()
        .map(|claim| TillSale::of_view(&claim.view))
        .collect();
    let money = Money(platform);
    // What the notifications say about `sale`, as it arrives.
    let arriving = |sale: &TillSale| {
        let input = NotificationInput {
            sales: state.sales.clone(),
            just_paid: None,
            alert_for: Some(sale.order_id.clone()),
            // Not read here: the ongoing notification names the place alone.
            store: None,
            location: pos_text::place_of(sale, model.point_of_sale, model.place_name().as_deref())
                .map(str::to_owned),
        };
        notification::notification(&input, device, &money)
    };
    // A keypad till shows nothing: the cashier made each sale on it
    // (core.md decision 260).
    let keypad = model.point_of_sale == Some(monokulo_client::PointOfSale::Till);
    let (alerts, ongoing) = if keypad {
        (Vec::new(), None)
    } else if device == Device::Android {
        let ongoing = new.first().and_then(|sale| arriving(sale).ongoing);
        (Vec::new(), ongoing)
    } else {
        let alerts = new.iter().filter_map(|sale| arriving(sale).alert).collect();
        (alerts, None)
    };
    BackgroundCheck {
        alerts,
        ongoing,
        problem: state.problem.clone(),
        finished,
        steps: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
