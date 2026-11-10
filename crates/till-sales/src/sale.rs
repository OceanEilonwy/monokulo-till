//! A sale on the till, the code it shows, and the till's state (Android:
//! `TillSale`, `TillState` in `Till.kt`, and `PaymentCode`, `ReceiptCode`
//! in the `receipt` module).

use std::collections::BTreeMap;

use monokulo_client::OrderId;
use monokulo_client::{SalePhase, UnixSeconds};
use orf_codec::Receipt;
use serde::{Deserialize, Serialize};
use till_ledger::SaleView;
use till_pos::FoundSale;

use crate::kotlin_json;

/// The code to show for a sale: the payment URI, with the receipt when it
/// could be added. Its JSON is the Kotlin `PaymentCode`'s.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentCode {
    pub uri: String,
    /// Whether `uri` carries the receipt.
    pub has_receipt: bool,
    /// Why it doesn't, when it doesn't but one was asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

impl PaymentCode {
    /// `payment_uri` (as the order gave it), with `receipt` in it when
    /// there is one and it encodes (docs/design.md §4). Paying never
    /// depends on the receipt: whenever one can't be added, the plain URI
    /// is the code, and `why` says what went wrong.
    pub fn of(payment_uri: &str, receipt: Option<&Receipt>) -> PaymentCode {
        let Some(receipt) = receipt else {
            return PaymentCode::plain(payment_uri);
        };
        match orf_codec::query::append(payment_uri, receipt) {
            Ok(uri) => PaymentCode {
                uri,
                has_receipt: true,
                why: None,
            },
            Err(e) => PaymentCode {
                uri: payment_uri.to_owned(),
                has_receipt: false,
                why: Some(e.to_string()),
            },
        }
    }

    /// `payment_uri` alone: no receipt was asked for.
    pub fn plain(payment_uri: &str) -> PaymentCode {
        PaymentCode {
            uri: payment_uri.to_owned(),
            has_receipt: false,
            why: None,
        }
    }
}

/// A sale on the till's screen, and in its saved shift ([`crate::ShiftStore`]).
///
/// Its JSON is what the Kotlin `TillSale` wrote with the Android app's
/// settings (`explicitNulls = false`, defaults left out), field for field
/// and byte for byte, so a shift saved by either reads in the other
/// ([`crate::shift_json`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TillSale {
    pub order_id: OrderId,
    /// Its price, as the order said it: kept as Kotlin kept it, `amount`
    /// and `currency` as written, and read once as money.
    #[serde(flatten)]
    pub price: till_types::Price,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// What to show as the QR code: with the receipt when this till found
    /// the sale and could add it.
    pub code: PaymentCode,
    pub phase: SalePhase,
    pub expires_at: UnixSeconds,
    pub xmr: String,
    /// The address the customer pays: what "Copy address" copies.
    pub address: String,
    /// The POS sale this till found, when it found it: what a refund needs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub found: Option<FoundSale>,
    /// What was bought, as the receipt this till built from the POS's order
    /// (docs/design.md §4). It is shown on this till's own screens and
    /// never leaves the device except inside `code`: never sent to
    /// Monokulo, never logged. `None` for another till's sale, or when the
    /// order couldn't be read.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "kotlin_json::receipt::serialize",
        deserialize_with = "kotlin_json::receipt::deserialize"
    )]
    pub receipt: Option<Receipt>,
    /// Whom the sale is for, when the POS's order names them: the sale
    /// page's title. Kept as `receipt` is, on this till's own screens only:
    /// never in the code, never sent to Monokulo, never logged, never on a
    /// notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    /// The device that made the order, for a sale another till found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// Where to read the order again: its `self` link, or its sale event's
    /// `mk:order`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Monokulo said nothing more will happen to the order (`is_final`). An
    /// expired or cancelled order isn't, while Monokulo still watches it for
    /// a late payment that turns it paid.
    #[serde(default, skip_serializing_if = "is_false")]
    pub settled: bool,
    /// When it was rung up: the POS's time for this till's own, the
    /// event's for another's.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub created_at: UnixSeconds,
    /// When its payment was first seen on the network, if it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_at: Option<UnixSeconds>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paid_at: Option<UnixSeconds>,
    /// When it ended unpaid: the code ran out, the order was cancelled or
    /// the payment reversed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<UnixSeconds>,
    /// When the cashier closed it on this till (the close sheet's "Not
    /// sure" or "No"), if they did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<UnixSeconds>,
    /// When it was refunded in the POS: by this till through the POS's API
    /// ([`till_pos::Refunds::ByTill`]), or when the till saw the cashier had
    /// done it there ([`till_pos::Refunds::InPos`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refunded_at: Option<UnixSeconds>,
    /// When the cashier was told to refund it in the POS themselves
    /// ([`till_pos::Refunds::InPos`]): until `refunded_at`, and only while
    /// the sale's point of sale ([`FoundSale::point_of_sale`]) is the
    /// till's, the till looks for the refund.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refund_pending_at: Option<UnixSeconds>,
    /// How many confirmations its payment has, of the
    /// `confirmations_required` the store asks for its amount: what a seen
    /// or confirming sale's ring grows by and its "Confirming · 4 of 10"
    /// counts. `None` until Monokulo says, when the ring fills by step
    /// instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmations_required: Option<u32>,
    /// A cash sale the till offers to take in Monero, with no order made for
    /// it yet (core.md decision 116): the cashier turned off "Treat cash
    /// sales as Monero". Its `order_id` is [`TillSale::OFFERED`] and the
    /// payment's id; its code, address and XMR are empty until it's taken
    /// ([`crate::Till::take_in_monero`]), when a sale with the order takes
    /// its place.
    #[serde(default, skip_serializing_if = "is_false")]
    pub offered: bool,
    /// When the cashier chose "Keep as cash" for an offered sale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept_as_cash_at: Option<UnixSeconds>,
    /// When the cashier said a cash sale's customer paid in real cash after
    /// all ("Paid in cash"): its code comes down, and nothing is refunded,
    /// as the point of sale already has the cash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paid_in_cash_at: Option<UnixSeconds>,
    /// What only the ledger knows (docs/ledger.md §5, §7): whether its order
    /// can be renewed, and its warnings. Empty for a sale read from a kept
    /// shift or a fixture, and left out of their JSON.
    #[serde(default, skip_serializing_if = "SaleExtras::is_empty")]
    pub extras: SaleExtras,
}

/// What the ledger adds to a sale (docs/ledger.md §5, §7).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaleExtras {
    /// The ledger's key for the sale (`sq:<payment>`, `mk:<order>`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sale_key: String,
    /// Its order offers `renew-order`, so New code can renew it (core.md
    /// decision 171): false for an order an older Monokulo made.
    #[serde(default, skip_serializing_if = "is_false")]
    pub renewable: bool,
    /// Paid more than asked (or twice): the extra is owed back to the
    /// customer (core.md decision 168).
    #[serde(default, skip_serializing_if = "is_false")]
    pub overpaid: bool,
    /// When the cashier said the extra went back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_refunded_at: Option<UnixSeconds>,
    /// Paid after it was refunded in the point of sale.
    #[serde(default, skip_serializing_if = "is_false")]
    pub paid_after_refund: bool,
    /// Paid after the cashier closed it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub paid_after_close: bool,
    /// Another counter's sale, with this counter's device chosen.
    #[serde(default, skip_serializing_if = "is_false")]
    pub other_counter: bool,
    /// The code ran out by this device's clock before Monokulo said so.
    #[serde(default, skip_serializing_if = "is_false")]
    pub clock_lapsed: bool,
}

impl SaleExtras {
    pub fn is_empty(&self) -> bool {
        *self == SaleExtras::default()
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(at: &UnixSeconds) -> bool {
    at.0 == 0
}

impl TillSale {
    /// What an offered cash sale's `order_id` starts with, before the
    /// payment's id: no Monokulo order id does.
    pub const OFFERED: &'static str = "cash:";

    /// The ledger's view of a sale as the screens take it: field for field,
    /// but that a code this device's clock says ran out shows as expired
    /// ("No payment yet") before Monokulo says so (docs/ledger.md §5).
    pub fn of_view(v: &SaleView) -> TillSale {
        let lapsed = v.clock_lapsed && v.phase != SalePhase::Paid;
        TillSale {
            order_id: v.order_id.clone(),
            price: v.price.clone(),
            reference: v.reference.clone(),
            code: PaymentCode {
                uri: v.code.uri.clone(),
                has_receipt: v.code.has_receipt,
                why: v.code.why.clone(),
            },
            phase: if lapsed { SalePhase::Expired } else { v.phase },
            expires_at: v.expires_at,
            xmr: v.xmr.clone(),
            address: v.address.clone(),
            found: v.found.clone(),
            receipt: v.receipt.clone(),
            customer_name: v.customer_name.clone(),
            device_name: v.device_name.clone(),
            url: v.url.clone(),
            settled: v.settled,
            created_at: v.created_at,
            seen_at: v.seen_at,
            paid_at: v.paid_at,
            ended_at: v
                .ended_at
                .or_else(|| lapsed.then_some(v.expires_at).filter(|at| at.0 > 0)),
            closed_at: v.closed_at,
            refunded_at: v.refunded_at,
            refund_pending_at: v.refund_pending_at,
            confirmations: v.confirmations,
            confirmations_required: v.confirmations_required,
            offered: v.offered,
            kept_as_cash_at: v.kept_as_cash_at,
            paid_in_cash_at: v.paid_in_cash_at,
            extras: SaleExtras {
                sale_key: v.sale.text(),
                renewable: v.renewable,
                overpaid: v.overpaid,
                extra_refunded_at: v.extra_refunded_at,
                paid_after_refund: v.paid_after_refund,
                paid_after_close: v.paid_after_close,
                other_counter: v.other_counter,
                clock_lapsed: lapsed,
            },
        }
    }

    /// Unpaid, its code over by Monokulo's word or this device's clock, and
    /// not refunded, closed or settled in cash: the cashier may have its
    /// order renewed for a new code ("New code", docs/ledger.md §7), when
    /// the till can see its point-of-sale payment and the store's Monokulo
    /// can renew the order (core.md decision 171).
    pub fn may_new_code(&self) -> bool {
        self.found.is_some()
            && self.extras.renewable
            && !self.offered
            && matches!(self.phase, SalePhase::Expired | SalePhase::Cancelled)
            && self.closed_at.is_none()
            && self.refunded_at.is_none()
            && self.refund_pending_at.is_none()
            && self.paid_in_cash_at.is_none()
            && self.kept_as_cash_at.is_none()
    }

    /// Worth reading again: Monokulo may still move it on. An offered cash
    /// sale has no order to read.
    pub fn open(&self) -> bool {
        !self.offered && !self.settled && self.phase != SalePhase::Paid
    }

    /// Rung up as cash at the point of sale ([`FoundSale::cash`]), whether
    /// or not it's been taken in Monero since.
    pub fn cash(&self) -> bool {
        self.found.as_ref().is_some_and(|found| found.cash)
    }

    /// A cash sale paid in Monero: the point of sale's books say cash,
    /// so the end-of-day count needs to know (core.md decision 117).
    pub fn cash_paid_in_monero(&self) -> bool {
        self.cash() && self.phase == SalePhase::Paid
    }

    /// The point of sale's device that took it, by id and by name, when the
    /// point of sale said.
    pub fn pos_device(&self) -> Option<(&till_pos::PosDeviceId, Option<&str>)> {
        let found = self.found.as_ref()?;
        Some((found.device_id.as_ref()?, found.device_name.as_deref()))
    }

    /// Its payment is on the network and not yet paid: its confirmations
    /// are worth following ([`crate::Till::follow_confirmations`]).
    pub fn confirming(&self) -> bool {
        self.open() && matches!(self.phase, SalePhase::Seen | SalePhase::Confirming)
    }

    pub fn refunded(&self) -> bool {
        self.refunded_at.is_some()
    }
}

/// What the till shows: its sales, newest first, and the last thing that
/// went wrong.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TillState {
    /// What the ledger holds for the shift, newest first: never changed
    /// here but by reading the ledger again ([`crate::Books`]).
    pub sales: Vec<TillSale>,
    /// The ledger's views of the same sales, in the same order.
    pub views: Vec<SaleView>,
    /// When the shift's window starts: the later of the shift's start and
    /// the trading day's (04:00 local, docs/ledger.md §9).
    pub shift_from: Option<UnixSeconds>,
    pub problem: Option<String>,
    /// The POS sign-in has ended: the cashier must sign in again.
    pub sign_in_again: bool,
    /// Why the POS refused a refund, by order: shown on that sale's page
    /// beside its Refund button until it's tried again, which the next
    /// check, clearing `problem`, doesn't undo.
    pub refund_problems: BTreeMap<OrderId, String>,
}

impl TillState {
    /// The sale for `order_id`, if the till has it.
    pub fn sale(&self, order_id: &OrderId) -> Option<&TillSale> {
        self.sales.iter().find(|sale| sale.order_id == order_id)
    }
}

#[cfg(test)]
mod tests {
    //! Ported from the Android app's `ReceiptCodeTest`, whose `ReceiptCode`
    //! is [`PaymentCode::of`] here.

    use serde_json::json;

    use super::*;

    const URI: &str = "monero:8Bx?tx_amount=0.5";

    fn receipt(lines: serde_json::Value) -> Receipt {
        serde_json::from_value(json!({
            "orf_version": "0.1.0",
            "receipt": {"receipt_id": "#1042", "issue_timestamp": "2026-10-22T09:15:00-07:00", "receipt_type": "sale"},
            "merchant": {"legal_name": "The Bean Room", "country_code": "US"},
            "line_items": lines,
            "tax": {"tax_lines": [{"rate": 0.0888, "tax_amount": 1.13}]},
            "payment": {"payment_method": "monero"},
            "total": 13.88,
            "currency": "USD"
        }))
        .unwrap()
    }

    #[test]
    fn a_receipt_rides_in_the_payment_uri_and_reads_back_exactly() {
        let sent = receipt(json!([
            {"description": "Flat white, oat milk", "quantity": 2.0, "unit_price": 4.5, "line_total": 9.0},
            {"description": "Almond croissant", "quantity": 1.0, "unit_price": 3.75, "line_total": 3.75}
        ]));
        let code = PaymentCode::of(URI, Some(&sent));
        assert!(code.has_receipt);
        assert!(code.uri.starts_with(&format!("{URI}&orf=")), "{}", code.uri);
        let back = orf_codec::query::from_uri(&code.uri).unwrap().unwrap();
        assert_eq!(back, sent);
    }

    #[test]
    fn without_a_receipt_or_one_the_codec_refuses_the_plain_uri_is_the_code() {
        assert_eq!(
            PaymentCode::of(URI, None),
            PaymentCode {
                uri: URI.into(),
                has_receipt: false,
                why: None
            }
        );
        // Half a cent can't be carried exactly.
        let refused = PaymentCode::of(
            URI,
            Some(&receipt(json!([
                {"description": "Gum", "quantity": 1.0, "unit_price": 0.005, "line_total": 0.005}
            ]))),
        );
        assert!(!refused.has_receipt);
        assert_eq!(refused.uri, URI);
        assert!(refused.why.is_some());
    }
}
