//! A sale on the till, the code it shows, and the till's state (Android:
//! `TillSale`, `TillState` in `Till.kt`, and `PaymentCode`, `ReceiptCode`
//! in the `receipt` module).

use std::collections::BTreeMap;

use monokulo_client::SalePhase;
use orf_codec::Receipt;
use serde::{Deserialize, Serialize};
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
    pub order_id: String,
    pub amount: String,
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// What to show as the QR code: with the receipt when this till found
    /// the sale and could add it.
    pub code: PaymentCode,
    pub phase: SalePhase,
    /// Unix seconds.
    pub expires_at: i64,
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
    /// When it was rung up, Unix seconds: the POS's time for this till's
    /// own, the event's for another's.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub created_at: i64,
    /// When its payment was first seen on the network, if it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paid_at: Option<i64>,
    /// When it ended unpaid: the code ran out, the order was cancelled or
    /// the payment reversed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    /// When the cashier closed it on this till (the close sheet's "Not
    /// sure" or "No"), if they did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<i64>,
    /// When it was refunded in the POS: by this till through the POS's API
    /// ([`till_pos::Refunds::ByTill`]), or when the till saw the cashier had
    /// done it there ([`till_pos::Refunds::InPos`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refunded_at: Option<i64>,
    /// When the cashier was told to refund it in the POS themselves
    /// ([`till_pos::Refunds::InPos`]): until `refunded_at`, and only while
    /// the sale's point of sale ([`FoundSale::point_of_sale`]) is the
    /// till's, the till looks for the refund.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refund_pending_at: Option<i64>,
    /// How many confirmations its payment has, of the
    /// `confirmations_required` the store asks for its amount: what a seen
    /// or confirming sale's ring grows by and its "Confirming · 4 of 10"
    /// counts. `None` until Monokulo says, when the ring fills by step
    /// instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmations_required: Option<u32>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

impl TillSale {
    /// Worth reading again: Monokulo may still move it on.
    pub fn open(&self) -> bool {
        !self.settled && self.phase != SalePhase::Paid
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
    pub sales: Vec<TillSale>,
    pub problem: Option<String>,
    /// The POS sign-in has ended: the cashier must sign in again.
    pub sign_in_again: bool,
    /// Why the POS refused a refund, by order: shown on that sale's page
    /// beside its Refund button until it's tried again, which the next
    /// check, clearing `problem`, doesn't undo.
    pub refund_problems: BTreeMap<String, String>,
}

impl TillState {
    /// The sale for `order_id`, if the till has it.
    pub fn sale(&self, order_id: &str) -> Option<&TillSale> {
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
