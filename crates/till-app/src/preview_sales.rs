//! Sales for the previews, built here so a platform's on-device tests can't
//! drift from the saved shift's schema (core.md decision 108). Android's
//! tests used to keep a hand copy of `TillSale`'s JSON (`SavedShift.kt`);
//! now they build a [`PreviewSale`] and hand the engine
//! [`preview_sales_json`]'s answer as `TillPreview::sales_json`.

use monokulo_client::{PointOfSale, SalePhase};
use serde_json::json;
use till_pos::FoundSale;
use till_sales::{PaymentCode, TillSale, shift_json};

use crate::Pos;

/// Where a preview sale's payment is: `SalePhase`, for the platforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PreviewPhase {
    Waiting,
    Partial,
    Seen,
    Confirming,
    Paid,
    Expired,
    Cancelled,
    DoubleSpend,
}

impl From<PreviewPhase> for SalePhase {
    fn from(phase: PreviewPhase) -> SalePhase {
        match phase {
            PreviewPhase::Waiting => SalePhase::Waiting,
            PreviewPhase::Partial => SalePhase::Partial,
            PreviewPhase::Seen => SalePhase::Seen,
            PreviewPhase::Confirming => SalePhase::Confirming,
            PreviewPhase::Paid => SalePhase::Paid,
            PreviewPhase::Expired => SalePhase::Expired,
            PreviewPhase::Cancelled => SalePhase::Cancelled,
            PreviewPhase::DoubleSpend => SalePhase::DoubleSpend,
        }
    }
}

/// A sale for a preview: the tests' `testSale`, $8.40 of this till's on
/// Square, waiting, unless they say otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PreviewSale {
    pub order_id: String,
    /// Waiting when not said.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub phase: Option<PreviewPhase>,
    #[cfg_attr(feature = "uniffi", uniffi(default = "8.40"))]
    pub amount: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = "USD"))]
    pub currency: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = "0.0336"))]
    pub xmr: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = "4abc"))]
    pub address: String,
    /// The receipt's number at the point of sale; the order id when not said.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub reference: Option<String>,
    /// Unix seconds; far off when not said.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub expires_at: Option<i64>,
    /// This till found it at its point of sale; else another till's sale.
    #[cfg_attr(feature = "uniffi", uniffi(default = true))]
    pub found: bool,
    /// The point of sale that took it; Square when not said.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub found_pos: Option<Pos>,
    /// The order's title at the point of sale (Clover's "Table 4").
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub found_title: Option<String>,
    /// The design's sample receipt: 15 items on 8 lines, $79.21.
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub receipt: bool,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub customer_name: Option<String>,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub device_name: Option<String>,
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub settled: bool,
    #[cfg_attr(feature = "uniffi", uniffi(default = 1761000000))]
    pub created_at: i64,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub seen_at: Option<i64>,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub paid_at: Option<i64>,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub ended_at: Option<i64>,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub closed_at: Option<i64>,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub refunded_at: Option<i64>,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub refund_pending_at: Option<i64>,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub confirmations: Option<u32>,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub confirmations_required: Option<u32>,
}

impl PreviewSale {
    /// `order_id`'s sale with every other field as its default says.
    pub fn new(order_id: &str) -> PreviewSale {
        PreviewSale {
            order_id: order_id.into(),
            phase: None,
            amount: "8.40".into(),
            currency: "USD".into(),
            xmr: "0.0336".into(),
            address: "4abc".into(),
            reference: None,
            expires_at: None,
            found: true,
            found_pos: None,
            found_title: None,
            receipt: false,
            customer_name: None,
            device_name: None,
            settled: false,
            created_at: 1_761_000_000,
            seen_at: None,
            paid_at: None,
            ended_at: None,
            closed_at: None,
            refunded_at: None,
            refund_pending_at: None,
            confirmations: None,
            confirmations_required: None,
        }
    }

    fn till_sale(&self) -> TillSale {
        let reference = self
            .reference
            .clone()
            .unwrap_or_else(|| self.order_id.clone());
        let currency: till_pos::CurrencyCode =
            self.currency.parse().unwrap_or(till_pos::CurrencyCode::USD);
        let amount = till_pos::Money::from_decimal(&self.amount, currency)
            .unwrap_or(till_pos::Money::new(0, currency));
        let receipt = self.receipt.then(sample_receipt);
        let uri = format!(
            "monero:{}?tx_amount={}&tx_description={}",
            self.address, self.xmr, self.order_id
        );
        TillSale {
            order_id: self.order_id.clone(),
            amount: self.amount.clone(),
            currency: self.currency.clone(),
            reference: Some(reference.clone()),
            code: PaymentCode::of(&uri, receipt.as_ref()),
            phase: self.phase.unwrap_or(PreviewPhase::Waiting).into(),
            expires_at: self.expires_at.unwrap_or(i64::MAX / 2),
            xmr: self.xmr.clone(),
            address: self.address.clone(),
            found: self.found.then(|| FoundSale {
                id: format!("p_{}", self.order_id),
                amount,
                reference: Some(reference),
                order_id: Some("o".into()),
                created_at: "2026-10-22T22:50:00Z".into(),
                updated_at: None,
                status: None,
                device_id: None,
                device_name: None,
                cash: false,
                title: self.found_title.clone(),
                point_of_sale: self.found_pos.map_or(PointOfSale::Square, Into::into),
            }),
            receipt,
            customer_name: self.customer_name.clone(),
            device_name: self.device_name.clone(),
            url: None,
            settled: self.settled,
            created_at: self.created_at,
            seen_at: self.seen_at,
            paid_at: self.paid_at,
            ended_at: self.ended_at,
            closed_at: self.closed_at,
            refunded_at: self.refunded_at,
            refund_pending_at: self.refund_pending_at,
            confirmations: self.confirmations,
            confirmations_required: self.confirmations_required,
            offered: false,
            kept_as_cash_at: None,
            paid_in_cash_at: None,
            extras: till_sales::SaleExtras::default(),
        }
    }
}

/// The design's order: 15 items on 8 lines, so a sale's card shows three
/// and "+5 more".
fn sample_receipt<R: serde::de::DeserializeOwned>() -> R {
    let lines = [
        ("Flat white", 2.0, 4.5, 9.0),
        ("Oat latte", 1.0, 5.25, 5.25),
        ("Almond croissant", 3.0, 3.75, 11.25),
        ("Banana bread", 1.0, 4.5, 4.5),
        ("Cold brew", 2.0, 5.0, 10.0),
        ("Chai", 1.0, 4.75, 4.75),
        ("Choc chip cookie", 4.0, 2.5, 10.0),
        ("House blend beans, 250 g", 1.0, 18.0, 18.0),
    ]
    .map(|(description, quantity, unit_price, line_total)| {
        json!({"description": description, "quantity": quantity, "unit_price": unit_price, "line_total": line_total})
    });
    serde_json::from_value(json!({
        "orf_version": "0.1.0",
        "receipt": {"receipt_id": "k4Rt", "issue_timestamp": "2026-10-22T16:07:00-07:00", "receipt_type": "sale"},
        "merchant": {"legal_name": "The Bean Room", "country_code": "US"},
        "line_items": lines,
        "tax": {"tax_lines": [{"rate": 0.08875, "tax_amount": 6.46}]},
        "total": 79.21,
        "currency": "USD",
    }))
    .expect("the sample receipt is a receipt")
}

/// `sales`, newest first, as the saved shift writes them: what
/// `TillPreview::sales_json` takes.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn preview_sales_json(sales: Vec<PreviewSale>) -> String {
    let sales: Vec<TillSale> = sales.iter().map(PreviewSale::till_sale).collect();
    shift_json::encode(&sales)
}

/// Whether `sale`'s code is up (`TillSale::live`): not paid, not ended, not
/// closed.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn preview_sale_live(sale: PreviewSale) -> bool {
    sale.till_sale().live()
}

/// Which of the ring's four steps `phase` is at (`sale_text::step`).
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn preview_phase_step(phase: PreviewPhase) -> u8 {
    till_screens::sale_text::step(phase.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_sale_is_the_saved_shifts_sale_and_reads_back() {
        let paid = PreviewSale {
            phase: Some(PreviewPhase::Paid),
            paid_at: Some(1_761_000_100),
            receipt: true,
            customer_name: Some("Ana López".into()),
            ..PreviewSale::new("m2Qe")
        };
        let json = preview_sales_json(vec![PreviewSale::new("a1"), paid.clone()]);
        let sales = shift_json::decode(&json).expect("the saved shift reads it");
        assert_eq!(sales.len(), 2);
        let waiting = &sales[0];
        assert_eq!(waiting.reference.as_deref(), Some("a1"));
        assert_eq!(waiting.found.as_ref().unwrap().amount.minor, 840);
        assert!(waiting.live() && preview_sale_live(PreviewSale::new("a1")));
        let read = &sales[1];
        assert_eq!(read.phase, SalePhase::Paid);
        assert!(read.code.has_receipt, "the receipt goes into the code");
        assert_eq!(read.receipt.as_ref().unwrap().line_items.len(), 8);
        assert!(!preview_sale_live(paid));
        assert_eq!(preview_phase_step(PreviewPhase::Confirming), 3);
        assert_eq!(preview_phase_step(PreviewPhase::DoubleSpend), 0);
    }
}
