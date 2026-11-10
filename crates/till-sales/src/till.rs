//! The till (Android: `Till.kt`), driving the ledger (docs/ledger.md §6 to
//! §8): what the point of sale lists, what its orders read, what Monokulo's
//! events and reads say and what the cashier does each become one fact,
//! recorded in the [`Books`]. The till keeps no sales of its own: its
//! state is the ledger's, read back after each write.

use std::collections::HashSet;
use std::future::pending;
use std::sync::Arc;
use std::time::Duration;

use monokulo_client::{
    Clock, Connection, MonokuloApi, MonokuloError, Order, SaleEvent, SalePhase, SystemClock,
    payment_uri, problems,
};
use till_ledger::{
    Action, Fact, MarkKind, OrderCreated, OrderId, OrderKey, OrderObserved, OrderRead, Outcome,
    PaymentId, PaymentSeen, Quote, RefundSeen, RefundStatus, SaleKey, ShiftMark, Tender,
    may_make_order,
};
use till_pos::{FoundSale, PosError, RefundInPos, Refunds, SaleSource, decimal_amount};
use tokio::sync::watch;

use crate::{Books, TillSale, TillState};

#[cfg(test)]
mod tests;

/// How the cashier set the till up (core.md decisions 113 and 116): this
/// counter's point-of-sale device, when known, whether its alerts are for
/// every device's sales, and whether a cash sale is treated as Monero as
/// it's found. Kept as the ledger's settings ([`Books::keep_options`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TillOptions {
    pub device_id: Option<String>,
    pub every_device: bool,
    pub cash_as_monero: bool,
}

impl Default for TillOptions {
    /// Every device's sales, and cash treated as Monero: the till before
    /// it's told otherwise.
    fn default() -> TillOptions {
        TillOptions {
            device_id: None,
            every_device: false,
            cash_as_monero: true,
        }
    }
}

/// What a read of an order says about its sale: its phase, whether
/// Monokulo has settled it, and its confirmations of those it needs, when
/// Monokulo said ([`Till::on_status`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderStatus {
    pub phase: SalePhase,
    /// Monokulo said nothing more will happen to it ([`TillSale::settled`]).
    pub settled: bool,
    pub confirmations: Option<u32>,
    pub required: Option<u32>,
}

impl OrderStatus {
    /// `phase`, not settled, with no count.
    pub fn of(phase: SalePhase) -> OrderStatus {
        OrderStatus {
            phase,
            settled: false,
            confirmations: None,
            required: None,
        }
    }

    /// The same, settled or not as `settled` says.
    pub fn settled(self, settled: bool) -> OrderStatus {
        OrderStatus { settled, ..self }
    }

    /// The same, with `confirmations` of `required`.
    pub fn counted(self, confirmations: u32, required: u32) -> OrderStatus {
        OrderStatus {
            confirmations: Some(confirmations),
            required: Some(required),
            ..self
        }
    }
}

impl From<&Order> for OrderStatus {
    fn from(order: &Order) -> OrderStatus {
        OrderStatus {
            phase: order.phase(),
            settled: order.is_final,
            confirmations: order.confirmations,
            required: order.confirmations_required,
        }
    }
}

/// What only [`Till::check`] touches, held while it runs.
#[derive(Default)]
struct Checking {
    /// The sales the last check found, by id: what the next one's count
    /// leaves out.
    last_found: HashSet<String>,
    /// When the timer last asked the POS about pending refunds
    /// ([`Till::REFUND_LOOK_SECONDS`]).
    last_refund_look_at: Option<i64>,
}

/// The till (docs/design.md §1): turns the POS's Monero sales into Monokulo
/// orders, and what the POS and Monokulo say about them into facts. A sale
/// it finds is handled once it has an order (or isn't this till's to
/// make, docs/ledger.md §7); until then every [`Till::check`] tries again,
/// and the idempotency key makes a retry, or another till finding the same
/// sale, get the same order.
pub struct Till {
    source: Arc<dyn SaleSource>,
    monokulo: MonokuloApi,
    connection: Connection,
    /// Signed in to a point of sale: a Monokulo event never makes a sale
    /// here, it waits for its order (docs/ledger.md §3).
    with_pos: bool,
    clock: Arc<dyn Clock>,
    books: Arc<Books>,
    checking: tokio::sync::Mutex<Checking>,
    /// Hears how long each Monokulo order took to make ([`Till::with_timer`]).
    timer: Option<Timer>,
}

/// Hears a step's name and how long it took: iOS's extension times its
/// look with it (docs/decisions/core.md 203).
pub type Timer = Arc<dyn Fn(&'static str, Duration) + Send + Sync>;

impl Till {
    /// The most sales the shift shows, newest first.
    pub const MAX_SALES: usize = 500;

    /// The timer asks the POS about pending refunds at most this often, in
    /// seconds (decision 130 in docs/decisions/android.md).
    pub const REFUND_LOOK_SECONDS: i64 = 30;

    /// Refunds made this long before the refunds cursor are listed again,
    /// in case the point of sale was slow to list them.
    pub const REFUNDS_OVERLAP_SECONDS: i64 = 120;

    /// How often [`Till::follow_confirmations`] reads a seen or confirming
    /// sale again: a Monero block comes about every two minutes, so a
    /// confirmation shows within a sixth of one, for a request a sale every
    /// 20 s while it's on its way.
    pub const CONFIRMATIONS_READ: Duration = Duration::from_secs(20);

    /// How long [`Till::lapse_on_time`] waits to try again when the clock it
    /// lapsed by hadn't reached a code's end yet.
    pub const LAPSE_RETRY: Duration = Duration::from_secs(1);

    /// A till over `source`, making orders at `connection`'s store and
    /// keeping what it learns in `books`. It reads the device's clock until
    /// told otherwise ([`Till::with_clock`]).
    pub fn new(
        source: Arc<dyn SaleSource>,
        monokulo: MonokuloApi,
        connection: Connection,
        books: Arc<Books>,
    ) -> Till {
        Till {
            source,
            monokulo,
            connection,
            with_pos: true,
            clock: Arc::new(SystemClock),
            books,
            checking: tokio::sync::Mutex::new(Checking::default()),
            timer: None,
        }
    }

    /// The same till, telling `timer` how long each order it makes takes
    /// (`"monokulo.create"`).
    pub fn with_timer(self, timer: Timer) -> Till {
        Till {
            timer: Some(timer),
            ..self
        }
    }

    /// The same till, not signed in to a point of sale: it shows the sales
    /// Monokulo's events tell it of, with their plain codes.
    pub fn without_pos(self) -> Till {
        Till {
            with_pos: false,
            ..self
        }
    }

    /// The same till, at the counter whose point-of-sale device is
    /// `device_id` (core.md decision 113), cash treated as Monero.
    pub fn with_device_id(self, device_id: impl Into<String>) -> Till {
        self.with_options(&TillOptions {
            device_id: Some(device_id.into()),
            ..TillOptions::default()
        })
    }

    /// The same till, set up as `options` say, which the ledger keeps.
    pub fn with_options(self, options: &TillOptions) -> Till {
        self.books.keep_options(options, self.now_ms());
        self
    }

    /// The same till, stamping the cashier's marks with `clock`'s time,
    /// which the ledger reads too.
    pub fn with_clock(self, clock: Arc<dyn Clock>) -> Till {
        self.books.set_clock(clock.clone());
        Till { clock, ..self }
    }

    /// The books the till keeps its facts in.
    pub fn books(&self) -> &Arc<Books> {
        &self.books
    }

    /// The till's state as it changes: its sales, newest first, and the
    /// last thing that went wrong.
    pub fn subscribe(&self) -> watch::Receiver<TillState> {
        self.books.subscribe()
    }

    /// The till's state now.
    pub fn state(&self) -> TillState {
        self.books.state()
    }

    /// The point of sale the till's source watches.
    pub fn point_of_sale(&self) -> till_pos::PointOfSale {
        self.source.point_of_sale()
    }

    /// `sq:` and the start of SHA-256 of the POS's payment id: the same for
    /// every till, and opaque ([`OrderKey`]). One payment is one order: New
    /// code renews it (core.md decision 167).
    pub fn idempotency_key(sale: &FoundSale) -> String {
        OrderKey::new(PaymentId::new(sale.id.as_str())).key()
    }

    /// When the next live code runs out, Unix seconds; `None` with none
    /// live.
    pub fn next_lapse(state: &TillState) -> Option<i64> {
        state
            .sales
            .iter()
            .filter(|sale| sale.live() && sale.expires_at > 0)
            .map(|sale| sale.expires_at)
            .min()
    }

    fn now_ms(&self) -> i64 {
        self.clock.now().saturating_mul(1000)
    }

    fn record(&self, fact: Fact) -> bool {
        self.books.record(fact).is_some_and(|r| r.changed)
    }

    fn mark(&self, sale: &SaleKey, mark: MarkKind) {
        self.record(Fact::Mark {
            sale: sale.clone(),
            mark,
            at: self.now_ms(),
        });
    }

    /// The sale for `order_id` as the shift shows it, with its key.
    fn sale(&self, order_id: &str) -> Option<(SaleKey, TillSale)> {
        let state = self.books.state();
        let at = state
            .views
            .iter()
            .position(|v| v.order_id == order_id || v.sale.text() == order_id)?;
        Some((state.views[at].sale.clone(), state.sales[at].clone()))
    }

    /// Looks for new sales: each becomes a fact, its order is read once
    /// (and again when the payment changes), and the order is made when
    /// it's this till's to make. Answers how many of the POS's sales this
    /// check found that the last one hadn't, another device's included
    /// (the Bell rings for every sale at the location): a sale whose order
    /// failed, found again, isn't counted twice. 0 when the POS couldn't be
    /// read.
    pub async fn check(&self) -> usize {
        self.check_with(false).await
    }

    /// [`Till::check`], with the point of sale's refunds listed while its
    /// payments are, rather than after: for a look someone waits on (iOS's
    /// extension, whose alert waits for the look, docs/decisions/core.md
    /// 204). A refund of a sale this very look first records may be missed
    /// by this listing; the next one lists it again
    /// ([`Till::REFUNDS_OVERLAP_SECONDS`]).
    pub async fn check_at_once(&self) -> usize {
        self.check_with(true).await
    }

    async fn check_with(&self, at_once: bool) -> usize {
        let mut checking = self.checking.lock().await;
        let fresh = if at_once {
            let (fresh, ()) = tokio::join!(
                self.take_new_sales(&mut checking.last_found),
                self.read_pos_refunds()
            );
            fresh
        } else {
            let fresh = self.take_new_sales(&mut checking.last_found).await;
            if fresh.is_some() {
                self.read_pos_refunds().await;
            }
            fresh
        };
        let Some(fresh) = fresh else {
            return 0;
        };
        self.look_for_refunds(&mut checking).await;
        self.books.looked();
        fresh
    }

    /// The point of sale's new sales, each taken ([`Till::take`]): how many
    /// the last look hadn't found, or `None` when it couldn't be read.
    async fn take_new_sales(&self, last_found: &mut HashSet<String>) -> Option<usize> {
        let found = match self.source.new_sales().await {
            Ok(found) => found,
            Err(e) => {
                self.books.change(|s| with_pos_problem(s, &e));
                self.books.looked();
                return None;
            }
        };
        let fresh = found
            .iter()
            .filter(|sale| !last_found.contains(&sale.id))
            .count();
        *last_found = found.iter().map(|sale| sale.id.clone()).collect();
        let mut problem = None;
        for sale in &found {
            match self.take(sale).await {
                Ok(()) => self.source.handled(sale),
                Err(e) => problem = Some(e),
            }
        }
        self.books.change(|s| TillState {
            problem: problem.clone(),
            sign_in_again: false,
            ..s.clone()
        });
        Some(fresh)
    }

    /// Reads the refunds the point of sale lists since this till's refunds
    /// cursor (less [`Till::REFUNDS_OVERLAP_SECONDS`]), or since the
    /// shift's window began before the first, and records each one of a
    /// sale the ledger has as a `PosRefundSeen`: refunds made in Square's
    /// own app or Dashboard as well as the till's. The cursor, kept in the
    /// ledger, moves to the newest refund listed, but no further than the
    /// oldest still pending, so its completion is listed again. A point of
    /// sale that lists none (Clover), or a listing that fails, changes
    /// nothing: the next look asks again.
    async fn read_pos_refunds(&self) {
        let pos = self.source.point_of_sale();
        let name = refunds_cursor(pos);
        let Ok((cursor, window)) = self
            .books
            .with(|ledger| Ok((ledger.cursor(name)?, ledger.shift_window()?)))
        else {
            return;
        };
        let kept = cursor.as_deref().and_then(till_pos::time::unix_seconds);
        let from = match kept {
            Some(at) => at.saturating_sub(Till::REFUNDS_OVERLAP_SECONDS),
            None => window.from.div_euclid(1000),
        };
        let begin = till_pos::time::instant(from);
        let Ok(Some(refunds)) = self.source.refunds_since(&begin).await else {
            return;
        };
        let mut pending: Option<i64> = None;
        let mut newest: Option<i64> = None;
        for refund in &refunds {
            let key = SaleKey::pos(pos, refund.payment_id.as_str());
            let at = refund
                .created_at
                .as_deref()
                .and_then(till_pos::time::unix_seconds);
            let status = RefundStatus::of_pos(&refund.status);
            if let Some(at) = at {
                newest = newest.max(Some(at));
                // Only a refund Square says is pending holds the cursor: one
                // whose status this build doesn't know may never change to
                // one it does, and would hold it there for good (core.md
                // decision 170).
                if refund.status == till_pos::RefundStatus::Pending {
                    pending = Some(pending.map_or(at, |p| p.min(at)));
                }
            }
            if self.books.row(&key).is_none() {
                // Not a sale this till knows: a card sale's refund, say.
                continue;
            }
            self.record(Fact::PosRefundSeen(RefundSeen {
                sale: key,
                refund_id: refund.id.clone(),
                amount_minor: refund.amount_minor,
                status,
            }));
        }
        let next = pending.or(newest).or(kept).unwrap_or(from);
        if kept != Some(next) {
            // A hint for listing less, never truth: one not kept lists a
            // little more next time.
            let next = till_pos::time::instant(next);
            let _ = self.books.with(|ledger| ledger.set_cursor(name, &next));
        }
    }

    /// One listed sale: its payment and its order's read recorded, and its
    /// order made when it's this till's to make. `Err` when the order
    /// couldn't be made, so the sale is found again.
    async fn take(&self, sale: &FoundSale) -> Result<(), String> {
        let seen = payment_seen(sale);
        let key = seen.sale();
        let updated_at = seen.updated_at;
        self.record(Fact::PosPaymentSeen(seen));
        let read_already = self
            .books
            .row(&key)
            .and_then(|row| row.shared.pos.order)
            .is_some_and(|read| read.updated_at >= updated_at);
        if !read_already {
            let read = match self.source.order(sale).await {
                Ok(read) => OrderRead {
                    sale: key.clone(),
                    updated_at,
                    receipt: None,
                    why: None,
                    customer_name: read.customer_name.clone(),
                }
                .with_receipt(read.receipt.as_ref(), sale),
                Err(e) => OrderRead {
                    sale: key.clone(),
                    updated_at,
                    receipt: None,
                    why: Some(format!("Couldn't read the order at the point of sale: {e}")),
                    customer_name: None,
                },
            };
            self.record(Fact::PosOrderRead(read));
        }
        let Some(view) = self.books.view(&key) else {
            return Ok(());
        };
        if view.has_order() || view.offered {
            return Ok(());
        }
        if !may_make_order(&view, &self.books.view_settings(), false) {
            // Another counter's cash sale: its own till makes the order.
            return Ok(());
        }
        let order = self.create_order(sale).await?;
        self.created(&key, &order, sale);
        Ok(())
    }

    /// Records the order made for `key`, and what it said of itself
    /// (another till may have made it already, and the store's events may
    /// have moved it on, or renewed it).
    fn created(&self, key: &SaleKey, order: &Order, sale: &FoundSale) {
        self.record(Fact::OrderCreated(OrderCreated {
            sale: key.clone(),
            order_id: OrderId::new(order.order_id.as_str()),
            url: Some(order.url.clone()).filter(|url| !url.is_empty()),
            payment_uri: order.payment_uri(),
            address: order.address.clone(),
            xmr: payment_uri::xmr(order.piconero),
            amount: order
                .amount
                .clone()
                .unwrap_or_else(|| decimal_amount(sale.amount_minor, &sale.currency)),
            currency: order
                .currency
                .clone()
                .unwrap_or_else(|| sale.currency.clone()),
            reference: order.merchant_order_id.clone(),
            expires_at: order.expires_at,
            confirmations_required: order.confirmations_required,
            device_name: None,
            renew: order.renew.clone(),
        }));
        self.on_order(order);
    }

    /// Asks the POS whether the sales the cashier was told to refund there
    /// are refunded now, at most every [`Till::REFUND_LOOK_SECONDS`]: a
    /// check runs every 2 s without a Bell, and each pending sale costs a
    /// request. The sheet's "I've refunded it" ([`Till::confirm_refund`])
    /// asks at once, so the cashier never waits on this. A failure waits
    /// for the next look. Only the sales this source's point of sale took
    /// are asked about.
    async fn look_for_refunds(&self, checking: &mut Checking) {
        let Refunds::InPos(refunds) = self.source.refunds() else {
            return;
        };
        let pos = self.source.point_of_sale();
        let state = self.books.state();
        let pending: Vec<(SaleKey, FoundSale)> = state
            .views
            .iter()
            .zip(&state.sales)
            .filter(|(_, sale)| sale.refund_pending_at.is_some() && sale.refunded_at.is_none())
            .filter_map(|(view, sale)| {
                let found = sale
                    .found
                    .as_ref()
                    .filter(|found| found.point_of_sale == pos)?;
                Some((view.sale.clone(), found.clone()))
            })
            .collect();
        if pending.is_empty() {
            return;
        }
        let at = self.clock.now();
        if checking
            .last_refund_look_at
            .is_some_and(|last| at - last < Till::REFUND_LOOK_SECONDS)
        {
            return;
        }
        checking.last_refund_look_at = Some(at);
        for (key, found) in pending {
            if seen_refunded(refunds, &found).await {
                self.mark(&key, MarkKind::RefundConfirmed);
            }
        }
    }

    /// What the store's event stream said. On a till with no point of sale,
    /// a new sale's event makes the sale (with its plain code); on one with
    /// a point of sale, an event for an order not yet made here waits in
    /// the ledger for it.
    pub fn on_event(&self, event: &SaleEvent) {
        let Some(phase) = event.phase() else {
            return;
        };
        let order_id = OrderId::new(event.order_id.as_str());
        if !self.with_pos && phase == SalePhase::Waiting {
            let known = self
                .books
                .with(|ledger| ledger.order(&order_id))
                .ok()
                .flatten();
            if known.is_none() {
                self.record(Fact::OrderCreated(event_order(event)));
            }
        }
        self.record(Fact::OrderObserved(OrderObserved {
            order_id,
            phase,
            confirmations: stepped(phase),
            is_final: false,
            received: None,
            overpaid: false,
            // The quote the order had then: a renewal's `sale.waiting`
            // carries its new one (core.md decision 169).
            quote: Some(Quote {
                expires_at: event.expires_at,
                piconero: event.piconero,
            }),
            renew: None,
        }));
    }

    /// What a read of an order said about its sale (after the stream
    /// reconnects, say): its phase, whether it's [`TillSale::settled`], and
    /// its confirmations when Monokulo said.
    pub fn on_status(&self, order_id: &str, status: OrderStatus) {
        self.record(Fact::OrderObserved(OrderObserved {
            order_id: OrderId::new(order_id),
            phase: status.phase,
            confirmations: status.confirmations.or(stepped(status.phase)),
            is_final: status.settled,
            received: None,
            overpaid: false,
            quote: None,
            renew: None,
        }));
    }

    /// `order`, read again (or as a renewal answered): its phase, whether
    /// it's settled, its confirmations, its quote and its `renew-order`
    /// form.
    pub fn on_order(&self, order: &Order) {
        let order_id = OrderId::new(order.order_id.as_str());
        if let Some(required) = order.confirmations_required {
            self.learn_required(&order_id, required);
        }
        self.record(Fact::OrderObserved(OrderObserved {
            order_id,
            phase: order.phase(),
            confirmations: order.confirmations.or(stepped(order.phase())),
            is_final: order.is_final,
            received: None,
            overpaid: order.status.overpaid(),
            quote: Some(Quote {
                expires_at: order.expires_at,
                piconero: order.piconero,
            }),
            renew: order.renew.clone(),
        }));
    }

    /// The store's threshold for an order made before Monokulo said it (an
    /// older create-order answer): the order as made, with it, which the
    /// ledger's merge takes over the one without.
    fn learn_required(&self, order_id: &OrderId, required: u32) {
        let Ok(Some(key)) = self.books.with(|ledger| ledger.order(order_id)) else {
            return;
        };
        let Some(row) = self.books.row(&key) else {
            return;
        };
        let Some(made) = row.shared.order.filter(|o| o.order.order_id == *order_id) else {
            return;
        };
        if made.order.confirmations_required.is_some() {
            return;
        }
        let o = &made.order;
        self.record(Fact::OrderCreated(OrderCreated {
            sale: key.clone(),
            order_id: o.order_id.clone(),
            url: o.url.clone(),
            payment_uri: o.payment_uri.clone(),
            address: o.address.clone(),
            xmr: o.xmr.clone(),
            amount: o.amount.clone(),
            currency: o.currency.clone(),
            reference: o.reference.clone(),
            expires_at: o.expires_at,
            confirmations_required: Some(required),
            device_name: o.device_name.clone(),
            renew: o.renew.clone(),
        }));
    }

    /// Reads every sale whose payment is seen or confirming
    /// ([`TillSale::confirming`]) at once and then each
    /// [`Till::CONFIRMATIONS_READ`], so its ring and its "4 of 10" follow
    /// the blocks: the store's event stream says when a payment is seen and
    /// when it's first in a block, never each confirmation after. One wait,
    /// started again whenever which sales those are changes (a sale paid, a
    /// new one seen); nothing is read while there are none. A read that
    /// fails waits for the next; one that lands after the sale was paid
    /// can't take it back (the ledger's merge). Runs until dropped.
    pub async fn follow_confirmations(&self) {
        let mut rx = self.books.subscribe();
        let mut urls = confirming_urls(&rx.borrow_and_update());
        loop {
            urls = tokio::select! {
                () = self.read_confirmations(&urls) => unreachable!("reading never ends by itself"),
                urls = changed(&mut rx, &urls, confirming_urls) => urls,
            };
        }
    }

    /// Reads `urls` now and every [`Till::CONFIRMATIONS_READ`]; never ends.
    async fn read_confirmations(&self, urls: &[String]) {
        if urls.is_empty() {
            return pending().await;
        }
        loop {
            for url in urls {
                if let Ok(order) = self.monokulo.order(&self.connection, url).await {
                    self.on_order(&order);
                }
            }
            tokio::time::sleep(Till::CONFIRMATIONS_READ).await;
        }
    }

    /// The cashier closed the sale for now: its code comes down and it goes
    /// to the shift, where Monokulo's late payment still reaches it and the
    /// cashier can refund it. Nothing is said to Monokulo (docs/ledger.md
    /// §7: a close stays on this till).
    pub fn close(&self, order_id: &str) {
        if let Some((key, _)) = self.sale(order_id) {
            self.mark(&key, MarkKind::Closed);
        }
    }

    /// Watching stopped: every live sale's code comes down, closed as the
    /// close sheet's "Not sure" closes one, so none stays up with nothing
    /// watching it and each waits in the shift to be refunded or paid late;
    /// and the shift's window ends.
    pub fn close_live(&self) {
        let state = self.books.state();
        for (view, sale) in state.views.iter().zip(&state.sales) {
            if sale.live() {
                self.mark(&view.sale, MarkKind::Closed);
            }
        }
        self.record(Fact::ShiftMarked {
            mark: ShiftMark::Stopped,
            at: self.now_ms(),
        });
    }

    /// The clock passed a live sale's `expires_at`: its code is ended by
    /// this device's clock ("No payment yet"), so it no longer waits,
    /// pulses or takes the panel, even when Monokulo is late to say so. A
    /// local mark only: Monokulo's phase is untouched, and a payment still
    /// pays it (decision 174 in docs/decisions/android.md).
    pub fn lapse(&self) {
        let at = self.clock.now();
        let state = self.books.state();
        for (view, sale) in state.views.iter().zip(&state.sales) {
            if sale.live() && (1..=at).contains(&sale.expires_at) {
                self.mark(&view.sale, MarkKind::ClockLapsed);
            }
        }
    }

    /// Ends each live code when its time runs out ([`Till::lapse`]),
    /// whether or not Monokulo has said so yet: one wait, on `now_ms` (Unix
    /// milliseconds), for the next code to run out, started again whenever
    /// that changes. A lapse that ends nothing (the wall clock stepped back
    /// during the wait, or the till's clock reads a second behind `now_ms`)
    /// tries again [`Till::LAPSE_RETRY`] later until the code is ended, so a
    /// dead code never stays up waiting for some other change. Runs until
    /// dropped.
    pub async fn lapse_on_time(&self, now_ms: impl Fn() -> i64) {
        let mut rx = self.books.subscribe();
        let mut next = Till::next_lapse(&rx.borrow_and_update());
        loop {
            next = tokio::select! {
                () = self.lapse_when(next, &now_ms) => unreachable!("lapsing never ends by itself"),
                next = changed(&mut rx, &next, Till::next_lapse) => next,
            };
        }
    }

    /// Waits for `next` by `now_ms` and lapses until it's ended; never ends.
    async fn lapse_when(&self, next: Option<i64>, now_ms: &impl Fn() -> i64) {
        let Some(next) = next else {
            return pending().await;
        };
        let wait = (next.saturating_mul(1000) - now_ms()).max(0);
        tokio::time::sleep(Duration::from_millis(wait as u64)).await;
        loop {
            self.lapse();
            // Ended, the sales change and this wait is replaced by the next one.
            if Till::next_lapse(&self.books.state()) != Some(next) {
                break;
            }
            tokio::time::sleep(Till::LAPSE_RETRY).await;
        }
        pending().await
    }

    /// "Show the code again" on a closed sale: its code is back up.
    pub fn reopen(&self, order_id: &str) {
        if let Some((key, _)) = self.sale(order_id) {
            self.mark(&key, MarkKind::Reopened);
        }
    }

    /// The Monokulo order for `sale`: its amount, currency and receipt
    /// number, and the idempotency key, never whom it's for (names stay on
    /// the device). What went wrong, in the till's words.
    async fn create_order(&self, sale: &FoundSale) -> Result<Order, String> {
        let started = std::time::Instant::now();
        let made = self
            .monokulo
            .create_order(
                &self.connection,
                &decimal_amount(sale.amount_minor, &sale.currency),
                &sale.currency,
                sale.reference.as_deref(),
                &Till::idempotency_key(sale),
            )
            .await
            .map_err(|e| creation_problem(&e));
        if let Some(timer) = &self.timer {
            timer("monokulo.create", started.elapsed());
        }
        made
    }

    /// Notes that the cashier asked for `action` on `key`, runs `act`, and
    /// notes how it ended; what went wrong is also kept as the till's
    /// problem.
    async fn acted(
        &self,
        key: &SaleKey,
        action: Action,
        act: impl Future<Output = Result<Order, String>>,
    ) -> Result<Order, String> {
        self.record(Fact::ActionAsked {
            sale: key.clone(),
            action,
            at: self.now_ms(),
        });
        let made = act.await;
        self.record(Fact::ActionDone {
            sale: key.clone(),
            action,
            outcome: match &made {
                Ok(_) => Outcome::Done,
                Err(e) => Outcome::Failed(e.clone()),
            },
            at: self.now_ms(),
        });
        if let Err(e) = &made {
            self.books.change(|s| TillState {
                problem: Some(e.clone()),
                ..s.clone()
            });
        }
        made
    }

    /// Makes the order for `key` as the cashier asked (`action`), noting the
    /// action and how it ended: the new order's id, or what went wrong
    /// (also kept as the till's problem).
    async fn order_for(
        &self,
        key: &SaleKey,
        found: &FoundSale,
        action: Action,
    ) -> Result<String, String> {
        let order = self.acted(key, action, self.create_order(found)).await?;
        self.created(key, &order, found);
        Ok(order.order_id)
    }

    /// "Take it in Monero" on an offered cash sale: its order is made now,
    /// by the cashier, from any till. The new order's id, or what went
    /// wrong. A sale that isn't offered any more answers its own id.
    pub async fn take_in_monero(&self, order_id: &str) -> Result<String, String> {
        let Some((key, offered)) = self.sale(order_id) else {
            return Err("That sale isn't on the till any more.".into());
        };
        let Some(found) = offered.found.clone().filter(|_| offered.offered) else {
            return Ok(order_id.to_owned());
        };
        self.order_for(&key, &found, Action::TakeInMonero).await
    }

    /// "New code" on a sale whose code ran out unpaid (docs/ledger.md §7,
    /// core.md decisions 167 to 172): Monokulo renews its one order, the
    /// same order and address with a new price and expiry, through the
    /// order's `renew-order` form, for the code the sale shows (so two
    /// tills asking at once get one renewal). Only when the order offers
    /// the form: never a second order instead. The order's id (unchanged),
    /// or what went wrong.
    pub async fn new_code(&self, order_id: &str) -> Result<String, String> {
        let Some((key, sale)) = self.sale(order_id) else {
            return Err("That sale isn't on the till any more.".into());
        };
        if !sale.may_new_code() {
            return Err("That sale can't have a new code.".into());
        }
        let order = self
            .books
            .row(&key)
            .and_then(|row| row.shared.order)
            .map(|o| o.order);
        let Some((renew, expires_at)) = order.and_then(|o| Some((o.renew?, o.expires_at))) else {
            return Err(CANT_RENEW.into());
        };
        let renewing = async {
            self.monokulo
                .renew_order(&self.connection, &renew, expires_at)
                .await
                .map_err(|e| renewal_problem(&e))
        };
        let order = self.acted(&key, Action::NewCode, renewing).await?;
        self.on_order(&order);
        Ok(order.order_id)
    }

    /// "Keep as cash" on an offered cash sale: it stays listed, quietly.
    pub fn keep_as_cash(&self, order_id: &str) {
        if let Some((key, sale)) = self.sale(order_id)
            && sale.offered
        {
            self.mark(&key, MarkKind::KeptAsCash);
        }
    }

    /// "Paid in cash" on a cash sale with a code: the customer paid real
    /// cash after all, so its code comes down and nothing is refunded (the
    /// point of sale already has the cash). Only a cash sale, and never one
    /// already paid in Monero.
    pub fn paid_in_cash(&self, order_id: &str) {
        if let Some((key, sale)) = self.sale(order_id)
            && sale.cash()
            && !sale.offered
            && sale.phase != SalePhase::Paid
        {
            self.mark(&key, MarkKind::PaidInCash);
        }
    }

    /// Refunds the POS sale of an order that was never paid: through the
    /// POS when the till can ([`Refunds::ByTill`]), else marks it for the
    /// cashier to refund there ([`Refunds::InPos`]), which sends nothing. A
    /// cash sale is never refunded at all ([`TillSale::cash`]). A
    /// sale another point of sale took (the cashier changed it since) is
    /// only ever marked: its payment id means nothing to this source's POS.
    /// A paid sale is never refunded from here, nor one already refunded:
    /// the payment may land while the cashier's "No" is on its way. Why the
    /// POS refused is kept with the sale ([`TillState::refund_problems`]).
    pub async fn refund(&self, order_id: &str) {
        let Some((key, sale)) = self.sale(order_id) else {
            return;
        };
        if sale.phase == SalePhase::Paid || sale.refunded_at.is_some() {
            return;
        }
        let Some(found) = sale.found else {
            return;
        };
        if found.cash {
            // Never refunded through the point of sale: it took cash, which
            // the cashier hands back if need be (core.md decision 116).
            return;
        }
        let by_till = match self.source.refunds() {
            Refunds::ByTill(by_till) if found.point_of_sale == self.source.point_of_sale() => {
                by_till
            }
            _ => {
                self.mark(&key, MarkKind::RefundPending);
                return;
            }
        };
        self.record(Fact::ActionAsked {
            sale: key.clone(),
            action: Action::Refund,
            at: self.now_ms(),
        });
        let outcome = match by_till.refund(&found).await {
            Ok(made) => {
                // Under the point of sale's own id, as its refunds listing
                // gives it to every till; its idempotency key when it
                // answered without one.
                let (refund_id, status) = match made {
                    Some(made) => (made.id, RefundStatus::of_pos(&made.status)),
                    None => (refund_key(&found), RefundStatus::Completed),
                };
                self.record(Fact::PosRefundSeen(RefundSeen {
                    sale: key.clone(),
                    refund_id,
                    amount_minor: found.amount_minor,
                    status,
                }));
                Outcome::Done
            }
            Err(e) => Outcome::Failed(format!(
                "Couldn't refund it in the POS: {e}. Refund it there instead."
            )),
        };
        self.record(Fact::ActionDone {
            sale: key,
            action: Action::Refund,
            outcome,
            at: self.now_ms(),
        });
    }

    /// The refund sheet's "I've refunded it", for a sale the cashier was
    /// told to refund in the POS ([`TillSale::refund_pending_at`]).
    ///
    /// When the sale's point of sale is this source's and the source can
    /// ask it ([`Refunds::InPos`]), asks now: true, and the sale is
    /// refunded, when it shows the refund; false, and the sale stays as it
    /// was, when it doesn't yet or couldn't be asked.
    ///
    /// Otherwise nothing can check: the sale's point of sale isn't the
    /// till's any more (the cashier changed it), or the source can't ask
    /// its own ([`Refunds::ByTill`]). The cashier's word marks the sale
    /// refunded, so it isn't left needing action for the rest of the shift.
    pub async fn confirm_refund(&self, order_id: &str) -> bool {
        let Some((key, sale)) = self.sale(order_id) else {
            return false;
        };
        if sale.refunded_at.is_some() {
            return true;
        }
        if sale.refund_pending_at.is_none() {
            return false;
        }
        let Some(found) = sale.found else {
            return false;
        };
        if let Refunds::InPos(refunds) = self.source.refunds()
            && found.point_of_sale == self.source.point_of_sale()
            && !seen_refunded(refunds, &found).await
        {
            return false;
        }
        self.mark(&key, MarkKind::RefundConfirmed);
        true
    }
}

/// The confirmations a phase stands for when Monokulo gives no count: seen,
/// in the pool with none; confirming, at least the block it's in.
fn stepped(phase: SalePhase) -> Option<u32> {
    match phase {
        SalePhase::Seen => Some(0),
        SalePhase::Confirming => Some(1),
        _ => None,
    }
}

/// The idempotency key of a refund made through the POS (decision 21): the
/// refund's id in the ledger.
fn refund_key(found: &FoundSale) -> String {
    format!("till-refund-{}", found.id)
        .chars()
        .take(45)
        .collect()
}

/// Where the till's place in `pos`'s refunds listing is kept in the ledger.
fn refunds_cursor(pos: till_pos::PointOfSale) -> &'static str {
    match pos {
        till_pos::PointOfSale::Square => "square.refunds",
        till_pos::PointOfSale::Clover => "clover.refunds",
    }
}

/// A listed sale as the ledger's fact, with the payment's status and
/// update time as the point of sale gave them (Square's `status` and
/// `updated_at`), so a payment it changed since replaces the one seen and
/// has its order read again. Where the listing gives no update time
/// (Clover), the payment's own time stands for it: the POS only lists a
/// payment once it's completed.
fn payment_seen(sale: &FoundSale) -> PaymentSeen {
    PaymentSeen {
        pos: sale.point_of_sale,
        payment: PaymentId::new(sale.id.as_str()),
        amount_minor: sale.amount_minor,
        currency: sale.currency.clone(),
        tender: if sale.cash {
            Tender::Cash
        } else {
            Tender::Monero
        },
        status: sale
            .status
            .clone()
            .unwrap_or(till_pos::PaymentStatus::Completed),
        device_id: sale.device_id.as_deref().map(Into::into),
        device_name: sale.device_name.clone(),
        receipt_number: sale.reference.clone(),
        order_ref: sale.order_id.clone(),
        title: sale.title.clone(),
        created_at: sale.created_at.clone(),
        updated_at: sale
            .updated_at
            .as_deref()
            .and_then(till_pos::time::unix_millis)
            .or_else(|| till_pos::time::unix_millis(&sale.created_at))
            .unwrap_or(0),
    }
}

/// Builds an [`OrderRead`]'s receipt, or says why there is none.
trait WithReceipt {
    fn with_receipt(self, receipt: Option<&orf_codec::Receipt>, sale: &FoundSale) -> OrderRead;
}

impl WithReceipt for OrderRead {
    fn with_receipt(self, receipt: Option<&orf_codec::Receipt>, sale: &FoundSale) -> OrderRead {
        let reason = if sale.order_id.is_none() {
            "The point of sale gave no order for this sale."
        } else {
            "The point of sale's order couldn't be made into a receipt."
        };
        let built = OrderRead::new(
            self.sale.clone(),
            self.updated_at,
            receipt.ok_or_else(|| reason.to_owned()),
            self.customer_name.clone(),
        );
        OrderRead {
            receipt: built.receipt,
            why: built.why,
            ..self
        }
    }
}

/// The order a `sale.waiting` event describes, for a till with no point of
/// sale: the sale is known only from Monokulo, its code plain.
fn event_order(event: &SaleEvent) -> OrderCreated {
    OrderCreated {
        sale: SaleKey::Order(OrderId::new(event.order_id.as_str())),
        order_id: OrderId::new(event.order_id.as_str()),
        url: event.order_url.clone(),
        payment_uri: payment_uri::build(&event.address, event.piconero),
        address: event.address.clone(),
        xmr: payment_uri::xmr(event.piconero),
        amount: event.amount.clone(),
        currency: event.currency.clone(),
        reference: event.merchant_order_id.clone(),
        expires_at: event.expires_at,
        confirmations_required: None,
        device_name: Some(event.device_name.clone()).filter(|name| !name.trim().is_empty()),
        renew: None,
    }
}

/// `e` shown, unless the sign-in has already ended and `e` only says so
/// again: the words that said why it ended ("Clover's sign-in has ended")
/// stay until the cashier signs in, rather than the next check's bare
/// "Sign in to Clover." A failure that isn't the POS's answer or the
/// till's own refusal (decision 14) is shown after "Couldn't reach the
/// POS: ".
fn with_pos_problem(s: &TillState, e: &PosError) -> TillState {
    match e {
        PosError::Answered { .. } if s.sign_in_again && e.sign_in_again() => s.clone(),
        PosError::Answered { .. } | PosError::NotAsked(_) => TillState {
            problem: Some(e.to_string()),
            sign_in_again: e.sign_in_again(),
            ..s.clone()
        },
        _ => TillState {
            problem: Some(format!("Couldn't reach the POS: {e}")),
            ..s.clone()
        },
    }
}

/// What the till says when an order couldn't be made: what Monokulo's
/// problem means and who can fix it ([`problems::say`]). No order was made,
/// so the sale stays unhandled and the next check asks again with the same
/// idempotency key.
/// What New code says when the store's Monokulo can't renew an order
/// (core.md decision 171).
const CANT_RENEW: &str =
    "This store's Monokulo can't make a new code for a sale. Update it, or ring the sale up again.";

/// What a failed renewal says: Monokulo's own words when it refused (a
/// payment reached the order, say); [`CANT_RENEW`] when the form wasn't
/// there to send.
fn renewal_problem(e: &MonokuloError) -> String {
    let gone = e.status() == 405 || (e.status() == 404 && e.problem() != Some("not-found"));
    if gone {
        CANT_RENEW.into()
    } else if e.answered() {
        problems::say(e)
    } else {
        format!("Couldn't make a new code: {e}")
    }
}

fn creation_problem(e: &MonokuloError) -> String {
    if e.answered() {
        problems::say(e)
    } else {
        format!("Couldn't make the payment code: {e}")
    }
}

/// Whether the POS shows `found` refunded; false when it couldn't be asked.
async fn seen_refunded(refunds: &dyn RefundInPos, found: &FoundSale) -> bool {
    refunds.refunded(found).await.unwrap_or(false)
}

/// The `self` links of the sales worth following ([`TillSale::confirming`]).
fn confirming_urls(state: &TillState) -> Vec<String> {
    state
        .sales
        .iter()
        .filter(|sale| sale.confirming())
        .filter_map(|sale| sale.url.clone())
        .collect()
}

/// Waits until `of` the state differs from `current`, and gives it. The
/// books hold the sender, so this waits for as long as they're there.
async fn changed<T: PartialEq>(
    rx: &mut watch::Receiver<TillState>,
    current: &T,
    of: impl Fn(&TillState) -> T,
) -> T {
    loop {
        if rx.changed().await.is_err() {
            return pending().await;
        }
        let new = of(&rx.borrow_and_update());
        if new != *current {
            return new;
        }
    }
}
