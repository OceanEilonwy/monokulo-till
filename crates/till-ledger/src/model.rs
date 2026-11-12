//! The sale model (docs/ledger.md §5): what the sources said about a sale
//! ([`SharedSale`]), what this device did ([`LocalMarks`]), and the pure
//! merges that fold one fact into each.
//!
//! The shared merge is a join: commutative, associative and idempotent over
//! the same facts, so every till fed the same facts in any order, with
//! repeats, holds the same `SharedSale`, and a stale fact can't move a sale
//! backwards.
//!
//! A sale has one Monokulo order (core.md decision 167). New code renews it:
//! the same order, a new quote (price and expiry, decision 169). An ending
//! belongs to the quote it was seen under; a payment to the order
//! (decision 170).

use std::collections::BTreeMap;

use monokulo_client::{SalePhase, payment_uri};
use serde::{Deserialize, Serialize};

use crate::facts::{
    Action, MarkKind, OrderCreated, OrderObserved, OrderRead, Outcome, PaymentSeen, Quote,
    RefundSeen, RefundStatus,
};
use crate::ids::OrderId;
use crate::{UnixMillis, UnixSeconds};

/// What the sources said about one sale.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "SharedSaleWire")]
pub struct SharedSale {
    pub pos: PosFacts,
    /// The sale's one Monokulo order, once it has one.
    pub order: Option<SaleOrder>,
    /// The POS's refund, if any: beside the order, never instead of it.
    pub refund: Option<PosRefund>,
}

/// A [`SharedSale`] as read: as this build writes it, or with the
/// `attempts` a build before core.md decision 167 wrote (in a compacted
/// sale's snapshot), which become its one order (decision 173).
#[derive(Deserialize)]
struct SharedSaleWire {
    #[serde(default)]
    pos: PosFacts,
    #[serde(default)]
    order: Option<SaleOrder>,
    #[serde(default)]
    refund: Option<PosRefund>,
    #[serde(default)]
    attempts: Vec<SaleOrder>,
}

impl From<SharedSaleWire> for SharedSale {
    fn from(wire: SharedSaleWire) -> SharedSale {
        // The paid one if any (money always shows), else the latest code.
        let order = wire.order.or_else(|| {
            let paid = wire.attempts.iter().find(|a| a.payment.is_paid()).cloned();
            paid.or_else(|| {
                wire.attempts
                    .into_iter()
                    .max_by(|a, b| a.order.stands_over(&b.order))
            })
        });
        SharedSale {
            pos: wire.pos,
            order: order.map(SaleOrder::settled),
            refund: wire.refund,
        }
    }
}

/// The latest payment and order read from the POS.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosFacts {
    pub payment: Option<PaymentSeen>,
    pub order: Option<OrderRead>,
}

/// A sale's Monokulo order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleOrder {
    /// What create-order said, with the latest quote.
    pub order: OrderFacts,
    /// Where its payment is, from [`SaleOrder::observed`] and the quote.
    pub payment: PaymentPhase,
    pub observed: Observations,
}

/// What create-order said about the order, with its latest quote: the
/// price and expiry of its code, which a renewal replaces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderFacts {
    pub order_id: OrderId,
    pub url: Option<String>,
    /// The latest quote's plain payment URI.
    pub payment_uri: String,
    pub address: String,
    /// The latest quote's XMR.
    pub xmr: String,
    #[serde(flatten)]
    pub price: till_types::Price,
    pub reference: Option<String>,
    /// When the latest quote runs out, Unix seconds.
    pub expires_at: UnixSeconds,
    pub confirmations_required: Option<u32>,
    pub device_name: Option<String>,
    /// Where its `renew-order` form goes: New code, when Monokulo offers it
    /// (decision 171).
    #[serde(default)]
    pub renew: Option<String>,
    /// The first quote's expiry, as create-order answered (the earliest of
    /// its answers): observations made before quotes were recorded belong
    /// to it (decision 170). 0 in a body written before then, read as
    /// `expires_at`.
    #[serde(default)]
    pub first_expires_at: UnixSeconds,
    /// The exchange rate its XMR was worked out at, from a wallet backend
    /// (decision 228); `None` for a Monokulo order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_used: Option<till_types::PriceUsed>,
}

impl From<&OrderCreated> for OrderFacts {
    fn from(o: &OrderCreated) -> OrderFacts {
        OrderFacts {
            order_id: o.order_id.clone(),
            url: o.url.clone(),
            payment_uri: o.payment_uri.clone(),
            address: o.address.clone(),
            xmr: o.xmr.clone(),
            price: o.price.clone(),
            reference: o.reference.clone(),
            expires_at: o.expires_at,
            confirmations_required: o.confirmations_required,
            device_name: o.device_name.clone(),
            renew: o.renew.clone(),
            first_expires_at: o.expires_at,
            price_used: o.price_used.clone(),
        }
    }
}

fn max_opt<T: Ord>(a: Option<T>, b: Option<T>) -> Option<T> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

/// The larger of two, by `key`, keeping `mine` on a tie.
fn larger<T: Ord + Clone>(mine: &mut T, theirs: &T) {
    if theirs > mine {
        *mine = theirs.clone();
    }
}

impl OrderFacts {
    /// The first quote's expiry.
    pub fn first_expiry(&self) -> UnixSeconds {
        if self.first_expires_at == UnixSeconds(0) {
            self.expires_at
        } else {
            self.first_expires_at.min(self.expires_at)
        }
    }

    /// Its quote: the expiry first, so the latest wins.
    fn quote(&self) -> (UnixSeconds, &str, &str) {
        (self.expires_at, &self.xmr, &self.payment_uri)
    }

    /// Takes a quote, if it's later than the one it has.
    fn take_quote(&mut self, expires_at: UnixSeconds, xmr: String, payment_uri: String) {
        if (expires_at, xmr.as_str(), payment_uri.as_str()) > self.quote() {
            self.expires_at = expires_at;
            self.xmr = xmr;
            self.payment_uri = payment_uri;
        }
    }

    /// Takes a quote Monokulo gave for the order (an event, a read, a
    /// renewal's answer).
    fn see_quote(&mut self, quote: &Quote) {
        self.take_quote(
            quote.expires_at,
            payment_uri::xmr(quote.piconero),
            payment_uri::build(&self.address, quote.piconero),
        );
    }

    /// Joins another account of the same order: the latest quote, the
    /// earliest first one, and each other part's larger value (a part
    /// known beats one not, as the store's threshold learnt later does).
    fn join(&mut self, other: &OrderFacts) {
        let first = self.first_expiry().min(other.first_expiry());
        self.take_quote(
            other.expires_at,
            other.xmr.clone(),
            other.payment_uri.clone(),
        );
        self.first_expires_at = first;
        self.url = max_opt(self.url.take(), other.url.clone());
        self.confirmations_required =
            max_opt(self.confirmations_required, other.confirmations_required);
        self.device_name = max_opt(self.device_name.take(), other.device_name.clone());
        self.renew = max_opt(self.renew.take(), other.renew.clone());
        self.price_used = max_opt(self.price_used.take(), other.price_used.clone());
        self.reference = max_opt(self.reference.take(), other.reference.clone());
        larger(&mut self.address, &other.address);
        // One order's two accounts carry one price; a known one beats an
        // empty one, as above.
        larger(&mut self.price, &other.price);
    }

    /// Which of two orders made for one sale stands, as `Ordering`: only a
    /// build before core.md decision 167 made two (its New code), and the
    /// later code stands (decision 173). By first expiry, then id.
    fn stands_over(&self, other: &OrderFacts) -> std::cmp::Ordering {
        (self.first_expiry(), &self.order_id).cmp(&(other.first_expiry(), &other.order_id))
    }
}

/// Where the order's payment is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentPhase {
    Waiting,
    Seen,
    Confirming {
        n: u32,
    },
    /// Piconero received.
    Partial {
        received: u64,
    },
    Paid {
        overpaid: bool,
    },
    Expired,
    Cancelled,
    DoubleSpend,
}

impl PaymentPhase {
    /// Monokulo's phase for it.
    pub fn sale_phase(self) -> SalePhase {
        match self {
            PaymentPhase::Waiting => SalePhase::Waiting,
            PaymentPhase::Seen => SalePhase::Seen,
            PaymentPhase::Confirming { .. } => SalePhase::Confirming,
            PaymentPhase::Partial { .. } => SalePhase::Partial,
            PaymentPhase::Paid { .. } => SalePhase::Paid,
            PaymentPhase::Expired => SalePhase::Expired,
            PaymentPhase::Cancelled => SalePhase::Cancelled,
            PaymentPhase::DoubleSpend => SalePhase::DoubleSpend,
        }
    }

    pub fn is_paid(self) -> bool {
        matches!(self, PaymentPhase::Paid { .. })
    }

    /// The code is up: waiting, seen, confirming or part paid.
    pub fn is_live(self) -> bool {
        matches!(
            self,
            PaymentPhase::Waiting
                | PaymentPhase::Seen
                | PaymentPhase::Confirming { .. }
                | PaymentPhase::Partial { .. }
        )
    }
}

/// Everything Monokulo has said about one order, joined: each part only
/// grows, so the order observations arrive in never matters.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observations {
    pub paid: Option<PaidSeen>,
    pub double_spend: bool,
    /// The ending of the latest quote one was seen under, while Monokulo
    /// still watches the order (not `is_final`).
    pub ended: Option<EndedSeen>,
    /// An ending Monokulo called final (`is_final`). Monokulo renews an
    /// order only while it isn't final, so there is at most one, whatever
    /// quote it was seen under: kept apart from [`Self::ended`], it never
    /// gives way to another quote's ending, so progress dropped under it
    /// stays dropped whatever order observations arrive in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_final: Option<FinalEnd>,
    pub seen: bool,
    /// The most confirmations a Confirming observation had.
    pub confirming: Option<u32>,
    /// The most piconero a Partial observation had.
    pub partial: Option<u64>,
    /// The most confirmations any observation had.
    pub confirmations: Option<u32>,
    /// Monokulo said nothing more will happen (`is_final`).
    pub settled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaidSeen {
    pub overpaid: bool,
    /// A Paid observation said `is_final`: Monokulo's own resolution, which
    /// overrides a double spend.
    pub is_final: bool,
}

/// A final ending: expired or, if any final observation said so, cancelled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalEnd {
    pub cancelled: bool,
}

impl FinalEnd {
    fn join(mine: &mut Option<FinalEnd>, cancelled: bool) {
        mine.get_or_insert_default().cancelled |= cancelled;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndedSeen {
    pub cancelled: bool,
    pub is_final: bool,
    /// The expiry of the quote it was seen under; `None` for one observed
    /// before quotes were recorded, which belongs to the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<UnixSeconds>,
}

impl EndedSeen {
    /// Joins another ending in: the later quote's, and both of one quote's.
    fn join(mine: &mut Option<EndedSeen>, theirs: EndedSeen) {
        match mine {
            Some(m) if m.quote == theirs.quote => {
                m.cancelled |= theirs.cancelled;
                m.is_final |= theirs.is_final;
            }
            Some(m) if m.quote > theirs.quote => {}
            _ => *mine = Some(theirs),
        }
    }
}

impl Observations {
    /// Joins one observation in.
    pub fn observe(&mut self, o: &OrderObserved) {
        self.confirmations = max_opt(self.confirmations, o.confirmations);
        self.settled |= o.is_final;
        match o.phase {
            SalePhase::Waiting => {}
            SalePhase::Seen => self.seen = true,
            SalePhase::Confirming => {
                self.confirming = max_opt(self.confirming, Some(o.confirmations.unwrap_or(0)));
            }
            SalePhase::Partial => {
                self.partial = max_opt(self.partial, Some(o.received.unwrap_or(0)));
            }
            SalePhase::Paid => {
                let paid = self.paid.get_or_insert_default();
                paid.overpaid |= o.overpaid;
                paid.is_final |= o.is_final;
            }
            SalePhase::Expired | SalePhase::Cancelled if o.is_final => {
                FinalEnd::join(&mut self.ended_final, o.phase == SalePhase::Cancelled);
            }
            SalePhase::Expired | SalePhase::Cancelled => EndedSeen::join(
                &mut self.ended,
                EndedSeen {
                    cancelled: o.phase == SalePhase::Cancelled,
                    is_final: false,
                    quote: o.quote.as_ref().map(|q| q.expires_at),
                },
            ),
            SalePhase::DoubleSpend => self.double_spend = true,
        }
        self.normalise();
    }

    /// Drops what can no longer change the phase, so a stale observation
    /// (a Seen after Paid) changes nothing and isn't journalled. Once paid,
    /// reversed or finally ended, progress doesn't count; Confirming and
    /// Partial imply Seen. Both only depend on parts that only grow (a
    /// final ending is kept apart, in [`Self::ended_final`], so no other
    /// quote's ending replaces it), so the join stays commutative,
    /// associative and idempotent.
    fn normalise(&mut self) {
        // Kept before final endings had their own place: moved there.
        if let Some(e) = self.ended
            && e.is_final
        {
            FinalEnd::join(&mut self.ended_final, e.cancelled);
            self.ended = None;
        }
        let decided = self.paid.is_some() || self.double_spend || self.ended_final.is_some();
        if decided {
            self.seen = false;
            self.confirming = None;
            self.partial = None;
        } else if self.confirming.is_some() || self.partial.is_some() {
            self.seen = true;
        }
    }

    /// Joins another account of the same order's observations in (a sale
    /// kept from Monokulo alone, or a snapshot).
    fn join(&mut self, other: &Observations) {
        if let Some(p) = other.paid {
            let paid = self.paid.get_or_insert_default();
            paid.overpaid |= p.overpaid;
            paid.is_final |= p.is_final;
        }
        if let Some(e) = other.ended {
            if e.is_final {
                FinalEnd::join(&mut self.ended_final, e.cancelled);
            } else {
                EndedSeen::join(&mut self.ended, e);
            }
        }
        if let Some(f) = other.ended_final {
            FinalEnd::join(&mut self.ended_final, f.cancelled);
        }
        self.double_spend |= other.double_spend;
        self.seen |= other.seen;
        self.confirming = max_opt(self.confirming, other.confirming);
        self.partial = max_opt(self.partial, other.partial);
        self.confirmations = max_opt(self.confirmations, other.confirmations);
        self.settled |= other.settled;
        self.normalise();
    }

    /// The phase the observations add up to, for an order whose latest
    /// quote runs out at `expires_at` and whose first ran out at `first`:
    /// - Paid absorbs everything, except that a double spend needs a final
    ///   Paid (Monokulo's own resolution) to be overridden.
    /// - A double spend stays once seen.
    /// - An ending counts only for the quote it was seen under (one with no
    ///   quote, for the first): a renewed order is waiting again.
    /// - An ending Monokulo calls final (`is_final`) beats progress, under
    ///   whatever quote it was seen (none follows a final ending); one it
    ///   doesn't (it still watches for a late payment) beats only Waiting,
    ///   so a later Seen, Confirming or Partial still shows.
    /// - Otherwise the further phase: Seen < Confirming (more
    ///   confirmations) < Partial (more received).
    pub fn phase(&self, expires_at: UnixSeconds, first: UnixSeconds) -> PaymentPhase {
        if let Some(paid) = self.paid
            && (paid.is_final || !self.double_spend)
        {
            return PaymentPhase::Paid {
                overpaid: paid.overpaid,
            };
        }
        if self.double_spend {
            return PaymentPhase::DoubleSpend;
        }
        if let Some(f) = self.ended_final {
            return if f.cancelled {
                PaymentPhase::Cancelled
            } else {
                PaymentPhase::Expired
            };
        }
        let ended = self
            .ended
            .filter(|e| e.quote.unwrap_or(first) >= expires_at)
            .map(|e| {
                if e.cancelled {
                    PaymentPhase::Cancelled
                } else {
                    PaymentPhase::Expired
                }
            });
        if let Some(received) = self.partial {
            return PaymentPhase::Partial { received };
        }
        if let Some(n) = self.confirming {
            return PaymentPhase::Confirming { n };
        }
        if self.seen {
            return PaymentPhase::Seen;
        }
        ended.unwrap_or(PaymentPhase::Waiting)
    }
}

/// The POS's refund of a sale's payment.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PosRefund {
    pub refund_id: String,
    pub amount_minor: i64,
    pub status: RefundStatus,
}

impl PosRefund {
    fn key(&self) -> (u8, &str, i64) {
        (self.status.rank(), &self.refund_id, self.amount_minor)
    }
}

impl SaleOrder {
    fn new(order: OrderFacts) -> SaleOrder {
        SaleOrder {
            order,
            payment: PaymentPhase::Waiting,
            observed: Observations::default(),
        }
        .settled()
    }

    /// With its phase worked out again from its observations and quote.
    fn settled(mut self) -> SaleOrder {
        self.payment = self
            .observed
            .phase(self.order.expires_at, self.order.first_expiry());
        self
    }

    fn settle(&mut self) {
        self.payment = self
            .observed
            .phase(self.order.expires_at, self.order.first_expiry());
    }

    /// Joins an observation of this order: its quote and what it said.
    fn observe(&mut self, o: &OrderObserved) {
        if let Some(quote) = &o.quote {
            self.order.see_quote(quote);
        }
        self.order.renew = max_opt(self.order.renew.take(), o.renew.clone());
        self.observed.observe(o);
        self.settle();
    }

    /// Joins another account of the same order.
    fn join(&mut self, other: &SaleOrder) {
        self.order.join(&other.order);
        self.observed.join(&other.observed);
        self.settle();
    }
}

impl SharedSale {
    /// Joins a payment the POS listed: the later `updated_at` wins (ties
    /// broken by content, so the join doesn't depend on order).
    pub fn see_payment(&mut self, p: &PaymentSeen) {
        let replace = match &self.pos.payment {
            None => true,
            Some(old) => p.rank() > old.rank(),
        };
        if replace {
            self.pos.payment = Some(p.clone());
        }
    }

    /// Joins an order read: one for a later payment `updated_at` replaces an
    /// earlier one.
    pub fn read_order(&mut self, r: &OrderRead) {
        let replace = match &self.pos.order {
            None => true,
            Some(old) => (r.updated_at, r) > (old.updated_at, old),
        };
        if replace {
            self.pos.order = Some(r.clone());
        }
    }

    /// Joins a refund: a later status of the same refund, or a completed
    /// one over any other.
    pub fn see_refund(&mut self, r: &RefundSeen) {
        let new = PosRefund {
            refund_id: r.refund_id.clone(),
            amount_minor: r.amount_minor,
            status: r.status,
        };
        let replace = match &self.refund {
            None => true,
            Some(old) => new.key() > old.key(),
        };
        if replace {
            self.refund = Some(new);
        }
    }

    /// Joins the order create-order made (or a Monokulo event described).
    pub fn create_order(&mut self, o: &OrderCreated) {
        self.take_order(&SaleOrder::new(OrderFacts::from(o)));
    }

    /// Joins `order`: the same order joins in; another (only a build before
    /// core.md decision 167 made two for one sale) stands only if it's the
    /// later code, and replaces this one with what's known of it.
    fn take_order(&mut self, order: &SaleOrder) {
        match &mut self.order {
            None => self.order = Some(order.clone()),
            Some(mine) if mine.order.order_id == order.order.order_id => mine.join(order),
            Some(mine) => {
                if order.order.stands_over(&mine.order).is_gt() {
                    *mine = order.clone();
                }
            }
        }
    }

    /// Joins an observation of the sale's order; one of another order (one
    /// an older build's New code replaced) changes nothing.
    pub fn observe(&mut self, o: &OrderObserved) {
        if let Some(mine) = &mut self.order
            && mine.order.order_id == o.order_id
        {
            mine.observe(o);
        }
    }

    /// Joins what Monokulo said about `order` in `other` (a sale kept from
    /// Monokulo alone) into this sale's order, when it's that one: the
    /// observations only, since this sale's own create-order answer stands
    /// for the order.
    pub fn take_observations(&mut self, other: &SharedSale, order: &OrderId) {
        let Some(mine) = &mut self.order else {
            return;
        };
        if mine.order.order_id != *order {
            return;
        }
        if let Some(theirs) = other.order.as_ref().filter(|o| o.order.order_id == *order) {
            mine.observed.join(&theirs.observed);
            mine.settle();
        }
    }

    /// Joins another view of the same sale (a snapshot over a row).
    pub fn join(&mut self, other: &SharedSale) {
        if let Some(p) = &other.pos.payment {
            self.see_payment(p);
        }
        if let Some(r) = &other.pos.order {
            self.read_order(r);
        }
        if let Some(r) = &other.refund {
            let replace = match &self.refund {
                None => true,
                Some(old) => r.key() > old.key(),
            };
            if replace {
                self.refund = Some(r.clone());
            }
        }
        if let Some(o) = &other.order {
            self.take_order(o);
        }
    }

    /// The phase the sale shows, from its order.
    pub fn phase(&self) -> Option<PaymentPhase> {
        self.order.as_ref().map(|o| o.payment)
    }

    /// When it was rung up, Unix ms, from the POS's payment.
    pub fn rung_at(&self) -> Option<UnixMillis> {
        let p = self.pos.payment.as_ref()?;
        Some(UnixSeconds(p.created_at.as_second()).to_millis())
    }

    /// Ended: paid, refunded, or ended unpaid with `is_final` (what may be
    /// compacted, with the local marks' ends, §12).
    pub fn ended(&self) -> bool {
        if self
            .refund
            .as_ref()
            .is_some_and(|r| r.status == RefundStatus::Completed)
        {
            return true;
        }
        match &self.order {
            Some(o) => match o.payment {
                PaymentPhase::Paid { .. } => true,
                PaymentPhase::Expired | PaymentPhase::Cancelled | PaymentPhase::DoubleSpend => {
                    o.observed.settled
                }
                _ => false,
            },
            None => false,
        }
    }
}

/// A local time, with the journal's seq to order two in the same ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Stamp {
    pub at: UnixMillis,
    pub seq: i64,
}

/// A local close.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseMark {
    pub at: Stamp,
}

/// A refund the cashier was told to make in the POS (Clover), and when
/// they confirmed it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundMark {
    pub pending_at: Option<UnixMillis>,
    pub confirmed_at: Option<UnixMillis>,
}

/// An action this till took on the sale, and how it ended.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionMark {
    pub asked_at: Option<UnixMillis>,
    pub done_at: Option<UnixMillis>,
    pub outcome: Option<Outcome>,
}

/// What this device did about a sale, and when it first saw its phases:
/// layered over the shared state, never mixed into it. Times are this
/// device's, Unix ms.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMarks {
    /// The latest close; it counts while no later reopen undoes it
    /// ([`LocalMarks::closed_at`]).
    pub closed: Option<CloseMark>,
    pub reopened: Option<Stamp>,
    pub kept_as_cash: Option<UnixMillis>,
    pub paid_in_cash: Option<UnixMillis>,
    /// When the cashier said an overpayment's extra went back.
    #[serde(default)]
    pub extra_refunded: Option<UnixMillis>,
    pub refund: Option<RefundMark>,
    /// When this device's clock last said a code ran out; it counts only
    /// for a quote that ran out before it ([`crate::view`]).
    pub clock_lapsed: Option<UnixMillis>,
    /// When this device first knew the sale.
    pub first_at: Option<UnixMillis>,
    pub seen_at: Option<UnixMillis>,
    pub paid_at: Option<UnixMillis>,
    pub ended_at: Option<UnixMillis>,
    /// When this device first saw the POS's refund completed.
    pub refunded_at: Option<UnixMillis>,
    /// Kept under each action's snake-case name (`make_order`), as when
    /// the keys were strings ([`action_keys`]).
    #[serde(default, with = "action_keys")]
    pub actions: BTreeMap<Action, ActionMark>,
}

fn min_opt(a: Option<UnixMillis>, b: UnixMillis) -> Option<UnixMillis> {
    Some(a.map_or(b, |a| a.min(b)))
}

impl LocalMarks {
    /// Folds in a mark made at `at` (journal seq `seq`).
    pub fn mark(&mut self, mark: MarkKind, at: UnixMillis, seq: i64) {
        let stamp = Stamp { at, seq };
        match mark {
            MarkKind::Closed => {
                if self.closed.is_none_or(|c| stamp > c.at) {
                    self.closed = Some(CloseMark { at: stamp });
                }
            }
            MarkKind::Reopened => {
                if self.reopened.is_none_or(|r| stamp > r) {
                    self.reopened = Some(stamp);
                }
            }
            MarkKind::KeptAsCash => self.kept_as_cash = min_opt(self.kept_as_cash, at),
            MarkKind::PaidInCash => self.paid_in_cash = min_opt(self.paid_in_cash, at),
            MarkKind::ExtraRefunded => self.extra_refunded = min_opt(self.extra_refunded, at),
            MarkKind::RefundPending => {
                let r = self.refund.get_or_insert_default();
                r.pending_at = min_opt(r.pending_at, at);
            }
            MarkKind::RefundConfirmed => {
                let r = self.refund.get_or_insert_default();
                r.confirmed_at = min_opt(r.confirmed_at, at);
            }
            // The latest: a renewed code that runs out again lapses again
            // (core.md decision 170).
            MarkKind::ClockLapsed => self.clock_lapsed = max_opt(self.clock_lapsed, Some(at)),
        }
    }

    pub fn action(&mut self, action: Action, outcome: Option<&Outcome>, at: UnixMillis) {
        let entry = self.actions.entry(action).or_default();
        match outcome {
            None => {
                entry.asked_at = Some(at);
                entry.done_at = None;
                entry.outcome = None;
            }
            Some(outcome) => {
                entry.done_at = Some(at);
                entry.outcome = Some(outcome.clone());
            }
        }
    }

    /// Joins another row's marks for the same sale (a sale kept from
    /// Monokulo alone, folded into the point-of-sale sale it is): the later
    /// close, reopen and clock lapse, the earliest of every first time, and
    /// the actions this row has no word of.
    pub fn join(&mut self, other: &LocalMarks) {
        if let Some(c) = other.closed
            && self.closed.is_none_or(|mine| c.at > mine.at)
        {
            self.closed = Some(c);
        }
        if let Some(r) = other.reopened
            && self.reopened.is_none_or(|mine| r > mine)
        {
            self.reopened = Some(r);
        }
        let earliest = |mine: &mut Option<UnixMillis>, theirs: Option<UnixMillis>| {
            if let Some(t) = theirs {
                *mine = min_opt(*mine, t);
            }
        };
        earliest(&mut self.kept_as_cash, other.kept_as_cash);
        earliest(&mut self.paid_in_cash, other.paid_in_cash);
        earliest(&mut self.extra_refunded, other.extra_refunded);
        self.clock_lapsed = max_opt(self.clock_lapsed, other.clock_lapsed);
        earliest(&mut self.first_at, other.first_at);
        earliest(&mut self.seen_at, other.seen_at);
        earliest(&mut self.paid_at, other.paid_at);
        earliest(&mut self.ended_at, other.ended_at);
        earliest(&mut self.refunded_at, other.refunded_at);
        if let Some(theirs) = other.refund {
            let mine = self.refund.get_or_insert_default();
            earliest(&mut mine.pending_at, theirs.pending_at);
            earliest(&mut mine.confirmed_at, theirs.confirmed_at);
        }
        for (name, action) in &other.actions {
            self.actions.entry(*name).or_insert_with(|| action.clone());
        }
    }

    /// When the cashier closed it, unless they reopened it since.
    pub fn closed_at(&self) -> Option<UnixMillis> {
        let closed = self.closed?;
        match self.reopened {
            Some(r) if r > closed.at => None,
            _ => Some(closed.at.at),
        }
    }

    /// Notes when this device first saw the sale, first saw it reach its
    /// phase, and first saw its refund completed.
    pub fn saw(&mut self, shared: &SharedSale, at: UnixMillis) {
        self.first_at = min_opt(self.first_at, at);
        if shared
            .refund
            .as_ref()
            .is_some_and(|r| r.status == RefundStatus::Completed)
        {
            self.refunded_at = min_opt(self.refunded_at, at);
        }
        match shared.phase() {
            Some(PaymentPhase::Seen | PaymentPhase::Confirming { .. }) => {
                self.seen_at = min_opt(self.seen_at, at);
            }
            Some(PaymentPhase::Paid { .. }) => {
                self.paid_at = min_opt(self.paid_at, at);
            }
            Some(PaymentPhase::Expired | PaymentPhase::Cancelled | PaymentPhase::DoubleSpend) => {
                self.ended_at = min_opt(self.ended_at, at);
            }
            _ => {}
        }
    }

    /// Ended locally: closed, kept as cash, paid in cash or refunded.
    pub fn ended(&self) -> bool {
        self.closed_at().is_some()
            || self.kept_as_cash.is_some()
            || self.paid_in_cash.is_some()
            || self.refund.is_some_and(|r| r.confirmed_at.is_some())
            || self.refunded_at.is_some()
    }
}

/// The name an action is kept under in [`LocalMarks::actions`]' JSON.
fn action_name(action: Action) -> &'static str {
    match action {
        Action::MakeOrder => "make_order",
        Action::TakeInMonero => "take_in_monero",
        Action::NewCode => "new_code",
        Action::Refund => "refund",
    }
}

/// [`LocalMarks::actions`] as a JSON object keyed by [`action_name`]. A key
/// that names no action (from a newer build) is skipped rather than
/// failing the whole sale's marks.
mod action_keys {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serializer};

    use super::{ActionMark, action_name};
    use crate::facts::Action;

    const ALL: [Action; 4] = [
        Action::MakeOrder,
        Action::TakeInMonero,
        Action::NewCode,
        Action::Refund,
    ];

    pub fn serialize<S: Serializer>(
        actions: &BTreeMap<Action, ActionMark>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        s.collect_map(actions.iter().map(|(a, mark)| (action_name(*a), mark)))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeMap<Action, ActionMark>, D::Error> {
        let named = BTreeMap::<String, ActionMark>::deserialize(d)?;
        Ok(named
            .into_iter()
            .filter_map(|(name, mark)| {
                let action = ALL.into_iter().find(|a| action_name(*a) == name)?;
                Some((action, mark))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The actions keep the JSON keys they had as strings, and marks kept
    /// before still read, a key naming no action skipped.
    #[test]
    fn local_marks_keep_their_action_keys() {
        let old = r#"{"closed":null,"reopened":null,"kept_as_cash":null,"paid_in_cash":null,"refund":null,"clock_lapsed":null,"first_at":null,"seen_at":null,"paid_at":null,"ended_at":null,"refunded_at":null,"actions":{"make_order":{"asked_at":5,"done_at":6,"outcome":"Done"},"refund":{"asked_at":7,"done_at":null,"outcome":null},"teleport":{"asked_at":1,"done_at":null,"outcome":null}}}"#;
        let marks: LocalMarks = serde_json::from_str(old).unwrap();
        assert_eq!(marks.actions.len(), 2);
        assert_eq!(
            marks.actions[&Action::MakeOrder].done_at,
            Some(UnixMillis(6))
        );
        assert_eq!(marks.actions[&Action::Refund].asked_at, Some(UnixMillis(7)));
        let json = serde_json::to_string(&marks).unwrap();
        assert!(
            json.contains(
                r#""actions":{"make_order":{"asked_at":5,"done_at":6,"outcome":"Done"},"refund":"#
            ),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<LocalMarks>(&json).unwrap(), marks);
    }

    fn obs(phase: SalePhase, is_final: bool) -> OrderObserved {
        OrderObserved {
            order_id: "o".into(),
            phase,
            confirmations: None,
            is_final,
            received: None,
            overpaid: false,
            quote: None,
            renew: None,
        }
    }

    fn phase_of(list: &[OrderObserved]) -> PaymentPhase {
        let mut o = Observations::default();
        for x in list {
            o.observe(x);
        }
        o.phase(UnixSeconds(0), UnixSeconds(0))
    }

    #[test]
    fn paid_absorbs_everything_but_a_double_spend_needs_a_final_paid() {
        use SalePhase::*;
        for other in [Waiting, Seen, Confirming, Partial, Expired, Cancelled] {
            for f in [false, true] {
                assert!(phase_of(&[obs(other, f), obs(Paid, false)]).is_paid());
                assert!(phase_of(&[obs(Paid, false), obs(other, f)]).is_paid());
            }
        }
        assert_eq!(
            phase_of(&[obs(DoubleSpend, false), obs(Paid, false)]),
            PaymentPhase::DoubleSpend
        );
        assert!(phase_of(&[obs(DoubleSpend, false), obs(Paid, true)]).is_paid());
        assert!(phase_of(&[obs(Paid, true), obs(DoubleSpend, false)]).is_paid());
    }

    #[test]
    fn an_ending_not_final_does_not_block_a_later_payment() {
        use SalePhase::*;
        assert_eq!(
            phase_of(&[obs(Waiting, false), obs(Expired, false)]),
            PaymentPhase::Expired
        );
        assert_eq!(
            phase_of(&[obs(Expired, false), obs(Seen, false)]),
            PaymentPhase::Seen
        );
        assert_eq!(
            phase_of(&[obs(Cancelled, false), obs(Seen, false)]),
            PaymentPhase::Seen
        );
        assert_eq!(
            phase_of(&[obs(Seen, false), obs(Expired, true)]),
            PaymentPhase::Expired
        );
        assert_eq!(
            phase_of(&[obs(Expired, false), obs(Cancelled, false)]),
            PaymentPhase::Cancelled
        );
    }

    #[test]
    fn the_further_phase_wins() {
        let mut c = obs(SalePhase::Confirming, false);
        c.confirmations = Some(4);
        let mut c2 = c.clone();
        c2.confirmations = Some(2);
        assert_eq!(
            phase_of(&[c.clone(), c2.clone(), obs(SalePhase::Seen, false)]),
            PaymentPhase::Confirming { n: 4 }
        );
        let mut p = obs(SalePhase::Partial, false);
        p.received = Some(10);
        assert_eq!(phase_of(&[p, c]), PaymentPhase::Partial { received: 10 });
    }

    fn created(order: &str, expires_at: i64, xmr: &str) -> OrderCreated {
        OrderCreated {
            sale: crate::ids::SaleKey::pos(monokulo_client::PointOfSale::Square, "p1"),
            order_id: order.into(),
            url: None,
            payment_uri: format!("monero:8a?tx_amount={xmr}"),
            address: "8a".into(),
            xmr: xmr.into(),
            price: till_types::Price::new("24.50", "USD"),
            reference: None,
            expires_at: UnixSeconds(expires_at),
            confirmations_required: None,
            device_name: None,
            renew: None,
            price_used: None,
        }
    }

    fn under(phase: SalePhase, expires_at: i64, piconero: u64) -> OrderObserved {
        OrderObserved {
            quote: Some(Quote {
                expires_at: UnixSeconds(expires_at),
                piconero,
            }),
            ..obs(phase, false)
        }
    }

    /// Every order of `facts`, joined into one sale, in that order.
    fn sale_of(facts: &[Result<OrderCreated, OrderObserved>]) -> SharedSale {
        let mut sale = SharedSale::default();
        for fact in facts {
            match fact {
                Ok(c) => sale.create_order(c),
                Err(o) => sale.observe(o),
            }
        }
        sale
    }

    /// A renewal is a later quote on the same order: whatever order the
    /// facts arrive in, the order is waiting again on the new code, and the
    /// old quote's ending doesn't end it; one under the new quote does, and
    /// a payment against either always shows.
    #[test]
    fn a_renewal_is_a_new_quote_and_an_ending_belongs_to_its_quote() {
        use SalePhase::*;
        let facts = [
            Ok(created("o", 100, "0.15")),
            Err(under(Expired, 100, 150_000_000_000)),
            Err(OrderObserved {
                order_id: "o".into(),
                ..obs(Expired, false)
            }),
            Err(under(Waiting, 1_000, 160_000_000_000)),
        ];
        // Every order of the four.
        let orders: [[usize; 4]; 6] = [
            [0, 1, 2, 3],
            [0, 3, 1, 2],
            [0, 3, 2, 1],
            [0, 2, 3, 1],
            [0, 1, 3, 2],
            [0, 2, 1, 3],
        ];
        let mut sales = orders
            .iter()
            .map(|order| sale_of(&order.map(|i| facts[i].clone())));
        let first = sales.next().unwrap();
        for other in sales {
            assert_eq!(other, first);
        }
        let order = first.order.as_ref().unwrap();
        assert_eq!(order.payment, PaymentPhase::Waiting);
        assert_eq!(
            (order.order.expires_at, order.order.xmr.as_str()),
            (UnixSeconds(1_000), "0.16")
        );
        assert_eq!(order.order.payment_uri, "monero:8a?tx_amount=0.16");
        assert_eq!(order.order.first_expiry(), UnixSeconds(100));

        let mut ended = first.clone();
        ended.observe(&under(Expired, 1_000, 160_000_000_000));
        assert_eq!(ended.phase(), Some(PaymentPhase::Expired));
        let mut paid = first.clone();
        paid.observe(&under(Paid, 100, 150_000_000_000));
        assert!(paid.phase().unwrap().is_paid(), "paid at the old price");
    }

    /// Two orders for one sale (only an older build's New code made them):
    /// the later code stands, whatever order they arrive in, and what was
    /// said of the other changes nothing.
    #[test]
    fn of_two_orders_for_one_sale_the_later_code_stands() {
        use SalePhase::*;
        let one = Ok(created("o1", 100, "0.15"));
        let two = Ok(created("o2", 1_000, "0.16"));
        let paid_one = Err(OrderObserved {
            order_id: "o1".into(),
            ..obs(Paid, false)
        });
        for facts in [
            [one.clone(), paid_one.clone(), two.clone()],
            [two.clone(), one.clone(), paid_one.clone()],
            [paid_one.clone(), two.clone(), one.clone()],
        ] {
            let sale = sale_of(&facts);
            let order = sale.order.unwrap();
            assert_eq!(order.order.order_id.as_str(), "o2");
            assert_eq!(order.payment, PaymentPhase::Waiting);
        }
    }

    /// A compacted sale written before decision 167 keeps its paid order,
    /// else its latest.
    #[test]
    fn a_snapshots_attempts_become_its_one_order() {
        let attempt = |order: &str, expires_at: i64, paid: bool| {
            let mut o = SaleOrder::new(OrderFacts::from(&created(order, expires_at, "0.15")));
            if paid {
                o.observe(&obs(SalePhase::Paid, false));
            }
            let mut v = serde_json::to_value(&o).unwrap();
            v["attempt"] = serde_json::json!(1);
            v
        };
        let legacy = |attempts: Vec<serde_json::Value>| {
            let v = serde_json::json!({
                "pos": {"payment": null, "order": null},
                "attempts": attempts,
                "refund": null,
            });
            serde_json::from_value::<SharedSale>(v).unwrap()
        };
        let latest = legacy(vec![attempt("o1", 100, false), attempt("o2", 1_000, false)]);
        assert_eq!(latest.order.unwrap().order.order_id.as_str(), "o2");
        let paid = legacy(vec![attempt("o1", 100, true), attempt("o2", 1_000, false)]);
        let paid = paid.order.unwrap();
        assert_eq!(paid.order.order_id.as_str(), "o1");
        assert!(paid.payment.is_paid());
    }

    #[test]
    fn a_reopen_after_a_close_undoes_it_and_a_later_close_counts_again() {
        let mut m = LocalMarks::default();
        m.mark(MarkKind::Closed, UnixMillis(10), 1);
        assert_eq!(m.closed_at(), Some(UnixMillis(10)));
        m.mark(MarkKind::Reopened, UnixMillis(10), 2);
        assert_eq!(m.closed_at(), None);
        m.mark(MarkKind::Closed, UnixMillis(20), 3);
        assert_eq!(m.closed_at(), Some(UnixMillis(20)));
    }
}
