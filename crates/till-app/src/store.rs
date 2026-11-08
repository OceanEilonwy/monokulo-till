//! Everything the app keeps (Android: `Stores.kt`), by the same names and
//! in the same JSON, kept through [`Platform`]'s secrets: the Android
//! Keystore, or the iOS Keychain in a group the extension shares
//! (docs/ios-plan.md §2, "Storage").
//!
//! A value is the UTF-8 bytes of the string `Stores.kt` kept under the same
//! key, so an Android till upgraded onto the engine keeps its connection,
//! its sign-ins and its shift. A value that can't be read is "none", as the
//! Kotlin's `runCatching` made it; a secret store that can't answer is a
//! [`SecretError`], never "none" (decision 17 in docs/decisions/core.md):
//! taking it for "none" would replace a saved connection with nothing.
//!
//! - [`Store`]: each key, read and written as `Stores.kt` did.
//! - [`Model`] and [`Kept`]: the small values, held in memory and written
//!   through as they change.
//! - [`PosTokenStore`], [`PosCursor`], [`Tenders`], [`ShiftKept`] and
//!   [`WakeUpsKept`]: the leaf crates' store traits over it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use monokulo_client::{Connection, PointOfSale};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use till_bell::{BellAddress, BellRegistration, WakeUpChoice, WakeUpStore};
use till_pos::clover::{CloverMerchant, TenderStore};
use till_pos::square::SquareLocation;
use till_pos::{CursorStore, Pkce, PosTokens, TokenStore};
use till_sales::{ShiftStore, TillSale, shift_json};

use crate::{Platform, SecretError};

/// Where a line for the system log goes.
pub type Log = Arc<dyn Fn(String) + Send + Sync>;

/// The keys `Stores.kt` writes, each with the shape of its value.
pub mod keys {
    use monokulo_client::PointOfSale;

    /// `Connection.save`'s JSON: `{"app_key":…,"api":…}`, with `onion_api`
    /// and `tor` only when set.
    pub const CONNECTION: &str = "connection";
    /// `SquareLocation`'s JSON.
    pub const SQUARE_LOCATION: &str = "square.location";
    /// `square` or `clover`; absent on a till from before there was a
    /// choice, which is Square's when signed in to Square.
    pub const POS_KIND: &str = "pos.kind";
    /// `true`, or absent.
    pub const POS_SKIPPED: &str = "pos.skipped";
    /// The key a till from before there was a choice kept `pos.skipped`
    /// under: read, and removed whenever it is written.
    pub const SQUARE_SKIPPED: &str = "square.skipped";
    /// `true` or `false`; absent is on.
    pub const SHIFT_ON: &str = "shift.on";
    /// Unix seconds, in decimal.
    pub const SHIFT_STARTED: &str = "shift.started";
    /// `true`, or absent.
    pub const SETUP_DONE: &str = "setup.done";
    /// `true` or `false`; absent is on.
    pub const BUBBLE_ON: &str = "bubble.on";
    /// `true` or `false`.
    pub const BUBBLE_LEFT: &str = "bubble.left";
    /// Pixels, in decimal.
    pub const BUBBLE_Y: &str = "bubble.y";
    /// The pending sign-in, whichever point of sale's: `{"verifier":…,
    /// "state":…,"applicationId":…,"pointOfSale":"clover"}`. Named for
    /// Square, which had it first.
    pub const PENDING_SIGN_IN: &str = "square.pending";
    /// The application id, when a Bell's rather than the build's.
    pub const SQUARE_APPLICATION: &str = "square.application";
    pub const CLOVER_APPLICATION: &str = "clover.application";
    /// `{"type":"monokulo"}` or `{"type":"own","bell":{"url":…}}`; absent
    /// is the timer alone.
    pub const WAKE_UPS_CHOICE: &str = "wakeups.choice";
    /// `{"url":…}`.
    pub const WAKE_UPS_OWN: &str = "wakeups.own";
    /// `BellRegistration`'s JSON.
    pub const WAKE_UPS_REGISTRATION: &str = "wakeups.registration";
    /// Debug builds' stand-ins: an address, never blank.
    pub const DEBUG_SQUARE_BASE: &str = "debug.square.base";
    pub const DEBUG_CLOVER_BASE: &str = "debug.clover.base";
    /// `CloverMerchant`'s JSON.
    pub const CLOVER_MERCHANT: &str = "clover.merchant";
    /// The shift's sales, `till_sales::shift_json`; absent when none.
    pub const SHIFT_SALES: &str = "shift.sales";

    /// `PosTokens`' JSON: `square.tokens` (a name from before Clover) or
    /// `clover.tokens`.
    pub fn tokens(pos: PointOfSale) -> &'static str {
        match pos {
            PointOfSale::Square => "square.tokens",
            PointOfSale::Clover => "clover.tokens",
        }
    }

    /// Where the search for sales has got to: an RFC 3339 time, as
    /// `Instant.toString()` wrote it.
    pub fn since(pos: PointOfSale) -> String {
        format!("{}.since", pos.wire())
    }

    /// The sales handled since then: a JSON array of payment ids.
    pub fn handled(pos: PointOfSale) -> String {
        format!("{}.handled", pos.wire())
    }

    /// The Monero tender's id at a Clover merchant.
    pub fn tender(merchant_id: &str) -> String {
        format!("clover.tender.{merchant_id}")
    }
}

/// A point-of-sale sign-in under way (Android: `PendingSignIn`): which
/// point of sale, its PKCE, and the application it signs in through when
/// that is a Bell's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingSignIn {
    pub point_of_sale: PointOfSale,
    pub pkce: Pkce,
    pub application_id: Option<String>,
}

/// `Stores.kt`'s `PkceState`, as kotlinx wrote it.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PkceState {
    verifier: String,
    state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    application_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    point_of_sale: Option<String>,
}

/// Every key, read and written as `Stores.kt` did.
#[derive(Clone)]
pub struct Store {
    platform: Arc<dyn Platform>,
}

impl Store {
    pub fn new(platform: Arc<dyn Platform>) -> Store {
        Store { platform }
    }

    /// The string kept under `key`. Bytes that aren't UTF-8 are none, as a
    /// value the Keystore couldn't decrypt was.
    pub fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self
            .platform
            .secret_get(key.to_owned())?
            .and_then(|bytes| String::from_utf8(bytes).ok()))
    }

    /// Keeps `value` under `key`; `None` removes it.
    pub fn put(&self, key: &str, value: Option<&str>) -> Result<(), SecretError> {
        match value {
            Some(value) => self
                .platform
                .secret_put(key.to_owned(), value.as_bytes().to_vec()),
            None => self.platform.secret_remove(key.to_owned()),
        }
    }

    /// The JSON under `key`; none when it doesn't read.
    fn read<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, SecretError> {
        Ok(self
            .get(key)?
            .and_then(|json| serde_json::from_str(&json).ok()))
    }

    fn write<T: Serialize>(&self, key: &str, value: Option<&T>) -> Result<(), SecretError> {
        let json = value.map(|value| serde_json::to_string(value).expect("always JSON"));
        self.put(key, json.as_deref())
    }

    fn flag(&self, key: &str) -> Result<bool, SecretError> {
        Ok(self.get(key)?.as_deref() == Some("true"))
    }

    /// `true`, or removed.
    fn put_flag(&self, key: &str, on: bool) -> Result<(), SecretError> {
        self.put(key, on.then_some("true"))
    }

    /// On unless `false`.
    fn switch(&self, key: &str) -> Result<bool, SecretError> {
        Ok(self.get(key)?.as_deref() != Some("false"))
    }

    fn put_switch(&self, key: &str, on: bool) -> Result<(), SecretError> {
        self.put(key, Some(if on { "true" } else { "false" }))
    }

    pub fn connection(&self) -> Result<Option<Connection>, SecretError> {
        Ok(self
            .get(keys::CONNECTION)?
            .and_then(|saved| Connection::restore(&saved)))
    }

    pub fn set_connection(&self, connection: Option<&Connection>) -> Result<(), SecretError> {
        self.put(
            keys::CONNECTION,
            connection.map(Connection::save).as_deref(),
        )
    }

    /// The point of sale chosen; a till from before there was a choice,
    /// signed in to Square, is a Square till.
    pub fn point_of_sale(&self) -> Result<Option<PointOfSale>, SecretError> {
        if let Some(wire) = self.get(keys::POS_KIND)? {
            return Ok(PointOfSale::from_wire(&wire));
        }
        Ok(self
            .tokens(PointOfSale::Square)?
            .map(|_| PointOfSale::Square))
    }

    pub fn set_point_of_sale(&self, pos: Option<PointOfSale>) -> Result<(), SecretError> {
        self.put(keys::POS_KIND, pos.map(PointOfSale::wire))
    }

    pub fn without_pos(&self) -> Result<bool, SecretError> {
        Ok(self.flag(keys::POS_SKIPPED)? || self.flag(keys::SQUARE_SKIPPED)?)
    }

    pub fn set_without_pos(&self, on: bool) -> Result<(), SecretError> {
        self.put_flag(keys::POS_SKIPPED, on)?;
        self.put(keys::SQUARE_SKIPPED, None)
    }

    pub fn shift_started_at(&self) -> Result<Option<i64>, SecretError> {
        Ok(self.get(keys::SHIFT_STARTED)?.and_then(|s| s.parse().ok()))
    }

    pub fn set_shift_started_at(&self, at: Option<i64>) -> Result<(), SecretError> {
        self.put(keys::SHIFT_STARTED, at.map(|at| at.to_string()).as_deref())
    }

    pub fn bubble_y(&self) -> Result<Option<i32>, SecretError> {
        Ok(self.get(keys::BUBBLE_Y)?.and_then(|s| s.parse().ok()))
    }

    pub fn set_bubble_y(&self, y: Option<i32>) -> Result<(), SecretError> {
        self.put(keys::BUBBLE_Y, y.map(|y| y.to_string()).as_deref())
    }

    /// One kept before there was a choice is Square's; one naming a point
    /// of sale this build doesn't know is none.
    pub fn pending_sign_in(&self) -> Result<Option<PendingSignIn>, SecretError> {
        let Some(state) = self.read::<PkceState>(keys::PENDING_SIGN_IN)? else {
            return Ok(None);
        };
        let pos = match &state.point_of_sale {
            None => Some(PointOfSale::Square),
            Some(wire) => PointOfSale::from_wire(wire),
        };
        Ok(pos.map(|point_of_sale| PendingSignIn {
            point_of_sale,
            pkce: Pkce {
                verifier: state.verifier,
                state: state.state,
            },
            application_id: state.application_id,
        }))
    }

    pub fn set_pending_sign_in(&self, pending: Option<&PendingSignIn>) -> Result<(), SecretError> {
        let state = pending.map(|p| PkceState {
            verifier: p.pkce.verifier.clone(),
            state: p.pkce.state.clone(),
            application_id: p.application_id.clone(),
            point_of_sale: Some(p.point_of_sale.wire().to_owned()),
        });
        self.write(keys::PENDING_SIGN_IN, state.as_ref())
    }

    pub fn wake_up_choice(&self) -> Result<WakeUpChoice, SecretError> {
        Ok(self.read(keys::WAKE_UPS_CHOICE)?.unwrap_or_default())
    }

    /// The timer alone is kept as nothing.
    pub fn set_wake_up_choice(&self, choice: &WakeUpChoice) -> Result<(), SecretError> {
        self.write(
            keys::WAKE_UPS_CHOICE,
            Some(choice).filter(|c| **c != WakeUpChoice::TimerOnly),
        )
    }

    /// A blank address is kept as nothing.
    fn put_address(&self, key: &str, url: Option<&str>) -> Result<(), SecretError> {
        self.put(key, url.filter(|url| !url.trim().is_empty()))
    }

    pub fn tokens(&self, pos: PointOfSale) -> Result<Option<PosTokens>, SecretError> {
        self.read(keys::tokens(pos))
    }

    pub fn set_tokens(
        &self,
        pos: PointOfSale,
        tokens: Option<&PosTokens>,
    ) -> Result<(), SecretError> {
        self.write(keys::tokens(pos), tokens)
    }

    pub fn since(&self, pos: PointOfSale) -> Result<Option<String>, SecretError> {
        self.get(&keys::since(pos))
    }

    pub fn handled(&self, pos: PointOfSale) -> Result<Vec<String>, SecretError> {
        Ok(self.read(&keys::handled(pos))?.unwrap_or_default())
    }

    pub fn save_cursor(
        &self,
        pos: PointOfSale,
        since: &str,
        handled: &[String],
    ) -> Result<(), SecretError> {
        self.put(&keys::since(pos), Some(since))?;
        self.write(&keys::handled(pos), Some(&handled))
    }

    /// A new sign-in to `pos` starts from now.
    pub fn forget_cursor(&self, pos: PointOfSale) -> Result<(), SecretError> {
        self.put(&keys::since(pos), None)?;
        self.put(&keys::handled(pos), None)
    }

    pub fn tender_id(&self, merchant_id: &str) -> Result<Option<String>, SecretError> {
        self.get(&keys::tender(merchant_id))
    }

    pub fn set_tender_id(
        &self,
        merchant_id: &str,
        tender_id: Option<&str>,
    ) -> Result<(), SecretError> {
        self.put(&keys::tender(merchant_id), tender_id)
    }

    /// The shift's sales; none when nothing readable is kept.
    pub fn shift_sales(&self) -> Result<Vec<TillSale>, SecretError> {
        Ok(self
            .get(keys::SHIFT_SALES)?
            .and_then(|saved| shift_json::decode(&saved).ok())
            .unwrap_or_default())
    }

    /// An empty shift is kept as nothing.
    pub fn set_shift_sales(&self, sales: &[TillSale]) -> Result<(), SecretError> {
        let saved = (!sales.is_empty()).then(|| shift_json::encode(sales));
        self.put(keys::SHIFT_SALES, saved.as_deref())
    }

    /// Everything [`Model`] holds, read now.
    pub fn load(&self) -> Result<Model, SecretError> {
        Ok(Model {
            connection: self.connection()?,
            location: self.read(keys::SQUARE_LOCATION)?,
            point_of_sale: self.point_of_sale()?,
            without_pos: self.without_pos()?,
            shift_on: self.switch(keys::SHIFT_ON)?,
            shift_started_at: self.shift_started_at()?,
            setup_done: self.flag(keys::SETUP_DONE)?,
            bubble_on: self.switch(keys::BUBBLE_ON)?,
            bubble_left: self.flag(keys::BUBBLE_LEFT)?,
            bubble_y: self.bubble_y()?,
            pending_sign_in: self.pending_sign_in()?,
            square_application_id: self.get(keys::SQUARE_APPLICATION)?,
            clover_application_id: self.get(keys::CLOVER_APPLICATION)?,
            wake_up_choice: self.wake_up_choice()?,
            own_bell: self.read(keys::WAKE_UPS_OWN)?,
            registration: self.read(keys::WAKE_UPS_REGISTRATION)?,
            square_base_url: self.get(keys::DEBUG_SQUARE_BASE)?,
            clover_base_url: self.get(keys::DEBUG_CLOVER_BASE)?,
            clover_merchant: self.read(keys::CLOVER_MERCHANT)?,
        })
    }

    /// Writes the keys whose values differ between `old` and `new`, as
    /// `Stores.kt`'s setters wrote them.
    pub fn save(&self, old: &Model, new: &Model) -> Result<(), SecretError> {
        if old.connection != new.connection {
            self.set_connection(new.connection.as_ref())?;
        }
        if old.location != new.location {
            self.write(keys::SQUARE_LOCATION, new.location.as_ref())?;
        }
        if old.point_of_sale != new.point_of_sale {
            self.set_point_of_sale(new.point_of_sale)?;
        }
        if old.without_pos != new.without_pos {
            self.set_without_pos(new.without_pos)?;
        }
        if old.shift_on != new.shift_on {
            self.put_switch(keys::SHIFT_ON, new.shift_on)?;
        }
        if old.shift_started_at != new.shift_started_at {
            self.set_shift_started_at(new.shift_started_at)?;
        }
        if old.setup_done != new.setup_done {
            self.put_flag(keys::SETUP_DONE, new.setup_done)?;
        }
        if old.bubble_on != new.bubble_on {
            self.put_switch(keys::BUBBLE_ON, new.bubble_on)?;
        }
        if old.bubble_left != new.bubble_left {
            self.put_switch(keys::BUBBLE_LEFT, new.bubble_left)?;
        }
        if old.bubble_y != new.bubble_y {
            self.set_bubble_y(new.bubble_y)?;
        }
        if old.pending_sign_in != new.pending_sign_in {
            self.set_pending_sign_in(new.pending_sign_in.as_ref())?;
        }
        if old.square_application_id != new.square_application_id {
            self.put(
                keys::SQUARE_APPLICATION,
                new.square_application_id.as_deref(),
            )?;
        }
        if old.clover_application_id != new.clover_application_id {
            self.put(
                keys::CLOVER_APPLICATION,
                new.clover_application_id.as_deref(),
            )?;
        }
        if old.wake_up_choice != new.wake_up_choice {
            self.set_wake_up_choice(&new.wake_up_choice)?;
        }
        if old.own_bell != new.own_bell {
            self.write(keys::WAKE_UPS_OWN, new.own_bell.as_ref())?;
        }
        if old.registration != new.registration {
            self.write(keys::WAKE_UPS_REGISTRATION, new.registration.as_ref())?;
        }
        if old.square_base_url != new.square_base_url {
            self.put_address(keys::DEBUG_SQUARE_BASE, new.square_base_url.as_deref())?;
        }
        if old.clover_base_url != new.clover_base_url {
            self.put_address(keys::DEBUG_CLOVER_BASE, new.clover_base_url.as_deref())?;
        }
        if old.clover_merchant != new.clover_merchant {
            self.write(keys::CLOVER_MERCHANT, new.clover_merchant.as_ref())?;
        }
        Ok(())
    }

    /// Forgets the store and everything about this till's point of sale
    /// (`Stores.forgetAll`): the shift switch, the bubble and the debug
    /// stand-ins stay. `model` is what was kept, for the Clover merchant
    /// whose tender id goes too.
    pub fn forget_all(&self, model: &Model) -> Result<Model, SecretError> {
        let forgotten = Model {
            shift_on: model.shift_on,
            bubble_on: model.bubble_on,
            bubble_left: model.bubble_left,
            bubble_y: model.bubble_y,
            square_base_url: model.square_base_url.clone(),
            clover_base_url: model.clover_base_url.clone(),
            ..Model::default()
        };
        self.save(model, &forgotten)?;
        // Each written as Stores.forgetAll writes it, whatever was held.
        self.set_connection(None)?;
        self.set_point_of_sale(None)?;
        self.set_without_pos(false)?;
        for pos in PointOfSale::ALL {
            self.set_tokens(pos, None)?;
            self.forget_cursor(pos)?;
        }
        if let Some(merchant) = &model.clover_merchant {
            self.set_tender_id(&merchant.id, None)?;
        }
        self.set_shift_sales(&[])?;
        Ok(forgotten)
    }
}

/// The small values the app keeps, in memory: everything but the sign-ins,
/// cursors, tender ids and the shift, which another process (the iOS
/// notification extension) may change, so they are read through each time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    /// The store's key and API, nothing else about it (decision 50 of
    /// android.md).
    pub connection: Option<Connection>,
    /// The Square location this till watches.
    pub location: Option<SquareLocation>,
    /// The point of sale chosen, in setup or Settings (decision 126).
    pub point_of_sale: Option<PointOfSale>,
    /// The cashier chose to run without a point of sale.
    pub without_pos: bool,
    /// The till watches for sales: on until the cashier stops it.
    pub shift_on: bool,
    /// When this shift began, Unix seconds.
    pub shift_started_at: Option<i64>,
    /// The cashier went through setup to the end.
    pub setup_done: bool,
    /// Android's bubble: wanted, which side, how far down.
    pub bubble_on: bool,
    pub bubble_left: bool,
    pub bubble_y: Option<i32>,
    pub pending_sign_in: Option<PendingSignIn>,
    /// The applications the till signs in through, when a Bell's.
    pub square_application_id: Option<String>,
    pub clover_application_id: Option<String>,
    /// Settings › Wake-ups.
    pub wake_up_choice: WakeUpChoice,
    pub own_bell: Option<BellAddress>,
    pub registration: Option<BellRegistration>,
    /// Debug builds' stand-ins.
    pub square_base_url: Option<String>,
    pub clover_base_url: Option<String>,
    /// The Clover merchant this till is at (decision 127).
    pub clover_merchant: Option<CloverMerchant>,
}

impl Default for Model {
    /// A till nothing was kept for: watching on, the bubble wanted.
    fn default() -> Model {
        Model {
            connection: None,
            location: None,
            point_of_sale: None,
            without_pos: false,
            shift_on: true,
            shift_started_at: None,
            setup_done: false,
            bubble_on: true,
            bubble_left: false,
            bubble_y: None,
            pending_sign_in: None,
            square_application_id: None,
            clover_application_id: None,
            wake_up_choice: WakeUpChoice::TimerOnly,
            own_bell: None,
            registration: None,
            square_base_url: None,
            clover_base_url: None,
            clover_merchant: None,
        }
    }
}

impl Model {
    /// Where the till watches, by name: its Square location, or its Clover
    /// merchant.
    pub fn place_name(&self) -> Option<String> {
        match self.point_of_sale? {
            PointOfSale::Square => self.location.as_ref().map(|l| l.name.clone()),
            PointOfSale::Clover => self.clover_merchant.as_ref().map(|m| m.name.clone()),
        }
    }
}

/// The [`Model`] in memory over its [`Store`], written through.
pub struct Kept {
    store: Store,
    model: Mutex<Model>,
    log: Log,
}

impl Kept {
    /// Reads the model; a store that can't answer is an error.
    pub fn load(store: Store, log: Log) -> Result<Kept, SecretError> {
        let model = store.load()?;
        Ok(Kept {
            store,
            model: Mutex::new(model),
            log,
        })
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn log(&self, line: String) {
        (self.log)(line)
    }

    fn lock(&self) -> MutexGuard<'_, Model> {
        self.model.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The model now.
    pub fn model(&self) -> Model {
        self.lock().clone()
    }

    pub fn with<R>(&self, f: impl FnOnce(&Model) -> R) -> R {
        f(&self.lock())
    }

    /// Changes the model, writing what changed first: when the store
    /// refuses, the model stays as it was.
    pub fn update(&self, change: impl FnOnce(&mut Model)) -> Result<(), SecretError> {
        let mut model = self.lock();
        let mut new = model.clone();
        change(&mut new);
        self.store.save(&model, &new)?;
        *model = new;
        Ok(())
    }

    /// Changes the model in memory whatever the store says, and logs a
    /// write that failed: for what can't report one (till-bell's
    /// [`WakeUpStore`], decision 20).
    pub fn update_anyway(&self, change: impl FnOnce(&mut Model)) {
        let mut model = self.lock();
        let mut new = model.clone();
        change(&mut new);
        if let Err(e) = self.store.save(&model, &new) {
            self.log(format!("till-core: couldn't keep a change: {e}"));
        }
        *model = new;
    }

    /// Forgets everything `Stores.forgetAll` forgot.
    pub fn forget_all(&self) -> Result<(), SecretError> {
        let mut model = self.lock();
        *model = self.store.forget_all(&model)?;
        Ok(())
    }
}

/// A point of sale's sign-in (till-pos's [`TokenStore`]), read through each
/// time so a refresh another process made is seen.
///
/// `TokenStore::load` returns an `Option`, so a store that can't answer
/// reads as signed out here: logged, and never saved over. It should
/// return a `Result` (decision 17), which is till-pos's to change.
pub struct PosTokenStore {
    store: Store,
    pos: PointOfSale,
    log: Log,
}

impl PosTokenStore {
    pub fn new(store: Store, pos: PointOfSale, log: Log) -> PosTokenStore {
        PosTokenStore { store, pos, log }
    }
}

impl TokenStore for PosTokenStore {
    fn load(&self) -> Option<PosTokens> {
        self.store.tokens(self.pos).unwrap_or_else(|e| {
            (self.log)(format!(
                "till-core: couldn't read the {} sign-in: {e}",
                self.pos.wire()
            ));
            None
        })
    }

    fn save(&self, tokens: Option<PosTokens>) {
        if let Err(e) = self.store.set_tokens(self.pos, tokens.as_ref()) {
            (self.log)(format!(
                "till-core: couldn't keep the {} sign-in: {e}",
                self.pos.wire()
            ));
        }
    }
}

/// Where a point of sale's search for sales has got to (till-pos's
/// [`CursorStore`]). A cursor that can't be read starts from now, as a
/// first look does.
pub struct PosCursor {
    store: Store,
    pos: PointOfSale,
    log: Log,
}

impl PosCursor {
    pub fn new(store: Store, pos: PointOfSale, log: Log) -> PosCursor {
        PosCursor { store, pos, log }
    }

    fn logged<T>(&self, what: &str, result: Result<T, SecretError>) -> Option<T> {
        result
            .map_err(|e| {
                (self.log)(format!(
                    "till-core: couldn't {what} the {} cursor: {e}",
                    self.pos.wire()
                ))
            })
            .ok()
    }
}

impl CursorStore for PosCursor {
    fn since(&self) -> Option<String> {
        self.logged("read", self.store.since(self.pos)).flatten()
    }

    fn handled(&self) -> Vec<String> {
        self.logged("read", self.store.handled(self.pos))
            .unwrap_or_default()
    }

    fn save(&self, since: &str, handled: &[String]) {
        self.logged("keep", self.store.save_cursor(self.pos, since, handled));
    }
}

/// The Monero tender's id at each Clover merchant (decision 129). One that
/// can't be read is looked for again in Clover.
pub struct Tenders {
    store: Store,
    log: Log,
}

impl Tenders {
    pub fn new(store: Store, log: Log) -> Tenders {
        Tenders { store, log }
    }
}

impl TenderStore for Tenders {
    fn tender_id(&self, merchant_id: &str) -> Option<String> {
        self.store.tender_id(merchant_id).unwrap_or_else(|e| {
            (self.log)(format!("till-core: couldn't read the tender id: {e}"));
            None
        })
    }

    fn save(&self, merchant_id: &str, tender_id: Option<&str>) {
        if let Err(e) = self.store.set_tender_id(merchant_id, tender_id) {
            (self.log)(format!("till-core: couldn't keep the tender id: {e}"));
        }
    }
}

/// The shift's sales (till-sales' [`ShiftStore`]), encrypted like
/// everything else (decision 64 of android.md).
///
/// `ShiftStore::load` can't fail, so a store that can't answer loads an
/// empty shift; then nothing is saved over the kept one until a load
/// succeeds, or the till's next change would replace the shift with
/// nothing. It should return a `Result`.
pub struct ShiftKept {
    store: Store,
    log: Log,
    unread: AtomicBool,
}

impl ShiftKept {
    pub fn new(store: Store, log: Log) -> ShiftKept {
        ShiftKept {
            store,
            log,
            unread: AtomicBool::new(false),
        }
    }

    /// Whether the kept shift couldn't be read, so isn't being saved over.
    pub fn unread(&self) -> bool {
        self.unread.load(Ordering::SeqCst)
    }
}

impl ShiftStore for ShiftKept {
    fn load(&self) -> Vec<TillSale> {
        match self.store.shift_sales() {
            Ok(sales) => {
                self.unread.store(false, Ordering::SeqCst);
                sales
            }
            Err(e) => {
                self.unread.store(true, Ordering::SeqCst);
                (self.log)(format!("till-core: couldn't read the shift: {e}"));
                Vec::new()
            }
        }
    }

    fn save(&self, sales: &[TillSale]) {
        if self.unread() {
            (self.log)("till-core: the kept shift wasn't read, so isn't saved over".into());
            return;
        }
        if let Err(e) = self.store.set_shift_sales(sales) {
            (self.log)(format!("till-core: couldn't keep the shift: {e}"));
        }
    }
}

/// Settings › Wake-ups' choice and registration (till-bell's
/// [`WakeUpStore`]): read from memory, written through; a write that fails
/// is logged and the memory keeps the change (decision 20).
pub struct WakeUpsKept(pub Arc<Kept>);

impl WakeUpStore for WakeUpsKept {
    fn choice(&self) -> WakeUpChoice {
        self.0.with(|m| m.wake_up_choice.clone())
    }

    fn set_choice(&self, choice: WakeUpChoice) {
        self.0.update_anyway(|m| m.wake_up_choice = choice);
    }

    fn own_bell(&self) -> Option<BellAddress> {
        self.0.with(|m| m.own_bell.clone())
    }

    fn set_own_bell(&self, bell: Option<BellAddress>) {
        self.0.update_anyway(|m| m.own_bell = bell);
    }

    fn registration(&self) -> Option<BellRegistration> {
        self.0.with(|m| m.registration.clone())
    }

    fn set_registration(&self, registration: Option<BellRegistration>) {
        self.0.update_anyway(|m| m.registration = registration);
    }
}

#[cfg(test)]
mod tests;
