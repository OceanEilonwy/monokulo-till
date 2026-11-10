//! Where a sale stands on this till (Android: `Standing.kt`).

use monokulo_client::SalePhase;

use crate::TillSale;

/// Where a sale stands on this till (the design's shift rows and detail
/// pages): what Monokulo says about its payment, together with what the
/// cashier did here (closed it for now, refunded it in the POS). A row's
/// words, its icon and which page it opens all follow from it, so nothing
/// else reads the phase and the cashier's marks side by side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Standing {
    /// Its code is up: waiting, seen, confirming or part paid, and not
    /// closed.
    Live(SalePhase),
    /// Paid, and more than asked (or twice): the extra is owed back to the
    /// customer until the cashier says it went (core.md decision 168).
    Overpaid,
    /// Paid: `late` after the cashier closed it or its code ran out (which
    /// one, `closed_at` and `ended_at` say), `after_refund` after it was
    /// refunded in the POS, so the cashier has to ring it up again or
    /// return it.
    Paid { late: bool, after_refund: bool },
    /// Closed for now, not paid: Monokulo still watches it for a late
    /// payment.
    Closed,
    /// Its code ran out (or the order was cancelled) with no payment.
    Expired,
    /// The payment was spent twice on the network, so it doesn't count.
    Reversed,
    /// Refunded in the POS, nothing paid.
    Refunded,
    /// The cashier was told to refund it in the POS
    /// ([`till_pos::Refunds::InPos`]), and the POS doesn't show it refunded
    /// yet: nothing paid.
    RefundInPos,
    /// Another till found it and it ended, or was closed, unpaid: that till
    /// has the POS sale.
    OtherTill,
    /// A cash sale offered in Monero, with no order yet: `kept` once the
    /// cashier chose "Keep as cash" (core.md decision 116).
    CashOffer { kept: bool },
    /// A cash sale whose customer paid in real cash after all: closed, with
    /// nothing to refund.
    PaidInCash,
}

impl Standing {
    /// Paid on time and never refunded.
    pub const PAID: Standing = Standing::Paid {
        late: false,
        after_refund: false,
    };
}

impl TillSale {
    /// Where the sale stands. Payment outranks everything; then a cash
    /// sale offered in Monero or paid in real cash (core.md decision 116);
    /// then a code that is up; then, for a sale this till found, a refund, a refund to
    /// make in the POS, a reversal and an ending, before a plain close
    /// (decisions 60 onwards in docs/decisions/android.md).
    pub fn standing(&self) -> Standing {
        if self.phase == SalePhase::Paid
            && self.extras.overpaid
            && self.extras.extra_refunded_at.is_none()
        {
            Standing::Overpaid
        } else if self.phase == SalePhase::Paid {
            Standing::Paid {
                late: self.closed_at.is_some() || self.ended_at.is_some(),
                after_refund: self.refunded(),
            }
        } else if self.offered {
            Standing::CashOffer {
                kept: self.kept_as_cash_at.is_some(),
            }
        } else if self.paid_in_cash_at.is_some() {
            Standing::PaidInCash
        } else if !self.phase.is_final() && self.closed_at.is_none() {
            Standing::Live(self.phase)
        } else if self.found.is_none() {
            Standing::OtherTill
        } else if self.refunded() {
            Standing::Refunded
        } else if self.refund_pending_at.is_some() {
            Standing::RefundInPos
        } else if self.phase == SalePhase::DoubleSpend {
            Standing::Reversed
        } else if self.phase.is_unpaid() {
            Standing::Expired
        } else {
            Standing::Closed
        }
    }

    /// Its code is up: what the bubble lights for, the notification follows
    /// and the shift opens as a code.
    pub fn live(&self) -> bool {
        matches!(self.standing(), Standing::Live(_))
    }

    /// Needs the cashier: closed, expired or reversed and this till can
    /// refund it in the POS; told to refund it there and not yet seen
    /// refunded; or paid after it was refunded, so the sale must be rung up
    /// again or the payment returned.
    pub fn needs_action(&self) -> bool {
        match self.standing() {
            Standing::Closed
            | Standing::Expired
            | Standing::Reversed
            | Standing::RefundInPos
            | Standing::Overpaid => true,
            Standing::Paid { after_refund, .. } => after_refund,
            _ => false,
        }
    }
}
