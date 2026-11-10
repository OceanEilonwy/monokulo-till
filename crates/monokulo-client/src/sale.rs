//! Where a sale's payment is, and the store's `sale.*` events (Android:
//! `core/Sale.kt`).

use reqwest::Url;
use serde::{Deserialize, Serialize};

/// Where a sale's payment is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SalePhase {
    Waiting,
    Seen,
    Confirming,
    Paid,
    Partial,
    Expired,
    DoubleSpend,
    /// Cancelled at the store's POS. Only the POS cancels, and only its own
    /// orders, so an app's never is; but one read back still shows as ended
    /// unpaid, like [`SalePhase::Expired`].
    Cancelled,
}

impl SalePhase {
    /// Every phase, in the order they are declared.
    pub const ALL: [SalePhase; 8] = [
        SalePhase::Waiting,
        SalePhase::Seen,
        SalePhase::Confirming,
        SalePhase::Paid,
        SalePhase::Partial,
        SalePhase::Expired,
        SalePhase::DoubleSpend,
        SalePhase::Cancelled,
    ];

    /// Nothing more will happen to it.
    pub fn is_final(self) -> bool {
        self == SalePhase::Paid || self.is_unpaid()
    }

    /// Nothing of the money has been seen yet (waiting, or part paid and
    /// waiting for the rest), so the code's time left is what matters: the
    /// countdown shows beside the code.
    pub fn counts_down(self) -> bool {
        matches!(self, SalePhase::Waiting | SalePhase::Partial)
    }

    /// It ended without being paid: expired, cancelled or reversed.
    pub fn is_unpaid(self) -> bool {
        matches!(
            self,
            SalePhase::Expired | SalePhase::Cancelled | SalePhase::DoubleSpend
        )
    }

    /// The phase a `sale.*` event name puts a sale in, or `None` for another
    /// event. Monokulo never sends `sale.cancelled`.
    pub fn of_event(name: &str) -> Option<SalePhase> {
        Some(match name {
            "sale.waiting" => SalePhase::Waiting,
            "sale.seen" => SalePhase::Seen,
            "sale.confirming" => SalePhase::Confirming,
            "sale.paid" => SalePhase::Paid,
            "sale.partial" => SalePhase::Partial,
            "sale.expired" => SalePhase::Expired,
            "sale.double_spend" => SalePhase::DoubleSpend,
            _ => return None,
        })
    }
}

/// One `sale.*` event from the store's event stream (`GET /api/v1/events`):
/// an order a connected app made, and what just happened to it. Never
/// anything about what was bought. It links its order (`mk:order`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleEvent {
    pub event: String,
    pub order_id: crate::order::OrderId,
    pub amount: String,
    pub currency: String,
    pub address: String,
    #[serde(rename = "xmr_amount_piconero")]
    pub piconero: u64,
    #[serde(default)]
    pub merchant_order_id: Option<String>,
    pub expires_at: i64,
    #[serde(default)]
    pub device_name: String,
    pub at: i64,
    /// Its order (`mk:order`), absolute: where to read it again.
    #[serde(skip)]
    pub order_url: Option<String>,
}

impl SaleEvent {
    /// The phase the event puts its sale in, or `None` for a name this build
    /// doesn't know.
    pub fn phase(&self) -> Option<SalePhase> {
        SalePhase::of_event(&self.event)
    }

    /// The event in `text`, ignoring fields it doesn't know, with its
    /// `mk:order` link resolved against `stream`, the URL it came from.
    pub fn decode(text: &str, stream: &Url) -> Result<SaleEvent, serde_json::Error> {
        let body: serde_json::Value = serde_json::from_str(text)?;
        let href = body
            .pointer("/_links/mk:order/href")
            .and_then(serde_json::Value::as_str)
            .and_then(|href| stream.join(href).ok());
        let mut event: SaleEvent = serde_json::from_value(body)?;
        event.order_url = href.map(String::from);
        Ok(event)
    }
}
