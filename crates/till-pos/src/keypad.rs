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
//! payment. It's kept through a [`ChargeStore`] too, so a restart doesn't
//! forget it: the first request may have reached the backend and made the
//! payment before it failed (core.md decision 256).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use monokulo_client::PointOfSale;
use orf_codec::{LineItem, Merchant, Payment, Receipt, ReceiptInfo, ReceiptType};
use serde::{Deserialize, Serialize};
use till_types::Money;

use crate::time::Rfc3339Instant;
use crate::{FoundSale, PosError, Refunds, SaleOrder, SaleSource};

/// What a keypad sale's id starts with: no Square or Clover payment id does.
pub const KEYPAD_ID: &str = "kp-";

/// The most a reference keeps, in characters, as Monokulo's web POS does.
pub const REFERENCE_MAX: usize = 120;

/// How long a failed charge is kept for a retry when nothing else is said
/// ([`KeypadSource::set_window`]), in seconds: two hours, the longest a
/// payment code of the till's own wallet lasts.
pub const DEFAULT_WINDOW: i64 = 120 * 60;

/// A charge that failed, as kept while it waits to be tried again: the
/// sale (its id, from which the idempotency key is made, its amount and
/// reference, when it was rung up) and when it was last tried. It is not a
/// sale of the till's: nothing is recorded until the payment is made
/// (decision 250).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCharge {
    pub sale: FoundSale,
    pub tried_at: Rfc3339Instant,
}

/// Where the app keeps the failed charge, one at most. A read that fails
/// is logged by the app and taken as none: at worst a retry makes a new
/// key, as before the charge was kept.
pub trait ChargeStore: Send + Sync {
    fn load(&self) -> Option<PendingCharge>;
    /// Keeps `charge` in place of what was kept; `None` forgets it.
    fn save(&self, charge: Option<&PendingCharge>);
}

/// The keypad's sales: the charge made and not yet handled.
pub struct KeypadSource {
    /// Who the receipts are from: the store's or the wallet's name.
    merchant: Mutex<String>,
    pending: Mutex<Option<PendingCharge>>,
    store: Option<Arc<dyn ChargeStore>>,
    /// How long after its last try a failed charge is kept, in seconds.
    window: Mutex<i64>,
}

impl Default for KeypadSource {
    fn default() -> Self {
        KeypadSource::new()
    }
}

impl KeypadSource {
    /// A keypad that keeps its failed charge in memory only.
    pub fn new() -> KeypadSource {
        KeypadSource {
            merchant: Mutex::new(String::new()),
            pending: Mutex::new(None),
            store: None,
            window: Mutex::new(DEFAULT_WINDOW),
        }
    }

    /// A keypad that keeps its failed charge in `store`, starting with the
    /// one kept there.
    pub fn kept(store: Arc<dyn ChargeStore>) -> KeypadSource {
        KeypadSource {
            pending: Mutex::new(store.load()),
            store: Some(store),
            ..KeypadSource::new()
        }
    }

    /// A failed charge is kept for `seconds` after its last try: as long as
    /// the payment it may have made can be paid.
    pub fn set_window(&self, seconds: i64) {
        *self.window.lock().unwrap_or_else(|e| e.into_inner()) = seconds.max(0);
    }

    fn window(&self) -> i64 {
        *self.window.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keeps `charge`, in memory and in the store.
    fn keep(&self, pending: &mut Option<PendingCharge>, charge: Option<PendingCharge>) {
        if let Some(store) = &self.store {
            store.save(charge.as_ref());
        }
        *pending = charge;
    }

    /// The failed charge waiting for a retry at `now`, if any: one past its
    /// window is forgotten. The keypad shows its amount and reference again
    /// after a restart.
    pub fn restore(&self, now: Rfc3339Instant) -> Option<FoundSale> {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let live = pending
            .as_ref()
            .is_some_and(|c| now.as_second() <= c.tried_at.as_second() + self.window());
        if !live && pending.is_some() {
            self.keep(&mut pending, None);
        }
        pending.as_ref().map(|c| c.sale.clone())
    }

    /// Forgets the failed charge: the cashier cleared the keypad, or typed
    /// another amount or reference.
    pub fn forget(&self) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if pending.is_some() {
            self.keep(&mut pending, None);
        }
    }

    /// Receipts from now on name `name` as the merchant.
    pub fn set_merchant(&self, name: &str) {
        *self.merchant.lock().unwrap_or_else(|e| e.into_inner()) = name.trim().to_owned();
    }

    /// A charge of `amount` with `reference` (trimmed, at most
    /// [`REFERENCE_MAX`] characters; none when empty), rung up `at`: the
    /// one still pending with the same amount and reference when there is
    /// one within its window (a retry), else a new sale with a random id.
    /// Either is kept, tried `at`, until it's handled or forgotten.
    pub fn charge(&self, amount: Money, reference: Option<&str>, at: Rfc3339Instant) -> FoundSale {
        let reference = reference
            .map(|r| r.trim().chars().take(REFERENCE_MAX).collect::<String>())
            .filter(|r| !r.is_empty());
        let window = self.window();
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(sale) = pending
            .as_ref()
            .filter(|c| at.as_second() <= c.tried_at.as_second() + window)
            .map(|c| &c.sale)
            .filter(|s| s.amount == amount && s.reference == reference)
            .cloned()
        {
            let tried = PendingCharge {
                sale: sale.clone(),
                tried_at: at,
            };
            self.keep(&mut pending, Some(tried));
            return sale;
        }
        let sale = FoundSale {
            id: new_id(),
            amount,
            reference: reference.clone(),
            order_id: None,
            created_at: at.clone(),
            updated_at: None,
            status: None,
            device_id: None,
            device_name: None,
            cash: false,
            title: reference,
            point_of_sale: PointOfSale::Till,
        };
        // Only the last charge is retried: an older failed one is given up.
        let kept = PendingCharge {
            sale: sale.clone(),
            tried_at: at,
        };
        self.keep(&mut pending, Some(kept));
        sale
    }

    /// The charges made and not yet handled.
    pub fn pending(&self) -> Vec<FoundSale> {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|c| c.sale.clone())
            .collect()
    }

    fn merchant(&self) -> String {
        let name = self
            .merchant
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if name.is_empty() {
            "Monokulo Till".into()
        } else {
            name
        }
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
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if pending.as_ref().is_some_and(|c| c.sale.id == sale.id) {
            self.keep(&mut pending, None);
        }
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

    /// A store in memory, as the app's settings store keeps it: JSON.
    #[derive(Default)]
    struct Kept(Mutex<Option<String>>);

    impl ChargeStore for Kept {
        fn load(&self) -> Option<PendingCharge> {
            let json = self.0.lock().unwrap().clone()?;
            serde_json::from_str(&json).ok()
        }
        fn save(&self, charge: Option<&PendingCharge>) {
            *self.0.lock().unwrap() = charge.map(|c| serde_json::to_string(c).unwrap());
        }
    }

    fn later(seconds: i64) -> Rfc3339Instant {
        Rfc3339Instant::from_second(at().as_second() + seconds)
    }

    #[test]
    fn a_failed_charge_survives_a_restart_with_its_key() {
        let store = Arc::new(Kept::default());
        let usd = Money::new(2450, CurrencyCode::USD);
        let first = KeypadSource::kept(store.clone()).charge(usd, Some("Table 4"), at());
        // The app restarts: a new keypad over the same store.
        let keypad = KeypadSource::kept(store.clone());
        let restored = keypad.restore(later(60)).expect("kept through the restart");
        assert_eq!(restored, first, "the same amount, reference, id and time");
        let retry = keypad.charge(usd, Some("Table 4"), later(90));
        assert_eq!(retry.id, first.id, "a retry after a restart keeps its key");
        keypad.handled(&retry);
        assert!(store.load().is_none(), "a charge made is forgotten");
        assert_eq!(KeypadSource::kept(store).restore(later(120)), None);
    }

    #[test]
    fn a_failed_charge_is_forgotten_when_cleared_or_changed() {
        let store = Arc::new(Kept::default());
        let keypad = KeypadSource::kept(store.clone());
        let usd = |minor| Money::new(minor, CurrencyCode::USD);
        keypad.charge(usd(100), None, at());
        keypad.forget();
        assert!(store.load().is_none(), "cleared on the keypad");
        assert!(keypad.pending().is_empty());
        let first = keypad.charge(usd(100), None, at());
        let other = keypad.charge(usd(200), None, at());
        assert_eq!(store.load().map(|c| c.sale), Some(other.clone()));
        assert_ne!(other.id, first.id, "another amount is another key");
    }

    #[test]
    fn a_failed_charge_is_kept_for_its_window_after_its_last_try() {
        let store = Arc::new(Kept::default());
        let keypad = KeypadSource::kept(store.clone());
        keypad.set_window(30 * 60);
        let usd = Money::new(500, CurrencyCode::USD);
        let first = keypad.charge(usd, None, at());
        // Tried again 20 minutes on: the window runs from then.
        assert_eq!(keypad.charge(usd, None, later(20 * 60)).id, first.id);
        let keypad = KeypadSource::kept(store.clone());
        keypad.set_window(30 * 60);
        assert!(
            keypad.restore(later(50 * 60)).is_some(),
            "30 minutes after the retry"
        );
        assert_eq!(keypad.restore(later(50 * 60 + 1)), None, "past its window");
        assert!(store.load().is_none(), "and forgotten");
        let late = KeypadSource::kept(store.clone());
        late.charge(usd, None, at());
        late.set_window(60);
        assert_ne!(late.charge(usd, None, later(61)).id, first.id);
    }

    #[test]
    fn the_receipt_is_one_line_the_amount_with_the_reference() {
        let keypad = KeypadSource::new();
        keypad.set_merchant("Corner Café");
        let sale = keypad.charge(
            Money::new(125, CurrencyCode::EUR),
            Some("Table 4\nby the door"),
            at(),
        );
        let r = receipt(&sale, "Corner Café").unwrap();
        assert_eq!(r.line_items.len(), 1);
        assert_eq!(r.line_items[0].description, "Table 4 by the door");
        assert_eq!((r.total, r.currency.as_str()), (1.25, "EUR"));
        assert_eq!(r.merchant.legal_name, "Corner Café");
        assert!(orf_codec::encode(&r).is_ok(), "the codec takes it");
        let plain = keypad.charge(Money::new(500, CurrencyCode::EUR), None, at());
        assert_eq!(
            receipt(&plain, "x").unwrap().line_items[0].description,
            "Sale"
        );
    }
}
