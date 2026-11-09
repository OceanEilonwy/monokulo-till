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

use std::sync::Arc;
use std::time::Duration;

use monokulo_client::{Clock, MonokuloApi, SystemClock, tor};
use till_ledger::{ClaimPolicy, Process};
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
}

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
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => return BackgroundCheck::failed(format!("Couldn't start the core: {e}")),
    };
    let http = match tor::http_client(tor::SOCKS) {
        Ok(http) => http,
        Err(e) => return BackgroundCheck::failed(format!("Couldn't start the core: {e}")),
    };
    runtime.block_on(check(
        config,
        platform,
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
pub(crate) async fn check(
    config: EngineConfig,
    platform: Arc<dyn Platform>,
    http: reqwest::Client,
    clock: Arc<dyn Clock>,
    budget: Duration,
) -> BackgroundCheck {
    let deadline = tokio::time::Instant::now() + budget;
    // No listener to log to: what goes wrong is in the answer.
    let log: Log = Arc::new(|_| {});
    let kept = match Kept::load(Store::new(platform.clone()), log) {
        Ok(kept) => Arc::new(kept),
        Err(e) => {
            return BackgroundCheck::failed(format!("Couldn't read what this till keeps: {e}"));
        }
    };
    let model = kept.model();
    let Some(connection) = model.connection.clone() else {
        return BackgroundCheck::nothing();
    };
    if !model.shift_on {
        return BackgroundCheck::nothing();
    }
    let device = config.device;
    let side = match Side::new(
        Arc::new(config),
        kept,
        platform.clone(),
        Process::Extension,
        http.clone(),
        clock.clone(),
    ) {
        Ok(side) => side,
        Err(problem) => return BackgroundCheck::failed(problem),
    };
    let Some(source) = side.sale_source() else {
        return BackgroundCheck::nothing();
    };
    let books = side.books().clone();
    let monokulo = MonokuloApi::new(http, side.identity(), clock.clone());
    let till = Till::new(source, monokulo.clone(), connection.clone(), books.clone())
        .with_clock(clock)
        .with_options(&model.till_options());
    books.refresh();
    // Woken by a ding for a sale: what this round finds is new, even on a
    // ledger just made or imported, as the extension's look always alerted
    // (docs/decisions/core.md 155). The app's first look still learns.
    if let Err(e) = books.with(|l| l.set_learning(false)) {
        return BackgroundCheck::failed(format!("Couldn't read the sales this till keeps: {e}"));
    }
    let round = async {
        till.check().await;
        // Monokulo's open orders, read again (docs/ledger.md §8).
        let urls: Vec<String> = till
            .state()
            .sales
            .iter()
            .filter(|sale| sale.open())
            .filter_map(|sale| sale.url.clone())
            .collect();
        for url in urls {
            if let Ok(order) = monokulo.order(&connection, &url).await {
                till.on_order(&order);
            }
        }
    };
    let finished = tokio::time::timeout_at(deadline, round).await.is_ok();
    // The one alert this round shows: the newest new sale it left due. The
    // app (or the next push) claims the rest.
    let claims = books.claim_due(ClaimPolicy::NewestNewSale);
    // Another process may have written what this round found first.
    books.refresh();
    let state = till.state();
    let new: Vec<TillSale> = claims
        .iter()
        .map(|claim| TillSale::of_view(&claim.view))
        .collect();
    for claim in &claims {
        books.posted(claim);
    }
    books.suspend();
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
    let (alerts, ongoing) = if device == Device::Android {
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
    }
}

#[cfg(test)]
mod tests;
