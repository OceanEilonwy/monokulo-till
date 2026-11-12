//! [`WalletBackend`]: the device's own wallet as the till's payment backend
//! (docs/view-key-mode.md §5). One fob-wallet wallet, view-only, in Manual
//! Sync: it never looks for blocks by itself, and Till asks it to only
//! while its event stream is polled, which the runner does only while the
//! app is in the foreground (§10).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use async_trait::async_trait;
use fob_wallet::{Ask, Incoming, KeyedRequest, SyncListener, SyncStatus, Wallet};
use futures_util::stream::{self, BoxStream, StreamExt};
use monokulo_client::{Clock, OrderStatus, SalePhase};
use till_payments::{
    BackendError, BackendEvent, BackendInfo, OrderId, Payment, PaymentBackend, PaymentRef,
    PaymentRequest, Sharing, WalletLink,
};
use till_types::{PriceUsed, UnixSeconds};
use tokio::sync::Notify;

use crate::keys::Network;
use crate::prices::{PriceSource, Prices, ProviderSource, provider_for};
use crate::status::{self, PaymentView, StatusInputs};

/// The wallet file's name in the engine's data directory, beside the
/// ledger's `till.db`.
pub const WALLET_FILE: &str = "wallet.db";

/// How often the stream looks while a code is on screen, while payments
/// are open, and otherwise (§5.5).
pub const FOCUSED: Duration = Duration::from_secs(2);
pub const OPEN: Duration = Duration::from_secs(10);
pub const IDLE: Duration = Duration::from_secs(60);

/// Where a wallet file is, in `data_dir`.
pub fn wallet_path(data_dir: &Path) -> PathBuf {
    data_dir.join(WALLET_FILE)
}

/// The file key kept in a link: 64 hex digits.
pub fn file_key_of(link: &WalletLink) -> Result<[u8; 32], BackendError> {
    let text = link.file_key.expose_str();
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| {
            text.get(i..i + 2)
                .and_then(|b| u8::from_str_radix(b, 16).ok())
        })
        .collect::<Option<Vec<u8>>>()
        .filter(|b| b.len() == 32)
        .ok_or_else(|| BackendError::Other("The wallet's key isn't one Till made.".into()))?;
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

/// The network a link names.
pub fn network_of(link: &WalletLink) -> Network {
    match link.network.as_str() {
        "stagenet" => Network::Stagenet,
        _ => Network::Mainnet,
    }
}

/// The device's own wallet, as a payment backend.
#[derive(Clone)]
pub struct WalletBackend {
    inner: Arc<Inner>,
}

struct Inner {
    wallet: Wallet,
    link: WalletLink,
    prices: Prices,
    clock: Arc<dyn Clock>,
    /// The code on screen, as the till last hinted.
    focus: Mutex<Option<PaymentRef>>,
    /// Woken by a hint, a request made, or the wallet saying requests
    /// changed.
    wake: Notify,
    /// The requests the wallet said changed, not yet looked at.
    changed: Mutex<BTreeSet<u64>>,
    /// What each request's payment was last said to be.
    told: Mutex<BTreeMap<u64, Payment>>,
    /// The prices each payment was made at, this run: a payment read again
    /// carries the price it was made at.
    priced: Mutex<BTreeMap<u64, PriceUsed>>,
}

/// Turns the wallet's word that requests changed into a turn of the stream.
struct Listener(Weak<Inner>);

impl SyncListener for Listener {
    fn on_status(&self, _: SyncStatus) {}

    fn on_requests(&self, ids: Vec<u64>) {
        if let Some(inner) = self.0.upgrade() {
            lock(&inner.changed).extend(ids);
            inner.wake.notify_one();
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn fob(e: fob_wallet::FobError) -> BackendError {
    BackendError::Other(e.to_string())
}

impl WalletBackend {
    /// Opens the wallet `link` names in `data_dir`, set up as the link
    /// says, prices from the link's provider.
    pub async fn open(
        data_dir: &Path,
        link: &WalletLink,
        clock: Arc<dyn Clock>,
    ) -> Result<WalletBackend, BackendError> {
        let provider = provider_for(
            link.price_provider.as_deref(),
            link.price_currency.as_deref(),
        );
        WalletBackend::open_with(data_dir, link, clock, Arc::new(ProviderSource(provider))).await
    }

    /// [`WalletBackend::open`] without waiting: the wallet file opened here
    /// and now, as it was set up when it was made ([`crate::make_wallet`]
    /// set it up as the link says, and a change in Settings sets it up
    /// again). For the engine's side, which makes its backend without
    /// waiting.
    pub fn open_now(
        data_dir: &Path,
        link: &WalletLink,
        clock: Arc<dyn Clock>,
    ) -> Result<WalletBackend, BackendError> {
        let key = file_key_of(link)?;
        let wallet = Wallet::open(&wallet_path(data_dir), key).map_err(fob)?;
        let provider = provider_for(
            link.price_provider.as_deref(),
            link.price_currency.as_deref(),
        );
        Ok(WalletBackend::over(
            wallet,
            link.clone(),
            clock,
            Arc::new(ProviderSource(provider)),
        ))
    }

    /// The backend over a wallet just made at the end of setup.
    pub fn of_made(made: crate::Made, clock: Arc<dyn Clock>) -> WalletBackend {
        let provider = provider_for(
            made.link.price_provider.as_deref(),
            made.link.price_currency.as_deref(),
        );
        WalletBackend::over(
            made.wallet,
            made.link,
            clock,
            Arc::new(ProviderSource(provider)),
        )
    }

    /// [`WalletBackend::open`] with its prices from `source`.
    pub async fn open_with(
        data_dir: &Path,
        link: &WalletLink,
        clock: Arc<dyn Clock>,
        source: Arc<dyn PriceSource>,
    ) -> Result<WalletBackend, BackendError> {
        let key = file_key_of(link)?;
        let path = wallet_path(data_dir);
        let wallet = tokio::task::spawn_blocking(move || Wallet::open(&path, key))
            .await
            .map_err(|e| BackendError::Other(e.to_string()))?
            .map_err(fob)?;
        apply(&wallet, link).await?;
        Ok(WalletBackend::over(wallet, link.clone(), clock, source))
    }

    fn over(
        wallet: Wallet,
        link: WalletLink,
        clock: Arc<dyn Clock>,
        source: Arc<dyn PriceSource>,
    ) -> WalletBackend {
        let inner = Arc::new(Inner {
            wallet,
            prices: Prices::new(source, clock.clone()),
            clock,
            link,
            focus: Mutex::new(None),
            wake: Notify::new(),
            changed: Mutex::new(BTreeSet::new()),
            told: Mutex::new(BTreeMap::new()),
            priced: Mutex::new(BTreeMap::new()),
        });
        inner
            .wallet
            .listen(Arc::new(Listener(Arc::downgrade(&inner))));
        WalletBackend { inner }
    }

    /// The link this backend was opened with.
    pub fn link(&self) -> &WalletLink {
        &self.inner.link
    }

    /// What Settings warns of (§5.3): money at this till's addresses that
    /// counts toward none of its payments, and a sign that another till
    /// uses the same number.
    pub async fn warnings(&self) -> Vec<String> {
        let unexpected = self.inner.wallet.unexpected_payments().await;
        let mut warnings = Vec::new();
        if unexpected.iter().any(|u| u.foreign_tag) {
            warnings.push(format!(
                "Another till may be using till number {}.",
                self.inner.link.till_number
            ));
        }
        let odd = unexpected.iter().filter(|u| !u.foreign_tag).count();
        if odd > 0 {
            warnings.push(if odd == 1 {
                "A payment of an unexpected amount: check it.".to_owned()
            } else {
                format!("{odd} payments of unexpected amounts: check them.")
            });
        }
        if self.inner.link.till_number > fob_wallet::wallet::POOL_SIZE {
            warnings.push(format!(
                "Till number {} is past the 50 accounts most wallets look at. Ask your wallet to look further, or it may not show this till's takings.",
                self.inner.link.till_number
            ));
        }
        warnings
    }

    /// Looks at the chain now: from the last block scanned to the newest,
    /// then the pool (`FromLast`, §5.4).
    pub async fn catch_up(&self) -> Result<(), BackendError> {
        self.inner.wallet.catch_up().await.map_err(fob)
    }
}

/// Sets the wallet up as `link` says: Manual Sync, this till's account,
/// Settled Confirmations, and its node or light server.
pub(crate) async fn apply(wallet: &Wallet, link: &WalletLink) -> Result<(), BackendError> {
    wallet.set_manual_sync(true).await.map_err(fob)?;
    wallet
        .set_receive_account(link.till_number.max(1))
        .await
        .map_err(fob)?;
    let confirmations = link.settled_confirmations.clamp(
        fob_wallet::manual::SETTLED_MIN,
        fob_wallet::manual::SETTLED_MAX,
    );
    if wallet.manual_settings().await.settled_confirmations != confirmations {
        wallet
            .set_settled_confirmations(confirmations)
            .await
            .map_err(fob)?;
    }
    match (&link.light_server, &link.node) {
        (Some(server), _) => {
            if wallet.light_server().await.map(|s| s.url) != Some(server.clone()) {
                wallet.set_light_server(server, false).await.map_err(fob)?;
            }
            wallet.use_light_wallet(true).await.map_err(fob)?;
        }
        (None, Some(node)) => {
            if wallet.light_mode().await {
                wallet.use_light_wallet(false).await.map_err(fob)?;
            }
            if wallet.node().await.url != fob_wallet::node::normalize(node) {
                wallet.set_node(node, false).await.map_err(fob)?;
            }
        }
        (None, None) => {}
    }
    Ok(())
}

impl Inner {
    fn now(&self) -> i64 {
        self.clock.now().0
    }

    fn settled(&self) -> u32 {
        self.link.settled_confirmations.clamp(
            fob_wallet::manual::SETTLED_MIN,
            fob_wallet::manual::SETTLED_MAX,
        )
    }

    /// The payment request `request` is, with the payments toward it.
    fn payment(&self, request: &KeyedRequest, incoming: &[Incoming]) -> Payment {
        let price = lock(&self.priced).get(&request.id).cloned();
        payment_of(request, incoming, self.settled(), self.now(), price)
    }

    async fn read(&self, id: u64) -> Option<Payment> {
        let request = self.wallet.keyed_request(id).await?;
        let incoming = self.wallet.request_payments(id).await.ok()?;
        Some(self.payment(&request, &incoming))
    }

    /// The requests still worth watching: not final, or ended within the
    /// late window.
    async fn watched(&self) -> Vec<KeyedRequest> {
        let now = self.now().max(0) as u64;
        self.wallet
            .keyed_requests()
            .await
            .into_iter()
            .filter(|r| r.releases() >= now)
            .collect()
    }

    /// How long until the stream looks again, as things stand.
    async fn pace(&self) -> Duration {
        if lock(&self.focus).is_some() {
            return FOCUSED;
        }
        let now = self.now().max(0) as u64;
        let open = self.wallet.keyed_requests().await.iter().any(|r| {
            r.open_at(now)
                || lock(&self.told)
                    .get(&r.id)
                    .is_some_and(|p| !p.is_final && r.releases() >= now)
        });
        if open { OPEN } else { IDLE }
    }

    /// One turn of the stream: the newest block asked for, a catch-up when
    /// there's a new one, else a look at the pool while payments are open;
    /// then every watched payment that changed.
    async fn turn(&self, look_at_pool: bool) -> Result<Vec<Payment>, BackendError> {
        let before = self.wallet.status().tip;
        self.wallet.refresh_tip().await;
        let tip = self.wallet.status().tip;
        if tip != before || before == 0 {
            self.wallet.catch_up().await.map_err(fob)?;
        } else if look_at_pool {
            self.wallet.look_at_pool().await.map_err(fob)?;
        }
        Ok(self.changes().await)
    }

    /// The payments that changed since they were last said: those the
    /// wallet named, and those whose time ran out.
    async fn changes(&self) -> Vec<Payment> {
        let named: BTreeSet<u64> = std::mem::take(&mut *lock(&self.changed));
        let mut ids: BTreeSet<u64> = self.watched().await.iter().map(|r| r.id).collect();
        ids.extend(named);
        let mut changed = Vec::new();
        for id in ids {
            let Some(payment) = self.read(id).await else {
                continue;
            };
            let mut told = lock(&self.told);
            if told.get(&id) != Some(&payment) {
                told.insert(id, payment.clone());
                changed.push(payment);
            }
        }
        changed
    }
}

/// The request's id in a payment's reference, `w:<account>:<index>:<tag>:<id>`.
pub fn request_id(reference: &str) -> Option<u64> {
    let rest = reference.strip_prefix("w:")?;
    let parts: Vec<&str> = rest.split(':').collect();
    match parts.as_slice() {
        [_, _, _, id] => id.parse().ok(),
        _ => None,
    }
}

/// A payment's reference and order id: `w:<account>:<index>:<tag>:<id>`,
/// unique however often its address is used again (§5.3).
pub fn reference_of(request: &KeyedRequest) -> String {
    format!(
        "w:{}:{}:{}:{}",
        request.account, request.minor, request.tag, request.id
    )
}

/// A request as the till's payment, judged from the payments toward it
/// (docs/view-key-mode.md §5.6): mokulo's status over those not double
/// spent, Settled Confirmations as the confirmations required, expiry from
/// the request. Final once Paid at Settled Confirmations, Cancelled, or
/// Expired (or double spent) past the late window.
pub fn payment_of(
    request: &KeyedRequest,
    incoming: &[Incoming],
    settled: u32,
    now: i64,
    price: Option<PriceUsed>,
) -> Payment {
    let views: Vec<PaymentView> = incoming
        .iter()
        .filter(|p| !p.double_spent)
        .map(|p| PaymentView {
            amount: p.atomic,
            confirmations: p.confirmations,
            is_zero_conf: p.height.is_none(),
        })
        .collect();
    let double_spent = incoming.iter().any(|p| p.double_spent);
    let status = status::derive_status(
        &views,
        StatusInputs {
            amount: request.atomic,
            confirmations_required: u64::from(settled),
            now,
            expires_at: request.expires_at as i64,
        },
    );
    let covered = matches!(
        status,
        OrderStatus::Paid
            | OrderStatus::Overpaid
            | OrderStatus::Confirming
            | OrderStatus::Unconfirmed
    );
    let phase = if covered {
        status.phase()
    } else if double_spent {
        SalePhase::DoubleSpend
    } else if request.cancelled.is_some() && status != OrderStatus::Partial {
        SalePhase::Cancelled
    } else {
        status.phase()
    };
    let past_late_window = now > request.releases() as i64;
    let is_final = match phase {
        SalePhase::Paid => true,
        SalePhase::Cancelled => true,
        SalePhase::Expired | SalePhase::DoubleSpend => past_late_window,
        _ => false,
    };
    let received = status::total(&views);
    let reference = reference_of(request);
    Payment {
        reference: PaymentRef::new(reference.as_str()),
        order_id: OrderId::new(reference),
        address: request.address.clone(),
        xmr_piconero: request.atomic,
        expires_at: UnixSeconds(request.expires_at as i64),
        phase,
        confirmations: Some(
            u32::try_from(status::confirmations(&views, request.atomic)).unwrap_or(u32::MAX),
        ),
        required: Some(settled),
        is_final,
        received: Some(received),
        overpaid: status == OrderStatus::Overpaid,
        price,
        amount: None,
        currency: None,
        receipt: request.note.clone(),
        device_name: None,
        renew: None,
    }
}

#[async_trait]
impl PaymentBackend for WalletBackend {
    async fn describe(&self) -> Result<BackendInfo, BackendError> {
        let inner = &self.inner;
        let through = match (&inner.link.light_server, &inner.link.node) {
            (Some(server), _) => format!("Your wallet, through {}", host_of(server)),
            (None, Some(node)) => format!("Your wallet, through {}", host_of(node)),
            (None, None) => "Your wallet".to_owned(),
        };
        Ok(BackendInfo {
            name: through,
            address: inner.wallet.primary_address(),
            currency: None,
            sharing: Sharing::ThisTill,
            monokulo: None,
            warnings: self.warnings().await,
        })
    }

    async fn create(&self, req: PaymentRequest) -> Result<Payment, BackendError> {
        let inner = &self.inner;
        if let Some(made) = inner.wallet.keyed_request_by_key(&req.key).await {
            let incoming = inner.wallet.request_payments(made.id).await.map_err(fob)?;
            return Ok(inner.payment(&made, &incoming));
        }
        let converted = inner
            .prices
            .convert(&req.amount)
            .await
            .map_err(BackendError::Other)?;
        let expires_at =
            inner.now().max(0) as u64 + u64::from(inner.link.expiry_minutes.max(1)) * 60;
        let made = inner
            .wallet
            .request_payment(Ask {
                key: req.key,
                atomic: converted.atomic,
                expires_at,
                note: req.reference,
            })
            .await
            .map_err(fob)?;
        if let Some(price) = converted.price {
            lock(&inner.priced).insert(made.id, price);
        }
        let payment = inner.payment(&made, &[]);
        lock(&inner.told).insert(made.id, payment.clone());
        inner.wake.notify_one();
        Ok(payment)
    }

    /// Catches up first (§5.5), then answers from what the wallet knows. A
    /// catch-up that fails still answers, from what was known, since
    /// expiry is the clock's: the stream says the node failed.
    async fn refresh(&self, refs: &[PaymentRef]) -> Result<Vec<Payment>, BackendError> {
        let inner = &self.inner;
        let caught_up = inner.wallet.catch_up().await;
        let mut payments = Vec::new();
        for reference in refs {
            if let Some(id) = request_id(reference.as_str())
                && let Some(payment) = inner.read(id).await
            {
                lock(&inner.told).insert(id, payment.clone());
                payments.push(payment);
            }
        }
        match caught_up {
            Err(e) if payments.is_empty() && !refs.is_empty() => Err(fob(e)),
            _ => Ok(payments),
        }
    }

    /// The loop of §5.5, run only while polled: `Ready` first (the till
    /// refreshes, which catches up), then a turn at the pace the state
    /// asks for, or sooner when woken, with every payment that changed. A
    /// turn that fails ends the stream on the error; the runner opens it
    /// again after a wait.
    fn events(&self) -> BoxStream<'static, Result<BackendEvent, BackendError>> {
        struct Looking {
            inner: Arc<Inner>,
            queue: VecDeque<Result<BackendEvent, BackendError>>,
            started: bool,
            ended: bool,
        }
        let state = Looking {
            inner: self.inner.clone(),
            queue: VecDeque::new(),
            started: false,
            ended: false,
        };
        stream::unfold(state, |mut state| async move {
            loop {
                if let Some(next) = state.queue.pop_front() {
                    if next.is_err() {
                        state.ended = true;
                    }
                    return Some((next, state));
                }
                if state.ended {
                    return None;
                }
                if !state.started {
                    state.started = true;
                    return Some((Ok(BackendEvent::Ready), state));
                }
                let pace = state.inner.pace().await;
                let woken = tokio::time::timeout(pace, state.inner.wake.notified())
                    .await
                    .is_ok();
                // Asleep with nothing open, only the newest block is asked.
                let pool = woken || pace != IDLE;
                match state.inner.turn(pool).await {
                    Ok(changed) => state
                        .queue
                        .extend(changed.into_iter().map(|p| Ok(BackendEvent::Changed(p)))),
                    Err(e) => state.queue.push_back(Err(e)),
                }
            }
        })
        .boxed()
    }

    fn sharing(&self) -> Sharing {
        Sharing::ThisTill
    }

    fn focus(&self, payment: Option<&PaymentRef>) {
        let mut focus = lock(&self.inner.focus);
        if focus.as_ref() != payment {
            *focus = payment.cloned();
            drop(focus);
            self.inner.wake.notify_one();
        }
    }

    /// Forgets the wallet: its file goes (§11). The view key can't be
    /// taken back from anyone who has it; this till just stops holding it.
    async fn disconnect(&self) -> Result<(), BackendError> {
        self.inner.wallet.remove_file().map_err(fob)
    }
}

/// "node.example.com" from "https://node.example.com:18089".
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?']).next().unwrap_or(rest).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prices::tests::{FixedSource, TestClock};
    use crate::setup::{Made, WalletSetup, make_wallet};
    use std::sync::atomic::{AtomicI64, Ordering};
    use till_types::{CurrencyCode, Money};

    fn request(atomic: u64, expires_at: u64) -> KeyedRequest {
        KeyedRequest {
            id: 7,
            key: "sq:o1".into(),
            account: 2,
            minor: 4,
            address: "8Address".into(),
            base: atomic - atomic % 1_000_000,
            tag: atomic % 1_000_000,
            atomic,
            created: 1_000,
            expires_at,
            height: 100,
            cancelled: None,
            note: Some("#42".into()),
        }
    }

    fn paid(atomic: u64, height: Option<u64>, confirmations: u64) -> Incoming {
        Incoming {
            tx: format!("tx{atomic}{confirmations}"),
            atomic,
            height,
            confirmations,
            double_spent: false,
            exact: true,
        }
    }

    #[test]
    fn a_request_reads_as_a_payment_through_each_phase() {
        let r = request(153_125_123_456, 2_800);
        let at = |incoming: &[Incoming], now| payment_of(&r, incoming, 10, now, None);

        let waiting = at(&[], 1_500);
        assert_eq!(waiting.phase, SalePhase::Waiting);
        assert_eq!(waiting.reference.as_str(), "w:2:4:123456:7");
        assert_eq!(waiting.order_id.as_str(), "w:2:4:123456:7");
        assert_eq!(request_id(waiting.reference.as_str()), Some(7));
        assert_eq!(
            (waiting.required, waiting.receipt.as_deref()),
            (Some(10), Some("#42"))
        );
        assert!(!waiting.is_final);

        assert_eq!(at(&[paid(r.atomic, None, 0)], 1_500).phase, SalePhase::Seen);
        let confirming = at(&[paid(r.atomic, Some(200), 3)], 1_500);
        assert_eq!(
            (confirming.phase, confirming.confirmations),
            (SalePhase::Confirming, Some(3))
        );
        let done = at(&[paid(r.atomic, Some(200), 10)], 1_500);
        assert_eq!(done.phase, SalePhase::Paid);
        assert!(done.is_final && !done.overpaid);
        let over = at(&[paid(r.atomic + 5, Some(200), 10)], 1_500);
        assert!(over.overpaid && over.is_final);

        let partial = at(&[paid(1_000, Some(200), 12)], 1_500);
        assert_eq!(
            (partial.phase, partial.received),
            (SalePhase::Partial, Some(1_000))
        );

        let expired = at(&[], 3_000);
        assert_eq!(expired.phase, SalePhase::Expired);
        assert!(
            !expired.is_final,
            "a late payment is still its own for a day"
        );
        let long_gone = at(&[], 2_800 + 24 * 3600 + 1);
        assert!(long_gone.is_final);

        let mut ds = paid(r.atomic, None, 0);
        ds.double_spent = true;
        assert_eq!(at(&[ds.clone()], 1_500).phase, SalePhase::DoubleSpend);
        // A double spend that didn't matter: the other payment covers it.
        assert_eq!(
            at(&[ds, paid(r.atomic, Some(200), 10)], 1_500).phase,
            SalePhase::Paid
        );

        let mut called_off = r.clone();
        called_off.cancelled = Some(1_200);
        let cancelled = payment_of(&called_off, &[], 10, 1_500, None);
        assert_eq!(cancelled.phase, SalePhase::Cancelled);
        assert!(cancelled.is_final);
    }

    #[test]
    fn a_host_is_named_without_its_scheme_or_port_path() {
        assert_eq!(
            host_of("https://node.example.com:18089"),
            "node.example.com:18089"
        );
        assert_eq!(host_of("node.example.com/x"), "node.example.com");
    }

    /// A wallet made as setup makes one, with prices of 160 of anything.
    async fn backend(dir: &Path) -> (WalletBackend, Arc<TestClock>, Arc<FixedSource>) {
        let keys = fob_keys::Keys::generate(Network::Stagenet).keys;
        let view = crate::keys::keys_from_typed(
            Network::Stagenet,
            &keys.primary_address(),
            &keys.view_key_hex(),
        )
        .unwrap();
        // These tests never ask the node (the network's default): nothing
        // here needs the chain.
        let setup = WalletSetup {
            restore_height: Some(2_000_000),
            ..WalletSetup::new(3)
        };
        let Made { link, .. } = make_wallet(dir, &view, &setup).await.unwrap();
        let clock = Arc::new(TestClock(AtomicI64::new(1_000)));
        let source = Arc::new(FixedSource::new());
        let backend = WalletBackend::open_with(dir, &link, clock.clone(), source.clone())
            .await
            .unwrap();
        (backend, clock, source)
    }

    #[tokio::test]
    async fn a_payment_is_made_once_per_key_in_this_tills_account_and_says_its_price() {
        let dir = tempfile::tempdir().unwrap();
        let (backend, _clock, source) = backend(dir.path()).await;
        let info = backend.describe().await.unwrap();
        assert_eq!(info.sharing, Sharing::ThisTill);
        assert!(info.address.starts_with('5'), "{}", info.address);

        let ask = PaymentRequest {
            key: "sq:o1".into(),
            amount: Money::new(2450, CurrencyCode::USD),
            reference: Some("#42".into()),
        };
        let made = backend.create(ask.clone()).await.unwrap();
        assert!(
            made.reference.as_str().starts_with("w:3:"),
            "{:?}",
            made.reference
        );
        assert_eq!(made.phase, SalePhase::Waiting);
        assert_eq!(
            made.xmr_piconero / 1_000_000,
            153_125,
            "the price, then the tag"
        );
        assert_eq!(made.expires_at, UnixSeconds(1_000 + 30 * 60));
        assert_eq!(made.price.as_ref().unwrap().provider, "Kraken");
        assert!(
            made.address.starts_with('7'),
            "a subaddress: {}",
            made.address
        );

        // Asked again, as New code or a retried charge would: the same one,
        // and no second price.
        let again = backend.create(ask).await.unwrap();
        assert_eq!(again.reference, made.reference);
        assert_eq!(again.xmr_piconero, made.xmr_piconero);
        assert_eq!(source.asked.load(Ordering::SeqCst), 1);

        // Read again from what the wallet knows, as refresh answers after
        // its catch-up.
        let id = request_id(made.reference.as_str()).unwrap();
        let read = backend.inner.read(id).await.unwrap();
        assert_eq!(read.phase, SalePhase::Waiting);
        assert_eq!(
            read.price, made.price,
            "it carries the price it was made at"
        );
    }

    #[tokio::test]
    async fn the_stream_says_ready_then_a_code_that_ran_out() {
        let dir = tempfile::tempdir().unwrap();
        let (backend, clock, _) = backend(dir.path()).await;
        let made = backend
            .create(PaymentRequest {
                key: "sq:o2".into(),
                amount: Money::new(100, CurrencyCode::USD),
                reference: None,
            })
            .await
            .unwrap();
        clock.0.store(1_000 + 31 * 60, Ordering::SeqCst);
        // What a turn finds, without a node to ask.
        let changed = backend.inner.changes().await;
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].reference, made.reference);
        assert_eq!(changed[0].phase, SalePhase::Expired);
        assert!(backend.inner.changes().await.is_empty(), "said once");

        let mut events = backend.events();
        assert!(matches!(events.next().await, Some(Ok(BackendEvent::Ready))));
    }
}
