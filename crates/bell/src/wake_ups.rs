//! Keeping the till registered with the chosen Bell (Android:
//! `push/BellWakeUps.kt`).

use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use monokulo_client::{Clock, PointOfSale};
use tokio::runtime::Handle;
use tokio::sync::{Mutex, broadcast, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{
    BellAddress, BellClient, BellError, BellProblem, BellRegistration, DeviceError, PosSignIn,
    PushFailure, PushState, PushSubscription, PushSubscriptions, WakeUpChoice, WakeUpStatus,
    WakeUpStore, pos_words,
};

/// Which distributor would carry the dings (decision 91).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Distributor {
    /// The one to use, by its name (an Android package name).
    Found(String),
    /// Nothing on the device can carry dings.
    None,
    /// Several installed and no default: the cashier picks one of these.
    Choose(Vec<String>),
}

/// The distributors on the device, as [`BellWakeUps`] needs them.
///
/// On Android these are UnifiedPush's distributors. On iOS there is one,
/// always found: APNs, through the relay; [`Distributors::changes`] never
/// sends there.
pub trait Distributors: Send + Sync {
    /// The saved distributor while it is installed, else the default (then
    /// saved), else what there is to choose from.
    fn resolve(&self) -> Result<Distributor, DeviceError>;

    /// The cashier's pick from [`Distributor::Choose`].
    fn save(&self, package_name: &str);

    /// Sends when an app came or went, which may have been a distributor.
    fn changes(&self) -> broadcast::Receiver<()>;
}

/// The till's point-of-sale sign-in, as registering with a Bell needs it
/// (decision 132).
#[async_trait]
pub trait PosSide: Send + Sync {
    /// Where the till is signed in; `None` while it isn't signed in to a
    /// point of sale (or runs without one).
    fn signed_in(&self) -> Result<Option<PosSignIn>, DeviceError>;

    /// An access token for [`BellClient::register`], refreshed if it needs
    /// to be; `None` when the point of sale wants the cashier to sign in
    /// again. It is shown to the Bell once, to prove the location.
    async fn access_token(&self) -> Result<Option<String>, DeviceError>;
}

/// A timer-found sale with no ding this long before it is a missed wake-up
/// (decision 94).
pub const MISSED_AFTER: Duration = Duration::from_secs(60);
/// The first wait before an unreachable Bell is tried again; it doubles
/// each time, to [`LAST_RETRY`].
pub const FIRST_RETRY: Duration = Duration::from_secs(60);
pub const LAST_RETRY: Duration = Duration::from_secs(15 * 60);
/// How long taking a registration back may hold anything up.
pub const UNREGISTER_WAIT: Duration = Duration::from_secs(10);
/// How long the platform gets to answer with an endpoint (a distributor may
/// have to reach its server).
pub const SUBSCRIBE_WAIT: Duration = Duration::from_secs(30);

/// The distributor refused the subscription.
pub const PUSH_REFUSED: &str = "the UnifiedPush app refused";
/// The distributor's endpoint came without Web Push keys.
pub const PUSH_NO_KEYS: &str = "the UnifiedPush app gave no keys";
/// A distributor just installed answers nothing until it has been opened
/// once (decision 123).
pub const PUSH_SILENT: &str = "the UnifiedPush app didn't answer: open it once";
/// The distributor couldn't reach its server.
pub const PUSH_OFFLINE: &str = "the UnifiedPush app has no connection";
/// Something on this device failed during a pass (decision 120).
pub const TILL_FAILED: &str = "something on this device failed";

/// Wake-ups over a Bell (docs/design.md §3; decisions 91, 94, 95, 97):
/// keeps the till registered with the Bell the [`WakeUpStore`]'s choice
/// names, and nothing registered when it names none. One pass at a time
/// brings what the Bell holds into line with the choice;
/// [`BellWakeUps::status`] says how that went, the shift paces its timer by
/// it, and Settings shows it.
///
/// Its work runs as tasks on the tokio runtime it was made in, timed by
/// tokio's clock (so tests can pause it) and dated by the [`Clock`].
/// Dropping it ends them, except a registration the Bell already has in
/// hand, whose answer is still kept.
pub struct BellWakeUps {
    inner: Arc<Inner>,
}

impl BellWakeUps {
    /// Wake-ups as they were left: an app start, which registers again (or
    /// finishes taking a registration back). `monokular` is the project's
    /// Bell, what [`WakeUpChoice::MonokularBell`] means.
    ///
    /// # Panics
    ///
    /// Outside a tokio runtime.
    pub fn new(
        push: Arc<dyn PushSubscriptions>,
        distributors: Arc<dyn Distributors>,
        bells: Arc<dyn BellClient>,
        store: Arc<dyn WakeUpStore>,
        monokular: BellAddress,
        pos: Arc<dyn PosSide>,
        clock: Arc<dyn Clock>,
    ) -> BellWakeUps {
        let choice = store.choice();
        let status = if choice == WakeUpChoice::TimerOnly {
            WakeUpStatus::Off
        } else {
            WakeUpStatus::Connecting
        };
        let inner = Arc::new(Inner {
            push,
            distributors,
            bells,
            store,
            monokular,
            pos,
            clock,
            runtime: Handle::current(),
            process: CancellationToken::new(),
            choice: watch::Sender::new(choice.clone()),
            status: watch::Sender::new(status.clone()),
            lock: Arc::new(Mutex::new(())),
            jobs: StdMutex::default(),
            mem: StdMutex::default(),
            #[cfg(test)]
            history: StdMutex::new(vec![status]),
        });
        inner.listen();
        // An app start: register again, or finish taking a registration back.
        if choice != WakeUpChoice::TimerOnly || inner.store.registration().is_some() {
            inner.run(false);
        }
        BellWakeUps { inner }
    }

    /// The cashier's choice, now and as it changes.
    pub fn choice(&self) -> watch::Receiver<WakeUpChoice> {
        self.inner.choice.subscribe()
    }

    /// Where the chosen Bell stands, now and as it changes.
    pub fn status(&self) -> watch::Receiver<WakeUpStatus> {
        self.inner.status.subscribe()
    }

    /// The cashier chose (Settings): saved at once, then the old
    /// registration is taken back and the new Bell registered with, as
    /// decision 95 orders it. Choosing what is already chosen does nothing,
    /// unless the status is [`WakeUpStatus::Refused`] or
    /// [`WakeUpStatus::Unreachable`], which tries again.
    pub fn choose(&self, choice: WakeUpChoice) {
        let inner = &self.inner;
        let retrying = matches!(
            *inner.status.borrow(),
            WakeUpStatus::Refused { .. } | WakeUpStatus::Unreachable { .. }
        );
        if choice == inner.store.choice() && !retrying {
            return;
        }
        inner.store.set_choice(choice.clone());
        if let WakeUpChoice::OwnBell { bell } = &choice {
            inner.store.set_own_bell(Some(bell.clone()));
        }
        let off = choice == WakeUpChoice::TimerOnly;
        inner.set_choice(choice);
        inner.set_status(if off {
            WakeUpStatus::Off
        } else {
            WakeUpStatus::Connecting
        });
        inner.mem().waits = 0;
        inner.run(true);
    }

    /// The cashier picked a distributor from
    /// [`WakeUpStatus::ChooseDistributor`]: saved, then registration goes
    /// on.
    pub fn choose_distributor(&self, package_name: &str) {
        let inner = &self.inner;
        inner.distributors.save(package_name);
        if inner.store.choice() == WakeUpChoice::TimerOnly {
            return;
        }
        inner.set_status(WakeUpStatus::Connecting);
        inner.run(true);
    }

    /// Register again (`POST /v1/register` with the current subscription;
    /// the Bell keeps the id): an app start, a shift starting, a new
    /// point-of-sale sign-in, a changed location, or a status to retry.
    /// Nothing happens for [`WakeUpChoice::TimerOnly`] or while a
    /// registration is under way.
    pub fn recheck(&self) {
        if self.inner.store.choice() == WakeUpChoice::TimerOnly {
            return;
        }
        self.inner.run(false);
    }

    /// A check the shift's *timer* started (not a ding, not the app
    /// opening) found `sales` new sales at the point of sale. With a Bell
    /// connected and no ding in the 60 s before, that is a wake-up the Bell
    /// missed (decision 94): the status becomes
    /// [`WakeUpStatus::MissingWakeups`] and the till registers once again.
    pub fn timer_found(&self, sales: u32) {
        let inner = &self.inner;
        if sales == 0 {
            return;
        }
        let WakeUpStatus::Connected { last_ding_at } = *inner.status.borrow() else {
            return;
        };
        // A sale the Bell rang for, or one from before the till was connected, isn't a missed wake-up.
        let heard = {
            let mem = inner.mem();
            mem.last_ding_at.max(mem.connected_at)
        };
        let Some(heard) = heard else { return };
        if inner.clock.now() - heard <= MISSED_AFTER.as_secs() as i64 {
            return;
        }
        inner.set_status(WakeUpStatus::MissingWakeups { last_ding_at });
        inner.run(false);
    }

    /// Take the registration back (`DELETE`, awaited, 10 s at most) and end
    /// the subscription: the cashier signed out of the point of sale or
    /// changed it, or the device was disconnected. The choice stays (status
    /// [`WakeUpStatus::NoPos`]) unless `forget`, which puts it back to
    /// Timer only.
    pub async fn unregister(&self, forget: bool) {
        let inner = &self.inner;
        let retry = inner.jobs().retry.take();
        if let Some(retry) = retry {
            retry.cancel();
        }
        inner.cancel_and_join_pass().await;
        inner.mem().departed = None;
        let _pass = inner.lock.lock().await;
        if let Some(registration) = inner.store.registration() {
            inner.take_back(registration).await;
        }
        inner.unsubscribe();
        if forget {
            inner.store.set_choice(WakeUpChoice::TimerOnly);
        }
        let choice = inner.store.choice();
        let off = choice == WakeUpChoice::TimerOnly;
        inner.set_choice(choice);
        inner.set_status(if off {
            WakeUpStatus::Off
        } else {
            WakeUpStatus::NoPos
        });
    }

    /// Every status this has had, in order, each change once.
    #[cfg(test)]
    pub(crate) fn history(&self) -> Vec<WakeUpStatus> {
        self.inner.history.lock().unwrap().clone()
    }

    /// Whether it is still listening to the platform.
    #[cfg(test)]
    pub(crate) fn is_running(&self) -> bool {
        let jobs = self.inner.jobs();
        !self.inner.process.is_cancelled() && jobs.listeners.iter().all(|l| !l.is_finished())
    }
}

impl Drop for BellWakeUps {
    fn drop(&mut self) {
        self.inner.process.cancel();
    }
}

/// Why the Bell couldn't be reached, in the cashier's words (A1.7).
pub fn unreachable_reason(e: &BellError) -> String {
    match e {
        BellError::NotABell => "it isn't a Bell".into(),
        BellError::Problem(problem) => format!("it answered {}", problem.status),
        BellError::Timeout => "it didn't answer".into(),
        BellError::Connection(_) => "no connection".into(),
        BellError::Device(_) => TILL_FAILED.into(),
    }
}

/// What a Bell's problem means for the till (decision 95, A1.7), signed in
/// to `point_of_sale` (`None` before a sign-in): the Bell names its slugs by
/// point of sale, `square-refused`, `clover-unavailable` (B46). `now` is
/// Unix seconds.
pub fn problem_status(
    problem: &BellProblem,
    point_of_sale: Option<PointOfSale>,
    now: i64,
) -> WakeUpStatus {
    let Some(slug) = problem.slug.as_deref() else {
        return status_only(problem);
    };
    let refused = point_of_sale.map(|pos| format!("{}-refused", pos.wire()));
    let unavailable = point_of_sale.map(|pos| format!("{}-unavailable", pos.wire()));
    match (point_of_sale, slug) {
        (Some(pos), _) if refused.as_deref() == Some(slug) => {
            WakeUpStatus::refused(pos_words::refused(pos))
        }
        (_, "too-many-registrations") => WakeUpStatus::refused(format!(
            "this {} already has 32 tills registered",
            pos_words::place(point_of_sale)
        )),
        (_, "endpoint-registered") => WakeUpStatus::refused(format!(
            "this device is registered under another {}",
            pos_words::account(point_of_sale)
        )),
        (_, "rate-limited") => {
            let wait = problem
                .retry_after
                .unwrap_or(FIRST_RETRY.as_secs() as i64)
                .max(1);
            WakeUpStatus::Refused {
                reason: format!(
                    "too many registrations from this network: trying again in {}",
                    about(wait)
                ),
                retry_at: Some(now + wait),
            }
        }
        (_, "internal") => {
            WakeUpStatus::unreachable(unreachable_reason(&BellError::Problem(problem.clone())))
        }
        _ if unavailable.as_deref() == Some(slug) => {
            WakeUpStatus::unreachable(unreachable_reason(&BellError::Problem(problem.clone())))
        }
        _ => status_only(problem),
    }
}

/// An error answer with nothing more to go on than its status and detail.
fn status_only(problem: &BellProblem) -> WakeUpStatus {
    if problem.status >= 500 {
        WakeUpStatus::unreachable(unreachable_reason(&BellError::Problem(problem.clone())))
    } else {
        WakeUpStatus::refused(
            problem
                .detail
                .clone()
                .unwrap_or_else(|| format!("it answered {}", problem.status)),
        )
    }
}

/// A wait of `seconds` in words loose enough to stay true while the
/// footnote shows them, since it doesn't count down (decision 124).
pub fn about(seconds: i64) -> String {
    if seconds <= 90 {
        "about a minute".into()
    } else {
        format!("about {} minutes", (seconds + 30) / 60)
    }
}

/// The platform couldn't make the subscription (decision 95).
pub fn push_failed_status(reason: PushFailure) -> WakeUpStatus {
    match reason {
        PushFailure::NoDistributor | PushFailure::Lost => WakeUpStatus::NoDistributor,
        PushFailure::Network => WakeUpStatus::unreachable(PUSH_OFFLINE),
        PushFailure::NoKeys => WakeUpStatus::unreachable(PUSH_NO_KEYS),
        PushFailure::ActionRequired | PushFailure::Other => WakeUpStatus::unreachable(PUSH_REFUSED),
    }
}

struct Inner {
    push: Arc<dyn PushSubscriptions>,
    distributors: Arc<dyn Distributors>,
    bells: Arc<dyn BellClient>,
    store: Arc<dyn WakeUpStore>,
    monokular: BellAddress,
    pos: Arc<dyn PosSide>,
    clock: Arc<dyn Clock>,
    runtime: Handle,
    /// Cancelled when the [`BellWakeUps`] is dropped: every task ends.
    process: CancellationToken,
    choice: watch::Sender<WakeUpChoice>,
    status: watch::Sender<WakeUpStatus>,
    /// Held for each pass and each unregistering: what the Bell holds
    /// changes under it only.
    lock: Arc<Mutex<()>>,
    jobs: StdMutex<Jobs>,
    mem: StdMutex<Mem>,
    #[cfg(test)]
    history: StdMutex<Vec<WakeUpStatus>>,
}

#[derive(Default)]
struct Jobs {
    /// The latest pass. Each pass holds the one before it until that one
    /// has ended.
    pass: Option<Pass>,
    /// The wait before the next pass, while one is set.
    retry: Option<CancellationToken>,
    #[cfg_attr(not(test), allow(dead_code))]
    listeners: Vec<JoinHandle<()>>,
}

struct Pass {
    handle: JoinHandle<()>,
    cancel: CancellationToken,
}

#[derive(Default)]
struct Mem {
    waits: u32,
    /// The key and the distributor this run subscribed with; `None` until
    /// it has (the platform is asked again at every start).
    subscribed_for: Option<String>,
    subscribed_through: Option<String>,
    /// The distributor a change of apps took away, until its
    /// [`PushFailure::Lost`] arrives (decision 118).
    departed: Option<String>,
    /// The subscription the last pass took from the platform, whether or
    /// not the Bell then took it: an endpoint arriving outside a pass is
    /// news only when it isn't this one (decision 123).
    taken: Option<PushSubscription>,
    last_ding_at: Option<i64>,
    connected_at: Option<i64>,
}

/// A task was cancelled at one of its waits.
struct Cancelled;

/// Why a pass's registering stopped short of a status.
enum Stop {
    Cancelled,
    Failed(BellError),
}

impl From<Cancelled> for Stop {
    fn from(_: Cancelled) -> Stop {
        Stop::Cancelled
    }
}

impl From<BellError> for Stop {
    fn from(e: BellError) -> Stop {
        Stop::Failed(e)
    }
}

impl From<DeviceError> for Stop {
    fn from(e: DeviceError) -> Stop {
        Stop::Failed(e.into())
    }
}

/// `future`, unless `cancel` is cancelled first (or already).
async fn or_cancel<F: Future>(
    cancel: &CancellationToken,
    future: F,
) -> Result<F::Output, Cancelled> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(Cancelled),
        output = future => Ok(output),
    }
}

impl Inner {
    fn jobs(&self) -> MutexGuard<'_, Jobs> {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn mem(&self) -> MutexGuard<'_, Mem> {
        self.mem.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn status_now(&self) -> WakeUpStatus {
        self.status.borrow().clone()
    }

    fn set_choice(&self, choice: WakeUpChoice) {
        self.choice.send_if_modified(|held| {
            let changed = *held != choice;
            *held = choice;
            changed
        });
    }

    fn set_status(&self, status: WakeUpStatus) {
        self.update_status(|_| Some(status));
    }

    /// The status `change` makes of the current one, if it makes one.
    fn update_status(&self, change: impl FnOnce(&WakeUpStatus) -> Option<WakeUpStatus>) {
        self.status.send_if_modified(|held| match change(held) {
            Some(status) if status != *held => {
                #[cfg(test)]
                self.history.lock().unwrap().push(status.clone());
                *held = status;
                true
            }
            _ => false,
        });
    }

    /// Listen to the platform: dings, the subscription's changes, and apps
    /// coming and going.
    fn listen(self: &Arc<Self>) {
        let mut dings = self.push.dings();
        let me = Arc::clone(self);
        let on_dings = self.runtime.spawn(async move {
            loop {
                match or_cancel(&me.process, dings.recv()).await {
                    Ok(Ok(())) => me.ding(),
                    // Dings missed while busy: the newest is next.
                    Ok(Err(broadcast::error::RecvError::Lagged(_))) => {}
                    _ => break,
                }
            }
        });
        let mut state = self.push.state();
        let me = Arc::clone(self);
        let on_state = self.runtime.spawn(async move {
            loop {
                let current = state.borrow_and_update().clone();
                me.push_changed(current);
                if !matches!(or_cancel(&me.process, state.changed()).await, Ok(Ok(()))) {
                    break;
                }
            }
        });
        let mut changes = self.distributors.changes();
        let me = Arc::clone(self);
        let on_changes = self.runtime.spawn(async move {
            // Changes missed while busy are one change: a look sees them all.
            while let Ok(Ok(()) | Err(broadcast::error::RecvError::Lagged(_))) =
                or_cancel(&me.process, changes.recv()).await
            {
                me.distributors_changed();
            }
        });
        self.jobs().listeners = vec![on_dings, on_state, on_changes];
    }

    /// Start a pass now: `restart` stops one under way (the choice
    /// changed); otherwise one under way is enough.
    fn run(self: &Arc<Self>, restart: bool) {
        let mut jobs = self.jobs();
        if self.process.is_cancelled() {
            return;
        }
        if let Some(retry) = jobs.retry.take() {
            retry.cancel();
        }
        if let Some(pass) = &jobs.pass
            && !restart
            && !pass.handle.is_finished()
            && !pass.cancel.is_cancelled()
        {
            return;
        }
        let running = jobs.pass.take();
        if let Some(running) = &running {
            running.cancel.cancel();
        }
        let cancel = self.process.child_token();
        let me = Arc::clone(self);
        let token = cancel.clone();
        let handle = self.runtime.spawn(async move {
            if let Some(running) = running
                && or_cancel(&token, running.handle).await.is_err()
            {
                return;
            }
            let Ok(_pass) = or_cancel(&token, Arc::clone(&me.lock).lock_owned()).await else {
                return;
            };
            let _ = me.reconcile(&token).await;
        });
        jobs.pass = Some(Pass { handle, cancel });
    }

    /// Cancel the pass under way, if there is one, and wait for it to end.
    async fn cancel_and_join_pass(&self) {
        let pass = self.jobs().pass.take();
        if let Some(pass) = pass {
            pass.cancel.cancel();
            let _ = pass.handle.await;
        }
    }

    /// A pass: what the Bell holds brought into line with the choice,
    /// ending in a status (decision 95's order).
    async fn reconcile(self: &Arc<Self>, cancel: &CancellationToken) -> Result<(), Cancelled> {
        let bell = self.store.choice().bell(&self.monokular);
        if let Some(registration) = self.store.registration()
            && Some(&registration.bell) != bell.as_ref()
        {
            self.take_back(registration).await;
        }
        let Some(bell) = bell else {
            self.unsubscribe();
            self.settle(WakeUpStatus::Off);
            return Ok(());
        };
        self.update_status(|before| match before {
            WakeUpStatus::Connected { .. } | WakeUpStatus::MissingWakeups { .. } => None,
            _ => Some(WakeUpStatus::Connecting),
        });
        // Read once: a pass registers for the one sign-in it started with.
        let signed_in = match self.pos.signed_in() {
            Ok(signed_in) => signed_in,
            Err(_) => {
                self.settle(WakeUpStatus::unreachable(TILL_FAILED));
                return Ok(());
            }
        };
        let outcome = match self.register(&bell, signed_in.as_ref(), cancel).await {
            Ok(status) => status,
            Err(Stop::Cancelled) => return Err(Cancelled),
            Err(Stop::Failed(BellError::Problem(problem))) => problem_status(
                &problem,
                signed_in.map(|s| s.point_of_sale),
                self.clock.now(),
            ),
            Err(Stop::Failed(e)) => WakeUpStatus::unreachable(unreachable_reason(&e)),
        };
        self.settle(outcome);
        Ok(())
    }

    /// Register with `bell` for `signed_in`, stopping at the first thing in
    /// the way; the status that leaves. The Bell's entry for the till's
    /// point of sale is checked in acceptance T5.2's order: there at all,
    /// then the till's environment among its environments, then that
    /// environment's application (decisions 132, 305).
    async fn register(
        &self,
        bell: &BellAddress,
        signed_in: Option<&PosSignIn>,
        cancel: &CancellationToken,
    ) -> Result<WakeUpStatus, Stop> {
        let info = or_cancel(cancel, self.bells.info(bell)).await??;
        let Some(signed_in) = signed_in else {
            self.release().await;
            return Ok(WakeUpStatus::NoPos);
        };
        let point_of_sale = signed_in.point_of_sale;
        let Some(environments) = info.get(point_of_sale) else {
            self.release().await;
            return Ok(WakeUpStatus::refused(pos_words::not_rung(point_of_sale)));
        };
        // Environments other than the till's, ones this build doesn't know included, are never read.
        let Some(application_id) = environments.get(&signed_in.environment) else {
            self.release().await;
            return Ok(WakeUpStatus::refused(pos_words::not_rung_in(
                point_of_sale,
                &signed_in.environment,
            )));
        };
        if *application_id != signed_in.application_id {
            let application_id = application_id.clone();
            self.release().await;
            return Ok(WakeUpStatus::SignInNeeded {
                point_of_sale,
                application_id,
            });
        }
        let through = match self.distributors.resolve()? {
            Distributor::None => {
                self.release().await;
                self.unsubscribe();
                return Ok(WakeUpStatus::NoDistributor);
            }
            Distributor::Choose(packages) => {
                self.release().await;
                return Ok(WakeUpStatus::ChooseDistributor { packages });
            }
            Distributor::Found(package_name) => package_name,
        };
        // Another distributor than the one subscribed through: that one went, and its end is on its way (decision 122).
        let before = self.mem().subscribed_through.clone();
        let before = before.or_else(|| self.store.registration().map(|r| r.distributor));
        if let Some(before) = before
            && before != through
        {
            self.mem().departed = Some(before);
        }
        let subscription = match self
            .subscription(&info.vapid_public_key, &through, cancel)
            .await?
        {
            PushState::Subscribed(subscription) => subscription,
            PushState::Failed(reason) => return Ok(push_failed_status(reason)),
            PushState::None | PushState::Subscribing => {
                return Ok(WakeUpStatus::unreachable(PUSH_SILENT));
            }
        };
        // The Bell keys a registration by its endpoint: a new one is a new registration (decision 95). So is one
        // made for another point of sale or environment, or before registrations named theirs (decision 306).
        if let Some(registration) = self.store.registration()
            && (registration.subscription.endpoint != subscription.endpoint
                || !registration.is_for(signed_in))
        {
            self.take_back(registration).await;
        }
        let Some(token) = or_cancel(cancel, self.pos.access_token()).await?? else {
            self.release().await;
            return Ok(WakeUpStatus::NoPos);
        };
        // Once the Bell has the request, its answer is kept whatever happens to the pass: not a wait to cancel at.
        let answer = self
            .bells
            .register(
                bell,
                point_of_sale,
                &signed_in.environment,
                &token,
                &signed_in.location_id,
                &subscription,
            )
            .await?;
        self.store.set_registration(Some(BellRegistration {
            bell: bell.clone(),
            registration_id: answer.registration_id,
            vapid_public_key: info.vapid_public_key.clone(),
            subscription,
            distributor: through,
            point_of_sale: Some(point_of_sale.wire().to_owned()),
            environment: Some(signed_in.environment.clone()),
        }));
        let last_ding_at = self.mem().last_ding_at;
        Ok(match self.status_now() {
            missing @ WakeUpStatus::MissingWakeups { .. } => missing,
            _ => WakeUpStatus::Connected { last_ding_at },
        })
    }

    /// The subscription for `vapid_public_key` through `through`: the one
    /// held, or a new one the platform gives.
    async fn subscription(
        &self,
        vapid_public_key: &str,
        through: &str,
        cancel: &CancellationToken,
    ) -> Result<PushState, Cancelled> {
        let held = self.push.state().borrow().clone();
        let (subscribed_for, subscribed_through) = {
            let mem = self.mem();
            (mem.subscribed_for.clone(), mem.subscribed_through.clone())
        };
        let state = if matches!(held, PushState::Subscribed(_))
            && subscribed_for.as_deref() == Some(vapid_public_key)
            && subscribed_through.as_deref() == Some(through)
        {
            held
        } else {
            // A rotated key, or another distributor: the old subscription ends first (D16).
            let previous =
                subscribed_for.or_else(|| self.store.registration().map(|r| r.vapid_public_key));
            if previous.is_some_and(|key| key != vapid_public_key) {
                self.push.unsubscribe();
            }
            {
                let mut mem = self.mem();
                mem.subscribed_for = Some(vapid_public_key.to_owned());
                mem.subscribed_through = Some(through.to_owned());
            }
            self.push.subscribe(vapid_public_key);
            let mut state = self.push.state();
            let answered = tokio::time::timeout(
                SUBSCRIBE_WAIT,
                state.wait_for(|s| *s != PushState::Subscribing),
            );
            match or_cancel(cancel, answered).await? {
                Ok(Ok(answer)) => answer.clone(),
                _ => PushState::Subscribing,
            }
        };
        self.mem().taken = match &state {
            PushState::Subscribed(subscription) => Some(subscription.clone()),
            _ => None,
        };
        Ok(state)
    }

    /// Take a registration back: `DELETE`, tried once within the time it
    /// gets, and forgotten here whatever the answer (decision 97). Not a
    /// wait to cancel at.
    async fn take_back(&self, registration: BellRegistration) {
        let unregistering = self
            .bells
            .unregister(&registration.bell, &registration.registration_id);
        // The Bell drops a dead endpoint by itself (its failure limit), so a failure is let go.
        let _ = tokio::time::timeout(UNREGISTER_WAIT, unregistering).await;
        self.store.set_registration(None);
        // Its dings went with it: the next registration's Last wake-up starts at None yet.
        self.mem().last_ding_at = None;
    }

    /// No registration while the till can't use it: the Bell would ring for
    /// nothing.
    async fn release(&self) {
        if let Some(registration) = self.store.registration() {
            self.take_back(registration).await;
        }
    }

    /// End the platform's subscription, one an earlier run of the app made
    /// included. The app ending it is owed no [`PushFailure::Lost`], so no
    /// departure waits for one (decision 118).
    fn unsubscribe(&self) {
        {
            let mut mem = self.mem();
            mem.subscribed_for = None;
            mem.subscribed_through = None;
            mem.departed = None;
            mem.taken = None;
        }
        self.push.unsubscribe();
    }

    /// The pass is over: `outcome` shown, and a retry set for the statuses
    /// that retry by themselves (D5).
    fn settle(self: &Arc<Self>, outcome: WakeUpStatus) {
        let now = self.clock.now();
        if matches!(outcome, WakeUpStatus::Connected { .. })
            && !matches!(self.status_now(), WakeUpStatus::Connected { .. })
        {
            self.mem().connected_at = Some(now);
        }
        self.set_status(outcome.clone());
        let wait = match outcome {
            WakeUpStatus::Unreachable { .. } => self.backoff(),
            WakeUpStatus::Refused {
                retry_at: Some(at), ..
            } => Duration::from_secs((at - now).max(0) as u64),
            _ => {
                self.mem().waits = 0;
                return;
            }
        };
        let cancel = self.process.child_token();
        self.jobs().retry = Some(cancel.clone());
        let me = Arc::clone(self);
        self.runtime.spawn(async move {
            if or_cancel(&cancel, tokio::time::sleep(wait)).await.is_ok() {
                me.run(false);
            }
        });
    }

    /// 1 minute, doubling to 15 (D5).
    fn backoff(&self) -> Duration {
        let mut mem = self.mem();
        let wait = (FIRST_RETRY * (1 << mem.waits.min(4))).min(LAST_RETRY);
        mem.waits += 1;
        wait
    }

    fn ding(&self) {
        let at = self.clock.now();
        self.mem().last_ding_at = Some(at);
        self.update_status(|status| match status {
            WakeUpStatus::Connected { .. } | WakeUpStatus::MissingWakeups { .. } => {
                Some(WakeUpStatus::Connected {
                    last_ding_at: Some(at),
                })
            }
            _ => None,
        });
    }

    /// The platform changed things outside a pass: a new endpoint, one it
    /// gives after the pass stopped waiting for it (decision 123), or the
    /// subscription ended. A subscription the last pass already took is
    /// nothing new, whatever the Bell made of it.
    fn push_changed(self: &Arc<Self>, state: PushState) {
        if self.store.choice() == WakeUpChoice::TimerOnly {
            return;
        }
        let held = self.store.registration();
        match state {
            PushState::Subscribed(subscription) => {
                let taken = self.mem().taken.clone();
                if held.map(|r| r.subscription).as_ref() != Some(&subscription)
                    && taken.as_ref() != Some(&subscription)
                {
                    self.run(false);
                }
            }
            PushState::Failed(PushFailure::Lost) if held.is_some() => {
                let me = Arc::clone(self);
                self.runtime.spawn(async move {
                    let _ = me.distributor_gone().await;
                });
            }
            _ => {}
        }
    }

    /// The subscription ended: the Bell's registration goes with it
    /// (decision 91). The distributor ending it itself is
    /// [`WakeUpStatus::NoDistributor`]; one uninstalled with another left
    /// (the connector reports that as an end too) is a pass through what is
    /// installed (decision 118).
    async fn distributor_gone(self: Arc<Self>) -> Result<(), Cancelled> {
        let gone = {
            let mut mem = self.mem();
            let departed = mem.departed.take();
            departed.or_else(|| mem.subscribed_through.clone())
        };
        or_cancel(&self.process, self.cancel_and_join_pass()).await?;
        let _pass = or_cancel(&self.process, self.lock.lock()).await?;
        if let Some(registration) = self.store.registration() {
            self.take_back(registration).await;
        }
        {
            let mut mem = self.mem();
            mem.subscribed_for = None;
            mem.subscribed_through = None;
            mem.taken = None;
        }
        if self.store.choice() == WakeUpChoice::TimerOnly {
            return Ok(());
        }
        match self.distributors.resolve() {
            Ok(Distributor::None) => self.settle(WakeUpStatus::NoDistributor),
            Ok(Distributor::Found(package_name)) if Some(&package_name) == gone.as_ref() => {
                self.settle(WakeUpStatus::NoDistributor)
            }
            Ok(Distributor::Found(_) | Distributor::Choose(_)) => self.run(false),
            Err(_) => self.settle(WakeUpStatus::unreachable(TILL_FAILED)),
        }
        Ok(())
    }

    /// An app came or went: look again when waiting for a distributor, or
    /// when the one in use may be gone.
    fn distributors_changed(self: &Arc<Self>) {
        if self.store.choice() == WakeUpChoice::TimerOnly {
            return;
        }
        let waiting = matches!(
            self.status_now(),
            WakeUpStatus::NoDistributor | WakeUpStatus::ChooseDistributor { .. }
        );
        let through = self.mem().subscribed_through.clone();
        let through = through.or_else(|| self.store.registration().map(|r| r.distributor));
        // A failure to look is a look at the next pass, not the end of this listener (decision 120).
        let moved = through.as_ref().is_some_and(|through| {
            !matches!(self.distributors.resolve(), Ok(Distributor::Found(found)) if found == *through)
        });
        if moved {
            self.mem().departed = through;
        }
        if waiting || moved {
            self.run(false);
        }
    }
}

/// Wake-ups' registration lifecycle against a recording Bell, distributor
/// and point of sale (A13): every transition of A3, A6, A7, A10 and A11, on
/// paused time, for a Square till and a Clover one (acceptance T5.4).
/// Ported from `BellWakeUpsTest`.
#[cfg(test)]
mod tests;
