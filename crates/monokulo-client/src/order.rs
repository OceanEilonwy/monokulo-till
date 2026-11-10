//! An order, as Monokulo's API answers it (Android: `core/Order.kt`).

use serde::{Deserialize, Serialize};

use crate::payment_uri;
use crate::sale::SalePhase;

/// An order, as the API answers it: made with the `create-order` form, or
/// read at its `self` link. Fields it doesn't know are ignored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub order_id: String,
    /// `pending`, `unconfirmed`, `confirming`, `paid`, `partial`, `overpaid`,
    /// `expired` or `cancelled`.
    #[serde(default = "pending")]
    pub status: String,
    #[serde(default)]
    pub is_final: bool,
    /// The subaddress to pay: this order's alone.
    pub address: String,
    #[serde(rename = "xmr_amount_piconero")]
    pub piconero: u64,
    /// The price it was made with; `None` for an order Monokulo has no
    /// record of.
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub merchant_order_id: Option<String>,
    pub expires_at: i64,
    /// Confirmations its payment has; `None` from a Monokulo that doesn't say.
    #[serde(default)]
    pub confirmations: Option<u32>,
    /// Confirmations it needs to count as paid: the store's threshold for its
    /// amount. `None` from a Monokulo that doesn't say.
    #[serde(default)]
    pub confirmations_required: Option<u32>,
    /// Its `self` link, absolute: where to read it again.
    #[serde(skip)]
    pub url: String,
    /// Where its `renew-order` form is sent, absolute: a new price and
    /// expiry for the same order once it expired unpaid (New code).
    /// `None` from a Monokulo that can't renew an order, or for a key that
    /// may not.
    #[serde(skip)]
    pub renew: Option<String>,
}

fn pending() -> String {
    "pending".to_owned()
}

impl Order {
    /// The XMR to pay, as the code and the screens write it.
    pub fn xmr(&self) -> String {
        payment_uri::xmr(self.piconero)
    }

    /// The plain payment URI, which the receipt is appended to.
    pub fn payment_uri(&self) -> String {
        payment_uri::build(&self.address, self.piconero)
    }

    /// The phase it puts a sale in.
    pub fn phase(&self) -> SalePhase {
        Order::phase_of(&self.status)
    }

    /// The phase an order's `status` puts a sale in. A status this build
    /// doesn't know is still waiting.
    pub fn phase_of(status: &str) -> SalePhase {
        match status {
            "unconfirmed" => SalePhase::Seen,
            "confirming" => SalePhase::Confirming,
            "paid" | "overpaid" => SalePhase::Paid,
            "partial" => SalePhase::Partial,
            "expired" => SalePhase::Expired,
            "cancelled" => SalePhase::Cancelled,
            _ => SalePhase::Waiting,
        }
    }
}
