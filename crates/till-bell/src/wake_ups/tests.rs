//! Each scenario runs for a till signed in to Square (`square::…`, Android's
//! `BellWakeUpsTest`) and one signed in to Clover, its merchant as the
//! location (`clover::…`, `CloverBellWakeUpsTest`), on tokio's paused
//! clock. `run_current` and `wait` stand in for kotlinx-coroutines-test's
//! `runCurrent` and `advanceTimeBy`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use monokulo_client::{Clock, FixedClock, PointOfSale};
use tokio::sync::{broadcast, watch};
use tokio::time::Instant;

use super::*;
use crate::{BellInfo, BellRegistered};

/// A till signed in to one point of sale, as the scenarios run for it.
#[derive(Clone, Copy)]
struct TestTill {
    point_of_sale: PointOfSale,
    application_id: &'static str,
    environment: &'static str,
    /// Another environment of the same point of sale.
    other_environment: &'static str,
    /// Another application of the same point of sale and environment: a Bell's.
    other_application_id: &'static str,
    location_id: &'static str,
    token: &'static str,
    /// Why a Bell without the till's environment refuses it, in the cashier's words.
    not_rung_here: &'static str,
}

const SQUARE: TestTill = TestTill {
    point_of_sale: PointOfSale::Square,
    application_id: "sandbox-sq0idb-dev",
    environment: "sandbox",
    other_environment: "production",
    other_application_id: "sandbox-sq0idb-other",
    location_id: "L1",
    token: "EAAAtoken",
    not_rung_here: "it doesn't ring Square sandbox tills",
};

const CLOVER: TestTill = TestTill {
    point_of_sale: PointOfSale::Clover,
    application_id: "MONOKULARBELL",
    environment: "north_america",
    other_environment: "europe",
    other_application_id: "OTHERCLOVERAP",
    location_id: "MCLOVERDEV001",
    token: "clv-access-token",
    not_rung_here: "it doesn't ring Clover tills in North America",
};

const KEY: &str = "BOS-key";
const NTFY: &str = "io.heckel.ntfy";
const NEXTPUSH: &str = "org.unifiedpush.distributor.nextpush";

const fn minutes(n: u64) -> Duration {
    Duration::from_secs(n * 60)
}

const fn seconds(n: u64) -> Duration {
    Duration::from_secs(n)
}

type InfoAnswer = Arc<dyn Fn() -> Result<BellInfo, BellError> + Send + Sync>;
type RegisterAnswer = Arc<dyn Fn() -> Result<BellRegistered, BellError> + Send + Sync>;

/// What the stand-in Bell does with a `DELETE`.
#[derive(Clone, Copy)]
enum Unregister {
    Answers,
    Fails,
    NeverAnswers,
}

/// A Bell that records each call, answering as the test sets it.
struct FakeBell {
    calls: Mutex<Vec<String>>,
    /// The point of sale each POST registered for.
    registered_for: Mutex<Vec<PointOfSale>>,
    /// The environment each POST registered in.
    registered_in: Mutex<Vec<String>>,
    infos: Mutex<HashMap<BellAddress, InfoAnswer>>,
    /// `None`: `br_1`, `br_2`, … in turn.
    register: Mutex<Option<RegisterAnswer>>,
    unregister: Mutex<Unregister>,
    ids: Mutex<u32>,
}

impl FakeBell {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn set_info(&self, bell: &BellAddress, answer: InfoAnswer) {
        self.infos.lock().unwrap().insert(bell.clone(), answer);
    }

    fn set_register(
        &self,
        answer: impl Fn() -> Result<BellRegistered, BellError> + Send + Sync + 'static,
    ) {
        *self.register.lock().unwrap() = Some(Arc::new(answer));
    }
}

#[async_trait]
impl BellClient for FakeBell {
    async fn info(&self, bell: &BellAddress) -> Result<BellInfo, BellError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("GET {}", bell.display()));
        let answer = self.infos.lock().unwrap().get(bell).cloned();
        match answer {
            Some(answer) => answer(),
            None => Err(BellError::Connection("connection refused".into())),
        }
    }

    async fn register(
        &self,
        bell: &BellAddress,
        point_of_sale: PointOfSale,
        environment: &str,
        access_token: &Secret,
        location_id: &str,
        subscription: &PushSubscription,
    ) -> Result<BellRegistered, BellError> {
        self.calls.lock().unwrap().push(format!(
            "POST {} {} {location_id} {}",
            bell.display(),
            access_token.expose_str(),
            subscription.endpoint
        ));
        self.registered_for.lock().unwrap().push(point_of_sale);
        self.registered_in
            .lock()
            .unwrap()
            .push(environment.to_owned());
        let answer = self.register.lock().unwrap().clone();
        match answer {
            Some(answer) => answer(),
            None => {
                let mut ids = self.ids.lock().unwrap();
                *ids += 1;
                Ok(BellRegistered {
                    registration_id: format!("br_{ids}"),
                })
            }
        }
    }

    async fn unregister(&self, bell: &BellAddress, registration_id: &str) -> Result<(), BellError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("DELETE {} {registration_id}", bell.display()));
        let behaviour = *self.unregister.lock().unwrap();
        match behaviour {
            Unregister::Answers => Ok(()),
            Unregister::Fails => Err(BellError::Connection("connection refused".into())),
            Unregister::NeverAnswers => std::future::pending().await,
        }
    }
}

/// What subscribing comes to.
#[derive(Clone, Copy)]
enum Answer {
    /// A subscription for the current endpoint.
    Subscribed,
    Fails(PushFailure),
    /// Nothing: it stays waiting.
    Silent,
}

struct FakePush {
    calls: Mutex<Vec<String>>,
    endpoint: Mutex<String>,
    answer: Mutex<Answer>,
    state: watch::Sender<PushState>,
    dings: broadcast::Sender<()>,
}

impl FakePush {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn subscription(&self) -> PushSubscription {
        PushSubscription {
            endpoint: self.endpoint.lock().unwrap().clone(),
            p256dh: "BKey".into(),
            auth: "auth".into(),
        }
    }

    /// `state.value = …`, as a `MutableStateFlow` takes it: the same value again is no change.
    fn set_state(&self, state: PushState) {
        self.state.send_if_modified(|held| {
            let changed = *held != state;
            *held = state;
            changed
        });
    }

    fn state_now(&self) -> PushState {
        self.state.borrow().clone()
    }

    fn ding(&self) {
        let _ = self.dings.send(());
    }
}

impl PushSubscriptions for FakePush {
    fn state(&self) -> watch::Receiver<PushState> {
        self.state.subscribe()
    }

    fn dings(&self) -> broadcast::Receiver<()> {
        self.dings.subscribe()
    }

    fn subscribe(&self, vapid_public_key: &str) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("subscribe {vapid_public_key}"));
        self.set_state(PushState::Subscribing);
        let answer = *self.answer.lock().unwrap();
        match answer {
            Answer::Subscribed => self.set_state(PushState::Subscribed(self.subscription())),
            Answer::Fails(reason) => self.set_state(PushState::Failed(reason)),
            Answer::Silent => {}
        }
    }

    fn unsubscribe(&self) {
        self.calls.lock().unwrap().push("unsubscribe".into());
        self.set_state(PushState::None);
    }
}

struct FakeDistributors {
    resolved: Mutex<Distributor>,
    saved: Mutex<Vec<String>>,
    changes: broadcast::Sender<()>,
}

impl FakeDistributors {
    fn set(&self, distributor: Distributor) {
        *self.resolved.lock().unwrap() = distributor;
    }

    fn changed(&self) {
        let _ = self.changes.send(());
    }
}

impl Distributors for FakeDistributors {
    fn resolve(&self) -> Result<Distributor, DeviceError> {
        Ok(self.resolved.lock().unwrap().clone())
    }

    fn save(&self, package_name: &str) {
        self.saved.lock().unwrap().push(package_name.to_owned());
        self.set(Distributor::Found(package_name.to_owned()));
    }

    fn changes(&self) -> broadcast::Receiver<()> {
        self.changes.subscribe()
    }
}

#[derive(Default)]
struct MemoryStore {
    choice: Mutex<WakeUpChoice>,
    own_bell: Mutex<Option<BellAddress>>,
    registration: Mutex<Option<BellRegistration>>,
}

impl WakeUpStore for MemoryStore {
    fn choice(&self) -> WakeUpChoice {
        self.choice.lock().unwrap().clone()
    }

    fn set_choice(&self, choice: WakeUpChoice) {
        *self.choice.lock().unwrap() = choice;
    }

    fn own_bell(&self) -> Option<BellAddress> {
        self.own_bell.lock().unwrap().clone()
    }

    fn set_own_bell(&self, bell: Option<BellAddress>) {
        *self.own_bell.lock().unwrap() = bell;
    }

    fn registration(&self) -> Option<BellRegistration> {
        self.registration.lock().unwrap().clone()
    }

    fn set_registration(&self, registration: Option<BellRegistration>) {
        *self.registration.lock().unwrap() = registration;
    }
}

struct FakePos {
    point_of_sale: PointOfSale,
    /// `None`: not signed in.
    application_id: Mutex<Option<String>>,
    environment: Mutex<String>,
    location_id: String,
    token: Mutex<Option<String>>,
}

impl FakePos {
    fn sign_in(&self, application_id: Option<&str>) {
        *self.application_id.lock().unwrap() = application_id.map(str::to_owned);
    }

    fn move_to(&self, environment: &str) {
        *self.environment.lock().unwrap() = environment.to_owned();
    }
}

#[async_trait]
impl PosSide for FakePos {
    fn signed_in(&self) -> Result<Option<PosSignIn>, DeviceError> {
        Ok(self
            .application_id
            .lock()
            .unwrap()
            .clone()
            .map(|application_id| PosSignIn {
                point_of_sale: self.point_of_sale,
                application_id,
                environment: self.environment.lock().unwrap().clone(),
                location_id: self.location_id.clone(),
            }))
    }

    async fn access_token(&self) -> Result<Option<Secret>, DeviceError> {
        Ok(self.token.lock().unwrap().clone().map(Secret::new))
    }
}

/// Unix seconds as tokio's paused clock has them, from 0 at the scenario's start.
struct PausedClock(Instant);

impl Clock for PausedClock {
    fn now(&self) -> i64 {
        self.0.elapsed().as_secs() as i64
    }
}

/// One scenario's world: the stand-ins, the store, and the app's process.
struct Scenario {
    till: TestTill,
    monokulo_bell: BellAddress,
    own: BellAddress,
    bell: Arc<FakeBell>,
    push: Arc<FakePush>,
    distributors: Arc<FakeDistributors>,
    store: Arc<MemoryStore>,
    pos: Arc<FakePos>,
    clock: Arc<PausedClock>,
    /// The app's process: a new start ends the one before.
    process: Mutex<Option<Arc<BellWakeUps>>>,
}

impl Scenario {
    fn new(till: TestTill) -> Scenario {
        let s = Scenario {
            till,
            monokulo_bell: BellAddress::parse("bell.monokulo.com", false).unwrap(),
            own: BellAddress::parse("bell.thebeanroom.coffee", false).unwrap(),
            bell: Arc::new(FakeBell {
                calls: Mutex::default(),
                registered_for: Mutex::default(),
                registered_in: Mutex::default(),
                infos: Mutex::default(),
                register: Mutex::default(),
                unregister: Mutex::new(Unregister::Answers),
                ids: Mutex::default(),
            }),
            push: Arc::new(FakePush {
                calls: Mutex::default(),
                endpoint: Mutex::new("https://ntfy.sh/up1".into()),
                answer: Mutex::new(Answer::Subscribed),
                state: watch::Sender::new(PushState::None),
                dings: broadcast::Sender::new(8),
            }),
            distributors: Arc::new(FakeDistributors {
                resolved: Mutex::new(Distributor::Found(NTFY.into())),
                saved: Mutex::default(),
                changes: broadcast::Sender::new(8),
            }),
            store: Arc::default(),
            pos: Arc::new(FakePos {
                point_of_sale: till.point_of_sale,
                application_id: Mutex::new(Some(till.application_id.into())),
                environment: Mutex::new(till.environment.into()),
                location_id: till.location_id.into(),
                token: Mutex::new(Some(till.token.into())),
            }),
            clock: Arc::new(PausedClock(Instant::now())),
            process: Mutex::default(),
        };
        s.bell.set_info(&s.monokulo_bell, s.info());
        s.bell.set_info(&s.own, s.info());
        s
    }

    /// A Bell's info with an entry for the till's point of sale only.
    fn info(&self) -> InfoAnswer {
        self.info_with(self.till.application_id, self.till.environment, KEY)
    }

    fn info_with(&self, app: &str, environment: &str, key: &str) -> InfoAnswer {
        let info = BellInfo {
            points_of_sale: [(
                self.till.point_of_sale.wire().to_owned(),
                map(&[(environment, app)]),
            )]
            .into(),
            vapid_public_key: key.to_owned(),
        };
        Arc::new(move || Ok(info.clone()))
    }

    /// What the till posts: its token, its location and the endpoint.
    fn posted(&self) -> String {
        format!("{} {}", self.till.token, self.till.location_id)
    }

    fn now(&self) -> i64 {
        self.clock.now()
    }

    fn wake_ups(&self) -> BellWakeUps {
        BellWakeUps::new(
            self.push.clone(),
            self.distributors.clone(),
            self.bell.clone(),
            self.store.clone(),
            self.monokulo_bell.clone(),
            self.pos.clone(),
            self.clock.clone(),
        )
    }

    /// A fresh start of the app over the store, on this scenario's clock.
    async fn start(&self) -> Arc<BellWakeUps> {
        if let Some(before) = self.process.lock().unwrap().take() {
            before.inner.process.cancel();
        }
        let wake_ups = Arc::new(self.wake_ups());
        *self.process.lock().unwrap() = Some(wake_ups.clone());
        run_current().await;
        wake_ups
    }

    async fn connected(&self) -> Arc<BellWakeUps> {
        self.connected_to(WakeUpChoice::MonokuloBell).await
    }

    async fn connected_to(&self, choice: WakeUpChoice) -> Arc<BellWakeUps> {
        let wake_ups = self.start().await;
        wake_ups.choose(choice);
        run_current().await;
        assert_eq!(status(&wake_ups), connected(None));
        self.bell.calls.lock().unwrap().clear();
        self.push.calls.lock().unwrap().clear();
        wake_ups
    }

    fn registration(&self) -> Option<BellRegistration> {
        self.store.registration()
    }
}

fn map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

use std::collections::BTreeMap;

/// Let every task that can run, run, without moving the clock.
async fn run_current() {
    for _ in 0..1000 {
        tokio::task::yield_now().await;
    }
}

/// Move the clock on by `duration`, running each task as its time comes, then run what is due.
async fn wait(duration: Duration) {
    tokio::time::sleep(duration).await;
    run_current().await;
}

fn status(wake_ups: &BellWakeUps) -> WakeUpStatus {
    wake_ups.status().borrow().clone()
}

/// Where the status history stands, for [`seen_since`].
fn mark(wake_ups: &BellWakeUps) -> usize {
    wake_ups.history().len() - 1
}

/// Every status from `mark` on, the one it was at then included.
fn seen_since(wake_ups: &BellWakeUps, mark: usize) -> Vec<WakeUpStatus> {
    wake_ups.history()[mark..].to_vec()
}

fn connected(last_ding_at: Option<i64>) -> WakeUpStatus {
    WakeUpStatus::Connected { last_ding_at }
}

fn missing(last_ding_at: Option<i64>) -> WakeUpStatus {
    WakeUpStatus::MissingWakeups { last_ding_at }
}

fn unreachable(reason: &str) -> WakeUpStatus {
    WakeUpStatus::unreachable(reason)
}

fn refused(reason: &str) -> WakeUpStatus {
    WakeUpStatus::refused(reason)
}

fn refused_until(reason: &str, retry_at: i64) -> WakeUpStatus {
    WakeUpStatus::Refused {
        reason: reason.into(),
        retry_at: Some(retry_at),
    }
}

fn own(bell: &BellAddress) -> WakeUpChoice {
    WakeUpChoice::OwnBell { bell: bell.clone() }
}

fn subscription(endpoint: &str, p256dh: &str, auth: &str) -> PushSubscription {
    PushSubscription {
        endpoint: endpoint.into(),
        p256dh: p256dh.into(),
        auth: auth.into(),
    }
}

fn problem(status: u16, slug: Option<&str>) -> BellError {
    BellError::Problem(BellProblem {
        status,
        slug: slug.map(BellSlug::from_slug),
        detail: None,
        retry_after: None,
    })
}

macro_rules! scenarios {
    ($($name:ident),* $(,)?) => {
        mod square {
            $(
                #[tokio::test(start_paused = true)]
                async fn $name() {
                    super::$name(super::SQUARE).await;
                }
            )*
        }
        mod clover {
            $(
                #[tokio::test(start_paused = true)]
                async fn $name() {
                    super::$name(super::CLOVER).await;
                }
            )*
        }
    };
}

scenarios!(
    the_pace_is_30_s_while_connected_and_2_s_for_every_other_status,
    timer_only_a_fresh_install_asks_nothing_of_anyone_a2_11,
    picking_the_bell_saves_it_says_connecting_at_once_then_reads_subscribes_and_registers_a3_13_a3_14,
    picking_my_own_bell_keeps_its_address_for_the_row,
    picking_the_bell_already_chosen_does_nothing_a3_18,
    a_bell_that_cant_be_reached_is_unreachable_retried_1_2_4_minutes_on_to_15_and_connects_when_its_back_a11_1_a4_20,
    an_unreachable_bells_reasons_in_the_cashiers_words,
    an_answer_that_isnt_a_bells_is_unreachable_and_nothing_is_posted_a11_7,
    a_bell_without_the_tills_environment_is_refused_for_good_in_words_naming_it_with_no_sign_in_and_no_post_a3_14,
    a_till_signed_in_elsewhere_is_refused_by_a_bell_for_this_environment_alone_naming_the_tills,
    a_bell_ringing_both_environments_registers_the_till_in_its_own_through_its_own_application_decision_305,
    a_bell_ringing_both_environments_asks_for_a_sign_in_through_the_tills_environments_application_not_the_others,
    a_till_moved_to_the_other_environment_takes_its_registration_back_and_registers_again_in_the_new_one_decision_306,
    a_registration_kept_from_before_registrations_named_their_environment_is_taken_back_and_made_again_at_the_next_start_decision_306,
    a_bell_that_doesnt_run_the_tills_point_of_sale_is_refused_for_good_before_its_environment_or_application_t5_2,
    the_environment_is_checked_before_the_application_t5_2,
    an_environment_this_build_doesnt_know_is_passed_over_beside_the_tills_or_alone,
    a_bell_for_both_points_of_sale_registers_the_till_for_its_own_with_its_own_location,
    a_bell_on_another_application_asks_for_a_sign_in_through_it_and_registers_nothing_until_then_a6_26,
    without_a_sign_in_the_bell_waits_for_one_a3_14,
    a_sign_in_the_point_of_sale_ended_at_the_token_is_no_pos_and_takes_back_what_was_registered,
    no_distributor_is_no_distributor_with_the_choice_kept_and_one_installed_connects_by_itself_a7_29_a7_30,
    several_distributors_and_no_default_the_cashier_picks_and_it_is_saved_a7_31,
    the_distributor_failing_is_unreachable_with_its_reason_retried,
    each_problem_type_is_its_refusal_and_only_rate_limited_retries_by_itself_a1_7_a11_5_a11_6,
    a_refusal_waits_for_a_re_pick_a_sign_in_or_a_start_but_rate_limited_comes_back_after_retry_after,
    switching_bells_takes_the_old_registration_back_before_registering_with_the_new_a3_17,
    timer_only_takes_the_registration_back_once_ends_the_subscription_and_keeps_my_own_bells_address_a10_1,
    signing_out_takes_the_registration_back_and_keeps_the_choice_awaited_a10_2,
    a_registration_taken_back_takes_its_last_wake_up_with_it_so_the_next_one_starts_at_none_yet,
    disconnecting_takes_the_registration_back_and_forgets_the_choice_a10_3,
    a_bell_that_cant_take_the_registration_back_is_tried_once_and_dropped_here_a10_7,
    a_bell_that_never_answers_the_delete_holds_the_sign_out_10_s_at_most_a10_7,
    unregistering_stops_a_registration_under_way_and_takes_back_what_it_got,
    every_app_start_registers_again_with_the_same_subscription_a4_21_a3_19,
    a_shift_starting_registers_again_without_leaving_connected_so_the_pace_holds_a4_21,
    an_app_start_with_timer_only_finishes_taking_back_a_registration_left_behind,
    a_new_endpoint_from_the_distributor_deletes_the_old_registration_then_registers_the_new_a7_32,
    an_endpoint_changed_while_the_app_was_away_is_caught_at_the_next_start,
    the_distributor_ending_the_subscription_takes_the_registration_back_and_is_no_distributor_a7_32,
    the_distributor_uninstalled_takes_the_registration_back_and_one_installed_again_connects_a7_32_a7_30,
    the_distributor_uninstalled_with_two_others_left_takes_the_registration_back_before_the_cashier_picks_decision_108,
    the_distributor_uninstalled_with_another_left_connects_through_it_when_the_connectors_end_comes_after_decision_118,
    the_distributor_uninstalled_with_another_left_connects_through_it_when_the_connectors_end_comes_first_decision_118,
    the_distributor_uninstalled_with_two_left_and_the_connectors_end_arriving_asks_the_cashier_never_no_distributor_decision_118,
    a_distributor_installed_after_the_last_one_went_keeps_its_own_end_of_the_subscription_read_as_the_cashiers_decision_118,
    a_distributor_uninstalled_while_the_app_was_away_is_known_at_the_start_from_the_registration_decision_122,
    a_pass_failing_on_this_device_for_any_reason_leaves_the_till_running_and_retries_decision_120,
    an_app_installed_that_isnt_the_distributor_changes_nothing,
    a_rotated_vapid_key_subscribes_again_for_it_then_registers_the_new_subscription_a9_8,
    a_rotated_key_is_caught_at_an_app_start_too,
    a_timer_found_sale_with_no_ding_in_60_s_is_a_missed_wake_up_2_s_and_one_registration_again_a11_3,
    a_ding_then_a_timer_found_sale_stays_connected_a11_3,
    a_timer_check_that_finds_nothing_or_one_just_after_connecting_proves_nothing,
    a_timer_found_sale_while_not_connected_changes_nothing,
    an_endpoint_the_distributor_gives_after_the_pass_stopped_waiting_registers_at_once_decision_123,
    an_unreachable_bell_is_tried_again_on_recheck_before_its_wait_is_out_decision_123,
    a_ding_while_not_connected_changes_no_status,
    the_bells_refusals_and_outages_for_the_tills_point_of_sale_are_its_words_t5_4,
);

async fn the_pace_is_30_s_while_connected_and_2_s_for_every_other_status(till: TestTill) {
    let statuses = [
        WakeUpStatus::Off,
        WakeUpStatus::NoPos,
        WakeUpStatus::NoDistributor,
        WakeUpStatus::ChooseDistributor {
            packages: vec!["a".into(), "b".into()],
        },
        WakeUpStatus::Connecting,
        missing(Some(5)),
        unreachable("no connection"),
        refused_until("x", 5),
        WakeUpStatus::SignInNeeded {
            point_of_sale: till.point_of_sale,
            application_id: "sq0idp-x".into(),
        },
    ];
    for status in statuses {
        assert_eq!(status.interval(), seconds(2), "{status:?}");
    }
    assert_eq!(connected(None).interval(), seconds(30));
    assert_eq!(connected(Some(5)).interval(), seconds(30));
}

async fn timer_only_a_fresh_install_asks_nothing_of_anyone_a2_11(till: TestTill) {
    let s = Scenario::new(till);
    let wake_ups = s.start().await;
    wait(minutes(30)).await;
    assert_eq!(*wake_ups.choice().borrow(), WakeUpChoice::TimerOnly);
    assert_eq!(status(&wake_ups), WakeUpStatus::Off);
    assert_eq!(s.bell.calls(), Vec::<String>::new());
    assert_eq!(s.push.calls(), Vec::<String>::new());
    wake_ups.recheck();
    wake_ups.timer_found(3);
    run_current().await;
    assert_eq!(s.bell.calls(), Vec::<String>::new());
}

async fn picking_the_bell_saves_it_says_connecting_at_once_then_reads_subscribes_and_registers_a3_13_a3_14(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    assert_eq!(s.store.choice(), WakeUpChoice::MonokuloBell);
    assert_eq!(*wake_ups.choice().borrow(), WakeUpChoice::MonokuloBell);
    assert_eq!(status(&wake_ups), WakeUpStatus::Connecting);
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
    assert_eq!(
        s.bell.calls(),
        [
            "GET bell.monokulo.com".to_owned(),
            format!("POST bell.monokulo.com {} https://ntfy.sh/up1", s.posted())
        ]
    );
    assert_eq!(s.push.calls(), [format!("subscribe {KEY}")]);
    let registration = s.registration().expect("a registration");
    assert_eq!(registration.registration_id, "br_1");
    assert_eq!(registration.bell, s.monokulo_bell);
    assert_eq!(registration.vapid_public_key, KEY);
    assert_eq!(registration.distributor, NTFY);
    assert_eq!(registration.subscription.endpoint, "https://ntfy.sh/up1");
}

async fn picking_my_own_bell_keeps_its_address_for_the_row(till: TestTill) {
    let s = Scenario::new(till);
    let wake_ups = s.start().await;
    wake_ups.choose(own(&s.own));
    run_current().await;
    assert_eq!(s.store.own_bell(), Some(s.own.clone()));
    assert_eq!(status(&wake_ups), connected(None));
    assert_eq!(s.bell.calls()[0], "GET bell.thebeanroom.coffee");
}

async fn picking_the_bell_already_chosen_does_nothing_a3_18(till: TestTill) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(s.bell.calls(), Vec::<String>::new());
    assert_eq!(status(&wake_ups), connected(None));
}

async fn a_bell_that_cant_be_reached_is_unreachable_retried_1_2_4_minutes_on_to_15_and_connects_when_its_back_a11_1_a4_20(
    till: TestTill,
) {
    let s = Scenario::new(till);
    s.bell.infos.lock().unwrap().remove(&s.monokulo_bell);
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(status(&wake_ups), unreachable("no connection"));
    assert_eq!(s.bell.calls(), ["GET bell.monokulo.com"]);
    assert_eq!(
        s.push.calls(),
        Vec::<String>::new(),
        "nothing subscribed for a Bell not read"
    );
    for (i, gap) in [1, 2, 4, 8, 15, 15].map(minutes).into_iter().enumerate() {
        wait(gap - seconds(1)).await;
        assert_eq!(s.bell.calls().len(), i + 1, "not before {gap:?}");
        wait(seconds(1)).await;
        assert_eq!(s.bell.calls().len(), i + 2, "after {gap:?}");
    }
    s.bell.set_info(&s.monokulo_bell, s.info());
    wait(minutes(15)).await;
    assert_eq!(status(&wake_ups), connected(None));
    assert!(s.bell.calls().last().unwrap().starts_with("POST"));
}

async fn an_unreachable_bells_reasons_in_the_cashiers_words(_: TestTill) {
    assert_eq!(
        unreachable_reason(&BellError::Connection("connection refused".into())),
        "no connection"
    );
    assert_eq!(
        unreachable_reason(&BellError::Connection("no route".into())),
        "no connection"
    );
    // The JVM's SocketTimeoutException and InterruptedIOException are both a timeout here.
    assert_eq!(unreachable_reason(&BellError::Timeout), "it didn't answer");
    assert_eq!(unreachable_reason(&problem(503, None)), "it answered 503");
    assert_eq!(unreachable_reason(&BellError::NotABell), "it isn't a Bell");
}

async fn an_answer_that_isnt_a_bells_is_unreachable_and_nothing_is_posted_a11_7(till: TestTill) {
    let s = Scenario::new(till);
    s.bell
        .set_info(&s.own, Arc::new(|| Err(BellError::NotABell)));
    let wake_ups = s.start().await;
    wake_ups.choose(own(&s.own));
    run_current().await;
    assert_eq!(status(&wake_ups), unreachable("it isn't a Bell"));
    assert_eq!(s.bell.calls(), ["GET bell.thebeanroom.coffee"]);
}

async fn a_bell_without_the_tills_environment_is_refused_for_good_in_words_naming_it_with_no_sign_in_and_no_post_a3_14(
    till: TestTill,
) {
    let s = Scenario::new(till);
    s.bell.set_info(
        &s.monokulo_bell,
        s.info_with(till.other_application_id, till.other_environment, KEY),
    );
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(status(&wake_ups), refused(till.not_rung_here));
    assert_eq!(
        status(&wake_ups),
        refused(&pos_words::not_rung_in(
            till.point_of_sale,
            till.environment
        ))
    );
    wait(minutes(60)).await;
    assert_eq!(
        s.bell.calls(),
        ["GET bell.monokulo.com"],
        "no retry on its own"
    );
    assert_eq!(s.push.calls(), Vec::<String>::new());
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(
        s.bell.calls().len(),
        2,
        "re-picking a refused Bell tries again"
    );
}

async fn a_till_signed_in_elsewhere_is_refused_by_a_bell_for_this_environment_alone_naming_the_tills(
    till: TestTill,
) {
    let s = Scenario::new(till);
    s.pos.move_to(till.other_environment);
    let wake_ups = BellWakeUps::new(
        s.push.clone(),
        s.distributors.clone(),
        s.bell.clone(),
        s.store.clone(),
        s.monokulo_bell.clone(),
        s.pos.clone(),
        Arc::new(FixedClock::at(0)),
    );
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(
        status(&wake_ups),
        refused(&pos_words::not_rung_in(
            till.point_of_sale,
            till.other_environment
        ))
    );
}

/// A Bell ringing the till's environment and the other one, through `other_app` there.
fn both_environments(s: &Scenario, other_app: &str, own_app: &str) -> InfoAnswer {
    let info = BellInfo {
        points_of_sale: [(
            s.till.point_of_sale.wire().to_owned(),
            map(&[
                (s.till.other_environment, other_app),
                (s.till.environment, own_app),
            ]),
        )]
        .into(),
        vapid_public_key: KEY.into(),
    };
    Arc::new(move || Ok(info.clone()))
}

async fn a_bell_ringing_both_environments_registers_the_till_in_its_own_through_its_own_application_decision_305(
    till: TestTill,
) {
    let s = Scenario::new(till);
    s.bell.set_info(
        &s.monokulo_bell,
        both_environments(&s, till.other_application_id, till.application_id),
    );
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
    assert_eq!(*s.bell.registered_in.lock().unwrap(), [till.environment]);
    let registration = s.registration().unwrap();
    assert_eq!(
        registration.point_of_sale.as_deref(),
        Some(till.point_of_sale.wire())
    );
    assert_eq!(registration.environment.as_deref(), Some(till.environment));
}

async fn a_bell_ringing_both_environments_asks_for_a_sign_in_through_the_tills_environments_application_not_the_others(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let bells_own = till.other_application_id;
    s.bell.set_info(
        &s.monokulo_bell,
        both_environments(&s, "another-environments-app", bells_own),
    );
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(
        status(&wake_ups),
        WakeUpStatus::SignInNeeded {
            point_of_sale: till.point_of_sale,
            application_id: bells_own.into()
        }
    );
    assert!(!s.bell.calls().iter().any(|c| c.starts_with("POST")));
}

async fn a_till_moved_to_the_other_environment_takes_its_registration_back_and_registers_again_in_the_new_one_decision_306(
    till: TestTill,
) {
    let s = Scenario::new(till);
    s.bell.set_info(
        &s.monokulo_bell,
        both_environments(&s, till.other_application_id, till.application_id),
    );
    let wake_ups = s.connected().await;
    s.pos.move_to(till.other_environment);
    s.pos.sign_in(Some(till.other_application_id));
    wake_ups.recheck();
    run_current().await;
    assert_eq!(
        s.bell.calls(),
        [
            "GET bell.monokulo.com".to_owned(),
            "DELETE bell.monokulo.com br_1".to_owned(),
            format!("POST bell.monokulo.com {} https://ntfy.sh/up1", s.posted()),
        ]
    );
    assert_eq!(
        *s.bell.registered_in.lock().unwrap(),
        [till.environment, till.other_environment]
    );
    assert_eq!(
        s.registration().unwrap().environment.as_deref(),
        Some(till.other_environment)
    );
    assert_eq!(status(&wake_ups), connected(None));
}

async fn a_registration_kept_from_before_registrations_named_their_environment_is_taken_back_and_made_again_at_the_next_start_decision_306(
    till: TestTill,
) {
    let s = Scenario::new(till);
    s.store.set_choice(WakeUpChoice::MonokuloBell);
    s.store.set_registration(Some(BellRegistration {
        bell: s.monokulo_bell.clone(),
        registration_id: "br_old".into(),
        vapid_public_key: KEY.into(),
        subscription: subscription("https://ntfy.sh/up1", "BKey", "auth"),
        distributor: NTFY.into(),
        point_of_sale: None,
        environment: None,
    }));
    let wake_ups = s.start().await;
    assert_eq!(
        s.bell.calls(),
        [
            "GET bell.monokulo.com".to_owned(),
            "DELETE bell.monokulo.com br_old".to_owned(),
            format!("POST bell.monokulo.com {} https://ntfy.sh/up1", s.posted()),
        ]
    );
    assert_eq!(
        s.registration().unwrap().environment.as_deref(),
        Some(till.environment)
    );
    assert_eq!(status(&wake_ups), connected(None));
    wake_ups.recheck();
    run_current().await;
    let calls = s.bell.calls();
    assert_eq!(
        calls.iter().filter(|c| c.starts_with("DELETE")).count(),
        1,
        "one made for the till's environment is kept: {calls:?}"
    );
}

async fn a_bell_that_doesnt_run_the_tills_point_of_sale_is_refused_for_good_before_its_environment_or_application_t5_2(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let other = PointOfSale::ALL
        .into_iter()
        .find(|p| *p != till.point_of_sale)
        .unwrap();
    let info = BellInfo {
        points_of_sale: [(
            other.wire().to_owned(),
            map(&[("elsewhere", "someone-else")]),
        )]
        .into(),
        vapid_public_key: KEY.into(),
    };
    s.bell
        .set_info(&s.monokulo_bell, Arc::new(move || Ok(info.clone())));
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(
        status(&wake_ups),
        refused(&format!(
            "it doesn't ring {} tills",
            till.point_of_sale.display_name()
        ))
    );
    wait(minutes(60)).await;
    assert_eq!(
        s.bell.calls(),
        ["GET bell.monokulo.com"],
        "no retry on its own"
    );
    assert_eq!(s.push.calls(), Vec::<String>::new());
}

async fn the_environment_is_checked_before_the_application_t5_2(till: TestTill) {
    let s = Scenario::new(till);
    s.bell.set_info(
        &s.monokulo_bell,
        s.info_with(till.other_application_id, till.other_environment, KEY),
    );
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert!(
        matches!(status(&wake_ups), WakeUpStatus::Refused { .. }),
        "{:?}",
        status(&wake_ups)
    );
}

async fn an_environment_this_build_doesnt_know_is_passed_over_beside_the_tills_or_alone(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let info = BellInfo {
        points_of_sale: [(
            till.point_of_sale.wire().to_owned(),
            map(&[
                ("staging", "staging-app"),
                (till.environment, till.application_id),
            ]),
        )]
        .into(),
        vapid_public_key: KEY.into(),
    };
    s.bell
        .set_info(&s.monokulo_bell, Arc::new(move || Ok(info.clone())));
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
    s.bell
        .set_info(&s.own, s.info_with(till.application_id, "staging", KEY));
    wake_ups.choose(own(&s.own));
    run_current().await;
    assert_eq!(
        status(&wake_ups),
        refused(&pos_words::not_rung_in(
            till.point_of_sale,
            till.environment
        ))
    );
}

async fn a_bell_for_both_points_of_sale_registers_the_till_for_its_own_with_its_own_location(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let other = PointOfSale::ALL
        .into_iter()
        .find(|p| *p != till.point_of_sale)
        .unwrap();
    let info = BellInfo {
        points_of_sale: [
            (
                other.wire().to_owned(),
                map(&[("elsewhere", "someone-else")]),
            ),
            (
                till.point_of_sale.wire().to_owned(),
                map(&[(till.environment, till.application_id)]),
            ),
        ]
        .into(),
        vapid_public_key: KEY.into(),
    };
    s.bell
        .set_info(&s.monokulo_bell, Arc::new(move || Ok(info.clone())));
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
    assert_eq!(*s.bell.registered_for.lock().unwrap(), [till.point_of_sale]);
    assert_eq!(
        s.bell.calls().last().unwrap(),
        &format!("POST bell.monokulo.com {} https://ntfy.sh/up1", s.posted())
    );
}

async fn a_bell_on_another_application_asks_for_a_sign_in_through_it_and_registers_nothing_until_then_a6_26(
    till: TestTill,
) {
    let s = Scenario::new(till);
    s.bell.set_info(
        &s.monokulo_bell,
        s.info_with(till.other_application_id, till.environment, KEY),
    );
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(
        status(&wake_ups),
        WakeUpStatus::SignInNeeded {
            point_of_sale: till.point_of_sale,
            application_id: till.other_application_id.into()
        }
    );
    assert_eq!(s.bell.calls(), ["GET bell.monokulo.com"]);
    assert_eq!(
        s.push.calls(),
        Vec::<String>::new(),
        "no distributor registration"
    );
    wait(minutes(60)).await;
    assert_eq!(s.bell.calls().len(), 1, "no retry on its own");
    // The sign-in through the Bell's application finished; the app asks again.
    s.pos.sign_in(Some(till.other_application_id));
    wake_ups.recheck();
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
}

async fn without_a_sign_in_the_bell_waits_for_one_a3_14(till: TestTill) {
    let s = Scenario::new(till);
    s.pos.sign_in(None);
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(status(&wake_ups), WakeUpStatus::NoPos);
    assert_eq!(s.store.choice(), WakeUpChoice::MonokuloBell);
    assert_eq!(s.bell.calls(), ["GET bell.monokulo.com"]);
}

async fn a_sign_in_the_point_of_sale_ended_at_the_token_is_no_pos_and_takes_back_what_was_registered(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    *s.pos.token.lock().unwrap() = None;
    wake_ups.recheck();
    run_current().await;
    assert_eq!(status(&wake_ups), WakeUpStatus::NoPos);
    assert_eq!(
        s.bell.calls(),
        ["GET bell.monokulo.com", "DELETE bell.monokulo.com br_1"]
    );
    assert_eq!(s.registration(), None);
}

async fn no_distributor_is_no_distributor_with_the_choice_kept_and_one_installed_connects_by_itself_a7_29_a7_30(
    till: TestTill,
) {
    let s = Scenario::new(till);
    s.distributors.set(Distributor::None);
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(status(&wake_ups), WakeUpStatus::NoDistributor);
    assert_eq!(s.store.choice(), WakeUpChoice::MonokuloBell);
    assert_eq!(s.bell.calls(), ["GET bell.monokulo.com"], "no POST");
    s.distributors.set(Distributor::Found(NTFY.into()));
    s.distributors.changed();
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
}

async fn several_distributors_and_no_default_the_cashier_picks_and_it_is_saved_a7_31(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let installed = vec![
        "io.heckel.ntfy".to_owned(),
        "org.unifiedpush.distributor.sunup".to_owned(),
    ];
    s.distributors.set(Distributor::Choose(installed.clone()));
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(
        status(&wake_ups),
        WakeUpStatus::ChooseDistributor {
            packages: installed
        }
    );
    wake_ups.choose_distributor("org.unifiedpush.distributor.sunup");
    assert_eq!(status(&wake_ups), WakeUpStatus::Connecting);
    run_current().await;
    assert_eq!(
        *s.distributors.saved.lock().unwrap(),
        ["org.unifiedpush.distributor.sunup"]
    );
    assert_eq!(status(&wake_ups), connected(None));
}

async fn the_distributor_failing_is_unreachable_with_its_reason_retried(till: TestTill) {
    let s = Scenario::new(till);
    *s.push.answer.lock().unwrap() = Answer::Fails(PushFailure::Other);
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(status(&wake_ups), unreachable(PUSH_REFUSED));
    *s.push.answer.lock().unwrap() = Answer::Fails(PushFailure::NoKeys);
    wait(minutes(1)).await;
    assert_eq!(status(&wake_ups), unreachable(PUSH_NO_KEYS));
    *s.push.answer.lock().unwrap() = Answer::Silent;
    wait(minutes(2) + SUBSCRIBE_WAIT).await;
    assert_eq!(status(&wake_ups), unreachable(PUSH_SILENT));
    assert!(!s.bell.calls().iter().any(|c| c.starts_with("POST")));
    assert_eq!(
        push_failed_status(PushFailure::Network),
        unreachable(PUSH_OFFLINE)
    );
    assert_eq!(
        push_failed_status(PushFailure::NoDistributor),
        WakeUpStatus::NoDistributor
    );
}

async fn each_problem_type_is_its_refusal_and_only_rate_limited_retries_by_itself_a1_7_a11_5_a11_6(
    _: TestTill,
) {
    fn problem(
        status: u16,
        slug: Option<&str>,
        detail: Option<&str>,
        retry_after: Option<i64>,
        at: Option<PointOfSale>,
    ) -> WakeUpStatus {
        let problem = BellProblem {
            status,
            slug: slug.map(BellSlug::from_slug),
            detail: detail.map(str::to_owned),
            retry_after,
        };
        problem_status(&problem, at, 1000)
    }
    let square = Some(PointOfSale::Square);
    let clover = Some(PointOfSale::Clover);
    assert_eq!(
        problem(401, Some("square-refused"), None, None, square),
        refused("Square didn't confirm this location. Sign in to Square again.")
    );
    assert_eq!(
        problem(401, Some("clover-refused"), None, None, clover),
        refused("Clover didn't confirm this merchant. Sign in to Clover again.")
    );
    assert_eq!(
        problem(409, Some("too-many-registrations"), None, None, clover),
        refused("this merchant already has 32 tills registered")
    );
    assert_eq!(
        problem(409, Some("endpoint-registered"), None, None, clover),
        refused("this device is registered under another Clover account")
    );
    assert_eq!(
        problem(503, Some("clover-unavailable"), None, None, clover),
        unreachable("it answered 503")
    );
    // Another point of sale's slug is a refusal like any other, in the Bell's words.
    assert_eq!(
        problem(
            401,
            Some("clover-refused"),
            Some("Refused by Clover"),
            None,
            square
        ),
        refused("Refused by Clover")
    );
    assert_eq!(
        problem(401, Some("square-refused"), None, None, None),
        refused("it answered 401")
    );
    assert_eq!(
        problem(409, Some("too-many-registrations"), None, None, square),
        refused("this location already has 32 tills registered")
    );
    assert_eq!(
        problem(409, Some("endpoint-registered"), None, None, square),
        refused("this device is registered under another Square account")
    );
    assert_eq!(
        problem(429, Some("rate-limited"), None, Some(42), square),
        refused_until(
            "too many registrations from this network: trying again in about a minute",
            1042
        )
    );
    assert_eq!(
        problem(429, Some("rate-limited"), None, Some(600), square),
        refused_until(
            "too many registrations from this network: trying again in about 10 minutes",
            1600
        )
    );
    assert_eq!(
        [1, 90, 91, 900].map(about),
        [
            "about a minute",
            "about a minute",
            "about 2 minutes",
            "about 15 minutes"
        ]
    );
    assert_eq!(
        problem(
            400,
            Some("invalid-request"),
            Some("subscription.keys.p256dh isn't a P-256 point"),
            None,
            square
        ),
        refused("subscription.keys.p256dh isn't a P-256 point")
    );
    assert_eq!(
        problem(413, Some("payload-too-large"), None, None, square),
        refused("it answered 413")
    );
    assert_eq!(
        problem(
            503,
            Some("square-unavailable"),
            Some("Square didn't answer"),
            None,
            square
        ),
        unreachable("it answered 503")
    );
    assert_eq!(
        problem(500, Some("internal"), None, None, square),
        unreachable("it answered 500")
    );
    assert_eq!(
        problem(502, None, None, None, square),
        unreachable("it answered 502")
    );
}

async fn a_refusal_waits_for_a_re_pick_a_sign_in_or_a_start_but_rate_limited_comes_back_after_retry_after(
    till: TestTill,
) {
    let s = Scenario::new(till);
    s.bell
        .set_register(|| Err(problem(409, Some("endpoint-registered"))));
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(
        status(&wake_ups),
        refused(&format!(
            "this device is registered under another {} account",
            till.point_of_sale.display_name()
        ))
    );
    wait(minutes(60)).await;
    assert_eq!(s.bell.calls().len(), 2);
    s.bell.set_register(|| {
        Err(BellError::Problem(BellProblem {
            status: 429,
            slug: Some(BellSlug::RateLimited),
            detail: None,
            retry_after: Some(30),
        }))
    });
    wake_ups.recheck();
    run_current().await;
    let WakeUpStatus::Refused { retry_at, .. } = status(&wake_ups) else {
        panic!("{:?}", status(&wake_ups))
    };
    assert_eq!(retry_at, Some(s.now() + 30));
    s.bell.set_register(|| {
        Ok(BellRegistered {
            registration_id: "br_9".into(),
        })
    });
    wait(seconds(29)).await;
    assert_eq!(s.bell.calls().len(), 4);
    wait(seconds(1)).await;
    assert_eq!(status(&wake_ups), connected(None));
    assert_eq!(s.registration().unwrap().registration_id, "br_9");
}

async fn switching_bells_takes_the_old_registration_back_before_registering_with_the_new_a3_17(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    wake_ups.choose(own(&s.own));
    run_current().await;
    assert_eq!(
        s.bell.calls(),
        [
            "DELETE bell.monokulo.com br_1".to_owned(),
            "GET bell.thebeanroom.coffee".to_owned(),
            format!(
                "POST bell.thebeanroom.coffee {} https://ntfy.sh/up1",
                s.posted()
            ),
        ]
    );
    assert_eq!(s.registration().unwrap().bell, s.own);
    assert_eq!(status(&wake_ups), connected(None));
}

async fn timer_only_takes_the_registration_back_once_ends_the_subscription_and_keeps_my_own_bells_address_a10_1(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected_to(own(&s.own)).await;
    wake_ups.choose(WakeUpChoice::TimerOnly);
    assert_eq!(status(&wake_ups), WakeUpStatus::Off);
    run_current().await;
    assert_eq!(s.bell.calls(), ["DELETE bell.thebeanroom.coffee br_1"]);
    assert_eq!(s.push.calls(), ["unsubscribe"]);
    assert_eq!(s.registration(), None);
    assert_eq!(s.store.choice(), WakeUpChoice::TimerOnly);
    assert_eq!(s.store.own_bell(), Some(s.own.clone()));
    wake_ups.recheck();
    run_current().await;
    assert_eq!(s.bell.calls().len(), 1);
}

async fn signing_out_takes_the_registration_back_and_keeps_the_choice_awaited_a10_2(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    wake_ups.unregister(false).await;
    assert_eq!(s.bell.calls(), ["DELETE bell.monokulo.com br_1"]);
    assert_eq!(s.push.calls(), ["unsubscribe"]);
    assert_eq!(s.registration(), None);
    assert_eq!(s.store.choice(), WakeUpChoice::MonokuloBell);
    assert_eq!(status(&wake_ups), WakeUpStatus::NoPos);
    // Signed in again (same application): registers with no re-pick.
    wake_ups.recheck();
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
}

async fn a_registration_taken_back_takes_its_last_wake_up_with_it_so_the_next_one_starts_at_none_yet(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    s.push.ding();
    run_current().await;
    assert_eq!(status(&wake_ups), connected(Some(s.now())));
    // The point of sale changed (or the till signed out): the registration goes, then comes back for the new sign-in.
    wake_ups.unregister(false).await;
    wake_ups.recheck();
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
}

async fn disconnecting_takes_the_registration_back_and_forgets_the_choice_a10_3(till: TestTill) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    wake_ups.unregister(true).await;
    assert_eq!(s.bell.calls(), ["DELETE bell.monokulo.com br_1"]);
    assert_eq!(s.store.choice(), WakeUpChoice::TimerOnly);
    assert_eq!(*wake_ups.choice().borrow(), WakeUpChoice::TimerOnly);
    assert_eq!(status(&wake_ups), WakeUpStatus::Off);
    wait(minutes(30)).await;
    assert_eq!(s.bell.calls().len(), 1, "and nothing after");
}

async fn a_bell_that_cant_take_the_registration_back_is_tried_once_and_dropped_here_a10_7(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    *s.bell.unregister.lock().unwrap() = Unregister::Fails;
    wake_ups.unregister(false).await;
    assert_eq!(s.bell.calls(), ["DELETE bell.monokulo.com br_1"]);
    assert_eq!(s.registration(), None);
}

async fn a_bell_that_never_answers_the_delete_holds_the_sign_out_10_s_at_most_a10_7(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    *s.bell.unregister.lock().unwrap() = Unregister::NeverAnswers;
    let started = Instant::now();
    wake_ups.unregister(false).await;
    assert_eq!(started.elapsed(), seconds(10));
    assert_eq!(s.registration(), None);
    assert_eq!(status(&wake_ups), WakeUpStatus::NoPos);
}

async fn unregistering_stops_a_registration_under_way_and_takes_back_what_it_got(till: TestTill) {
    let s = Scenario::new(till);
    let wake_ups = s.start().await;
    let held = Arc::new(AtomicBool::new(false));
    let registered = held.clone();
    s.bell.set_register(move || {
        registered.store(true, Ordering::SeqCst);
        Ok(BellRegistered {
            registration_id: "br_1".into(),
        })
    });
    *s.push.answer.lock().unwrap() = Answer::Silent;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(
        status(&wake_ups),
        WakeUpStatus::Connecting,
        "waiting on the distributor"
    );
    wake_ups.unregister(true).await;
    assert!(!s.bell.calls().iter().any(|c| c.starts_with("POST")));
    assert_eq!(status(&wake_ups), WakeUpStatus::Off);
    assert!(!held.load(Ordering::SeqCst));
}

async fn every_app_start_registers_again_with_the_same_subscription_a4_21_a3_19(till: TestTill) {
    let s = Scenario::new(till);
    s.connected().await;
    let second = s.start().await;
    assert_eq!(*second.choice().borrow(), WakeUpChoice::MonokuloBell);
    assert_eq!(
        s.bell.calls(),
        [
            "GET bell.monokulo.com".to_owned(),
            format!("POST bell.monokulo.com {} https://ntfy.sh/up1", s.posted())
        ]
    );
    assert_eq!(
        s.push.calls(),
        [format!("subscribe {KEY}")],
        "the connector asked again, as its docs say"
    );
    assert_eq!(status(&second), connected(None));
}

async fn a_shift_starting_registers_again_without_leaving_connected_so_the_pace_holds_a4_21(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    let mark = mark(&wake_ups);
    wake_ups.recheck();
    run_current().await;
    assert_eq!(
        s.bell.calls(),
        [
            "GET bell.monokulo.com".to_owned(),
            format!("POST bell.monokulo.com {} https://ntfy.sh/up1", s.posted())
        ]
    );
    assert_eq!(
        s.push.calls(),
        Vec::<String>::new(),
        "the subscription this run made is still good"
    );
    let seen = seen_since(&wake_ups, mark);
    assert!(
        seen.iter()
            .all(|s| matches!(s, WakeUpStatus::Connected { .. })),
        "{seen:?}"
    );
}

async fn an_app_start_with_timer_only_finishes_taking_back_a_registration_left_behind(
    till: TestTill,
) {
    let s = Scenario::new(till);
    s.store.set_registration(Some(BellRegistration {
        bell: s.monokulo_bell.clone(),
        registration_id: "br_old".into(),
        vapid_public_key: KEY.into(),
        subscription: subscription("https://ntfy.sh/up0", "BKey", "auth"),
        distributor: NTFY.into(),
        point_of_sale: None,
        environment: None,
    }));
    let wake_ups = s.start().await;
    assert_eq!(s.bell.calls(), ["DELETE bell.monokulo.com br_old"]);
    assert_eq!(s.registration(), None);
    assert_eq!(status(&wake_ups), WakeUpStatus::Off);
}

async fn a_new_endpoint_from_the_distributor_deletes_the_old_registration_then_registers_the_new_a7_32(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    s.push.set_state(PushState::Subscribed(subscription(
        "https://ntfy.sh/up2",
        "BKey2",
        "auth2",
    )));
    run_current().await;
    assert_eq!(
        s.bell.calls(),
        [
            "GET bell.monokulo.com".to_owned(),
            "DELETE bell.monokulo.com br_1".to_owned(),
            format!("POST bell.monokulo.com {} https://ntfy.sh/up2", s.posted()),
        ]
    );
    assert_eq!(
        s.registration().unwrap().subscription.endpoint,
        "https://ntfy.sh/up2"
    );
    assert_eq!(status(&wake_ups), connected(None));
}

async fn an_endpoint_changed_while_the_app_was_away_is_caught_at_the_next_start(till: TestTill) {
    let s = Scenario::new(till);
    s.connected().await;
    *s.push.endpoint.lock().unwrap() = "https://ntfy.sh/up2".into();
    s.start().await;
    assert_eq!(
        s.bell.calls(),
        [
            "GET bell.monokulo.com".to_owned(),
            "DELETE bell.monokulo.com br_1".to_owned(),
            format!("POST bell.monokulo.com {} https://ntfy.sh/up2", s.posted()),
        ]
    );
}

async fn the_distributor_ending_the_subscription_takes_the_registration_back_and_is_no_distributor_a7_32(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    s.push.set_state(PushState::Failed(PushFailure::Lost));
    run_current().await;
    assert_eq!(s.bell.calls(), ["DELETE bell.monokulo.com br_1"]);
    assert_eq!(s.registration(), None);
    assert_eq!(status(&wake_ups), WakeUpStatus::NoDistributor);
}

async fn the_distributor_uninstalled_takes_the_registration_back_and_one_installed_again_connects_a7_32_a7_30(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    s.distributors.set(Distributor::None);
    s.distributors.changed();
    run_current().await;
    assert_eq!(status(&wake_ups), WakeUpStatus::NoDistributor);
    assert_eq!(
        s.bell.calls(),
        ["GET bell.monokulo.com", "DELETE bell.monokulo.com br_1"]
    );
    assert_eq!(s.push.calls(), ["unsubscribe"]);
    s.distributors.set(Distributor::Found(NTFY.into()));
    s.distributors.changed();
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
}

async fn the_distributor_uninstalled_with_two_others_left_takes_the_registration_back_before_the_cashier_picks_decision_108(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    let left = vec![
        "org.unifiedpush.distributor.nextpush".to_owned(),
        "org.unifiedpush.distributor.sunup".to_owned(),
    ];
    s.distributors.set(Distributor::Choose(left.clone()));
    s.distributors.changed();
    run_current().await;
    assert_eq!(
        s.bell.calls(),
        ["GET bell.monokulo.com", "DELETE bell.monokulo.com br_1"]
    );
    assert_eq!(s.registration(), None);
    assert_eq!(
        status(&wake_ups),
        WakeUpStatus::ChooseDistributor { packages: left }
    );
}

/// The used distributor uninstalled with [`NEXTPUSH`] left: the change of apps, and the connector's end of the
/// subscription in `end_first` order.
async fn uninstalled_with_another_left(till: TestTill, end_first: bool) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    let mark = mark(&wake_ups);
    s.distributors.set(Distributor::Found(NEXTPUSH.into()));
    if end_first {
        s.push.set_state(PushState::Failed(PushFailure::Lost));
    }
    s.distributors.changed();
    if !end_first {
        s.push.set_state(PushState::Failed(PushFailure::Lost));
    }
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
    let seen = seen_since(&wake_ups, mark);
    assert!(!seen.contains(&WakeUpStatus::NoDistributor), "{seen:?}");
    assert!(s.registration().is_some());
    assert_eq!(
        s.push.state_now(),
        PushState::Subscribed(s.push.subscription())
    );
    let calls = s.bell.calls();
    assert!(
        calls.last().unwrap().starts_with("POST bell.monokulo.com"),
        "{calls:?}"
    );
    wait(minutes(30)).await;
    assert_eq!(status(&wake_ups), connected(None));
}

async fn the_distributor_uninstalled_with_another_left_connects_through_it_when_the_connectors_end_comes_after_decision_118(
    till: TestTill,
) {
    uninstalled_with_another_left(till, false).await;
}

async fn the_distributor_uninstalled_with_another_left_connects_through_it_when_the_connectors_end_comes_first_decision_118(
    till: TestTill,
) {
    uninstalled_with_another_left(till, true).await;
}

async fn the_distributor_uninstalled_with_two_left_and_the_connectors_end_arriving_asks_the_cashier_never_no_distributor_decision_118(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    let left = vec![
        NEXTPUSH.to_owned(),
        "org.unifiedpush.distributor.sunup".to_owned(),
    ];
    let mark = mark(&wake_ups);
    s.distributors.set(Distributor::Choose(left.clone()));
    s.distributors.changed();
    s.push.set_state(PushState::Failed(PushFailure::Lost));
    run_current().await;
    assert_eq!(
        status(&wake_ups),
        WakeUpStatus::ChooseDistributor { packages: left }
    );
    let seen = seen_since(&wake_ups, mark);
    assert!(!seen.contains(&WakeUpStatus::NoDistributor), "{seen:?}");
    assert_eq!(s.registration(), None);
}

async fn a_distributor_installed_after_the_last_one_went_keeps_its_own_end_of_the_subscription_read_as_the_cashiers_decision_118(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    s.distributors.set(Distributor::None);
    s.distributors.changed();
    run_current().await;
    assert_eq!(status(&wake_ups), WakeUpStatus::NoDistributor);
    s.distributors.set(Distributor::Found(NEXTPUSH.into()));
    *s.push.endpoint.lock().unwrap() = "https://push.example/np1".into();
    s.distributors.changed();
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
    assert_eq!(s.registration().unwrap().distributor, NEXTPUSH);
    s.bell.calls.lock().unwrap().clear();
    s.push.set_state(PushState::Failed(PushFailure::Lost));
    run_current().await;
    assert_eq!(status(&wake_ups), WakeUpStatus::NoDistributor);
    assert_eq!(s.bell.calls(), ["DELETE bell.monokulo.com br_2"]);
    wait(minutes(30)).await;
    let calls = s.bell.calls();
    assert!(!calls.iter().any(|c| c.starts_with("POST")), "{calls:?}");
}

async fn a_distributor_uninstalled_while_the_app_was_away_is_known_at_the_start_from_the_registration_decision_122(
    till: TestTill,
) {
    let s = Scenario::new(till);
    s.connected().await;
    assert_eq!(s.registration().unwrap().distributor, NTFY);
    s.distributors.set(Distributor::Found(NEXTPUSH.into()));
    *s.push.endpoint.lock().unwrap() = "https://push.example/np1".into();
    let wake_ups = s.start().await;
    let mark = mark(&wake_ups);
    assert_eq!(status(&wake_ups), connected(None));
    assert_eq!(s.registration().unwrap().distributor, NEXTPUSH);
    s.bell.calls.lock().unwrap().clear();
    // The connector's end of the old distributor's token, broadcast by the start's look.
    s.push.set_state(PushState::Failed(PushFailure::Lost));
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
    let seen = seen_since(&wake_ups, mark);
    assert!(!seen.contains(&WakeUpStatus::NoDistributor), "{seen:?}");
    assert_eq!(
        s.bell.calls(),
        [
            "DELETE bell.monokulo.com br_2".to_owned(),
            "GET bell.monokulo.com".to_owned(),
            format!(
                "POST bell.monokulo.com {} https://push.example/np1",
                s.posted()
            ),
        ]
    );
    assert_eq!(s.registration().unwrap().distributor, NEXTPUSH);
}

async fn a_pass_failing_on_this_device_for_any_reason_leaves_the_till_running_and_retries_decision_120(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let failing: InfoAnswer =
        Arc::new(|| Err(DeviceError("the store couldn't be read".into()).into()));
    s.bell.set_info(&s.monokulo_bell, failing.clone());
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(status(&wake_ups), unreachable(TILL_FAILED));
    assert!(wake_ups.is_running());
    s.bell.set_info(&s.monokulo_bell, s.info());
    wait(minutes(1)).await;
    assert_eq!(status(&wake_ups), connected(None));
    s.bell.set_info(&s.own, failing);
    wake_ups.choose(own(&s.own));
    run_current().await;
    assert_eq!(status(&wake_ups), unreachable(TILL_FAILED));
    s.bell.set_info(&s.own, s.info());
    wake_ups.choose(own(&s.own));
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
}

async fn an_app_installed_that_isnt_the_distributor_changes_nothing(till: TestTill) {
    let s = Scenario::new(till);
    s.connected().await;
    s.distributors.changed();
    run_current().await;
    assert_eq!(s.bell.calls(), Vec::<String>::new());
}

async fn a_rotated_vapid_key_subscribes_again_for_it_then_registers_the_new_subscription_a9_8(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    s.bell.set_info(
        &s.monokulo_bell,
        s.info_with(till.application_id, till.environment, "BNewKey"),
    );
    *s.push.endpoint.lock().unwrap() = "https://ntfy.sh/up2".into();
    wake_ups.recheck();
    run_current().await;
    assert_eq!(s.push.calls(), ["unsubscribe", "subscribe BNewKey"]);
    assert_eq!(
        s.bell.calls(),
        [
            "GET bell.monokulo.com".to_owned(),
            "DELETE bell.monokulo.com br_1".to_owned(),
            format!("POST bell.monokulo.com {} https://ntfy.sh/up2", s.posted()),
        ]
    );
    assert_eq!(s.registration().unwrap().vapid_public_key, "BNewKey");
}

async fn a_rotated_key_is_caught_at_an_app_start_too(till: TestTill) {
    let s = Scenario::new(till);
    s.connected().await;
    s.bell.set_info(
        &s.monokulo_bell,
        s.info_with(till.application_id, till.environment, "BNewKey"),
    );
    s.start().await;
    assert_eq!(s.push.calls(), ["unsubscribe", "subscribe BNewKey"]);
}

async fn a_timer_found_sale_with_no_ding_in_60_s_is_a_missed_wake_up_2_s_and_one_registration_again_a11_3(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    wait(seconds(61)).await;
    wake_ups.timer_found(1);
    assert_eq!(status(&wake_ups), missing(None));
    assert_eq!(status(&wake_ups).interval(), seconds(2));
    run_current().await;
    assert_eq!(
        s.bell.calls(),
        [
            "GET bell.monokulo.com".to_owned(),
            format!("POST bell.monokulo.com {} https://ntfy.sh/up1", s.posted())
        ]
    );
    assert_eq!(
        status(&wake_ups),
        missing(None),
        "registering again proves nothing"
    );
    wake_ups.timer_found(2);
    run_current().await;
    assert_eq!(s.bell.calls().len(), 2, "once");
    wait(seconds(5)).await;
    s.push.ding();
    run_current().await;
    assert_eq!(status(&wake_ups), connected(Some(s.now())));
    assert_eq!(status(&wake_ups).interval(), seconds(30));
}

async fn a_ding_then_a_timer_found_sale_stays_connected_a11_3(till: TestTill) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    wait(minutes(5)).await;
    s.push.ding();
    run_current().await;
    let at = s.now();
    assert_eq!(status(&wake_ups), connected(Some(at)), "Last wake-up");
    wait(seconds(30)).await;
    wake_ups.timer_found(1);
    run_current().await;
    assert_eq!(status(&wake_ups), connected(Some(at)));
    assert_eq!(s.bell.calls(), Vec::<String>::new());
    wait(seconds(31)).await;
    wake_ups.timer_found(1);
    assert_eq!(status(&wake_ups), missing(Some(at)), "61 s after the ding");
}

async fn a_timer_check_that_finds_nothing_or_one_just_after_connecting_proves_nothing(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wake_ups = s.connected().await;
    wait(seconds(30)).await;
    wake_ups.timer_found(1);
    assert_eq!(
        status(&wake_ups),
        connected(None),
        "a sale from before the Bell was connected"
    );
    wait(minutes(10)).await;
    wake_ups.timer_found(0);
    assert_eq!(status(&wake_ups), connected(None));
}

async fn a_timer_found_sale_while_not_connected_changes_nothing(till: TestTill) {
    let s = Scenario::new(till);
    s.bell.infos.lock().unwrap().remove(&s.monokulo_bell);
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    wait(seconds(10)).await;
    wake_ups.timer_found(3);
    assert_eq!(status(&wake_ups), unreachable("no connection"));
}

async fn an_endpoint_the_distributor_gives_after_the_pass_stopped_waiting_registers_at_once_decision_123(
    till: TestTill,
) {
    let s = Scenario::new(till);
    *s.push.answer.lock().unwrap() = Answer::Silent;
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    wait(SUBSCRIBE_WAIT).await;
    assert_eq!(status(&wake_ups), unreachable(PUSH_SILENT));
    assert_eq!(s.bell.calls(), ["GET bell.monokulo.com"]);
    // The distributor, opened at last, answers the registration it was sent.
    s.push
        .set_state(PushState::Subscribed(s.push.subscription()));
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
    assert_eq!(
        s.bell.calls(),
        [
            "GET bell.monokulo.com".to_owned(),
            "GET bell.monokulo.com".to_owned(),
            format!("POST bell.monokulo.com {} https://ntfy.sh/up1", s.posted()),
        ]
    );
    assert_eq!(
        s.push.calls(),
        [format!("subscribe {KEY}")],
        "the subscription held is the one registered"
    );
    // The wait that was set is gone with it: nothing more is asked of the Bell.
    wait(minutes(20)).await;
    assert_eq!(s.bell.calls().len(), 3);
}

async fn an_unreachable_bell_is_tried_again_on_recheck_before_its_wait_is_out_decision_123(
    till: TestTill,
) {
    let s = Scenario::new(till);
    *s.push.answer.lock().unwrap() = Answer::Silent;
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    wait(SUBSCRIBE_WAIT).await;
    assert_eq!(status(&wake_ups), unreachable(PUSH_SILENT));
    *s.push.answer.lock().unwrap() = Answer::Subscribed;
    wait(seconds(5)).await;
    wake_ups.recheck();
    run_current().await;
    assert_eq!(status(&wake_ups), connected(None));
    assert_eq!(
        s.bell
            .calls()
            .iter()
            .filter(|c| c.starts_with("POST"))
            .count(),
        1
    );
}

async fn a_ding_while_not_connected_changes_no_status(till: TestTill) {
    let s = Scenario::new(till);
    s.pos.sign_in(None);
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    s.push.ding();
    run_current().await;
    assert_eq!(status(&wake_ups), WakeUpStatus::NoPos);
}

async fn the_bells_refusals_and_outages_for_the_tills_point_of_sale_are_its_words_t5_4(
    till: TestTill,
) {
    let s = Scenario::new(till);
    let wire = till.point_of_sale.wire();
    let refused_slug = format!("{wire}-refused");
    s.bell
        .set_register(move || Err(problem(401, Some(&refused_slug))));
    let wake_ups = s.start().await;
    wake_ups.choose(WakeUpChoice::MonokuloBell);
    run_current().await;
    assert_eq!(
        status(&wake_ups),
        refused(&pos_words::refused(till.point_of_sale))
    );
    let unavailable_slug = format!("{wire}-unavailable");
    s.bell
        .set_register(move || Err(problem(503, Some(&unavailable_slug))));
    wake_ups.recheck();
    run_current().await;
    assert_eq!(status(&wake_ups), unreachable("it answered 503"));
    s.bell.set_register(|| {
        Ok(BellRegistered {
            registration_id: "br_7".into(),
        })
    });
    wait(minutes(1)).await;
    assert_eq!(
        status(&wake_ups),
        connected(None),
        "an outage is retried by itself"
    );
}
