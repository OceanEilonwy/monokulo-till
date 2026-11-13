//! The engine against a platform with its secrets in memory and stand-ins
//! for Monokulo and Square: what it publishes, what it keeps under
//! `Stores.kt`'s keys, and what it asks the platform to do.

use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use monokulo_client::FixedClock;
use till_screens::onboarding::OnboardingPage;
use till_screens::shift::ShiftFilter;
use till_screens::wallet_setup::{CheckMark, PaymentsMode};
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

/// A data directory of the test's own, for the ledger and the locks.
pub(crate) fn data_dir() -> String {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("till-engine-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_string_lossy().into_owned()
}

fn config(square: Option<&MockServer>) -> EngineConfig {
    EngineConfig {
        data_dir: data_dir(),
        device: Device::Iphone,
        device_name: "iPhone 17".into(),
        app_version: "0.1.0".into(),
        debug: true,
        utc_offset: 0,
        hours_24: false,
        square: SquareSetup {
            application_id: "sq0idp-till".into(),
            environment: crate::SquareEnvironment::Production,
            redirect_uri: "https://till.monokulo.com/square".into(),
            base_url: square.map(MockServer::uri),
            return_uri: None,
        },
        clover: CloverSetup {
            application_id: "clv-till".into(),
            environment: crate::CloverEnvironment::Sandbox,
            redirect_uri: "https://till.monokulo.com/clover".into(),
            base_url: None,
            return_uri: None,
        },
        bell_url: "https://bell.monokulo.com".into(),
        relay_url: None,
        large_screen: false,
    }
}

/// The platform Monokulo's apps list shows is the device's.
#[test]
fn the_platform_is_the_devices() {
    assert_eq!(config(None).platform(), "ios");
    let ipad = EngineConfig {
        device: Device::Ipad,
        ..config(None)
    };
    assert_eq!(ipad.platform(), "ios");
    let android = EngineConfig {
        device: Device::Android,
        ..config(None)
    };
    assert_eq!(android.platform(), "android");
}

#[test]
fn an_android_tablet_draws_the_ipads_pages_and_a_phone_doesnt() {
    let on = |device, large_screen| {
        EngineConfig {
            device,
            large_screen,
            ..config(None)
        }
        .tablet()
    };
    assert!(on(Device::Ipad, false));
    assert!(on(Device::Android, true));
    assert!(!on(Device::Android, false));
    // iOS says which it is by its device: the flag is Android's.
    assert!(!on(Device::Iphone, true));
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

// Every server here is the test's own (`MockServer::builder().start()`),
// never wiremock's pooled ones: an engine a test leaves running keeps
// posting to the addresses it was given, and a pooled server handed on to
// the next test on the same port took those stray requests as its own
// (the relay test's flake on CI, 2026-11-12).

/// Monokulo's entry and one order, far from running out.
async fn monokulo() -> MockServer {
    let server = MockServer::builder().start().await;
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
            r#"{"order_id":"ord_1","status":"pending","is_final":false,"address":"8Bx","xmr_amount_piconero":80000000000,"amount_received_piconero":0,"confirmations":0,"confirmations_required":10,
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
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/v2/payments"))
        .respond_with(json(PAYMENTS))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/refunds"))
        // As Square answers: the refund made, still PENDING.
        .respond_with(json(
            r#"{"refund":{"id":"r1","status":"PENDING","amount_money":{"amount":1388,"currency":"USD"},"payment_id":"pay_monero","location_id":"L1","created_at":"2026-10-22T16:08:01.000Z"}}"#,
        ))
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
    // V1: Monokulo (docs/view-key-mode.md §11.1).
    engine.send(Action::ChoosePaymentsMode {
        mode: PaymentsMode::Monokulo,
    });
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

/// Decision 213: an action that waits is logged by its name alone, never
/// with the redirect, auth secret, token or link it carries.
#[test]
fn a_waiting_action_is_logged_without_what_it_carries() {
    let platform = Arc::new(FakePlatform::default());
    platform.fail(true);
    let (engine, rx, recorder) = start(config(None), platform.clone());
    assert!(problem(&rx).starts_with("Couldn't read what this till keeps"));
    engine.send(Action::SignInReturned {
        url: "https://till.example/square?code=sq0cgp-SECRETCODE&state=STATE1".into(),
    });
    engine.send(Action::PushEndpoint {
        url: "https://push.example/1".into(),
        p256dh: Some("BKey".into()),
        auth: Some("AUTHSECRET".into()),
    });
    engine.send(Action::PushToken {
        token: Some("APNSTOKEN".into()),
    });
    engine.send(Action::SharedText {
        text: "https://pay.example/apps/connect#token=ct_SHARED".into(),
    });
    platform.fail(false);
    engine.send(Action::Opened);
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Work { problem: None, .. }) => Some(()),
        _ => None,
    });
    let logs = recorder.1.lock().unwrap().clone();
    for name in ["SignInReturned", "PushEndpoint", "SharedText"] {
        assert!(
            logs.iter().any(|l| l.contains(&format!("so {name} waits"))),
            "{name}: {logs:?}"
        );
    }
    for secret in [
        "SECRETCODE",
        "STATE1",
        "AUTHSECRET",
        "APNSTOKEN",
        "ct_SHARED",
    ] {
        assert!(
            !logs.iter().any(|l| l.contains(secret)),
            "{secret}: {logs:?}"
        );
    }
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
    let monokulo = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/apps/connect"))
        .respond_with(json(r#"{"app_key":"ak_x","api":"/api/v1"}"#))
        .mount(&monokulo)
        .await;
    let platform = Arc::new(FakePlatform::default());
    let (engine, rx, _) = start(config(None), platform.clone());
    engine.send(Action::Next);
    // V1: Monokulo (docs/view-key-mode.md §11.1).
    engine.send(Action::ChoosePaymentsMode {
        mode: PaymentsMode::Monokulo,
    });
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
    let square = MockServer::builder().start().await;
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
    let square = MockServer::builder().start().await;
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
    assert_eq!(
        platform.get("shift.sales"),
        None,
        "the kept shift went into the ledger"
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
            .filter(|r| r.method.as_str() == "POST" && r.url.path() == "/v2/refunds")
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
        .find(|r| r.method.as_str() == "POST" && r.url.path() == "/v2/refunds")
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

/// Core.md decision 223: "New shift" opens a new window now, whether or not
/// the shift was just started.
#[tokio::test(flavor = "multi_thread")]
async fn new_shift_starts_a_new_window_now() {
    let monokulo = monokulo().await;
    let square = square().await;
    let platform = signed_in_till(&monokulo);
    platform.set("shift.started", "1000");
    let (engine, _rx, _) = start(config(Some(&square)), platform.clone());
    engine.send(Action::NewShift);
    let mut started = None;
    for _ in 0..100 {
        started = platform
            .get("shift.started")
            .and_then(|v| v.parse::<i64>().ok());
        if started.is_some_and(|at| at > 1000) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(started.is_some_and(|at| at > 1000), "{started:?}");
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

/// An iPad that kept the dropped floating window (core.md decision 225)
/// reads it as the alert, and has no Live Activity.
#[tokio::test(flavor = "multi_thread")]
async fn an_ipad_that_kept_the_floating_window_shows_sales_by_alert() {
    let monokulo = monokulo().await;
    let square = square().await;
    let platform = signed_in_till(&monokulo);
    platform.set("sales.show", "pip");
    let ipad = EngineConfig {
        device: Device::Ipad,
        ..config(Some(&square))
    };
    let (_engine, rx, _) = start(ipad, platform);
    assert_eq!(live_activity(&rx), None, "no Live Activity on iPad");
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
    engine.poke(|s| s.paid_dwell_ended(&"ord_1".into()));
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
    engine.poke(|s| s.paid_dwell_ended(&"ord_1".into()));
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
    let monokulo = MockServer::builder().start().await;
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
    let monokulo = MockServer::builder().start().await;
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

/// "Close the sale", then "No: refund it in Square": the till refunds the
/// Square payment once (decision 21's idempotency key) and the sale's page
/// shows it refunded, though Square answers the refund PENDING (core.md
/// decision 181).
#[tokio::test(flavor = "multi_thread")]
async fn closing_then_refunding_refunds_in_square_and_the_page_shows_it() {
    let monokulo = monokulo().await;
    let square = square().await;
    let (engine, rx, _) = start(config(Some(&square)), signed_in_till(&monokulo));
    let sale = Screen::Sale {
        order_id: "ord_1".into(),
    };
    screen(&rx, sale.clone());
    engine.send(Action::AskClose {
        order_id: "ord_1".into(),
    });
    engine.send(Action::Close {
        order_id: "ord_1".into(),
        choice: CloseChoice::RefundIt,
    });
    let page = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Sale { order_id, page, .. })
            if order_id == "ord_1" && page.line.starts_with("Refunded") =>
        {
            Some(page.clone())
        }
        _ => None,
    });
    assert!(page.amount_muted, "refunded, it no longer counts");
    assert!(
        page.buttons.is_empty(),
        "nothing left to refund: {:?}",
        page.buttons
    );
    let refunds: Vec<String> = square
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() == "POST" && r.url.path() == "/v2/refunds")
        .map(|r| String::from_utf8(r.body).unwrap())
        .collect();
    assert_eq!(refunds.len(), 1, "{refunds:?}");
    assert!(
        refunds[0].contains(r#""idempotency_key":"till-refund-pay_monero""#),
        "{}",
        refunds[0]
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
    // V1: Monokulo (docs/view-key-mode.md §11.1).
    engine.send(Action::ChoosePaymentsMode {
        mode: PaymentsMode::Monokulo,
    });
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
    // V1: Monokulo (docs/view-key-mode.md §11.1).
    engine.send(Action::ChoosePaymentsMode {
        mode: PaymentsMode::Monokulo,
    });
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
    assert_eq!(way(&rx, Screen::PaymentsMode), Some((true, true)));
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
        assert_eq!(of_place(place(&s), &Screen::Till), Some(s));
    }
    assert_eq!(of_place(place(&Screen::Settings), &Screen::Till), None);
    // On a keypad till the shift's place is the keypad.
    assert_eq!(
        of_place(place(&Screen::Keypad), &Screen::Keypad),
        Some(Screen::Keypad)
    );
}

/// Waits up to 10 s for `server` to have a request that `pick` takes.
/// Pick by method and path, never any request: wiremock lends its
/// servers from a pool, so a request an earlier test's engine still had in
/// flight can land on this test's server.
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
    let bell = MockServer::builder().start().await;
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
    let relay = MockServer::builder().start().await;
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

    // The APNs registration itself, not merely a request to the right path.
    let registered = request_to(&relay, |r| {
        r.method.as_str() == "POST"
            && r.url.path() == "/v1/register"
            && serde_json::from_slice::<serde_json::Value>(&r.body)
                .is_ok_and(|body| body["platform"] == "apns")
    })
    .await;
    let sent: serde_json::Value = serde_json::from_slice(&registered.body).unwrap();
    assert_eq!(
        sent,
        serde_json::json!({"platform": "apns", "token": APNS, "app": "till", "vapid_public_key": VAPID})
    );
    let rung = request_to(&bell, |r| {
        r.method.as_str() == "POST"
            && r.url.path() == "/v1/register"
            && serde_json::from_slice::<serde_json::Value>(&r.body)
                .is_ok_and(|body| body.get("subscription").is_some())
    })
    .await;
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
    let server = MockServer::builder().start().await;
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
        mode: crate::ShowSales::SideBySide,
    });
    wait_for(&rx, |_| platform.get("sales.show").filter(|v| v == "side"));
    engine.send(Action::Next);
    screen(&rx, Screen::Ready);
}

/// Core.md decision 221: with cash as Monero off, a cash sale is ignored:
/// no row, no alert, no code, nothing for the cashier to take.
#[tokio::test(flavor = "multi_thread")]
async fn with_cash_as_monero_off_a_cash_sale_is_ignored() {
    let monokulo = monokulo().await;
    let square = square_with_device("dev_front", "Front iPad", r#""source_type":"CASH""#).await;
    let platform = signed_in_till(&monokulo);
    platform.set("cash.monero", "false");
    let (engine, rx, _) = start(config(Some(&square)), platform.clone());
    engine.send(Action::Backgrounded);
    shift_with(&rx, 0);
    // Give the till time to read Square's payment, then see that nothing
    // about it ever reached the screens.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    while let Ok(heard) = rx.try_recv() {
        match heard {
            Heard::Update(Update::Shift { view }) => {
                assert_eq!(rows(&view), 0, "a cash sale is no row: {view:?}")
            }
            Heard::Update(Update::Notification { view }) => {
                assert!(view.alert.is_none(), "alerted: {view:?}")
            }
            _ => {}
        }
    }
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

/// Core.md decision 142: Clover runs on its own device, with Till beside it.
/// A Clover payment names its device by id alone, so the engine reads
/// Clover's devices once for their names, keeps them, and the counters
/// menu and Settings call the device by Clover's name.
#[tokio::test(flavor = "multi_thread")]
async fn a_clover_till_names_the_device_by_clovers_list_and_offers_the_counters_settings() {
    let monokulo = monokulo().await;
    let clover = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/v3/merchants/MID1/payments"))
        .respond_with(json(
            r#"{"elements":[{"id":"PAY_1","amount":1388,"result":"SUCCESS","createdTime":1761149220000,
                "tender":{"id":"T_MONERO"},"device":{"id":"DEV3F2A"},"refunds":{"elements":[]}}]}"#,
        ))
        .mount(&clover)
        .await;
    Mock::given(method("GET"))
        .and(path("/v3/merchants/MID1/devices"))
        .respond_with(json(
            r#"{"elements":[{"id":"DEV3F2A","name":"Front counter","model":"Clover Mini"}]}"#,
        ))
        .mount(&clover)
        .await;
    let connection = format!(r#"{{"app_key":"ak_1","api":"{}/api/v1"}}"#, monokulo.uri());
    let platform = FakePlatform::with(&[
        ("setup.done", "true"),
        ("pos.kind", "clover"),
        (
            "clover.merchant",
            r#"{"id":"MID1","name":"The Bean Room","currency":"USD"}"#,
        ),
        (
            "clover.tokens",
            r#"{"access_token":"c1","expires_at":"2030-01-01T00:00:00Z","refresh_token":"r1","merchant_id":"MID1"}"#,
        ),
        ("clover.tender.MID1", "T_MONERO"),
        ("clover.since", "2026-10-22T16:06:00Z"),
        ("clover.handled", "[]"),
        ("shift.started", "1761148000"),
        (
            "counter.device",
            r#"{"id":"DEV3F2A","learnedAt":1761148000}"#,
        ),
    ]);
    platform.set("connection", &connection);
    let mut config = config(None);
    config.clover.base_url = Some(clover.uri());
    let (engine, rx, _) = start(config, platform.clone());
    let counters = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Settings { view }) => view
            .counters
            .as_ref()
            .and_then(|c| c.this_device.as_ref())
            .filter(|this| this.label == "This device: Front counter")
            .map(|_| view.counters.clone().unwrap()),
        _ => None,
    });
    assert_eq!(counters.every_device.label, "Every device at The Bean Room");
    assert_eq!(
        platform.get("clover.devices").as_deref(),
        Some(r#"{"DEV3F2A":"Front counter"}"#)
    );
    let asked = clover
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path().ends_with("/devices"))
        .count();
    assert_eq!(asked, 1, "Clover's devices are read once");
    drop(engine);
}

/// The owner's fix 2: a payment while Till is behind is a notice on iPad and
/// on an iPhone without the Live Activity, as on Android; on an iPhone with
/// the Live Activity, the island says it.
#[tokio::test(flavor = "multi_thread")]
async fn a_payment_while_behind_is_a_notice_on_ipad_and_on_an_iphone_without_the_live_activity() {
    for (device, island, notice) in [
        (Device::Ipad, None, true),
        (Device::Iphone, Some("false"), true),
        (Device::Iphone, None, false),
    ] {
        let monokulo = monokulo().await;
        let square = square().await;
        let platform = signed_in_till(&monokulo);
        if let Some(on) = island {
            platform.set("island.activity", on);
        }
        let mut config = config(Some(&square));
        config.device = device;
        let (engine, rx, _) = start(config, platform);
        engine.send(Action::Backgrounded);
        wait_for(&rx, |heard| match heard {
            Heard::Update(Update::Notification { view }) => view.alert.clone(),
            _ => None,
        });
        engine.poke(|s| {
            let p = s.parts.as_ref().unwrap();
            p.side.books().record(till_ledger::Fact::OrderObserved(
                till_ledger::OrderObserved {
                    order_id: till_ledger::OrderId::new("ord_1"),
                    phase: monokulo_client::SalePhase::Paid,
                    confirmations: None,
                    is_final: true,
                    received: None,
                    overpaid: false,
                    quote: None,
                    renew: None,
                },
            ));
        });
        // Two round trips through the engine's queue: the claim it made as
        // it recorded the payment is delivered by the second.
        let mut noticed = false;
        for _ in 0..2 {
            engine.send(Action::CopyAmount {
                order_id: "ord_1".into(),
            });
            wait_for(&rx, |heard| match heard {
                Heard::Effect(Effect::Notify { paid: true, .. }) => {
                    noticed = true;
                    None
                }
                Heard::Effect(Effect::CopyText { .. }) => Some(()),
                _ => None,
            });
        }
        assert_eq!(noticed, notice, "{device:?}, island {island:?}");
    }
}

/// The onboarding page published last, once it's one `pick` takes.
fn page_where<T>(rx: &Receiver<Heard>, pick: impl Fn(&OnboardingPage) -> Option<T>) -> T {
    wait_for(rx, |heard| match heard {
        Heard::Update(Update::Onboarding { view }) => pick(&view.page),
        _ => None,
    })
}

/// docs/view-key-mode.md §11.1: V1, V2, the address and view key typed
/// and checked as they are, This till, and the wallet made and kept as
/// the till's link, before the point of sale. A debug build sets up on
/// stagenet.
#[test]
fn a_wallet_typed_by_hand_is_checked_made_and_kept_as_the_tills_link() {
    let platform = Arc::new(FakePlatform::default());
    let config = config(None);
    let data_dir = config.data_dir.clone();
    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::Welcome);
    engine.send(Action::Next);
    screen(&rx, Screen::PaymentsMode);
    // Continue waits for a choice.
    engine.send(Action::Next);
    let mode = page_where(&rx, |page| match page {
        OnboardingPage::PaymentsMode { view } => Some(view.clone()),
        _ => None,
    });
    assert!(!mode.next.enabled);
    engine.peek(|s| assert_eq!(s.screen, Screen::PaymentsMode));
    engine.send(Action::ChoosePaymentsMode {
        mode: PaymentsMode::Wallet,
    });
    engine.send(Action::Next);
    screen(&rx, Screen::WalletEntry);
    engine.send(Action::ChooseWalletWay {
        way: till_screens::wallet_setup::WalletWay::Keys,
    });
    screen(&rx, Screen::WalletKeys);

    let keys = fob_keys::Keys::generate(till_wallet::Network::Stagenet).keys;
    let address = keys.primary_address();
    engine.send(Action::EditWalletKeys {
        network: till_screens::wallet_setup::WalletNetwork::Stagenet,
        address: address.clone(),
        view_key: keys.public_view_key_hex(),
    });
    let wrong = page_where(&rx, |page| match page {
        OnboardingPage::WalletKeys { view } if view.view_key.note.is_some() => Some(view.clone()),
        _ => None,
    });
    assert_eq!(wrong.address.note.as_deref(), Some("A Stagenet address"));
    assert!(!wrong.next.enabled);
    engine.send(Action::EditWalletKeys {
        network: till_screens::wallet_setup::WalletNetwork::Stagenet,
        address: address.clone(),
        view_key: keys.view_key_hex().to_string(),
    });
    page_where(&rx, |page| match page {
        OnboardingPage::WalletKeys { view } if view.next.enabled => Some(()),
        _ => None,
    });
    engine.send(Action::Next);
    screen(&rx, Screen::ThisTill);
    engine.send(Action::StepTillNumber { up: true });
    engine.send(Action::StepPaidAfter { up: true });
    let this_till = page_where(&rx, |page| match page {
        OnboardingPage::ThisTill { view }
            if view.number.as_ref().is_some_and(|n| n.value == "2")
                && view.paid_after.value == "15 confirmations" =>
        {
            Some(view.clone())
        }
        _ => None,
    });
    assert_eq!(
        this_till.number.unwrap().detail.as_deref(),
        Some("Account 2 in your wallet")
    );
    // Continue waits while the node is checked; the test makes the wallet
    // whatever the check says.
    engine.peek(|s| assert!(s.wallet_step().is_some()));
    engine.poke(|s| s.finish_wallet());
    screen(&rx, Screen::PointOfSale);
    let saved = platform.get("connection").expect("the link kept");
    let link = till_payments::Link::restore(&saved).expect("a link");
    let till_payments::Link::Wallet(wallet) = link else {
        panic!("a wallet link: {saved}");
    };
    assert_eq!(
        (wallet.network.as_str(), wallet.till_number),
        ("stagenet", 2)
    );
    assert_eq!(wallet.settled_confirmations, 15);
    assert!(
        !saved.contains(&*keys.view_key_hex()),
        "the keys are in the wallet file only"
    );
    assert!(
        std::path::Path::new(&data_dir)
            .join(till_wallet::WALLET_FILE)
            .exists()
    );
    // With only this till's payments, "Set up later" isn't offered.
    let pos = page_where(&rx, |page| match page {
        OnboardingPage::PointOfSale { view } => Some(view.clone()),
        _ => None,
    });
    assert!(pos.skip.is_none(), "{:?}", pos.skip);
}

/// A stagenet light wallet server on 127.0.0.1 that answers `/get_version`
/// as monokulo-lws does (and anything else with an empty wallet): its
/// address.
fn light_server_stub() -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut request = Vec::new();
            let mut buf = [0; 4096];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
                let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
                    continue;
                };
                let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                let len = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if request.len() >= end + 4 + len {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&request);
            let body = if text.starts_with("POST /get_version") {
                r#"{"server_type":"monokulo-lws","monero_version_full":"","blockchain_height":2250000,"api":2,"max_subaddresses":10000,"network":"stage","testnet":true}"#
            } else if text.starts_with("POST /login") {
                r#"{"new_address":true,"start_height":2249990}"#
            } else if text.starts_with("POST /get_address_txs") {
                r#"{"total_received":"0","scanned_height":2249999,"scanned_block_height":2249999,"start_height":2249990,"transaction_height":2249999,"blockchain_height":2250000,"transactions":[]}"#
            } else {
                r#"{"new_subaddrs":[]}"#
            };
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    url
}

/// docs/view-key-mode.md §5.7, §11.1: the light wallet server's address is
/// checked before the wallet, the wallet's network is taken from it, This
/// till's node checks its proofs, and the link keeps the server.
#[test]
fn a_light_wallet_server_is_checked_then_kept_with_the_wallet() {
    let platform = Arc::new(FakePlatform::default());
    let mut config = config(None);
    // A release build starts on mainnet: the server's network wins.
    config.debug = false;
    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::Welcome);
    engine.send(Action::Next);
    engine.send(Action::ChoosePaymentsMode {
        mode: PaymentsMode::LightServer,
    });
    engine.send(Action::Next);
    screen(&rx, Screen::LightServer);
    let empty = page_where(&rx, |page| match page {
        OnboardingPage::LightServer { view } => Some(view.clone()),
        _ => None,
    });
    assert!(!empty.next.enabled, "nothing typed");
    assert!(empty.footnote.contains("sees every payment"));

    // Plain http to another machine fails its check and stays.
    engine.send(Action::EditLightServer {
        url: "http://192.0.2.1:8443".into(),
        self_signed: false,
    });
    engine.send(Action::Next);
    let refused = page_where(&rx, |page| match page {
        OnboardingPage::LightServer { view }
            if view.checks.iter().any(|c| c.mark == CheckMark::Failed) =>
        {
            Some(view.clone())
        }
        _ => None,
    });
    assert!(refused.checks[0].text.starts_with("Needs https"));
    engine.peek(|s| assert_eq!(s.screen, Screen::LightServer));

    let server = light_server_stub();
    engine.send(Action::EditLightServer {
        url: server.clone(),
        self_signed: false,
    });
    engine.send(Action::Next);
    screen(&rx, Screen::WalletEntry);
    // Back and on again: not checked twice.
    engine.send(Action::Back);
    screen(&rx, Screen::LightServer);
    let checked = page_where(&rx, |page| match page {
        OnboardingPage::LightServer { view } => Some(view.clone()),
        _ => None,
    });
    assert_eq!(checked.checks.len(), 3);
    assert!(checked.checks.iter().all(|c| c.mark == CheckMark::Passed));
    assert_eq!(checked.checks[1].text, "Stagenet");
    engine.send(Action::Next);
    screen(&rx, Screen::WalletEntry);

    engine.send(Action::ChooseWalletWay {
        way: till_screens::wallet_setup::WalletWay::Keys,
    });
    screen(&rx, Screen::WalletKeys);
    let typing = page_where(&rx, |page| match page {
        OnboardingPage::WalletKeys { view } => Some(view.clone()),
        _ => None,
    });
    assert!(
        typing.networks[1].selected,
        "the server's network: stagenet"
    );
    let keys = fob_keys::Keys::generate(till_wallet::Network::Stagenet).keys;
    engine.send(Action::EditWalletKeys {
        network: till_screens::wallet_setup::WalletNetwork::Stagenet,
        address: keys.primary_address(),
        view_key: keys.view_key_hex().to_string(),
    });
    page_where(&rx, |page| match page {
        OnboardingPage::WalletKeys { view } if view.next.enabled => Some(()),
        _ => None,
    });
    engine.send(Action::Next);
    screen(&rx, Screen::ThisTill);
    let this_till = page_where(&rx, |page| match page {
        OnboardingPage::ThisTill { view } => Some(view.clone()),
        _ => None,
    });
    assert_eq!(this_till.node.unwrap().label, "Node for proofs");
    engine.poke(|s| s.finish_wallet());
    screen(&rx, Screen::PointOfSale);
    let saved = platform.get("connection").expect("the link kept");
    let Some(till_payments::Link::Wallet(wallet)) = till_payments::Link::restore(&saved) else {
        panic!("a wallet link: {saved}");
    };
    assert_eq!(wallet.light_server.as_deref(), Some(server.as_str()));
    assert!(!wallet.light_self_signed);
    assert_eq!(wallet.network, "stagenet");
    assert_eq!(
        wallet.node.as_deref(),
        Some(till_wallet::setup::default_node(till_wallet::Network::Stagenet).as_str()),
        "the node for proofs"
    );
}

/// Connect to Fob by two codes (C1 to C4): the answer opens only with the
/// request on screen; Fob chose the account and shared no node, so This
/// till asks for the node alone.
#[test]
fn connect_to_fob_takes_fobs_sealed_answer_and_skips_the_till_number() {
    let platform = Arc::new(FakePlatform::default());
    let (engine, rx, _) = start(config(None), platform.clone());
    screen(&rx, Screen::Welcome);
    engine.send(Action::Next);
    engine.send(Action::ChoosePaymentsMode {
        mode: PaymentsMode::Wallet,
    });
    engine.send(Action::Next);
    screen(&rx, Screen::WalletEntry);
    engine.send(Action::ConnectFob);
    screen(&rx, Screen::FobRequest);
    let request = page_where(&rx, |page| match page {
        OnboardingPage::FobRequest { view } => Some(view.clone()),
        _ => None,
    });
    assert_eq!(request.pattern.cells.len(), 25);
    // Fob, on a phone: it scans the request and seals its answer.
    let asked = fob_codes::connect::Request::parse(&request.qr).unwrap();
    assert_eq!(asked.app, "Till");
    let keys = fob_keys::Keys::generate(till_wallet::Network::Stagenet).keys;
    let grant = fob_codes::connect::Grant {
        address: keys.primary_address(),
        view_key: keys.view_key_hex().to_string(),
        network: "stagenet".into(),
        account: 3,
        label: "Till (Corner Café)".into(),
        restore_height: 2_250_000,
        wallet_id: "w1".into(),
        wallet_name: "Corner Café".into(),
        node: None,
        light: None,
        prices: Some(fob_codes::connect::SharedPrices {
            provider: "Kraken".into(),
            currency: "AUD".into(),
        }),
    };
    engine.send(Action::FobAnswer {
        text: "monero:4abc".into(),
    });
    page_where(&rx, |page| match page {
        OnboardingPage::FobRequest { view } => view.problem.clone(),
        _ => None,
    });
    engine.send(Action::FobAnswer {
        text: fob_codes::connect::seal(&asked, &grant).unwrap(),
    });
    screen(&rx, Screen::FobConnected);
    let connected = page_where(&rx, |page| match page {
        OnboardingPage::FobConnected { view } => Some(view.clone()),
        _ => None,
    });
    let facts: Vec<String> = connected.facts.iter().map(|f| f.value.clone()).collect();
    assert_eq!(
        facts,
        [
            "Corner Café",
            "Till (Corner Café) · 3",
            "Kraken · AUD · from Fob"
        ]
    );
    engine.send(Action::StepCodeExpiry { up: true });
    engine.send(Action::Next);
    screen(&rx, Screen::ThisTill);
    let this_till = page_where(&rx, |page| match page {
        OnboardingPage::ThisTill { view } => Some(view.clone()),
        _ => None,
    });
    assert!(this_till.number.is_none(), "Fob chose the account");
    assert!(this_till.node.is_some());
    assert_eq!(this_till.expiry.value, "45 minutes");
    engine.poke(|s| s.finish_wallet());
    screen(&rx, Screen::PointOfSale);
    let saved = platform.get("connection").unwrap();
    let Some(till_payments::Link::Wallet(wallet)) = till_payments::Link::restore(&saved) else {
        panic!("a wallet link: {saved}");
    };
    assert_eq!(wallet.till_number, 3);
    assert_eq!(wallet.expiry_minutes, 45);
    assert_eq!(wallet.price_provider.as_deref(), Some("Kraken"));
    let fob = wallet.fob.unwrap();
    assert_eq!(
        (
            fob.wallet_name.as_str(),
            fob.prices_from_fob,
            fob.node_from_fob
        ),
        ("Corner Café", true, false)
    );
}

/// D3: the words are taken once, and the count is checked first.
#[test]
fn seed_words_are_counted_and_turned_into_the_view_key() {
    let (engine, rx, _) = start(config(None), Arc::new(FakePlatform::default()));
    screen(&rx, Screen::Welcome);
    engine.send(Action::Next);
    engine.send(Action::ChoosePaymentsMode {
        mode: PaymentsMode::Wallet,
    });
    engine.send(Action::Next);
    engine.send(Action::ChooseWalletWay {
        way: till_screens::wallet_setup::WalletWay::Seed,
    });
    screen(&rx, Screen::WalletSeed);
    engine.send(Action::EnterSeed {
        words: "one two three".into(),
    });
    let problem = page_where(&rx, |page| match page {
        OnboardingPage::WalletSeed { view } => view.problem.clone(),
        _ => None,
    });
    assert_eq!(problem, "Enter all 25 words: there are 3.");
    let restored = fob_keys::Keys::generate(till_wallet::Network::Stagenet);
    let (_, words) = restored.keys.seed_words().unwrap();
    engine.send(Action::ChooseSeedLength { words: 16 });
    engine.send(Action::EnterSeed {
        words: words.to_string(),
    });
    screen(&rx, Screen::ThisTill);
    engine.send(Action::Back);
    screen(&rx, Screen::WalletSeed);
}

/// docs/view-key-mode.md §10: a till on its own wallet registers no Bell
/// and runs no foreground service.
#[test]
fn a_till_on_its_own_wallet_has_no_foreground_service_and_no_bell() {
    let platform = FakePlatform::with(&[("setup.done", "true")]);
    let link = till_payments::Link::Wallet(till_payments::WalletLink::new(
        "stagenet",
        1,
        till_types::Secret::from("00".repeat(32)),
    ));
    platform.set("connection", &link.save());
    let (engine, rx, _) = start(config(None), platform);
    screen(&rx, Screen::Till);
    engine.peek(|s| {
        let p = s.parts.as_ref().unwrap();
        assert!(p.side.wallet_mode());
        assert_eq!(
            till_bell::PosSide::signed_in(&*p.side).unwrap(),
            None,
            "nothing for a Bell to register"
        );
    });
}

/// A till set up on its own wallet (made in `data_dir`), at Square's Front
/// Counter: what Settings and its changes are tried on.
fn wallet_till(data_dir: &str) -> (Arc<FakePlatform>, till_payments::WalletLink) {
    wallet_till_with(data_dir, till_wallet::WalletSetup::new(2))
}

/// [`wallet_till`], set up as `setup` says.
fn wallet_till_with(
    data_dir: &str,
    setup: till_wallet::WalletSetup,
) -> (Arc<FakePlatform>, till_payments::WalletLink) {
    let keys = fob_keys::Keys::generate(till_wallet::Network::Stagenet).keys;
    let view = till_wallet::keys::keys_from_typed(
        till_wallet::Network::Stagenet,
        &keys.primary_address(),
        &keys.view_key_hex(),
    )
    .unwrap();
    let made = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(till_wallet::make_wallet(
            std::path::Path::new(data_dir),
            &view,
            &setup,
        ))
        .unwrap();
    let link = made.link.clone();
    drop(made);
    let platform = FakePlatform::with(&[("setup.done", "true")]);
    platform.set(
        "connection",
        &till_payments::Link::Wallet(link.clone()).save(),
    );
    (platform, link)
}

fn kept_wallet(platform: &FakePlatform) -> till_payments::WalletLink {
    let saved = platform.get("connection").expect("a link");
    match till_payments::Link::restore(&saved) {
        Some(till_payments::Link::Wallet(wallet)) => wallet,
        other => panic!("a wallet link: {other:?}"),
    }
}

/// Decision 234: Settings shows the wallet in place of a store, and its
/// steppers and price source change the wallet's link.
#[test]
fn wallet_settings_show_the_wallet_and_change_its_link() {
    let config = config(None);
    let (platform, link) = wallet_till(&config.data_dir);
    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::Till);
    engine.send(Action::OpenSettings);
    let section = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Settings { view }) => view.wallet.clone(),
        _ => None,
    });
    assert!(section.wallet_address.starts_with('5'));
    assert_eq!(
        section.till_number.unwrap().detail.as_deref(),
        Some("In your wallet: account 2")
    );
    assert_eq!(section.disconnect.label, "Disconnect this wallet");
    engine.send(Action::StepTillNumber { up: true });
    wait_for(&rx, |_| {
        (kept_wallet(&platform).till_number == 3).then_some(())
    });
    engine.send(Action::StepPaidAfter { up: true });
    wait_for(&rx, |_| {
        (kept_wallet(&platform).settled_confirmations == 15).then_some(())
    });
    engine.send(Action::StepCodeExpiry { up: false });
    wait_for(&rx, |_| {
        (kept_wallet(&platform).expiry_minutes == 20).then_some(())
    });
    engine.send(Action::ChoosePriceProvider {
        name: "CoinGecko".into(),
    });
    wait_for(&rx, |_| {
        (kept_wallet(&platform).price_provider.as_deref() == Some("CoinGecko")).then_some(())
    });
    // A provider the currency has none of isn't taken.
    engine.send(Action::ChoosePriceProvider {
        name: "Nobody".into(),
    });
    let kept = kept_wallet(&platform);
    assert_eq!(kept.file_key, link.file_key, "the same wallet");
    assert_eq!(kept.price_provider.as_deref(), Some("CoinGecko"));
}

/// Decision 245: on a light wallet server, Settings shows the server in
/// the node's row, checks it as it opens, and Change takes a new server
/// (checked by the wallet first), never a node.
#[test]
fn light_server_settings_show_and_change_the_server() {
    let config = config(None);
    let first = light_server_stub();
    let (platform, link) = wallet_till_with(
        &config.data_dir,
        till_wallet::WalletSetup {
            light_server: Some(first.clone()),
            ..till_wallet::WalletSetup::new(2)
        },
    );
    assert_eq!(link.light_server.as_deref(), Some(first.as_str()));
    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::Till);
    engine.send(Action::OpenSettings);
    let section = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Settings { view }) => view
            .wallet
            .clone()
            .filter(|w| w.node.as_ref().is_some_and(|n| n.checks.len() == 3)),
        _ => None,
    });
    assert_eq!(section.mode.value, "Your light wallet server");
    let row = section.node.unwrap();
    assert_eq!(row.label, "Light wallet server");
    assert_eq!(row.url, first);
    assert!(
        row.checks.iter().all(|c| c.mark == CheckMark::Passed),
        "{:?}",
        row.checks
    );
    assert!(
        section
            .facts
            .iter()
            .any(|f| f.label == "Proofs checked with")
    );

    let second = light_server_stub();
    engine.send(Action::SetNode {
        url: second.clone(),
    });
    wait_for(&rx, |_| {
        (kept_wallet(&platform).light_server.as_deref() == Some(second.as_str())).then_some(())
    });
    let kept = kept_wallet(&platform);
    assert_eq!(kept.node, link.node, "the node for proofs stays");
    assert_eq!(kept.file_key, link.file_key, "the same wallet");
}

/// Decision 253: Settings' "Accept its own certificate" switch changes the
/// light wallet server's setting once the wallet's check of it passes, and
/// the lines under the row are checked again; a server that fails the
/// check changes nothing.
#[test]
fn light_server_settings_switch_its_certificate_after_checking_it() {
    let config = config(None);
    let first = light_server_stub();
    let (platform, link) = wallet_till_with(
        &config.data_dir,
        till_wallet::WalletSetup {
            light_server: Some(first.clone()),
            ..till_wallet::WalletSetup::new(2)
        },
    );
    assert!(!link.light_self_signed);
    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::Till);
    engine.send(Action::OpenSettings);
    let row = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Settings { view }) => view
            .wallet
            .clone()
            .and_then(|w| w.node)
            .filter(|n| n.checks.len() == 3),
        _ => None,
    });
    let switch = row.self_signed.expect("the certificate switch");
    assert_eq!(switch.label, "Accept its own certificate");
    assert!(!switch.selected && switch.enabled);

    engine.send(Action::SetLightServer {
        url: first.clone(),
        self_signed: true,
    });
    // Checked again as the switch changes.
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Settings { view }) => view
            .wallet
            .as_ref()
            .and_then(|w| w.node.as_ref())
            .filter(|n| n.checks.iter().any(|c| c.mark == CheckMark::Checking))
            .map(|_| ()),
        _ => None,
    });
    let row = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Settings { view }) => {
            view.wallet.clone().and_then(|w| w.node).filter(|n| {
                n.self_signed.as_ref().is_some_and(|s| s.selected)
                    && n.checks.len() == 3
                    && n.checks.iter().all(|c| c.mark == CheckMark::Passed)
            })
        }
        _ => None,
    });
    assert_eq!(row.url, first);
    let kept = kept_wallet(&platform);
    assert!(kept.light_self_signed);
    assert_eq!(kept.light_server.as_deref(), Some(first.as_str()));
    assert_eq!(kept.file_key, link.file_key, "the same wallet");

    // A server that doesn't answer is refused: the link stays as it was.
    let dead = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port())
    };
    engine.send(Action::SetLightServer {
        url: dead,
        self_signed: false,
    });
    let section = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Settings { view }) => {
            view.wallet.clone().filter(|w| w.problem.is_some())
        }
        _ => None,
    });
    let row = section.node.unwrap();
    assert_eq!(row.url, first);
    assert!(row.self_signed.unwrap().selected);
    let kept = kept_wallet(&platform);
    assert_eq!(kept.light_server.as_deref(), Some(first.as_str()));
    assert!(kept.light_self_signed);
}

/// Disconnect forgets the wallet's link and deletes its file.
#[test]
fn disconnecting_a_wallet_deletes_its_file() {
    let config = config(None);
    let data_dir = config.data_dir.clone();
    let (platform, _) = wallet_till(&data_dir);
    let file = std::path::Path::new(&data_dir).join(till_wallet::WALLET_FILE);
    assert!(file.exists());
    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::Till);
    engine.send(Action::OpenSettings);
    engine.send(Action::Disconnect);
    screen(&rx, Screen::Welcome);
    assert!(platform.get("connection").is_none());
    assert!(!file.exists(), "the wallet file is gone");
}

/// Decision 241: a wallet payment whose code ran out unpaid offers New
/// code, as a Monokulo one does; while its code is up, it doesn't.
#[test]
fn a_wallet_payment_whose_code_ran_out_offers_new_code() {
    let config = config(None);
    let (platform, _) = wallet_till(&config.data_dir);
    let (engine, rx, _) = start(config, platform);
    screen(&rx, Screen::Till);
    // A keyed request in this till's account, as the wallet backend says
    // it stands while its code is up and once it ran out.
    let request = fob_wallet::KeyedRequest {
        id: 1,
        key: "sq:pay_w".into(),
        account: 2,
        minor: 1,
        address: "7Sub".into(),
        base: 84_000_000_000,
        tag: 123_456,
        atomic: 84_000_123_456,
        created: 1_761_149_220,
        expires_at: 1_761_150_000,
        height: 100,
        cancelled: None,
        note: Some("bP9m".into()),
    };
    let at = |now: u64| till_wallet::backend::payment_of(&request, &[], 10, now as i64, None);
    let (live, ran_out) = (at(request.expires_at - 60), at(request.expires_at + 60));
    assert!(live.renew.is_some(), "a wallet payment offers New code");
    let order_id = live.order_id.as_str().to_owned();
    let made = order_id.clone();
    let observed = |p: &till_payments::Payment| {
        till_ledger::Fact::OrderObserved(till_ledger::OrderObserved {
            order_id: till_ledger::OrderId::new(p.order_id.as_str()),
            phase: p.phase,
            confirmations: Some(p.confirmations),
            is_final: p.is_final,
            received: Some(p.received),
            overpaid: p.overpaid,
            quote: None,
            renew: p.renew.clone(),
        })
    };
    let (seen_live, seen_ran_out) = (observed(&live), observed(&ran_out));
    let renew = live.renew.clone();
    engine.poke(move |s| {
        let books = s.parts.as_ref().unwrap().side.books();
        books.record(till_ledger::Fact::PosPaymentSeen(
            till_ledger::PaymentSeen {
                pos: monokulo_client::PointOfSale::Square,
                payment: till_ledger::PaymentId::new("pay_w"),
                amount: till_types::Money::new(1388, till_types::CurrencyCode::USD),
                tender: till_ledger::Tender::Monero,
                status: "COMPLETED".into(),
                device_id: None,
                device_name: None,
                receipt_number: Some("bP9m".into()),
                order_ref: None,
                title: None,
                created_at: "2026-10-22T16:07:00Z".parse().unwrap(),
                updated_at: till_types::UnixMillis(1_761_149_220_000),
            },
        ));
        books.record(till_ledger::Fact::OrderCreated(till_ledger::OrderCreated {
            sale: till_ledger::SaleKey::pos(monokulo_client::PointOfSale::Square, "pay_w"),
            order_id: till_ledger::OrderId::new(made.as_str()),
            url: Some(made.clone()),
            payment_uri: "monero:7Sub?tx_amount=0.084123456".into(),
            address: "7Sub".into(),
            xmr: "0.084123456".into(),
            price: till_types::Price::new("13.88", "USD"),
            reference: Some("bP9m".into()),
            expires_at: till_types::UnixSeconds(1_761_150_000),
            confirmations_required: Some(10),
            device_name: None,
            renew,
            price_used: None,
            follows: None,
        }));
        books.record(seen_live);
    });
    let actions = |rx: &Receiver<Heard>, line: &str| {
        wait_for(rx, |heard| match heard {
            Heard::Update(Update::Sale {
                order_id: o, page, ..
            }) if o.as_str() == order_id && page.line.starts_with(line) => {
                Some(page.buttons.iter().map(|b| b.action).collect::<Vec<_>>())
            }
            _ => None,
        })
    };
    engine.send(Action::OpenSale {
        order_id: order_id.as_str().into(),
    });
    let live = actions(&rx, "");
    assert!(
        !live.contains(&till_screens::sale_page::PageAction::NewCode),
        "{live:?}"
    );
    engine.poke(move |s| {
        let books = s.parts.as_ref().unwrap().side.books();
        books.record(seen_ran_out);
    });
    let ended = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Sale {
            order_id: o, page, ..
        }) if o.as_str() == order_id
            && page
                .buttons
                .iter()
                .any(|b| b.action == till_screens::sale_page::PageAction::NewCode) =>
        {
            Some(page.clone())
        }
        _ => None,
    });
    let new_code = ended
        .buttons
        .iter()
        .find(|b| b.action == till_screens::sale_page::PageAction::NewCode)
        .unwrap();
    assert_eq!(new_code.press.label, "New code");
}

/// Connect to Fob's request outlives the process: a till started again
/// comes back on C1 with the same request, and Fob's answer opens.
#[test]
fn a_fob_request_is_kept_across_a_restart() {
    let platform = Arc::new(FakePlatform::default());
    let config = config(None);
    let (engine, rx, _) = start(config.clone(), platform.clone());
    screen(&rx, Screen::Welcome);
    engine.send(Action::Next);
    engine.send(Action::ChoosePaymentsMode {
        mode: PaymentsMode::Wallet,
    });
    engine.send(Action::Next);
    engine.send(Action::ConnectFob);
    let first = page_where(&rx, |page| match page {
        OnboardingPage::FobRequest { view } => Some(view.qr.clone()),
        _ => None,
    });
    assert!(platform.get("fob.request").is_some());
    drop(engine);

    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::FobRequest);
    let again = page_where(&rx, |page| match page {
        OnboardingPage::FobRequest { view } => Some(view.qr.clone()),
        _ => None,
    });
    assert_eq!(again, first);
    let asked = fob_codes::connect::Request::parse(&again).unwrap();
    let keys = fob_keys::Keys::generate(till_wallet::Network::Stagenet).keys;
    let grant = fob_codes::connect::Grant {
        address: keys.primary_address(),
        view_key: keys.view_key_hex().to_string(),
        network: "stagenet".into(),
        account: 1,
        label: "Till".into(),
        restore_height: 2_250_000,
        wallet_id: "w1".into(),
        wallet_name: "Corner Café".into(),
        node: None,
        light: None,
        prices: None,
    };
    engine.send(Action::FobAnswer {
        text: fob_codes::connect::seal(&asked, &grant).unwrap(),
    });
    screen(&rx, Screen::FobConnected);
    assert!(
        platform.get("fob.request").is_none(),
        "forgotten once answered"
    );
}

/// Prices (docs/view-key-mode.md §8): a till on its own wallet at a Square
/// location in dollars is asked where prices come from between the point
/// of sale and alerts; the provider picked and the location's currency are
/// kept in the wallet's link, so the wallet keeps that price fresh.
#[test]
fn a_wallet_till_picks_its_price_provider_between_the_point_of_sale_and_alerts() {
    let config = config(None);
    let (platform, _) = wallet_till(&config.data_dir);
    platform.remove("setup.done");
    platform.set("pos.kind", "square");
    platform.set(
        "square.location",
        r#"{"id":"L1","name":"Front counter","country":"US","currency":"USD"}"#,
    );
    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::Alerts);
    engine.send(Action::Back);
    screen(&rx, Screen::Prices);
    let prices = page_where(&rx, |page| match page {
        OnboardingPage::Prices { view } => Some(view.clone()),
        _ => None,
    });
    assert_eq!(prices.intro_bold, "US dollars");
    let names: Vec<_> = prices.providers.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Kraken", "CoinGecko"]);
    assert!(
        prices.providers[0].selected,
        "the first that prices dollars"
    );
    engine.send(Action::ChoosePriceProvider {
        name: "CoinGecko".into(),
    });
    wait_for(&rx, |_| {
        (kept_wallet(&platform).price_provider.as_deref() == Some("CoinGecko")).then_some(())
    });
    engine.send(Action::Next);
    screen(&rx, Screen::Alerts);
    let kept = kept_wallet(&platform);
    assert_eq!(kept.price_currency.as_deref(), Some("USD"));
    assert_eq!(kept.price_provider.as_deref(), Some("CoinGecko"));
    engine.send(Action::Back);
    screen(&rx, Screen::Prices);
    engine.send(Action::Back);
    screen(&rx, Screen::PointOfSale);
}

/// Till's keypad (docs/view-key-mode.md §7): on a till whose point of sale
/// is its own keypad, the keypad is home; XMR only takes a decimal point;
/// Charge makes the sale and opens its code, and leaving the code comes
/// back to the keypad with the sale put aside, live.
#[test]
fn a_keypad_till_charges_from_its_keypad_and_puts_the_sale_aside() {
    use till_screens::keypad::KeypadKey;
    let config = config(None);
    let (platform, _) = wallet_till(&config.data_dir);
    platform.set("pos.kind", "till");
    platform.set("keypad.currency", "XMR");
    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::Keypad);
    let keypad = |rx: &Receiver<Heard>,
                  want: &dyn Fn(&till_screens::keypad::KeypadView) -> bool| {
        wait_for(rx, |heard| match heard {
            Heard::Update(Update::Keypad { view: Some(view) }) if want(view) => Some(view.clone()),
            _ => None,
        })
    };
    let empty = keypad(&rx, &|v| v.amount == "0 XMR");
    assert!(!empty.charge.enabled, "nothing to charge at zero");
    assert!(empty.keys.iter().any(|k| k.key == KeypadKey::Point));
    for key in [
        KeypadKey::Digit { digit: 1 },
        KeypadKey::Point,
        KeypadKey::Digit { digit: 5 },
    ] {
        engine.send(Action::KeypadPress { key });
    }
    engine.send(Action::KeypadReference { text: "Sam".into() });
    let typed = keypad(&rx, &|v| v.amount == "1.5 XMR" && v.reference == "Sam");
    assert_eq!(typed.charge.label, "Charge 1.5 XMR");
    engine.send(Action::Charge);
    let order_id = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Screen {
            screen: Screen::Sale { order_id },
        }) => Some(order_id.clone()),
        _ => None,
    });
    engine.send(Action::Back);
    screen(&rx, Screen::Keypad);
    // Published as the sale was made, while its code was up.
    let aside = engine.peek(|s| s.keypad_view(s.parts.as_ref().unwrap()));
    assert_eq!(aside.aside.len(), 1, "{aside:?}");
    assert_eq!(aside.aside[0].order_id, order_id);
    assert_eq!(aside.aside[0].label, "Sam");
    assert_eq!(
        aside.amount, "0 XMR",
        "the keypad is cleared for the next one"
    );
    engine.send(Action::OpenSales);
    screen(&rx, Screen::Till);
    engine.send(Action::Back);
    screen(&rx, Screen::Keypad);
}

/// Setup with Till's keypad on the shop's own wallet: the keypad's currency
/// is chosen on step 4, the wallet prices it, and Prices comes next; XMR
/// only goes straight on to alerts.
#[test]
fn choosing_the_keypad_on_a_wallet_asks_for_its_currency_then_prices() {
    let config = config(None);
    let (platform, _) = wallet_till(&config.data_dir);
    platform.remove("setup.done");
    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::PointOfSale);
    engine.send(Action::ChoosePointOfSale {
        pos: crate::Pos::Till,
    });
    wait_for(&rx, |_| {
        (platform.get("pos.kind").as_deref() == Some("till")).then_some(())
    });
    let choose = page_where(&rx, |page| match page {
        OnboardingPage::PointOfSale { view }
            if view
                .next
                .as_ref()
                .is_some_and(|n| n.label == "Continue with Till") =>
        {
            Some(view.clone())
        }
        _ => None,
    });
    assert!(choose.currencies.is_empty(), "the choice alone first");
    engine.send(Action::Next);
    let currency = page_where(&rx, |page| match page {
        OnboardingPage::PointOfSale { view } if !view.currencies.is_empty() => Some(view.clone()),
        _ => None,
    });
    assert_eq!(currency.currencies[0].name, "None (XMR only)");
    assert!(
        !currency.next.unwrap().enabled,
        "Continue waits for a currency"
    );
    engine.send(Action::ChooseKeypadCurrency { code: "AUD".into() });
    wait_for(&rx, |_| {
        (kept_wallet(&platform).price_currency.as_deref() == Some("AUD")).then_some(())
    });
    assert_eq!(platform.get("keypad.currency").as_deref(), Some("AUD"));
    engine.send(Action::Next);
    screen(&rx, Screen::Prices);
    engine.send(Action::Back);
    screen(&rx, Screen::PointOfSale);
    engine.send(Action::ChooseKeypadCurrency { code: "XMR".into() });
    wait_for(&rx, |_| {
        (platform.get("keypad.currency").as_deref() == Some("XMR")).then_some(())
    });
    engine.send(Action::Next);
    screen(&rx, Screen::Alerts);
}

/// Prices runs on a till through the shop's light wallet server too: it's
/// the same wallet, so the same step comes between the point of sale and
/// alerts.
#[test]
fn a_light_server_till_is_asked_where_prices_come_from_too() {
    let config = config(None);
    let (platform, mut link) = wallet_till(&config.data_dir);
    link.light_server = Some("https://lws.example.com".into());
    platform.set("connection", &till_payments::Link::Wallet(link).save());
    platform.remove("setup.done");
    platform.set("pos.kind", "square");
    platform.set(
        "square.location",
        r#"{"id":"L1","name":"Front counter","country":"US","currency":"USD"}"#,
    );
    let (engine, rx, _) = start(config, platform.clone());
    screen(&rx, Screen::Alerts);
    engine.send(Action::Back);
    screen(&rx, Screen::Prices);
}
