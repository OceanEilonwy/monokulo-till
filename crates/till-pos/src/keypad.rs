//! "Till" as a point of sale: a keypad on the device (docs/view-key-mode.md
//! §7), for a shop with no point of sale. A charge typed on the keypad is a
//! sale of its own, found here rather than listed by a point of sale's API:
//! [`KeypadSource::charge`] makes it, the till takes it at once
//! (`Till::charge`), and it's handled. Its receipt is one line: the amount,
//! with the reference as its description.
//!
//! A charge that failed is kept until it's handled, so a Charge tried again
//! with the same amount and reference is the same sale, with the same
//! idempotency key: a failed request tried again never makes a second
//! payment.

use std::sync::Mutex;

use async_trait::async_trait;
use monokulo_client::PointOfSale;
use orf_codec::{LineItem, Merchant, Payment, Receipt, ReceiptInfo, ReceiptType};
use till_types::Money;

use crate::time::Rfc3339Instant;
use crate::{FoundSale, PosError, Refunds, SaleOrder, SaleSource};

/// What a keypad sale's id starts with: no Square or Clover payment id does.
pub const KEYPAD_ID: &str = "kp-";

/// The most a reference keeps, in characters, as Monokulo's web POS does.
pub const REFERENCE_MAX: usize = 120;

/// The keypad's sales: the charges made and not yet handled.
pub struct KeypadSource {
    /// Who the receipts are from: the store's or the wallet's name.
    merchant: Mutex<String>,
    pending: Mutex<Vec<FoundSale>>,
}

impl Default for KeypadSource {
    fn default() -> Self {
        KeypadSource::new()
    }
}

impl KeypadSource {
    pub fn new() -> KeypadSource {
        KeypadSource {
            merchant: Mutex::new(String::new()),
            pending: Mutex::new(Vec::new()),
        }
    }

    /// Receipts from now on name `name` as the merchant.
    pub fn set_merchant(&self, name: &str) {
        *self.merchant.lock().unwrap_or_else(|e| e.into_inner()) = name.trim().to_owned();
    }

    /// A charge of `amount` with `reference` (trimmed, at most
    /// [`REFERENCE_MAX`] characters; none when empty), rung up `at`: the
    /// one still pending with the same amount and reference when there is
    /// one (a retry), else a new sale with a random id.
    pub fn charge(&self, amount: Money, reference: Option<&str>, at: Rfc3339Instant) -> FoundSale {
        let reference = reference
            .map(|r| r.trim().chars().take(REFERENCE_MAX).collect::<String>())
            .filter(|r| !r.is_empty());
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(sale) = pending
            .iter()
            .find(|s| s.amount == amount && s.reference == reference)
        {
            return sale.clone();
        }
        let sale = FoundSale {
            id: new_id(),
            amount,
            reference: reference.clone(),
            order_id: None,
            created_at: at,
            updated_at: None,
            status: None,
            device_id: None,
            device_name: None,
            cash: false,
            title: reference,
            point_of_sale: PointOfSale::Till,
        };
        // Only the last charge is retried: an older failed one is given up.
        pending.clear();
        pending.push(sale.clone());
        sale
    }

    /// The charges made and not yet handled.
    pub fn pending(&self) -> Vec<FoundSale> {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn merchant(&self) -> String {
        let name = self.merchant.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if name.is_empty() { "Monokulo Till".into() } else { name }
    }
}

/// `kp-` and 16 random hex digits.
fn new_id() -> String {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("the operating system's random numbers");
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{KEYPAD_ID}{hex}")
}

/// A keypad sale's one-line receipt: the amount, described by its
/// reference (or "Sale"), from `merchant`, numbered by the end of its id.
/// The country isn't known to the keypad, so it's ISO 3166's "ZZ".
pub fn receipt(sale: &FoundSale, merchant: &str) -> Option<Receipt> {
    let digits = sale.amount.currency.minor_digits();
    let total = sale.amount.minor as f64 / 10f64.powi(digits as i32);
    let at = jiff::Timestamp::from_second(sale.created_at.as_second()).ok()?;
    let description = sale
        .reference
        .as_deref()
        .map(one_line)
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| "Sale".into());
    Some(Receipt {
        orf_version: orf_codec::ORF_VERSION.into(),
        receipt: ReceiptInfo {
            receipt_id: sale
                .id
                .trim_start_matches(KEYPAD_ID)
                .chars()
                .take(6)
                .collect(),
            issue_timestamp: orf_codec::issue_timestamp(at, jiff::tz::Offset::UTC),
            receipt_type: ReceiptType::Sale,
            status: None,
        },
        merchant: Merchant {
            merchant_id: None,
            legal_name: one_line(merchant),
            display_name: None,
            country_code: "ZZ".into(),
            tax_identifiers: None,
        },
        customer: None,
        line_items: vec![LineItem {
            line_item_id: None,
            description,
            quantity: 1.0,
            unit_price: total,
            net_amount: None,
            line_total: total,
        }],
        tax: None,
        payment: Some(Payment {
            payment_method: Some("monero".into()),
            provider: None,
            authorization_code: None,
        }),
        total,
        currency: sale.amount.currency.to_string(),
        trust: None,
        extensions: None,
    })
}

/// `text` on one line: the codec refuses line breaks.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[async_trait]
impl SaleSource for KeypadSource {
    fn point_of_sale(&self) -> PointOfSale {
        PointOfSale::Till
    }

    /// None: a charge is taken the moment it's made (`Till::charge`), and a
    /// failed one waits for the cashier to try again, never for a look.
    async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError> {
        Ok(Vec::new())
    }

    async fn order(&self, sale: &FoundSale) -> Result<SaleOrder, PosError> {
        Ok(SaleOrder {
            receipt: receipt(sale, &self.merchant()),
            customer_name: None,
        })
    }

    /// Nothing was taken by card: what came in Monero goes back by hand.
    fn refunds(&self) -> Refunds<'_> {
        Refunds::ByHand
    }

    fn handled(&self, sale: &FoundSale) {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|s| s.id != sale.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use till_types::CurrencyCode;

    fn at() -> Rfc3339Instant {
        "2026-11-13T10:00:00Z".parse().unwrap()
    }

    #[test]
    fn a_charge_tried_again_is_the_same_sale_until_its_handled() {
        let keypad = KeypadSource::new();
        let usd = |minor| Money::new(minor, CurrencyCode::USD);
        let first = keypad.charge(usd(125), Some("  Sam "), at());
        assert!(first.id.starts_with(KEYPAD_ID));
        assert_eq!(first.reference.as_deref(), Some("Sam"));
        assert_eq!(first.point_of_sale, PointOfSale::Till);
        let again = keypad.charge(usd(125), Some("Sam"), at());
        assert_eq!(again.id, first.id, "a retry keeps its key");
        let other = keypad.charge(usd(126), Some("Sam"), at());
        assert_ne!(other.id, first.id, "another amount is another sale");
        assert_eq!(keypad.pending(), vec![other.clone()]);
        keypad.handled(&other);
        assert!(keypad.pending().is_empty());
        let next = keypad.charge(usd(126), Some("Sam"), at());
        assert_ne!(next.id, other.id, "a handled charge is never reused");
        assert_eq!(keypad.charge(usd(1), Some("   "), at()).reference, None);
    }

    #[test]
    fn the_receipt_is_one_line_the_amount_with_the_reference() {
        let keypad = KeypadSource::new();
        keypad.set_merchant("Corner Café");
        let sale = keypad.charge(Money::new(125, CurrencyCode::EUR), Some("Table 4\nby the door"), at());
        let r = receipt(&sale, "Corner Café").unwrap();
        assert_eq!(r.line_items.len(), 1);
        assert_eq!(r.line_items[0].description, "Table 4 by the door");
        assert_eq!((r.total, r.currency.as_str()), (1.25, "EUR"));
        assert_eq!(r.merchant.legal_name, "Corner Café");
        assert!(orf_codec::encode(&r).is_ok(), "the codec takes it");
        let plain = keypad.charge(Money::new(500, CurrencyCode::EUR), None, at());
        assert_eq!(receipt(&plain, "x").unwrap().line_items[0].description, "Sale");
    }
}
