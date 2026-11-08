//! One look for new sales with no engine (docs/ios-plan.md §2, §5): the iOS
//! Notification Service Extension's entry when a ding arrives as an alert,
//! and Android's when a ding wakes a process with no shift on screen.
//!
//! It reads the persisted model, asks the point of sale for new sales once,
//! makes their Monokulo orders, keeps the shift and the cursor, and answers
//! what the alert should say. Nothing runs after it returns: no engine, no
//! listener, no event stream. The sign-in's refresh takes the same file
//! lock as the app's (decision 13), so the two never spend one refresh
//! token twice.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use monokulo_client::{Clock, MonokuloApi, SystemClock, tor};
use till_sales::Till;

use crate::faces::{self, Money, NotificationFace};
use crate::side::Side;
use crate::store::{Kept, Log, Store};
use crate::{EngineConfig, Platform};

/// What a background look found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct BackgroundCheck {
    /// One alert per sale this look made an order for whose code is up,
    /// newest first: what the notification should say in place of the
    /// ding's own words.
    pub alerts: Vec<NotificationFace>,
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
    let side = match Side::new(Arc::new(config), kept, http.clone(), clock.clone()) {
        Ok(side) => side,
        Err(problem) => return BackgroundCheck::failed(problem),
    };
    let Some(source) = side.sale_source() else {
        return BackgroundCheck::nothing();
    };
    let monokulo = MonokuloApi::new(http, side.identity(), clock.clone());
    let till = Till::new(source, monokulo, connection)
        .with_clock(clock)
        .with_store(side.shift_kept().clone());
    if till.shift_unread() {
        // A look now would save its orders over the shift it couldn't read.
        return BackgroundCheck::failed("Couldn't read the shift this till keeps.");
    }
    let before: HashSet<String> = till
        .state()
        .sales
        .into_iter()
        .map(|sale| sale.order_id)
        .collect();
    let finished = tokio::time::timeout_at(deadline, till.check())
        .await
        .is_ok();
    let state = till.state();
    let money = Money(platform);
    BackgroundCheck {
        alerts: state
            .sales
            .iter()
            .filter(|sale| !before.contains(&sale.order_id) && sale.live())
            .map(|sale| faces::new_sale(sale, &money, false))
            .collect(),
        problem: state.problem,
        finished,
    }
}

#[cfg(test)]
mod tests;
