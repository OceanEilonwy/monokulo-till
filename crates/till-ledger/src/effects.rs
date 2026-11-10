//! Effects (docs/ledger.md §6): what a change in one sale's view asks this
//! till to do. Settings, whether the app is in front and which sale the
//! cashier has open feed this, never the merge.

use serde::{Deserialize, Serialize};

use crate::facts::AlertKind;
use crate::ids::SaleKey;
use crate::model::PaymentPhase;
use crate::view::{SaleView, ViewSettings};

/// One effect: an alert of `kind` for `sale`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Effect {
    pub sale: SaleKey,
    pub kind: AlertKind,
}

impl Effect {
    /// The notification's id: `sale_key + kind`, so a repeat delivery
    /// replaces rather than adds.
    pub fn notification_id(&self) -> String {
        notification_id(&self.sale, self.kind)
    }
}

pub fn notification_id(sale: &SaleKey, kind: AlertKind) -> String {
    format!("{}#{}", sale.text(), kind.wire())
}

/// What else `diff` reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiffContext<'a> {
    pub settings: ViewSettings,
    /// The app is in front (its `Foreground` heartbeat hasn't run out).
    pub foreground: bool,
    /// The sale the cashier has open on screen, if any: it gets the paid
    /// moment even when it's another counter's.
    pub open_sale: Option<&'a SaleKey>,
}

fn has_order(view: &SaleView) -> bool {
    view.has_order()
}

/// The effects of one sale's view going from `old` (none: the sale is new
/// here) to `new`.
///
/// - **New sale**: the sale gets its first order while live, on a sale
///   whose alerts are this till's; a cash sale only with cash-as-Monero on
///   (taken in Monero by the cashier, it's on screen already).
/// - **Paid**: newly paid, this till's (or open on screen): the paid moment
///   in front, a notice behind (the open sale only in front).
/// - **Ended**: newly expired, cancelled or reversed while live, this
///   till's.
pub fn diff(old: Option<&SaleView>, new: &SaleView, cx: &DiffContext<'_>) -> Vec<Effect> {
    let mut effects = Vec::new();
    let effect = |kind| Effect {
        sale: new.sale.clone(),
        kind,
    };
    let had_order = old.is_some_and(has_order);
    if !had_order
        && has_order(new)
        && new.live
        && new.alerts_here
        && (!new.cash || cx.settings.cash_as_monero)
    {
        effects.push(effect(AlertKind::NewSale));
    }
    let was_paid = old.is_some_and(|o| o.payment.is_some_and(PaymentPhase::is_paid));
    let paid = new.payment.is_some_and(PaymentPhase::is_paid);
    if paid && !was_paid {
        let open = cx.open_sale == Some(&new.sale);
        if cx.foreground && (new.alerts_here || open) {
            effects.push(effect(AlertKind::PaidMoment));
        } else if !cx.foreground && new.alerts_here {
            effects.push(effect(AlertKind::Paid));
        }
    }
    let ended = |v: &SaleView| {
        matches!(
            v.payment,
            Some(PaymentPhase::Expired | PaymentPhase::Cancelled | PaymentPhase::DoubleSpend)
        )
    };
    if ended(new) && old.is_some_and(|o| o.live) && new.alerts_here {
        effects.push(effect(AlertKind::Ended));
    }
    effects
}
