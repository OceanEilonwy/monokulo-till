//! The engine against a platform with its secrets in memory and stand-ins
//! for Monokulo and Square: what it publishes, what it keeps under
//! `Stores.kt`'s keys, and what it asks the platform to do.

use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use monokulo_client::FixedClock;
use till_screens::onboarding::OnboardingPage;
use till_screens::shift::ShiftFilter;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::testing::{FakePlatform, FakePush, Heard, Recorder, wait_for};
use crate::{
    Action, CloseChoice, CloverSetup, Device, Effect, Engine, EngineConfig, Screen, SquareSetup,
    Update, WakeUps,
};

use super::Options;

/// 2026-10-22T16:08:00Z: a minute after the stand-in's Monero payment.
const NOW: i64 = 1_761_149_280;

const TOKEN: &str = "ct_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn config(square: Option<&MockServer>) -> EngineConfig {
    EngineConfig {
        data_dir: std::env::temp_dir().to_string_lossy().into_owned(),
        device: Device::Iphone,
        device_name: "iPhone 17".into(),
        app_version: "0.1.0".into(),
        platform: "ios".into(),
        debug: true,
        utc_offset: 0,
        hours_24: false,
        square: SquareSetup {
            application_id: "sq0idp-till".into(),
            sandbox: false,
            redirect_uri: "https://till.monokulo.com/square".into(),
            base_url: square.map(MockServer::uri),
            return_uri: None,
        },
        clover: CloverSetup {
            application_id: "clv-till".into(),
            environment: "sandbox".into(),
            redirect_uri: "https://till.monokulo.com/clover".into(),
            base_url: None,
            return_uri: None,
        },
        bell_url: "https://bell.monokulo.com".into(),
    }
}

/// A client that keeps no idle connections (decision 32).
fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .unwrap()
}

fn start(
    config: EngineConfig,
    platform: Arc<FakePlatform>,
) -> (Arc<Engine>, Receiver<Heard>, Arc<Recorder>) {
    let (recorder, rx) = Recorder::new();
    let engine = Engine::start_with(
        config,
        platform,
        Arc::new(FakePush::default()),
        recorder.clone(),
        Options {
            clock: Some(Arc::new(FixedClock::at(NOW))),
            http: Some(http()),
        },
    );
    (engine, rx, recorder)
}

fn screen(rx: &Receiver<Heard>, wanted: Screen) {
    wait_for(rx, |heard| match heard {
        Heard::Update(Update::Screen { screen }) if *screen == wanted => Some(()),
        _ => None,
    });
}

fn problem(rx: &Receiver<Heard>) -> String {
    wait_for(rx, |heard| match heard {
        Heard::Update(Update::Work {
            problem: Some(problem),
            ..
        }) => Some(problem.clone()),
        _ => None,
    })
}

fn json(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_owned(), "application/json")
}

/// Monokulo's entry and one order, far from running out.
async fn monokulo() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"_links":{"self":{"href":"/api/v1"}},"_templates":{"create-order":{"method":"POST","target":"/api/v1/orders","properties":[]}}}"#,
            "application/prs.hal-forms+json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/orders"))
        .respond_with(ResponseTemplate::new(201).set_body_raw(
            r#"{"order_id":"ord_1","status":"pending","is_final":false,"address":"8Bx","xmr_amount_piconero":80000000000,
               "amount":"13.88","currency":"USD","merchant_order_id":"bP9m","expires_at":1893456000,
               "_links":{"self":{"href":"/api/v1/orders/ord_1"}}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    server
}

pub(crate) const PAYMENTS: &str = r#"{"payments":[{"id":"pay_monero","created_at":"2026-10-22T16:07:00.512Z","status":"COMPLETED","source_type":"EXTERNAL","amount_money":{"amount":1388,"currency":"USD"},"location_id":"L1","order_id":"ord_sq_1","receipt_number":"bP9m","external_details":{"type":"OTHER","source":"Monero"}}]}"#;

/// Square, with one Monero payment at the location.
async fn square() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/payments"))
        .respond_with(json(PAYMENTS))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/refunds"))
        .respond_with(json(r#"{"refund":{"id":"r1","status":"PENDING"}}"#))
        .mount(&server)
        .await;
    server
}

/// What an Android till signed in to Square at "Front counter", through
/// setup, kept: in `Stores.kt`'s keys.
pub(crate) fn signed_in_till(monokulo: &MockServer) -> Arc<FakePlatform> {
    let connection = format!(r#"{{"app_key":"ak_1","api":"{}/api/v1"}}"#, monokulo.uri());
    let platform = FakePlatform::with(&[
        ("setup.done", "true"),
        ("pos.kind", "square"),
        (
            "square.location",
            r#"{"id":"L1","name":"Front counter","country":"US","currency":"USD"}"#,
        ),
        (
            "square.tokens",
            r#"{"access_token":"EAAA1","expires_at":"2030-01-01T00:00:00Z","refresh_token":"r1","merchant_id":"M1"}"#,
        ),
        ("square.since", "2026-10-22T16:06:00Z"),
        ("square.handled", "[]"),
        ("shift.started", "1761148000"),
    ]);
    platform.set("connection", &connection);
    platform
}

#[test]
fn a_new_till_publishes_its_version_then_welcome() {
    let (_engine, rx, _) = start(config(None), Arc::new(FakePlatform::default()));
    let version = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Core { version }) => Some(version.clone()),
        _ => None,
    });
    assert_eq!(version, crate::CORE_VERSION);
    screen(&rx, Screen::Welcome);
    let page = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Onboarding { view }) => Some(view.page.clone()),
        _ => None,
    });
    assert!(matches!(page, OnboardingPage::Welcome { .. }));
    let view = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::WakeUps { view }) => Some(view.clone()),
        _ => None,
    });
    assert!(view.timer.selected);
}

#[test]
fn each_slice_is_published_only_when_it_changes_and_all_again_when_opened() {
    let (engine, rx, _) = start(config(None), Arc::new(FakePlatform::default()));
    screen(&rx, Screen::Welcome);
    wait_for(&rx, |heard| {
        matches!(heard, Heard::Update(Update::WakeUps { .. })).then_some(())
    });
    // Backgrounded changes no slice.
    engine.send(Action::Backgrounded);
    engine.send(Action::Next);
    screen(&rx, Screen::Scan);
    let heard: Vec<Heard> =
        std::iter::from_fn(|| rx.recv_timeout(Duration::from_millis(300)).ok()).collect();
    assert!(
        heard.iter().all(|h| !matches!(
            h,
            Heard::Update(Update::Core { .. } | Update::WakeUps { .. })
        )),
        "{heard:?}"
    );
    engine.send(Action::Opened);
    wait_for(&rx, |heard| {
        matches!(heard, Heard::Update(Update::Core { .. })).then_some(())
    });
    screen(&rx, Screen::Scan);
}

#[test]
fn a_store_that_cant_answer_says_so_and_is_read_again_when_opened() {
    let platform = Arc::new(FakePlatform::default());
    platform.fail(true);
    let (engine, rx, recorder) = start(config(None), platform.clone());
    assert!(problem(&rx).starts_with("Couldn't read what this till keeps"));
    engine.send(Action::Next);
    platform.fail(false);
    engine.send(Action::Opened);
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Work { problem: None, .. }) => Some(()),
        _ => None,
    });
    wait_for(&rx, |heard| {
        matches!(heard, Heard::Update(Update::WakeUps { .. })).then_some(())
    });
    assert!(
        recorder
            .1
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("waits"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn connecting_keeps_the_connection_in_stores_kts_key() {
    let monokulo = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/apps/connect"))
        .respond_with(json(r#"{"app_key":"ak_x","api":"/api/v1"}"#))
        .mount(&monokulo)
        .await;
    let platform = Arc::new(FakePlatform::default());
    let (engine, rx, _) = start(config(None), platform.clone());
    engine.send(Action::Next);
    screen(&rx, Screen::Scan);
    engine.send(Action::ConnectLink {
        url: "https://pay.example/shop".into(),
    });
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Onboarding { view }) => match &view.page {
            OnboardingPage::Scan { problem: Some(_) } => Some(()),
            _ => None,
        },
        _ => None,
    });
    engine.send(Action::ConnectLink {
        url: format!("{}/apps/connect#token={TOKEN}", monokulo.uri()),
    });
    screen(&rx, Screen::Confirm);
    engine.send(Action::Connect);
    screen(&rx, Screen::Connected);
    assert_eq!(
        platform.get("connection"),
        Some(format!(
            r#"{{"app_key":"ak_x","api":"{}/api/v1"}}"#,
            monokulo.uri()
        ))
    );
    engine.send(Action::Next);
    screen(&rx, Screen::PointOfSale);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sign_in_opens_its_page_and_keeps_the_attempt_as_stores_kt_did() {
    let monokulo = monokulo().await;
    let square = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(json(
            r#"{"access_token":"EAAA9","expires_at":"2030-01-01T00:00:00Z","refresh_token":"r9","merchant_id":"M1"}"#,
        ))
        .mount(&square)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/locations"))
        .respond_with(json(
            r#"{"locations":[{"id":"L1","name":"Front counter","country":"US","currency":"USD","status":"ACTIVE"}]}"#,
        ))
        .mount(&square)
        .await;
    let platform = FakePlatform::default();
    platform.set(
        "connection",
        &format!(r#"{{"app_key":"ak_1","api":"{}/api/v1"}}"#, monokulo.uri()),
    );
    let platform = Arc::new(platform);
    let (engine, rx, _) = start(config(Some(&square)), platform.clone());
    screen(&rx, Screen::PointOfSale);

    engine.send(Action::ChoosePointOfSale {
        pos: crate::Pos::Square,
    });
    wait_for(&rx, |_| platform.get("pos.kind").filter(|k| k == "square"));
    engine.send(Action::SignIn {
        application_id: None,
    });
    let url = wait_for(&rx, |heard| match heard {
        Heard::Effect(Effect::OpenUrl { url }) => Some(url.clone()),
        _ => None,
    });
    assert!(
        url.starts_with(&format!(
            "{}/oauth2/authorize?client_id=sq0idp-till",
            square.uri()
        )),
        "{url}"
    );
    let pending: serde_json::Value =
        serde_json::from_str(&platform.get("square.pending").unwrap()).unwrap();
    assert_eq!(pending["pointOfSale"], "square");
    assert_eq!(
        pending.get("applicationId"),
        None,
        "the build's own application"
    );

    engine.send(Action::SignInReturned {
        url: "https://till.monokulo.com/clover?code=c&state=s&merchant_id=MID1".into(),
    });
    assert_eq!(
        problem(&rx),
        "That Clover sign-in wasn't started here. Try again."
    );
    engine.send(Action::SignInReturned {
        url: "https://till.monokulo.com/square?code=c&state=another".into(),
    });
    assert_eq!(
        problem(&rx),
        "That sign-in wasn't the one this app started. Try again."
    );

    let state = pending["state"].as_str().unwrap();
    engine.send(Action::SignInReturned {
        url: format!("https://till.monokulo.com/square?code=c1&state={state}"),
    });
    wait_for(&rx, |_| platform.get("square.location"));
    assert!(platform.get("square.tokens").unwrap().contains("EAAA9"));
    assert_eq!(platform.get("square.pending"), None);
    assert_eq!(
        engine.peek(|s| s.screen.clone()),
        Screen::PointOfSale,
        "setup goes on to the test sale"
    );
}

/// Decision 57: a debug build's sign-in comes back on its own scheme, and
/// the token exchange still names the redirect Square knows.
#[tokio::test(flavor = "multi_thread")]
async fn a_sign_in_back_on_the_builds_return_link_is_taken_and_exchanged_for_the_redirect() {
    let monokulo = monokulo().await;
    let square = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(json(
            r#"{"access_token":"EAAA9","expires_at":"2030-01-01T00:00:00Z","refresh_token":"r9","merchant_id":"M1"}"#,
        ))
        .mount(&square)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/locations"))
        .respond_with(json(
            r#"{"locations":[{"id":"L1","name":"Front counter","country":"US","currency":"USD","status":"ACTIVE"}]}"#,
        ))
        .mount(&square)
        .await;
    let platform = FakePlatform::with(&[
        ("pos.kind", "square"),
        (
            "square.pending",
            r#"{"verifier":"v-1","state":"s-1","pointOfSale":"square"}"#,
        ),
    ]);
    platform.set(
        "connection",
        &format!(r#"{{"app_key":"ak_1","api":"{}/api/v1"}}"#, monokulo.uri()),
    );
    let mut config = config(Some(&square));
    config.square.return_uri = Some("monokulo-till-debug://square".into());
    config.clover.return_uri = Some("monokulo-till-debug://clover".into());
    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::PointOfSale);

    engine.send(Action::SignInReturned {
        url: "monokulo-till-debug://clover?code=c&state=s-1&merchant_id=MID1".into(),
    });
    assert_eq!(
        problem(&rx),
        "That Clover sign-in wasn't started here. Try again.",
        "Clover's return link is Clover's"
    );
    engine.send(Action::SignInReturned {
        url: "monokulo-till-debug://square?code=c1&state=s-1".into(),
    });
    wait_for(&rx, |_| platform.get("square.location"));
    assert!(platform.get("square.tokens").unwrap().contains("EAAA9"));
    let exchange = square
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.url.path() == "/oauth2/token")
        .expect("the code exchanged");
    let body: serde_json::Value = serde_json::from_slice(&exchange.body).unwrap();
    assert_eq!(body["redirect_uri"], "https://till.monokulo.com/square");
    assert_eq!(body["code"], "c1");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upgraded_till_runs_its_shift_makes_orders_and_tells_the_platform() {
    let monokulo = monokulo().await;
    let square = square().await;
    let platform = signed_in_till(&monokulo);
    // A sale whose code was up when the app was last ended: told of then.
    platform.set(
        "shift.sales",
        r#"[{"orderId":"ord_0","amount":"5.00","currency":"USD","code":{"uri":"monero:8Cy","hasReceipt":false},"phase":"Waiting","expiresAt":1893456000,"xmr":"0.03","address":"8Cy"}]"#,
    );
    let (engine, rx, _) = start(config(Some(&square)), platform.clone());
    // In the back: the new sale is alerted.
    engine.send(Action::Backgrounded);
    screen(&rx, Screen::Till);
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Service { running: true }) => Some(()),
        _ => None,
    });
    // The shift lists the kept sale and the new one, at the till's place,
    // published with the alert.
    let mut shift = None;
    let alert = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Shift { view }) if rows(view) == 2 => {
            shift = Some(view.clone());
            None
        }
        Heard::Update(Update::Notification { view }) => view.alert.clone(),
        _ => None,
    });
    let shift = shift.expect("the shift with both sales");
    assert_eq!(shift.top.place.as_deref(), Some("Front counter"));
    assert_eq!(alert.order_id, "ord_1");
    assert_eq!(alert.title, "New Monero sale · $13.88");
    assert_eq!(
        alert.body,
        "Front counter. Tap here to show the payment code."
    );
    assert!(
        platform
            .get("shift.sales")
            .unwrap()
            .contains(r#""orderId":"ord_1""#)
    );
    assert_eq!(
        platform.get("square.handled").as_deref(),
        Some(r#"["pay_monero"]"#)
    );

    engine.send(Action::CopyAddress {
        order_id: "ord_1".into(),
    });
    let mut after = None;
    let copied = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Notification { view }) => {
            after = Some(view.clone());
            None
        }
        Heard::Effect(Effect::CopyText { text }) => Some(text.clone()),
        _ => None,
    });
    assert_eq!(copied, "8Bx");
    // The next change goes without the alert: it was posted once.
    if let Some(after) = after {
        assert_eq!(after.alert, None, "alerted once");
    }

    // The close sheet's "No": the till refunds it in Square and closes it.
    engine.send(Action::Close {
        order_id: "ord_1".into(),
        choice: CloseChoice::RefundIt,
    });
    for _ in 0..100 {
        let refunds = square
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.url.path() == "/v2/refunds")
            .count();
        if refunds == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let refund = square
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.url.path() == "/v2/refunds")
        .expect("a refund asked of Square");
    let body = String::from_utf8(refund.body).unwrap();
    assert!(body.contains(r#""payment_id":"pay_monero""#), "{body}");

    engine.send(Action::StopShift);
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Service { running: false }) => Some(()),
        _ => None,
    });
    assert_eq!(platform.get("shift.on").as_deref(), Some("false"));
}

/// The Live Activity's state as the engine publishes it next.
fn live_activity(rx: &Receiver<Heard>) -> Option<till_screens::LiveActivityState> {
    wait_for(rx, |heard| match heard {
        Heard::Update(Update::LiveActivity { state }) => Some(state.clone()),
        _ => None,
    })
}

/// Decision 40: on iPhone the Live Activity follows the till's own sales,
/// carries the code only with "Show code in island" on, and goes with the
/// switch.
#[tokio::test(flavor = "multi_thread")]
async fn the_live_activity_follows_the_tills_own_sale_and_the_islands_switches() {
    use till_screens::live_activity::ActivityPhase;
    let monokulo = monokulo().await;
    let square = square().await;
    let (engine, rx, _) = start(config(Some(&square)), signed_in_till(&monokulo));
    let waiting = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::LiveActivity { state: Some(state) })
            if state.phase == ActivityPhase::Waiting =>
        {
            Some(state.clone())
        }
        _ => None,
    });
    assert_eq!(waiting.sale.as_deref(), Some("#bP9m"));
    assert_eq!(waiting.amount.as_deref(), Some("13.88"));
    assert_eq!(waiting.waiting_count, 1);
    assert_eq!(waiting.payment_uri, None, "the code stays in the app");

    engine.send(Action::SetCodeInIsland { on: true });
    let with_code = live_activity(&rx).unwrap();
    assert_eq!(
        with_code.payment_uri.as_deref(),
        Some("monero:8Bx?tx_amount=0.08")
    );
    engine.send(Action::SetLiveActivity { on: false });
    assert_eq!(live_activity(&rx), None, "no activity with the switch off");
}

/// Android's ongoing notification alerts a sale that arrives while the app
/// is in the back, in one update, and has no Live Activity.
#[tokio::test(flavor = "multi_thread")]
async fn androids_ongoing_notification_alerts_a_new_sale_once() {
    let monokulo = monokulo().await;
    let square = square().await;
    let android = EngineConfig {
        device: Device::Android,
        ..config(Some(&square))
    };
    let (engine, rx, _) = start(android, signed_in_till(&monokulo));
    engine.send(Action::Backgrounded);
    let (face, alert) = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::LiveActivity { .. }) => panic!("a Live Activity on Android"),
        Heard::Update(Update::Notification { view }) => match &view.ongoing {
            Some(till_screens::notification::OngoingFace::Sale { face, alert, .. }) => {
                Some((face.clone(), *alert))
            }
            _ => None,
        },
        _ => None,
    });
    assert!(alert, "a heads-up as it arrives");
    assert_eq!(face.title, "New Monero sale · Front counter");
    let next = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Notification { view }) => Some(view.clone()),
        _ => None,
    });
    assert!(
        matches!(
            next.ongoing,
            Some(till_screens::notification::OngoingFace::Sale { alert: false, .. })
        ),
        "{next:?}"
    );
}

/// Android's bubble over other apps: its panel carries the sale with its
/// header, hero and order; its countdown ticks while the panel is up with
/// the app in the back (decision 56); and where it rests is kept under
/// `Stores.kt`'s keys.
#[tokio::test(flavor = "multi_thread")]
async fn the_bubbles_panel_counts_down_while_it_is_up_and_its_place_is_kept() {
    let monokulo = monokulo().await;
    let square = square().await;
    let android = EngineConfig {
        device: Device::Android,
        ..config(Some(&square))
    };
    let platform = signed_in_till(&monokulo);
    platform.set("bubble.left", "true");
    platform.set("bubble.y", "412");
    let (engine, rx, _) = start(android, platform.clone());
    engine.send(Action::Permissions {
        alerts: true,
        bubble: true,
    });
    engine.send(Action::Backgrounded);
    let bubble = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Bubble { view }) if view.shown && view.panel.is_some() => {
            Some(view.clone())
        }
        _ => None,
    });
    assert!((bubble.left, bubble.y) == (true, Some(412)), "{bubble:?}");
    let panel = bubble.panel.unwrap();
    assert_eq!(panel.order_id, "ord_1");
    assert_eq!(panel.where_from, "Front counter");
    assert_eq!(panel.hero, till_screens::bubble::PanelHero::Code);
    assert!(panel.time_left.is_some() && panel.ticks);
    assert!(bubble.pulses, "a code waits with the panel down");

    // The panel down and the app in the back: nothing counts down on screen.
    tokio::time::sleep(super::till::TICK + Duration::from_millis(300)).await;
    assert!(!engine.peek(|s| s.ticking));
    engine.send(Action::PanelShown { up: true });
    let up = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Bubble { view }) => Some(view.clone()),
        _ => None,
    });
    assert!(!up.pulses, "no pulse with the panel up");
    assert!(engine.peek(|s| s.ticking), "the panel's countdown ticks");

    engine.send(Action::MoveBubble {
        left: false,
        y: 900,
    });
    let moved = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Bubble { view }) if !view.left => Some(view.y),
        _ => None,
    });
    assert_eq!(moved, Some(900));
    assert_eq!(platform.get("bubble.left").as_deref(), Some("false"));
    assert_eq!(platform.get("bubble.y").as_deref(), Some("900"));
}

#[test]
fn choosing_wake_ups_keeps_the_choice_in_stores_kts_key() {
    let platform = Arc::new(FakePlatform::default());
    let (engine, rx, _) = start(config(None), platform.clone());
    screen(&rx, Screen::Welcome);
    engine.send(Action::ChooseWakeUps {
        choice: WakeUps::MonokuloBell,
    });
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::WakeUps { view }) if view.monokulo_bell.selected => Some(()),
        _ => None,
    });
    assert_eq!(
        platform.get("wakeups.choice").as_deref(),
        Some(r#"{"type":"monokulo"}"#)
    );
    engine.send(Action::ChooseWakeUps {
        choice: WakeUps::OwnBell {
            address: "not a bell".into(),
        },
    });
    engine.send(Action::ChooseWakeUps {
        choice: WakeUps::Timer,
    });
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::WakeUps { view }) if view.timer.selected => Some(()),
        _ => None,
    });
    assert_eq!(
        platform.get("wakeups.choice"),
        None,
        "the timer alone is kept as nothing"
    );
}

/// Settings' island section, as the engine publishes it.
fn island(rx: &Receiver<Heard>) -> Option<till_screens::settings::IslandSection> {
    wait_for(rx, |heard| match heard {
        Heard::Update(Update::Settings { view }) => Some(view.island.clone()),
        _ => None,
    })
}

/// Decision 47: Settings › Dynamic Island's switches are kept in keys of
/// their own on iPhone, and shown as kept.
#[test]
fn the_islands_switches_are_kept_and_shown_on_iphone_only() {
    let platform = Arc::new(FakePlatform::default());
    let (engine, rx, _) = start(config(None), platform.clone());
    let shown = island(&rx).expect("an iPhone has the island section");
    assert!(shown.live_activity && !shown.code_in_island, "the defaults");
    engine.send(Action::SetCodeInIsland { on: true });
    let shown = island(&rx).unwrap();
    assert!(shown.code_in_island);
    assert_eq!(platform.get("island.code").as_deref(), Some("true"));
    engine.send(Action::SetLiveActivity { on: false });
    assert!(!island(&rx).unwrap().live_activity);
    assert_eq!(platform.get("island.activity").as_deref(), Some("false"));

    let android = Arc::new(FakePlatform::default());
    let mut config = config(None);
    config.device = Device::Android;
    let (engine, rx, _) = start(config, android.clone());
    assert_eq!(island(&rx), None);
    engine.send(Action::SetCodeInIsland { on: true });
    engine.peek(|_| ());
    assert_eq!(android.get("island.code"), None, "nothing kept on Android");
}

#[test]
fn work_done_for_an_earlier_generation_is_dropped() {
    let (engine, rx, _) = start(config(None), Arc::new(FakePlatform::default()));
    screen(&rx, Screen::Welcome);
    let now = engine.peek(|s| s.generation);
    let _ = engine.tx.send(super::Msg::Done(
        now + 1,
        super::apply(|s| s.problem = Some("stale".into())),
    ));
    let _ = engine.tx.send(super::Msg::Done(
        now,
        super::apply(|s| s.problem = Some("current".into())),
    ));
    assert_eq!(problem(&rx), "current");
}

/// The sale rows a shift lists.
fn rows(view: &till_screens::ShiftView) -> usize {
    view.entries
        .iter()
        .filter(|e| matches!(e, till_screens::shift::ShiftEntry::Row { .. }))
        .count()
}

/// Connects to a stand-in whose handshake gives an onion address, after
/// `probe` (if any) told the engine about Orbot; returns what was kept.
async fn connect_with_onion(probe: Option<bool>) -> String {
    let monokulo = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/apps/connect"))
        .respond_with(json(
            r#"{"app_key":"ak_x","api":"/api/v1","onion_api":"http://abc.onion/api/v1"}"#,
        ))
        .mount(&monokulo)
        .await;
    let platform = Arc::new(FakePlatform::default());
    let (engine, rx, _) = start(config(None), platform.clone());
    if let Some(running) = probe {
        engine.send(Action::TorProbed { running });
    }
    engine.send(Action::ConnectLink {
        url: format!("{}/apps/connect#token={TOKEN}", monokulo.uri()),
    });
    screen(&rx, Screen::Confirm);
    engine.send(Action::Connect);
    screen(&rx, Screen::Connected);
    platform.get("connection").unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_till_connects_over_tor_only_once_a_probe_found_orbot_running() {
    let on = connect_with_onion(Some(true)).await;
    assert!(on.contains(r#""tor":true"#), "{on}");
    assert!(
        on.contains(r#""onion_api":"http://abc.onion/api/v1""#),
        "{on}"
    );
    let off = connect_with_onion(Some(false)).await;
    assert!(!off.contains(r#""tor""#), "{off}");
    let unprobed = connect_with_onion(None).await;
    assert!(!unprobed.contains(r#""tor""#), "{unprobed}");
}

#[test]
fn settings_says_when_orbot_isnt_running_for_a_till_on_tor() {
    let platform = FakePlatform::with(&[("setup.done", "true")]);
    platform.set(
        "connection",
        r#"{"app_key":"ak_1","api":"https://pay.example/api/v1","onion_api":"http://abc.onion/api/v1","tor":true}"#,
    );
    let android = EngineConfig {
        device: Device::Android,
        ..config(None)
    };
    let (engine, rx, _) = start(android, platform);
    engine.send(Action::TorProbed { running: false });
    let footnote = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Settings { view }) => view
            .tor
            .as_ref()
            .map(|tor| tor.footnote.clone())
            .filter(|f| f.starts_with("Orbot isn't running")),
        _ => None,
    });
    assert!(footnote.ends_with("or turn this off."), "{footnote}");
}

/// The screen and how it came, as the platform hears them: the `Nav` sent
/// before each `Screen`.
fn arrival(rx: &Receiver<Heard>, wanted: Screen) -> bool {
    let mut by_itself = false;
    wait_for(rx, |heard| match heard {
        Heard::Update(Update::Nav { by_itself: b, .. }) => {
            by_itself = *b;
            None
        }
        Heard::Update(Update::Screen { screen }) if *screen == wanted => Some(()),
        _ => None,
    });
    by_itself
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_sale_rises_out_of_the_resting_shift_by_itself_with_its_screens() {
    let monokulo = monokulo().await;
    let square = square().await;
    let (engine, rx, _) = start(config(Some(&square)), signed_in_till(&monokulo));
    let sale = Screen::Sale {
        order_id: "ord_1".into(),
    };
    assert!(arrival(&rx, sale.clone()), "the sales put it there");
    let (view, page, code) = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Sale {
            order_id,
            view,
            page,
            code,
            ..
        }) if order_id == "ord_1" => Some((view.clone(), page.clone(), code.clone())),
        _ => None,
    });
    assert_eq!(view.amount, "$13.88");
    assert_eq!(view.code, "monero:8Bx?tx_amount=0.08");
    assert!(view.closes, "a live sale has the X");
    assert_eq!(page.amount, "$13.88");
    assert!(!code.done);
    assert_eq!(
        engine.peek(|s| s.back_target()),
        Some(Screen::Till),
        "Back from a sale previews the shift"
    );

    // The X asks; the sheet goes without an answer.
    engine.send(Action::AskClose {
        order_id: "ord_1".into(),
    });
    let answers = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::CloseSheet {
            view: Some(view), ..
        }) => Some(view.answers.len()),
        _ => None,
    });
    assert_eq!(answers, 3);
    engine.send(Action::DismissClose);
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::CloseSheet { view: None, .. }) => Some(()),
        _ => None,
    });

    // Back: the cashier's, so not by itself; the sale stays live in the tray
    // and doesn't come up again.
    engine.send(Action::Back);
    assert!(!arrival(&rx, Screen::Till));
    engine.send(Action::Landed {
        screen: Screen::Till,
    });
    engine.send(Action::ChooseFilter {
        filter: ShiftFilter::Paid,
    });
    let chosen = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Shift { view }) => view
            .chips
            .iter()
            .find(|chip| chip.selected)
            .map(|chip| chip.filter)
            .filter(|f| *f == ShiftFilter::Paid),
        _ => None,
    });
    assert_eq!(chosen, ShiftFilter::Paid);
    assert_eq!(engine.peek(|s| s.screen.clone()), Screen::Till);

    // The panel's "Open the shift" leaves every sale; opening one brings it
    // back up, the cashier's doing.
    engine.send(Action::OpenSale {
        order_id: "ord_1".into(),
    });
    assert!(!arrival(&rx, sale));
}

#[test]
fn back_from_settings_previews_where_it_was_opened_from() {
    let platform = FakePlatform::with(&[("setup.done", "true")]);
    platform.set(
        "connection",
        r#"{"app_key":"ak_1","api":"https://pay.example/api/v1"}"#,
    );
    let (engine, rx, _) = start(config(None), platform);
    screen(&rx, Screen::Till);
    engine.send(Action::OpenSettings);
    let target = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Nav { back_target, .. }) => back_target.clone(),
        _ => None,
    });
    assert_eq!(target, Screen::Till);
    screen(&rx, Screen::Settings);
    engine.send(Action::OpenPointOfSale);
    screen(&rx, Screen::PointOfSale);
    engine.send(Action::Back);
    screen(&rx, Screen::Settings);
}

#[test]
fn the_till_and_the_sales_screens_map_to_places_and_back() {
    use super::till::{place, screen as of_place};
    for s in [
        Screen::Till,
        Screen::Sale {
            order_id: "o".into(),
        },
        Screen::Code {
            order_id: "o".into(),
        },
        Screen::Order {
            order_id: "o".into(),
        },
    ] {
        assert_eq!(of_place(place(&s)), Some(s));
    }
    assert_eq!(of_place(place(&Screen::Settings)), None);
}
