//! Ported from the Android app's `MonokuloApiTest`: the client against a
//! stand-in Monokulo that answers as Monokulo's API does, HAL-FORMS with
//! relative links and problem+json errors. The answers themselves are checked
//! against Monokulo's own OpenAPI document in `monokulo_contract.rs`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use monokulo_client::{
    AppIdentity, ConnectCode, Connection, FixedClock, MonokuloApi, MonokuloError, SalePhase,
    StreamEvent, proof,
};
use sha2::{Digest, Sha256};
use till_types::{CurrencyCode, Money};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate, matchers::any};

/// The entry as Monokulo answers an app's key: relative links, and its forms.
const ENTRY: &str = r#"{
    "_links":{"self":{"href":"/api/v1"},"mk:store":{"href":"/api/v1/store"},"mk:orders":{"href":"/api/v1/orders"},
      "mk:events":{"href":"/api/v1/events","type":"text/event-stream"},"mk:app":{"href":"/api/v1/app"},
      "curies":[{"name":"mk","href":"https://docs.monokulo.com/rels/{rel}","templated":true}]},
    "_templates":{"create-order":{"method":"POST","target":"/api/v1/orders","contentType":"application/json","properties":[]},
      "disconnect":{"method":"DELETE","target":"/api/v1/app","properties":[]}}}"#;

const ONION: &str = "http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/api/v1";

type Route = Arc<dyn Fn(&Request) -> ResponseTemplate + Send + Sync>;

/// What the stand-in answers, by method and path (anything else is a 404
/// problem), and every request it saw.
#[derive(Default)]
struct Routes {
    routes: Mutex<HashMap<String, Route>>,
    seen: Mutex<Vec<Request>>,
}

struct Dispatch(Arc<Routes>);

impl Respond for Dispatch {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.0.seen.lock().unwrap().push(request.clone());
        let route = self.0.routes.lock().unwrap().get(&line(request)).cloned();
        match route {
            Some(route) => route(request),
            None => problem(404, "Not found", "No such thing.", "x", ""),
        }
    }
}

fn line(request: &Request) -> String {
    format!("{} {}", request.method, request.url.path())
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request.headers.get(name).and_then(|v| v.to_str().ok())
}

fn hal(body: &str, code: u16) -> ResponseTemplate {
    ResponseTemplate::new(code).set_body_raw(body.to_owned(), "application/prs.hal-forms+json")
}

/// The entry as Monokulo answers it: kept per key for an hour, with its
/// version.
fn entry(body: &str, etag: &str) -> ResponseTemplate {
    hal(body, 200)
        .insert_header("Cache-Control", "private, max-age=3600")
        .insert_header("Vary", "Authorization")
        .insert_header("ETag", etag)
}

fn problem(status: u16, title: &str, detail: &str, kind: &str, extra: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_raw(
        format!(
            r#"{{"type":"https://docs.monokulo.com/problems/{kind}","title":"{title}","status":{status},"detail":"{detail}"{extra}}}"#
        ),
        "application/problem+json",
    )
}

fn json(body: &str, code: u16) -> ResponseTemplate {
    ResponseTemplate::new(code).set_body_raw(body.to_owned(), "application/json")
}

fn order(status: &str) -> String {
    format!(
        r##"{{"order_id":"order_1","status":"{status}","is_final":false,
        "address":"8Bx","xmr_amount_piconero":500000000000,"amount_received_piconero":0,"amount":"24.50",
        "currency":"USD","merchant_order_id":"#1042","show_merchant_order_id":true,"confirmations":0,
        "confirmations_required":10,"created_at":1,"expires_at":99,
        "_links":{{"self":{{"href":"/api/v1/orders/order_1"}},"mk:order-events":{{"href":"/api/v1/orders/order_1/events"}}}}}}"##
    )
}

struct Stand {
    server: MockServer,
    routes: Arc<Routes>,
    /// The clock the entry is kept by.
    clock: FixedClock,
    api: MonokuloApi,
}

fn identity() -> AppIdentity {
    AppIdentity {
        slug: "till".into(),
        name: "Till".into(),
        version: "0.1.0".into(),
        platform: "android".into(),
        device_name: "Pixel 8".into(),
    }
}

impl Stand {
    async fn start() -> Stand {
        let server = MockServer::start().await;
        let routes = Arc::new(Routes::default());
        Mock::given(any())
            .respond_with(Dispatch(routes.clone()))
            .mount(&server)
            .await;
        let clock = FixedClock::at(1_000);
        let api = MonokuloApi::new(reqwest::Client::new(), identity(), Arc::new(clock.clone()));
        let stand = Stand {
            server,
            routes,
            clock,
            api,
        };
        stand.route("GET /api/v1", |_| entry(ENTRY, "\"e1\""));
        stand
    }

    fn route(
        &self,
        line: &str,
        answer: impl Fn(&Request) -> ResponseTemplate + Send + Sync + 'static,
    ) {
        self.routes
            .routes
            .lock()
            .unwrap()
            .insert(line.to_owned(), Arc::new(answer));
    }

    fn seen(&self) -> Vec<Request> {
        self.routes.seen.lock().unwrap().clone()
    }

    fn lines(&self) -> Vec<String> {
        self.seen().iter().map(line).collect()
    }

    fn port(&self) -> u16 {
        self.server.address().port()
    }

    /// The server by name (`localhost`), as a till that connected to that
    /// name reaches it.
    fn base(&self) -> String {
        format!("http://localhost:{}", self.port())
    }

    fn connection(&self) -> Connection {
        Connection::new("ak_secret", format!("{}/api/v1", self.base()))
    }

    fn token() -> String {
        format!("ct_{}", "ab".repeat(32))
    }

    fn code(&self) -> ConnectCode {
        ConnectCode {
            connect_url: format!("{}/apps/connect", self.base()),
            token: Stand::token(),
        }
    }
}

fn address() -> String {
    format!("4{}Q2xyz789", "A".repeat(85))
}

#[tokio::test]
async fn connecting_redeems_the_code_at_the_link_for_a_key_and_the_apis_entry_at_the_address_it_used()
 {
    let stand = Stand::start().await;
    stand.route("POST /apps/connect", |_| {
        json(r#"{"app_key":"ak_x","api":"/api/v1"}"#, 200)
    });
    let connection = stand.api.connect(&stand.code()).await.unwrap();
    assert_eq!(
        connection,
        Connection::new("ak_x", format!("{}/api/v1", stand.base()))
    );
    assert_eq!(
        connection.onion_api, None,
        "a Monokulo with no onion address gives none"
    );
    let seen = stand.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(header(&seen[0], "Monokulo-Client"), Some("till/0.1.0"));
    let sent = String::from_utf8(seen[0].body.clone()).unwrap();
    assert!(
        sent.contains(&format!(r#""token":"{}""#, Stand::token()))
            && sent.contains(r#""device_name":"Pixel 8""#),
        "{sent}"
    );
}

#[tokio::test]
async fn a_monokulo_with_an_onion_address_gives_its_entry_there_too() {
    let stand = Stand::start().await;
    stand.route("POST /apps/connect", |_| {
        json(
            &format!(r#"{{"app_key":"ak_x","api":"/api/v1","onion_api":"{ONION}"}}"#),
            200,
        )
    });
    let connection = stand.api.connect(&stand.code()).await.unwrap();
    assert_eq!(connection.onion_api.as_deref(), Some(ONION));
    assert_eq!(
        connection.entry(),
        format!("{}/api/v1", stand.base()),
        "the till goes over Tor only when it chooses to"
    );
    assert_eq!(
        Connection {
            tor: true,
            ..connection
        }
        .entry(),
        ONION
    );
}

#[tokio::test]
async fn a_monokulo_under_a_path_is_connected_and_used_under_it() {
    let stand = Stand::start().await;
    stand.route("POST /monokulo/apps/connect", |_| {
        json(r#"{"app_key":"ak_x","api":"/monokulo/api/v1"}"#, 200)
    });
    stand.route("GET /monokulo/api/v1", |_| {
        entry(&ENTRY.replace("\"/api/v1", "\"/monokulo/api/v1"), "\"e1\"")
    });
    stand.route("GET /monokulo/api/v1/store", |_| {
        hal(r#"{"name":"Bakery","base_currency":"EUR"}"#, 200)
    });
    stand.route("POST /monokulo/api/v1/orders", |_| {
        hal(
            &order("pending").replace("\"/api/v1", "\"/monokulo/api/v1"),
            201,
        )
    });
    let code = ConnectCode::parse(
        &format!(
            "{}/monokulo/apps/connect#token={}",
            stand.base(),
            Stand::token()
        ),
        true,
    )
    .unwrap();
    let connection = stand.api.connect(&code).await.unwrap();
    assert_eq!(connection.api, format!("{}/monokulo/api/v1", stand.base()));
    stand.api.store(&connection).await.unwrap();
    let order = stand
        .api
        .create_order(
            &connection,
            &Money::new(2450, CurrencyCode::USD),
            None,
            "sq:abc",
        )
        .await
        .unwrap();
    assert_eq!(
        order.url,
        format!("{}/monokulo/api/v1/orders/order_1", stand.base())
    );
    assert_eq!(
        stand.lines(),
        [
            "POST /monokulo/apps/connect",
            "GET /monokulo/api/v1",
            "GET /monokulo/api/v1/store",
            "POST /monokulo/api/v1/orders"
        ]
    );
}

#[tokio::test]
async fn a_refused_code_says_why_by_its_problem() {
    let stand = Stand::start().await;
    stand.route("POST /apps/connect", |_| {
        problem(
            503,
            "No public address",
            "This Monokulo has no public address yet. Ask its admin to set one.",
            "no-public-address",
            "",
        )
    });
    let e = stand.api.connect(&stand.code()).await.unwrap_err();
    assert_eq!(e.status(), 503);
    assert_eq!(e.problem(), Some("no-public-address"));
    assert_eq!(
        e.to_string(),
        "This Monokulo has no public address yet. Ask its admin to set one."
    );
}

#[tokio::test]
async fn a_busy_monokulo_says_how_long_to_wait() {
    let stand = Stand::start().await;
    stand.route("POST /apps/connect", |_| {
        problem(
            429,
            "Too many requests",
            "Slow down.",
            "rate-limited",
            r#","retry_after":30"#,
        )
        .insert_header("Retry-After", "30")
    });
    let e = stand.api.connect(&stand.code()).await.unwrap_err();
    assert_eq!(e.problem(), Some("rate-limited"));
    assert_eq!(e.retry_after(), Some(30));
}

#[tokio::test]
async fn a_challenge_is_solved_and_the_request_sent_again_with_its_proof() {
    let stand = Stand::start().await;
    let challenge = "c0ffee.abc";
    stand.route("POST /apps/connect", move |request| {
        match header(request, "Monokulo-Proof") {
            None => problem(
                429,
                "Prove it",
                "Solve the challenge.",
                "challenge-required",
                &format!(r#","challenge":{{"challenge":"{challenge}","difficulty":8}}"#),
            ),
            Some(proof) => {
                let nonce = proof.strip_prefix(&format!("{challenge}.")).unwrap_or("");
                let hash = Sha256::digest(format!("{challenge}{nonce}"));
                if proof::zero_bits(&hash) >= 8 {
                    json(r#"{"app_key":"ak_x","api":"/api/v1"}"#, 200)
                } else {
                    problem(400, "Bad proof", proof, "invalid-request", "")
                }
            }
        }
    });
    assert_eq!(
        stand.api.connect(&stand.code()).await.unwrap().app_key,
        "ak_x"
    );
    let seen = stand.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].body, seen[1].body, "the same request again");
}

#[tokio::test]
async fn the_store_and_its_wallet_are_read_live_found_from_the_entry() {
    let stand = Stand::start().await;
    stand.route("GET /api/v1/store", |_| {
        hal(
            &format!(
                r#"{{"name":"Bakery","base_currency":"EUR","wallet":{{"name":"Shop","network":"stagenet","address":"{}"}},"_links":{{}}}}"#,
                address()
            ),
            200,
        )
    });
    let store = stand.api.store(&stand.connection()).await.unwrap();
    assert_eq!(store.name, "Bakery");
    assert_eq!(store.base_currency, "EUR");
    let wallet = store.wallet.unwrap();
    assert_eq!(wallet.address, address());
    assert_eq!(wallet.address_short(), "4AAAAAAA…Q2xyz789");
    assert_eq!(stand.lines(), ["GET /api/v1", "GET /api/v1/store"]);
    let host = format!("localhost:{}", stand.port());
    for request in stand.seen() {
        assert_eq!(header(&request, "Authorization"), Some("Bearer ak_secret"));
        assert_eq!(
            header(&request, "Host"),
            Some(host.as_str()),
            "links resolve against the address the till used"
        );
    }
}

#[tokio::test]
async fn a_store_without_a_wallet_says_so_by_leaving_it_out() {
    let stand = Stand::start().await;
    stand.route("GET /api/v1/store", |_| {
        hal(r#"{"name":"Bakery","base_currency":"EUR"}"#, 200)
    });
    assert_eq!(
        stand.api.store(&stand.connection()).await.unwrap().wallet,
        None
    );
}

#[tokio::test]
async fn the_entry_is_kept_for_its_max_age_then_revalidated_with_its_etag() {
    let stand = Stand::start().await;
    stand.route("GET /api/v1/store", |_| {
        hal(r#"{"name":"Bakery","base_currency":"EUR"}"#, 200)
    });
    stand.route("GET /api/v1/app", |_| {
        hal(
            r#"{"app_id":"app_1","device_name":"Front counter","platform":"android","connected_at":5}"#,
            200,
        )
    });
    stand.api.store(&stand.connection()).await.unwrap();
    assert_eq!(
        stand
            .api
            .app(&stand.connection())
            .await
            .unwrap()
            .device_name,
        "Front counter"
    );
    let entries = |stand: &Stand| -> Vec<Request> {
        stand
            .seen()
            .into_iter()
            .filter(|r| r.url.path() == "/api/v1")
            .collect()
    };
    assert_eq!(entries(&stand).len(), 1, "kept: one read for both");
    stand.clock.advance(3_600);
    stand.route("GET /api/v1", |request| {
        if header(request, "If-None-Match") == Some("\"e1\"") {
            ResponseTemplate::new(304)
                .insert_header("Cache-Control", "private, max-age=3600")
                .insert_header("ETag", "\"e1\"")
        } else {
            entry(ENTRY, "\"e1\"")
        }
    });
    stand.api.store(&stand.connection()).await.unwrap();
    stand.api.store(&stand.connection()).await.unwrap();
    let entries = entries(&stand);
    assert_eq!(
        entries.len(),
        2,
        "asked again once it expired, then kept another hour"
    );
    assert_eq!(
        header(entries.last().unwrap(), "If-None-Match"),
        Some("\"e1\"")
    );
}

#[tokio::test]
async fn each_key_keeps_its_own_entry() {
    let stand = Stand::start().await;
    stand.route("GET /api/v1/store", |_| {
        hal(r#"{"name":"Bakery","base_currency":"EUR"}"#, 200)
    });
    stand.api.store(&stand.connection()).await.unwrap();
    let other = Connection {
        app_key: "ak_other".into(),
        ..stand.connection()
    };
    stand.api.store(&other).await.unwrap();
    let keys: Vec<_> = stand
        .seen()
        .iter()
        .filter(|r| r.url.path() == "/api/v1")
        .map(|r| header(r, "Authorization").unwrap_or("").to_owned())
        .collect();
    assert_eq!(keys, ["Bearer ak_secret", "Bearer ak_other"]);
}

#[tokio::test]
async fn a_link_that_answers_404_has_the_entry_read_again_and_followed_anew() {
    let stand = Stand::start().await;
    stand.route("GET /api/v1/store", |_| {
        problem(404, "Not found", "No such thing.", "not-found", "")
    });
    stand.route("GET /api/v1/shop", |_| {
        hal(r#"{"name":"Bakery","base_currency":"EUR"}"#, 200)
    });
    stand.api.onion_api(&stand.connection()).await.unwrap();
    stand.route("GET /api/v1", |_| {
        entry(&ENTRY.replace("/api/v1/store", "/api/v1/shop"), "\"e2\"")
    });
    assert_eq!(
        stand.api.store(&stand.connection()).await.unwrap().name,
        "Bakery"
    );
    assert_eq!(
        stand.lines(),
        [
            "GET /api/v1",
            "GET /api/v1/store",
            "GET /api/v1",
            "GET /api/v1/shop"
        ]
    );
}

#[tokio::test]
async fn orders_are_made_with_the_create_order_form_with_no_description() {
    let stand = Stand::start().await;
    stand.route("POST /api/v1/orders", |_| {
        hal(&order("pending"), 201).insert_header("Location", "/api/v1/orders/order_1")
    });
    let order = stand
        .api
        .create_order(
            &stand.connection(),
            &Money::new(2450, CurrencyCode::USD),
            Some("#1042"),
            "sq:abc",
        )
        .await
        .unwrap();
    assert_eq!(order.payment_uri(), "monero:8Bx?tx_amount=0.5");
    assert_eq!(order.url, format!("{}/api/v1/orders/order_1", stand.base()));
    assert_eq!(order.phase(), SalePhase::Waiting);
    let seen = stand.seen();
    let request = seen.last().unwrap();
    assert_eq!(request.method.as_str(), "POST");
    let sent = String::from_utf8(request.body.clone()).unwrap();
    assert!(
        sent.contains(r#""idempotency_key":"sq:abc""#) && sent.contains(r#""currency":"USD""#),
        "{sent}"
    );
    assert!(!sent.contains("description"), "{sent}");
}

#[tokio::test]
async fn an_order_is_read_again_at_its_link() {
    let stand = Stand::start().await;
    stand.route("GET /api/v1/orders/order_1", |_| {
        hal(&order("confirming"), 200)
    });
    let order = stand
        .api
        .order(
            &stand.connection(),
            &format!("{}/api/v1/orders/order_1", stand.base()),
        )
        .await
        .unwrap();
    assert_eq!(order.phase(), SalePhase::Confirming);
    assert_eq!(
        stand.lines(),
        ["GET /api/v1/orders/order_1"],
        "no entry needed"
    );
}

/// An order Monokulo can renew carries its `renew-order` form: the client
/// keeps where it goes, resolved, and sends it with the expiry of the code
/// being replaced, reading the renewed order from the answer. An order
/// without the form (an older Monokulo) has none.
#[tokio::test]
async fn an_order_is_renewed_with_its_own_renew_order_form() {
    let stand = Stand::start().await;
    let with_form = |status: &str, expires_at: i64| {
        let mut body: serde_json::Value = serde_json::from_str(&order(status)).unwrap();
        body["expires_at"] = expires_at.into();
        body["_templates"] = serde_json::json!({
            "renew-order": {"method": "POST", "target": "/api/v1/orders/order_1",
                            "contentType": "application/json", "properties": []}
        });
        hal(&body.to_string(), 200)
    };
    stand.route("GET /api/v1/orders/order_1", move |_| {
        with_form("expired", 99)
    });
    stand.route("POST /api/v1/orders/order_1", move |_| {
        with_form("pending", 1_299)
    });
    let link = format!("{}/api/v1/orders/order_1", stand.base());
    let expired = stand.api.order(&stand.connection(), &link).await.unwrap();
    assert_eq!(expired.renew.as_deref(), Some(link.as_str()));
    let renewed = stand
        .api
        .renew_order(&stand.connection(), expired.renew.as_deref().unwrap(), 99)
        .await
        .unwrap();
    assert_eq!(
        (
            renewed.order_id.as_str(),
            renewed.expires_at,
            renewed.phase()
        ),
        ("order_1", 1_299, SalePhase::Waiting)
    );
    let seen = stand.seen();
    let request = seen.last().unwrap();
    assert_eq!(line(request), "POST /api/v1/orders/order_1");
    assert_eq!(header(request, "authorization"), Some("Bearer ak_secret"));
    assert_eq!(
        String::from_utf8(request.body.clone()).unwrap(),
        r#"{"expires_at":99}"#
    );

    stand.route("GET /api/v1/orders/order_2", |_| {
        hal(&order("expired"), 200)
    });
    let older = stand
        .api
        .order(
            &stand.connection(),
            &format!("{}/api/v1/orders/order_2", stand.base()),
        )
        .await
        .unwrap();
    assert_eq!(older.renew, None, "an older Monokulo offers no renewal");
}

#[tokio::test]
async fn an_order_monokulo_has_no_price_reference_or_end_for_leaves_them_out_and_reads_as_none() {
    let stand = Stand::start().await;
    stand.route("GET /api/v1/orders/order_2", |_| {
        hal(
            r#"{"order_id":"order_2","status":"pending","is_final":false,"address":"8Bx",
                "xmr_amount_piconero":500000000000,"amount_received_piconero":0,"show_merchant_order_id":false,
                "confirmations":0,"confirmations_required":10,"created_at":1,"expires_at":99,
                "_links":{"self":{"href":"/api/v1/orders/order_2"}}}"#,
            200,
        )
    });
    let order = stand
        .api
        .order(
            &stand.connection(),
            &format!("{}/api/v1/orders/order_2", stand.base()),
        )
        .await
        .unwrap();
    assert_eq!(order.order_id, "order_2");
    assert_eq!(order.amount, None);
    assert_eq!(order.currency, None);
    assert_eq!(order.merchant_order_id, None);
    assert_eq!(order.phase(), SalePhase::Waiting);
    assert_eq!(order.payment_uri(), "monero:8Bx?tx_amount=0.5");
}

#[tokio::test]
async fn disconnecting_uses_the_disconnect_form() {
    let stand = Stand::start().await;
    stand.route("DELETE /api/v1/app", |_| ResponseTemplate::new(204));
    stand.api.disconnect(&stand.connection()).await.unwrap();
    assert_eq!(stand.lines().last().unwrap(), "DELETE /api/v1/app");
}

#[tokio::test]
async fn an_unknown_key_hears_401_at_the_entry_with_the_problems_detail() {
    let stand = Stand::start().await;
    stand.route("GET /api/v1", |_| {
        problem(
            401,
            "No valid key",
            "This app was disconnected.",
            "unauthorized",
            "",
        )
    });
    let e = stand.api.store(&stand.connection()).await.unwrap_err();
    assert_eq!(e.status(), 401);
    assert_eq!(e.to_string(), "This app was disconnected.");
}

#[tokio::test]
async fn a_form_the_key_may_not_use_isnt_tried() {
    let stand = Stand::start().await;
    stand.route("GET /api/v1", |_| {
        hal(r#"{"_links":{"self":{"href":"/api/v1"}}}"#, 200)
    });
    let e = stand
        .api
        .create_order(
            &stand.connection(),
            &Money::new(100, CurrencyCode::USD),
            None,
            "k",
        )
        .await
        .unwrap_err();
    assert_eq!(e.status(), 403);
    assert_eq!(stand.seen().len(), 1, "only the entry was read");
}

#[tokio::test]
async fn the_event_stream_gives_ready_then_sales_skipping_what_it_doesnt_know() {
    let stand = Stand::start().await;
    let sale = r#"{"event":"sale.waiting","order_id":"order_1","amount":"1","currency":"XMR","address":"8Bx",
        "xmr_amount_piconero":1000000000000,"expires_at":9,"device_name":"Till","at":1,
        "_links":{"mk:order":{"href":"/api/v1/orders/order_1"}}}"#
        .replace('\n', "");
    stand.route("GET /api/v1/events", move |_| {
        ResponseTemplate::new(200).set_body_raw(
            format!(
                "event: ready\ndata: {{\"at\":1}}\n\n\
                 : keep-alive\n\n\
                 event: something.new\ndata: {{}}\n\n\
                 event: sale.waiting\ndata: {sale}\n\n\
                 event: lagged\ndata: {{\"missed\":3}}\n\n"
            ),
            "text/event-stream",
        )
    });
    let events: Vec<StreamEvent> = stand
        .api
        .events(&stand.connection())
        .take(3)
        .map(Result::unwrap)
        .collect()
        .await;
    assert_eq!(events[0], StreamEvent::Ready);
    let StreamEvent::Sale(sale) = &events[1] else {
        panic!("a sale, not {:?}", events[1])
    };
    assert_eq!(sale.order_id, "order_1");
    assert_eq!(
        sale.order_url.as_deref(),
        Some(format!("{}/api/v1/orders/order_1", stand.base()).as_str()),
        "its mk:order, resolved"
    );
    assert_eq!(events[2], StreamEvent::Lagged);
    let seen = stand.seen();
    let stream = seen.last().unwrap();
    assert_eq!(stream.url.path(), "/api/v1/events");
    assert_eq!(header(stream, "Authorization"), Some("Bearer ak_secret"));
}

#[tokio::test]
async fn a_stream_refused_with_401_says_so() {
    let stand = Stand::start().await;
    stand.route("GET /api/v1/events", |_| {
        problem(401, "No valid key", "This app was disconnected.", "x", "")
    });
    let items: Vec<_> = stand.api.events(&stand.connection()).collect().await;
    assert_eq!(items.len(), 1, "it ends with its error");
    assert_eq!(items[0].as_ref().unwrap_err().status(), 401);
}

#[tokio::test]
async fn over_tor_the_till_reads_the_onion_entry_and_follows_it_there_never_the_web_address() {
    let stand = Stand::start().await;
    let onion = Stand::start().await;
    onion.route("GET /api/v1/store", |_| {
        hal(r#"{"name":"Bakery","base_currency":"EUR"}"#, 200)
    });
    stand.route("GET /api/v1/store", |_| {
        hal(r#"{"name":"Bakery","base_currency":"EUR"}"#, 200)
    });
    let connection = Connection {
        onion_api: Some(format!("{}/api/v1", onion.server.uri())),
        tor: true,
        ..stand.connection()
    };
    assert_eq!(stand.api.store(&connection).await.unwrap().name, "Bakery");
    assert_eq!(onion.lines(), ["GET /api/v1", "GET /api/v1/store"]);
    assert!(stand.seen().is_empty(), "nothing went to the web address");
    stand
        .api
        .store(&Connection {
            tor: false,
            ..connection
        })
        .await
        .unwrap();
    assert_eq!(
        stand.lines(),
        ["GET /api/v1", "GET /api/v1/store"],
        "off Tor, the web address"
    );
}

#[tokio::test]
async fn the_entry_says_where_the_onion_entry_is_while_there_is_one() {
    let stand = Stand::start().await;
    assert_eq!(
        stand.api.onion_api(&stand.connection()).await.unwrap(),
        None
    );
    stand.route("GET /api/v1", |_| {
        entry(
            &ENTRY.replace(
                r#""self":{"href":"/api/v1"}"#,
                &format!(r#""self":{{"href":"/api/v1"}},"mk:onion-api":{{"href":"{ONION}"}}"#),
            ),
            "\"e1\"",
        )
    });
    let other = Connection {
        app_key: "ak_2".into(),
        ..stand.connection()
    };
    assert_eq!(
        stand.api.onion_api(&other).await.unwrap().as_deref(),
        Some(ONION)
    );
}

// Not in the Kotlin, where OkHttp's read timeout did this.

/// A stream that goes quiet, keep-alives and all, ends as dead, so the
/// caller reconnects.
#[tokio::test]
async fn a_silent_stream_ends_as_dead() {
    let stand = Stand::start().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    stand.route("GET /api/v1", move |_| {
        entry(
            &ENTRY.replace(
                r#""/api/v1/events""#,
                &format!(r#""http://127.0.0.1:{port}/api/v1/events""#),
            ),
            "\"e1\"",
        )
    });
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 1024];
        let _ = socket.read(&mut request).await.unwrap();
        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n";
        let ready = "event: ready\ndata: {}\n\n";
        socket
            .write_all(format!("{head}{:x}\r\n{ready}\r\n", ready.len()).as_bytes())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    let api = stand
        .api
        .clone()
        .with_stream_silence(Duration::from_millis(300));
    let items: Vec<_> = api.events(&stand.connection()).collect().await;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].as_ref().unwrap(), &StreamEvent::Ready);
    assert!(
        matches!(items[1], Err(MonokuloError::StreamSilent(_))),
        "{:?}",
        items[1]
    );
}

/// The engine runs the client on its own task, so what it returns must be
/// `Send`.
#[tokio::test]
async fn the_clients_futures_and_stream_can_move_between_threads() {
    let stand = Stand::start().await;
    stand.route("GET /api/v1/store", |_| {
        hal(r#"{"name":"Bakery","base_currency":"EUR"}"#, 200)
    });
    let api = stand.api.clone();
    let connection = stand.connection();
    let store = tokio::spawn(async move { api.store(&connection).await.map(|s| s.name) });
    assert_eq!(store.await.unwrap().unwrap(), "Bakery");
    let events = stand.api.events(&stand.connection());
    tokio::spawn(async move { events.collect::<Vec<_>>().await })
        .await
        .unwrap();
}
