//! A background look against stand-ins for Square and Monokulo, as
//! till-sales' tests do, over an Android till's kept state.

use std::sync::Arc;
use std::time::Duration;

use monokulo_client::FixedClock;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::engine::tests::{PAYMENTS, signed_in_till};
use crate::testing::FakePlatform;
use crate::{CloverSetup, Device, SquareSetup};

fn config(square: &MockServer) -> EngineConfig {
    EngineConfig {
        data_dir: std::env::temp_dir().to_string_lossy().into_owned(),
        device: Device::Iphone,
        device_name: "iPhone 17".into(),
        app_version: "0.1.0".into(),
        platform: "ios".into(),
        debug: false,
        utc_offset: 0,
        hours_24: false,
        square: SquareSetup {
            application_id: "sq0idp-till".into(),
            sandbox: false,
            redirect_uri: "https://till.monokulo.com/square".into(),
            base_url: Some(square.uri()),
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

async fn square(delay: Duration) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/payments"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(PAYMENTS, "application/json")
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    server
}

/// [`background_check`] as a platform calls it: on a thread of its own.
async fn look(
    config: EngineConfig,
    platform: Arc<FakePlatform>,
    deadline_ms: u64,
) -> BackgroundCheck {
    tokio::task::spawn_blocking(move || background_check(config, platform, deadline_ms))
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_look_makes_orders_for_new_sales_keeps_them_and_says_what_the_alert_says() {
    let (monokulo, square) = (monokulo().await, square(Duration::ZERO).await);
    let platform = signed_in_till(&monokulo);
    let check = look(config(&square), platform.clone(), 10_000).await;
    assert_eq!(check.problem, None);
    assert!(check.finished);
    assert_eq!(check.alerts.len(), 1);
    let alert = &check.alerts[0];
    assert_eq!(alert.order_id, "ord_1");
    assert_eq!(alert.title, "Sale #bP9m · $13.88");
    assert_eq!(
        alert.body,
        "Monero sale. Tap to show the customer the code."
    );
    assert_eq!(check.ongoing, None, "iOS has no ongoing notification");
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

    let again = look(config(&square), platform, 10_000).await;
    assert!(
        again.alerts.is_empty(),
        "a sale handled once isn't told of again"
    );
    assert!(again.finished);
}

/// Android's ding path: the shift's ongoing notification, alerting about
/// the sale the look found, in till-screens' words.
#[tokio::test(flavor = "multi_thread")]
async fn on_android_a_look_answers_the_ongoing_notification_alerting_about_the_new_sale() {
    let (monokulo, square) = (monokulo().await, square(Duration::ZERO).await);
    let mut config = config(&square);
    config.device = Device::Android;
    let check = look(config.clone(), signed_in_till(&monokulo), 10_000).await;
    assert!(
        check.alerts.is_empty(),
        "Android alerts with its ongoing one"
    );
    let Some(OngoingFace::Sale {
        order_id,
        face,
        alert,
        ..
    }) = check.ongoing
    else {
        panic!("{:?}", check.ongoing);
    };
    assert_eq!(order_id, "ord_1");
    assert!(alert);
    assert_eq!(face.title, "New Monero sale · Front counter");
    assert_eq!(face.public_text, "$13.88 · waiting for payment");
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_connection_or_a_shift_on_nothing_is_asked() {
    let (monokulo, square) = (monokulo().await, square(Duration::ZERO).await);
    let none = look(config(&square), Arc::new(FakePlatform::default()), 10_000).await;
    assert_eq!(
        none,
        BackgroundCheck {
            alerts: Vec::new(),
            ongoing: None,
            problem: None,
            finished: true
        }
    );
    let platform = signed_in_till(&monokulo);
    platform.set("shift.on", "false");
    let off = look(config(&square), platform, 10_000).await;
    assert!(off.alerts.is_empty() && off.problem.is_none());
    assert!(square.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_store_that_cant_answer_is_the_answers_problem() {
    let square = square(Duration::ZERO).await;
    let platform = Arc::new(FakePlatform::default());
    platform.fail(true);
    let check = look(config(&square), platform, 10_000).await;
    assert!(
        check
            .problem
            .unwrap()
            .starts_with("Couldn't read what this till keeps")
    );
    assert!(check.alerts.is_empty());
}

/// On tokio's paused clock: the deadline passes while Square, on a thread
/// of its own, takes a minute to answer.
#[tokio::test(start_paused = true)]
async fn the_deadline_ends_the_look_and_what_wasnt_done_waits_for_the_next() {
    let (monokulo, square) = (monokulo().await, square(Duration::from_secs(60)).await);
    let platform = signed_in_till(&monokulo);
    let started = std::time::Instant::now();
    let http = reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .unwrap();
    let check = check(
        config(&square),
        platform.clone(),
        http,
        Arc::new(FixedClock::at(1_761_149_280)),
        Duration::from_secs(5),
    )
    .await;
    assert!(!check.finished);
    assert!(check.alerts.is_empty());
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "not Square's minute"
    );
    assert_eq!(platform.get("shift.sales"), None);
    assert_eq!(platform.get("square.handled").as_deref(), Some("[]"));
}

/// The Notification Service Extension's question: a ding the relay carried,
/// opened with the keys the engine made when it registered.
#[tokio::test]
async fn a_relayed_ding_opens_with_the_kept_keys_for_the_tills_point_of_sale() {
    use crate::store::Store;
    use crate::web_push::PushKeys;
    use crate::web_push::tests::encrypt;

    let monokulo = monokulo().await;
    let platform = signed_in_till(&monokulo);
    let encrypted = |plaintext: &[u8]| {
        let keys = PushKeys::load_or_make(&Store::new(platform.clone()))
            .unwrap()
            .unwrap();
        encrypt(&keys.p256dh().unwrap(), &keys.auth(), plaintext)
    };
    let ding = encrypted(br#"{"wake":"square"}"#);
    assert!(is_relayed_ding(platform.clone(), ding.clone()));
    assert!(
        !is_relayed_ding(platform.clone(), encrypted(br#"{"wake":"clover"}"#)),
        "a ding for another point of sale"
    );
    assert!(!is_relayed_ding(
        platform.clone(),
        encrypted(br#"{"sale":"paid"}"#)
    ));
    assert!(!is_relayed_ding(platform.clone(), b"not a push".to_vec()));
    assert!(
        !is_relayed_ding(signed_in_till(&monokulo), ding),
        "another device's keys"
    );
}
