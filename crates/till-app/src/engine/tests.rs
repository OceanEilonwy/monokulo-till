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
    Action, CloseChoice, CloverSetup, CopyKind, CounterChoice, Device, Effect, Engine,
    EngineConfig, Screen, SquareSetup, Update, WakeUps,
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
        relay_url: None,
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

/// Decision 109: a problem no screen's view carries (a refused write from
/// Settings) is for the platform to show itself.
#[test]
fn a_problem_no_view_shows_is_the_platforms_to_show() {
    let platform = FakePlatform::with(&[("setup.done", "true")]);
    platform.set(
        "connection",
        r#"{"app_key":"ak_1","api":"https://pay.example/api/v1"}"#,
    );
    let (engine, rx, _) = start(config(None), platform.clone());
    screen(&rx, Screen::Till);
    engine.send(Action::OpenSettings);
    screen(&rx, Screen::Settings);
    platform.fail(true);
    engine.send(Action::SetLiveActivity { on: false });
    let (problem, shown) = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Work {
            problem: Some(problem),
            shown_problem,
            ..
        }) => Some((problem.clone(), shown_problem.clone())),
        _ => None,
    });
    assert!(problem.starts_with("Couldn't keep that"), "{problem}");
    assert_eq!(shown, Some(problem), "Settings has no place for it: shown");
}

/// Step 2's next problem, as its page says it.
fn scan_problem(rx: &Receiver<Heard>) -> String {
    wait_for(rx, |heard| match heard {
        Heard::Update(Update::Onboarding { view }) => match &view.page {
            OnboardingPage::Scan { problem } => problem.clone(),
            _ => None,
        },
        _ => None,
    })
}

/// Decision 60's choices are the engine's: step 4's Skip is "Not now"
/// until the till is connected to a point of sale, and its Next does
/// nothing before one is chosen.
#[test]
fn step_4s_skip_before_a_sign_in_is_not_now() {
    let platform = FakePlatform::with(&[(
        "connection",
        r#"{"app_key":"ak_1","api":"https://pay.example/api/v1"}"#,
    )]);
    let (engine, rx, _) = start(config(None), platform.clone());
    screen(&rx, Screen::PointOfSale);
    engine.send(Action::Next);
    assert_eq!(
        engine.peek(|s| s.screen.clone()),
        Screen::PointOfSale,
        "nothing chosen: no way on"
    );
    engine.send(Action::Skip);
    screen(&rx, Screen::Alerts);
    assert_eq!(platform.get("pos.skipped").as_deref(), Some("true"));
    // Step 5's "Not now" goes on, to Ready on an iPhone.
    engine.send(Action::Skip);
    screen(&rx, Screen::Ready);
}

#[test]
fn androids_use_notifications_only_turns_the_bubble_off() {
    let platform = FakePlatform::with(&[
        (
            "connection",
            r#"{"app_key":"ak_1","api":"https://pay.example/api/v1"}"#,
        ),
        ("pos.skipped", "true"),
    ]);
    let android = EngineConfig {
        device: Device::Android,
        ..config(None)
    };
    let (engine, rx, _) = start(android, platform.clone());
    screen(&rx, Screen::Alerts);
    engine.send(Action::Skip);
    screen(&rx, Screen::Float);
    engine.send(Action::Skip);
    screen(&rx, Screen::Ready);
    assert_eq!(platform.get("bubble.on").as_deref(), Some("false"));
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
    // Each way in has its own words for what isn't a connect code.
    engine.send(Action::PasteConnectLink { text: None });
    assert_eq!(
        scan_problem(&rx),
        "There's nothing to paste. Copy the connect link on the dashboard first."
    );
    engine.send(Action::PasteConnectLink {
        text: Some("https://pay.example/shop".into()),
    });
    assert_eq!(scan_problem(&rx), "That isn't a store's connect link.");
    engine.send(Action::ScanConnectCode {
        text: "https://pay.example/shop".into(),
    });
    assert_eq!(scan_problem(&rx), "That code isn't a store's connect code.");
    engine.send(Action::PasteConnectLink {
        text: Some(" ".into()),
    });
    assert_eq!(
        scan_problem(&rx),
        "There's nothing to paste. Copy the connect link on the dashboard first."
    );
    engine.send(Action::ScanConnectCode {
        text: format!("{}/apps/connect#token={TOKEN}", monokulo.uri()),
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
    // O1's "Continue with Square", then O2's sign-in (decision 121).
    engine.send(Action::Next);
    engine.send(Action::Next);
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
    // Connected: Skip is "Skip the test", on to alerts, the till not
    // without a point of sale.
    engine.send(Action::Skip);
    screen(&rx, Screen::Alerts);
    assert_eq!(platform.get("pos.skipped"), None);
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
    assert_eq!(alert.title, "Sale #bP9m · $13.88");
    assert_eq!(
        alert.body,
        "Monero sale. Tap to show the customer the code."
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
        Heard::Effect(Effect::CopyText { text, kind }) => Some((text.clone(), *kind)),
        _ => None,
    });
    assert_eq!(copied, ("8Bx".into(), CopyKind::Address));
    // The next change goes without the alert: it was posted once.
    if let Some(after) = after {
        assert_eq!(after.alert, None, "alerted once");
    }
    engine.send(Action::CopyAmount {
        order_id: "ord_1".into(),
    });
    let copied = wait_for(&rx, |heard| match heard {
        Heard::Effect(Effect::CopyText { text, kind }) => Some((text.clone(), *kind)),
        _ => None,
    });
    assert_eq!(copied, ("0.08".into(), CopyKind::Amount));

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

/// The iPad's floating window (ipad-redesign.md decision 2) draws the Live
/// Activity's state, with the code while the sale waits, only while a new
/// sale shows up in a floating window.
#[tokio::test(flavor = "multi_thread")]
async fn the_ipads_floating_window_follows_the_sale_only_while_chosen() {
    use till_screens::live_activity::ActivityPhase;
    let monokulo = monokulo().await;
    let square = square().await;
    let platform = signed_in_till(&monokulo);
    platform.set("sales.show", "pip");
    let ipad = EngineConfig {
        device: Device::Ipad,
        ..config(Some(&square))
    };
    let (engine, rx, _) = start(ipad, platform);
    let waiting = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::LiveActivity { state: Some(state) })
            if state.phase == ActivityPhase::Waiting =>
        {
            Some(state.clone())
        }
        _ => None,
    });
    assert_eq!(waiting.amount.as_deref(), Some("13.88"));
    assert_eq!(
        waiting.payment_uri.as_deref(),
        Some("monero:8Bx?tx_amount=0.08"),
        "the window shows the code"
    );
    engine.send(Action::ChooseShowSales {
        mode: crate::ShowSales::Alert,
    });
    assert_eq!(live_activity(&rx), None, "no window with alerts chosen");
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
    assert_eq!(face.title, "Sale #bP9m · $13.88");
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

/// Decision 95: the bubble's paid moment is the engine's. A payment holds
/// the bubble green for the dwell; with the panel up on that sale and no
/// code waiting, the dwell's end asks Android to collapse the panel, and
/// the bubble stays green until the panel has gone. The dwell's timer is
/// stood in for by calling its end.
#[tokio::test(flavor = "multi_thread")]
async fn the_bubbles_paid_moment_holds_green_through_the_collapse_it_asks_for() {
    use till_screens::bubble::{BubbleLook, PaidMoment};
    let monokulo = monokulo().await;
    let square = square().await;
    let android = EngineConfig {
        device: Device::Android,
        ..config(Some(&square))
    };
    let (engine, rx, _) = start(android, signed_in_till(&monokulo));
    engine.send(Action::Permissions {
        alerts: true,
        bubble: true,
    });
    engine.send(Action::Backgrounded);
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Bubble { view }) if view.shown && view.panel.is_some() => Some(()),
        _ => None,
    });
    engine.send(Action::PanelShown { up: true });

    // A code still waiting when the dwell ends: the panel moves on to it,
    // no collapse.
    engine.poke(|s| s.just_paid("ord_1".into()));
    assert_eq!(engine.peek(|s| s.paid_moment), PaidMoment::Held);
    engine.poke(|s| s.paid_dwell_ended("ord_1"));
    assert_eq!(engine.peek(|s| s.paid_moment), PaidMoment::None);

    // Nothing waiting: the dwell's end collapses the panel.
    engine.send(Action::Close {
        order_id: "ord_1".into(),
        choice: CloseChoice::CloseForNow,
    });
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Bubble { view }) if view.look == BubbleLook::Idle => Some(()),
        _ => None,
    });
    engine.poke(|s| s.just_paid("ord_1".into()));
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Bubble { view }) if view.look == BubbleLook::Paid => Some(()),
        _ => None,
    });
    engine.poke(|s| s.paid_dwell_ended("ord_1"));
    wait_for(&rx, |heard| match heard {
        Heard::Effect(Effect::LowerPanel) => Some(()),
        Heard::Update(Update::Bubble { view }) => {
            assert_eq!(
                view.look,
                BubbleLook::Paid,
                "green until the collapse lands"
            );
            None
        }
        _ => None,
    });
    assert_eq!(engine.peek(|s| s.paid_moment), PaidMoment::Lowering);
    engine.send(Action::PanelShown { up: false });
    let after = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Bubble { view }) => Some(view.look),
        _ => None,
    });
    assert_eq!(after, BubbleLook::Idle, "landed: the green fades");
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
fn my_own_bell_is_typed_on_the_platform_and_checked_by_the_engine() {
    let (engine, rx, _) = start(config(None), Arc::new(FakePlatform::default()));
    screen(&rx, Screen::Welcome);
    engine.send(Action::EditOwnBell {
        picked: true,
        typed: Some("not a bell".into()),
    });
    let field = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::WakeUps { view }) => view.own_field.clone(),
        _ => None,
    });
    assert!(!field.use_this.enabled, "not a Bell's address");
    assert!(field.problem.is_some());
    engine.send(Action::EditOwnBell {
        picked: true,
        typed: Some("https://bell.example.com".into()),
    });
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::WakeUps { view }) => view
            .own_field
            .as_ref()
            .filter(|f| f.use_this.enabled && f.problem.is_none())
            .map(|_| ()),
        _ => None,
    });
    engine.send(Action::ChooseWakeUps {
        choice: WakeUps::OwnBell {
            address: "https://bell.example.com".into(),
        },
    });
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::WakeUps { view }) if view.own_bell.selected => Some(()),
        _ => None,
    });
    engine.send(Action::ChooseWakeUps {
        choice: WakeUps::Timer,
    });
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::WakeUps { view }) if view.own_field.is_none() => Some(()),
        _ => None,
    });
    // Tapped again, the saved address is chosen at once.
    engine.send(Action::EditOwnBell {
        picked: true,
        typed: None,
    });
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::WakeUps { view }) if view.own_bell.selected => Some(()),
        _ => None,
    });
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

/// Decision 105: Settings' "Copy wallet address" goes through the engine,
/// which copies the store's whole address as every other copy does.
#[tokio::test(flavor = "multi_thread")]
async fn settings_copies_the_stores_whole_wallet_address_through_the_engine() {
    let monokulo = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"_links":{"self":{"href":"/api/v1"},"mk:store":{"href":"/api/v1/store"},"mk:app":{"href":"/api/v1/app"}}}"#,
            "application/prs.hal-forms+json",
        ))
        .mount(&monokulo)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/store"))
        .respond_with(json(
            r#"{"name":"The Bean Room","base_currency":"USD","wallet":{"name":"Shop","network":"mainnet","address":"4AwholeADDRESS"}}"#,
        ))
        .mount(&monokulo)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/app"))
        .respond_with(json(r#"{"app_id":"a1","device_name":"Front"}"#))
        .mount(&monokulo)
        .await;
    let platform = FakePlatform::with(&[("setup.done", "true")]);
    platform.set(
        "connection",
        &format!(r#"{{"app_key":"ak_1","api":"{}/api/v1"}}"#, monokulo.uri()),
    );
    let (engine, rx, _) = start(config(None), platform);
    screen(&rx, Screen::Till);
    engine.send(Action::OpenSettings);
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Settings { view })
            if view
                .store
                .as_ref()
                .is_some_and(|s| s.wallet_address.is_some()) =>
        {
            Some(())
        }
        _ => None,
    });
    engine.send(Action::CopyWalletAddress);
    let copied = wait_for(&rx, |heard| match heard {
        Heard::Effect(Effect::CopyText { text, kind }) => Some((text.clone(), *kind)),
        _ => None,
    });
    assert_eq!(copied, ("4AwholeADDRESS".into(), CopyKind::Address));
}

/// Decision 103: the platform hands over shared text whole, and the engine
/// finds the store's connect link in it.
#[test]
fn shared_text_with_a_connect_link_in_it_asks_to_connect() {
    let (engine, rx, _) = start(config(None), Arc::new(FakePlatform::default()));
    screen(&rx, Screen::Welcome);
    engine.send(Action::SharedText {
        text: "nothing to see".into(),
    });
    engine.send(Action::SharedText {
        text: format!("Connect your till:\nhttps://pay.example/apps/connect#token={TOKEN} thanks"),
    });
    screen(&rx, Screen::Confirm);
}

/// Decision 102: Android looks for Orbot as the app comes forward and as
/// Settings (or step 3) comes up, when the engine asks; iOS never does.
#[test]
fn android_probes_for_orbot_when_the_engine_asks_and_ios_never_does() {
    let till = || {
        let platform = FakePlatform::with(&[("setup.done", "true")]);
        platform.set(
            "connection",
            r#"{"app_key":"ak_1","api":"https://pay.example/api/v1"}"#,
        );
        platform
    };
    let android = EngineConfig {
        device: Device::Android,
        ..config(None)
    };
    let (engine, rx, _) = start(android, till());
    screen(&rx, Screen::Till);
    engine.send(Action::Opened);
    wait_for(&rx, |heard| match heard {
        Heard::Effect(Effect::ProbeTor) => Some(()),
        _ => None,
    });
    // Published again as it came forward, on the shift.
    screen(&rx, Screen::Till);
    engine.send(Action::OpenSettings);
    let mut probed = false;
    wait_for(&rx, |heard| match heard {
        Heard::Effect(Effect::ProbeTor) => {
            probed = true;
            None
        }
        Heard::Update(Update::Screen {
            screen: Screen::Settings,
        }) => Some(()),
        _ => None,
    });
    assert!(probed, "Settings came up: the probe goes before it");

    let (engine, rx, _) = start(config(None), till());
    screen(&rx, Screen::Till);
    engine.send(Action::Opened);
    engine.send(Action::OpenSettings);
    wait_for(&rx, |heard| match heard {
        Heard::Effect(Effect::ProbeTor) => panic!("iOS has no Tor"),
        Heard::Update(Update::Screen {
            screen: Screen::Settings,
        }) => Some(()),
        _ => None,
    });
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

/// Decision 96: the close sheet's answer lands where `close::outcome` says,
/// whoever asked it. "No: refund it" from the shift's row opens the sale,
/// whose page shows the refund; the platform sends the answer alone.
#[tokio::test(flavor = "multi_thread")]
async fn refunding_from_the_shifts_row_opens_the_sale_where_the_refund_shows() {
    let monokulo = monokulo().await;
    let square = square().await;
    let (engine, rx, _) = start(config(Some(&square)), signed_in_till(&monokulo));
    let sale = Screen::Sale {
        order_id: "ord_1".into(),
    };
    screen(&rx, sale.clone());
    engine.send(Action::Back);
    screen(&rx, Screen::Till);
    engine.send(Action::AskClose {
        order_id: "ord_1".into(),
    });
    engine.send(Action::Close {
        order_id: "ord_1".into(),
        choice: CloseChoice::RefundIt,
    });
    assert!(
        !arrival(&rx, sale),
        "the cashier's answer put it there, not the sales"
    );
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

/// The Nav sent before `wanted`: whether the system's Back is the app's
/// there.
fn back_enabled_on(rx: &Receiver<Heard>, wanted: Screen) -> bool {
    let mut enabled = None;
    wait_for(rx, |heard| match heard {
        Heard::Update(Update::Nav { back_enabled, .. }) => {
            enabled = Some(*back_enabled);
            None
        }
        Heard::Update(Update::Screen { screen }) if *screen == wanted => Some(()),
        _ => None,
    });
    enabled.expect("a Nav before the screen")
}

/// Decision 98 (android.md 77): the system's Back is the app's everywhere
/// but Welcome, the shift and step 4 during setup; step 4 opened from
/// Settings goes back to it.
#[test]
fn back_is_the_apps_everywhere_but_welcome_the_shift_and_setups_step_4() {
    let (engine, rx, _) = start(config(None), Arc::new(FakePlatform::default()));
    assert!(!back_enabled_on(&rx, Screen::Welcome));
    engine.send(Action::Next);
    assert!(back_enabled_on(&rx, Screen::Scan));

    let platform = FakePlatform::with(&[("setup.done", "true")]);
    platform.set(
        "connection",
        r#"{"app_key":"ak_1","api":"https://pay.example/api/v1"}"#,
    );
    let (engine, rx, _) = start(config(None), platform);
    assert!(!back_enabled_on(&rx, Screen::Till));
    engine.send(Action::OpenSettings);
    assert!(back_enabled_on(&rx, Screen::Settings));
    engine.send(Action::OpenPointOfSale);
    assert!(
        back_enabled_on(&rx, Screen::PointOfSale),
        "from Settings, step 4 goes back"
    );
}

/// Ported from Android's `NavTest` (decision 101): back is toward the
/// shift, and backwards through setup, whose steps slide.
#[test]
fn back_is_toward_the_shift_and_backwards_through_setup() {
    use super::till::direction;
    let sale = |id: &str| Screen::Sale {
        order_id: id.into(),
    };
    let back = |from: Screen, to: Screen| direction(&from, &to).0;
    assert!(back(sale("a"), Screen::Till));
    assert!(back(
        Screen::Code {
            order_id: "a".into()
        },
        sale("a")
    ));
    assert!(back(Screen::Settings, Screen::Till));
    assert!(back(Screen::Scan, Screen::Welcome));
    assert!(!back(Screen::Till, sale("a")));
    assert!(!back(
        sale("a"),
        Screen::Order {
            order_id: "a".into()
        }
    ));
    assert!(!back(Screen::Welcome, Screen::Scan));

    let steps = |from: Screen, to: Screen| direction(&from, &to);
    assert_eq!(steps(Screen::Welcome, Screen::Scan), (false, true));
    assert_eq!(steps(Screen::Scan, Screen::Welcome), (true, true));
    assert_eq!(steps(Screen::Float, Screen::Ready), (false, true));
    assert_eq!(
        steps(Screen::Ready, Screen::Till),
        (false, true),
        "starting the first shift"
    );
    assert_eq!(steps(Screen::Alerts, Screen::PointOfSale), (true, true));
    assert!(!steps(sale("a"), Screen::Till).1, "a sale isn't a step");
    assert!(!steps(Screen::Till, Screen::Settings).1);
}

/// The Nav before a screen says which way the change to it went.
#[test]
fn the_nav_before_a_screen_says_which_way_it_went() {
    let (engine, rx, _) = start(config(None), Arc::new(FakePlatform::default()));
    screen(&rx, Screen::Welcome);
    engine.send(Action::Next);
    let way = |rx: &Receiver<Heard>, wanted: Screen| {
        let mut way = None;
        wait_for(rx, |heard| match heard {
            Heard::Update(Update::Nav { back, steps, .. }) => {
                way = Some((*back, *steps));
                None
            }
            Heard::Update(Update::Screen { screen }) if *screen == wanted => Some(()),
            _ => None,
        });
        way
    };
    assert_eq!(way(&rx, Screen::Scan), Some((false, true)));
    engine.send(Action::Back);
    assert_eq!(way(&rx, Screen::Welcome), Some((true, true)));
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

/// Waits up to 10 s for `server` to have a request that `pick` takes.
async fn request_to(
    server: &MockServer,
    pick: impl Fn(&wiremock::Request) -> bool,
) -> wiremock::Request {
    for _ in 0..200 {
        let requests = server.received_requests().await.unwrap();
        if let Some(found) = requests.into_iter().find(|r| pick(r)) {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no such request within 10 s");
}

/// iOS (design.md §5): the engine registers the APNs token with the relay
/// for the Bell's VAPID key, gives the Bell the relay's endpoint with this
/// device's keys, and opens a ding the relay carries, which wakes the
/// shift.
#[tokio::test(flavor = "multi_thread")]
async fn on_ios_the_apns_token_goes_through_the_relay_to_the_bell_and_a_relayed_ding_wakes_the_shift()
 {
    const APNS: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";
    const VAPID: &str =
        "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM";
    let (monokulo, square) = (monokulo().await, square().await);
    let bell = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/info"))
        .respond_with(json(&format!(
            r#"{{"points_of_sale":{{"square":{{"production":{{"application_id":"sq0idp-till"}}}}}},"vapid_public_key":"{VAPID}","version":"0.3.0"}}"#,
        )))
        .mount(&bell)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/register"))
        .respond_with(ResponseTemplate::new(201).set_body_raw(
            r#"{"registration_id":"br_1","point_of_sale":"square","environment":"production","application_id":"sq0idp-till"}"#,
            "application/json",
        ))
        .mount(&bell)
        .await;
    let relay = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/register"))
        .respond_with(ResponseTemplate::new(201).set_body_raw(
            r#"{"endpoint":"https://relay.example/v1/push/ph_1","expires_at":1893456000}"#,
            "application/json",
        ))
        .mount(&relay)
        .await;
    let platform = signed_in_till(&monokulo);
    let mut config = config(Some(&square));
    config.relay_url = Some(relay.uri());
    let (engine, _rx, _) = start(config, platform.clone());
    engine.send(Action::PushToken {
        token: Some(APNS.into()),
    });
    engine.send(Action::ChooseWakeUps {
        choice: WakeUps::OwnBell {
            address: bell.uri(),
        },
    });

    let registered = request_to(&relay, |_| true).await;
    let sent: serde_json::Value = serde_json::from_slice(&registered.body).unwrap();
    assert_eq!(
        sent,
        serde_json::json!({"platform": "apns", "token": APNS, "app": "till", "vapid_public_key": VAPID})
    );
    let rung = request_to(&bell, |r| r.method.as_str() == "POST").await;
    let sent: serde_json::Value = serde_json::from_slice(&rung.body).unwrap();
    let subscription = &sent["subscription"];
    assert_eq!(
        subscription["endpoint"],
        "https://relay.example/v1/push/ph_1"
    );
    let keys = crate::web_push::PushKeys::load(&crate::store::Store::new(platform.clone()))
        .unwrap()
        .unwrap();
    assert_eq!(subscription["keys"]["p256dh"], keys.p256dh().unwrap());
    assert_eq!(subscription["keys"]["auth"], keys.auth());

    // With the Bell connected the timer is 30 s, so a look within a second
    // of the ding is the ding's.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let looks = || async {
        square
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/v2/payments")
            .count()
    };
    let before = looks().await;
    let ding = crate::web_push::tests::encrypt(
        &keys.p256dh().unwrap(),
        &keys.auth(),
        br#"{"wake":"square"}"#,
    );
    engine.send(Action::PushReceived {
        content: ding,
        decrypted: false,
    });
    for _ in 0..20 {
        if looks().await > before {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the ding didn't wake the shift");
}

/// Square, with one Monero payment at the location taken on `device`.
async fn square_with_device(device: &str, name: &str, source: &str) -> MockServer {
    let server = MockServer::start().await;
    let payments = PAYMENTS
        .replace(
            r#""location_id":"L1""#,
            &format!(
                r#""location_id":"L1","device_details":{{"device_id":"{device}","device_name":"{name}"}}"#
            ),
        )
        .replace(r#""source_type":"EXTERNAL""#, source);
    Mock::given(method("GET"))
        .and(path("/v2/payments"))
        .respond_with(json(&payments))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/orders/ord_sq_1"))
        .respond_with(json(
            r#"{"order":{"id":"ord_sq_1","location_id":"L1","created_at":"2026-10-22T16:06:41Z","total_money":{"amount":1388,"currency":"USD"},"ticket_name":"Sam"}}"#,
        ))
        .mount(&server)
        .await;
    server
}

/// The shift with `n` rows, as next published.
fn shift_with(rx: &Receiver<Heard>, n: usize) -> till_screens::ShiftView {
    wait_for(rx, |heard| match heard {
        Heard::Update(Update::Notification { view }) if view.alert.is_some() => {
            panic!("alerted: {view:?}")
        }
        Heard::Update(Update::Shift { view }) if rows(view) == n => Some(view.clone()),
        _ => None,
    })
}

fn first_row(view: &till_screens::ShiftView) -> till_screens::shift::ShiftRow {
    view.entries
        .iter()
        .find_map(|e| match e {
            till_screens::shift::ShiftEntry::Row { row, .. } => Some(row.clone()),
            _ => None,
        })
        .expect("a row")
}

#[tokio::test(flavor = "multi_thread")]
async fn another_counters_sale_is_listed_but_never_alerts_or_takes_the_screen() {
    let monokulo = monokulo().await;
    let square = square_with_device("dev_bar", "Bar iPad", r#""source_type":"EXTERNAL""#).await;
    let platform = signed_in_till(&monokulo);
    platform.set(
        "counter.device",
        r#"{"id":"dev_front","name":"Front iPad","learnedAt":1761148000}"#,
    );
    let (engine, rx, _) = start(config(Some(&square)), platform.clone());
    engine.send(Action::Backgrounded);
    screen(&rx, Screen::Till);
    let shift = shift_with(&rx, 1);
    let row = first_row(&shift);
    assert!(row.other_counter);
    assert_eq!(row.device.as_deref(), Some("Bar iPad"));
    assert_eq!(row.name, "Sam", "the ticket's name leads the row");
    let menu = shift.counters.expect("the title menu");
    assert_eq!(
        menu.title, "All counters",
        "other counters' sales show by default"
    );
    // Everything the batch published is in: no alert, and the shift stays.
    let (alert, on) = engine.peek(|s| (s.alert_for.clone(), s.screen.clone()));
    assert_eq!((alert, on), (None, Screen::Till));
    for heard in rx.try_iter() {
        if let Heard::Update(Update::Notification { view }) = heard {
            assert_eq!(view.alert, None, "another counter's sale never alerts");
        }
    }
    engine.send(Action::ChooseCounter {
        choice: CounterChoice::Device {
            device_id: "dev_front".into(),
        },
    });
    let mine = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Shift { view }) if rows(view) == 0 => Some(view.clone()),
        _ => None,
    });
    assert_eq!(mine.counters.unwrap().title, "Front iPad");
}

#[tokio::test(flavor = "multi_thread")]
async fn alerts_for_every_device_take_another_counters_sale_too() {
    let monokulo = monokulo().await;
    let square = square_with_device("dev_bar", "Bar iPad", r#""source_type":"EXTERNAL""#).await;
    let platform = signed_in_till(&monokulo);
    platform.set(
        "counter.device",
        r#"{"id":"dev_front","name":"Front iPad","learnedAt":1761148000}"#,
    );
    platform.set("counter.every", "true");
    let (engine, rx, _) = start(config(Some(&square)), platform.clone());
    engine.send(Action::Backgrounded);
    let alert = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Notification { view }) => view.alert.clone(),
        _ => None,
    });
    assert_eq!(alert.title, "Sam · $13.88");
    assert_eq!(alert.body, "Monero sale. Tap to show Sam the code.");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_test_sale_names_this_counters_device_and_c4_asks_which_sales_alert() {
    let monokulo = monokulo().await;
    let square = square_with_device("dev_front", "Front iPad", r#""source_type":"EXTERNAL""#).await;
    let platform = signed_in_till(&monokulo);
    platform.set("setup.done", "false");
    let mut config = config(Some(&square));
    config.device = Device::Ipad;
    let (engine, rx, _) = start(config, platform.clone());
    // Signed in at a location, setup opens on Alerts: back to the test sale.
    screen(&rx, Screen::Alerts);
    engine.send(Action::Back);
    screen(&rx, Screen::PointOfSale);
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Onboarding { view }) => match &view.page {
            OnboardingPage::PointOfSale { view } => {
                view.status.as_ref().filter(|s| s.found).map(|_| ())
            }
            _ => None,
        },
        _ => None,
    });
    engine.send(Action::Next);
    screen(&rx, Screen::Counter);
    let kept = platform.get("counter.device").expect("the device kept");
    assert!(
        kept.contains(r#""id":"dev_front""#) && kept.contains("Front iPad"),
        "{kept}"
    );
    let step = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Onboarding { view }) => match &view.page {
            OnboardingPage::Counter { view } => Some(view.clone()),
            _ => None,
        },
        _ => None,
    });
    assert_eq!(step.device, "Front iPad");
    assert_eq!(step.found, "Test sale found · $13.88 · Sale #bP9m");
    engine.send(Action::SetAlertFor { every_device: true });
    wait_for(&rx, |_| platform.get("counter.every"));
    engine.send(Action::Next);
    screen(&rx, Screen::Alerts);
    engine.send(Action::Next);
    screen(&rx, Screen::ShowSales);
    engine.send(Action::ChooseShowSales {
        mode: crate::ShowSales::FloatingWindow,
    });
    wait_for(&rx, |_| platform.get("sales.show").filter(|v| v == "pip"));
    engine.send(Action::Next);
    screen(&rx, Screen::Ready);
}

#[tokio::test(flavor = "multi_thread")]
async fn with_cash_offered_a_cash_sale_waits_quietly_and_take_it_in_monero_opens_its_code() {
    let monokulo = monokulo().await;
    let square = square_with_device("dev_front", "Front iPad", r#""source_type":"CASH""#).await;
    let platform = signed_in_till(&monokulo);
    platform.set("cash.monero", "false");
    let (engine, rx, _) = start(config(Some(&square)), platform.clone());
    engine.send(Action::Backgrounded);
    let shift = shift_with(&rx, 1);
    let order_id = shift
        .entries
        .iter()
        .find_map(|e| match e {
            till_screens::shift::ShiftEntry::Row { order_id, .. } => Some(order_id.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(order_id, "cash:pay_monero");
    assert!(shift.entries.iter().any(|e| matches!(
        e,
        till_screens::shift::ShiftEntry::Earlier { label } if label == "Cash · paying in Monero?"
    )));
    engine.send(Action::Opened);
    engine.send(Action::TakeInMonero { order_id });
    screen(
        &rx,
        Screen::Sale {
            order_id: "ord_1".into(),
        },
    );
    let page = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Sale { view, .. }) => Some(view.clone()),
        _ => None,
    });
    assert_eq!(page.title, "Sam");
    assert_eq!(
        page.cash
            .iter()
            .map(|b| b.press.label.as_str())
            .collect::<Vec<_>>(),
        ["Paid in cash"]
    );
    engine.send(Action::PaidInCash {
        order_id: "ord_1".into(),
    });
    // The sale's page says how it ended.
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Sale { page, .. }) if page.heading == "Paid in cash" => Some(()),
        _ => None,
    });
    assert!(
        !square
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/v2/refunds"),
        "a cash sale is never refunded in Square"
    );
}

/// Issue 1 of the iPad test (core.md decision 123): this counter's sale,
/// announced first by another till (so kept as another till's, quiet),
/// alerts once this till's own Square finds it on this counter's device,
/// and once only.
#[tokio::test(flavor = "multi_thread")]
async fn this_counters_sale_another_till_announced_first_alerts_when_found_here() {
    let monokulo = monokulo().await;
    let square = square_with_device("dev_front", "Front iPad", r#""source_type":"EXTERNAL""#).await;
    let platform = signed_in_till(&monokulo);
    platform.set(
        "counter.device",
        r#"{"id":"dev_front","name":"Front iPad","learnedAt":1761148000}"#,
    );
    // Monokulo's event for it came first: no `found`, another till's.
    platform.set(
        "shift.sales",
        r#"[{"orderId":"ord_1","amount":"13.88","currency":"USD","reference":"bP9m","code":{"uri":"monero:8Bx?tx_amount=0.08","hasReceipt":false},"phase":"Waiting","expiresAt":1893456000,"xmr":"0.08","address":"8Bx","deviceName":"Bar iPad"}]"#,
    );
    let (engine, rx, _) = start(config(Some(&square)), platform.clone());
    engine.send(Action::Backgrounded);
    let alert = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Notification { view }) => view.alert.clone(),
        _ => None,
    });
    assert_eq!(alert.order_id, "ord_1");
    assert_eq!(alert.title, "Sam · $13.88");
    assert_eq!(
        platform.get("alerts.posted").as_deref(),
        Some(r#"["ord_1"]"#)
    );
    // Looks go on (a ding wakes one); the sale is never alerted again.
    engine.send(Action::PushReceived {
        content: br#"{"wake":"square"}"#.to_vec(),
        decrypted: true,
    });
    engine.send(Action::CopyAmount {
        order_id: "ord_1".into(),
    });
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Notification { view }) if view.alert.is_some() => {
            panic!("alerted twice: {view:?}")
        }
        Heard::Effect(Effect::CopyText { .. }) => Some(()),
        _ => None,
    });
}
