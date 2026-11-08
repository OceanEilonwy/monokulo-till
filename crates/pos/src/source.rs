//! What the till asks of a point of sale (Android: `SaleSource.kt`, and
//! `CursorStore` from `SquareSource.kt`).

use async_trait::async_trait;
use monokulo_client::PointOfSale;
use serde::{Deserialize, Serialize};

use crate::PosError;

/// A point of sale the till watches for Monero sales (docs/design.md §1,
/// "POS sources"): Square, Clover.
#[async_trait]
pub trait SaleSource: Send + Sync {
    /// The point of sale this source watches. A sale another one took (the
    /// cashier changed the point of sale since) is never refunded or asked
    /// about through this one.
    fn point_of_sale(&self) -> PointOfSale;

    /// Sales tendered as Monero since the source started watching, oldest
    /// first, that haven't been [handled](SaleSource::handled). Calling it
    /// again gives the same ones until they are.
    async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError>;

    /// The sale's receipt and whom it's for, from one read of its order: no
    /// receipt when one can't be made, and no name at a point of sale that
    /// gives none.
    async fn order(&self, sale: &FoundSale) -> Result<SaleOrder, PosError>;

    /// How a sale never paid in Monero is refunded at this point of sale.
    fn refunds(&self) -> Refunds<'_>;

    /// `sale` has its order: don't give it again.
    fn handled(&self, sale: &FoundSale);
}

/// What the till takes from a sale's order at the point of sale: its
/// receipt, and the customer's name when the point of sale gives one
/// (Square's fulfillment recipient). The name stays on the device; the
/// receipt leaves it only in the sale's code.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaleOrder {
    pub receipt: Option<orf_codec::Receipt>,
    pub customer_name: Option<String>,
}

/// Who refunds a sale never paid in Monero (decision 130 in
/// docs/decisions/android.md). A choice of two traits rather than a
/// `refund()` that does nothing at a point of sale with no refund in its
/// API: there, the till can only look, by construction.
#[derive(Clone, Copy)]
pub enum Refunds<'a> {
    /// The till does, through the point of sale's API (Square).
    ByTill(&'a dyn RefundByTill),
    /// The cashier does, in the point of sale, and the till watches for it
    /// (Clover: no API refunds an external-tender payment). It carries no
    /// words: what the cashier is told names the sale's own point of sale
    /// ([`FoundSale::point_of_sale`]), worded in the app (decision 136).
    InPos(&'a dyn RefundInPos),
}

/// A point of sale the till refunds through.
#[async_trait]
pub trait RefundByTill: Send + Sync {
    /// Refunds `sale` in full.
    async fn refund(&self, sale: &FoundSale) -> Result<(), PosError>;
}

/// A point of sale the cashier refunds in, which the till asks.
#[async_trait]
pub trait RefundInPos: Send + Sync {
    /// Whether the point of sale now shows `sale` refunded.
    async fn refunded(&self, sale: &FoundSale) -> Result<bool, PosError>;
}

/// A Monero sale the point of sale closed. Its JSON is the Kotlin
/// `FoundSale`'s, which a saved shift holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundSale {
    /// The point of sale's id for the payment: what the till's idempotency
    /// key is made from.
    pub id: String,
    /// In the currency's minor unit (cents).
    pub amount_minor: i64,
    pub currency: String,
    /// What the point of sale printed or showed for it ("bP9m"), if
    /// anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// The point of sale's order, for the receipt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    /// RFC 3339.
    pub created_at: String,
    /// The device that took it, when the point of sale says: to show it on
    /// its own till only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// The name the cashier gave the order ("Table 4"), when the point of
    /// sale has one: Clover's title; Square leaves it `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The point of sale that took it, so a shift's saved sales keep naming
    /// it after the till changes point of sale. A sale saved before Clover
    /// joined reads as Square's. Written as Kotlin named the enum's
    /// constants: `"Square"`, `"Clover"`.
    #[serde(default = "square", with = "kotlin_name")]
    pub point_of_sale: PointOfSale,
}

fn square() -> PointOfSale {
    PointOfSale::Square
}

/// A [`PointOfSale`] as kotlinx.serialization wrote the Kotlin enum: by its
/// constant's name.
mod kotlin_name {
    use monokulo_client::PointOfSale;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(pos: &PointOfSale, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(pos.display_name())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<PointOfSale, D::Error> {
        let name = String::deserialize(d)?;
        PointOfSale::ALL
            .into_iter()
            .find(|pos| pos.display_name() == name)
            .ok_or_else(|| D::Error::unknown_variant(&name, &["Square", "Clover"]))
    }
}

/// Where a source keeps its place, so a restart neither repeats nor skips
/// sales. The app implements it, one per point of sale.
pub trait CursorStore: Send + Sync {
    /// RFC 3339: payments from here on are looked at. `None` before the
    /// first look.
    fn since(&self) -> Option<String>;
    /// The payment ids already handled, oldest first, each once.
    fn handled(&self) -> Vec<String>;
    /// Keeps the cursor and the handled ids in place of what was kept.
    fn save(&self, since: &str, handled: &[String]);
}

/// Payments made this long before the cursor are looked at again, in case
/// the point of sale was slow to list them (decision 15).
#[cfg(any(feature = "square", feature = "clover"))]
pub(crate) const OVERLAP_SECONDS: i64 = 120;

/// The most payment ids remembered as handled (decision 15).
#[cfg(any(feature = "square", feature = "clover"))]
const REMEMBERED: usize = 500;

/// Marks `sale` handled in `cursor`: its id remembered (the newest
/// [`REMEMBERED`] kept), and the cursor moved up to the sale, never back.
/// A cursor that can't be read is replaced by the sale's time; a sale whose
/// time can't be read leaves the cursor where it was.
#[cfg(any(feature = "square", feature = "clover"))]
pub(crate) fn mark_handled(cursor: &dyn CursorStore, sale: &FoundSale) {
    let mut handled = cursor.handled();
    if !handled.contains(&sale.id) {
        handled.push(sale.id.clone());
    }
    let excess = handled.len().saturating_sub(REMEMBERED);
    handled.drain(..excess);
    let saved = cursor.since();
    let since = match (
        saved.as_deref().and_then(crate::time::parse),
        crate::time::parse(&sale.created_at),
    ) {
        (Some(saved), Some(at)) => crate::time::format(saved.max(at)),
        (None, Some(at)) => crate::time::format(at),
        (Some(saved), None) => crate::time::format(saved),
        (None, None) => saved.unwrap_or_else(|| sale.created_at.clone()),
    };
    cursor.save(&since, &handled);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_found_sale_reads_and_writes_the_kotlin_json() {
        let saved = r#"{"id":"pay_1","amountMinor":1388,"currency":"USD","reference":"bP9m","orderId":"ord_1","createdAt":"2026-10-22T16:07:00Z"}"#;
        let sale: FoundSale = serde_json::from_str(saved).unwrap();
        assert_eq!(
            sale.point_of_sale,
            PointOfSale::Square,
            "saved before Clover"
        );
        assert_eq!(sale.device_id, None);
        let clover = FoundSale {
            point_of_sale: PointOfSale::Clover,
            title: Some("Table 4".into()),
            ..sale
        };
        let json = serde_json::to_string(&clover).unwrap();
        assert!(json.contains(r#""pointOfSale":"Clover""#), "{json}");
        assert!(json.contains(r#""title":"Table 4""#), "{json}");
        assert!(!json.contains("deviceId"), "{json}");
        assert_eq!(serde_json::from_str::<FoundSale>(&json).unwrap(), clover);
    }
}
