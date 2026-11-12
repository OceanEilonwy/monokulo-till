//! Ported from the Android app's `MonokuloContractTest`: the client against
//! Monokulo's own OpenAPI document, generated from the pinned Monokulo commit
//! (`monokulo.sha` in `android/gradle.properties`) by
//! `tools/monokulo-openapi/generate.sh`, never committed. A stand-in answers
//! each request with what Monokulo recorded for that route from a real
//! exchange (the entry and each resource as an app's key sees them, the app
//! handshake and its problems, the event stream), so a renamed route, link,
//! form, field or event fails here.
//!
//! `MONOKULO_OPENAPI` names the document. Without it the tests say so and
//! pass without checking, so `cargo test` works offline; CI sets it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use monokulo_client::{
    AppIdentity, ConnectCode, Connection, MonokuloApi, MonokuloError, StreamEvent, SystemClock,
    problems, tor,
};
use reqwest::Url;
use serde_json::Value;
use till_types::{CurrencyCode, Money};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate, matchers::any};

const ETAG: &str = "\"entry-1\"";

/// The document, or `None` (said once per test) when `MONOKULO_OPENAPI`
/// isn't set.
fn openapi() -> Option<Value> {
    let Ok(path) = std::env::var("MONOKULO_OPENAPI") else {
        eprintln!(
            "MONOKULO_OPENAPI isn't set, so the client isn't checked against Monokulo's OpenAPI \
             document. Generate it with tools/monokulo-openapi/generate.sh at android/gradle.properties' \
             monokulo.sha and set MONOKULO_OPENAPI to its path."
        );
        return None;
    };
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("MONOKULO_OPENAPI is {path}, which can't be read: {e}"));
    Some(serde_json::from_str(&text).expect("Monokulo's OpenAPI document is JSON"))
}

/// The stand-in's state, shared with its responder.
struct Shared {
    openapi: Value,
    seen: Mutex<Vec<String>>,
    /// Which recorded problem each `METHOD path` answers with, by name; any
    /// other answers its success.
    problems: Mutex<HashMap<String, String>>,
    /// Whether the Monokulo answering has an onion address: picks the
    /// `-with-onion` examples.
    onion: AtomicBool,
    /// What went wrong answering, raised after the test so it isn't lost as
    /// a 500.
    failure: Mutex<Option<String>>,
}

struct Contract {
    server: MockServer,
    shared: Arc<Shared>,
    api: MonokuloApi,
}

impl Drop for Contract {
    fn drop(&mut self) {
        if let Some(failure) = self.shared.failure.lock().unwrap().take()
            && !std::thread::panicking()
        {
            panic!("the stand-in failed answering: {failure}");
        }
    }
}

fn new_api() -> MonokuloApi {
    MonokuloApi::new(
        reqwest::Client::new(),
        AppIdentity {
            slug: "till".into(),
            name: "Till".into(),
            version: "0.1.0".into(),
            platform: "android".into(),
            device_name: "Pixel 8".into(),
        },
        Arc::new(SystemClock),
    )
}

struct Dispatch(Arc<Shared>);

impl Respond for Dispatch {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.0.answer(request).unwrap_or_else(|e| {
            self.0.failure.lock().unwrap().get_or_insert(e);
            ResponseTemplate::new(500)
        })
    }
}

impl Contract {
    async fn start() -> Option<Contract> {
        let openapi = openapi()?;
        let server = MockServer::start().await;
        let shared = Arc::new(Shared {
            openapi,
            seen: Mutex::default(),
            problems: Mutex::default(),
            onion: AtomicBool::new(false),
            failure: Mutex::default(),
        });
        Mock::given(any())
            .respond_with(Dispatch(shared.clone()))
            .mount(&server)
            .await;
        Some(Contract {
            server,
            shared,
            api: new_api(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.server.uri())
    }

    fn connection(&self) -> Connection {
        Connection::new("ak_secret", self.url("/api/v1"))
    }

    fn code(&self) -> ConnectCode {
        ConnectCode {
            connect_url: self.url("/apps/connect"),
            token: format!("ct_{}1", "0".repeat(63)).into(),
        }
    }

    fn seen(&self) -> Vec<String> {
        self.shared.seen.lock().unwrap().clone()
    }

    fn answer_with(&self, line: &str, problem: &str) {
        let mut problems = self.shared.problems.lock().unwrap();
        problems.clear();
        problems.insert(line.to_owned(), problem.to_owned());
    }

    fn set_onion(&self) {
        self.shared.onion.store(true, Ordering::SeqCst);
    }
}

impl Shared {
    fn paths(&self) -> &serde_json::Map<String, Value> {
        self.openapi["paths"]
            .as_object()
            .expect("the document has paths")
    }

    fn schemas(&self) -> &serde_json::Map<String, Value> {
        self.openapi["components"]["schemas"]
            .as_object()
            .expect("the document has schemas")
    }

    /// The OpenAPI path template `path` matches, or `None`.
    fn template(&self, path: &str) -> Option<&str> {
        self.paths()
            .keys()
            .find(|template| matches_template(template, path))
            .map(String::as_str)
    }

    fn operation(&self, method: &str, path: &str) -> Option<&Value> {
        let template = self.template(path)?;
        self.paths()[template]
            .get(method.to_ascii_lowercase())
            .filter(|op| op.is_object())
    }

    /// A response's content type and its recorded example, as `key` (`ak`,
    /// or `no-key` for the handshake) sees it: `ak` from a Monokulo without
    /// an onion address, `ak-with-onion` from one with (a route whose answer
    /// doesn't change with it has only the first).
    fn recorded(
        &self,
        response: &Value,
        key: &str,
    ) -> Result<Option<(String, Option<Value>)>, String> {
        let Some((kind, media)) = response
            .get("content")
            .and_then(Value::as_object)
            .and_then(|c| c.iter().next())
        else {
            return Ok(None);
        };
        let example = match media.get("example") {
            Some(example) => Some(example.clone()),
            None => match media.get("examples").and_then(Value::as_object) {
                Some(examples) => {
                    let onion = self.onion.load(Ordering::SeqCst);
                    let named = onion
                        .then(|| examples.get(&format!("{key}-with-onion")))
                        .flatten()
                        .or_else(|| examples.get(key))
                        .ok_or_else(|| {
                            format!(
                                "Monokulo recorded no {key} example; it has {:?}",
                                examples.keys().collect::<Vec<_>>()
                            )
                        })?;
                    named.get("value").cloned()
                }
                None => None,
            },
        };
        Ok(Some((kind.clone(), example)))
    }

    /// The problem `name` as Monokulo recorded it for `op`: its status, the
    /// headers that response lists (`Retry-After` as the example's
    /// `retry_after`) and the example body.
    fn recorded_problem(&self, op: &Value, name: &str) -> Result<ResponseTemplate, String> {
        for (code, response) in responses(op) {
            let Some(body) = problem_examples(response)
                .and_then(|examples| examples.get(name))
                .and_then(|example| example.get("value"))
            else {
                continue;
            };
            let mut answer = ResponseTemplate::new(code.parse::<u16>().map_err(|e| e.to_string())?);
            let lists_retry = response
                .get("headers")
                .and_then(Value::as_object)
                .is_some_and(|h| h.contains_key("Retry-After"));
            if lists_retry && let Some(retry) = body.get("retry_after") {
                answer = answer.insert_header("Retry-After", plain(retry));
            }
            return Ok(answer.set_body_raw(body.to_string(), "application/problem+json"));
        }
        Err(format!(
            "Monokulo recorded no {name} problem for this route"
        ))
    }

    fn answer(&self, request: &Request) -> Result<ResponseTemplate, String> {
        let path = request.url.path().to_owned();
        let line = format!("{} {path}", request.method);
        self.seen.lock().unwrap().push(line.clone());
        let Some(op) = self.operation(request.method.as_str(), &path) else {
            return Ok(ResponseTemplate::new(404));
        };
        if request.method.as_str() == "POST" {
            self.check_sent(op, &request.body)?;
        }
        if let Some(name) = self.problems.lock().unwrap().get(&line) {
            return self.recorded_problem(op, name);
        }
        let (code, response) = responses(op)
            .find(|(code, _)| code.starts_with('2'))
            .ok_or("Monokulo records no success here")?;
        let code: u16 = code.parse().map_err(|_| "a status that isn't a number")?;
        let headers = response.get("headers").and_then(Value::as_object);
        let lists = |name: &str| headers.is_some_and(|h| h.contains_key(name));
        let if_none_match = request
            .headers
            .get("If-None-Match")
            .and_then(|v| v.to_str().ok());
        if lists("ETag") && if_none_match == Some(ETAG) {
            if op["responses"].get("304").is_none() {
                return Err("Monokulo doesn't answer 304 here".into());
            }
            return Ok(ResponseTemplate::new(304)
                .insert_header("ETag", ETAG)
                .insert_header("Cache-Control", "private, max-age=3600"));
        }
        let key = if path == "/apps/connect" {
            "no-key"
        } else {
            "ak"
        };
        let Some((kind, example)) = self.recorded(response, key)? else {
            return Ok(ResponseTemplate::new(code));
        };
        let mut answer = ResponseTemplate::new(code);
        if lists("ETag") {
            answer = answer.insert_header("ETag", ETAG);
        }
        if lists("Cache-Control") {
            answer = answer.insert_header("Cache-Control", "private, max-age=3600");
        }
        Ok(match example {
            None => answer.insert_header("Content-Type", kind.as_str()),
            Some(Value::String(text)) => answer.set_body_raw(text, &kind),
            Some(other) => answer.set_body_raw(other.to_string(), &kind),
        })
    }

    /// What the app sends is what the operation takes: every required field,
    /// nothing it doesn't know.
    fn check_sent(&self, op: &Value, sent: &[u8]) -> Result<(), String> {
        let reference = op
            .pointer("/requestBody/content/application~1json/schema/$ref")
            .and_then(Value::as_str)
            .ok_or("the operation takes no JSON")?;
        let schema = &self.schemas()[reference.rsplit('/').next().unwrap_or(reference)];
        let known: Vec<&String> = schema["properties"]
            .as_object()
            .ok_or("the schema has no properties")?
            .keys()
            .collect();
        let required: Vec<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let fields: Vec<String> = serde_json::from_slice::<Value>(sent)
            .map_err(|e| format!("sent something that isn't JSON: {e}"))?
            .as_object()
            .ok_or("sent something that isn't a JSON object")?
            .keys()
            .cloned()
            .collect();
        if !fields.iter().all(|f| known.contains(&f)) {
            return Err(format!("sent {fields:?}; Monokulo takes {known:?}"));
        }
        if !required.iter().all(|r| fields.iter().any(|f| f == r)) {
            return Err(format!("sent {fields:?}; Monokulo needs {required:?}"));
        }
        Ok(())
    }
}

fn matches_template(template: &str, path: &str) -> bool {
    let template: Vec<&str> = template.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    template.len() == path.len()
        && template.iter().zip(&path).all(|(t, p)| {
            if t.starts_with('{') && t.ends_with('}') {
                !p.is_empty()
            } else {
                t == p
            }
        })
}

fn responses(op: &Value) -> impl Iterator<Item = (&String, &Value)> {
    op["responses"].as_object().into_iter().flatten()
}

fn problem_examples(response: &Value) -> Option<&serde_json::Map<String, Value>> {
    response
        .pointer("/content/application~1problem+json/examples")
        .and_then(Value::as_object)
}

/// A JSON scalar as plain text: a string without its quotes.
fn plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn path_of(url: &str) -> String {
    Url::parse(url).expect("an absolute URL").path().to_owned()
}

#[tokio::test]
async fn connecting_sends_what_the_handshake_takes_and_keeps_the_key_and_the_entry() {
    let Some(stand) = Contract::start().await else {
        return;
    };
    let connection = stand.api.connect(&stand.code()).await.unwrap();
    assert!(connection.app_key.expose_str().starts_with("ak_"));
    assert_eq!(connection.api.as_str(), stand.url("/api/v1"));
    assert_eq!(
        connection.onion_api, None,
        "a Monokulo without an onion address names none"
    );
    assert_eq!(stand.seen(), ["POST /apps/connect"]);
}

#[tokio::test]
async fn connecting_to_a_monokulo_with_an_onion_address_keeps_its_onion_entry_too() {
    let Some(stand) = Contract::start().await else {
        return;
    };
    stand.set_onion();
    let connection = stand.api.connect(&stand.code()).await.unwrap();
    assert!(connection.app_key.expose_str().starts_with("ak_"));
    assert_eq!(connection.api.as_str(), stand.url("/api/v1"));
    let onion = connection
        .onion_api
        .expect("Monokulo recorded the handshake with an onion address");
    assert!(tor::is_onion(&onion), "{onion}");
    assert_eq!(stand.seen(), ["POST /apps/connect"]);
}

#[tokio::test]
async fn the_handshakes_problems_come_back_by_name_with_their_detail() {
    let Some(stand) = Contract::start().await else {
        return;
    };
    for (name, status) in [
        ("invalid-connect-code", 401),
        ("no-public-address", 503),
        ("invalid-request", 400),
    ] {
        stand.answer_with("POST /apps/connect", name);
        let e = stand.api.connect(&stand.code()).await.unwrap_err();
        assert_eq!(e.status(), status);
        assert_eq!(e.problem(), Some(name));
        assert!(!e.to_string().trim().is_empty());
    }
}

#[tokio::test]
async fn the_store_its_wallet_and_the_app_read_as_monokulo_writes_them_from_one_entry() {
    let Some(stand) = Contract::start().await else {
        return;
    };
    let store = stand.api.store(&stand.connection()).await.unwrap();
    assert!(!store.name.is_empty() && !store.base_currency.is_empty());
    let app = stand.api.app(&stand.connection()).await.unwrap();
    assert!(!app.device_name.is_empty() && !app.app_id.is_empty());
    assert_eq!(
        stand.seen(),
        ["GET /api/v1", "GET /api/v1/store", "GET /api/v1/app"]
    );
}

#[tokio::test]
async fn the_entry_an_apps_key_reads_links_the_onion_entry_only_when_there_is_one() {
    let Some(stand) = Contract::start().await else {
        return;
    };
    assert_eq!(
        stand.api.onion_api(&stand.connection()).await.unwrap(),
        None
    );
    stand.set_onion();
    let linked = new_api()
        .onion_api(&stand.connection())
        .await
        .unwrap()
        .expect("the entry links the onion entry");
    assert!(tor::is_onion(&linked), "{linked}");
}

#[tokio::test]
async fn an_order_is_made_with_the_create_order_form_read_as_monokulo_writes_it_and_read_again_at_its_self_link()
 {
    let Some(stand) = Contract::start().await else {
        return;
    };
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
    assert!(!order.address.is_empty() && order.piconero > 0 && order.expires_at > 0);
    assert!(
        order
            .payment_uri()
            .starts_with(&format!("monero:{}?tx_amount=", order.address))
    );
    assert!(
        stand.shared.template(&path_of(&order.url)).is_some(),
        "its self link is a route"
    );
    assert_eq!(
        stand
            .api
            .order(&stand.connection(), &order.url)
            .await
            .unwrap()
            .order_id,
        order.order_id,
        "the order made is the one read"
    );
}

#[tokio::test]
async fn an_order_says_how_many_confirmations_its_payment_has_of_the_stores_threshold() {
    let Some(stand) = Contract::start().await else {
        return;
    };
    let made = stand
        .api
        .create_order(
            &stand.connection(),
            &Money::new(2450, CurrencyCode::USD),
            Some("#1042"),
            "sq:abc",
        )
        .await
        .unwrap();
    let read = stand
        .api
        .order(&stand.connection(), &made.url)
        .await
        .unwrap();
    for order in [made, read] {
        assert_eq!(order.confirmations, 0, "nothing paid yet");
        assert_eq!(order.received, 0, "nothing received yet");
        assert!(
            order.confirmations_required > 0,
            "the threshold the ring and \"4 of 10\" count to: {}",
            order.confirmations_required
        );
    }
}

/// An order carries its `renew-order` form, which renews it (New code,
/// core.md decisions 167 to 171): what the client sends is what Monokulo
/// takes, the answer is the same order, and a refusal is heard by name.
#[tokio::test]
async fn an_order_is_renewed_with_its_renew_order_form_as_monokulo_records_it() {
    let Some(stand) = Contract::start().await else {
        return;
    };
    let made = stand
        .api
        .create_order(
            &stand.connection(),
            &Money::new(2450, CurrencyCode::USD),
            Some("#1042"),
            "sq:abc",
        )
        .await
        .unwrap();
    let renew = made
        .renew
        .clone()
        .expect("Monokulo offers the app's key the order's renew-order form");
    assert!(
        stand.shared.template(&path_of(&renew)).is_some(),
        "its target is a route: {renew}"
    );
    let renewed = stand
        .api
        .renew_order(&stand.connection(), &renew, made.expires_at)
        .await
        .unwrap();
    assert_eq!(renewed.order_id, made.order_id, "the same order");
    assert_eq!(renewed.address, made.address, "at the same address");

    stand.answer_with(&format!("POST {}", path_of(&renew)), "order-not-renewable");
    let e = stand
        .api
        .renew_order(&stand.connection(), &renew, made.expires_at)
        .await
        .unwrap_err();
    assert_eq!(e.status(), 409);
    assert_eq!(e.problem(), Some("order-not-renewable"));
    assert!(!e.to_string().trim().is_empty());
}

#[tokio::test]
async fn making_an_order_hears_its_recorded_problems_by_name_with_their_detail() {
    let Some(stand) = Contract::start().await else {
        return;
    };
    for (name, status) in [
        ("rates-unavailable", 503),
        ("engine-unavailable", 503),
        ("idempotency-conflict", 409),
    ] {
        stand.answer_with("POST /api/v1/orders", name);
        let e = stand
            .api
            .create_order(
                &stand.connection(),
                &Money::new(2450, CurrencyCode::USD),
                Some("#1042"),
                "sq:abc",
            )
            .await
            .unwrap_err();
        assert_eq!(e.status(), status, "{name}");
        assert_eq!(e.problem(), Some(name));
        assert!(!e.to_string().trim().is_empty(), "{name}");
    }
}

/// Each route the app calls, as the stand-in sees it.
#[derive(Clone, Copy, Debug)]
enum Route {
    Connect,
    Entry,
    Store,
    App,
    Disconnect,
    CreateOrder,
    Order,
    RenewOrder,
    Events,
}

impl Route {
    const ALL: [(Route, &'static str, &'static str); 9] = [
        (Route::Connect, "POST", "/apps/connect"),
        (Route::Entry, "GET", "/api/v1"),
        (Route::Store, "GET", "/api/v1/store"),
        (Route::App, "GET", "/api/v1/app"),
        (Route::Disconnect, "DELETE", "/api/v1/app"),
        (Route::CreateOrder, "POST", "/api/v1/orders"),
        (Route::Order, "GET", "/api/v1/orders/ord_1"),
        (Route::RenewOrder, "POST", "/api/v1/orders/ord_1"),
        (Route::Events, "GET", "/api/v1/orders/events"),
    ];

    /// The call that reaches the route, through a client of its own (so the
    /// entry isn't kept).
    async fn call(self, stand: &Contract) -> Result<(), MonokuloError> {
        let api = new_api();
        let connection = stand.connection();
        match self {
            Route::Connect => api.connect(&stand.code()).await.map(drop),
            Route::Entry => api.onion_api(&connection).await.map(drop),
            Route::Store => api.store(&connection).await.map(drop),
            Route::App => api.app(&connection).await.map(drop),
            Route::Disconnect => api.disconnect(&connection).await,
            Route::CreateOrder => api
                .create_order(
                    &connection,
                    &Money::new(2450, CurrencyCode::USD),
                    Some("#1042"),
                    "sq:abc",
                )
                .await
                .map(drop),
            Route::Order => api
                .order(&connection, &stand.url("/api/v1/orders/ord_1"))
                .await
                .map(drop),
            Route::RenewOrder => api
                .renew_order(&connection, &stand.url("/api/v1/orders/ord_1"), 99)
                .await
                .map(drop),
            Route::Events => match api.events(&connection).next().await {
                Some(Err(e)) => Err(e),
                _ => Ok(()),
            },
        }
    }
}

#[tokio::test]
async fn every_problem_monokulo_lists_for_a_route_the_app_calls_comes_back_by_name_and_the_app_has_words_for_it()
 {
    let Some(stand) = Contract::start().await else {
        return;
    };
    let mut heard = 0;
    for (route, method, path) in Route::ALL {
        let op = stand
            .shared
            .operation(method, path)
            .unwrap_or_else(|| panic!("Monokulo has no {method} {path}"))
            .clone();
        for (code, response) in responses(&op) {
            let Some(examples) = problem_examples(response) else {
                continue;
            };
            for (name, example) in examples {
                stand.answer_with(&format!("{method} {path}"), name);
                let e = route
                    .call(&stand)
                    .await
                    .expect_err(&format!("{method} {path} {name}"));
                assert_eq!(e.status().to_string(), *code, "{method} {path} {name}");
                assert_eq!(e.problem(), Some(name.as_str()), "{method} {path}");
                assert!(
                    problems::knows(name),
                    "the app has no words for {name}, which {method} {path} can answer"
                );
                assert!(!problems::say(&e).trim().is_empty(), "{name}");
                if let Some(retry) = example.pointer("/value/retry_after") {
                    assert_eq!(
                        e.retry_after().map(|r| r.to_string()),
                        Some(plain(retry)),
                        "{method} {path} {name} waits as long as it says"
                    );
                }
                heard += 1;
            }
        }
    }
    assert!(heard > 30, "heard only {heard} problems");
}

/// The store's order stream (`mk:orders-events`): its events are `ready`,
/// `order` and `lagged`, and an `order` is the order as reading it answers,
/// the one an app made.
#[tokio::test]
async fn the_order_stream_monokulo_sends_carries_the_order_as_reading_it_answers() {
    let Some(stand) = Contract::start().await else {
        return;
    };
    let mut names: Vec<String> = stand
        .shared
        .operation("GET", "/api/v1/orders/events")
        .unwrap()
        .pointer("/responses/200/content/text~1event-stream/x-events")
        .and_then(Value::as_object)
        .expect("Monokulo lists its events")
        .keys()
        .cloned()
        .collect();
    names.sort();
    assert_eq!(names, ["lagged", "order", "ready"]);
    let events: Vec<StreamEvent> = stand
        .api
        .events(&stand.connection())
        .take(2)
        .map(Result::unwrap)
        .collect()
        .await;
    assert_eq!(events[0], StreamEvent::Ready);
    let StreamEvent::Order(sent) = &events[1] else {
        panic!("an order, not {:?}", events[1])
    };
    assert!(
        stand.shared.template(&path_of(&sent.url)).is_some(),
        "its self link is a route"
    );
    let read = stand
        .api
        .order(&stand.connection(), &sent.url)
        .await
        .unwrap();
    assert_eq!(
        read.order_id, sent.order_id,
        "the order it carries is the one it links"
    );
    assert!(sent.confirmations_required > 0);
    let made = stand
        .api
        .create_order(
            &stand.connection(),
            &Money::new(2450, CurrencyCode::USD),
            Some("#1042"),
            "sq:abc",
        )
        .await
        .unwrap();
    assert_eq!(made.order_id, sent.order_id, "and the one an app made");
}

#[tokio::test]
async fn disconnecting_is_the_entrys_disconnect_form() {
    let Some(stand) = Contract::start().await else {
        return;
    };
    stand.api.disconnect(&stand.connection()).await.unwrap();
    assert_eq!(stand.seen().last().unwrap(), "DELETE /api/v1/app");
}
