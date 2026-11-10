//! Square's API, as the till uses it (Android: `SquareApi.kt`).

use std::sync::Arc;

use monokulo_client::PointOfSale;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{SquareAuth, SquareConfig, answered};
use crate::http;
use crate::receipts::one_line;
use crate::{PaymentStatus, PosError, RefundStatus};

/// An amount in a currency's minor unit, as Square writes it. The
/// currency stays Square's text here: one that isn't a code is dropped
/// where a sale is made of it, never failing the page it's on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount: i64,
    pub currency: String,
}

impl Money {
    /// `money` as Square writes it.
    pub fn of(money: till_types::Money) -> Money {
        Money {
            amount: money.minor,
            currency: money.currency.to_string(),
        }
    }
}

/// The parts of a Square payment the till reads.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SquarePayment {
    pub id: String,
    pub created_at: String,
    /// When Square last changed the payment (a refund, a status change):
    /// RFC 3339.
    #[serde(default)]
    pub updated_at: Option<String>,
    pub status: PaymentStatus,
    #[serde(default)]
    pub source_type: Option<SourceType>,
    pub amount_money: Money,
    #[serde(default)]
    pub total_money: Option<Money>,
    #[serde(default)]
    pub location_id: Option<String>,
    #[serde(default)]
    pub order_id: Option<String>,
    #[serde(default)]
    pub receipt_number: Option<String>,
    #[serde(default)]
    pub external_details: Option<External>,
    #[serde(default)]
    pub device_details: Option<Device>,
}

till_types::wire_enum!(
    /// What a Square payment was tendered with (its `source_type`).
    SourceType {
        Card = "CARD",
        BankAccount = "BANK_ACCOUNT",
        Wallet = "WALLET",
        BuyNowPayLater = "BUY_NOW_PAY_LATER",
        SquareAccount = "SQUARE_ACCOUNT",
        Cash = "CASH",
        /// An Other tender: the Monero one, among others.
        External = "EXTERNAL",
    }
);

till_types::wire_enum!(
    /// A Square location's status.
    LocationStatus {
        Active = "ACTIVE",
        Inactive = "INACTIVE",
    }
);

/// A payment's `external_details`: an Other tender's.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct External {
    #[serde(rename = "type")]
    pub kind: String,
    /// The tender's name ("Monero").
    pub source: String,
}

/// A payment's `device_details`: the Square device that took it, and the
/// name the seller gave that device in Square ("Front iPad").
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Device {
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub device_name: Option<String>,
}

/// The parts of a Square order a receipt is made from.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SquareOrder {
    pub id: String,
    pub location_id: String,
    pub created_at: String,
    #[serde(default)]
    pub line_items: Vec<LineItem>,
    #[serde(default)]
    pub taxes: Vec<OrderTax>,
    pub total_money: Money,
    #[serde(default)]
    pub total_tip_money: Option<Money>,
    #[serde(default)]
    pub total_discount_money: Option<Money>,
    /// The order's pickups, deliveries and shipments, kept as plain JSON:
    /// only the name of whom each is for is read
    /// ([`customer_name`](Self::customer_name)), and a shape Square changes
    /// there never costs the sale its receipt.
    #[serde(default)]
    pub fulfillments: Option<serde_json::Value>,
    /// The name typed for the order at the counter (Square's "ticket
    /// name"): "Sam", "Table 4".
    #[serde(default)]
    pub ticket_name: Option<String>,
}

/// One line of a Square order.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct LineItem {
    #[serde(default)]
    pub name: Option<String>,
    /// A decimal: "2", "1.5".
    pub quantity: String,
    #[serde(default)]
    pub variation_name: Option<String>,
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
    #[serde(default)]
    pub base_price_money: Option<Money>,
    #[serde(default)]
    pub gross_sales_money: Option<Money>,
}

/// A line's modifier ("Oat milk").
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Modifier {
    #[serde(default)]
    pub name: Option<String>,
}

/// A tax on a Square order.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct OrderTax {
    #[serde(default)]
    pub name: Option<String>,
    /// A percentage, as a decimal string: "8.875".
    #[serde(default)]
    pub percentage: Option<String>,
    #[serde(default)]
    pub applied_money: Option<Money>,
}

impl SquareOrder {
    /// Where a fulfillment names whom it's for, by its type.
    const RECIPIENT_DETAILS: [&'static str; 3] =
        ["pickup_details", "delivery_details", "shipment_details"];

    /// The most of a name kept, in characters: a title shows far less, and
    /// a shift keeps it for every sale.
    const MAX_NAME: usize = 80;

    /// Who the order is for: the recipient the seller named on a pickup,
    /// delivery or shipment ("Ana López"), else the name typed for the
    /// order at the counter (its `ticket_name`, "Sam"), on one line and
    /// never blank (core.md decision 112). `None` when neither names
    /// anyone; an order with only a `customer_id` would need Square's
    /// Customers API, which the till's scopes don't reach.
    pub fn customer_name(&self) -> Option<String> {
        self.recipient().or_else(|| {
            self.ticket_name
                .as_deref()
                .map(Self::kept)
                .filter(|name| !name.is_empty())
        })
    }

    /// `name` on one line, cut to [`Self::MAX_NAME`] characters.
    fn kept(name: &str) -> String {
        one_line(name).chars().take(Self::MAX_NAME).collect()
    }

    /// The first fulfillment recipient's `display_name` that isn't blank.
    fn recipient(&self) -> Option<String> {
        let fulfillments = self.fulfillments.as_ref()?.as_array()?;
        fulfillments
            .iter()
            .filter_map(|f| f.as_object())
            .flat_map(|f| {
                Self::RECIPIENT_DETAILS
                    .iter()
                    .filter_map(|key| f.get(*key)?.as_object())
            })
            .filter_map(|details| details.get("recipient")?.get("display_name")?.as_str())
            .map(Self::kept)
            .find(|name| !name.is_empty())
    }
}

/// A Square location: where the till takes payments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SquareLocation {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_name: Option<String>,
    /// ISO 3166 alpha-2.
    pub country: String,
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<LocationStatus>,
}

/// A refund of a payment, as Square lists it (`GET /v2/refunds`) or
/// answers a refund with.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SquareRefund {
    pub id: String,
    pub status: RefundStatus,
    pub amount_money: Money,
    pub payment_id: String,
    /// RFC 3339; absent from some answers.
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub location_id: Option<String>,
}

#[derive(Deserialize)]
struct RefundPage {
    #[serde(default)]
    refunds: Vec<SquareRefund>,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct RefundResult {
    #[serde(default)]
    refund: Option<SquareRefund>,
}

#[derive(Deserialize)]
struct PaymentPage {
    #[serde(default)]
    payments: Vec<SquarePayment>,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct OrderResult {
    order: SquareOrder,
}

#[derive(Deserialize)]
struct Locations {
    #[serde(default)]
    locations: Vec<SquareLocation>,
}

/// Square's API, as the till uses it, with its own sign-in. Every request
/// asks for [`SquareConfig::API_VERSION`]; a `401` means signing in again.
#[derive(Clone)]
pub struct SquareApi {
    http: reqwest::Client,
    config: SquareConfig,
    auth: Arc<SquareAuth>,
}

impl SquareApi {
    pub fn new(http: reqwest::Client, config: SquareConfig, auth: Arc<SquareAuth>) -> SquareApi {
        SquareApi { http, config, auth }
    }

    /// Payments at `location_id` made at or after `begin_time` (RFC 3339),
    /// oldest first: page after page until Square gives no cursor.
    pub async fn payments(
        &self,
        location_id: &str,
        begin_time: &str,
    ) -> Result<Vec<SquarePayment>, PosError> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut query = vec![
                ("location_id", location_id),
                ("begin_time", begin_time),
                ("sort_order", "ASC"),
                ("limit", "100"),
            ];
            if let Some(cursor) = &cursor {
                query.push(("cursor", cursor));
            }
            let url = http::url(self.config.connect_base(), &["v2", "payments"], &query);
            let page: PaymentPage = self.get(&url).await?;
            all.extend(page.payments);
            match page.cursor {
                Some(next) => cursor = Some(next),
                None => return Ok(all),
            }
        }
    }

    /// Refunds at `location_id` made at or after `begin_time` (RFC 3339),
    /// oldest first, whatever took them (the till, Square's own app, the
    /// Dashboard): page after page until Square gives no cursor. Reading
    /// them needs only `PAYMENTS_READ`.
    pub async fn refunds(
        &self,
        location_id: &str,
        begin_time: &str,
    ) -> Result<Vec<SquareRefund>, PosError> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut query = vec![
                ("location_id", location_id),
                ("begin_time", begin_time),
                ("sort_order", "ASC"),
                ("limit", "100"),
            ];
            if let Some(cursor) = &cursor {
                query.push(("cursor", cursor));
            }
            let url = http::url(self.config.connect_base(), &["v2", "refunds"], &query);
            let page: RefundPage = self.get(&url).await?;
            all.extend(page.refunds);
            match page.cursor {
                Some(next) => cursor = Some(next),
                None => return Ok(all),
            }
        }
    }

    /// One order.
    pub async fn order(&self, order_id: &str) -> Result<SquareOrder, PosError> {
        let url = http::url(self.config.connect_base(), &["v2", "orders", order_id], &[]);
        Ok(self.get::<OrderResult>(&url).await?.order)
    }

    /// The seller's locations that aren't inactive.
    pub async fn locations(&self) -> Result<Vec<SquareLocation>, PosError> {
        let url = http::url(self.config.connect_base(), &["v2", "locations"], &[]);
        let mut locations = self.get::<Locations>(&url).await?.locations;
        locations.retain(|l| l.status != Some(LocationStatus::Inactive));
        Ok(locations)
    }

    /// Refunds a payment in full. `idempotency_key` makes a retry refund it
    /// once. The refund as Square answered with it, when it did.
    pub async fn refund(
        &self,
        payment_id: &str,
        amount: &Money,
        idempotency_key: &str,
        reason: &str,
    ) -> Result<Option<SquareRefund>, PosError> {
        let url = http::url(self.config.connect_base(), &["v2", "refunds"], &[]);
        let body = json!({
            "idempotency_key": idempotency_key,
            "payment_id": payment_id,
            "amount_money": {"amount": amount.amount, "currency": amount.currency},
            "reason": reason,
        });
        let answer = self.send(self.http.post(url).json(&body)).await?;
        // The refund is made whatever its answer says: one Square words
        // differently only costs the till the refund's own id.
        Ok(serde_json::from_str::<RefundResult>(&answer)
            .ok()
            .and_then(|r| r.refund))
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, PosError> {
        let body = self.send(self.http.get(url)).await?;
        http::read(PointOfSale::Square, &body)
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<String, PosError> {
        let token = self.auth.access_token().await?;
        let request = request
            .bearer_auth(token)
            .header("Square-Version", SquareConfig::API_VERSION);
        let answer = http::send(request).await?;
        if answer.ok() {
            return Ok(answer.body);
        }
        match answered(answer.status, &answer.body) {
            PosError::Answered {
                pos,
                status,
                message,
                ..
            } if status == 401 => Err(PosError::sign_in(pos, status, message)),
            e => Err(e),
        }
    }
}
