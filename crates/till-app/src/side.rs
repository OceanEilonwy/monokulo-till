//! What the leaf crates ask of the app (Android: `Graph`, the stores behind
//! it and `AppViewModel`'s `switchSide`): till-sales' [`RunnerSide`] and
//! [`PosSwitchSide`], till-bell's [`PosSide`], and the points of sale made
//! from the persisted model.

use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use async_trait::async_trait;
use monokulo_client::{AppIdentity, Clock, Connection, MonokuloApi, PointOfSale};
use till_bell::{BellWakeUps, DeviceError, PosSide, PosSignIn};
use till_ledger::{ClaimPolicy, Process};
use till_payments::{Link, MonokuloBackend, PaymentBackend, WalletLink};
use till_pos::clover::{
    CloverApi, CloverAuth, CloverConfig, CloverEnvironment, CloverMerchant, CloverSource,
};
use till_pos::square::{SquareApi, SquareAuth, SquareConfig, SquareSource};
use till_pos::{PosEnvironment, SaleSource, TokenLock, TokenStore};
use till_sales::{Books, LiveStore, PosSwitchSide, RunnerSide};
use till_types::Secret;
use till_wallet::WalletBackend;

use crate::ledger::AppLedger;
use crate::store::{Kept, Model, PosCursor, PosTokenStore, Store, Tenders};
use crate::{EngineConfig, Platform};

/// The app's long-lived parts that the leaf crates call back into.
pub(crate) struct Side {
    pub config: Arc<EngineConfig>,
    pub kept: Arc<Kept>,
    pub http: reqwest::Client,
    pub clock: Arc<dyn Clock>,
    lock: Arc<dyn TokenLock>,
    ledger: Arc<AppLedger>,
    books: Arc<Books>,
    monokulo: MonokuloApi,
    live: OnceLock<Weak<LiveStore>>,
    wake_ups: OnceLock<Weak<BellWakeUps>>,
    /// The device's own wallet, opened once for the link it was opened
    /// with: one wallet file is never opened twice in a process.
    wallet: Mutex<Option<(WalletLink, Arc<WalletBackend>)>>,
}

impl Side {
    /// The parts over `kept`, with the ledger as `process` opens it.
    pub fn new(
        config: Arc<EngineConfig>,
        kept: Arc<Kept>,
        platform: Arc<dyn Platform>,
        process: Process,
        http: reqwest::Client,
        clock: Arc<dyn Clock>,
    ) -> Side {
        let ledger = Arc::new(AppLedger::new(
            &config.data_dir,
            platform,
            kept.clone(),
            process,
        ));
        // The app claims what the extension left after each look; the
        // extension claims the one alert it shows itself (docs/ledger.md §6).
        let claims = (process == Process::App).then_some(ClaimPolicy::All);
        let books = Arc::new(Books::new(ledger.clone(), claims));
        books.set_clock(clock.clone());
        let monokulo = MonokuloApi::new(http.clone(), identity_of(&config), clock.clone());
        Side {
            lock: Arc::new(FileLock::new(
                PathBuf::from(&config.data_dir).join("sign-in.lock"),
            )),
            ledger,
            books,
            monokulo,
            config,
            kept,
            http,
            clock,
            live: OnceLock::new(),
            wake_ups: OnceLock::new(),
            wallet: Mutex::new(None),
        }
    }

    /// The build's Clover.
    fn clover_environment(&self) -> CloverEnvironment {
        self.config.clover.environment.into()
    }

    /// Hands the side the parts made after it.
    pub fn attach(&self, live: &Arc<LiveStore>, wake_ups: &Arc<BellWakeUps>) {
        let _ = self.live.set(Arc::downgrade(live));
        let _ = self.wake_ups.set(Arc::downgrade(wake_ups));
    }

    pub fn wake_ups(&self) -> Option<Arc<BellWakeUps>> {
        self.wake_ups.get()?.upgrade()
    }

    pub fn store(&self) -> &Store {
        self.kept.store()
    }

    pub fn model(&self) -> Model {
        self.kept.model()
    }

    pub fn log(&self, line: String) {
        self.kept.log(line);
    }

    fn logger(&self) -> crate::store::Log {
        let kept = self.kept.clone();
        Arc::new(move |line| kept.log(line))
    }

    /// Changes the model; a write the store refused is logged, for callers
    /// (the leaf crates' traits) that can't say so.
    pub fn update(&self, change: impl FnOnce(&mut Model)) {
        if let Err(e) = self.kept.update(change) {
            self.log(format!("till-core: couldn't keep a change: {e}"));
        }
    }

    /// What the app says about itself to Monokulo.
    pub fn identity(&self) -> AppIdentity {
        identity_of(&self.config)
    }

    /// The client every Monokulo request goes through: one HTTP client and
    /// one set of kept entries for the process.
    pub fn monokulo(&self) -> &MonokuloApi {
        &self.monokulo
    }

    /// The payment backend a link names (core.md decision 228): the
    /// store's Monokulo, through the process's client, or the device's own
    /// wallet (decision 229).
    pub fn backend_of(&self, link: &Link) -> Option<Arc<dyn PaymentBackend>> {
        self.backend_reading(link, 1)
    }

    /// [`Side::backend_of`], refreshing up to `at_once` payments at once:
    /// for a look someone waits on (iOS's extension, core.md decision 204).
    pub fn backend_reading(&self, link: &Link, at_once: usize) -> Option<Arc<dyn PaymentBackend>> {
        match link {
            Link::Monokulo(connection) => Some(Arc::new(
                MonokuloBackend::new(self.monokulo.clone(), connection.clone())
                    .reading_at_once(at_once),
            )),
            Link::Wallet(link) => self
                .wallet_backend(link)
                .map(|backend| backend as Arc<dyn PaymentBackend>),
        }
    }

    /// The device's own wallet for `link`: the one already open, or the
    /// wallet file opened now. `None`, logged, when it can't be opened.
    pub fn wallet_backend(&self, link: &WalletLink) -> Option<Arc<WalletBackend>> {
        let mut held = self.wallet.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((open, backend)) = held.as_ref()
            && open == link
        {
            return Some(backend.clone());
        }
        // The file is closed before it's opened again for another link.
        *held = None;
        match WalletBackend::open_now(
            std::path::Path::new(&self.config.data_dir),
            link,
            self.clock.clone(),
        ) {
            Ok(backend) => {
                let backend = Arc::new(backend);
                *held = Some((link.clone(), backend.clone()));
                Some(backend)
            }
            Err(e) => {
                self.log(format!("till-core: couldn't open the wallet: {}", e.say()));
                None
            }
        }
    }

    /// Holds the wallet setup just made, for its link.
    pub fn adopt_wallet(&self, backend: WalletBackend) {
        let link = backend.link().clone();
        *self.wallet.lock().unwrap_or_else(|e| e.into_inner()) = Some((link, Arc::new(backend)));
    }

    /// Lets the wallet go: the till was disconnected from it.
    pub fn forget_wallet(&self) {
        *self.wallet.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// The till watches the shop's own wallet: no Bell, no background work
    /// (docs/view-key-mode.md §10).
    pub fn wallet_mode(&self) -> bool {
        self.kept
            .with(|m| m.connection.is_none() && m.wallet.is_some())
    }

    /// The payment backend for `connection`, the store's Monokulo.
    pub fn monokulo_backend(&self, connection: &Connection) -> Arc<dyn PaymentBackend> {
        Arc::new(MonokuloBackend::new(
            self.monokulo.clone(),
            connection.clone(),
        ))
    }

    /// The ledger as this process opens it.
    pub fn ledger(&self) -> &Arc<AppLedger> {
        &self.ledger
    }

    /// The till's books over the ledger.
    pub fn books(&self) -> &Arc<Books> {
        &self.books
    }

    pub fn tokens(&self, pos: PointOfSale) -> Arc<dyn TokenStore> {
        Arc::new(PosTokenStore::new(self.store().clone(), pos, self.logger()))
    }

    /// Signed in to `pos`. A sign-in that can't be read counts as one
    /// (decision 17): the till goes on asking, and the read's error is what
    /// it shows, never "sign in again".
    pub fn signed_in(&self, pos: PointOfSale) -> bool {
        !matches!(self.tokens(pos).load(), Ok(None))
    }

    /// A stand-in kept in a debug build, else the build's own.
    fn base_url(&self, kept: Option<String>, built: &Option<String>) -> Option<String> {
        match kept {
            Some(kept) if self.config.debug => Some(kept),
            _ => built.clone(),
        }
    }

    /// Square as the till signs in through it: a Bell's application once
    /// the till has signed in through one (decision 95), else the build's;
    /// `through` names another for one sign-in.
    pub fn square_config(&self, through: Option<&str>) -> SquareConfig {
        let model = self.model();
        let setup = &self.config.square;
        let application_id = through
            .map(str::to_owned)
            .or(model.square_application_id)
            .unwrap_or_else(|| setup.application_id.clone());
        let mut config = SquareConfig::new(
            application_id,
            setup.environment == crate::SquareEnvironment::Sandbox,
            &setup.redirect_uri,
        );
        config.base_url = self.base_url(model.square_base_url, &setup.base_url);
        config
    }

    pub fn clover_config(&self, through: Option<&str>) -> CloverConfig {
        let model = self.model();
        let setup = &self.config.clover;
        let application_id = through
            .map(str::to_owned)
            .or(model.clover_application_id)
            .unwrap_or_else(|| setup.application_id.clone());
        let mut config = CloverConfig::new(
            application_id,
            self.clover_environment(),
            &setup.redirect_uri,
        );
        config.user_agent = format!("Till/{}", self.config.app_version);
        config.base_url = self.base_url(model.clover_base_url, &setup.base_url);
        config
    }

    pub fn square_auth(&self, through: Option<&str>) -> Arc<SquareAuth> {
        Arc::new(
            SquareAuth::new(
                self.http.clone(),
                self.square_config(through),
                self.tokens(PointOfSale::Square),
                self.clock.clone(),
            )
            .with_lock(self.lock.clone()),
        )
    }

    pub fn square_api(&self) -> SquareApi {
        SquareApi::new(
            self.http.clone(),
            self.square_config(None),
            self.square_auth(None),
        )
    }

    pub fn clover_auth(&self, through: Option<&str>) -> Arc<CloverAuth> {
        Arc::new(
            CloverAuth::new(
                self.http.clone(),
                self.clover_config(through),
                self.tokens(PointOfSale::Clover),
                self.clock.clone(),
            )
            .with_lock(self.lock.clone()),
        )
    }

    pub fn clover_api(&self) -> CloverApi {
        CloverApi::new(
            self.http.clone(),
            self.clover_config(None),
            self.clover_auth(None),
        )
    }

    /// Clover's sales at `merchant`, for the till and for finding the
    /// Monero payment type.
    pub fn clover_source(&self, merchant: CloverMerchant) -> CloverSource {
        let log = self.logger();
        CloverSource::new(
            self.clover_api(),
            self.clover_config(None),
            merchant,
            Arc::new(PosCursor::new(
                self.store().clone(),
                PointOfSale::Clover,
                log.clone(),
            )),
            Arc::new(Tenders::new(self.store().clone(), log)),
            self.clock.clone(),
        )
    }

    /// The source for the point of sale chosen, while signed in to it at a
    /// location or merchant.
    pub fn sale_source(&self) -> Option<Arc<dyn SaleSource>> {
        let model = self.model();
        match model.point_of_sale? {
            PointOfSale::Square => {
                let location = model.location?;
                if !self.signed_in(PointOfSale::Square) {
                    return None;
                }
                Some(Arc::new(SquareSource::new(
                    self.square_api(),
                    self.square_config(None),
                    location,
                    Arc::new(PosCursor::new(
                        self.store().clone(),
                        PointOfSale::Square,
                        self.logger(),
                    )),
                    self.clock.clone(),
                )))
            }
            PointOfSale::Clover => {
                let merchant = model.clover_merchant?;
                if !self.signed_in(PointOfSale::Clover) {
                    return None;
                }
                Some(Arc::new(self.clover_source(merchant)))
            }
        }
    }

    /// Forgets the sign-in to `pos` on this device: its tokens, the
    /// application they came through, its location or merchant and the
    /// remembered tender id (acceptance T1.4).
    pub fn sign_out_here(&self, pos: PointOfSale) {
        self.tokens(pos).save(None);
        match pos {
            PointOfSale::Square => self.update(|m| {
                m.square_application_id = None;
                m.location = None;
            }),
            PointOfSale::Clover => {
                if let Some(merchant) = self.model().clover_merchant
                    && let Err(e) = self.store().set_tender_id(merchant.id.as_str(), None)
                {
                    self.log(format!("till-core: couldn't forget the tender id: {e}"));
                }
                self.update(|m| {
                    m.clover_application_id = None;
                    m.clover_merchant = None;
                });
            }
        }
    }
}

/// What the app says about itself to Monokulo, as `config` builds it.
fn identity_of(config: &EngineConfig) -> AppIdentity {
    AppIdentity {
        slug: "till".into(),
        name: "Monokulo Till".into(),
        version: config.app_version.clone(),
        platform: config.platform().to_owned(),
        device_name: config.device_name.chars().take(40).collect(),
    }
}

impl RunnerSide for Side {
    fn backend(&self) -> Option<Arc<dyn PaymentBackend>> {
        self.kept
            .with(|m| m.link())
            .and_then(|link| self.backend_of(&link))
    }

    fn read_store(&self, backend: &Arc<dyn PaymentBackend>) {
        if let Some(live) = self.live.get().and_then(Weak::upgrade) {
            live.read(Some(backend));
        }
    }

    fn start_shift(&self) {
        if self.kept.with(|m| m.shift_started_at.is_none()) {
            let now = self.clock.now();
            self.update(|m| m.shift_started_at = Some(now));
        }
    }

    fn source(&self) -> Option<Arc<dyn SaleSource>> {
        self.sale_source()
    }

    fn books(&self) -> Arc<Books> {
        self.books.clone()
    }

    fn interval(&self) -> Duration {
        self.wake_ups()
            .map(|w| w.status().borrow().interval())
            .unwrap_or(Duration::from_secs(2))
    }

    fn timer_found(&self, found: usize) {
        if let Some(w) = self.wake_ups() {
            w.timer_found(u32::try_from(found).unwrap_or(u32::MAX));
        }
    }

    fn till_options(&self) -> till_sales::TillOptions {
        self.kept.with(|m| m.till_options())
    }

    fn recheck_wake_ups(&self) {
        if self.wallet_mode() {
            return;
        }
        if let Some(w) = self.wake_ups() {
            w.recheck();
        }
    }
}

#[async_trait]
impl PosSwitchSide for Side {
    async fn take_bell_registration_back(&self) {
        if let Some(w) = self.wake_ups() {
            w.unregister(false).await;
        }
    }

    fn sign_out(&self, pos: PointOfSale) {
        self.sign_out_here(pos);
    }

    fn forget_cursor(&self, pos: PointOfSale) {
        if let Err(e) = self.store().forget_cursor(pos) {
            self.log(format!("till-core: couldn't forget the cursor: {e}"));
        }
    }

    fn choose(&self, pos: PointOfSale) {
        self.update(|m| m.point_of_sale = Some(pos));
    }
}

#[async_trait]
impl PosSide for Side {
    /// Where the till is signed in, for a Bell registration (decision
    /// 132): a store that can't answer is a device failure, retried.
    fn signed_in(&self) -> Result<Option<PosSignIn>, DeviceError> {
        let model = self.model();
        // The shop's own wallet registers no Bell: Till looks only while
        // it's open (docs/view-key-mode.md §10).
        if model.without_pos || (model.connection.is_none() && model.wallet.is_some()) {
            return Ok(None);
        }
        let device = |e: crate::SecretError| DeviceError(e.to_string());
        Ok(match model.point_of_sale {
            Some(PointOfSale::Square) => {
                let signed_in = self
                    .store()
                    .tokens(PointOfSale::Square)
                    .map_err(device)?
                    .is_some();
                match model.location {
                    Some(location) if signed_in => {
                        // The environment is the application's (decision 305).
                        let config = self.square_config(None);
                        Some(PosSignIn {
                            environment: PosEnvironment::Square(config.environment()),
                            application_id: config.application_id,
                            location_id: location.id.0,
                        })
                    }
                    _ => None,
                }
            }
            // A Clover merchant is its own location (acceptance D1).
            Some(PointOfSale::Clover) => {
                let signed_in = self
                    .store()
                    .tokens(PointOfSale::Clover)
                    .map_err(device)?
                    .is_some();
                match model.clover_merchant {
                    Some(merchant) if signed_in => Some(PosSignIn {
                        application_id: self.clover_config(None).application_id,
                        environment: PosEnvironment::Clover(self.clover_environment()),
                        location_id: merchant.id.into(),
                    }),
                    _ => None,
                }
            }
            None => None,
        })
    }

    async fn access_token(&self) -> Result<Option<Secret>, DeviceError> {
        let token = match self.model().point_of_sale {
            Some(PointOfSale::Square) => self.square_auth(None).access_token().await,
            Some(PointOfSale::Clover) => self.clover_auth(None).access_token().await,
            None => return Ok(None),
        };
        match token {
            Ok(token) => Ok(Some(token)),
            Err(e) if e.sign_in_again() => Ok(None),
            Err(e) => Err(DeviceError(e.to_string())),
        }
    }
}

/// The lock around a point of sale's refresh (decision 13): an exclusive
/// `flock` on a file in the data directory, which the iOS app and its
/// notification extension share. On Android only one process holds the
/// sign-in, so it never waits there, and costs a file. The alerts' record
/// takes one of its own (`alerted`, core.md decision 123).
pub(crate) struct FileLock {
    path: PathBuf,
    held: Mutex<Option<File>>,
}

impl FileLock {
    pub(crate) fn new(path: PathBuf) -> FileLock {
        FileLock {
            path,
            held: Mutex::new(None),
        }
    }
}

impl TokenLock for FileLock {
    fn acquire(&self) -> std::io::Result<()> {
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.path)?;
        // Blocks until no other file description holds it: another process,
        // or another sign-in in this one.
        file.lock()?;
        *self.held.lock().unwrap_or_else(|e| e.into_inner()) = Some(file);
        Ok(())
    }

    fn release(&self) {
        let held = self.held.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(file) = held {
            let _ = file.unlock();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn the_sign_in_lock_holds_off_a_second_holder_until_released() {
        let path = std::env::temp_dir().join(format!("till-lock-{}", std::process::id()));
        let lock = Arc::new(FileLock::new(path.clone()));
        let other = Arc::new(FileLock::new(path));
        lock.acquire().unwrap();
        let got = Arc::new(AtomicBool::new(false));
        let waiter = {
            let (other, got) = (other.clone(), got.clone());
            std::thread::spawn(move || {
                other.acquire().unwrap();
                got.store(true, Ordering::SeqCst);
                other.release();
            })
        };
        std::thread::sleep(Duration::from_millis(100));
        assert!(!got.load(Ordering::SeqCst), "held off");
        lock.release();
        waiter.join().unwrap();
        assert!(got.load(Ordering::SeqCst));
    }
}
