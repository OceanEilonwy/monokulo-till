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
        },
        clover: CloverSetup {
            application_id: "clv-till".into(),
            environment: "sandbox".into(),
            redirect_uri: "https://till.monokulo.com/clover".into(),
            base_url: None,
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
    screen(&rx, Screen::Till);
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Service { running: true }) => Some(()),
        _ => None,
    });
    let face = wait_for(&rx, |heard| match heard {
        Heard::Effect(Effect::Notify { face }) => Some(face.clone()),
        _ => None,
    });
    assert_eq!(face.id, "ord_1");
    assert_eq!(face.title, "Monero sale");
    assert_eq!(face.text, "$13.88 · Tap to show code");
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

    // The shift lists the kept sale and the new one, at the till's place.
    let shift = wait_for(&rx, |heard| match heard {
        Heard::Update(Update::Shift { view }) if rows(view) == 2 => Some(view.clone()),
        _ => None,
    });
    assert_eq!(shift.top.place.as_deref(), Some("Front counter"));

    engine.send(Action::CopyAddress {
        order_id: "ord_1".into(),
    });
    let copied = wait_for(&rx, |heard| match heard {
        Heard::Effect(Effect::CopyText { text }) => Some(text.clone()),
        _ => None,
    });
    assert_eq!(copied, "8Bx");

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
        choice: WakeUps::Timer,
    });
    wait_for(&rx, |heard| match heard {
        Heard::Update(Update::WakeUps { view }) if view.own_field.is_none() => Some(()),
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
