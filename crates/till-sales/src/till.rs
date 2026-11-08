//! The till's state machine (Android: `Till.kt`).

use std::collections::HashSet;
use std::future::pending;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use monokulo_client::{
    Clock, Connection, MonokuloApi, MonokuloError, Order, SaleEvent, SalePhase, SystemClock,
    payment_uri, problems,
};
use sha2::{Digest, Sha256};
use till_pos::{FoundSale, PosError, RefundInPos, Refunds, SaleOrder, SaleSource, decimal_amount};
use tokio::sync::watch;

use crate::{PaymentCode, TillSale, TillState};

#[cfg(test)]
mod tests;

/// Where the till keeps this shift's sales between runs (decision 64 in
/// docs/decisions/android.md): the app backs it with its encrypted store,
/// since a sale's receipt says what was bought. [`crate::shift_json`]
/// writes and reads them as the Android app does.
pub trait ShiftStore: Send + Sync {
    /// The sales as last saved, newest first; none when nothing was.
    fn load(&self) -> Vec<TillSale>;

    /// Keeps `sales` in place of what was saved.
    fn save(&self, sales: &[TillSale]);
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
/// orders, puts receipts in their codes, and follows them to paid or
/// expired. A sale it finds is handled once it has an order; until then
/// every [`Till::check`] tries again, and the idempotency key makes a
/// retry, or another till finding the same sale, get the same order.
///
/// Its state is a [`watch`] channel ([`Till::subscribe`]); every change to
/// the sales is saved to its [`ShiftStore`], when it has one.
pub struct Till {
    source: Arc<dyn SaleSource>,
    monokulo: MonokuloApi,
    connection: Connection,
    device_id: Option<String>,
    clock: Arc<dyn Clock>,
    store: Option<Arc<dyn ShiftStore>>,
    state: watch::Sender<TillState>,
    checking: tokio::sync::Mutex<Checking>,
    saving: Mutex<()>,
}

impl Till {
    /// The most sales a shift keeps, newest first: older ones drop off the
    /// list and the saved shift.
    pub const MAX_SALES: usize = 500;

    /// The timer asks the POS about pending refunds at most this often, in
    /// seconds (decision 130 in docs/decisions/android.md).
    pub const REFUND_LOOK_SECONDS: i64 = 30;

    /// How often [`Till::follow_confirmations`] reads a seen or confirming
    /// sale again: a Monero block comes about every two minutes, so a
    /// confirmation shows within a sixth of one, for a request a sale every
    /// 20 s while it's on its way.
    pub const CONFIRMATIONS_READ: Duration = Duration::from_secs(20);

    /// How long [`Till::lapse_on_time`] waits to try again when the clock it
    /// lapsed by hadn't reached a code's end yet.
    pub const LAPSE_RETRY: Duration = Duration::from_secs(1);

    /// A till over `source`, making orders at `connection`'s store. It takes
    /// every device's sales, reads the device's clock and saves nothing
    /// until told otherwise ([`Till::with_device_id`], [`Till::with_clock`],
    /// [`Till::with_store`]).
    pub fn new(source: Arc<dyn SaleSource>, monokulo: MonokuloApi, connection: Connection) -> Till {
        Till {
            source,
            monokulo,
            connection,
            device_id: None,
            clock: Arc::new(SystemClock),
            store: None,
            state: watch::Sender::new(TillState::default()),
            checking: tokio::sync::Mutex::new(Checking::default()),
            saving: Mutex::new(()),
        }
    }

    /// The same till, taking only `device_id`'s sales when the POS says
    /// which device took each; another device's are marked handled and
    /// left alone.
    pub fn with_device_id(self, device_id: impl Into<String>) -> Till {
        Till {
            device_id: Some(device_id.into()),
            ..self
        }
    }

    /// The same till, stamping the cashier's marks and a resync's with
    /// `clock`'s time.
    pub fn with_clock(self, clock: Arc<dyn Clock>) -> Till {
        Till { clock, ..self }
    }

    /// The same till, starting from the sales `store` saved (cut down to
    /// [`Till::MAX_SALES`]) and saving them again on every change.
    pub fn with_store(self, store: Arc<dyn ShiftStore>) -> Till {
        let mut sales = store.load();
        sales.truncate(Till::MAX_SALES);
        self.state.send_replace(TillState {
            sales,
            ..TillState::default()
        });
        Till {
            store: Some(store),
            ..self
        }
    }

    /// The till's state as it changes: its sales, newest first, and the
    /// last thing that went wrong.
    pub fn subscribe(&self) -> watch::Receiver<TillState> {
        self.state.subscribe()
    }

    /// The till's state now.
    pub fn state(&self) -> TillState {
        self.state.borrow().clone()
    }

    /// The point of sale the till's source watches.
    pub fn point_of_sale(&self) -> till_pos::PointOfSale {
        self.source.point_of_sale()
    }

    /// `sq:` and the start of SHA-256 of the POS's payment id: the same for
    /// every till, and opaque.
    pub fn idempotency_key(sale: &FoundSale) -> String {
        let hash = Sha256::digest(sale.id.as_bytes());
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        format!("sq:{}", &hex[..40])
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

    /// Changes the till's state, keeping at most [`Till::MAX_SALES`] sales,
    /// and saves the sales when they changed. The save writes the state as
    /// it is then, not as this change left it, so two changes racing still
    /// leave the latest on disk.
    fn change(&self, block: impl FnOnce(&TillState) -> TillState) {
        let mut moved = false;
        self.state.send_if_modified(|state| {
            let mut new = block(state);
            new.sales.truncate(Till::MAX_SALES);
            moved = new.sales != state.sales;
            let modified = new != *state;
            *state = new;
            modified
        });
        if let (true, Some(store)) = (moved, &self.store) {
            let _saving = self.saving.lock().unwrap_or_else(|e| e.into_inner());
            let sales = self.state.borrow().sales.clone();
            store.save(&sales);
        }
    }

    /// Changes the sale for `order_id` with `f`, leaving the rest.
    fn change_sale(&self, order_id: &str, f: impl Fn(&TillSale) -> TillSale) {
        self.change(|s| TillState {
            sales: s
                .sales
                .iter()
                .map(|sale| {
                    if sale.order_id == order_id {
                        f(sale)
                    } else {
                        sale.clone()
                    }
                })
                .collect(),
            ..s.clone()
        });
    }

    /// Looks for new sales; makes each an order with its code. Answers how
    /// many of the POS's sales this check found that the last one hadn't,
    /// another device's included (the Bell rings for every sale at the
    /// location): a sale whose order failed, found again, isn't counted
    /// twice. 0 when the POS couldn't be read.
    pub async fn check(&self) -> usize {
        let mut checking = self.checking.lock().await;
        let found = match self.source.new_sales().await {
            Ok(found) => found,
            Err(e) => {
                self.change(|s| with_pos_problem(s, &e));
                return 0;
            }
        };
        let fresh = found
            .iter()
            .filter(|sale| !checking.last_found.contains(&sale.id))
            .count();
        checking.last_found = found.iter().map(|sale| sale.id.clone()).collect();
        let mut problem = None;
        for sale in &found {
            if let (Some(mine), Some(theirs)) = (&self.device_id, &sale.device_id)
                && mine != theirs
            {
                self.source.handled(sale);
                continue;
            }
            let order = self
                .monokulo
                .create_order(
                    &self.connection,
                    &decimal_amount(sale.amount_minor, &sale.currency),
                    &sale.currency,
                    sale.reference.as_deref(),
                    &Till::idempotency_key(sale),
                )
                .await;
            let order = match order {
                Ok(order) => order,
                Err(e) => {
                    problem = Some(creation_problem(&e));
                    continue;
                }
            };
            self.source.handled(sale);
            let read = self.source.order(sale).await.ok();
            let code = PaymentCode::of(
                &order.payment_uri(),
                read.as_ref().and_then(|read| read.receipt.as_ref()),
            );
            self.add(&order, code, read, sale);
        }
        self.change(|s| TillState {
            problem: problem.clone(),
            sign_in_again: false,
            ..s.clone()
        });
        self.look_for_refunds(&mut checking).await;
        fresh
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
        let pending: Vec<(String, FoundSale)> = self
            .state
            .borrow()
            .sales
            .iter()
            .filter(|sale| sale.refund_pending_at.is_some() && sale.refunded_at.is_none())
            .filter_map(|sale| {
                let found = sale
                    .found
                    .as_ref()
                    .filter(|found| found.point_of_sale == pos)?;
                Some((sale.order_id.clone(), found.clone()))
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
        for (order_id, found) in pending {
            if seen_refunded(refunds, &found).await {
                self.mark_refunded(&order_id);
            }
        }
    }

    fn mark_refunded(&self, order_id: &str) {
        let at = self.clock.now();
        self.change_sale(order_id, |sale| TillSale {
            refunded_at: sale.refunded_at.or(Some(at)),
            ..sale.clone()
        });
    }

    /// What the store's event stream said.
    pub fn on_event(&self, event: &SaleEvent) {
        let Some(phase) = event.phase() else {
            return;
        };
        self.change(|s| {
            if s.sales.iter().any(|sale| sale.order_id == event.order_id) {
                TillState {
                    sales: s
                        .sales
                        .iter()
                        .map(|sale| {
                            if sale.order_id == event.order_id {
                                TillSale {
                                    url: sale.url.clone().or_else(|| event.order_url.clone()),
                                    ..moved(sale, phase, event.at)
                                }
                            } else {
                                sale.clone()
                            }
                        })
                        .collect(),
                    ..s.clone()
                }
            } else if phase == SalePhase::Waiting {
                // Another till's sale: shown with the plain code (no receipt here).
                let mut sales = Vec::with_capacity(s.sales.len() + 1);
                sales.push(event_sale(event));
                sales.extend(s.sales.iter().cloned());
                TillState { sales, ..s.clone() }
            } else {
                s.clone()
            }
        });
    }

    /// What a read of an order said about its sale (after the stream
    /// reconnects, say): its phase, whether it's [`TillSale::settled`], and
    /// its confirmations of those it needs when Monokulo said.
    pub fn on_status(&self, order_id: &str, status: OrderStatus) {
        let at = self.clock.now();
        self.change_sale(order_id, |sale| read(sale, status, at));
    }

    /// `order`, read again: its phase, whether it's settled and its
    /// confirmations.
    pub fn on_order(&self, order: &Order) {
        self.on_status(&order.order_id, OrderStatus::from(order));
    }

    /// A [`Till::follow_confirmations`] read of `order`, kept only while
    /// its sale is still [`TillSale::confirming`]: a read sent before
    /// `sale.paid` that lands after it would take a paid sale back to
    /// confirming, and its paid note would sound again when the next read
    /// closed it.
    pub(crate) fn on_confirmations(&self, order: &Order) {
        let at = self.clock.now();
        let status = OrderStatus::from(order);
        self.change_sale(&order.order_id, |sale| {
            if sale.confirming() {
                read(sale, status, at)
            } else {
                sale.clone()
            }
        });
    }

    /// Reads every sale whose payment is seen or confirming
    /// ([`TillSale::confirming`]) at once and then each
    /// [`Till::CONFIRMATIONS_READ`], so its ring and its "4 of 10" follow
    /// the blocks: the store's event stream says when a payment is seen and
    /// when it's first in a block, never each confirmation after. One wait,
    /// started again whenever which sales those are changes (a sale paid, a
    /// new one seen); nothing is read while there are none. A read that
    /// fails waits for the next. Runs until dropped.
    pub async fn follow_confirmations(&self) {
        let mut rx = self.state.subscribe();
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
                    self.on_confirmations(&order);
                }
            }
            tokio::time::sleep(Till::CONFIRMATIONS_READ).await;
        }
    }

    /// The cashier closed the sale for now: its code comes down and it goes
    /// to the shift, where Monokulo's late payment still reaches it and the
    /// cashier can refund it. Nothing is said to Monokulo.
    pub fn close(&self, order_id: &str) {
        let at = self.clock.now();
        self.change_sale(order_id, |sale| TillSale {
            closed_at: sale.closed_at.or(Some(at)),
            ..sale.clone()
        });
    }

    /// Watching stopped: every live sale's code comes down, closed as the
    /// close sheet's "Not sure" closes one, so none stays up with nothing
    /// watching it and each waits in the shift to be refunded or paid late.
    pub fn close_live(&self) {
        let at = self.clock.now();
        self.change(|s| TillState {
            sales: s
                .sales
                .iter()
                .map(|sale| {
                    if sale.live() {
                        TillSale {
                            closed_at: Some(at),
                            ..sale.clone()
                        }
                    } else {
                        sale.clone()
                    }
                })
                .collect(),
            ..s.clone()
        });
    }

    /// The clock passed a live sale's `expires_at`: its code is ended by the
    /// clock (expired, as Monokulo will say), so it no longer waits, pulses
    /// or takes the panel, even when Monokulo is late to say so. Only a
    /// payment moves it on from there, as for any sale that ended unpaid
    /// (decision 174 in docs/decisions/android.md).
    pub fn lapse(&self) {
        let at = self.clock.now();
        self.change(|s| TillState {
            sales: s
                .sales
                .iter()
                .map(|sale| {
                    if sale.live() && (1..=at).contains(&sale.expires_at) {
                        moved(sale, SalePhase::Expired, at)
                    } else {
                        sale.clone()
                    }
                })
                .collect(),
            ..s.clone()
        });
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
        let mut rx = self.state.subscribe();
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
            if Till::next_lapse(&self.state.borrow()) != Some(next) {
                break;
            }
            tokio::time::sleep(Till::LAPSE_RETRY).await;
        }
        pending().await
    }

    /// "Show the code again" on a closed sale: its code is back up.
    pub fn reopen(&self, order_id: &str) {
        self.change_sale(order_id, |sale| TillSale {
            closed_at: None,
            ..sale.clone()
        });
    }

    /// Refunds the POS sale of an order that was never paid: through the
    /// POS when the till can ([`Refunds::ByTill`]), else marks it for the
    /// cashier to refund there ([`Refunds::InPos`]), which sends nothing. A
    /// sale another point of sale took (the cashier changed it since) is
    /// only ever marked: its payment id means nothing to this source's POS.
    /// A paid sale is never refunded from here, nor one already refunded:
    /// the payment may land while the cashier's "No" is on its way. Why the
    /// POS refused is kept in [`TillState::refund_problems`].
    pub async fn refund(&self, order_id: &str) {
        let Some(sale) = self.state.borrow().sale(order_id).cloned() else {
            return;
        };
        if sale.phase == SalePhase::Paid || sale.refunded_at.is_some() {
            return;
        }
        let Some(found) = sale.found else {
            return;
        };
        self.change(|s| {
            let mut s = s.clone();
            s.refund_problems.remove(order_id);
            s
        });
        let by_till = match self.source.refunds() {
            Refunds::ByTill(by_till) if found.point_of_sale == self.source.point_of_sale() => {
                by_till
            }
            _ => {
                let at = self.clock.now();
                self.change_sale(order_id, |sale| TillSale {
                    refund_pending_at: sale.refund_pending_at.or(Some(at)),
                    ..sale.clone()
                });
                return;
            }
        };
        match by_till.refund(&found).await {
            Ok(()) => self.mark_refunded(order_id),
            Err(e) => self.change(|s| {
                let mut s = s.clone();
                s.refund_problems.insert(
                    order_id.to_owned(),
                    format!("Couldn't refund it in the POS: {e}. Refund it there instead."),
                );
                s
            }),
        }
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
        let Some(sale) = self.state.borrow().sale(order_id).cloned() else {
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
        self.mark_refunded(order_id);
        true
    }

    fn add(&self, order: &Order, code: PaymentCode, read: Option<SaleOrder>, found: &FoundSale) {
        // Another till may have made this order already, and the store's
        // events may have moved it on (paid, even): keep what they said, and
        // when, else the order's own. Read and replaced in the one change, so
        // a close or a refund stamped meanwhile on another thread isn't lost.
        let created_at =
            till_pos::time::unix_seconds(&found.created_at).unwrap_or_else(|| self.clock.now());
        let (receipt, customer_name) = match read {
            Some(read) => (read.receipt, read.customer_name),
            None => (None, None),
        };
        self.change(|s| {
            let known = s.sale(&order.order_id);
            let sale = TillSale {
                order_id: order.order_id.clone(),
                amount: order
                    .amount
                    .clone()
                    .unwrap_or_else(|| decimal_amount(found.amount_minor, &found.currency)),
                currency: order
                    .currency
                    .clone()
                    .unwrap_or_else(|| found.currency.clone()),
                reference: order.merchant_order_id.clone(),
                code: code.clone(),
                phase: known.map_or(order.phase(), |known| known.phase),
                expires_at: order.expires_at,
                xmr: payment_uri::xmr(order.piconero),
                address: order.address.clone(),
                found: Some(found.clone()),
                receipt: receipt.clone(),
                customer_name: customer_name.clone(),
                device_name: None,
                url: Some(order.url.clone()).filter(|url| !url.is_empty()),
                settled: known.is_none() && order.is_final,
                created_at,
                seen_at: known.and_then(|known| known.seen_at),
                paid_at: known.and_then(|known| known.paid_at),
                ended_at: known.and_then(|known| known.ended_at),
                closed_at: known.and_then(|known| known.closed_at),
                refunded_at: known.and_then(|known| known.refunded_at),
                refund_pending_at: known.and_then(|known| known.refund_pending_at),
                confirmations: known
                    .and_then(|known| known.confirmations)
                    .or(order.confirmations),
                confirmations_required: order
                    .confirmations_required
                    .or(known.and_then(|known| known.confirmations_required)),
            };
            let mut sales = Vec::with_capacity(s.sales.len() + 1);
            sales.push(sale);
            sales.extend(
                s.sales
                    .iter()
                    .filter(|sale| sale.order_id != order.order_id)
                    .cloned(),
            );
            TillState { sales, ..s.clone() }
        });
    }
}

/// `e` shown, unless the sign-in has already ended and `e` only says so
/// again: the words that said why it ended ("Clover's sign-in has ended")
/// stay until the cashier signs in, rather than the next check's bare
/// "Sign in to Clover." A failure that isn't the POS's answer
/// (decision 14) is shown after "Couldn't reach the POS: ".
fn with_pos_problem(s: &TillState, e: &PosError) -> TillState {
    match e {
        PosError::Answered { .. } if s.sign_in_again && e.sign_in_again() => s.clone(),
        PosError::Answered { .. } => TillState {
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
/// till holds the sender, so this waits for as long as the till is there.
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

/// The sale as a read of its order left it: moved to its phase, and its
/// count where the read gave one.
fn read(sale: &TillSale, status: OrderStatus, at: i64) -> TillSale {
    let moved = moved(sale, status.phase, at);
    TillSale {
        settled: status.settled,
        confirmations: status.confirmations.or(moved.confirmations),
        confirmations_required: status.required.or(moved.confirmations_required),
        ..moved
    }
}

/// The sale moved on to `to`, the time of its first step there kept: seen
/// when its payment first showed, paid, or ended unpaid. A step already
/// timed keeps its time (a resync says what, not when). Seen, its payment
/// has no confirmation yet; confirming, at least the one block it's in,
/// until a read of the order says how many.
fn moved(sale: &TillSale, to: SalePhase, at: i64) -> TillSale {
    let phase = after(sale.phase, to);
    TillSale {
        phase,
        confirmations: match phase {
            SalePhase::Seen => Some(0),
            SalePhase::Confirming => Some(sale.confirmations.unwrap_or(1).max(1)),
            _ => sale.confirmations,
        },
        seen_at: sale.seen_at.or_else(|| {
            matches!(
                phase,
                SalePhase::Seen | SalePhase::Confirming | SalePhase::Partial
            )
            .then_some(at)
        }),
        paid_at: sale.paid_at.or((phase == SalePhase::Paid).then_some(at)),
        ended_at: sale.ended_at.or(phase.is_unpaid().then_some(at)),
        ..sale.clone()
    }
}

/// Where a sale in `phase` goes when Monokulo says `to`. Only a payment
/// moves a sale that ended unpaid: a reversed one stays reversed (its order
/// reads `pending` again once the double spend voided the payment, and
/// carries nothing saying so), and an expired or cancelled one never goes
/// back to waiting. Otherwise, what Monokulo says.
fn after(phase: SalePhase, to: SalePhase) -> SalePhase {
    if to == SalePhase::Paid {
        to
    } else if phase == SalePhase::DoubleSpend || (phase.is_unpaid() && !to.is_unpaid()) {
        phase
    } else {
        to
    }
}

/// Another till's sale, from its `sale.waiting` event: the plain code (no
/// receipt here), and the device that made it.
fn event_sale(event: &SaleEvent) -> TillSale {
    TillSale {
        order_id: event.order_id.clone(),
        amount: event.amount.clone(),
        currency: event.currency.clone(),
        reference: event.merchant_order_id.clone(),
        code: PaymentCode::plain(&payment_uri::build(&event.address, event.piconero)),
        phase: SalePhase::Waiting,
        expires_at: event.expires_at,
        xmr: payment_uri::xmr(event.piconero),
        address: event.address.clone(),
        found: None,
        receipt: None,
        customer_name: None,
        device_name: Some(event.device_name.clone()).filter(|name| !name.trim().is_empty()),
        url: event.order_url.clone(),
        settled: false,
        created_at: event.at,
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
