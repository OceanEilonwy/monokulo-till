//! An order, as Monokulo's API answers it (Android: `core/Order.kt`).

use serde::{Deserialize, Serialize};

use crate::payment_uri;

till_types::text_id!(
    /// Monokulo's id for an order: what create-order answers, and what a
    /// sale event names. Every crate's order id is this one type.
    OrderId
);
use crate::sale::SalePhase;

/// An order, as the API answers it: made with the `create-order` form, or
/// read at its `self` link. Fields it doesn't know are ignored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub order_id: OrderId,
    #[serde(default = "pending")]
    pub status: OrderStatus,
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
    /// What has arrived so far, in piconero.
    #[serde(rename = "amount_received_piconero")]
    pub received: u64,
    /// Confirmations its payment has.
    pub confirmations: u32,
    /// Confirmations it needs to count as paid: the store's threshold for its
    /// amount, as the order was made with it.
    pub confirmations_required: u32,
    /// The device that made it, for an order one of the store's apps made.
    #[serde(default)]
    pub device_name: Option<String>,
    /// When a payment to it was found spent twice (Unix seconds), if one
    /// was.
    #[serde(default)]
    pub double_spend_detected_at: Option<i64>,
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

fn pending() -> OrderStatus {
    OrderStatus::Pending
}

till_types::wire_enum!(
    /// An order's `status`, as Monokulo writes it.
    OrderStatus {
        Pending = "pending",
        Unconfirmed = "unconfirmed",
        Confirming = "confirming",
        Paid = "paid",
        Overpaid = "overpaid",
        Partial = "partial",
        Expired = "expired",
        Cancelled = "cancelled",
    }
);

impl OrderStatus {
    /// The phase it puts a sale in. A status this build doesn't know
    /// ([`Other`](OrderStatus::Other)) leaves the sale waiting, as
    /// `pending` does: the order's code still takes a payment, and the
    /// next status Monokulo sends moves it on.
    pub fn phase(&self) -> SalePhase {
        match self {
            OrderStatus::Unconfirmed => SalePhase::Seen,
            OrderStatus::Confirming => SalePhase::Confirming,
            OrderStatus::Paid | OrderStatus::Overpaid => SalePhase::Paid,
            OrderStatus::Partial => SalePhase::Partial,
            OrderStatus::Expired => SalePhase::Expired,
            OrderStatus::Cancelled => SalePhase::Cancelled,
            OrderStatus::Pending | OrderStatus::Other(_) => SalePhase::Waiting,
        }
    }

    /// Paid more than asked.
    pub fn overpaid(&self) -> bool {
        *self == OrderStatus::Overpaid
    }
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

    /// The phase it puts a sale in: a payment spent twice reverses it
    /// until it's paid after all (a double spend stays once seen, as the
    /// ledger keeps it).
    pub fn phase(&self) -> SalePhase {
        match self.status {
            OrderStatus::Paid | OrderStatus::Overpaid => SalePhase::Paid,
            _ if self.double_spend_detected_at.is_some() => SalePhase::DoubleSpend,
            _ => self.status.phase(),
        }
    }

    /// The phase an order's `status` text puts a sale in
    /// ([`OrderStatus::phase`]).
    pub fn phase_of(status: &str) -> SalePhase {
        OrderStatus::from(status).phase()
    }
}
