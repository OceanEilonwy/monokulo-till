//! The Bell's HTTP API (Android: `push/BellApi.kt`).

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use monokulo_client::PointOfSale;
use reqwest::StatusCode;
use reqwest::header::{self, HeaderMap};
use serde::{Deserialize, Serialize};

use crate::{BellAddress, BellInfo, PushSubscription};

/// What `POST /v1/register` answers (monokulo's `docs/bell.md`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BellRegistered {
    pub registration_id: String,
}

/// The Bell's three routes a till uses, and nothing else.
/// [`BellWakeUps`](crate::BellWakeUps) drives it; tests fake it.
#[async_trait]
pub trait BellClient: Send + Sync {
    /// `GET /v1/info`.
    async fn info(&self, bell: &BellAddress) -> Result<BellInfo, BellError>;

    /// `POST /v1/register` for `point_of_sale` in `environment` (decision
    /// 305): the one request that carries the point of sale's token (A9.1).
    /// `location_id` is the Square location, or the Clover merchant (a
    /// Clover merchant is its own location: acceptance D1).
    async fn register(
        &self,
        bell: &BellAddress,
        point_of_sale: PointOfSale,
        environment: &str,
        access_token: &str,
        location_id: &str,
        subscription: &PushSubscription,
    ) -> Result<BellRegistered, BellError>;

    /// `DELETE /v1/register/{id}`: a `404` (taken back already) is as good
    /// as a `204`.
    async fn unregister(&self, bell: &BellAddress, registration_id: &str) -> Result<(), BellError>;
}

/// The Bell answered with an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BellProblem {
    /// The HTTP status.
    pub status: u16,
    /// The problem's type: the slug of the section of monokulo's
    /// `docs/bell.md` it links to (`rate-limited`). `None` for an answer
    /// that wasn't a problem.
    pub slug: Option<String>,
    /// The problem's detail, when it had one that isn't blank.
    pub detail: Option<String>,
    /// How many seconds to wait before asking again: `Retry-After`, else the
    /// problem's `retry_after`.
    pub retry_after: Option<i64>,
}

impl BellProblem {
    /// A problem with a status and nothing else: an error answer that
    /// wasn't a problem.
    pub fn status(status: u16) -> BellProblem {
        BellProblem {
            status,
            slug: None,
            detail: None,
            retry_after: None,
        }
    }
}

impl fmt::Display for BellProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the Bell answered {}", self.status)?;
        match &self.slug {
            Some(slug) => write!(f, " {slug}"),
            None => Ok(()),
        }
    }
}

/// Why a call to the Bell, or something a pass needed on the device,
/// failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BellError {
    /// The Bell answered with an error.
    #[error("{0}")]
    Problem(BellProblem),
    /// An answer that isn't a Bell's: a web page at a wrong address, say.
    #[error("not a Bell")]
    NotABell,
    /// The Bell didn't answer in time.
    #[error("the Bell didn't answer in time")]
    Timeout,
    /// The Bell couldn't be reached at all.
    #[error("couldn't reach the Bell: {0}")]
    Connection(String),
    /// Something on this device failed (decision 120).
    #[error(transparent)]
    Device(#[from] DeviceError),
}

/// Something on this device failed, as a pass meets it: the platform's
/// push or distributor side, the point of sale's sign-in, or the HTTP client
/// itself. A pass never takes the till down for it; it tries again as for
/// an unreachable Bell (decision 120).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct DeviceError(pub String);

/// How long a call to the Bell gets, unless the [`BellApi`] is made with
/// another.
pub const BELL_TIMEOUT: Duration = Duration::from_secs(10);

/// The Bell's HTTP API (monokulo's `docs/bell.md`). The Bell is reached
/// directly, never through a proxy or Tor (decision 98), and every call
/// ends within its timeout. No request carries an `Authorization` header;
/// the point of sale's token goes in [`BellClient::register`]'s body and
/// nowhere else (A9.1, A9.2).
#[derive(Clone, Debug)]
pub struct BellApi {
    http: reqwest::Client,
}

impl BellApi {
    /// A client whose calls each end within [`BELL_TIMEOUT`].
    pub fn new() -> Result<BellApi, BellError> {
        BellApi::with_timeout(BELL_TIMEOUT)
    }

    /// A client whose calls each end within `timeout`.
    pub fn with_timeout(timeout: Duration) -> Result<BellApi, BellError> {
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(timeout)
            .build()
            .map_err(|e| DeviceError(format!("the HTTP client: {e}")))?;
        Ok(BellApi { http })
    }

    /// Send `request` and read its whole answer.
    async fn call(&self, request: reqwest::RequestBuilder) -> Result<Answer, BellError> {
        let response = request
            .header(header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(unreached)?;
        let status = response.status();
        let headers = response.headers().clone();
        let text = response.text().await.map_err(unreached)?;
        Ok(Answer {
            status,
            headers,
            text,
        })
    }
}

#[async_trait]
impl BellClient for BellApi {
    async fn info(&self, bell: &BellAddress) -> Result<BellInfo, BellError> {
        let answer = self.call(self.http.get(bell.route("/v1/info"))).await?;
        if !answer.status.is_success() {
            return Err(answer.failure(answer.status.as_u16() < 500));
        }
        let wire: Info = serde_json::from_str(&answer.text).map_err(|_| BellError::NotABell)?;
        if wire.points_of_sale.is_empty() || !is_p256_point(&wire.vapid_public_key) {
            return Err(BellError::NotABell);
        }
        // The till reads its own point of sale's own environment only (BellWakeUps): a Bell may run a point of
        // sale or an environment this build doesn't know, in a shape it doesn't know, and that is never read.
        Ok(BellInfo {
            points_of_sale: wire
                .points_of_sale
                .into_iter()
                .map(|(pos, environments)| (pos, applications(&environments)))
                .collect(),
            vapid_public_key: wire.vapid_public_key,
        })
    }

    async fn register(
        &self,
        bell: &BellAddress,
        point_of_sale: PointOfSale,
        environment: &str,
        access_token: &str,
        location_id: &str,
        subscription: &PushSubscription,
    ) -> Result<BellRegistered, BellError> {
        let body = RegisterBody {
            point_of_sale: point_of_sale.wire(),
            environment,
            access_token,
            location_id,
            subscription: RegisterSubscription {
                endpoint: &subscription.endpoint,
                keys: RegisterKeys {
                    p256dh: &subscription.p256dh,
                    auth: &subscription.auth,
                },
            },
        };
        let answer = self
            .call(self.http.post(bell.route("/v1/register")).json(&body))
            .await?;
        if !answer.status.is_success() {
            return Err(answer.failure(false));
        }
        serde_json::from_str(&answer.text).map_err(|_| BellError::NotABell)
    }

    async fn unregister(&self, bell: &BellAddress, registration_id: &str) -> Result<(), BellError> {
        let route = bell.route(&format!("/v1/register/{registration_id}"));
        let answer = self.call(self.http.delete(route)).await?;
        if !answer.status.is_success() && answer.status != StatusCode::NOT_FOUND {
            return Err(answer.failure(false));
        }
        Ok(())
    }
}

/// An HTTP failure as one of [`BellError`]'s.
fn unreached(e: reqwest::Error) -> BellError {
    if e.is_timeout() {
        BellError::Timeout
    } else {
        BellError::Connection(e.to_string())
    }
}

/// A whole answer.
struct Answer {
    status: StatusCode,
    headers: HeaderMap,
    text: String,
}

impl Answer {
    /// An error answer: the Bell's problem when it gave one, else
    /// [`BellError::NotABell`] when `not_a_bell`, else its bare status.
    fn failure(&self, not_a_bell: bool) -> BellError {
        let is_problem = self
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/problem+json"));
        let problem = is_problem
            .then(|| serde_json::from_str::<Problem>(&self.text).ok())
            .flatten();
        if problem.is_none() && not_a_bell {
            return BellError::NotABell;
        }
        let problem = problem.unwrap_or_default();
        let retry_after = self
            .headers
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse().ok())
            .or(problem.retry_after);
        BellError::Problem(BellProblem {
            status: self.status.as_u16(),
            // The type is the section's address on the Bell's page: `…/docs/bell.md#rate-limited`.
            slug: problem
                .r#type
                .map(|t| t.rsplit('#').next().unwrap_or_default().to_owned())
                .filter(|slug| !slug.is_empty()),
            detail: problem.detail.filter(|d| !d.trim().is_empty()),
            retry_after,
        })
    }
}

/// `GET /v1/info` as it comes: each point of sale's environments are read
/// by [`applications`].
#[derive(Deserialize)]
struct Info {
    points_of_sale: BTreeMap<String, serde_json::Value>,
    vapid_public_key: String,
}

/// The register body; the Bell refuses a field it doesn't know.
#[derive(Serialize)]
struct RegisterBody<'a> {
    point_of_sale: &'a str,
    environment: &'a str,
    access_token: &'a str,
    location_id: &'a str,
    subscription: RegisterSubscription<'a>,
}

#[derive(Serialize)]
struct RegisterSubscription<'a> {
    endpoint: &'a str,
    keys: RegisterKeys<'a>,
}

#[derive(Serialize)]
struct RegisterKeys<'a> {
    p256dh: &'a str,
    auth: &'a str,
}

#[derive(Deserialize, Default)]
struct Problem {
    r#type: Option<String>,
    detail: Option<String>,
    retry_after: Option<i64>,
}

/// A point of sale's `{environment: {"application_id": …}}` as environment
/// to application; an entry of another shape is left out.
fn applications(environments: &serde_json::Value) -> BTreeMap<String, String> {
    let Some(environments) = environments.as_object() else {
        return BTreeMap::new();
    };
    environments
        .iter()
        .filter_map(|(environment, entry)| {
            let id = entry.get("application_id")?.as_str()?;
            Some((environment.clone(), id.to_owned()))
        })
        .collect()
}

/// base64url, with or without padding, as Java's URL decoder reads it.
const BASE64URL: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::URL_SAFE,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::Indifferent)
        .with_decode_allow_trailing_bits(true),
);

/// An uncompressed P-256 public point, base64url: 65 bytes, the first 4
/// (what a VAPID key is).
fn is_p256_point(key: &str) -> bool {
    match BASE64URL.decode(key.trim_end_matches('=')) {
        Ok(bytes) => bytes.len() == 65 && bytes[0] == 4,
        Err(_) => false,
    }
}

/// The Bell's client against a stand-in Bell: exactly the three requests,
/// and the point of sale's token in one of them only (A9.1, A9.2, T5.1).
/// Ported from `BellApiTest`.
#[cfg(test)]
mod tests;
