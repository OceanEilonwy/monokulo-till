//! Clover's REST API, as the till uses it (Android: `CloverApi.kt`).

use std::sync::Arc;

use monokulo_client::PointOfSale;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{CloverAuth, CloverConfig, clover_error, retry_after};
use crate::PosError;
use crate::http;

/// The Clover merchant the till signed in at: its name for the receipt,
/// and the currency and time zone its payments and orders are in
/// (`/properties`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloverMerchant {
    pub id: String,
    pub name: String,
    /// ISO 4217, the merchant's `defaultCurrency`.
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// ISO 3166 alpha-2, which a receipt needs: from the address, else the
    /// locale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

/// One of the merchant's tenders (payment types).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloverTender {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "yes")]
    pub visible: bool,
}

fn yes() -> bool {
    true
}

/// The parts of a Clover payment the till reads. Times are Unix
/// milliseconds.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloverPayment {
    pub id: String,
    /// In the currency's minor unit; a tip is apart from it.
    pub amount: i64,
    pub result: PaymentResult,
    pub created_time: i64,
    #[serde(default)]
    pub voided: bool,
    #[serde(default)]
    pub tender: Option<Ref>,
    #[serde(default)]
    pub order: Option<PaymentOrder>,
    #[serde(default)]
    pub device: Option<Ref>,
    #[serde(default)]
    pub refunds: Elements<CloverRefund>,
}

till_types::wire_enum!(
    /// How a Clover payment ended (its `result`). Only `SUCCESS` is a sale.
    PaymentResult {
        Success = "SUCCESS",
        Fail = "FAIL",
        Initiated = "INITIATED",
        Voided = "VOIDED",
        Voiding = "VOIDING",
        VoidFailed = "VOID_FAILED",
        Auth = "AUTH",
        AuthCompleted = "AUTH_COMPLETED",
        Discount = "DISCOUNT",
        OfflineRetrying = "OFFLINE_RETRYING",
        Pending = "PENDING",
    }
);

till_types::wire_enum!(
    /// Where a Clover order's payment stands (its `paymentState`). Only
    /// `REFUNDED` counts as refunded: a part refund, or a state Clover adds
    /// later, doesn't.
    PaymentState {
        Open = "OPEN",
        Paid = "PAID",
        Refunded = "REFUNDED",
        Credited = "CREDITED",
        PartiallyPaid = "PARTIALLY_PAID",
        PartiallyRefunded = "PARTIALLY_REFUNDED",
    }
);

/// One of the merchant's Clover devices (`/devices`): what the counters
/// menu calls a payment's device (core.md decision 142).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CloverDevice {
    pub id: String,
    /// What the merchant named it ("Front Counter"), when they did.
    #[serde(default)]
    pub name: Option<String>,
    /// "Clover Mini", "Clover Flex", ….
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub serial: Option<String>,
}

impl CloverDevice {
    /// What the till calls it: its name, else its model and the end of its
    /// serial ("Clover Mini 4F2A"), else nothing. One line, at most 80
    /// characters, as a Square device's name is.
    pub fn label(&self) -> Option<String> {
        let line = |s: &str| {
            let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
            (!one.is_empty()).then(|| one.chars().take(80).collect::<String>())
        };
        if let Some(name) = self.name.as_deref().and_then(line) {
            return Some(name);
        }
        let model = self.model.as_deref().and_then(line)?;
        let end: Option<String> = self.serial.as_deref().and_then(line).map(|serial| {
            let chars: Vec<char> = serial.chars().collect();
            chars[chars.len().saturating_sub(4)..].iter().collect()
        });
        Some(match end {
            Some(end) => format!("{model} {end}"),
            None => model,
        })
    }
}

/// A reference to another Clover object, by its id.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Ref {
    pub id: String,
}

/// A payment's order, expanded: its id, and the title the cashier gave it
/// ("Table 4").
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PaymentOrder {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
}

/// A refund of a Clover payment.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CloverRefund {
    pub id: String,
    pub amount: i64,
    #[serde(default)]
    pub voided: bool,
}

/// The parts of a Clover order a receipt is made from.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloverOrder {
    pub id: String,
    #[serde(default)]
    pub currency: Option<String>,
    /// In the currency's minor unit, tax included.
    pub total: i64,
    pub created_time: i64,
    #[serde(default)]
    pub payment_state: Option<PaymentState>,
    #[serde(default)]
    pub line_items: Elements<OrderLineItem>,
}

/// One line of a Clover order. Clover repeats a line item once per unit;
/// an item sold by measure has one line with `unit_qty`, its quantity in
/// thousandths, at `price` each.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderLineItem {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub price: i64,
    #[serde(default)]
    pub unit_qty: Option<i64>,
    #[serde(default)]
    pub note: Option<String>,
}

/// How Clover wraps a collection, nested or not.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Elements<T> {
    #[serde(default = "Vec::new")]
    pub elements: Vec<T>,
}

impl<T> Default for Elements<T> {
    fn default() -> Self {
        Elements {
            elements: Vec::new(),
        }
    }
}

#[derive(Deserialize)]
struct Merchant {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    address: Option<Address>,
}

#[derive(Deserialize)]
struct Address {
    #[serde(default)]
    country: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Properties {
    #[serde(default)]
    default_currency: Option<String>,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    locale: Option<String>,
}

/// Clover's REST API (`/v3`), as the till uses it, with its own sign-in.
/// Every request names the till ([`CloverConfig::user_agent`]), which
/// Clover requires. A `401` is met with a fresh token, then means signing
/// in again; a `429` says how long to wait.
#[derive(Clone)]
pub struct CloverApi {
    http: reqwest::Client,
    config: CloverConfig,
    auth: Arc<CloverAuth>,
}

impl CloverApi {
    /// The system tender a REST-made one must take the key of
    /// (decision 129 in docs/decisions/android.md).
    pub const EXTERNAL_PAYMENT: &'static str = "com.clover.tender.external_payment";

    /// Payments read a page at a time.
    const PAGE: usize = 100;

    /// The most pages one look reads: 2,000 payments of one tender since
    /// the last look.
    const MAX_PAGES: usize = 20;

    pub fn new(http: reqwest::Client, config: CloverConfig, auth: Arc<CloverAuth>) -> CloverApi {
        CloverApi { http, config, auth }
    }

    /// The merchant, with its address, then its properties. An answer for
    /// another merchant means signing in again; one with no currency is
    /// refused.
    pub async fn merchant(&self, merchant_id: &str) -> Result<CloverMerchant, PosError> {
        let merchant: Merchant = self
            .get(&self.url(merchant_id, &[], &[("expand", "address")]))
            .await?;
        if merchant.id != merchant_id {
            return Err(PosError::sign_in(
                PointOfSale::Clover,
                0,
                "Clover answered for another merchant. Sign in again.",
            ));
        }
        let properties: Properties = self
            .get(&self.url(merchant_id, &["properties"], &[]))
            .await?;
        let currency = properties
            .default_currency
            .filter(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase()))
            .ok_or_else(|| {
                PosError::answered(
                    PointOfSale::Clover,
                    0,
                    "Clover didn't say the merchant's currency.",
                )
            })?;
        Ok(CloverMerchant {
            id: merchant.id,
            name: merchant
                .name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "Clover merchant".into()),
            currency,
            timezone: properties.timezone,
            country: CloverApi::country(
                merchant.address.and_then(|a| a.country).as_deref(),
                properties.locale.as_deref(),
            ),
        })
    }

    /// The merchant's tenders.
    pub async fn tenders(&self, merchant_id: &str) -> Result<Vec<CloverTender>, PosError> {
        let url = self.url(merchant_id, &["tenders"], &[("limit", "1000")]);
        Ok(self.get::<Elements<CloverTender>>(&url).await?.elements)
    }

    /// Adds a tender labelled `label`. One made over REST must take a
    /// system tender's `labelKey`: the external payment's (decision 129).
    /// It opens no cash drawer and takes no tips. Needs Merchant write.
    pub async fn create_tender(
        &self,
        merchant_id: &str,
        label: &str,
    ) -> Result<CloverTender, PosError> {
        let body = json!({
            "label": label,
            "labelKey": CloverApi::EXTERNAL_PAYMENT,
            "enabled": true,
            "visible": true,
            "opensCashDrawer": false,
            "supportsTipping": false,
        });
        let url = self.url(merchant_id, &["tenders"], &[]);
        let text = self
            .send(|token| self.request(self.http.post(&url), token).json(&body))
            .await?;
        http::read(PointOfSale::Clover, &text)
    }

    /// The merchant's payments with `tender_id` made at or after `since_ms`
    /// (Unix ms), oldest first, with their tender, order and refunds: page
    /// after page until a short one, at most
    /// [`MAX_PAGES`](Self::MAX_PAGES).
    pub async fn payments(
        &self,
        merchant_id: &str,
        tender_id: &str,
        since_ms: i64,
    ) -> Result<Vec<CloverPayment>, PosError> {
        let mut all = Vec::new();
        let tender = format!("tender.id={tender_id}");
        let since = format!("createdTime>={since_ms}");
        let limit = Self::PAGE.to_string();
        for page in 0..Self::MAX_PAGES {
            let offset = (page * Self::PAGE).to_string();
            let url = self.url(
                merchant_id,
                &["payments"],
                &[
                    ("filter", &tender),
                    ("filter", &since),
                    ("expand", "tender,order,refunds"),
                    ("orderBy", "createdTime ASC"),
                    ("limit", &limit),
                    ("offset", &offset),
                ],
            );
            let elements = self.get::<Elements<CloverPayment>>(&url).await?.elements;
            let short = elements.len() < Self::PAGE;
            all.extend(elements);
            if short {
                break;
            }
        }
        Ok(all)
    }

    /// One payment, with its tender, order and refunds.
    pub async fn payment(
        &self,
        merchant_id: &str,
        payment_id: &str,
    ) -> Result<CloverPayment, PosError> {
        let url = self.url(
            merchant_id,
            &["payments", payment_id],
            &[("expand", "tender,order,refunds")],
        );
        self.get(&url).await
    }

    /// The merchant's devices, for their names. Clover's devices endpoint
    /// needs Merchant read, which the till already asks for (the merchant's
    /// name and currency come from it).
    pub async fn devices(&self, merchant_id: &str) -> Result<Vec<CloverDevice>, PosError> {
        let url = self.url(merchant_id, &["devices"], &[("limit", "1000")]);
        Ok(self.get::<Elements<CloverDevice>>(&url).await?.elements)
    }

    /// One order, with its line items.
    pub async fn order(&self, merchant_id: &str, order_id: &str) -> Result<CloverOrder, PosError> {
        let url = self.url(
            merchant_id,
            &["orders", order_id],
            &[("expand", "lineItems")],
        );
        self.get(&url).await
    }

    /// The merchant's country: the address's, else the locale's ("en_US"),
    /// when either is a two-letter country code.
    pub fn country(address: Option<&str>, locale: Option<&str>) -> Option<String> {
        let code =
            |s: String| (s.len() == 2 && s.bytes().all(|b| b.is_ascii_uppercase())).then_some(s);
        address
            .map(|a| a.trim().to_uppercase())
            .and_then(code)
            .or_else(|| {
                let locale = locale?.replace('-', "_");
                let (_, region) = locale.split_once('_')?;
                code(region.chars().take(2).collect::<String>().to_uppercase())
            })
    }

    fn url(&self, merchant_id: &str, path: &[&str], query: &[(&str, &str)]) -> String {
        let mut segments = vec!["v3", "merchants", merchant_id];
        segments.extend_from_slice(path);
        http::url(self.config.api_base(), &segments, query)
    }

    fn request(&self, request: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
        request
            .bearer_auth(token)
            .header(reqwest::header::USER_AGENT, &self.config.user_agent)
            .header(reqwest::header::ACCEPT, "application/json")
    }

    async fn get<T: DeserializeOwned>(&self, url: &str) -> Result<T, PosError> {
        let text = self
            .send(|token| self.request(self.http.get(url), token))
            .await?;
        http::read(PointOfSale::Clover, &text)
    }

    /// One request with the sign-in's access token, `build` making it for a
    /// token. A `401` before the token's time is up (Clover ended it early)
    /// is answered by refreshing it and asking once more. A fresh token
    /// refused too can't be helped by another: the till signs out, so no
    /// pass after spends a refresh token (decision 139).
    async fn send(
        &self,
        build: impl Fn(&str) -> reqwest::RequestBuilder,
    ) -> Result<String, PosError> {
        let token = self.auth.access_token().await?;
        match self.send_once(build(&token)).await {
            Err(e) if e.status() == Some(401) => {
                let fresh = self.auth.refresh_after_refusal(&token).await?;
                match self.send_once(build(&fresh)).await {
                    Err(again) if again.status() == Some(401) => {
                        self.auth.sign_out();
                        Err(again)
                    }
                    answered => answered,
                }
            }
            answered => answered,
        }
    }

    async fn send_once(&self, request: reqwest::RequestBuilder) -> Result<String, PosError> {
        let answer = http::send(request).await?;
        match answer.status {
            _ if answer.ok() => Ok(answer.body),
            401 => Err(PosError::sign_in(
                PointOfSale::Clover,
                401,
                "Clover's sign-in has ended. Sign in again.",
            )),
            429 => Err(PosError::Answered {
                pos: PointOfSale::Clover,
                status: 429,
                message: "Clover asked the till to slow down.".into(),
                sign_in_again: false,
                retry_after_seconds: Some(retry_after(answer.retry_after.as_deref()).unwrap_or(1)),
            }),
            status => Err(PosError::answered(
                PointOfSale::Clover,
                status,
                clover_error(&answer.body).unwrap_or_else(|| format!("Clover said {status}.")),
            )),
        }
    }
}
