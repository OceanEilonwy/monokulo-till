//! Ported from the Android app's `TillTest`, test for test: the till
//! against a stand-in Monokulo that answers each request with the next
//! answer queued, as MockWebServer did, and fake points of sale.
//!
//! The tests of what the till says for each problem Monokulo can answer
//! `POST /api/v1/orders` with take the problems from Monokulo's OpenAPI
//! document, as monokulo-client's contract test does: `MONOKULO_OPENAPI`
//! names it, and without it those tests say so and pass unchecked.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use monokulo_client::{AppIdentity, FixedClock, PointOfSale};
use serde_json::{Value, json};
use till_pos::{RefundByTill, RefundInPos};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate, matchers::any};

use super::*;
use crate::{Standing, shift_json};

/// What the stand-in answers, in order, and every request it saw.
#[derive(Default)]
struct Queue {
    answers: Mutex<VecDeque<ResponseTemplate>>,
    seen: Mutex<Vec<Request>>,
}

struct Dispatch(Arc<Queue>);

impl Respond for Dispatch {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.0.seen.lock().unwrap().push(request.clone());
        self.0
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| ResponseTemplate::new(503))
    }
}

/// The API's entry, which the client reads before its first order and
/// keeps for an hour.
fn entry() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("Cache-Control", "private, max-age=3600")
        .set_body_raw(
            r#"{"_links":{"self":{"href":"/api/v1"}},"_templates":{"create-order":{"method":"POST","target":"/api/v1/orders","properties":[]}}}"#,
            "application/prs.hal-forms+json",
        )
}

fn order() -> ResponseTemplate {
    ResponseTemplate::new(201).set_body_raw(
        r#"{"order_id":"ord_1","status":"pending","is_final":false,"address":"8Bx","xmr_amount_piconero":80000000000,
           "amount":"13.88","currency":"USD","merchant_order_id":"bP9m","expires_at":1761312000,
           "_links":{"self":{"href":"/api/v1/orders/ord_1"}}}"#,
        "application/json",
    )
}

/// The order as Monokulo reads it back with `confirmations` of the 10 the
/// store asks for.
fn read_back(status: &str, confirmations: u32, code: u16) -> ResponseTemplate {
    ResponseTemplate::new(code).set_body_raw(
        format!(
            r#"{{"order_id":"ord_1","status":"{status}","is_final":false,"address":"8Bx","xmr_amount_piconero":80000000000,
           "amount":"13.88","currency":"USD","merchant_order_id":"bP9m","expires_at":1761312000,
           "confirmations":{confirmations},"confirmations_required":10,"_links":{{"self":{{"href":"/api/v1/orders/ord_1"}}}}}}"#
        ),
        "application/json",
    )
}

struct Fixture {
    server: MockServer,
    queue: Arc<Queue>,
    taken: AtomicUsize,
    monokulo: MonokuloApi,
    connection: Connection,
}

/// A stand-in Monokulo with its entry queued, and a client for it. The
/// client keeps no idle connections, so no pool timer runs to move
/// tokio's paused clock on while a request is answered.
async fn fixture() -> Fixture {
    let server = MockServer::start().await;
    let queue = Arc::new(Queue::default());
    Mock::given(any())
        .respond_with(Dispatch(queue.clone()))
        .mount(&server)
        .await;
    let http = reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .unwrap();
    let monokulo = MonokuloApi::new(
        http,
        AppIdentity {
            slug: "till".into(),
            name: "Till".into(),
            version: "0.1.0".into(),
            platform: "android".into(),
            device_name: "Pixel".into(),
        },
        Arc::new(FixedClock::at(1_761_000_000)),
    );
    let connection = Connection::new("ak_1", format!("{}/api/v1", server.uri()));
    let fixture = Fixture {
        server,
        queue,
        taken: AtomicUsize::new(0),
        monokulo,
        connection,
    };
    fixture.enqueue(entry());
    fixture
}

impl Fixture {
    fn enqueue(&self, answer: ResponseTemplate) {
        self.queue.answers.lock().unwrap().push_back(answer);
    }

    fn till(&self, source: Arc<dyn SaleSource>) -> Till {
        Till::new(source, self.monokulo.clone(), self.connection.clone())
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.server.uri())
    }

    fn request_count(&self) -> usize {
        self.queue.seen.lock().unwrap().len()
    }

    /// The next request not yet taken, in the order they came.
    fn take_request(&self) -> Request {
        let n = self.taken.fetch_add(1, Ordering::SeqCst);
        self.queue.seen.lock().unwrap()[n].clone()
    }

    /// Answers the next order with the recorded problem `name`, as often as
    /// the client asks: twice for `challenge-required` (it solves the
    /// challenge and asks again) and for `unauthorized` (it reads the entry
    /// again and retries).
    fn answer_order_with(&self, openapi: &Value, name: &str) {
        self.enqueue(recorded_order_problem(openapi, name));
        match name {
            "challenge-required" => self.enqueue(recorded_order_problem(openapi, name)),
            "unauthorized" => {
                self.enqueue(entry());
                self.enqueue(recorded_order_problem(openapi, name));
            }
            _ => {}
        }
    }
}

/// Monokulo's OpenAPI document, or `None` (said once per test) when
/// `MONOKULO_OPENAPI` isn't set.
fn openapi() -> Option<Value> {
    let Ok(path) = std::env::var("MONOKULO_OPENAPI") else {
        eprintln!(
            "MONOKULO_OPENAPI isn't set, so what the till says for Monokulo's recorded problems isn't checked. \
             Generate the document with tools/monokulo-openapi/generate.sh at android/gradle.properties' \
             monokulo.sha and set MONOKULO_OPENAPI to its path."
        );
        return None;
    };
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("MONOKULO_OPENAPI is {path}, which can't be read: {e}"));
    Some(serde_json::from_str(&text).expect("Monokulo's OpenAPI document is JSON"))
}

/// `POST /api/v1/orders` in Monokulo's OpenAPI document.
fn create_order(openapi: &Value) -> &Value {
    &openapi["paths"]["/api/v1/orders"]["post"]
}

fn problem_examples(response: &Value) -> Option<&serde_json::Map<String, Value>> {
    response
        .get("content")?
        .get("application/problem+json")?
        .get("examples")?
        .as_object()
}

/// The problem `name` as Monokulo recorded it for `POST /api/v1/orders`:
/// its status, `Retry-After` when the response lists one, and the example
/// body.
fn recorded_order_problem(openapi: &Value, name: &str) -> ResponseTemplate {
    let responses = create_order(openapi)["responses"].as_object().unwrap();
    for (code, response) in responses {
        let Some(body) = problem_examples(response)
            .and_then(|examples| examples.get(name))
            .and_then(|example| example.get("value"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        let mut answer = ResponseTemplate::new(code.parse::<u16>().unwrap()).set_body_raw(
            Value::Object(body.clone()).to_string(),
            "application/problem+json",
        );
        let lists_retry_after = response
            .get("headers")
            .and_then(Value::as_object)
            .is_some_and(|headers| headers.contains_key("Retry-After"));
        if lists_retry_after && let Some(retry_after) = body.get("retry_after") {
            let content = match retry_after {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            answer = answer.insert_header("Retry-After", content.as_str());
        }
        return answer;
    }
    panic!("Monokulo recorded no {name} problem for POST /api/v1/orders")
}

/// The name of every problem Monokulo recorded for `POST /api/v1/orders`.
fn recorded_order_problems(openapi: &Value) -> Vec<String> {
    create_order(openapi)["responses"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|response| {
            problem_examples(response)
                .into_iter()
                .flat_map(|e| e.keys().cloned())
        })
        .collect()
}

fn found(
    id: &str,
    amount_minor: i64,
    currency: &str,
    reference: &str,
    order_id: &str,
    created_at: &str,
    device_id: Option<&str>,
) -> FoundSale {
    FoundSale {
        id: id.into(),
        amount_minor,
        currency: currency.into(),
        reference: Some(reference.into()),
        order_id: Some(order_id.into()),
        created_at: created_at.into(),
        device_id: device_id.map(Into::into),
        device_name: None,
        cash: false,
        title: None,
        point_of_sale: PointOfSale::Square,
    }
}

fn sale() -> FoundSale {
    found(
        "pay_monero",
        1388,
        "USD",
        "bP9m",
        "ord_sq_1",
        "2026-10-22T16:07:00Z",
        Some("dev_front"),
    )
}

/// [`sale`] as Clover took it: what an [`InPosSource`] finds.
fn clover_sale() -> FoundSale {
    FoundSale {
        point_of_sale: PointOfSale::Clover,
        ..sale()
    }
}

fn receipt() -> orf_codec::Receipt {
    serde_json::from_value(json!({
        "orf_version": "0.1.0",
        "receipt": {"receipt_id": "bP9m", "issue_timestamp": "2026-10-22T09:07:00-07:00", "receipt_type": "sale"},
        "merchant": {"legal_name": "The Bean Room", "country_code": "US"},
        "line_items": [
            {"description": "Flat white", "quantity": 2.0, "unit_price": 4.5, "line_total": 9.0},
            {"description": "Croissant", "quantity": 1.0, "unit_price": 3.75, "line_total": 3.75}
        ],
        "tax": {"tax_lines": [{"rate": 0.08875, "tax_amount": 1.13}]},
        "total": 13.88,
        "currency": "USD"
    }))
    .unwrap()
}

/// A `sale.*` event, its fields in the Kotlin constructor's order.
#[expect(
    clippy::too_many_arguments,
    reason = "the Kotlin SaleEvent's constructor"
)]
fn sale_event(
    event: &str,
    order_id: &str,
    amount: &str,
    currency: &str,
    address: &str,
    piconero: u64,
    merchant_order_id: Option<&str>,
    expires_at: i64,
    device_name: &str,
    at: i64,
) -> SaleEvent {
    SaleEvent {
        event: event.into(),
        order_id: order_id.into(),
        amount: amount.into(),
        currency: currency.into(),
        address: address.into(),
        piconero,
        merchant_order_id: merchant_order_id.map(Into::into),
        expires_at,
        device_name: device_name.into(),
        at,
        order_url: None,
    }
}

/// `name` for this till's `ord_1`.
fn ord_1(name: &str, at: i64) -> SaleEvent {
    sale_event(
        name,
        "ord_1",
        "13.88",
        "USD",
        "8Bx",
        80000000000,
        Some("bP9m"),
        99,
        "",
        at,
    )
}

/// `name` for `ord_1` expiring with its order, as the confirmations tests
/// send it.
fn event(name: &str) -> SaleEvent {
    sale_event(
        name,
        "ord_1",
        "13.88",
        "USD",
        "8Bx",
        80000000000,
        Some("bP9m"),
        1761312000,
        "",
        5,
    )
}

/// A POS that has the sales it's given.
struct FakeSource {
    sales: Mutex<Vec<FoundSale>>,
    receipt: Option<orf_codec::Receipt>,
    customer_name: Option<String>,
    handled_ids: Mutex<Vec<String>>,
    refunded: Mutex<Vec<String>>,
    /// The POS is down: refunds fail with this.
    refusal: Mutex<Option<String>>,
}

impl FakeSource {
    fn new(sales: Vec<FoundSale>) -> Arc<FakeSource> {
        FakeSource::with(sales, None, None)
    }

    fn with(
        sales: Vec<FoundSale>,
        receipt: Option<orf_codec::Receipt>,
        customer_name: Option<&str>,
    ) -> Arc<FakeSource> {
        Arc::new(FakeSource {
            sales: Mutex::new(sales),
            receipt,
            customer_name: customer_name.map(Into::into),
            handled_ids: Mutex::default(),
            refunded: Mutex::default(),
            refusal: Mutex::default(),
        })
    }

    fn handled_ids(&self) -> Vec<String> {
        self.handled_ids.lock().unwrap().clone()
    }

    fn refunded(&self) -> Vec<String> {
        self.refunded.lock().unwrap().clone()
    }
}

#[async_trait]
impl SaleSource for FakeSource {
    fn point_of_sale(&self) -> PointOfSale {
        PointOfSale::Square
    }
    async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError> {
        let handled = self.handled_ids();
        Ok(self
            .sales
            .lock()
            .unwrap()
            .iter()
            .filter(|sale| !handled.contains(&sale.id))
            .cloned()
            .collect())
    }
    async fn order(&self, _: &FoundSale) -> Result<SaleOrder, PosError> {
        Ok(SaleOrder {
            receipt: self.receipt.clone(),
            customer_name: self.customer_name.clone(),
        })
    }
    fn refunds(&self) -> Refunds<'_> {
        Refunds::ByTill(self)
    }
    fn handled(&self, sale: &FoundSale) {
        self.handled_ids.lock().unwrap().push(sale.id.clone());
    }
}

#[async_trait]
impl RefundByTill for FakeSource {
    async fn refund(&self, sale: &FoundSale) -> Result<(), PosError> {
        if let Some(refusal) = self.refusal.lock().unwrap().clone() {
            return Err(PosError::answered(PointOfSale::Square, 503, refusal));
        }
        self.refunded.lock().unwrap().push(sale.id.clone());
        Ok(())
    }
}

/// A POS where the cashier refunds a sale themselves (Clover): the till
/// can only look.
struct InPosSource {
    sales: Mutex<Vec<FoundSale>>,
    handled_ids: Mutex<Vec<String>>,
    /// The sales the POS shows refunded.
    refunded_there: Mutex<HashSet<String>>,
    /// How often the till asked.
    looks: AtomicUsize,
    /// The POS can't be reached: looks fail.
    down: Mutex<bool>,
}

impl InPosSource {
    fn new(sales: Vec<FoundSale>) -> Arc<InPosSource> {
        Arc::new(InPosSource {
            sales: Mutex::new(sales),
            handled_ids: Mutex::default(),
            refunded_there: Mutex::default(),
            looks: AtomicUsize::new(0),
            down: Mutex::new(false),
        })
    }

    fn looks(&self) -> usize {
        self.looks.load(Ordering::SeqCst)
    }

    fn set_down(&self, down: bool) {
        *self.down.lock().unwrap() = down;
    }

    fn refund_there(&self, id: &str) {
        self.refunded_there.lock().unwrap().insert(id.into());
    }
}

#[async_trait]
impl SaleSource for InPosSource {
    fn point_of_sale(&self) -> PointOfSale {
        PointOfSale::Clover
    }
    async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError> {
        let handled = self.handled_ids.lock().unwrap().clone();
        Ok(self
            .sales
            .lock()
            .unwrap()
            .iter()
            .filter(|sale| !handled.contains(&sale.id))
            .cloned()
            .collect())
    }
    async fn order(&self, _: &FoundSale) -> Result<SaleOrder, PosError> {
        Ok(SaleOrder::default())
    }
    fn refunds(&self) -> Refunds<'_> {
        Refunds::InPos(self)
    }
    fn handled(&self, sale: &FoundSale) {
        self.handled_ids.lock().unwrap().push(sale.id.clone());
    }
}

#[async_trait]
impl RefundInPos for InPosSource {
    async fn refunded(&self, sale: &FoundSale) -> Result<bool, PosError> {
        self.looks.fetch_add(1, Ordering::SeqCst);
        if *self.down.lock().unwrap() {
            return Err(PosError::answered(
                PointOfSale::Clover,
                503,
                "Clover is down.",
            ));
        }
        Ok(self.refunded_there.lock().unwrap().contains(&sale.id))
    }
}

/// A source whose `new_sales` fails with what `failure` holds, else gives
/// `inner`'s: Kotlin's `object : SaleSource by source`.
struct Failing<S> {
    inner: Arc<S>,
    failure: Mutex<Option<PosError>>,
}

impl<S> Failing<S> {
    fn new(inner: Arc<S>, failure: Option<PosError>) -> Arc<Failing<S>> {
        Arc::new(Failing {
            inner,
            failure: Mutex::new(failure),
        })
    }

    fn fail_with(&self, failure: Option<PosError>) {
        *self.failure.lock().unwrap() = failure;
    }
}

#[async_trait]
impl<S: SaleSource> SaleSource for Failing<S> {
    fn point_of_sale(&self) -> PointOfSale {
        self.inner.point_of_sale()
    }
    async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError> {
        let failure = self.failure.lock().unwrap().clone();
        match failure {
            Some(failure) => Err(failure),
            None => self.inner.new_sales().await,
        }
    }
    async fn order(&self, sale: &FoundSale) -> Result<SaleOrder, PosError> {
        self.inner.order(sale).await
    }
    fn refunds(&self) -> Refunds<'_> {
        self.inner.refunds()
    }
    fn handled(&self, sale: &FoundSale) {
        self.inner.handled(sale)
    }
}

/// A saved shift in memory, kept as the JSON the app writes, so what's
/// saved must round-trip.
#[derive(Default)]
struct MemoryShift {
    saved: Mutex<Option<String>>,
    saves: AtomicUsize,
}

impl MemoryShift {
    fn new() -> Arc<MemoryShift> {
        Arc::default()
    }

    fn of(saved: Option<String>) -> Arc<MemoryShift> {
        Arc::new(MemoryShift {
            saved: Mutex::new(saved),
            saves: AtomicUsize::new(0),
        })
    }

    fn saved(&self) -> Option<String> {
        self.saved.lock().unwrap().clone()
    }

    fn saves(&self) -> usize {
        self.saves.load(Ordering::SeqCst)
    }

    /// What was saved, read back as a new run would.
    fn reloaded(&self) -> Vec<TillSale> {
        MemoryShift::of(self.saved()).load().unwrap()
    }
}

impl ShiftStore for MemoryShift {
    fn load(&self) -> Result<Vec<TillSale>, till_pos::StoreError> {
        Ok(self
            .saved()
            .map(|saved| shift_json::decode(&saved).unwrap())
            .unwrap_or_default())
    }

    fn save(&self, sales: &[TillSale]) {
        self.saves.fetch_add(1, Ordering::SeqCst);
        *self.saved.lock().unwrap() = Some(shift_json::encode(sales));
    }
}

fn only(till: &Till) -> TillSale {
    let state = till.state();
    assert_eq!(state.sales.len(), 1, "{:?}", state.sales);
    state.sales[0].clone()
}

fn clock(at: i64) -> FixedClock {
    FixedClock::at(at)
}

fn body(request: &Request) -> String {
    String::from_utf8(request.body.clone()).unwrap()
}

#[tokio::test]
async fn a_monero_sale_becomes_an_order_whose_code_carries_its_receipt() {
    let f = fixture().await;
    let source = FakeSource::with(vec![sale()], Some(receipt()), None);
    let till = f.till(source.clone());
    f.enqueue(order());
    till.check().await;
    let shown = only(&till);
    assert_eq!(shown.order_id, "ord_1");
    assert!(shown.code.has_receipt, "{:?}", shown.code.why);
    assert!(
        shown.code.uri.starts_with("monero:8Bx?tx_amount=0.08&orf="),
        "{}",
        shown.code.uri
    );
    assert_eq!(source.handled_ids(), ["pay_monero"]);
    assert_eq!(
        shown.address, "8Bx",
        "Copy address copies the order's address"
    );
    assert_eq!(
        shown.url,
        Some(f.url("/api/v1/orders/ord_1")),
        "it is read again at its self link"
    );
    assert_eq!(f.take_request().url.path(), "/api/v1");
    let create = f.take_request();
    assert_eq!(create.url.path(), "/api/v1/orders");
    let sent = body(&create);
    assert!(
        sent.contains(r#""amount":"13.88""#) && sent.contains(r#""merchant_order_id":"bP9m""#),
        "{sent}"
    );
    assert!(
        sent.contains(&format!(
            r#""idempotency_key":"{}""#,
            Till::idempotency_key(&sale())
        )),
        "{sent}"
    );
    assert!(
        !sent.contains("Flat white"),
        "nothing bought goes to Monokulo: {sent}"
    );
}

#[tokio::test]
async fn whom_a_sale_is_for_stays_on_the_till_and_in_its_saved_shift() {
    let f = fixture().await;
    let shift = MemoryShift::new();
    let till = f
        .till(FakeSource::with(
            vec![sale()],
            Some(receipt()),
            Some("Ana López"),
        ))
        .with_store(shift.clone());
    f.enqueue(order());
    till.check().await;
    assert_eq!(only(&till).customer_name.as_deref(), Some("Ana López"));
    let again = f.till(FakeSource::new(vec![])).with_store(shift.clone());
    assert_eq!(only(&again).customer_name.as_deref(), Some("Ana López"));
    f.take_request();
    let sent = body(&f.take_request());
    assert!(
        !sent.contains("Ana"),
        "the name never goes to Monokulo: {sent}"
    );
    assert!(!only(&till).code.uri.contains("Ana"), "nor into the code");
}

/// A store whose shift can't be read (the Keychain locked): its saves are
/// counted, and there should be none.
#[derive(Default)]
struct UnreadableShift {
    saves: AtomicUsize,
}

impl ShiftStore for UnreadableShift {
    fn load(&self) -> Result<Vec<TillSale>, till_pos::StoreError> {
        Err(till_pos::StoreError("the Keychain is locked".into()))
    }

    fn save(&self, _: &[TillSale]) {
        self.saves.fetch_add(1, Ordering::SeqCst);
    }
}

/// Decision 42: a shift the store couldn't read is never saved over, though
/// the till goes on making orders.
#[tokio::test]
async fn a_shift_that_cant_be_read_is_never_saved_over() {
    let f = fixture().await;
    let shift = Arc::new(UnreadableShift::default());
    let till = f
        .till(FakeSource::new(vec![sale()]))
        .with_store(shift.clone());
    assert!(till.shift_unread());
    assert!(till.state().sales.is_empty());
    f.enqueue(order());
    till.check().await;
    assert_eq!(till.state().sales.len(), 1, "the sale is still made");
    assert_eq!(shift.saves.load(Ordering::SeqCst), 0);
    let readable = f
        .till(FakeSource::new(vec![]))
        .with_store(MemoryShift::new());
    assert!(!readable.shift_unread());
}

#[tokio::test]
async fn a_sale_whose_order_fails_is_tried_again_and_only_marked_handled_once_it_has_one() {
    let Some(openapi) = openapi() else { return };
    let f = fixture().await;
    let source = FakeSource::new(vec![sale()]);
    let till = f.till(source.clone());
    f.enqueue(recorded_order_problem(&openapi, "engine-unavailable"));
    till.check().await;
    assert!(till.state().sales.is_empty());
    assert!(till.state().problem.is_some());
    assert!(source.handled_ids().is_empty());
    f.enqueue(order());
    till.check().await;
    assert_eq!(till.state().sales.len(), 1);
    assert_eq!(till.state().problem, None);
    assert_eq!(
        Till::idempotency_key(&sale()),
        Till::idempotency_key(&FoundSale {
            amount_minor: 1,
            ..sale()
        }),
        "the key is the POS payment's alone"
    );
}

#[tokio::test]
async fn a_check_counts_the_sales_no_check_found_before_another_devices_included_and_a_retried_one_once()
 {
    let Some(openapi) = openapi() else { return };
    let f = fixture().await;
    let other = FoundSale {
        id: "pay_2".into(),
        device_id: Some("dev_back".into()),
        device_name: None,
        cash: false,
        ..sale()
    };
    let source = FakeSource::new(vec![sale(), other]);
    let till = f.till(source.clone());
    f.enqueue(recorded_order_problem(&openapi, "engine-unavailable"));
    assert_eq!(
        till.check().await,
        2,
        "this till's sale and the other device's: the Bell rings for both"
    );
    f.enqueue(order());
    assert_eq!(
        till.check().await,
        0,
        "the sale whose order failed was found before"
    );
    assert_eq!(till.check().await, 0, "nothing new");
    source.sales.lock().unwrap().push(FoundSale {
        id: "pay_3".into(),
        device_id: Some("dev_back".into()),
        device_name: None,
        cash: false,
        ..sale()
    });
    assert_eq!(till.check().await, 1);
    let down = f.till(Failing::new(
        source.clone(),
        Some(PosError::answered(
            PointOfSale::Square,
            503,
            "Square is down",
        )),
    ));
    assert_eq!(
        down.check().await,
        0,
        "a POS that can't be read found nothing"
    );
}

#[tokio::test]
async fn an_ended_sign_in_keeps_saying_why_until_the_cashier_signs_in_and_a_sale_found_after_clears_it()
 {
    let f = fixture().await;
    let source = Failing::new(
        InPosSource::new(vec![clover_sale()]),
        Some(PosError::sign_in(
            PointOfSale::Clover,
            401,
            "Clover's sign-in has ended. Sign in again.",
        )),
    );
    let till = f.till(source.clone());
    till.check().await;
    source.fail_with(Some(PosError::sign_in(
        PointOfSale::Clover,
        401,
        "Sign in to Clover.",
    )));
    till.check().await;
    till.check().await;
    assert_eq!(
        till.state().problem.as_deref(),
        Some("Clover's sign-in has ended. Sign in again."),
        "the next checks' missing sign-in doesn't replace why it ended"
    );
    assert!(till.state().sign_in_again);
    source.fail_with(Some(PosError::answered(
        PointOfSale::Clover,
        503,
        "Clover is busy.",
    )));
    till.check().await;
    assert_eq!(
        till.state().problem.as_deref(),
        Some("Clover is busy."),
        "a problem of another kind is still shown"
    );
    assert!(!till.state().sign_in_again);
    source.fail_with(None);
    f.enqueue(order());
    till.check().await;
    assert_eq!(till.state().problem, None);
    assert!(!till.state().sign_in_again);
}

#[tokio::test]
async fn without_a_receipt_the_plain_code_is_shown() {
    let f = fixture().await;
    let source = FakeSource::new(vec![sale()]);
    let till = f.till(source.clone());
    f.enqueue(order());
    till.check().await;
    let shown = only(&till);
    assert!(!shown.code.has_receipt);
    assert_eq!(shown.code.uri, "monero:8Bx?tx_amount=0.08");
    assert_eq!(source.handled_ids(), ["pay_monero"]);
    assert_eq!(f.request_count(), 2, "the entry, then one order");
}

/// The order for another sale, `ord_2`.
fn second_order() -> ResponseTemplate {
    ResponseTemplate::new(201).set_body_raw(
        r#"{"order_id":"ord_2","status":"pending","is_final":false,"address":"8Cz","xmr_amount_piconero":20000000000,
           "amount":"5.00","currency":"USD","merchant_order_id":"p4Xr","expires_at":1761312000,
           "_links":{"self":{"href":"/api/v1/orders/ord_2"}}}"#,
        "application/json",
    )
}

/// [`sale`] rung up as cash.
fn cash_sale() -> FoundSale {
    FoundSale {
        id: "pay_cash".into(),
        cash: true,
        ..sale()
    }
}

#[tokio::test]
async fn a_cash_sale_is_treated_as_monero_by_default_and_never_refunded() {
    let f = fixture().await;
    let source = FakeSource::with(vec![cash_sale()], Some(receipt()), Some("Sam"));
    let till = f.till(source.clone());
    f.enqueue(order());
    till.check().await;
    let shown = only(&till);
    assert!(shown.cash());
    assert!(
        shown.live(),
        "it alerts and shows its code like a Monero sale"
    );
    assert!(shown.code.has_receipt);
    assert_eq!(shown.customer_name.as_deref(), Some("Sam"));
    till.close("ord_1");
    till.refund("ord_1").await;
    assert!(
        source.refunded().is_empty(),
        "cash is never refunded in Square"
    );
    assert_eq!(only(&till).refund_pending_at, None);
}

#[tokio::test]
async fn paid_in_cash_closes_a_cash_sale_with_nothing_to_refund() {
    let f = fixture().await;
    let source = FakeSource::new(vec![
        cash_sale(),
        FoundSale {
            id: "pay_2".into(),
            ..sale()
        },
    ]);
    let till = f.till(source.clone());
    f.enqueue(order());
    f.enqueue(second_order());
    till.check().await;
    till.paid_in_cash("ord_1");
    let state = till.state();
    let cash = state.sale("ord_1").unwrap();
    assert_eq!(cash.standing(), Standing::PaidInCash);
    assert!(!cash.live());
    assert!(!cash.needs_action());
    till.paid_in_cash("ord_2");
    assert_eq!(
        till.state().sale("ord_2").unwrap().paid_in_cash_at,
        None,
        "only a cash sale"
    );
    assert!(source.refunded().is_empty());
}

#[tokio::test]
async fn with_cash_offered_a_cash_sale_waits_quietly_until_its_taken_in_monero() {
    let f = fixture().await;
    let source = FakeSource::with(vec![cash_sale()], Some(receipt()), Some("Sam"));
    let till = f.till(source.clone()).with_options(&TillOptions {
        cash_as_monero: false,
    });
    till.check().await;
    assert_eq!(f.request_count(), 0, "no order until it's taken");
    assert_eq!(source.handled_ids(), ["pay_cash"]);
    let offered = only(&till);
    assert_eq!(offered.order_id, "cash:pay_cash");
    assert!(offered.offered);
    assert_eq!(offered.standing(), Standing::CashOffer { kept: false });
    assert!(!offered.live() && !offered.needs_action() && !offered.open());
    assert_eq!(offered.customer_name.as_deref(), Some("Sam"));
    assert_eq!(offered.amount, "13.88");
    till.keep_as_cash("cash:pay_cash");
    assert_eq!(only(&till).standing(), Standing::CashOffer { kept: true });

    f.enqueue(order());
    assert_eq!(
        till.take_in_monero("cash:pay_cash").await.as_deref(),
        Ok("ord_1")
    );
    let taken = only(&till);
    assert_eq!(taken.order_id, "ord_1", "the order takes the offer's place");
    assert!(taken.cash() && taken.live() && !taken.offered);
    assert!(taken.code.has_receipt);
    assert_eq!(taken.customer_name.as_deref(), Some("Sam"));
    let body: Value = serde_json::from_slice(&f.queue.seen.lock().unwrap()[1].body).unwrap();
    assert!(
        !body.to_string().contains("Sam"),
        "names never go to Monokulo"
    );
    assert_eq!(
        till.take_in_monero("ord_1").await.as_deref(),
        Ok("ord_1"),
        "taken already"
    );
}

#[tokio::test]
async fn a_cash_sale_whose_order_fails_stays_offered_and_says_why() {
    let f = fixture().await;
    let source = FakeSource::new(vec![cash_sale()]);
    let till = f.till(source.clone()).with_options(&TillOptions {
        cash_as_monero: false,
    });
    till.check().await;
    f.enqueue(ResponseTemplate::new(503));
    assert!(till.take_in_monero("cash:pay_cash").await.is_err());
    assert!(only(&till).offered);
    assert!(till.state().problem.is_some());
}

#[tokio::test]
async fn another_devices_sale_is_taken_with_its_name_and_its_receipt() {
    let f = fixture().await;
    let other = FoundSale {
        id: "pay_2".into(),
        device_id: Some("dev_back".into()),
        device_name: Some("Bar iPad".into()),
        cash: false,
        ..sale()
    };
    let source = FakeSource::with(vec![sale(), other], Some(receipt()), Some("Sam"));
    let till = f.till(source.clone()).with_options(&TillOptions {
        cash_as_monero: true,
    });
    f.enqueue(order());
    f.enqueue(second_order());
    till.check().await;
    let state = till.state();
    assert_eq!(state.sales.len(), 2, "{:?}", state.sales);
    let theirs = state.sale("ord_2").unwrap();
    assert!(
        theirs.receipt.is_some(),
        "every till reads the receipt (decision 131)"
    );
    assert!(theirs.code.has_receipt);
    assert_eq!(theirs.customer_name.as_deref(), Some("Sam"));
    assert_eq!(theirs.pos_device(), Some(("dev_back", Some("Bar iPad"))));
    let mine = state.sale("ord_1").unwrap();
    assert!(mine.code.has_receipt);
    assert!(mine.receipt.is_some());
    assert_eq!(source.handled_ids(), ["pay_monero", "pay_2"]);
}

#[tokio::test]
async fn events_move_sales_along_and_other_tills_sales_appear_without_receipts() {
    let f = fixture().await;
    let till = f.till(FakeSource::with(vec![sale()], Some(receipt()), None));
    f.enqueue(order());
    till.check().await;
    let event = |name: &str, order: &str| SaleEvent {
        order_url: Some(format!(
            "https://pay.example/monokulo/api/v1/orders/{order}"
        )),
        ..sale_event(
            name,
            order,
            "5.00",
            "USD",
            "8Cy",
            30000000000,
            None,
            99,
            "Back",
            1,
        )
    };
    till.on_event(&event("sale.paid", "ord_1"));
    assert_eq!(only(&till).phase, SalePhase::Paid);
    till.on_event(&event("sale.waiting", "ord_2"));
    let theirs = till.state().sales[0].clone();
    assert_eq!(theirs.order_id, "ord_2");
    assert!(!theirs.code.has_receipt);
    assert_eq!(theirs.code.uri, "monero:8Cy?tx_amount=0.03");
    assert_eq!(theirs.address, "8Cy");
    assert_eq!(
        theirs.device_name.as_deref(),
        Some("Back"),
        "another till's sale says which device made it"
    );
    assert_eq!(
        theirs.url.as_deref(),
        Some("https://pay.example/monokulo/api/v1/orders/ord_2"),
        "read again at its event's mk:order link"
    );
    // An update for a sale never seen here is ignored.
    till.on_event(&event("sale.paid", "ord_3"));
    assert_eq!(till.state().sales.len(), 2);
}

#[tokio::test]
async fn a_sale_another_till_already_made_keeps_the_phase_the_stores_events_gave_it() {
    let f = fixture().await;
    let till = f.till(FakeSource::with(vec![sale()], Some(receipt()), None));
    // The other till's order arrives over the stream first, and is paid.
    till.on_event(&sale_event(
        "sale.waiting",
        "ord_1",
        "13.88",
        "USD",
        "8Bx",
        80000000000,
        Some("bP9m"),
        99,
        "Back",
        1,
    ));
    till.on_event(&sale_event(
        "sale.paid",
        "ord_1",
        "13.88",
        "USD",
        "8Bx",
        80000000000,
        Some("bP9m"),
        99,
        "Back",
        2,
    ));
    // Then this till finds the same Square sale and gets the same order back.
    f.enqueue(order());
    till.check().await;
    let shown = only(&till);
    assert_eq!(shown.phase, SalePhase::Paid);
    assert!(shown.found.is_some(), "now this till can refund it too");
}

#[tokio::test]
async fn an_unpaid_sale_is_refunded_in_the_pos() {
    let f = fixture().await;
    let source = FakeSource::new(vec![sale()]);
    let till = f.till(source.clone());
    f.enqueue(order());
    till.check().await;
    till.on_status("ord_1", OrderStatus::of(SalePhase::Expired));
    till.refund("ord_1").await;
    assert_eq!(source.refunded(), ["pay_monero"]);
    assert!(only(&till).refunded());
}

#[tokio::test]
async fn a_paid_sale_is_never_refunded_nor_a_refunded_one_twice() {
    let f = fixture().await;
    let source = FakeSource::new(vec![sale()]);
    let till = f.till(source.clone());
    f.enqueue(order());
    till.check().await;
    till.on_event(&ord_1("sale.paid", 2));
    till.refund("ord_1").await;
    let paid = only(&till);
    assert_eq!(paid.phase, SalePhase::Paid);
    assert_eq!(
        paid.refunded_at, None,
        "the payment landed as the cashier said no"
    );
    assert!(source.refunded().is_empty());

    let other = FakeSource::new(vec![sale()]);
    let again = f.till(other.clone());
    f.enqueue(order());
    again.check().await;
    again.on_status("ord_1", OrderStatus::of(SalePhase::Expired));
    again.refund("ord_1").await;
    again.refund("ord_1").await;
    assert_eq!(other.refunded(), ["pay_monero"], "one refund in the POS");
}

#[tokio::test]
async fn a_cancelled_order_ends_its_sale_unpaid_and_can_be_refunded() {
    let f = fixture().await;
    let source = FakeSource::new(vec![sale()]);
    let till = f.till(source.clone());
    f.enqueue(order());
    till.check().await;
    till.on_status("ord_1", OrderStatus::of(Order::phase_of("cancelled")));
    let shown = only(&till);
    assert_eq!(shown.phase, SalePhase::Cancelled);
    assert!(shown.phase.is_final() && shown.phase.is_unpaid());
    till.refund("ord_1").await;
    assert_eq!(source.refunded(), ["pay_monero"]);
}

#[tokio::test]
async fn an_expired_sale_stays_open_while_monokulo_watches_it_and_a_late_payment_pays_it_refunded_or_not()
 {
    let f = fixture().await;
    let till = f.till(FakeSource::new(vec![sale()]));
    f.enqueue(order());
    till.check().await;
    till.on_status("ord_1", OrderStatus::of(SalePhase::Expired).settled(false));
    assert!(
        only(&till).open(),
        "Monokulo still watches it for a late payment"
    );
    till.refund("ord_1").await;
    till.on_event(&sale_event(
        "sale.paid",
        "ord_1",
        "13.88",
        "USD",
        "8Bx",
        80000000000,
        Some("bP9m"),
        99,
        "Back",
        2,
    ));
    let shown = only(&till);
    assert_eq!(shown.phase, SalePhase::Paid);
    assert!(
        shown.refunded(),
        "the till still knows the Square sale was refunded"
    );
    assert!(!shown.open());
    till.on_status("ord_1", OrderStatus::of(SalePhase::Expired).settled(true));
    assert!(
        !only(&till).open(),
        "nothing more will happen once Monokulo says it's final"
    );
}

#[tokio::test]
async fn no_rate_or_a_busy_engine_says_so_at_the_till_and_the_next_check_asks_again_with_the_same_key()
 {
    let Some(openapi) = openapi() else { return };
    let f = fixture().await;
    let source = FakeSource::new(vec![sale()]);
    let till = f.till(source.clone());
    f.enqueue(recorded_order_problem(&openapi, "rates-unavailable"));
    till.check().await;
    assert_eq!(
        till.state().problem.as_deref(),
        Some(
            "Monokulo has no exchange rate right now, so there's no payment code yet. Trying again shortly."
        )
    );
    f.enqueue(recorded_order_problem(&openapi, "engine-unavailable"));
    till.check().await;
    assert_eq!(
        till.state().problem.as_deref(),
        Some(
            "Monokulo's payment engine is busy, so there's no payment code yet. Trying again shortly."
        )
    );
    assert!(
        source.handled_ids().is_empty(),
        "no order was made, so the sale is still to do"
    );
    f.enqueue(order());
    till.check().await;
    assert_eq!(till.state().problem, None);
    assert_eq!(till.state().sales.len(), 1);
    f.take_request();
    let keys: Vec<String> = (0..3)
        .map(|_| {
            let sent: Value = serde_json::from_str(&body(&f.take_request())).unwrap();
            sent["idempotency_key"].as_str().unwrap().to_owned()
        })
        .collect();
    assert_eq!(keys, vec![Till::idempotency_key(&sale()); 3]);
}

#[tokio::test]
async fn a_store_whose_settings_monokulo_cant_read_says_so_at_the_till_and_who_can_fix_it() {
    let Some(openapi) = openapi() else { return };
    let f = fixture().await;
    let source = FakeSource::new(vec![sale()]);
    let till = f.till(source.clone());
    f.answer_order_with(&openapi, "store-unavailable");
    till.check().await;
    assert_eq!(
        till.state().problem.as_deref(),
        Some(
            "Monokulo can't read this store's settings right now, so there's no payment code yet. \
             Trying again shortly; if it keeps happening, ask Monokulo's admin to check its database."
        )
    );
    assert!(
        source.handled_ids().is_empty(),
        "no order was made, so the sale is still to do"
    );
    f.enqueue(order());
    till.check().await;
    assert_eq!(till.state().problem, None);
    assert_eq!(till.state().sales.len(), 1);
}

#[tokio::test]
async fn a_sale_monokulo_cant_price_or_whose_key_it_used_for_another_order_tells_the_cashier_to_take_another_payment()
 {
    let Some(openapi) = openapi() else { return };
    let f = fixture().await;
    let till = f.till(FakeSource::new(vec![sale()]));
    f.answer_order_with(&openapi, "invalid-amount");
    till.check().await;
    assert_eq!(
        till.state().problem.as_deref(),
        Some(
            "Monokulo can't price this sale (unknown currency: NOPE), so it has no payment code. \
             Take another payment for it and refund it in the POS."
        )
    );
    f.answer_order_with(&openapi, "idempotency-conflict");
    till.check().await;
    assert_eq!(
        till.state().problem.as_deref(),
        Some(
            "Monokulo already has a different order for this POS sale, so it made no payment code. \
             Take another payment for it and refund it in the POS."
        )
    );
}

#[tokio::test]
async fn a_busy_store_says_how_long_it_asked_the_till_to_wait() {
    let Some(openapi) = openapi() else { return };
    let f = fixture().await;
    let till = f.till(FakeSource::new(vec![sale()]));
    f.answer_order_with(&openapi, "rate-limited");
    till.check().await;
    assert_eq!(
        till.state().problem.as_deref(),
        Some("The store is busy and asked for 120 seconds' wait before the next try.")
    );
}

#[tokio::test]
async fn every_problem_making_an_order_can_answer_has_its_own_words_at_the_till_and_leaves_the_sale_to_do()
 {
    let Some(openapi) = openapi() else { return };
    let f = fixture().await;
    let source = FakeSource::new(vec![sale()]);
    let till = f.till(source.clone());
    let names = recorded_order_problems(&openapi);
    for name in [
        "store-unavailable",
        "rates-unavailable",
        "engine-unavailable",
        "forbidden",
        "internal",
    ] {
        assert!(names.iter().any(|n| n == name), "{names:?}");
    }
    let mut said = HashSet::new();
    for name in &names {
        f.answer_order_with(&openapi, name);
        till.check().await;
        let problem = till
            .state()
            .problem
            .unwrap_or_else(|| panic!("{name}: no problem shown"));
        assert!(
            !problem.starts_with("Couldn't make the payment code"),
            "{name}: {problem}"
        );
        said.insert(problem);
    }
    assert!(
        source.handled_ids().is_empty(),
        "no order was made, so the sale is still to do"
    );
    assert!(
        said.len() + 2 >= names.len(),
        "each problem says its own thing (the three bad-request ones share words): {said:?}"
    );
}

#[tokio::test]
async fn closing_a_sale_for_now_takes_its_code_down_and_keeps_it_in_the_shift_where_a_late_payment_still_pays_it()
 {
    let f = fixture().await;
    let clock = clock(1_000);
    let till = f
        .till(FakeSource::with(vec![sale()], Some(receipt()), None))
        .with_clock(Arc::new(clock.clone()));
    f.enqueue(order());
    till.check().await;
    let shown = only(&till);
    assert_eq!(
        shown.created_at,
        till_pos::time::unix_seconds("2026-10-22T16:07:00Z").unwrap(),
        "rung up at the POS's time"
    );
    assert_eq!(
        shown.receipt,
        Some(receipt()),
        "what was bought stays on the till, for its own screens"
    );
    assert_eq!(shown.standing(), Standing::Live(SalePhase::Waiting));
    assert!(shown.live());
    assert!(!shown.needs_action());
    clock.set(1_060);
    till.close("ord_1");
    till.close("ord_1");
    let closed = only(&till);
    assert_eq!(closed.standing(), Standing::Closed);
    assert_eq!(
        closed.closed_at,
        Some(1_060),
        "closed once, at the first close"
    );
    assert!(!closed.live());
    assert!(
        closed.needs_action(),
        "Square has it as Monero: refund it or wait"
    );
    assert!(closed.open(), "Monokulo still watches it");
    till.on_event(&ord_1("sale.paid", 2_000));
    let paid = only(&till);
    assert_eq!(
        paid.standing(),
        Standing::Paid {
            late: true,
            after_refund: false
        }
    );
    assert_eq!(paid.paid_at, Some(2_000), "paid at the event's time");
    assert!(!paid.needs_action());
}

#[tokio::test]
async fn show_the_code_again_reopens_a_closed_sale() {
    let f = fixture().await;
    let till = f.till(FakeSource::new(vec![sale()]));
    f.enqueue(order());
    till.check().await;
    till.close("ord_1");
    till.reopen("ord_1");
    let shown = only(&till);
    assert_eq!(shown.standing(), Standing::Live(SalePhase::Waiting));
    assert_eq!(shown.closed_at, None);
}

#[tokio::test]
async fn refunding_a_closed_sale_marks_both_and_a_late_payment_then_says_paid_after_the_refund() {
    let f = fixture().await;
    let clock = clock(10);
    let source = FakeSource::new(vec![sale()]);
    let till = f.till(source.clone()).with_clock(Arc::new(clock.clone()));
    f.enqueue(order());
    till.check().await;
    till.close("ord_1");
    clock.set(20);
    till.refund("ord_1").await;
    let refunded = only(&till);
    assert_eq!(refunded.standing(), Standing::Refunded);
    assert_eq!(refunded.closed_at, Some(10));
    assert_eq!(refunded.refunded_at, Some(20));
    assert!(!refunded.needs_action(), "nothing more to do");
    assert_eq!(source.refunded(), ["pay_monero"]);
    till.on_event(&ord_1("sale.paid", 30));
    assert_eq!(
        only(&till).standing(),
        Standing::Paid {
            late: true,
            after_refund: true
        }
    );
    assert!(
        only(&till).needs_action(),
        "ring it up again, or return the payment"
    );
}

#[tokio::test]
async fn where_the_cashier_refunds_in_the_pos_refunding_marks_the_sale_to_refund_there_and_sends_nothing()
 {
    let f = fixture().await;
    let clock = clock(10);
    let source = InPosSource::new(vec![clover_sale()]);
    let shift = MemoryShift::new();
    let till = f
        .till(source.clone())
        .with_clock(Arc::new(clock.clone()))
        .with_store(shift.clone());
    f.enqueue(order());
    till.check().await;
    till.close("ord_1");
    clock.set(20);
    till.refund("ord_1").await;
    let pending = only(&till);
    assert_eq!(pending.standing(), Standing::RefundInPos);
    assert_eq!(pending.refund_pending_at, Some(20));
    assert_eq!(
        pending.refunded_at, None,
        "not refunded until the POS shows it"
    );
    assert!(
        pending.needs_action(),
        "the cashier still has to refund it there"
    );
    assert_eq!(source.looks(), 0, "refunding asks the POS nothing");
    clock.set(25);
    till.refund("ord_1").await;
    assert_eq!(
        only(&till).refund_pending_at,
        Some(20),
        "told once, when first told"
    );
    assert_eq!(
        only(&f.till(source.clone()).with_store(shift.clone())).standing(),
        Standing::RefundInPos,
        "and saved"
    );
}

#[tokio::test]
async fn ive_refunded_it_marks_the_sale_refunded_when_the_pos_shows_it_and_leaves_it_pending_otherwise()
 {
    let f = fixture().await;
    let clock = clock(10);
    let source = InPosSource::new(vec![clover_sale()]);
    let till = f.till(source.clone()).with_clock(Arc::new(clock.clone()));
    f.enqueue(order());
    till.check().await;
    till.on_status("ord_1", OrderStatus::of(SalePhase::Expired));
    till.refund("ord_1").await;
    assert!(
        !till.confirm_refund("ord_1").await,
        "the POS doesn't show it yet"
    );
    assert_eq!(only(&till).standing(), Standing::RefundInPos);
    source.set_down(true);
    assert!(
        !till.confirm_refund("ord_1").await,
        "the POS couldn't be asked"
    );
    assert_eq!(only(&till).standing(), Standing::RefundInPos);
    source.set_down(false);
    source.refund_there("pay_monero");
    clock.set(40);
    assert!(till.confirm_refund("ord_1").await);
    let refunded = only(&till);
    assert_eq!(refunded.standing(), Standing::Refunded);
    assert_eq!(refunded.refunded_at, Some(40));
    assert!(!refunded.needs_action());
    let looks = source.looks();
    assert!(
        till.confirm_refund("ord_1").await,
        "refunded already: nothing to ask"
    );
    assert_eq!(source.looks(), looks);
    till.on_event(&ord_1("sale.paid", 50));
    assert_eq!(
        only(&till).standing(),
        Standing::Paid {
            late: true,
            after_refund: true
        },
        "paid after the refund, as at Square"
    );
}

#[tokio::test]
async fn a_sale_told_to_refund_in_a_pos_the_till_no_longer_uses_is_refunded_on_the_cashiers_word() {
    let f = fixture().await;
    let clock = clock(10);
    let clover = InPosSource::new(vec![clover_sale()]);
    let shift = MemoryShift::new();
    let till = f
        .till(clover.clone())
        .with_clock(Arc::new(clock.clone()))
        .with_store(shift.clone());
    f.enqueue(order());
    till.check().await;
    till.on_status("ord_1", OrderStatus::of(SalePhase::Expired));
    till.refund("ord_1").await;
    assert_eq!(only(&till).standing(), Standing::RefundInPos);
    // The cashier changed the point of sale to Square: the shift's sales stay, Clover can't be asked.
    let square = FakeSource::new(vec![]);
    clock.set(100);
    let again = f
        .till(square.clone())
        .with_clock(Arc::new(clock.clone()))
        .with_store(shift.clone());
    assert_eq!(only(&again).standing(), Standing::RefundInPos);
    again.check().await;
    assert_eq!(clover.looks(), 0, "the timer asks no POS");
    assert_eq!(
        only(&again).standing(),
        Standing::RefundInPos,
        "and marks nothing itself"
    );
    assert!(
        again.confirm_refund("ord_1").await,
        "nothing to ask: the cashier's word"
    );
    let refunded = only(&again);
    assert_eq!(refunded.standing(), Standing::Refunded);
    assert_eq!(refunded.refunded_at, Some(100));
    assert!(!refunded.needs_action());
    assert!(
        square.refunded().is_empty(),
        "and nothing is refunded at Square"
    );
    assert_eq!(clover.looks(), 0);
    assert_eq!(
        shift.reloaded()[0].standing(),
        Standing::Refunded,
        "and saved"
    );
}

#[tokio::test]
async fn a_clover_sale_refunded_after_a_change_to_square_is_marked_to_refund_in_clover_never_sent_to_square()
 {
    let f = fixture().await;
    let clock = clock(10);
    let shift = MemoryShift::new();
    let clover = InPosSource::new(vec![clover_sale()]);
    let till = f
        .till(clover.clone())
        .with_clock(Arc::new(clock.clone()))
        .with_store(shift.clone());
    f.enqueue(order());
    till.check().await;
    till.on_status("ord_1", OrderStatus::of(SalePhase::Expired));
    // The cashier changed the point of sale to Square before refunding it.
    let square = FakeSource::new(vec![]);
    clock.set(50);
    let again = f
        .till(square.clone())
        .with_clock(Arc::new(clock.clone()))
        .with_store(shift.clone());
    again.refund("ord_1").await;
    assert!(
        square.refunded().is_empty(),
        "Clover's payment id is never sent to Square"
    );
    assert_eq!(again.state().refund_problems.get("ord_1"), None);
    let pending = only(&again);
    assert_eq!(pending.standing(), Standing::RefundInPos);
    assert_eq!(pending.refund_pending_at, Some(50));
    clock.set(60);
    assert!(
        again.confirm_refund("ord_1").await,
        "nothing can ask Clover: the cashier's word"
    );
    let refunded = only(&again);
    assert_eq!(refunded.standing(), Standing::Refunded);
    assert_eq!(refunded.refunded_at, Some(60));
    assert!(square.refunded().is_empty());
    assert_eq!(clover.looks(), 0);
    assert_eq!(
        shift.reloaded()[0].standing(),
        Standing::Refunded,
        "and saved"
    );
}

#[tokio::test]
async fn a_square_sale_refunded_after_a_change_to_clover_is_never_asked_about_at_clover() {
    let f = fixture().await;
    let clock = clock(10);
    let shift = MemoryShift::new();
    let till = f
        .till(FakeSource::new(vec![sale()]))
        .with_clock(Arc::new(clock.clone()))
        .with_store(shift.clone());
    f.enqueue(order());
    till.check().await;
    till.on_status("ord_1", OrderStatus::of(SalePhase::Expired));
    // The cashier changed the point of sale to Clover before refunding it.
    let clover = InPosSource::new(vec![]);
    clock.set(100);
    let again = f
        .till(clover.clone())
        .with_clock(Arc::new(clock.clone()))
        .with_store(shift.clone());
    again.refund("ord_1").await;
    assert_eq!(
        only(&again).standing(),
        Standing::RefundInPos,
        "refund it in Square"
    );
    assert_eq!(only(&again).refund_pending_at, Some(100));
    clock.set(200);
    again.check().await;
    assert_eq!(
        clover.looks(),
        0,
        "the timer asks Clover nothing about a Square payment"
    );
    assert_eq!(only(&again).standing(), Standing::RefundInPos);
    assert!(
        again.confirm_refund("ord_1").await,
        "nothing can ask Square: the cashier's word"
    );
    assert_eq!(clover.looks(), 0);
    assert_eq!(only(&again).standing(), Standing::Refunded);
    assert_eq!(only(&again).refunded_at, Some(200));
    assert_eq!(
        shift.reloaded()[0].standing(),
        Standing::Refunded,
        "and saved"
    );
}

#[tokio::test]
async fn ive_refunded_it_changes_nothing_for_a_sale_never_told_to_refund_in_the_pos() {
    let f = fixture().await;
    let till = f.till(FakeSource::new(vec![sale()]));
    f.enqueue(order());
    till.check().await;
    till.on_status("ord_1", OrderStatus::of(SalePhase::Expired));
    assert!(!till.confirm_refund("ord_1").await);
    assert_eq!(only(&till).standing(), Standing::Expired);
}

#[tokio::test]
async fn the_till_looks_for_a_refund_in_the_pos_at_most_every_30_seconds_and_marks_the_one_it_sees()
{
    let f = fixture().await;
    let clock = clock(100);
    let source = InPosSource::new(vec![clover_sale()]);
    let till = f.till(source.clone()).with_clock(Arc::new(clock.clone()));
    f.enqueue(order());
    till.check().await;
    assert_eq!(source.looks(), 0, "nothing pending, nothing asked");
    till.on_status("ord_1", OrderStatus::of(SalePhase::Expired));
    till.refund("ord_1").await;
    till.check().await;
    assert_eq!(source.looks(), 1);
    clock.set(129);
    till.check().await;
    assert_eq!(
        source.looks(),
        1,
        "not again within {} s",
        Till::REFUND_LOOK_SECONDS
    );
    clock.set(130);
    source.set_down(true);
    assert_eq!(
        till.check().await,
        0,
        "a failed look is no problem for the check"
    );
    assert_eq!(source.looks(), 2);
    assert_eq!(till.state().problem, None);
    assert_eq!(only(&till).standing(), Standing::RefundInPos);
    source.set_down(false);
    source.refund_there("pay_monero");
    clock.set(159);
    till.check().await;
    assert_eq!(source.looks(), 2);
    clock.set(160);
    till.check().await;
    assert_eq!(source.looks(), 3);
    assert_eq!(only(&till).standing(), Standing::Refunded);
    assert_eq!(only(&till).refunded_at, Some(160));
    clock.set(300);
    till.check().await;
    assert_eq!(source.looks(), 3, "nothing pending any more");
}

#[tokio::test]
async fn a_paid_sale_is_never_marked_to_refund_in_the_pos() {
    let f = fixture().await;
    let source = InPosSource::new(vec![clover_sale()]);
    let till = f.till(source.clone());
    f.enqueue(order());
    till.check().await;
    till.on_event(&ord_1("sale.paid", 2));
    till.refund("ord_1").await;
    let paid = only(&till);
    assert_eq!(paid.refund_pending_at, None);
    assert_eq!(paid.standing(), Standing::PAID);
    till.check().await;
    assert_eq!(source.looks(), 0);
}

#[tokio::test]
async fn an_expired_or_reversed_sale_needs_action_until_refunded_while_another_tills_never_does() {
    let f = fixture().await;
    let clock = clock(100);
    let till = f
        .till(FakeSource::new(vec![sale()]))
        .with_clock(Arc::new(clock.clone()));
    f.enqueue(order());
    till.check().await;
    till.on_status("ord_1", OrderStatus::of(SalePhase::Seen));
    clock.set(200);
    till.on_status("ord_1", OrderStatus::of(SalePhase::Expired));
    let expired = only(&till);
    assert_eq!(expired.standing(), Standing::Expired);
    assert_eq!(
        expired.seen_at,
        Some(100),
        "the first time it was seen is kept"
    );
    assert_eq!(expired.ended_at, Some(200));
    assert!(expired.needs_action());
    till.on_status("ord_1", OrderStatus::of(SalePhase::DoubleSpend));
    assert_eq!(only(&till).standing(), Standing::Reversed);
    assert_eq!(only(&till).ended_at, Some(200), "ended once");
    till.refund("ord_1").await;
    assert_eq!(only(&till).standing(), Standing::Refunded);
    // Another till's sale: this till can't refund it, so it never needs action here.
    till.on_event(&sale_event(
        "sale.waiting",
        "ord_2",
        "5.00",
        "USD",
        "8Cy",
        30000000000,
        None,
        99,
        "Back till",
        300,
    ));
    till.on_event(&sale_event(
        "sale.seen",
        "ord_2",
        "5.00",
        "USD",
        "8Cy",
        30000000000,
        None,
        99,
        "Back till",
        310,
    ));
    till.on_event(&sale_event(
        "sale.expired",
        "ord_2",
        "5.00",
        "USD",
        "8Cy",
        30000000000,
        None,
        99,
        "Back till",
        320,
    ));
    let theirs = till.state().sale("ord_2").unwrap().clone();
    assert_eq!(theirs.standing(), Standing::OtherTill);
    assert_eq!(theirs.created_at, 300);
    assert_eq!(theirs.seen_at, Some(310));
    assert_eq!(theirs.ended_at, Some(320));
    assert!(!theirs.needs_action());
    assert_eq!(
        theirs.receipt, None,
        "another till's sale has no items here"
    );
    till.close("ord_2");
    assert_eq!(
        till.state().sale("ord_2").unwrap().standing(),
        Standing::OtherTill,
        "closing it here changes nothing about who has its Square sale"
    );
}

#[tokio::test]
async fn a_till_started_on_a_saved_shift_shows_its_sales_marks_and_receipts_and_saves_every_change()
{
    let f = fixture().await;
    let shift = MemoryShift::new();
    let first = f
        .till(FakeSource::with(vec![sale()], Some(receipt()), None))
        .with_clock(Arc::new(clock(50)))
        .with_store(shift.clone());
    f.enqueue(order());
    first.check().await;
    first.close("ord_1");
    assert!(shift.saves() >= 2, "the new sale, then its close");
    assert!(
        !shift.saved().unwrap().contains("problem"),
        "only the sales are saved"
    );
    // Android ended the process: the next run starts from what was saved.
    let again = f.till(FakeSource::new(vec![])).with_store(shift.clone());
    let shown = only(&again);
    assert_eq!(shown.order_id, "ord_1");
    assert_eq!(shown.closed_at, Some(50));
    assert_eq!(shown.standing(), Standing::Closed);
    assert_eq!(
        shown.receipt,
        Some(receipt()),
        "what was bought is back for the till's own screens"
    );
    assert_eq!(shown.found, Some(sale()), "and it can still be refunded");
    assert!(shown.code.has_receipt);
    // A check that finds nothing changes nothing, so writes nothing.
    let saves = shift.saves();
    again.check().await;
    assert_eq!(shift.saves(), saves);
    again.on_status("ord_1", OrderStatus::of(SalePhase::Expired).settled(true));
    assert_eq!(shift.reloaded()[0].standing(), Standing::Expired);
}

#[tokio::test]
async fn a_saved_sale_keeps_the_point_of_sale_that_took_it_and_one_saved_before_clover_joined_reads_as_squares()
 {
    let f = fixture().await;
    let shift = MemoryShift::new();
    let clover = FoundSale {
        id: "PAY_MONERO_1".into(),
        point_of_sale: PointOfSale::Clover,
        ..sale()
    };
    let till = f
        .till(FakeSource::with(vec![clover], Some(receipt()), None))
        .with_store(shift.clone());
    f.enqueue(order());
    till.check().await;
    assert_eq!(
        shift.reloaded()[0]
            .found
            .as_ref()
            .map(|found| found.point_of_sale),
        Some(PointOfSale::Clover)
    );
    // A shift saved by a Square-only build has no pointOfSale in its found sale.
    let before = shift
        .saved()
        .unwrap()
        .replace(r#","pointOfSale":"Clover""#, "");
    assert!(!before.contains("pointOfSale"));
    assert_eq!(
        MemoryShift::of(Some(before)).load().unwrap()[0]
            .found
            .as_ref()
            .map(|found| found.point_of_sale),
        Some(PointOfSale::Square)
    );
}

#[tokio::test]
async fn a_shift_keeps_its_newest_sales_up_to_the_cap() {
    let f = fixture().await;
    let shift = MemoryShift::new();
    let till = f.till(FakeSource::new(vec![])).with_store(shift.clone());
    for i in 0..Till::MAX_SALES + 3 {
        till.on_event(&sale_event(
            "sale.waiting",
            &format!("ord_{i}"),
            "5.00",
            "USD",
            "8Cy",
            30000000000,
            None,
            99,
            "Back",
            i as i64,
        ));
    }
    let sales = till.state().sales;
    assert_eq!(sales.len(), Till::MAX_SALES);
    assert_eq!(
        sales[0].order_id,
        format!("ord_{}", Till::MAX_SALES + 2),
        "newest first"
    );
    assert_eq!(
        sales.last().unwrap().order_id,
        "ord_3",
        "the oldest three dropped off"
    );
    assert_eq!(shift.reloaded().len(), Till::MAX_SALES);
    // A saved shift longer than the cap (an older build's) is cut down as it loads.
    let long = MemoryShift::new();
    let doubled: Vec<TillSale> = sales
        .iter()
        .cloned()
        .chain(sales.iter().map(|sale| TillSale {
            order_id: format!("{}b", sale.order_id),
            ..sale.clone()
        }))
        .collect();
    long.save(&doubled);
    assert_eq!(
        f.till(FakeSource::new(vec![]))
            .with_store(long)
            .state()
            .sales
            .len(),
        Till::MAX_SALES
    );
}

#[tokio::test]
async fn a_refund_the_pos_refuses_leaves_the_sale_closed_and_unrefunded_says_why_on_that_sale_and_can_be_tried_again()
 {
    let f = fixture().await;
    let source = FakeSource::new(vec![sale()]);
    let till = f.till(source.clone());
    f.enqueue(order());
    till.check().await;
    // The close sheet's "No": close, then refund.
    till.close("ord_1");
    *source.refusal.lock().unwrap() = Some("Square is down".into());
    till.refund("ord_1").await;
    let failed = only(&till);
    assert_eq!(failed.standing(), Standing::Closed);
    assert!(!failed.refunded());
    assert!(failed.needs_action(), "still to refund");
    assert_eq!(
        till.state()
            .refund_problems
            .get("ord_1")
            .map(String::as_str),
        Some("Couldn't refund it in the POS: Square is down. Refund it there instead.")
    );
    till.check().await;
    assert_eq!(till.state().problem, None);
    assert!(
        till.state().refund_problems.contains_key("ord_1"),
        "a check doesn't wipe the sale's refund problem"
    );
    *source.refusal.lock().unwrap() = None;
    till.refund("ord_1").await;
    assert_eq!(only(&till).standing(), Standing::Refunded);
    assert!(till.state().refund_problems.is_empty());
}

#[tokio::test]
async fn a_closed_sale_keeps_its_marks_and_times_through_the_stream_and_a_resync() {
    let f = fixture().await;
    let clock = clock(10);
    let till = f
        .till(FakeSource::new(vec![sale()]))
        .with_clock(Arc::new(clock.clone()));
    f.enqueue(order());
    till.check().await;
    till.on_event(&ord_1("sale.seen", 5));
    clock.set(20);
    till.close("ord_1");
    clock.set(30);
    till.on_status("ord_1", OrderStatus::of(SalePhase::Seen));
    till.on_event(&ord_1("sale.expired", 40));
    let shown = only(&till);
    assert_eq!(shown.seen_at, Some(5));
    assert_eq!(shown.closed_at, Some(20));
    assert_eq!(shown.ended_at, Some(40));
    assert_eq!(
        shown.standing(),
        Standing::Expired,
        "the code running out outranks the close"
    );
}

#[tokio::test]
async fn a_reversed_sale_stays_reversed_through_a_resync_that_reads_pending_and_in_the_saved_shift()
{
    let f = fixture().await;
    let shift = MemoryShift::new();
    let clock = clock(10);
    let till = f
        .till(FakeSource::with(vec![sale()], Some(receipt()), None))
        .with_clock(Arc::new(clock.clone()))
        .with_store(shift.clone());
    f.enqueue(order());
    till.check().await;
    till.on_event(&ord_1("sale.seen", 5));
    till.on_event(&ord_1("sale.double_spend", 7));
    assert_eq!(only(&till).standing(), Standing::Reversed);
    // The app was restarted: the next run loads the saved shift, and the stream's resync reads the
    // order, which says pending again once the double spend voided its payment.
    clock.set(20);
    let again = f
        .till(FakeSource::new(vec![]))
        .with_clock(Arc::new(clock.clone()))
        .with_store(shift.clone());
    again.on_status(
        "ord_1",
        OrderStatus::of(Order::phase_of("pending")).settled(false),
    );
    let shown = only(&again);
    assert_eq!(
        shown.standing(),
        Standing::Reversed,
        "never back to waiting"
    );
    assert!(!shown.live(), "no code, bubble or chip");
    assert!(shown.needs_action());
    assert_eq!(shown.ended_at, Some(7));
    assert_eq!(shift.reloaded()[0].standing(), Standing::Reversed);
    // Nor does it read as expired once its code runs out: it still mustn't be handed over.
    again.on_status("ord_1", OrderStatus::of(SalePhase::Expired).settled(false));
    assert_eq!(only(&again).standing(), Standing::Reversed);
    // Only a payment moves it on.
    again.on_status("ord_1", OrderStatus::of(SalePhase::Paid).settled(true));
    assert_eq!(
        only(&again).standing(),
        Standing::Paid {
            late: true,
            after_refund: false
        }
    );
}

#[tokio::test]
async fn an_expired_sale_never_goes_back_to_waiting_whatever_a_resync_reads() {
    let f = fixture().await;
    let till = f
        .till(FakeSource::new(vec![sale()]))
        .with_clock(Arc::new(clock(10)));
    f.enqueue(order());
    till.check().await;
    till.on_status("ord_1", OrderStatus::of(SalePhase::Expired));
    till.on_status("ord_1", OrderStatus::of(SalePhase::Waiting));
    till.on_status("ord_1", OrderStatus::of(SalePhase::Seen));
    assert_eq!(only(&till).standing(), Standing::Expired);
    assert_eq!(
        only(&till).seen_at,
        None,
        "a step it didn't take isn't timed"
    );
}

#[tokio::test]
async fn a_code_whose_time_ran_out_is_expired_by_the_clock_and_a_payment_still_pays_it() {
    let f = fixture().await;
    let clock = clock(1761311999);
    let till = f
        .till(FakeSource::new(vec![sale()]))
        .with_clock(Arc::new(clock.clone()));
    f.enqueue(order());
    till.check().await;
    assert_eq!(Till::next_lapse(&till.state()), Some(1761312000));
    till.lapse();
    assert!(only(&till).live(), "not before its time");
    clock.set(1761312000);
    till.lapse();
    let lapsed = only(&till);
    assert_eq!(lapsed.standing(), Standing::Expired);
    assert!(
        !lapsed.live(),
        "so it never pulses, takes the panel or waits"
    );
    assert_eq!(lapsed.ended_at, Some(1761312000));
    assert_eq!(Till::next_lapse(&till.state()), None);
    till.on_status("ord_1", OrderStatus::of(SalePhase::Waiting));
    assert_eq!(
        only(&till).standing(),
        Standing::Expired,
        "a server late to expire it doesn't bring it back"
    );
    till.on_status("ord_1", OrderStatus::of(SalePhase::Paid));
    assert_eq!(only(&till).phase, SalePhase::Paid);
}

/// The wall clock of a test on tokio's paused clock, in Unix milliseconds:
/// `start` plus the virtual time since `base`.
#[derive(Clone)]
struct VirtualWall {
    start: i64,
    base: tokio::time::Instant,
}

impl VirtualWall {
    fn ms(&self) -> i64 {
        self.start + self.base.elapsed().as_millis() as i64
    }
}

/// A till's clock reading a second behind a [`VirtualWall`], as two clocks
/// may.
struct SecondBehind(VirtualWall);

impl Clock for SecondBehind {
    fn now(&self) -> i64 {
        (self.0.ms() - 1_000) / 1000
    }
}

#[tokio::test(start_paused = true)]
async fn a_code_is_ended_on_time_even_when_the_tills_clock_reads_a_second_behind_the_waits() {
    // The wait runs on the test's clock in milliseconds; the till stamps a second behind it.
    let wall = VirtualWall {
        start: 1_761_311_990_000,
        base: tokio::time::Instant::now(),
    };
    let f = fixture().await;
    let till = Arc::new(
        f.till(FakeSource::new(vec![sale()]))
            .with_clock(Arc::new(SecondBehind(wall.clone()))),
    );
    f.enqueue(order());
    till.check().await;
    let before = only(&till);
    let lapsing = tokio::spawn({
        let till = till.clone();
        let wall = wall.clone();
        async move { till.lapse_on_time(move || wall.ms()).await }
    });
    tokio::time::sleep(Duration::from_millis(10_000 + 1)).await;
    assert!(
        only(&till).live(),
        "at its end by the wait's clock, the till's own says a second to go"
    );
    tokio::time::sleep(Till::LAPSE_RETRY + Duration::from_millis(1)).await;
    let lapsed = only(&till);
    assert_eq!(
        lapsed.standing(),
        Standing::Expired,
        "ended within two seconds, with nothing else changing"
    );
    assert_eq!(
        TillSale {
            phase: SalePhase::Expired,
            ended_at: lapsed.ended_at,
            ..before
        },
        lapsed,
        "only the clock ended it"
    );
    assert_eq!(lapsed.ended_at, Some(1761312000));
    lapsing.abort();
}

#[tokio::test]
async fn stopping_watching_closes_every_live_sale_which_a_late_payment_still_pays() {
    let f = fixture().await;
    let till = f
        .till(FakeSource::new(vec![sale()]))
        .with_clock(Arc::new(clock(30)));
    f.enqueue(order());
    till.check().await;
    till.on_event(&sale_event(
        "sale.waiting",
        "ord_2",
        "5.00",
        "USD",
        "8Cy",
        30000000000,
        None,
        99,
        "Back till",
        20,
    ));
    till.close_live();
    let state = till.state();
    let [theirs, ours] = &state.sales[..] else {
        panic!("two sales: {:?}", state.sales)
    };
    assert_eq!(ours.standing(), Standing::Closed);
    assert_eq!(ours.closed_at, Some(30));
    assert!(ours.needs_action(), "to refund");
    assert_eq!(theirs.standing(), Standing::OtherTill);
    assert!(!theirs.live());
    till.on_status("ord_1", OrderStatus::of(SalePhase::Paid).settled(true));
    assert_eq!(
        till.state().sale("ord_1").unwrap().standing(),
        Standing::Paid {
            late: true,
            after_refund: false
        }
    );
}

#[tokio::test]
async fn a_sale_keeps_the_stores_threshold_from_its_order_and_its_events_say_what_they_can_of_the_count()
 {
    let f = fixture().await;
    let till = f.till(FakeSource::new(vec![sale()]));
    f.enqueue(read_back("pending", 0, 201));
    till.check().await;
    let made = only(&till);
    assert_eq!(made.confirmations, Some(0));
    assert_eq!(
        made.confirmations_required,
        Some(10),
        "the ring knows what it counts to before anything is paid"
    );
    till.on_event(&event("sale.seen"));
    assert_eq!(
        only(&till).confirmations,
        Some(0),
        "seen: in the pool, no block yet"
    );
    till.on_event(&event("sale.confirming"));
    assert_eq!(
        only(&till).confirmations,
        Some(1),
        "confirming: at least the block it's in"
    );
    till.on_status(
        "ord_1",
        OrderStatus::of(SalePhase::Confirming).counted(4, 10),
    );
    till.on_event(&event("sale.confirming"));
    assert_eq!(
        only(&till).confirmations,
        Some(4),
        "an event again never counts back what a read said"
    );
    till.on_event(&event("sale.seen"));
    assert_eq!(
        only(&till).confirmations,
        Some(0),
        "back in the pool (its block was lost), it has none"
    );
}

#[tokio::test]
async fn an_older_monokulo_that_gives_no_count_leaves_the_ring_to_fill_by_step() {
    let f = fixture().await;
    let till = f.till(FakeSource::new(vec![sale()]));
    f.enqueue(order());
    till.check().await;
    assert_eq!(only(&till).confirmations_required, None);
    till.on_status("ord_1", OrderStatus::of(SalePhase::Confirming));
    let shown = only(&till);
    assert_eq!(shown.confirmations, Some(1));
    assert_eq!(shown.confirmations_required, None, "nothing to count to");
}

#[tokio::test]
async fn a_confirmations_read_that_lands_after_sale_paid_leaves_the_sale_paid_so_its_paid_note_sounds_once()
 {
    let f = fixture().await;
    let till = f.till(FakeSource::new(vec![sale()]));
    f.enqueue(read_back("pending", 0, 201));
    till.check().await;
    till.on_event(&event("sale.confirming"));
    // The read left while the sale was confirming; sale.paid came in before it landed.
    f.enqueue(read_back("confirming", 9, 200));
    let late = f
        .monokulo
        .order(&f.connection, only(&till).url.as_deref().unwrap())
        .await
        .unwrap();
    till.on_event(&event("sale.paid"));
    let paid = only(&till);
    assert_eq!(paid.phase, SalePhase::Paid);
    till.on_confirmations(&late);
    assert_eq!(
        only(&till),
        paid,
        "a read of a sale no longer on its way changes nothing"
    );
}

#[tokio::test(start_paused = true)]
async fn a_confirming_sale_is_read_again_at_once_and_then_every_20_s_until_its_paid() {
    let f = fixture().await;
    let till = Arc::new(f.till(FakeSource::new(vec![sale()])));
    f.enqueue(order());
    till.check().await;
    f.take_request();
    f.take_request();
    let following = tokio::spawn({
        let till = till.clone();
        async move { till.follow_confirmations().await }
    });
    tokio::time::sleep(3 * Till::CONFIRMATIONS_READ).await;
    assert_eq!(
        f.request_count(),
        2,
        "nothing is read while no payment is on its way"
    );
    f.enqueue(read_back("confirming", 4, 200));
    f.enqueue(read_back("confirming", 5, 200));
    f.enqueue(read_back("paid", 10, 200));
    till.on_event(&event("sale.confirming"));
    let mut rx = till.subscribe();
    let counted = rx
        .wait_for(|s| s.sales[0].confirmations == Some(4))
        .await
        .unwrap()
        .sales[0]
        .clone();
    assert_eq!(
        counted.confirmations_required,
        Some(10),
        "read at once: the event only says it's in a block"
    );
    let start = tokio::time::Instant::now();
    rx.wait_for(|s| s.sales[0].confirmations == Some(5))
        .await
        .unwrap();
    assert_eq!(
        start.elapsed().as_millis(),
        Till::CONFIRMATIONS_READ.as_millis(),
        "the next read is a wait later"
    );
    let paid = rx
        .wait_for(|s| s.sales[0].phase == SalePhase::Paid)
        .await
        .unwrap()
        .sales[0]
        .clone();
    assert_eq!(paid.confirmations, Some(10));
    for n in 1..=3 {
        assert_eq!(
            f.take_request().url.path(),
            "/api/v1/orders/ord_1",
            "read {n} at its self link"
        );
    }
    tokio::time::sleep(10 * Till::CONFIRMATIONS_READ).await;
    assert_eq!(f.request_count(), 5, "paid, it's read no more");
    following.abort();
}

/// Square as both of this device's tills see it (the app's, and iOS's
/// Notification Service Extension's): one location's payments, every
/// device's, listed whether handled or not, and one cursor of handled
/// payments, which the two share as they share the kept cursor. Its order
/// reads can fail or hang.
#[derive(Default)]
struct SharedSquare {
    sales: Mutex<Vec<FoundSale>>,
    handled_ids: Mutex<Vec<String>>,
    order_fails: Mutex<bool>,
    order_hangs: Mutex<bool>,
    order_reads: AtomicUsize,
}

impl SharedSquare {
    fn new(sales: Vec<FoundSale>) -> Arc<SharedSquare> {
        Arc::new(SharedSquare {
            sales: Mutex::new(sales),
            ..SharedSquare::default()
        })
    }

    fn handled_ids(&self) -> Vec<String> {
        self.handled_ids.lock().unwrap().clone()
    }

    fn reads(&self) -> usize {
        self.order_reads.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SaleSource for SharedSquare {
    fn point_of_sale(&self) -> PointOfSale {
        PointOfSale::Square
    }
    async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError> {
        Ok(self.look().await?.new)
    }
    async fn look(&self) -> Result<till_pos::Look, PosError> {
        let handled = self.handled_ids();
        let listed = self.sales.lock().unwrap().clone();
        let new = listed
            .iter()
            .filter(|sale| !handled.contains(&sale.id))
            .cloned()
            .collect();
        Ok(till_pos::Look { new, listed })
    }
    async fn order(&self, _: &FoundSale) -> Result<SaleOrder, PosError> {
        self.order_reads.fetch_add(1, Ordering::SeqCst);
        if *self.order_hangs.lock().unwrap() {
            return pending().await;
        }
        if *self.order_fails.lock().unwrap() {
            return Err(PosError::answered(
                PointOfSale::Square,
                503,
                "Square is busy",
            ));
        }
        Ok(SaleOrder {
            receipt: Some(receipt()),
            customer_name: Some("Sam".into()),
        })
    }
    fn refunds(&self) -> Refunds<'_> {
        Refunds::InPos(self)
    }
    fn handled(&self, sale: &FoundSale) {
        self.handled_ids.lock().unwrap().push(sale.id.clone());
    }
}

#[async_trait]
impl RefundInPos for SharedSquare {
    async fn refunded(&self, _: &FoundSale) -> Result<bool, PosError> {
        Ok(false)
    }
}

/// `ord_1`'s `sale.waiting`, as Monokulo sends it to every till of the
/// store, naming the device whose till made it.
fn waiting_from(device_name: &str) -> SaleEvent {
    sale_event(
        "sale.waiting",
        "ord_1",
        "13.88",
        "USD",
        "8Bx",
        80000000000,
        Some("bP9m"),
        1761312000,
        device_name,
        5,
    )
}

#[tokio::test]
async fn a_check_cut_short_before_the_sale_is_kept_leaves_it_to_be_found_again() {
    let f = fixture().await;
    let square = SharedSquare::new(vec![sale()]);
    *square.order_hangs.lock().unwrap() = true;
    let shift = MemoryShift::of(None);
    let till = f.till(square.clone()).with_store(shift.clone());
    f.enqueue(order());
    let cut = tokio::time::timeout(Duration::from_millis(300), till.check()).await;
    assert!(
        cut.is_err(),
        "the deadline came while Square's order was read"
    );
    assert!(
        square.handled_ids().is_empty(),
        "the payment isn't marked handled before its sale is kept"
    );
    assert!(till.state().sales.is_empty());

    // The next look (the app's, or the next extension run) finishes it.
    *square.order_hangs.lock().unwrap() = false;
    let next = f.till(square.clone()).with_store(shift.clone());
    f.enqueue(order());
    next.check().await;
    let kept = only(&next);
    assert!(kept.found.is_some(), "this till's own sale");
    assert!(kept.code.has_receipt);
    assert_eq!(kept.customer_name.as_deref(), Some("Sam"));
    assert_eq!(square.handled_ids(), ["pay_monero"]);
    assert_eq!(shift.reloaded(), next.state().sales);
    let keys: Vec<Value> = f
        .queue
        .seen
        .lock()
        .unwrap()
        .iter()
        .filter(|request| request.method.as_str() == "POST")
        .map(|request| {
            serde_json::from_slice::<Value>(&request.body).unwrap()["idempotency_key"].clone()
        })
        .collect();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], keys[1], "the same order, made again by its key");
}

#[tokio::test]
async fn the_apps_till_takes_in_the_sale_its_extension_kept_rather_than_showing_it_as_another_tills()
 {
    let f = fixture().await;
    let square = SharedSquare::new(vec![sale()]);
    let shift = MemoryShift::of(None);
    // The app's till, running since before the sale.
    let app = f.till(square.clone()).with_store(shift.clone());
    // Monokulo's event for the order the extension is making reaches the
    // app before the extension has kept the sale: shown as another till's
    // for now, as the owner's iPad showed "From iPad".
    app.on_event(&waiting_from("iPad"));
    assert_eq!(only(&app).found, None);
    assert_eq!(only(&app).device_name.as_deref(), Some("iPad"));
    // The extension's look: the order, the receipt, kept, then handled.
    let extension = f.till(square.clone()).with_store(shift.clone());
    f.enqueue(order());
    extension.check().await;
    assert_eq!(square.handled_ids(), ["pay_monero"]);
    let requests = f.request_count();

    app.check().await;
    let shown = only(&app);
    assert!(shown.found.is_some(), "this device's own sale after all");
    assert_eq!(shown.device_name, None, "no longer from another till");
    assert!(shown.code.has_receipt);
    assert_eq!(shown.customer_name.as_deref(), Some("Sam"));
    assert_eq!(
        f.request_count(),
        requests,
        "nothing more asked of Monokulo"
    );
    assert!(
        shift.reloaded()[0].found.is_some(),
        "the app's save keeps what the extension found"
    );
}

#[tokio::test]
async fn an_event_for_a_sale_the_extension_already_kept_is_taken_in_at_once() {
    let f = fixture().await;
    let square = SharedSquare::new(vec![sale()]);
    let shift = MemoryShift::of(None);
    let app = f.till(square.clone()).with_store(shift.clone());
    let extension = f.till(square.clone()).with_store(shift.clone());
    f.enqueue(order());
    extension.check().await;
    app.on_event(&waiting_from("iPad"));
    let shown = only(&app);
    assert!(shown.found.is_some());
    assert!(shown.code.has_receipt);
    assert_eq!(shown.device_name, None);
}

#[tokio::test]
async fn a_sale_learned_from_monokulo_gets_its_receipt_from_the_payment_square_lists() {
    let f = fixture().await;
    let square = SharedSquare::new(vec![FoundSale {
        device_id: Some("dev_back".into()),
        device_name: Some("Bar iPad".into()),
        ..sale()
    }]);
    // Handled already here: this device's other process made its order and
    // its kept record was lost, or this till restarted since.
    square.handled_ids.lock().unwrap().push("pay_monero".into());
    let till = f.till(square.clone());
    till.on_event(&waiting_from("Bar iPad"));
    let requests = f.request_count();
    till.check().await;
    let shown = only(&till);
    assert_eq!(
        shown.found, None,
        "the sale stays another till's to alert and refund"
    );
    assert!(shown.code.has_receipt, "every till reads the receipt");
    assert_eq!(shown.receipt, Some(receipt()));
    assert_eq!(shown.customer_name.as_deref(), Some("Sam"));
    assert_eq!(shown.receipt_note, None);
    assert_eq!(f.request_count(), requests, "nothing sent to Monokulo");
}

#[tokio::test]
async fn a_failed_order_read_says_why_and_is_read_again_while_the_sale_waits() {
    let f = fixture().await;
    let square = SharedSquare::new(vec![sale()]);
    *square.order_fails.lock().unwrap() = true;
    let clock = Arc::new(clock(1_761_000_000));
    let logged = Arc::new(Mutex::new(Vec::<String>::new()));
    let log = logged.clone();
    let till = f
        .till(square.clone())
        .with_clock(clock.clone())
        .with_log(Arc::new(move |line| log.lock().unwrap().push(line)));
    f.enqueue(order());
    till.check().await;
    let shown = only(&till);
    assert!(!shown.code.has_receipt);
    assert_eq!(
        shown.receipt_note.as_deref(),
        Some("No receipt in the code: Square didn't return the order")
    );
    assert_eq!(square.handled_ids(), ["pay_monero"], "its order is kept");
    let lines = logged.lock().unwrap().clone();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("bP9m") && line.contains("Square is busy")),
        "{lines:?}"
    );
    till.check().await;
    assert_eq!(square.reads(), 1, "not read again at once");

    *square.order_fails.lock().unwrap() = false;
    clock.advance(Till::RECEIPT_RETRY_SECONDS);
    till.check().await;
    let shown = only(&till);
    assert_eq!(square.reads(), 2);
    assert!(
        shown.code.has_receipt,
        "the code takes the receipt while it waits"
    );
    assert_eq!(shown.receipt_note, None);
    assert_eq!(shown.customer_name.as_deref(), Some("Sam"));
    till.check().await;
    assert_eq!(square.reads(), 2, "not read once it has its receipt");
}

#[tokio::test]
async fn a_code_already_scanned_keeps_its_code_when_the_order_reads_later() {
    let f = fixture().await;
    let square = SharedSquare::new(vec![sale()]);
    *square.order_fails.lock().unwrap() = true;
    let clock = Arc::new(clock(1_761_000_000));
    let till = f.till(square.clone()).with_clock(clock.clone());
    f.enqueue(order());
    till.check().await;
    let code = only(&till).code;
    till.on_event(&ord_1("sale.seen", 6));
    *square.order_fails.lock().unwrap() = false;
    clock.advance(Till::RECEIPT_RETRY_SECONDS);
    till.check().await;
    assert_eq!(square.reads(), 1, "a scanned code isn't read for again");
    assert_eq!(only(&till).code, code);
}

#[tokio::test]
async fn a_till_with_no_point_of_sale_says_why_its_codes_have_no_receipt() {
    let f = fixture().await;
    let till = f.till(Arc::new(crate::NoSource));
    till.on_event(&waiting_from("Front iPad"));
    till.check().await;
    assert_eq!(
        only(&till).receipt_note.as_deref(),
        Some("This till isn't signed in to Square, so it can't read the order")
    );
}
