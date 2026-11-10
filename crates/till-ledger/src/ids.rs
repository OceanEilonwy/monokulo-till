//! Identities (docs/ledger.md §3): newtypes, so one sale's order can't be
//! applied to another by mistake.

use std::fmt;

use monokulo_client::PointOfSale;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use till_types::text_id;

text_id!(
    /// The point of sale's id for a payment.
    PaymentId
);
text_id!(
    /// Monokulo's id for an order: what create-order returns.
    OrderId
);
/// The point of sale's id for a device (a Square register, a Clover
/// station): till-pos's, one type from the payment to the journal.
pub use till_pos::PosDeviceId as DeviceId;

/// One sale. Written as text in the database: `sq:<payment>` and
/// `cl:<payment>` for a sale this till can see in its point of sale,
/// `mk:<order>` for one known only from Monokulo.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SaleKey {
    /// A sale this till can see in its POS.
    Pos {
        pos: PointOfSale,
        payment: PaymentId,
    },
    /// A sale known only from Monokulo, on a till with no POS sign-in.
    /// Never merged with a `Pos` key.
    Order(OrderId),
}

impl SaleKey {
    pub fn pos(pos: PointOfSale, payment: impl Into<PaymentId>) -> SaleKey {
        SaleKey::Pos {
            pos,
            payment: payment.into(),
        }
    }

    pub fn order(order: impl Into<OrderId>) -> SaleKey {
        SaleKey::Order(order.into())
    }

    /// The text kept in the database.
    pub fn text(&self) -> String {
        match self {
            SaleKey::Pos {
                pos: PointOfSale::Square,
                payment,
            } => format!("sq:{payment}"),
            SaleKey::Pos {
                pos: PointOfSale::Clover,
                payment,
            } => format!("cl:{payment}"),
            SaleKey::Order(order) => format!("mk:{order}"),
        }
    }

    /// The key `text` writes, or `None` for text no build wrote.
    pub fn parse(text: &str) -> Option<SaleKey> {
        let (prefix, id) = text.split_once(':')?;
        if id.is_empty() {
            return None;
        }
        Some(match prefix {
            "sq" => SaleKey::pos(PointOfSale::Square, id),
            "cl" => SaleKey::pos(PointOfSale::Clover, id),
            "mk" => SaleKey::order(id),
            _ => return None,
        })
    }

    /// The POS payment, for a POS sale.
    pub fn payment(&self) -> Option<&PaymentId> {
        match self {
            SaleKey::Pos { payment, .. } => Some(payment),
            SaleKey::Order(_) => None,
        }
    }

    /// The idempotency key of the sale's order, for a POS sale.
    pub fn order_key(&self) -> Option<OrderKey> {
        Some(OrderKey::new(self.payment()?.clone()))
    }
}

impl Ord for SaleKey {
    fn cmp(&self, other: &SaleKey) -> std::cmp::Ordering {
        self.text().cmp(&other.text())
    }
}

impl PartialOrd for SaleKey {
    fn partial_cmp(&self, other: &SaleKey) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for SaleKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text())
    }
}

impl Serialize for SaleKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.text())
    }
}

impl<'de> Deserialize<'de> for SaleKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<SaleKey, D::Error> {
        let text = String::deserialize(d)?;
        SaleKey::parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("not a sale key: {text}")))
    }
}

/// The idempotency key of a sale's Monokulo order: the key every till has
/// always used (`Till::idempotency_key`). One point-of-sale payment is one
/// order: New code renews it (core.md decision 167).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderKey {
    payment: PaymentId,
}

impl OrderKey {
    pub fn new(payment: PaymentId) -> OrderKey {
        OrderKey { payment }
    }

    pub fn payment(&self) -> &PaymentId {
        &self.payment
    }

    /// The key sent to create-order: `sq:` and 40 hex digits of the
    /// payment id's SHA-256 (for Clover's payments too).
    pub fn key(&self) -> String {
        let hash = Sha256::digest(self.payment.0.as_bytes());
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        format!("sq:{}", &hex[..40])
    }
}

impl fmt::Display for OrderKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.key())
    }
}

/// The rows of one Monokulo connection and POS account (§4): a hash of the
/// store's id and the POS merchant's, so signing out, forgetting the store
/// or changing the POS can delete exactly its rows.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Scope(pub String);

impl Scope {
    /// The scope of `store_id` with `merchant` (the POS account), or with no
    /// POS sign-in.
    pub fn of(store_id: &str, pos: Option<(PointOfSale, &str)>) -> Scope {
        let mut hash = Sha256::new();
        hash.update(b"till-ledger scope\0");
        hash.update(store_id.as_bytes());
        hash.update(b"\0");
        if let Some((pos, merchant)) = pos {
            hash.update(pos.wire().as_bytes());
            hash.update(b"\0");
            hash.update(merchant.as_bytes());
        }
        let digest = hash.finalize();
        Scope(digest[..16].iter().map(|b| format!("{b:02x}")).collect())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Till::idempotency_key`, as it is in till-sales today.
    fn todays_key(payment: &str) -> String {
        let hash = Sha256::digest(payment.as_bytes());
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        format!("sq:{}", &hex[..40])
    }

    #[test]
    fn the_key_is_todays_key() {
        for payment in ["pay_monero", "R2D2", "CLOVER-1"] {
            let key = OrderKey::new(PaymentId::from(payment));
            assert_eq!(key.key(), todays_key(payment));
            assert_eq!(
                SaleKey::pos(PointOfSale::Square, payment).order_key(),
                Some(key)
            );
        }
        assert_eq!(todays_key("pay_monero").len(), 43);
        assert_eq!(SaleKey::order("ord_1").order_key(), None);
    }

    #[test]
    fn sale_keys_read_back_from_their_text() {
        for key in [
            SaleKey::pos(PointOfSale::Square, "p1"),
            SaleKey::pos(PointOfSale::Clover, "p:2"),
            SaleKey::order("ord_9"),
        ] {
            assert_eq!(SaleKey::parse(&key.text()), Some(key.clone()));
            let json = serde_json::to_string(&key).unwrap();
            assert_eq!(serde_json::from_str::<SaleKey>(&json).unwrap(), key);
        }
        assert_eq!(SaleKey::parse("xx:1"), None);
        assert_eq!(SaleKey::parse("sq:"), None);
        assert_eq!(SaleKey::parse("nothing"), None);
    }

    #[test]
    fn scopes_differ_by_store_and_account() {
        let a = Scope::of("store", Some((PointOfSale::Square, "M1")));
        assert_eq!(a, Scope::of("store", Some((PointOfSale::Square, "M1"))));
        assert_ne!(a, Scope::of("store", Some((PointOfSale::Square, "M2"))));
        assert_ne!(a, Scope::of("store", Some((PointOfSale::Clover, "M1"))));
        assert_ne!(a, Scope::of("store", None));
        assert_ne!(a, Scope::of("other", Some((PointOfSale::Square, "M1"))));
    }
}
