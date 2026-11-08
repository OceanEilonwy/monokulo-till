//! A store's Monokulo, as a connected app uses it (Android:
//! `core/MonokuloApi.kt`).

use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Method, Response, StatusCode, Url};
use serde::Serialize;
use serde_json::Value;

use crate::clock::Clock;
use crate::connect_code::ConnectCode;
use crate::connection::Connection;
use crate::error::MonokuloError;
use crate::order::Order;
use crate::proof;
use crate::sale::{SaleEvent, SalePhase};
use crate::sse;
use crate::store::{AppConnection, Store};

type Result<T> = std::result::Result<T, MonokuloError>;

/// Runs `$body` with the entry bound to `$entry`; when something followed
/// from it answers 401, 404 or 405, reads the entry again and runs it once
/// more (the key changed, or the API moved on). The entry itself answering
/// 401 is the store having disconnected this device. (A macro rather than a
/// function taking an async closure, whose future the compiler can't yet
/// show is `Send`.)
macro_rules! followed {
    ($api:expr, $connection:expr, |$entry:ident| $body:block) => {{
        let first = {
            let $entry = $api.entry($connection).await?;
            $body
        };
        match first {
            Err(e) if STALE.contains(&e.status()) => {
                $api.forget($connection);
                let $entry = $api.entry($connection).await?;
                $body
            }
            done => done,
        }
    }};
}

/// What the app says about itself when it connects and on every request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppIdentity {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub platform: String,
    /// This device, for when the merchant didn't name it ("Pixel 8").
    pub device_name: String,
}

impl AppIdentity {
    /// The `Monokulo-Client` header's value: `<slug>/<version>`.
    pub fn client(&self) -> String {
        format!("{}/{}", self.slug, self.version)
    }
}

/// Something the store's event stream said.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::large_enum_variant,
    reason = "events arrive one at a time, so boxing a sale would save nothing"
)]
pub enum StreamEvent {
    /// It's open: re-read the orders on screen.
    Ready,
    /// Some events were missed: re-read the orders on screen.
    Lagged,
    Sale(SaleEvent),
}

/// What something followed from the entry answers when the entry should be
/// read again: the key changed, or the API moved on.
const STALE: [u16; 3] = [401, 404, 405];

/// How much of a failed answer's body is read for its problem.
const PROBLEM_BYTES: usize = 16_384;

/// How long the event stream may say nothing before it counts as dead.
/// Monokulo sends a comment every 25 seconds.
const STREAM_SILENCE: Duration = Duration::from_secs(60);

/// A form from `_templates`: how to send it, and where, resolved.
#[derive(Clone, Debug)]
struct Form {
    method: Method,
    target: Url,
}

/// The API's entry: where things are, for this key.
#[derive(Clone, Debug)]
struct Entry {
    store: Option<Url>,
    events: Option<Url>,
    app: Option<Url>,
    onion_api: Option<Url>,
    create_order: Option<Form>,
    disconnect: Option<Form>,
}

/// An entry as read, its `ETag`, and until when (Unix seconds) it may be
/// used without asking.
#[derive(Clone, Debug)]
struct Kept {
    entry: Entry,
    etag: Option<String>,
    fresh_until: i64,
}

/// A request, kept so it can be sent again with a proof of work.
struct Call {
    method: Method,
    url: Url,
    headers: HeaderMap,
    body: Option<Vec<u8>>,
}

struct Inner {
    http: reqwest::Client,
    identity: AppIdentity,
    clock: Arc<dyn Clock>,
    /// Each key's entry, kept as its `Cache-Control` says, by key and where
    /// it was read.
    entries: Mutex<HashMap<String, Kept>>,
}

/// A store's Monokulo, as a connected app uses it: `POST /apps/connect`,
/// then Monokulo's HTTP API from its entry, following its links and forms
/// (Monokulo's docs, `/api/` and `/apps/`).
///
/// Clones share one HTTP client and one set of kept entries. The entry is
/// kept by `clock`, for as long as its `Cache-Control` says.
#[derive(Clone)]
pub struct MonokuloApi {
    inner: Arc<Inner>,
    stream_silence: Duration,
}

impl MonokuloApi {
    /// A client sending through `http` ([`crate::tor::http_client`] in the
    /// app) as `identity`.
    pub fn new(http: reqwest::Client, identity: AppIdentity, clock: Arc<dyn Clock>) -> MonokuloApi {
        MonokuloApi {
            inner: Arc::new(Inner {
                http,
                identity,
                clock,
                entries: Mutex::new(HashMap::new()),
            }),
            stream_silence: STREAM_SILENCE,
        }
    }

    /// The same client with the event stream counted dead after `silence`
    /// with no bytes, rather than 60 seconds.
    pub fn with_stream_silence(mut self, silence: Duration) -> MonokuloApi {
        self.stream_silence = silence;
        self
    }

    /// Redeems `code`, once, at the link's own address, for this device's
    /// key, where the store's API is, and where it is on the onion address
    /// when the store has one. Going over Tor is left for the caller to
    /// choose ([`Connection::tor`]).
    pub async fn connect(&self, code: &ConnectCode) -> Result<Connection> {
        #[derive(Serialize)]
        struct Redeem<'a> {
            token: &'a str,
            slug: &'a str,
            name: &'a str,
            device_name: &'a str,
            platform: &'a str,
        }
        #[derive(serde::Deserialize)]
        struct Redeemed {
            app_key: String,
            api: String,
            #[serde(default)]
            onion_api: Option<String>,
        }
        let identity = &self.inner.identity;
        let body = serde_json::to_vec(&Redeem {
            token: &code.token,
            slug: &identity.slug,
            name: &identity.name,
            device_name: &identity.device_name,
            platform: &identity.platform,
        })?;
        let mut headers = HeaderMap::new();
        headers.insert("Monokulo-Client", header(&identity.client()));
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        let call = Call {
            method: Method::POST,
            url: address(&code.connect_url)?,
            headers,
            body: Some(body),
        };
        self.call(&call, |text, used| {
            let redeemed: Redeemed = serde_json::from_str(text)?;
            let api = used.join(&redeemed.api).map_err(|_| MonokuloError::NoApi)?;
            Ok(Connection {
                app_key: redeemed.app_key,
                api: api.into(),
                onion_api: redeemed
                    .onion_api
                    .and_then(|onion| used.join(&onion).ok())
                    .map(Into::into),
                tor: false,
            })
        })
        .await
    }

    /// The store as it is now: its name, base currency and wallet.
    pub async fn store(&self, connection: &Connection) -> Result<Store> {
        followed!(self, connection, |entry| {
            let url = entry
                .store
                .ok_or(MonokuloError::NotAllowed("read the store"))?;
            let call = self.keyed(connection, Method::GET, url);
            self.call(&call, |text, _| Ok(serde_json::from_str(text)?))
                .await
        })
    }

    /// This device's connection as the store has it: its name, above all.
    pub async fn app(&self, connection: &Connection) -> Result<AppConnection> {
        followed!(self, connection, |entry| {
            let url = entry
                .app
                .ok_or(MonokuloError::NotAllowed("read its connection"))?;
            let call = self.keyed(connection, Method::GET, url);
            self.call(&call, |text, _| Ok(serde_json::from_str(text)?))
                .await
        })
    }

    /// The entry on the store's onion address (`mk:onion-api`), or `None`
    /// while it has none.
    pub async fn onion_api(&self, connection: &Connection) -> Result<Option<String>> {
        Ok(self.entry(connection).await?.onion_api.map(Into::into))
    }

    /// Makes an order with the `create-order` form. `idempotency_key` makes
    /// a retry, or a second till finding the same sale, get the same order.
    /// Never a description: it would carry what was bought
    /// (docs/design.md §4).
    pub async fn create_order(
        &self,
        connection: &Connection,
        amount: &str,
        currency: &str,
        merchant_order_id: Option<&str>,
        idempotency_key: &str,
    ) -> Result<Order> {
        #[derive(Serialize)]
        struct NewOrder<'a> {
            amount: &'a str,
            currency: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            merchant_order_id: Option<&'a str>,
            idempotency_key: &'a str,
        }
        let body = serde_json::to_vec(&NewOrder {
            amount,
            currency,
            merchant_order_id,
            idempotency_key,
        })?;
        followed!(self, connection, |entry| {
            let form = entry
                .create_order
                .ok_or(MonokuloError::NotAllowed("make orders"))?;
            let mut call = self.keyed(connection, form.method, form.target);
            call.headers
                .insert("Content-Type", HeaderValue::from_static("application/json"));
            call.body = Some(body.clone());
            self.call(&call, read_order).await
        })
    }

    /// Reads an order again at `url`: its `self` link, or the `mk:order` link
    /// of a sale event. It needs no entry.
    pub async fn order(&self, connection: &Connection, url: &str) -> Result<Order> {
        let call = self.keyed(connection, Method::GET, address(url)?);
        let read = self.call(&call, read_order).await;
        if let Err(e) = &read
            && STALE.contains(&e.status())
        {
            self.forget(connection);
        }
        read
    }

    /// Disconnects this device with the `disconnect` form: its key stops
    /// working at once. The kept entry is forgotten whatever the answer.
    pub async fn disconnect(&self, connection: &Connection) -> Result<()> {
        let done = async {
            followed!(self, connection, |entry| {
                let form = entry
                    .disconnect
                    .ok_or(MonokuloError::NotAllowed("disconnect itself"))?;
                let call = self.keyed(connection, form.method, form.target);
                self.call(&call, |_, _| Ok(())).await
            })
        }
        .await;
        self.forget(connection);
        done
    }

    /// The store's sale events (`mk:events`), for as long as the stream is
    /// read. Events it doesn't know, and comments, are skipped.
    ///
    /// Nothing is sent until the stream is first polled. It ends after an
    /// error: the stream failed, the store closed it, or it said nothing for
    /// 60 seconds. The caller reconnects (the Android app waits 1 second,
    /// doubling to 30); when the stream answered 401, 404 or 405 the entry is
    /// read again first.
    pub fn events(
        &self,
        connection: &Connection,
    ) -> impl Stream<Item = Result<StreamEvent>> + Send + Unpin + 'static {
        let state = Streaming::Start {
            api: self.clone(),
            connection: connection.clone(),
        };
        Box::pin(futures_util::stream::unfold(state, Streaming::next))
    }

    fn key_of(connection: &Connection) -> String {
        format!("{} {}", connection.app_key, connection.entry())
    }

    fn forget(&self, connection: &Connection) {
        self.entries().remove(&MonokuloApi::key_of(connection));
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<String, Kept>> {
        self.inner.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn now(&self) -> i64 {
        self.inner.clock.now()
    }

    /// The entry for `connection`: the copy kept while it's fresh, else read
    /// again, with its `ETag` so an unchanged one is a `304` with no body.
    async fn entry(&self, connection: &Connection) -> Result<Entry> {
        let key = MonokuloApi::key_of(connection);
        let kept = self.entries().get(&key).cloned();
        if let Some(kept) = &kept
            && self.now() < kept.fresh_until
        {
            return Ok(kept.entry.clone());
        }
        let mut call = self.keyed(connection, Method::GET, address(connection.entry())?);
        if let Some(etag) = kept.as_ref().and_then(|k| k.etag.as_deref())
            && let Ok(value) = HeaderValue::from_str(etag)
        {
            call.headers.insert("If-None-Match", value);
        }
        let response = self.send(&call).await?;
        if response.status() == StatusCode::NOT_MODIFIED
            && let Some(kept) = kept
        {
            let fresh_until = self.now() + max_age(response.headers());
            self.entries().insert(
                key,
                Kept {
                    fresh_until,
                    ..kept.clone()
                },
            );
            return Ok(kept.entry);
        }
        if !response.status().is_success() {
            return Err(failure(response).await);
        }
        let used = response.url().clone();
        let etag = response
            .headers()
            .get("ETag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let age = max_age(response.headers());
        let entry = read_entry(&response.text().await?, &used)?;
        let fresh_until = self.now() + age;
        self.entries().insert(
            key,
            Kept {
                entry: entry.clone(),
                etag,
                fresh_until,
            },
        );
        Ok(entry)
    }

    fn keyed(&self, connection: &Connection, method: Method, url: Url) -> Call {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            header(&format!("Bearer {}", connection.app_key)),
        );
        headers.insert("Monokulo-Client", header(&self.inner.identity.client()));
        Call {
            method,
            url,
            headers,
            body: None,
        }
    }

    /// Runs `call`; `read` gets the body and the URL it came from, which
    /// links resolve against.
    async fn call<T>(&self, call: &Call, read: impl FnOnce(&str, &Url) -> Result<T>) -> Result<T> {
        let response = self.send(call).await?;
        if !response.status().is_success() {
            return Err(failure(response).await);
        }
        let used = response.url().clone();
        read(&response.text().await?, &used)
    }

    /// Sends `call`, and once more with a proof of work when Monokulo
    /// answers `challenge-required` (a busy Monokulo asks one of anyone).
    /// Any other 429 comes back as its problem.
    async fn send(&self, call: &Call) -> Result<Response> {
        let response = self.request(call, None).send().await?;
        if response.status() != StatusCode::TOO_MANY_REQUESTS {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response.bytes().await?;
        let challenge = serde_json::from_slice::<Value>(&body[..body.len().min(PROBLEM_BYTES)])
            .ok()
            .and_then(|body| {
                let challenge = body.get("challenge")?;
                let token = challenge.get("challenge")?.as_str()?.to_owned();
                let difficulty = u32::try_from(challenge.get("difficulty")?.as_u64()?).ok()?;
                Some((token, difficulty))
            });
        let Some((token, difficulty)) = challenge else {
            return Err(problem(status, &headers, &body));
        };
        let solved = tokio::task::spawn_blocking(move || proof::solve(&token, difficulty))
            .await
            .expect("solving a proof of work doesn't panic");
        Ok(self.request(call, Some(solved)).send().await?)
    }

    fn request(&self, call: &Call, proof: Option<String>) -> reqwest::RequestBuilder {
        let mut request = self
            .inner
            .http
            .request(call.method.clone(), call.url.clone())
            .headers(call.headers.clone());
        if let Some(proof) = proof {
            request = request.header("Monokulo-Proof", header(&proof));
        }
        if let Some(body) = &call.body {
            request = request.body(body.clone());
        }
        request
    }
}

/// The event stream's states, for [`futures_util::stream::unfold`].
enum Streaming {
    Start {
        api: MonokuloApi,
        connection: Connection,
    },
    Open {
        body: Pin<Box<dyn Stream<Item = reqwest::Result<bytes::Bytes>> + Send>>,
        reader: sse::Reader,
        url: Url,
        pending: VecDeque<StreamEvent>,
        silence: Duration,
    },
    Done,
}

impl Streaming {
    async fn next(self) -> Option<(Result<StreamEvent>, Streaming)> {
        let mut state = match self {
            Streaming::Start { api, connection } => {
                match Streaming::open(&api, &connection).await {
                    Ok(open) => open,
                    Err(e) => return Some((Err(e), Streaming::Done)),
                }
            }
            Streaming::Done => return None,
            open => open,
        };
        let Streaming::Open {
            body,
            reader,
            url,
            pending,
            silence,
        } = &mut state
        else {
            unreachable!("only an open stream gets here")
        };
        loop {
            if let Some(event) = pending.pop_front() {
                return Some((Ok(event), state));
            }
            let failed = match tokio::time::timeout(*silence, body.next()).await {
                Ok(Some(Ok(bytes))) => {
                    for event in reader.push(&bytes) {
                        if let Some(event) = stream_event(event, url) {
                            pending.push_back(event);
                        }
                    }
                    continue;
                }
                Ok(Some(Err(e))) => MonokuloError::Network(e),
                Ok(None) => MonokuloError::StreamClosed,
                Err(_) => MonokuloError::StreamSilent(silence.as_secs()),
            };
            return Some((Err(failed), Streaming::Done));
        }
    }

    async fn open(api: &MonokuloApi, connection: &Connection) -> Result<Streaming> {
        let url = api
            .entry(connection)
            .await?
            .events
            .ok_or(MonokuloError::NotAllowed("follow the store's sales"))?;
        let mut call = api.keyed(connection, Method::GET, url.clone());
        call.headers
            .insert("Accept", HeaderValue::from_static("text/event-stream"));
        let response = api.send(&call).await?;
        let is_stream = response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.trim()
                    .to_ascii_lowercase()
                    .starts_with("text/event-stream")
            });
        if !response.status().is_success() || !is_stream {
            let failed = failure(response).await;
            if STALE.contains(&failed.status()) {
                api.forget(connection);
            }
            return Err(failed);
        }
        Ok(Streaming::Open {
            body: Box::pin(response.bytes_stream()),
            reader: sse::Reader::default(),
            url,
            pending: VecDeque::new(),
            silence: api.stream_silence,
        })
    }
}

/// The stream event `event` is, or `None` for one this build doesn't know
/// or a sale it can't read.
fn stream_event(event: sse::Event, stream: &Url) -> Option<StreamEvent> {
    match event.name.as_deref()? {
        "ready" => Some(StreamEvent::Ready),
        "lagged" => Some(StreamEvent::Lagged),
        name if SalePhase::of_event(name).is_some() => SaleEvent::decode(&event.data, stream)
            .ok()
            .map(StreamEvent::Sale),
        _ => None,
    }
}

fn header(value: &str) -> HeaderValue {
    HeaderValue::from_str(value).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// `url` as an http or https address.
fn address(url: &str) -> Result<Url> {
    Url::parse(url)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .ok_or_else(|| MonokuloError::NotAnAddress(url.to_owned()))
}

fn read_entry(text: &str, used: &Url) -> Result<Entry> {
    let body: Value = serde_json::from_str(text)?;
    let link = |rel: &str| {
        let href = body.get("_links")?.get(rel)?.get("href")?.as_str()?;
        used.join(href).ok()
    };
    let form = |name: &str| {
        let form = body.get("_templates")?.get(name)?;
        let target = used.join(form.get("target")?.as_str()?).ok()?;
        let method = form
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_ascii_uppercase)
            .unwrap_or_else(|| "POST".into());
        Some(Form {
            method: Method::from_bytes(method.as_bytes()).ok()?,
            target,
        })
    };
    Ok(Entry {
        store: link("mk:store"),
        events: link("mk:events"),
        app: link("mk:app"),
        onion_api: link("mk:onion-api"),
        create_order: form("create-order"),
        disconnect: form("disconnect"),
    })
}

fn read_order(text: &str, used: &Url) -> Result<Order> {
    let body: Value = serde_json::from_str(text)?;
    let url = body
        .pointer("/_links/self/href")
        .and_then(Value::as_str)
        .and_then(|href| used.join(href).ok())
        .unwrap_or_else(|| used.clone());
    let mut order: Order = serde_json::from_value(body)?;
    order.url = url.into();
    Ok(order)
}

/// How long (seconds) an answer may be kept: its `max-age`, else not at all,
/// and never with `no-store`.
fn max_age(headers: &HeaderMap) -> i64 {
    let mut age = 0;
    for value in headers.get_all("Cache-Control") {
        let Ok(value) = value.to_str() else { continue };
        for directive in value.split(',') {
            let directive = directive.trim().to_ascii_lowercase();
            if directive == "no-store" {
                return 0;
            }
            if let Some(seconds) = directive.strip_prefix("max-age=") {
                age = seconds.trim_matches('"').parse().unwrap_or(0);
            }
        }
    }
    age.max(0)
}

/// What a failed answer said, as its problem has it (RFC 9457).
async fn failure(response: Response) -> MonokuloError {
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response.bytes().await.unwrap_or_default();
    problem(status, &headers, &body)
}

fn problem(status: u16, headers: &HeaderMap, body: &[u8]) -> MonokuloError {
    let body: Option<Value> = serde_json::from_slice(&body[..body.len().min(PROBLEM_BYTES)]).ok();
    let text = |name: &str| body.as_ref()?.get(name)?.as_str().map(String::from);
    let detail = text("detail")
        .or_else(|| text("title"))
        .unwrap_or_else(|| format!("HTTP {status}"));
    let problem = text("type").map(|t| t.rsplit('/').next().unwrap_or(&t).to_owned());
    let retry_after = body
        .as_ref()
        .and_then(|b| b.get("retry_after"))
        .and_then(Value::as_u64)
        .or_else(|| {
            headers
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok())
        });
    MonokuloError::Refused {
        status,
        detail,
        problem,
        retry_after,
    }
}
