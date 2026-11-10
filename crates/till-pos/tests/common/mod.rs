//! What the ported Kotlin tests share: in-memory stores, the fixtures, and
//! a stand-in point of sale that answers in the order answers were queued,
//! as OkHttp's MockWebServer did.

#![allow(dead_code)]

#[cfg(feature = "clover")]
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

use till_pos::{
    CursorStore, HandledMarker, PosTokens, Rfc3339Instant, StoreError, TokenLock, TokenStore,
};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// A fixture from `tests/fixtures`, copied verbatim from the Kotlin tests'
/// resources.
pub fn fixture(path: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// Unix seconds for an RFC 3339 instant.
pub fn at(text: &str) -> i64 {
    text.parse::<jiff::Timestamp>().unwrap().as_second()
}

/// Unix milliseconds for an RFC 3339 instant.
pub fn millis(text: &str) -> i64 {
    text.parse::<jiff::Timestamp>().unwrap().as_millisecond()
}

#[derive(Default)]
pub struct MemoryTokens(pub Mutex<Option<PosTokens>>);

impl MemoryTokens {
    pub fn holding(tokens: Option<PosTokens>) -> Arc<MemoryTokens> {
        Arc::new(MemoryTokens(Mutex::new(tokens)))
    }

    pub fn get(&self) -> Option<PosTokens> {
        self.0.lock().unwrap().clone()
    }

    pub fn set(&self, tokens: Option<PosTokens>) {
        *self.0.lock().unwrap() = tokens;
    }
}

impl TokenStore for MemoryTokens {
    fn load(&self) -> Result<Option<PosTokens>, StoreError> {
        Ok(self.get())
    }

    fn save(&self, tokens: Option<PosTokens>) {
        self.set(tokens);
    }
}

pub fn tokens(access: &str, expires: &str, refresh: &str, merchant: &str) -> PosTokens {
    PosTokens {
        access_token: access.into(),
        expires_at: expires.parse().unwrap(),
        refresh_token: Some(refresh.into()),
        merchant_id: merchant.into(),
    }
}

#[derive(Default)]
pub struct MemoryCursor(pub Mutex<(Option<String>, Vec<String>)>);

impl MemoryCursor {
    pub fn since(since: Option<&str>) -> Arc<MemoryCursor> {
        Arc::new(MemoryCursor(Mutex::new((
            since.map(Into::into),
            Vec::new(),
        ))))
    }

    pub fn since_value(&self) -> Option<String> {
        self.0.lock().unwrap().0.clone()
    }

    pub fn handled_ids(&self) -> Vec<String> {
        self.0.lock().unwrap().1.clone()
    }
}

/// Kept as the app's store keeps it: the text.
impl CursorStore for MemoryCursor {
    fn since(&self) -> Result<Option<Rfc3339Instant>, StoreError> {
        self.since_value()
            .map(|since| since.parse().map_err(|e| StoreError(format!("{e}"))))
            .transpose()
    }

    fn handled(&self) -> Result<Vec<HandledMarker>, StoreError> {
        Ok(self
            .handled_ids()
            .iter()
            .map(|id| id.parse().unwrap())
            .collect())
    }

    fn save(&self, since: &Rfc3339Instant, handled: &[HandledMarker]) {
        *self.0.lock().unwrap() = (
            Some(since.to_string()),
            handled.iter().map(ToString::to_string).collect(),
        );
    }
}

#[cfg(feature = "clover")]
#[derive(Default)]
pub struct MemoryTenders(pub Mutex<HashMap<String, String>>);

#[cfg(feature = "clover")]
impl MemoryTenders {
    pub fn id(&self, merchant: &str) -> Option<String> {
        self.0.lock().unwrap().get(merchant).cloned()
    }

    pub fn set(&self, merchant: &str, id: &str) {
        self.0.lock().unwrap().insert(merchant.into(), id.into());
    }
}

#[cfg(feature = "clover")]
impl till_pos::clover::TenderStore for MemoryTenders {
    fn tender_id(&self, merchant_id: &str) -> Result<Option<String>, StoreError> {
        Ok(self.id(merchant_id))
    }

    fn save(&self, merchant_id: &str, tender_id: Option<&str>) {
        let mut ids = self.0.lock().unwrap();
        match tender_id {
            Some(id) => ids.insert(merchant_id.into(), id.into()),
            None => ids.remove(merchant_id),
        };
    }
}

/// A store whose reads fail (the Keychain locked, the Keystore gone), which
/// counts what is saved over it: nothing should be.
#[derive(Default)]
pub struct Unreadable {
    pub saves: Mutex<u32>,
}

impl Unreadable {
    pub fn saves(&self) -> u32 {
        *self.saves.lock().unwrap()
    }

    fn refused<T>(&self) -> Result<T, StoreError> {
        Err(StoreError("the Keychain is locked".into()))
    }

    fn saved(&self) {
        *self.saves.lock().unwrap() += 1;
    }
}

impl TokenStore for Unreadable {
    fn load(&self) -> Result<Option<PosTokens>, StoreError> {
        self.refused()
    }

    fn save(&self, _: Option<PosTokens>) {
        self.saved();
    }
}

impl CursorStore for Unreadable {
    fn since(&self) -> Result<Option<Rfc3339Instant>, StoreError> {
        self.refused()
    }

    fn handled(&self) -> Result<Vec<HandledMarker>, StoreError> {
        self.refused()
    }

    fn save(&self, _: &Rfc3339Instant, _: &[HandledMarker]) {
        self.saved();
    }
}

#[cfg(feature = "clover")]
impl till_pos::clover::TenderStore for Unreadable {
    fn tender_id(&self, _: &str) -> Result<Option<String>, StoreError> {
        self.refused()
    }

    fn save(&self, _: &str, _: Option<&str>) {
        self.saved();
    }
}

/// A lock two sign-ins in one test share, standing in for the `flock` the
/// iOS app and its extension share.
#[derive(Default)]
pub struct SharedLock {
    held: Mutex<bool>,
    freed: Condvar,
}

impl TokenLock for SharedLock {
    fn acquire(&self) -> std::io::Result<()> {
        let mut held = self.held.lock().unwrap();
        while *held {
            held = self.freed.wait(held).unwrap();
        }
        *held = true;
        Ok(())
    }

    fn release(&self) {
        *self.held.lock().unwrap() = false;
        self.freed.notify_one();
    }
}

/// Answers queued in order, each given once.
struct Queue(Arc<Mutex<VecDeque<ResponseTemplate>>>);

impl Respond for Queue {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| ResponseTemplate::new(599).set_body_string("nothing was queued"))
    }
}

/// A stand-in point of sale: MockWebServer's `enqueue`, `takeRequest` and
/// `requestCount` over wiremock.
pub struct Stand {
    pub server: MockServer,
    queue: Arc<Mutex<VecDeque<ResponseTemplate>>>,
    taken: Mutex<usize>,
}

impl Stand {
    pub async fn start() -> Stand {
        let server = MockServer::start().await;
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        Mock::given(any())
            .respond_with(Queue(Arc::clone(&queue)))
            .mount(&server)
            .await;
        Stand {
            server,
            queue,
            taken: Mutex::new(0),
        }
    }

    /// The stand-in's address, with no slash at the end.
    pub fn url(&self) -> String {
        self.server.uri().trim_end_matches('/').to_owned()
    }

    pub fn enqueue(&self, answer: ResponseTemplate) {
        self.queue.lock().unwrap().push_back(answer);
    }

    /// The next request not yet taken.
    pub async fn take_request(&self) -> Request {
        let requests = self.server.received_requests().await.unwrap();
        let mut taken = self.taken.lock().unwrap();
        let request = requests
            .get(*taken)
            .cloned()
            .expect("a request not yet taken");
        *taken += 1;
        request
    }

    pub async fn request_count(&self) -> usize {
        self.server.received_requests().await.unwrap().len()
    }
}

/// A `200` with a JSON body.
pub fn json(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("Content-Type", "application/json")
        .set_body_string(body)
}

/// An answer with `status` and `body`.
pub fn status(code: u16, body: &str) -> ResponseTemplate {
    ResponseTemplate::new(code).set_body_string(body)
}

/// The first value of the query parameter `name`.
pub fn query(request: &Request, name: &str) -> Option<String> {
    query_values(request, name).into_iter().next()
}

/// Every value of the query parameter `name`, in order.
pub fn query_values(request: &Request, name: &str) -> Vec<String> {
    request
        .url
        .query_pairs()
        .filter(|(n, _)| n == name)
        .map(|(_, v)| v.into_owned())
        .collect()
}

pub fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .map(|v| v.to_str().unwrap().to_owned())
}

pub fn body_text(request: &Request) -> String {
    String::from_utf8(request.body.clone()).unwrap()
}

pub fn body_json(request: &Request) -> serde_json::Map<String, serde_json::Value> {
    serde_json::from_slice(&request.body).unwrap()
}
