use std::collections::BTreeSet;

use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::*;

const ENDPOINT: &str = "https://ntfy.sh/upAbc?up=1";
const P256DH: &str =
    "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM";
const AUTH: &str = "tBHItJI5svbpez7KI4CCXg";

/// A P-256 point, as a Bell's VAPID key is.
const VAPID: &str = P256DH;

fn subscription() -> PushSubscription {
    PushSubscription {
        endpoint: ENDPOINT.into(),
        p256dh: P256DH.into(),
        auth: AUTH.into(),
    }
}

/// A Bell ringing both of Square's environments and one of Clover's (decision 305).
fn info() -> String {
    format!(
        r#"{{"points_of_sale":{{"square":{{"production":{{"application_id":"sq0idp-live"}},"sandbox":{{"application_id":"sandbox-sq0idb-dev"}}}},
        "clover":{{"north_america":{{"application_id":"MONOKULARBELL"}}}}}},"vapid_public_key":"{VAPID}","version":"0.3.0"}}"#
    )
}

const REGISTERED: &str = r#"{"registration_id":"br_1","point_of_sale":"square","environment":"sandbox","application_id":"sandbox-sq0idb-dev"}"#;

/// A stand-in Bell under `/bell`, and the address the till has for it.
struct StandIn {
    server: MockServer,
    bell: BellAddress,
    api: BellApi,
}

impl StandIn {
    async fn start() -> StandIn {
        let server = MockServer::start().await;
        let bell = BellAddress::parse(&format!("{}/bell", server.uri()), true).unwrap();
        StandIn {
            server,
            bell,
            api: BellApi::new().unwrap(),
        }
    }

    /// Answer `verb route` with `response` from now on.
    async fn route(&self, verb: &str, route: &str, response: ResponseTemplate) {
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(response)
            .mount(&self.server)
            .await;
    }

    /// Every request the Bell has had, in order.
    async fn seen(&self) -> Vec<Request> {
        self.server.received_requests().await.unwrap_or_default()
    }

    async fn register(&self) -> Result<BellRegistered, BellError> {
        self.api
            .register(
                &self.bell,
                PointOfSale::Square,
                "sandbox",
                "EAAAtoken",
                "L1",
                &subscription(),
            )
            .await
    }
}

fn json(code: u16, body: &str) -> ResponseTemplate {
    ResponseTemplate::new(code).set_body_raw(body, "application/json")
}

fn problem(code: u16, slug: &str, detail: &str, retry_after: Option<i64>) -> ResponseTemplate {
    let mut body = format!(
        r#"{{"type":"https://github.com/OceanEilonwy/monokular/blob/main/docs/bell.md#{slug}","title":"t","status":{code},"detail":"{detail}""#
    );
    if let Some(retry_after) = retry_after {
        body += &format!(r#","retry_after":{retry_after}"#);
    }
    body += "}";
    let mut response = ResponseTemplate::new(code).set_body_raw(body, "application/problem+json");
    if let Some(retry_after) = retry_after {
        response = response.insert_header("Retry-After", retry_after.to_string().as_str());
    }
    response
}

fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

/// The body as the Bell gets it, its keys checked at every level (acceptance B5, T5.1): the Bell refuses a field it doesn't know.
fn sent_body(request: &Request) -> Value {
    let sent: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(
        keys(&sent),
        BTreeSet::from([
            "point_of_sale",
            "environment",
            "access_token",
            "location_id",
            "subscription"
        ])
    );
    assert_eq!(
        keys(&sent["subscription"]),
        BTreeSet::from(["endpoint", "keys"])
    );
    assert_eq!(
        keys(&sent["subscription"]["keys"]),
        BTreeSet::from(["p256dh", "auth"])
    );
    sent
}

fn map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[tokio::test]
async fn info_reads_each_point_of_sales_environments_and_their_applications_with_no_authorization_and_no_body()
 {
    let bell = StandIn::start().await;
    bell.route("GET", "/bell/v1/info", json(200, &info())).await;
    let read = bell.api.info(&bell.bell).await.unwrap();
    assert_eq!(
        read,
        BellInfo {
            points_of_sale: BTreeMap::from([
                (
                    "square".into(),
                    map(&[
                        ("production", "sq0idp-live"),
                        ("sandbox", "sandbox-sq0idb-dev")
                    ])
                ),
                ("clover".into(), map(&[("north_america", "MONOKULARBELL")])),
            ]),
            vapid_public_key: VAPID.into(),
        }
    );
    assert_eq!(
        read.get(PointOfSale::Clover),
        Some(&map(&[("north_america", "MONOKULARBELL")]))
    );
    let seen = bell.seen().await;
    assert_eq!(seen.len(), 1);
    assert!(seen[0].headers.get("Authorization").is_none());
    assert!(seen[0].body.is_empty());
}

#[tokio::test]
async fn a_bell_running_one_point_of_sale_lists_it_alone_and_one_this_build_doesnt_know_is_kept_unread()
 {
    let bell = StandIn::start().await;
    let body = format!(
        r#"{{"points_of_sale":{{"clover":{{"sandbox":{{"application_id":"MONOKULARBELL"}}}},"toast":{{"us":{{"application_id":"t"}}}}}},"vapid_public_key":"{VAPID}"}}"#
    );
    bell.route("GET", "/bell/v1/info", json(200, &body)).await;
    let read = bell.api.info(&bell.bell).await.unwrap();
    assert_eq!(read.get(PointOfSale::Square), None);
    assert_eq!(
        read.get(PointOfSale::Clover),
        Some(&map(&[("sandbox", "MONOKULARBELL")]))
    );
}

#[tokio::test]
async fn an_environment_this_build_doesnt_know_or_one_in_a_shape_it_doesnt_know_is_passed_over_rather_than_failing_the_bell()
 {
    let bell = StandIn::start().await;
    let body = format!(
        r#"{{"points_of_sale":{{"square":{{"production":{{"application_id":"sq0idp-live","region":"us"}},"staging":{{"application_id":"sq0idp-stg"}},
           "beta":{{"applications":["x"]}},"canary":"sq0idp-c"}},"toast":["x"]}},"vapid_public_key":"{VAPID}"}}"#
    );
    bell.route("GET", "/bell/v1/info", json(200, &body)).await;
    let read = bell.api.info(&bell.bell).await.unwrap();
    assert_eq!(
        read.get(PointOfSale::Square),
        Some(&map(&[
            ("production", "sq0idp-live"),
            ("staging", "sq0idp-stg")
        ]))
    );
    assert_eq!(read.points_of_sale.get("toast"), Some(&BTreeMap::new()));
}

#[tokio::test]
async fn register_sends_exactly_the_point_of_sale_its_environment_the_token_the_location_and_the_subscriptions_public_half()
 {
    let bell = StandIn::start().await;
    bell.route("POST", "/bell/v1/register", json(201, REGISTERED))
        .await;
    assert_eq!(
        bell.register().await,
        Ok(BellRegistered {
            registration_id: "br_1".into()
        })
    );
    let seen = bell.seen().await;
    assert_eq!(seen.len(), 1);
    assert!(seen[0].headers.get("Authorization").is_none());
    let expected: Value = serde_json::from_str(&format!(
        r#"{{"point_of_sale":"square","environment":"sandbox","access_token":"EAAAtoken","location_id":"L1",
           "subscription":{{"endpoint":"{ENDPOINT}","keys":{{"p256dh":"{P256DH}","auth":"{AUTH}"}}}}}}"#
    ))
    .unwrap();
    assert_eq!(sent_body(&seen[0]), expected);
}

#[tokio::test]
async fn a_clover_till_registers_its_merchant_as_the_location_and_its_region_as_the_environment_in_the_same_shape()
 {
    let bell = StandIn::start().await;
    bell.route(
        "POST",
        "/bell/v1/register",
        json(201, r#"{"registration_id":"br_2","point_of_sale":"clover","environment":"europe","application_id":"MONOKULARBELL"}"#),
    )
    .await;
    let registered = bell
        .api
        .register(
            &bell.bell,
            PointOfSale::Clover,
            "europe",
            "clv-token",
            "MCLOVERDEV001",
            &subscription(),
        )
        .await;
    assert_eq!(
        registered,
        Ok(BellRegistered {
            registration_id: "br_2".into()
        })
    );
    let seen = bell.seen().await;
    assert_eq!(seen.len(), 1);
    let expected: Value = serde_json::from_str(&format!(
        r#"{{"point_of_sale":"clover","environment":"europe","access_token":"clv-token","location_id":"MCLOVERDEV001",
           "subscription":{{"endpoint":"{ENDPOINT}","keys":{{"p256dh":"{P256DH}","auth":"{AUTH}"}}}}}}"#
    ))
    .unwrap();
    assert_eq!(sent_body(&seen[0]), expected);
    assert!(
        !String::from_utf8_lossy(&seen[0].body).contains("square_access_token"),
        "the old field is never sent"
    );
}

#[tokio::test]
async fn unregister_deletes_by_the_id_alone_and_a_404_is_as_good_as_a_204() {
    let bell = StandIn::start().await;
    bell.route(
        "DELETE",
        "/bell/v1/register/br_1",
        ResponseTemplate::new(204),
    )
    .await;
    bell.api.unregister(&bell.bell, "br_1").await.unwrap();
    bell.api.unregister(&bell.bell, "br_gone").await.unwrap();
    let seen = bell.seen().await;
    let routes: Vec<String> = seen
        .iter()
        .map(|r| format!("{} {}", r.method, r.url.path()))
        .collect();
    assert_eq!(
        routes,
        [
            "DELETE /bell/v1/register/br_1",
            "DELETE /bell/v1/register/br_gone"
        ]
    );
    for request in &seen {
        assert!(request.headers.get("Authorization").is_none());
        assert!(request.body.is_empty());
    }
}

#[tokio::test]
async fn the_token_is_in_one_request_of_the_three() {
    let bell = StandIn::start().await;
    bell.route("GET", "/bell/v1/info", json(200, &info())).await;
    bell.route("POST", "/bell/v1/register", json(201, REGISTERED))
        .await;
    bell.route(
        "DELETE",
        "/bell/v1/register/br_1",
        ResponseTemplate::new(204),
    )
    .await;
    bell.api.info(&bell.bell).await.unwrap();
    bell.register().await.unwrap();
    bell.api.unregister(&bell.bell, "br_1").await.unwrap();
    let carrying: Vec<String> = bell
        .seen()
        .await
        .into_iter()
        .filter(|request| {
            String::from_utf8_lossy(&request.body).contains("EAAAtoken")
                || request
                    .headers
                    .values()
                    .any(|v| String::from_utf8_lossy(v.as_bytes()).contains("EAAAtoken"))
        })
        .map(|request| request.method.to_string())
        .collect();
    assert_eq!(carrying, ["POST"]);
}

#[tokio::test]
async fn a_problem_is_read_by_its_types_slug_its_detail_and_retry_after() {
    let bell = StandIn::start().await;
    bell.route(
        "POST",
        "/bell/v1/register",
        problem(429, "rate-limited", "slow down", Some(42)),
    )
    .await;
    let Err(BellError::Problem(e)) = bell.register().await else {
        panic!("a problem")
    };
    assert_eq!(e.status, 429);
    assert_eq!(e.slug.as_deref(), Some("rate-limited"));
    assert_eq!(e.detail.as_deref(), Some("slow down"));
    assert_eq!(e.retry_after, Some(42));
}

#[tokio::test]
async fn an_error_without_a_problem_is_its_status_alone() {
    let bell = StandIn::start().await;
    bell.route(
        "POST",
        "/bell/v1/register",
        ResponseTemplate::new(502).set_body_string("<html>Bad gateway</html>"),
    )
    .await;
    assert_eq!(
        bell.register().await,
        Err(BellError::Problem(BellProblem::status(502)))
    );
}

#[tokio::test]
async fn info_from_something_that_isnt_a_bell_says_so() {
    let bell = StandIn::start().await;
    let info = info();
    let answers = [
        ResponseTemplate::new(200).set_body_string("<html>Hello</html>"),
        json(200, r#"{"payments":[]}"#),
        json(
            200,
            &format!(r#"{{"points_of_sale":{{}},"vapid_public_key":"{VAPID}"}}"#),
        ),
        json(200, &format!(r#"{{"vapid_public_key":"{VAPID}"}}"#)),
        json(200, &info.replace(VAPID, "BOS-key")),
        json(200, &info.replace(VAPID, &VAPID.replacen('B', "A", 1))),
        // The Bell before points of sale: its shape isn't one this till reads (acceptance D2).
        json(
            200,
            &format!(
                r#"{{"square_application_id":"sandbox-sq0idb-dev","environment":"sandbox","vapid_public_key":"{VAPID}","version":"0.1.0"}}"#
            ),
        ),
    ];
    for (i, answer) in answers.into_iter().enumerate() {
        bell.server.reset().await;
        bell.route("GET", "/bell/v1/info", answer).await;
        assert_eq!(
            bell.api.info(&bell.bell).await,
            Err(BellError::NotABell),
            "answer {i}"
        );
    }
    bell.server.reset().await;
    assert_eq!(
        bell.api.info(&bell.bell).await,
        Err(BellError::NotABell),
        "no route at all"
    );
}

#[tokio::test]
async fn a_bell_from_before_environments_reads_as_ringing_none_of_the_tills() {
    let bell = StandIn::start().await;
    // One application per point of sale, its environment beside it: no environment maps to an application.
    let body = format!(
        r#"{{"points_of_sale":{{"square":{{"application_id":"sandbox-sq0idb-dev","environment":"sandbox"}}}},"vapid_public_key":"{VAPID}"}}"#
    );
    bell.route("GET", "/bell/v1/info", json(200, &body)).await;
    assert_eq!(
        bell.api
            .info(&bell.bell)
            .await
            .unwrap()
            .get(PointOfSale::Square),
        Some(&BTreeMap::new())
    );
}

#[tokio::test]
async fn info_from_a_bell_that_cant_answer_is_its_status() {
    let bell = StandIn::start().await;
    bell.route("GET", "/bell/v1/info", ResponseTemplate::new(503))
        .await;
    assert_eq!(
        bell.api.info(&bell.bell).await,
        Err(BellError::Problem(BellProblem::status(503)))
    );
}

#[tokio::test]
async fn a_bell_that_doesnt_answer_in_time_is_a_timeout() {
    let bell = StandIn::start().await;
    let slow = BellApi::with_timeout(Duration::from_millis(200)).unwrap();
    bell.route(
        "GET",
        "/bell/v1/info",
        json(200, &info()).set_delay(Duration::from_secs(2)),
    )
    .await;
    assert_eq!(slow.info(&bell.bell).await, Err(BellError::Timeout));
}

#[tokio::test]
async fn a_bell_that_isnt_there_is_an_io_exception() {
    // A port nothing listens on (wiremock keeps its servers for reuse, so one can't be closed).
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let address = BellAddress::parse(&format!("http://127.0.0.1:{port}/bell"), true).unwrap();
    let e = BellApi::new().unwrap().info(&address).await.unwrap_err();
    assert!(
        matches!(e, BellError::Connection(_)),
        "no answer at all: {e:?}"
    );
}
