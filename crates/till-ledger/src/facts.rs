//! Facts (docs/ledger.md §4.1): what the sources said (shared) and what
//! this device did (local). Each is kept in the journal as its `kind` and a
//! versioned JSON body (`"v"`).

use monokulo_client::{PointOfSale, SalePhase};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use till_types::Money;

use crate::ids::{DeviceId, OrderId, PaymentId, SaleKey};
use crate::model::{LocalMarks, SharedSale};

/// The body version this build writes and reads. A body with a larger `v`
/// is kept and skipped, as is a kind this build doesn't know.
pub const FACT_VERSION: u32 = 1;

/// How a POS payment was tendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tender {
    /// The Monero tender (Square's external "Monero", Clover's tender).
    Monero,
    /// Cash at the point of sale (core.md decision 116).
    Cash,
}

/// A payment as the POS listed it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaymentSeen {
    pub pos: PointOfSale,
    pub payment: PaymentId,
    /// Journalled as `amount_minor` and `currency`.
    #[serde(flatten, with = "till_types::money::snake")]
    pub amount: Money,
    pub tender: Tender,
    /// The POS's status for it (Square's `COMPLETED`), as its text.
    pub status: till_pos::PaymentStatus,
    #[serde(default)]
    pub device_id: Option<DeviceId>,
    #[serde(default)]
    pub device_name: Option<String>,
    /// What the POS printed or showed for it ("bP9m").
    #[serde(default)]
    pub receipt_number: Option<String>,
    /// The POS's order, for the receipt.
    #[serde(default)]
    pub order_ref: Option<String>,
    /// The name the cashier gave the order (Clover's title).
    #[serde(default)]
    pub title: Option<String>,
    /// As the POS gave it (journalled as its text).
    pub created_at: till_pos::Rfc3339Instant,
    /// Unix ms: the later one wins.
    pub updated_at: i64,
}

impl PaymentSeen {
    pub fn sale(&self) -> SaleKey {
        SaleKey::pos(self.pos, self.payment.clone())
    }

    /// The order two listings of one payment are joined by: the later
    /// `updated_at`, then (for the join not to depend on arrival order) the
    /// content.
    pub(crate) fn rank(&self) -> (i64, String) {
        (
            self.updated_at,
            serde_json::to_string(self).expect("a payment always serialises"),
        )
    }
}

/// One read of the payment's order at the POS: its receipt, or why there
/// is none, and the customer's name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OrderRead {
    pub sale: SaleKey,
    /// The payment's `updated_at` (Unix ms) when its order was read: a read
    /// for a later one replaces an earlier one.
    pub updated_at: i64,
    /// The receipt as `orf_codec::encode_text` writes it.
    #[serde(default)]
    pub receipt: Option<String>,
    /// Why there is no receipt, when there isn't.
    #[serde(default)]
    pub why: Option<String>,
    #[serde(default)]
    pub customer_name: Option<String>,
}

impl OrderRead {
    /// `receipt` encoded for the journal, with why when it can't be.
    pub fn new(
        sale: SaleKey,
        updated_at: i64,
        receipt: Result<&orf_codec::Receipt, String>,
        customer_name: Option<String>,
    ) -> OrderRead {
        let (receipt, why) = match receipt.map(orf_codec::encode_text) {
            Ok(Ok(text)) => (Some(text), None),
            Ok(Err(e)) => (None, Some(e.to_string())),
            Err(why) => (None, Some(why)),
        };
        OrderRead {
            sale,
            updated_at,
            receipt,
            why,
            customer_name,
        }
    }
}

/// A refund's status at the POS.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RefundStatus {
    Pending,
    Failed,
    Rejected,
    Completed,
}

impl RefundStatus {
    /// The point of sale's status, as the ledger keeps it. One this build
    /// doesn't know is kept as pending, not done (core.md decision 170).
    pub fn of_pos(status: &till_pos::RefundStatus) -> RefundStatus {
        match status {
            till_pos::RefundStatus::Completed => RefundStatus::Completed,
            till_pos::RefundStatus::Failed => RefundStatus::Failed,
            till_pos::RefundStatus::Rejected => RefundStatus::Rejected,
            till_pos::RefundStatus::Pending | till_pos::RefundStatus::Other(_) => {
                RefundStatus::Pending
            }
        }
    }

    /// How far it got: a later status of the same refund outranks an
    /// earlier one, and a completed refund outranks any other.
    pub(crate) fn rank(self) -> u8 {
        match self {
            RefundStatus::Pending => 1,
            RefundStatus::Failed | RefundStatus::Rejected => 2,
            RefundStatus::Completed => 3,
        }
    }
}

/// A refund the POS listed for a sale's payment.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RefundSeen {
    pub sale: SaleKey,
    pub refund_id: String,
    pub amount_minor: i64,
    pub status: RefundStatus,
}

/// The order create-order made (or, on a till without a POS, the order a
/// Monokulo event or read described). A sale has one (core.md decision
/// 167); a body written before then may carry an `attempt`, which is
/// ignored (decision 173).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OrderCreated {
    pub sale: SaleKey,
    pub order_id: OrderId,
    /// Its `self` link: where to read it again.
    #[serde(default)]
    pub url: Option<String>,
    /// The plain payment URI.
    pub payment_uri: String,
    pub address: String,
    pub xmr: String,
    pub amount: String,
    pub currency: String,
    #[serde(default)]
    pub reference: Option<String>,
    /// Unix seconds.
    pub expires_at: i64,
    #[serde(default)]
    pub confirmations_required: Option<u32>,
    /// The device that made it, as Monokulo names it (an event's
    /// `device_name`), for a sale known only from Monokulo.
    #[serde(default)]
    pub device_name: Option<String>,
    /// Where its `renew-order` form goes (New code), when Monokulo offers
    /// it (decision 171).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renew: Option<String>,
}

/// The price and expiry an order had when Monokulo said something about
/// it: a renewal gives the order a new one (core.md decision 169).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Quote {
    /// Unix seconds.
    pub expires_at: i64,
    pub piconero: u64,
}

/// What Monokulo said about an order, from an event or a read.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderObserved {
    pub order_id: OrderId,
    pub phase: SalePhase,
    #[serde(default)]
    pub confirmations: Option<u32>,
    #[serde(default)]
    pub is_final: bool,
    /// Piconero received, for a partly paid order.
    #[serde(default)]
    pub received: Option<u64>,
    /// Paid more than asked (`overpaid`).
    #[serde(default)]
    pub overpaid: bool,
    /// The quote the order had when Monokulo said it; `None` in a body
    /// written before quotes were recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<Quote>,
    /// Where its `renew-order` form goes, from a read that offers it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renew: Option<String>,
}

/// What an alert is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AlertKind {
    /// A sale to show the customer the code for.
    NewSale,
    /// Paid while the app was behind: a notice.
    Paid,
    /// Paid while the app was in front: a haptic and the paid moment.
    PaidMoment,
    /// Its code ran out, it was cancelled or its payment reversed.
    Ended,
}

impl AlertKind {
    pub const ALL: [AlertKind; 4] = [
        AlertKind::NewSale,
        AlertKind::Paid,
        AlertKind::PaidMoment,
        AlertKind::Ended,
    ];

    pub fn wire(self) -> &'static str {
        match self {
            AlertKind::NewSale => "new_sale",
            AlertKind::Paid => "paid",
            AlertKind::PaidMoment => "paid_moment",
            AlertKind::Ended => "ended",
        }
    }

    pub fn from_wire(text: &str) -> Option<AlertKind> {
        AlertKind::ALL.into_iter().find(|k| k.wire() == text)
    }
}

/// Which process wrote: the app, or the iOS Notification Service Extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Process {
    App,
    Extension,
}

impl Process {
    pub fn wire(self) -> &'static str {
        match self {
            Process::App => "app",
            Process::Extension => "extension",
        }
    }

    pub fn from_wire(text: &str) -> Option<Process> {
        [Process::App, Process::Extension]
            .into_iter()
            .find(|p| p.wire() == text)
    }
}

/// `alerts.by`, and a snapshot's: the process's wire name, as the column
/// holds it. (Its own serde, in facts' bodies, is the variant's name.)
impl ToSql for Process {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.wire()))
    }
}

impl FromSql for Process {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Process> {
        let text = value.as_str()?;
        Process::from_wire(text).ok_or_else(|| FromSqlError::Other(unknown("process", text)))
    }
}

/// Where an alert stands (`alerts.state`, §6): left for a process to claim,
/// claimed and not yet delivered, or delivered. Kept as `due`, `pending`
/// and `posted`, in the column and in a snapshot's alerts alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertState {
    Due,
    Pending,
    Posted,
}

impl AlertState {
    pub const ALL: [AlertState; 3] = [AlertState::Due, AlertState::Pending, AlertState::Posted];

    pub fn as_str(self) -> &'static str {
        match self {
            AlertState::Due => "due",
            AlertState::Pending => "pending",
            AlertState::Posted => "posted",
        }
    }

    pub fn from_wire(text: &str) -> Option<AlertState> {
        AlertState::ALL.into_iter().find(|s| s.as_str() == text)
    }
}

impl ToSql for AlertState {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for AlertState {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<AlertState> {
        let text = value.as_str()?;
        AlertState::from_wire(text).ok_or_else(|| FromSqlError::Other(unknown("alert state", text)))
    }
}

fn unknown(what: &str, text: &str) -> Box<dyn std::error::Error + Send + Sync> {
    format!("not an {what}: {text:?}").into()
}

/// A snapshot alert's `by`, as `alerts.by` holds it (`app`, `extension`).
/// A name this build doesn't know reads as no process: it only says who
/// claimed it.
mod process_wire {
    use serde::{Deserialize, Deserializer, Serializer};

    use super::Process;

    pub fn serialize<S: Serializer>(by: &Option<Process>, s: S) -> Result<S::Ok, S::Error> {
        match by {
            Some(process) => s.serialize_some(process.wire()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Process>, D::Error> {
        Ok(Option::<String>::deserialize(d)?
            .as_deref()
            .and_then(Process::from_wire))
    }
}

/// Something the cashier asked this till to do about a sale (§7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Action {
    MakeOrder,
    TakeInMonero,
    NewCode,
    Refund,
}

/// How an action ended.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Outcome {
    Done,
    /// It failed, and why, in words for the cashier.
    Failed(String),
}

/// A local mark on one sale (§5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MarkKind {
    Closed,
    Reopened,
    KeptAsCash,
    PaidInCash,
    RefundPending,
    RefundConfirmed,
    /// This device's clock says the code ran out before Monokulo did.
    ClockLapsed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ShiftMark {
    Started,
    Stopped,
}

/// One fact. Shared facts are what the sources said; local facts what
/// this device did. Times in local facts are this device's, Unix ms.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Fact {
    PosPaymentSeen(PaymentSeen),
    PosOrderRead(OrderRead),
    PosRefundSeen(RefundSeen),
    OrderCreated(OrderCreated),
    OrderObserved(OrderObserved),

    AlertClaimed {
        sale: SaleKey,
        kind: AlertKind,
        notification_id: String,
        by: Process,
        at: i64,
    },
    AlertPosted {
        sale: SaleKey,
        kind: AlertKind,
    },
    ActionAsked {
        sale: SaleKey,
        action: Action,
        at: i64,
    },
    ActionDone {
        sale: SaleKey,
        action: Action,
        outcome: Outcome,
        at: i64,
    },
    Mark {
        sale: SaleKey,
        mark: MarkKind,
        at: i64,
    },
    /// `None` removes the setting.
    SettingChanged {
        name: String,
        value: Option<String>,
        at: i64,
    },
    ShiftMarked {
        mark: ShiftMark,
        at: i64,
    },
    /// The app is in front until `until` (Unix ms).
    Foreground {
        until: i64,
    },
    Imported {
        from: String,
        count: u32,
    },
    /// A compacted sale: its shared and local parts (§12).
    Snapshot(Box<SnapshotState>),
}

/// What a [`Fact::Snapshot`] keeps of one sale.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotState {
    pub sale: SaleKey,
    pub shared: SharedSale,
    pub marks: LocalMarks,
    pub rung_at: i64,
    /// The journal's last seq it stands for.
    pub applied_seq: i64,
    #[serde(default)]
    pub alerts: Vec<SnapshotAlert>,
}

/// One of a compacted sale's `alerts` rows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotAlert {
    pub kind: AlertKind,
    pub state: AlertState,
    pub claimed_at: Option<i64>,
    #[serde(default, with = "process_wire")]
    pub by: Option<Process>,
}

/// Which sale a fact is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum About {
    Sale(SaleKey),
    /// A Monokulo order, found through `orders` (or kept pending).
    Order(OrderId),
    Nothing,
}

#[derive(Serialize, Deserialize)]
struct Versioned<T> {
    v: u32,
    #[serde(flatten)]
    body: T,
}

impl Fact {
    /// The journal's `kind`.
    pub fn kind(&self) -> &'static str {
        match self {
            Fact::PosPaymentSeen(_) => "PosPaymentSeen",
            Fact::PosOrderRead(_) => "PosOrderRead",
            Fact::PosRefundSeen(_) => "PosRefundSeen",
            Fact::OrderCreated(_) => "OrderCreated",
            Fact::OrderObserved(_) => "OrderObserved",
            Fact::AlertClaimed { .. } => "AlertClaimed",
            Fact::AlertPosted { .. } => "AlertPosted",
            Fact::ActionAsked { .. } => "ActionAsked",
            Fact::ActionDone { .. } => "ActionDone",
            Fact::Mark { .. } => "Mark",
            Fact::SettingChanged { .. } => "SettingChanged",
            Fact::ShiftMarked { .. } => "ShiftMarked",
            Fact::Foreground { .. } => "Foreground",
            Fact::Imported { .. } => "Imported",
            Fact::Snapshot(_) => "Snapshot",
        }
    }

    /// A shared fact: what a source said. Only these enter `SharedSale`.
    pub fn is_shared(&self) -> bool {
        matches!(
            self,
            Fact::PosPaymentSeen(_)
                | Fact::PosOrderRead(_)
                | Fact::PosRefundSeen(_)
                | Fact::OrderCreated(_)
                | Fact::OrderObserved(_)
        )
    }

    pub fn about(&self) -> About {
        match self {
            Fact::PosPaymentSeen(p) => About::Sale(p.sale()),
            Fact::PosOrderRead(r) => About::Sale(r.sale.clone()),
            Fact::PosRefundSeen(r) => About::Sale(r.sale.clone()),
            Fact::OrderCreated(o) => About::Sale(o.sale.clone()),
            Fact::OrderObserved(o) => About::Order(o.order_id.clone()),
            Fact::AlertClaimed { sale, .. }
            | Fact::AlertPosted { sale, .. }
            | Fact::ActionAsked { sale, .. }
            | Fact::ActionDone { sale, .. }
            | Fact::Mark { sale, .. } => About::Sale(sale.clone()),
            Fact::Snapshot(s) => About::Sale(s.sale.clone()),
            Fact::SettingChanged { .. }
            | Fact::ShiftMarked { .. }
            | Fact::Foreground { .. }
            | Fact::Imported { .. } => About::Nothing,
        }
    }

    /// The journal's `body`: the fact's fields and `"v"`.
    pub fn body(&self) -> Vec<u8> {
        fn json<T: Serialize>(body: &T) -> Vec<u8> {
            serde_json::to_vec(&Versioned {
                v: FACT_VERSION,
                body,
            })
            .expect("a fact always serialises")
        }
        // Externally tagged: {"Kind": {...}}; the body is the inner value.
        let value = serde_json::to_value(self).expect("a fact always serialises");
        match value {
            serde_json::Value::Object(map) if map.len() == 1 => {
                let (_, inner) = map.into_iter().next().expect("one entry");
                match inner {
                    serde_json::Value::Object(_) => json(&inner),
                    other => json(&serde_json::json!({ "value": other })),
                }
            }
            other => json(&serde_json::json!({ "value": other })),
        }
    }

    /// The fact a journal row holds, or `None` for a kind or version this
    /// build doesn't know (the row is kept and skipped).
    pub fn decode(kind: &str, body: &[u8]) -> Option<Fact> {
        let mut value: serde_json::Value = serde_json::from_slice(body).ok()?;
        let map = value.as_object_mut()?;
        let v = map.remove("v")?.as_u64()?;
        if v > FACT_VERSION as u64 {
            return None;
        }
        fn parse<T: DeserializeOwned>(value: serde_json::Value) -> Option<T> {
            serde_json::from_value(value).ok()
        }
        let tagged = serde_json::json!({ kind: value });
        let fact: Fact = parse(tagged)?;
        Some(fact)
    }

    /// The journal's `hash`: SHA-256 of the kind, the sale key and the body.
    pub fn hash(&self, sale_key: Option<&str>) -> Vec<u8> {
        let mut hash = Sha256::new();
        hash.update(self.kind().as_bytes());
        hash.update(b"\0");
        hash.update(sale_key.unwrap_or("").as_bytes());
        hash.update(b"\0");
        hash.update(self.body());
        hash.finalize().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alerts table's text and a snapshot's alerts are as they were
    /// before the states had a type.
    #[test]
    fn alert_states_and_processes_keep_their_text() {
        for (state, text) in [
            (AlertState::Due, "due"),
            (AlertState::Pending, "pending"),
            (AlertState::Posted, "posted"),
        ] {
            assert_eq!(state.as_str(), text);
            assert_eq!(AlertState::from_wire(text), Some(state));
            assert_eq!(serde_json::to_value(state).unwrap(), text);
        }
        let kept = r#"{"kind":"NewSale","state":"pending","claimed_at":5,"by":"extension"}"#;
        let alert: SnapshotAlert = serde_json::from_str(kept).unwrap();
        assert_eq!(alert.state, AlertState::Pending);
        assert_eq!(alert.by, Some(Process::Extension));
        assert_eq!(serde_json::to_string(&alert).unwrap(), kept);
        let unclaimed: SnapshotAlert =
            serde_json::from_str(r#"{"kind":"Paid","state":"posted","claimed_at":null,"by":null}"#)
                .unwrap();
        assert_eq!(unclaimed.by, None);
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let (state, by): (String, String) = conn
            .query_row(
                "SELECT ?1, ?2",
                rusqlite::params![AlertState::Posted, Process::App],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((state.as_str(), by.as_str()), ("posted", "app"));
        let read: AlertState = conn.query_row("SELECT 'due'", [], |r| r.get(0)).unwrap();
        assert_eq!(read, AlertState::Due);
        assert!(
            conn.query_row("SELECT 'post'", [], |r| r.get::<_, AlertState>(0))
                .is_err()
        );
    }

    pub(crate) fn every_kind() -> Vec<Fact> {
        let sale = SaleKey::pos(PointOfSale::Square, "p1");
        vec![
            Fact::PosPaymentSeen(PaymentSeen {
                pos: PointOfSale::Square,
                payment: "p1".into(),
                amount: Money::new(1388, till_types::CurrencyCode::USD),
                tender: Tender::Monero,
                status: "COMPLETED".into(),
                device_id: Some("dev".into()),
                device_name: Some("Front iPad".into()),
                receipt_number: Some("bP9m".into()),
                order_ref: Some("o1".into()),
                title: None,
                created_at: "2026-11-09T10:00:00Z".parse().unwrap(),
                updated_at: 1,
            }),
            Fact::PosOrderRead(OrderRead {
                sale: sale.clone(),
                updated_at: 1,
                receipt: None,
                why: Some("no order".into()),
                customer_name: Some("Sam".into()),
            }),
            Fact::PosRefundSeen(RefundSeen {
                sale: sale.clone(),
                refund_id: "r".into(),
                amount_minor: 1388,
                status: RefundStatus::Pending,
            }),
            Fact::OrderObserved(OrderObserved {
                order_id: "ord".into(),
                phase: SalePhase::Seen,
                confirmations: Some(0),
                is_final: false,
                received: None,
                overpaid: false,
                quote: Some(Quote {
                    expires_at: 900,
                    piconero: 150_000_000_000,
                }),
                renew: Some("https://shop.example/api/v1/orders/ord".into()),
            }),
            Fact::Mark {
                sale: sale.clone(),
                mark: MarkKind::Closed,
                at: 5,
            },
            Fact::SettingChanged {
                name: "cash.monero".into(),
                value: Some("false".into()),
                at: 4,
            },
            Fact::Foreground { until: 10 },
            Fact::Imported {
                from: "shift".into(),
                count: 2,
            },
            Fact::AlertClaimed {
                sale,
                kind: AlertKind::NewSale,
                notification_id: "n".into(),
                by: Process::Extension,
                at: 3,
            },
        ]
    }

    #[test]
    fn every_fact_reads_back_from_its_kind_and_body() {
        for fact in every_kind() {
            let body = fact.body();
            let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["v"], 1, "{}", fact.kind());
            assert_eq!(Fact::decode(fact.kind(), &body), Some(fact.clone()));
        }
    }

    /// A journalled payment keeps its body (and so its hash): the amount
    /// as `amount_minor` and `currency`, the status as its text.
    #[test]
    fn a_payments_body_is_as_journalled_before() {
        let body = r#"{"v":1,"amount_minor":1388,"created_at":"2026-11-09T10:00:00Z","currency":"USD","device_id":"dev","device_name":"Front iPad","order_ref":"o1","payment":"p1","pos":"square","receipt_number":"bP9m","status":"COMPLETED","tender":"Monero","title":null,"updated_at":1}"#;
        let fact = Fact::decode("PosPaymentSeen", body.as_bytes()).unwrap();
        assert_eq!(fact, every_kind()[0]);
        assert_eq!(String::from_utf8(fact.body()).unwrap(), body);
    }

    #[test]
    fn unknown_kinds_and_newer_versions_are_skipped() {
        assert_eq!(Fact::decode("SomethingNew", br#"{"v":1}"#), None);
        let fact = Fact::Foreground { until: 10 };
        assert_eq!(Fact::decode("Foreground", br#"{"v":2,"until":10}"#), None);
        assert_eq!(
            Fact::decode("Foreground", br#"{"v":1,"until":10,"extra":true}"#),
            Some(fact)
        );
        assert_eq!(Fact::decode("Foreground", b"not json"), None);
    }

    /// An `OrderCreated` journalled before core.md decision 167 carries its
    /// `attempt`: read as the order it names, the attempt ignored; and an
    /// `OrderObserved` from before quotes has none.
    #[test]
    fn facts_written_before_one_order_per_sale_still_read() {
        let body = br#"{"v":1,"sale":"sq:p1","order_id":"o2","attempt":2,"url":null,
            "payment_uri":"monero:8a?tx_amount=0.15","address":"8a","xmr":"0.15",
            "amount":"24.50","currency":"USD","reference":null,"expires_at":900,
            "confirmations_required":10,"device_name":null}"#;
        let Some(Fact::OrderCreated(o)) = Fact::decode("OrderCreated", body) else {
            panic!("an order")
        };
        assert_eq!((o.order_id.as_str(), o.renew), ("o2", None));
        let body = br#"{"v":1,"order_id":"o2","phase":"Expired","confirmations":0,
            "is_final":false,"received":null,"overpaid":false}"#;
        let Some(Fact::OrderObserved(o)) = Fact::decode("OrderObserved", body) else {
            panic!("an observation")
        };
        assert_eq!(o.quote, None);
    }

    #[test]
    fn the_hash_covers_kind_sale_and_body() {
        let a = Fact::Foreground { until: 10 };
        assert_eq!(a.hash(None), a.hash(None));
        assert_ne!(a.hash(None), a.hash(Some("sq:1")));
        assert_ne!(a.hash(None), Fact::Foreground { until: 11 }.hash(None));
    }
}
