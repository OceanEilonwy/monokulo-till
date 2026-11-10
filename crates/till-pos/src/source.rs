//! What the till asks of a point of sale (Android: `SaleSource.kt`, and
//! `CursorStore` from `SquareSource.kt`).

use async_trait::async_trait;
use monokulo_client::PointOfSale;
use serde::{Deserialize, Serialize};
use till_types::Money;

use crate::{PosError, StoreError};

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

    /// The refunds made at the point of sale at or after `begin_time` (RFC
    /// 3339), oldest first, whoever made them: the till, or the point of
    /// sale's own app. `None` from a point of sale whose API lists none
    /// (Clover), which is the default.
    async fn refunds_since(
        &self,
        _begin_time: &str,
    ) -> Result<Option<Vec<ListedRefund>>, PosError> {
        Ok(None)
    }

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
    /// Refunds `sale` in full: the refund as the point of sale answered
    /// with it, when it said.
    async fn refund(&self, sale: &FoundSale) -> Result<Option<ListedRefund>, PosError>;
}

till_types::wire_enum!(
    /// A payment's status as Square writes it. Only a `COMPLETED` payment
    /// is a sale; any other, or one Square adds later
    /// ([`Other`](PaymentStatus::Other)), is left alone.
    PaymentStatus {
        Approved = "APPROVED",
        Pending = "PENDING",
        Completed = "COMPLETED",
        Canceled = "CANCELED",
        Failed = "FAILED",
    }
);

till_types::wire_enum!(
    /// A refund's status as Square writes it. One Square adds later
    /// ([`Other`](RefundStatus::Other)) is neither pending nor done: the
    /// till records it as not yet done, but it never holds the refunds
    /// listing's cursor back (core.md decision 170).
    RefundStatus {
        Pending = "PENDING",
        Completed = "COMPLETED",
        Rejected = "REJECTED",
        Failed = "FAILED",
    }
);

/// A refund of a payment, as the point of sale lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedRefund {
    /// The point of sale's id for the refund.
    pub id: String,
    /// The refunded payment's id ([`FoundSale::id`]).
    pub payment_id: String,
    /// In the currency's minor unit.
    pub amount_minor: i64,
    /// As the point of sale wrote it (Square's `COMPLETED`).
    pub status: RefundStatus,
    /// RFC 3339, when the point of sale says.
    pub created_at: Option<String>,
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
    /// What it was for: kept as the Kotlin `FoundSale` kept it, as
    /// `amountMinor` (in the currency's minor unit, cents) and `currency`.
    #[serde(flatten, with = "till_types::money::camel")]
    pub amount: Money,
    /// What the point of sale printed or showed for it ("bP9m"), if
    /// anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// The point of sale's order, for the receipt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    /// RFC 3339.
    pub created_at: String,
    /// When the point of sale last changed the payment, RFC 3339, when it
    /// says (Square's `updated_at`; Clover's listing gives none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// The point of sale's status for the payment (Square's `COMPLETED`),
    /// when it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<PaymentStatus>,
    /// The device that took it, when the point of sale says: to show it on
    /// its own till only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// What the point of sale calls that device ("Front iPad"), when it
    /// says: Square's `device_details.device_name`. Clover's payments name
    /// only the device's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// Rung up as cash at the point of sale (Square's `CASH` tender), not
    /// as the Monero tender (core.md decision 116). The till never refunds
    /// such a sale through the point of sale: the money was never taken by
    /// card.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cash: bool,
    /// The name the cashier gave the order ("Table 4"), when the point of
    /// sale has one: Clover's title; Square leaves it `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The point of sale that took it, so a shift's saved sales keep naming
    /// it after the till changes point of sale. Square, the Kotlin
    /// property's default, is left out, as kotlinx.serialization left it
    /// out, and a sale without one (saved before Clover joined) reads as
    /// Square's. Written as Kotlin named the enum's constants: `"Square"`,
    /// `"Clover"`.
    #[serde(
        default = "square",
        skip_serializing_if = "is_square",
        with = "kotlin_name"
    )]
    pub point_of_sale: PointOfSale,
}

fn square() -> PointOfSale {
    PointOfSale::Square
}

fn is_square(pos: &PointOfSale) -> bool {
    *pos == PointOfSale::Square
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
///
/// A read that fails is a [`StoreError`], never "no cursor yet": a source
/// that took it for one would start the cursor afresh over the kept one,
/// and forget the payments already handled.
pub trait CursorStore: Send + Sync {
    /// RFC 3339: payments from here on are looked at. `None` before the
    /// first look.
    fn since(&self) -> Result<Option<String>, StoreError>;
    /// The payment ids already handled, oldest first, each once.
    fn handled(&self) -> Result<Vec<String>, StoreError>;
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

/// What `sale` is remembered as once handled: its id, with `@` and its
/// `updated_at` after it when the point of sale gives one, so a payment the
/// point of sale changes since is given again ([`is_handled`]).
#[cfg(any(feature = "square", feature = "clover"))]
pub(crate) fn handled_marker(sale: &FoundSale) -> String {
    match &sale.updated_at {
        Some(updated) => format!("{}@{updated}", sale.id),
        None => sale.id.clone(),
    }
}

/// Whether `sale` is among `handled` as it is now: by its marker, or by its
/// bare id (remembered before markers carried the update time, or from a
/// point of sale that gives none).
#[cfg(any(feature = "square", feature = "clover"))]
pub(crate) fn is_handled(handled: &[String], sale: &FoundSale) -> bool {
    handled.contains(&sale.id) || handled.contains(&handled_marker(sale))
}

/// Marks `sale` handled in `cursor`: its marker ([`handled_marker`]) remembered (the newest
/// [`REMEMBERED`] kept), and the cursor moved up to the sale, never back.
/// A cursor whose time can't be parsed is replaced by the sale's time; a
/// sale whose time can't be read leaves the cursor where it was. A cursor
/// the store couldn't read is left alone: the sale is found again on the
/// next look, and its order is the same one (its idempotency key).
#[cfg(any(feature = "square", feature = "clover"))]
pub(crate) fn mark_handled(cursor: &dyn CursorStore, sale: &FoundSale) {
    let (Ok(mut handled), Ok(saved)) = (cursor.handled(), cursor.since()) else {
        return;
    };
    let marker = handled_marker(sale);
    if !handled.contains(&marker) {
        handled.push(marker);
    }
    let excess = handled.len().saturating_sub(REMEMBERED);
    handled.drain(..excess);
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
        assert_eq!(
            serde_json::to_string(&sale).unwrap(),
            saved,
            "Square, the default, left out as Kotlin left it out"
        );
        let clover = FoundSale {
            point_of_sale: PointOfSale::Clover,
            title: Some("Table 4".into()),
            ..sale
        };
        let json = serde_json::to_string(&clover).unwrap();
        assert!(json.contains(r#""pointOfSale":"Clover""#), "{json}");
        assert!(json.contains(r#""title":"Table 4""#), "{json}");
        assert!(!json.contains("deviceId"), "{json}");
        assert!(!json.contains("cash"), "{json}");
        assert_eq!(serde_json::from_str::<FoundSale>(&json).unwrap(), clover);
    }

    #[test]
    fn a_cash_sale_keeps_its_device_and_that_it_was_cash() {
        let sale = FoundSale {
            id: "pay_2".into(),
            amount: Money::new(2450, till_types::CurrencyCode::USD),
            reference: None,
            order_id: None,
            created_at: "2026-11-09T16:07:00Z".into(),
            updated_at: None,
            status: None,
            device_id: Some("dev_front".into()),
            device_name: Some("Front iPad".into()),
            cash: true,
            title: None,
            point_of_sale: PointOfSale::Square,
        };
        let json = serde_json::to_string(&sale).unwrap();
        assert!(json.contains(r#""deviceName":"Front iPad""#), "{json}");
        assert!(json.contains(r#""cash":true"#), "{json}");
        assert_eq!(serde_json::from_str::<FoundSale>(&json).unwrap(), sale);
    }
}
