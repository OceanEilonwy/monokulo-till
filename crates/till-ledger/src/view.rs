//! The view (docs/ledger.md §5): where the shared state, this device's
//! marks and its settings combine into what a screen shows. Its fields are
//! named as `till_sales::TillSale`'s, so mapping one to the other is a
//! field-for-field copy (times in Unix seconds, as `TillSale` keeps them).

use std::collections::BTreeMap;

use monokulo_client::SalePhase;
use orf_codec::Receipt;
use serde::{Deserialize, Serialize};
use till_pos::FoundSale;

use crate::UnixMillis;
use crate::facts::{Outcome, RefundStatus, Tender};
use crate::ids::{DeviceId, OrderId, SaleKey};
use crate::model::{LocalMarks, PaymentPhase, SharedSale};

/// What an offered cash sale's `order_id` starts with, before the
/// payment's id (`TillSale::OFFERED`).
pub const OFFERED: &str = "cash:";

/// The "why" of a sale's plain code on a till with no POS sign-in.
pub const WHY_NO_POS: &str =
    "This till isn't signed in to the point of sale, so it can't add the receipt.";

/// Setting names the ledger reads (`Fact::SettingChanged`).
pub mod setting {
    /// This counter's POS device id; absent for every device's sales.
    pub const DEVICE: &str = "counter.device";
    /// "true" to alert for every device's sales.
    pub const EVERY_DEVICE: &str = "counter.every";
    /// "false" to offer cash sales rather than make their orders.
    pub const CASH_AS_MONERO: &str = "cash.monero";
    /// Days of journal and sales kept (§12); 30 when absent.
    pub const RETENTION_DAYS: &str = "ledger.retention_days";
    /// Minutes after midnight, local, when the trading day starts; 240.
    pub const TRADING_DAY_START: &str = "shift.day_start";
    /// "false" to list only this counter's sales.
    pub const COUNTER_OTHERS: &str = "counter.others";
    /// How a new sale shows up: `ShowSales::wire`'s `pip` or `side`;
    /// absent for an alert.
    pub const SHOW_SALES: &str = "sales.show";
}

/// The settings that feed views and effects, never the merge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewSettings {
    /// This counter's device, when one was chosen.
    pub device: Option<DeviceId>,
    /// Alert for every device's sales, not just this counter's.
    pub every_device: bool,
    /// A cash sale gets its order as it's found; else it's offered.
    pub cash_as_monero: bool,
}

impl Default for ViewSettings {
    fn default() -> ViewSettings {
        ViewSettings {
            device: None,
            every_device: false,
            cash_as_monero: true,
        }
    }
}

impl ViewSettings {
    /// The settings in the `settings` table's rows.
    pub fn from_table(rows: &BTreeMap<String, String>) -> ViewSettings {
        ViewSettings {
            device: rows
                .get(setting::DEVICE)
                .filter(|d| !d.is_empty())
                .map(|d| DeviceId::new(d.as_str())),
            every_device: rows.get(setting::EVERY_DEVICE).is_some_and(|v| v == "true"),
            cash_as_monero: rows
                .get(setting::CASH_AS_MONERO)
                .is_none_or(|v| v != "false"),
        }
    }
}

/// The code to show: the payment URI, with the receipt when it could be
/// added (`till_sales::PaymentCode`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Code {
    pub uri: String,
    pub has_receipt: bool,
    /// Why there's no receipt in it, when there isn't and one was wanted.
    pub why: Option<String>,
}

/// Everything a screen needs about one sale.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SaleView {
    pub sale: SaleKey,
    /// Its order's id; `cash:<payment>` for a cash sale
    /// with no order; the sale key for a sale still waiting for its order.
    pub order_id: OrderId,
    /// The deciding order's price, else the POS payment's.
    #[serde(flatten)]
    pub price: till_types::Price,
    pub reference: Option<String>,
    pub code: Code,
    pub phase: SalePhase,
    /// Its order's phase; `None` with no order yet.
    pub payment: Option<PaymentPhase>,
    /// When its code (the order's latest quote) runs out, Unix seconds; 0
    /// with no order.
    pub expires_at: i64,
    pub xmr: String,
    pub address: String,
    /// The POS payment as `TillSale::found` holds it, when this till sees it.
    pub found: Option<FoundSale>,
    pub receipt: Option<Receipt>,
    pub customer_name: Option<String>,
    pub device_id: Option<DeviceId>,
    pub device_name: Option<String>,
    pub url: Option<String>,
    /// Monokulo said nothing more will happen to its order.
    pub settled: bool,
    pub created_at: i64,
    pub seen_at: Option<i64>,
    pub paid_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub refunded_at: Option<i64>,
    pub refund_pending_at: Option<i64>,
    pub confirmations: Option<u32>,
    pub confirmations_required: Option<u32>,
    /// A cash sale offered in Monero, with no order: cash as Monero is off,
    /// or it's another counter's, whose till makes its order
    /// ([`SaleView::for_its_counter`]).
    pub offered: bool,
    /// Offered only because it's another counter's cash sale, with cash as
    /// Monero on: its own counter's till makes the order. Listed quietly,
    /// never alerted, until the cashier here takes it in Monero.
    pub for_its_counter: bool,
    pub kept_as_cash_at: Option<i64>,
    pub paid_in_cash_at: Option<i64>,

    /// Rung up as cash at the POS.
    pub cash: bool,
    /// No order yet, and not offered: one is being made.
    pub waiting_for_order: bool,
    /// Its order offers `renew-order`: New code can renew it (core.md
    /// decision 171).
    pub renewable: bool,
    /// Another counter's sale, with this counter's device chosen.
    pub other_counter: bool,
    /// Its alerts are this till's: not another counter's, or every
    /// device's alert here.
    pub alerts_here: bool,
    /// The code is up: live and not closed or ended locally.
    pub live: bool,
    /// This device's clock says the code ran out before Monokulo did: "No
    /// payment yet".
    pub clock_lapsed: bool,
    /// Paid after it was refunded: refund the Monero.
    pub paid_after_refund: bool,
    /// Paid after the cashier closed it.
    pub paid_after_close: bool,
    /// Why the last refund this till tried failed.
    pub refund_problem: Option<String>,
}

/// A ledger time as a view shows it, in whole seconds.
fn secs(ms: Option<UnixMillis>) -> Option<i64> {
    ms.map(|ms| ms.to_seconds().0)
}

/// The view of one sale, from its shared state, this device's marks and
/// settings. `rung_at` is the `sales` row's (Unix ms).
pub fn view(
    sale: &SaleKey,
    shared: &SharedSale,
    marks: &LocalMarks,
    settings: &ViewSettings,
    rung_at: UnixMillis,
) -> SaleView {
    let payment = shared.pos.payment.as_ref();
    let deciding = shared.order.as_ref();
    let order = deciding.map(|a| &a.order);
    let cash = payment.is_some_and(|p| p.tender == Tender::Cash);
    let other_counter = match &settings.device {
        None => false,
        Some(mine) => match payment {
            Some(p) => p.device_id.as_ref().is_some_and(|d| d != mine),
            None => true,
        },
    };
    // A cash sale with no order is offered here when this till doesn't
    // make its order: with cash as Monero off, or as another counter's
    // sale, whose own till makes it (§7). That one waits for its counter,
    // quietly, until the cashier here takes it in Monero.
    let makes_cash_orders = settings.cash_as_monero && (settings.every_device || !other_counter);
    let offered = cash && deciding.is_none() && !makes_cash_orders;
    let for_its_counter = offered && settings.cash_as_monero;
    let waiting_for_order = deciding.is_none() && !offered;

    let found = payment.map(|p| FoundSale {
        id: p.payment.0.clone(),
        amount: p.amount,
        reference: p.receipt_number.clone(),
        order_id: p.order_ref.clone(),
        created_at: p.created_at.clone(),
        updated_at: None,
        status: None,
        device_id: p.device_id.clone(),
        device_name: p.device_name.clone(),
        cash,
        title: p.title.clone(),
        point_of_sale: p.pos,
    });

    let read = shared.pos.order.as_ref();
    let receipt = read
        .and_then(|r| r.receipt.as_deref())
        .and_then(|text| orf_codec::decode_text(text).ok());
    let customer_name = read.and_then(|r| r.customer_name.clone());

    let code = match order {
        None => Code::default(),
        Some(o) => match (&receipt, sale) {
            (Some(receipt), _) => match orf_codec::query::append(&o.payment_uri, receipt) {
                Ok(uri) => Code {
                    uri,
                    has_receipt: true,
                    why: None,
                },
                Err(e) => Code {
                    uri: o.payment_uri.clone(),
                    has_receipt: false,
                    why: Some(e.to_string()),
                },
            },
            (None, SaleKey::Order(_)) => Code {
                uri: o.payment_uri.clone(),
                has_receipt: false,
                why: Some(WHY_NO_POS.to_owned()),
            },
            (None, SaleKey::Pos { .. }) => Code {
                uri: o.payment_uri.clone(),
                has_receipt: false,
                why: read.and_then(|r| r.why.clone()),
            },
        },
    };

    let phase = shared.phase();
    let paid = phase.is_some_and(PaymentPhase::is_paid);

    let device_id = payment.and_then(|p| p.device_id.clone());
    // Refunded once the POS completes the refund, or as soon as it accepts
    // the one this till asked for: Square answers a refund PENDING and
    // completes it a little later, and the cashier who asked is done then,
    // as before the ledger (core.md decision 181). A refund made in the
    // POS's own app waits for completion, and a failed or rejected one
    // isn't one; [`SharedSale::ended`] waits for completion either way.
    let refund_asked = marks
        .actions
        .get(&crate::facts::Action::Refund);
    let refund_done_at = refund_asked
        .filter(|a| a.outcome == Some(Outcome::Done))
        .and_then(|a| a.done_at);
    let pos_refunded = shared.refund.as_ref().is_some_and(|r| match r.status {
        RefundStatus::Completed => true,
        RefundStatus::Pending => refund_done_at.is_some(),
        RefundStatus::Failed | RefundStatus::Rejected => false,
    });
    let refunded_at = marks
        .refunded_at
        .or(marks.refund.and_then(|r| r.confirmed_at))
        .or(if pos_refunded {
            Some(refund_done_at.unwrap_or(rung_at))
        } else {
            None
        });
    let closed_at = marks.closed_at();
    let live = phase.is_some_and(PaymentPhase::is_live)
        && closed_at.is_none()
        && marks.paid_in_cash.is_none()
        && refunded_at.is_none();
    let refund_problem = refund_asked.and_then(|a| match &a.outcome {
        Some(Outcome::Failed(why)) => Some(why.clone()),
        _ => None,
    });

    SaleView {
        sale: sale.clone(),
        order_id: match (order, payment) {
            (Some(o), _) => o.order_id.clone(),
            (None, Some(p)) if cash => format!("{OFFERED}{}", p.payment).into(),
            (None, _) => sale.text().into(),
        },
        price: order.map(|o| o.price.clone()).unwrap_or_else(|| {
            payment.map_or_else(
                || till_types::Price::new("", ""),
                |p| till_types::Price::of(p.amount),
            )
        }),
        reference: order
            .and_then(|o| o.reference.clone())
            .or_else(|| payment.and_then(|p| p.receipt_number.clone())),
        code,
        phase: phase.map_or(SalePhase::Waiting, PaymentPhase::sale_phase),
        payment: phase,
        expires_at: order.map_or(0, |o| o.expires_at.0),
        xmr: order.map(|o| o.xmr.clone()).unwrap_or_default(),
        address: order.map(|o| o.address.clone()).unwrap_or_default(),
        found,
        receipt,
        customer_name,
        device_name: payment
            .and_then(|p| p.device_name.clone())
            .or_else(|| order.and_then(|o| o.device_name.clone())),
        device_id,
        url: order.and_then(|o| o.url.clone()),
        settled: deciding.is_some_and(|a| a.observed.settled),
        created_at: rung_at.to_seconds().0,
        seen_at: secs(marks.seen_at),
        paid_at: secs(marks.paid_at),
        ended_at: secs(marks.ended_at),
        closed_at: secs(closed_at),
        refunded_at: secs(refunded_at),
        refund_pending_at: secs(marks.refund.and_then(|r| r.pending_at)),
        confirmations: deciding.and_then(|a| a.observed.confirmations),
        confirmations_required: order.and_then(|o| o.confirmations_required),
        offered,
        for_its_counter,
        kept_as_cash_at: secs(marks.kept_as_cash),
        paid_in_cash_at: secs(marks.paid_in_cash),
        cash,
        waiting_for_order,
        renewable: order.is_some_and(|o| o.renew.is_some()),
        alerts_here: !other_counter || settings.every_device,
        other_counter,
        live,
        // Only a lapse after this quote ran out: a renewed code is up again
        // (core.md decision 170).
        clock_lapsed: live
            && marks
                .clock_lapsed
                .zip(order)
                .is_some_and(|(at, o)| at >= o.expires_at.to_millis()),
        paid_after_refund: paid && refunded_at.is_some(),
        paid_after_close: paid && closed_at.is_some(),
        refund_problem,
    }
}

impl SaleView {
    /// It has its Monokulo order.
    pub fn has_order(&self) -> bool {
        self.payment.is_some()
    }
}

/// Whether this till may make the sale's next order (§7): a Monero sale's
/// from any till; a cash sale's (cash-as-Monero on) only from a till set to
/// the sale's device or to every device. Taking an offered sale in Monero
/// and New code are the cashier's, from any till: pass `by_cashier`.
pub fn may_make_order(view: &SaleView, settings: &ViewSettings, by_cashier: bool) -> bool {
    if by_cashier {
        return true;
    }
    if !view.cash {
        return true;
    }
    settings.cash_as_monero && (settings.every_device || !view.other_counter)
}
