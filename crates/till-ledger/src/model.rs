//! The sale model (docs/ledger.md §5): what the sources said about a sale
//! ([`SharedSale`]), what this device did ([`LocalMarks`]), and the pure
//! merges that fold one fact into each.
//!
//! The shared merge is a join: commutative, associative and idempotent over
//! the same facts, so every till fed the same facts in any order, with
//! repeats, holds the same `SharedSale`, and a stale fact can't move a sale
//! backwards.

use std::collections::BTreeMap;

use monokulo_client::SalePhase;
use serde::{Deserialize, Serialize};

use crate::facts::{
    Action, MarkKind, OrderCreated, OrderObserved, OrderRead, Outcome, PaymentSeen, RefundSeen,
    RefundStatus,
};
use crate::ids::OrderId;

/// What the sources said about one sale.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedSale {
    pub pos: PosFacts,
    /// Monokulo orders, attempt 1..n, in attempt order.
    pub attempts: Vec<Attempt>,
    /// The POS's refund, if any: beside the attempts, never instead of them.
    pub refund: Option<PosRefund>,
}

/// The latest payment and order read from the POS.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosFacts {
    pub payment: Option<PaymentSeen>,
    pub order: Option<OrderRead>,
}

/// One of a sale's Monokulo orders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub attempt: u8,
    pub order: OrderFacts,
    /// Where its payment is, as merged from [`Attempt::observed`].
    pub payment: PaymentPhase,
    pub observed: Observations,
}

/// What create-order said about an attempt's order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OrderFacts {
    pub order_id: OrderId,
    pub url: Option<String>,
    pub payment_uri: String,
    pub address: String,
    pub xmr: String,
    pub amount: String,
    pub currency: String,
    pub reference: Option<String>,
    pub expires_at: i64,
    pub confirmations_required: Option<u32>,
    pub device_name: Option<String>,
}

impl From<&OrderCreated> for OrderFacts {
    fn from(o: &OrderCreated) -> OrderFacts {
        OrderFacts {
            order_id: o.order_id.clone(),
            url: o.url.clone(),
            payment_uri: o.payment_uri.clone(),
            address: o.address.clone(),
            xmr: o.xmr.clone(),
            amount: o.amount.clone(),
            currency: o.currency.clone(),
            reference: o.reference.clone(),
            expires_at: o.expires_at,
            confirmations_required: o.confirmations_required,
            device_name: o.device_name.clone(),
        }
    }
}

/// Where an attempt's payment is.
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
    pub ended: Option<EndedSeen>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndedSeen {
    pub cancelled: bool,
    pub is_final: bool,
}

fn max_opt<T: Ord>(a: Option<T>, b: Option<T>) -> Option<T> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
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
            SalePhase::Expired | SalePhase::Cancelled => {
                let ended = self.ended.get_or_insert_default();
                ended.cancelled |= o.phase == SalePhase::Cancelled;
                ended.is_final |= o.is_final;
            }
            SalePhase::DoubleSpend => self.double_spend = true,
        }
        self.normalise();
    }

    /// Drops what can no longer change the phase, so a stale observation
    /// (a Seen after Paid) changes nothing and isn't journalled. Once paid,
    /// reversed or finally ended, progress doesn't count; Confirming and
    /// Partial imply Seen. Both only depend on parts that only grow, so the
    /// join stays commutative, associative and idempotent.
    fn normalise(&mut self) {
        let decided =
            self.paid.is_some() || self.double_spend || self.ended.is_some_and(|e| e.is_final);
        if decided {
            self.seen = false;
            self.confirming = None;
            self.partial = None;
        } else if self.confirming.is_some() || self.partial.is_some() {
            self.seen = true;
        }
    }

    /// Joins another order's observations in (two order ids for one
    /// attempt, which Monokulo shouldn't give).
    fn join(&mut self, other: &Observations) {
        if let Some(p) = other.paid {
            let paid = self.paid.get_or_insert_default();
            paid.overpaid |= p.overpaid;
            paid.is_final |= p.is_final;
        }
        if let Some(e) = other.ended {
            let ended = self.ended.get_or_insert_default();
            ended.cancelled |= e.cancelled;
            ended.is_final |= e.is_final;
        }
        self.double_spend |= other.double_spend;
        self.seen |= other.seen;
        self.confirming = max_opt(self.confirming, other.confirming);
        self.partial = max_opt(self.partial, other.partial);
        self.confirmations = max_opt(self.confirmations, other.confirmations);
        self.settled |= other.settled;
        self.normalise();
    }

    /// The phase the observations add up to:
    /// - Paid absorbs everything, except that a double spend needs a final
    ///   Paid (Monokulo's own resolution) to be overridden.
    /// - A double spend stays once seen.
    /// - An ending Monokulo calls final (`is_final`) beats progress; one it
    ///   doesn't (it still watches for a late payment) beats only Waiting,
    ///   so a later Seen, Confirming or Partial still shows.
    /// - Otherwise the further phase: Seen < Confirming (more
    ///   confirmations) < Partial (more received).
    pub fn phase(&self) -> PaymentPhase {
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
        let ended = self.ended.map(|e| {
            (
                e.is_final,
                if e.cancelled {
                    PaymentPhase::Cancelled
                } else {
                    PaymentPhase::Expired
                },
            )
        });
        if let Some((true, phase)) = ended {
            return phase;
        }
        if let Some(received) = self.partial {
            return PaymentPhase::Partial { received };
        }
        if let Some(n) = self.confirming {
            return PaymentPhase::Confirming { n };
        }
        if self.seen {
            return PaymentPhase::Seen;
        }
        match ended {
            Some((_, phase)) => phase,
            None => PaymentPhase::Waiting,
        }
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

    /// Joins an order create-order made, as its attempt.
    pub fn create_order(&mut self, o: &OrderCreated) {
        let attempt = o.attempt.max(1);
        let order = OrderFacts::from(o);
        match self.attempts.iter_mut().find(|a| a.attempt == attempt) {
            Some(existing) => {
                if order > existing.order {
                    existing.order = order;
                }
            }
            None => {
                let at = self
                    .attempts
                    .iter()
                    .position(|a| a.attempt > attempt)
                    .unwrap_or(self.attempts.len());
                self.attempts.insert(
                    at,
                    Attempt {
                        attempt,
                        order,
                        payment: PaymentPhase::Waiting,
                        observed: Observations::default(),
                    },
                );
            }
        }
    }

    /// Joins an observation of `attempt`'s order. An attempt with no order
    /// yet is ignored (the ledger keeps such observations pending).
    pub fn observe(&mut self, attempt: u8, o: &OrderObserved) {
        if let Some(a) = self.attempts.iter_mut().find(|a| a.attempt == attempt) {
            a.observed.observe(o);
            a.payment = a.observed.phase();
        }
    }

    /// Joins what Monokulo said about `order` in `other` (a sale kept from
    /// Monokulo alone) into this sale's `attempt`, whose order it is: the
    /// observations only, since this sale's own create-order answer stands
    /// for the order.
    pub fn take_observations(&mut self, other: &SharedSale, order: &OrderId, attempt: u8) {
        let Some(mine) = self.attempts.iter_mut().find(|a| a.attempt == attempt) else {
            return;
        };
        for theirs in other.attempts.iter().filter(|a| a.order.order_id == *order) {
            mine.observed.join(&theirs.observed);
        }
        mine.payment = mine.observed.phase();
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
        for a in &other.attempts {
            match self.attempts.iter_mut().find(|m| m.attempt == a.attempt) {
                Some(mine) => {
                    if a.order > mine.order {
                        mine.order = a.order.clone();
                    }
                    mine.observed.join(&a.observed);
                    mine.payment = mine.observed.phase();
                }
                None => {
                    let at = self
                        .attempts
                        .iter()
                        .position(|m| m.attempt > a.attempt)
                        .unwrap_or(self.attempts.len());
                    self.attempts.insert(at, a.clone());
                }
            }
        }
    }

    /// The attempt that decides what the sale shows: the first paid one,
    /// else the newest.
    pub fn deciding(&self) -> Option<&Attempt> {
        self.attempts
            .iter()
            .find(|a| a.payment.is_paid())
            .or_else(|| self.attempts.last())
    }

    /// The newest attempt number, 0 with no order.
    pub fn last_attempt(&self) -> u8 {
        self.attempts.last().map_or(0, |a| a.attempt)
    }

    /// The phase the sale shows, from its deciding attempt.
    pub fn phase(&self) -> Option<PaymentPhase> {
        self.deciding().map(|a| a.payment)
    }

    /// When it was rung up, Unix ms, from the POS's payment.
    pub fn rung_at(&self) -> Option<i64> {
        let p = self.pos.payment.as_ref()?;
        till_pos::time::unix_seconds(&p.created_at).map(|s| s * 1000)
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
        match self.deciding() {
            Some(a) => match a.payment {
                PaymentPhase::Paid { .. } => true,
                PaymentPhase::Expired | PaymentPhase::Cancelled | PaymentPhase::DoubleSpend => {
                    a.observed.settled
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
    pub at: i64,
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
    pub pending_at: Option<i64>,
    pub confirmed_at: Option<i64>,
}

/// An action this till took on the sale, and how it ended.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionMark {
    pub asked_at: Option<i64>,
    pub done_at: Option<i64>,
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
    pub kept_as_cash: Option<i64>,
    pub paid_in_cash: Option<i64>,
    pub refund: Option<RefundMark>,
    pub clock_lapsed: Option<i64>,
    /// When this device first knew the sale.
    pub first_at: Option<i64>,
    pub seen_at: Option<i64>,
    pub paid_at: Option<i64>,
    pub ended_at: Option<i64>,
    /// When this device first saw the POS's refund completed.
    pub refunded_at: Option<i64>,
    #[serde(default)]
    pub actions: BTreeMap<String, ActionMark>,
}

fn min_opt(a: Option<i64>, b: i64) -> Option<i64> {
    Some(a.map_or(b, |a| a.min(b)))
}

impl LocalMarks {
    /// Folds in a mark made at `at` (journal seq `seq`).
    pub fn mark(&mut self, mark: MarkKind, at: i64, seq: i64) {
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
            MarkKind::RefundPending => {
                let r = self.refund.get_or_insert_default();
                r.pending_at = min_opt(r.pending_at, at);
            }
            MarkKind::RefundConfirmed => {
                let r = self.refund.get_or_insert_default();
                r.confirmed_at = min_opt(r.confirmed_at, at);
            }
            MarkKind::ClockLapsed => self.clock_lapsed = min_opt(self.clock_lapsed, at),
        }
    }

    pub fn action(&mut self, action: Action, outcome: Option<&Outcome>, at: i64) {
        let entry = self.actions.entry(action_name(action)).or_default();
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
    /// close and reopen, the earliest of every first time, and the actions
    /// this row has no word of.
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
        let earliest = |mine: &mut Option<i64>, theirs: Option<i64>| {
            if let Some(t) = theirs {
                *mine = min_opt(*mine, t);
            }
        };
        earliest(&mut self.kept_as_cash, other.kept_as_cash);
        earliest(&mut self.paid_in_cash, other.paid_in_cash);
        earliest(&mut self.clock_lapsed, other.clock_lapsed);
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
            self.actions
                .entry(name.clone())
                .or_insert_with(|| action.clone());
        }
    }

    /// When the cashier closed it, unless they reopened it since.
    pub fn closed_at(&self) -> Option<i64> {
        let closed = self.closed?;
        match self.reopened {
            Some(r) if r > closed.at => None,
            _ => Some(closed.at.at),
        }
    }

    /// Notes when this device first saw the sale, first saw it reach its
    /// phase, and first saw its refund completed.
    pub fn saw(&mut self, shared: &SharedSale, at: i64) {
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

/// The key an action is kept under in [`LocalMarks::actions`].
pub fn action_name(action: Action) -> String {
    match action {
        Action::MakeOrder => "make_order",
        Action::TakeInMonero => "take_in_monero",
        Action::NewCode => "new_code",
        Action::Refund => "refund",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(phase: SalePhase, is_final: bool) -> OrderObserved {
        OrderObserved {
            order_id: "o".into(),
            phase,
            confirmations: None,
            is_final,
            received: None,
            overpaid: false,
        }
    }

    fn phase_of(list: &[OrderObserved]) -> PaymentPhase {
        let mut o = Observations::default();
        for x in list {
            o.observe(x);
        }
        o.phase()
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

    #[test]
    fn a_reopen_after_a_close_undoes_it_and_a_later_close_counts_again() {
        let mut m = LocalMarks::default();
        m.mark(MarkKind::Closed, 10, 1);
        assert_eq!(m.closed_at(), Some(10));
        m.mark(MarkKind::Reopened, 10, 2);
        assert_eq!(m.closed_at(), None);
        m.mark(MarkKind::Closed, 20, 3);
        assert_eq!(m.closed_at(), Some(20));
    }
}
