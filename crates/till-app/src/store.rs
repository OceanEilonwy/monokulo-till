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
//! - [`PosTokenStore`], [`PosCursor`], [`Tenders`] and [`WakeUpsKept`]:
//!   the leaf crates' store traits over it. The shift's sales are the
//!   ledger's now (docs/ledger.md); what an older build kept is read once,
//!   into it.

use std::sync::{Arc, Mutex, MutexGuard};

use monokulo_client::{Connection, PointOfSale};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use till_bell::{BellAddress, BellRegistration, WakeUpChoice, WakeUpStore};
use till_pos::clover::{CloverMerchant, CloverMerchantId, CloverTenderId, TenderStore};
use till_pos::square::SquareLocation;
use till_pos::{
    CursorStore, HandledMarker, OAuthState, Pkce, PosTokens, Rfc3339Instant, StoreError, TokenStore,
};
use till_sales::{TillSale, shift_json};
use till_types::{Secret, UnixSeconds};

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
    /// iPhone's Settings › Dynamic Island, which `Stores.kt` never had: the
    /// Live Activity each shift, `true` or `false`, absent is on (decision
    /// 40); never on but on iPhone, so absent on Android is off.
    pub const LIVE_ACTIVITY: &str = "island.activity";
    /// "Show code in island": `true`, or absent (off by default, design.md
    /// §6).
    pub const CODE_IN_ISLAND: &str = "island.code";
    /// iOS's Web Push keys for the relay's dings, which `Stores.kt` never
    /// had: `{"private":…,"auth":…}`, base64url (`web_push`). They survive
    /// `forget_all`, as the device's own.
    pub const PUSH_KEYS: &str = "push.keys";
    /// This counter's point-of-sale device, as the test sale named it
    /// (core.md decision 113): `{"id":…,"name":…,"learnedAt":…}`; absent
    /// until then, when every device's sales are this till's.
    pub const COUNTER_DEVICE: &str = "counter.device";
    /// "Every device at Front Counter": `true`, or absent (alerts for this
    /// device alone).
    pub const COUNTER_EVERY: &str = "counter.every";
    /// "Show other counters' sales in the list": `true` or `false`; absent
    /// is on.
    pub const COUNTER_OTHERS: &str = "counter.others";
    /// "Treat cash sales as Monero" (core.md decision 116): `true` or
    /// `false`; absent is on.
    pub const CASH_AS_MONERO: &str = "cash.monero";
    /// How a new sale shows up (core.md decision 120): `side`; absent is
    /// `alert`, and so is the dropped floating window's `pip` (decision
    /// 225).
    pub const SHOW_SALES: &str = "sales.show";
    /// What Clover calls the merchant's devices (core.md decision 142): a
    /// JSON object of names by device id; absent until read.
    pub const CLOVER_DEVICES: &str = "clover.devices";
    /// The sales whose new-sale alert this till has posted, by the app or
    /// iOS's extension (core.md decision 123): a JSON array of order ids,
    /// oldest first, the newest 500 kept. It survives `forget_all`: an
    /// order id is never another sale's.
    pub const ALERTED: &str = "alerts.posted";

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

/// This counter's point-of-sale device, as kept: its id and name, and when
/// the test sale named it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterKept {
    pub id: till_pos::PosDeviceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub learned_at: UnixSeconds,
}

/// `Stores.kt`'s `PkceState`, as kotlinx wrote it.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PkceState {
    verifier: Secret,
    state: OAuthState,
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

    /// Kept as its Unix seconds' digits.
    pub fn shift_started_at(&self) -> Result<Option<UnixSeconds>, SecretError> {
        Ok(self
            .get(keys::SHIFT_STARTED)?
            .and_then(|s| s.parse().ok())
            .map(UnixSeconds))
    }

    pub fn set_shift_started_at(&self, at: Option<UnixSeconds>) -> Result<(), SecretError> {
        self.put(
            keys::SHIFT_STARTED,
            at.map(|at| at.0.to_string()).as_deref(),
        )
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
            live_activity: self.switch(keys::LIVE_ACTIVITY)?,
            code_in_island: self.flag(keys::CODE_IN_ISLAND)?,
            counter_device: self.read(keys::COUNTER_DEVICE)?,
            alert_every: self.flag(keys::COUNTER_EVERY)?,
            show_others: self.switch(keys::COUNTER_OTHERS)?,
            clover_devices: self.read(keys::CLOVER_DEVICES)?.unwrap_or_default(),
            cash_as_monero: self.switch(keys::CASH_AS_MONERO)?,
            show_sales: crate::ShowSales::from_wire(self.get(keys::SHOW_SALES)?.as_deref()),
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
        if old.live_activity != new.live_activity {
            self.put_switch(keys::LIVE_ACTIVITY, new.live_activity)?;
        }
        if old.code_in_island != new.code_in_island {
            self.put_flag(keys::CODE_IN_ISLAND, new.code_in_island)?;
        }
        if old.counter_device != new.counter_device {
            self.write(keys::COUNTER_DEVICE, new.counter_device.as_ref())?;
        }
        if old.alert_every != new.alert_every {
            self.put_flag(keys::COUNTER_EVERY, new.alert_every)?;
        }
        if old.show_others != new.show_others {
            self.put_switch(keys::COUNTER_OTHERS, new.show_others)?;
        }
        if old.clover_devices != new.clover_devices {
            let names = (!new.clover_devices.is_empty()).then_some(&new.clover_devices);
            self.write(keys::CLOVER_DEVICES, names)?;
        }
        if old.cash_as_monero != new.cash_as_monero {
            self.put_switch(keys::CASH_AS_MONERO, new.cash_as_monero)?;
        }
        if old.show_sales != new.show_sales {
            self.put(keys::SHOW_SALES, new.show_sales.wire())?;
        }
        Ok(())
    }

    /// Forgets the store and everything about this till's point of sale
    /// (`Stores.forgetAll`): the shift switch, the bubble, the island's
    /// switches and the debug stand-ins stay. `model` is what was kept, for the Clover merchant
    /// whose tender id goes too.
    pub fn forget_all(&self, model: &Model) -> Result<Model, SecretError> {
        let forgotten = Model {
            shift_on: model.shift_on,
            bubble_on: model.bubble_on,
            bubble_left: model.bubble_left,
            bubble_y: model.bubble_y,
            square_base_url: model.square_base_url.clone(),
            clover_base_url: model.clover_base_url.clone(),
            live_activity: model.live_activity,
            code_in_island: model.code_in_island,
            show_others: model.show_others,
            cash_as_monero: model.cash_as_monero,
            show_sales: model.show_sales,
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
            self.set_tender_id(merchant.id.as_str(), None)?;
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
    pub shift_started_at: Option<UnixSeconds>,
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
    /// iPhone's Settings › Dynamic Island: the Live Activity each shift (on
    /// unless turned off), and the code in it (off unless turned on).
    pub live_activity: bool,
    pub code_in_island: bool,
    /// This counter's point-of-sale device, once a test sale named it
    /// (core.md decision 113); forgotten with the store.
    pub counter_device: Option<CounterKept>,
    /// Alerts for every device's sales, not only this counter's.
    pub alert_every: bool,
    /// The list opens on every counter's sales (on unless turned off).
    pub show_others: bool,
    /// What Clover calls the merchant's devices, by id (core.md decision
    /// 137); forgotten with the store.
    pub clover_devices: std::collections::BTreeMap<till_pos::PosDeviceId, String>,
    /// A cash sale is treated as Monero as it's found (on unless turned
    /// off, core.md decision 116).
    pub cash_as_monero: bool,
    /// How a new sale shows up (core.md decision 120).
    pub show_sales: crate::ShowSales,
}

impl Default for Model {
    /// A till nothing was kept for: watching on, the bubble and the Live
    /// Activity wanted.
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
            live_activity: true,
            code_in_island: false,
            counter_device: None,
            alert_every: false,
            show_others: true,
            clover_devices: Default::default(),
            cash_as_monero: true,
            show_sales: crate::ShowSales::Alert,
        }
    }
}

impl Model {
    /// How the till is set up (core.md decisions 113 and 116): this
    /// counter's device, and cash.
    pub fn till_options(&self) -> till_sales::TillOptions {
        till_sales::TillOptions {
            device_id: self.counter_device.as_ref().map(|d| d.id.clone()),
            every_device: self.alert_every,
            cash_as_monero: self.cash_as_monero,
        }
    }

    /// The settings the ledger keeps beside [`Model::till_options`]: whether
    /// other counters' sales are listed, and how a new sale shows up, in
    /// the words the platform's store keeps them in (decision 214).
    pub fn ledger_settings(&self) -> [(&'static str, Option<String>); 2] {
        use till_ledger::view::setting;
        [
            (setting::COUNTER_OTHERS, Some(self.show_others.to_string())),
            (
                setting::SHOW_SALES,
                self.show_sales.wire().map(String::from),
            ),
        ]
    }

    /// How the till channels sales by device (core.md decision 113).
    pub fn counters(&self) -> till_screens::counters::Counters {
        till_screens::counters::Counters {
            device: self
                .counter_device
                .as_ref()
                .map(|d| till_screens::counters::CounterDevice {
                    id: d.id.clone(),
                    name: d.name.clone(),
                }),
            every_device: self.alert_every,
            show_others: self.show_others,
            names: match self.point_of_sale {
                Some(PointOfSale::Clover) => self.clover_devices.clone(),
                _ => Default::default(),
            },
        }
    }

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
/// time so a refresh another process made is seen. A store that can't
/// answer is logged and reported, never taken for "signed out" (decision
/// 17).
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

/// `result`, its error logged as couldn't `what`, and made till-pos's.
fn logged<T>(log: &Log, what: String, result: Result<T, SecretError>) -> Result<T, StoreError> {
    result.map_err(|e| {
        log(format!("till-core: couldn't {what}: {e}"));
        StoreError(e.to_string())
    })
}

impl TokenStore for PosTokenStore {
    fn load(&self) -> Result<Option<PosTokens>, StoreError> {
        logged(
            &self.log,
            format!("read the {} sign-in", self.pos.wire()),
            self.store.tokens(self.pos),
        )
    }

    fn save(&self, tokens: Option<PosTokens>) {
        let _ = logged(
            &self.log,
            format!("keep the {} sign-in", self.pos.wire()),
            self.store.set_tokens(self.pos, tokens.as_ref()),
        );
    }
}

/// Where a point of sale's search for sales has got to (till-pos's
/// [`CursorStore`]). A cursor that can't be read is reported, so the source
/// neither starts it afresh nor saves over it.
pub struct PosCursor {
    store: Store,
    pos: PointOfSale,
    log: Log,
}

impl PosCursor {
    pub fn new(store: Store, pos: PointOfSale, log: Log) -> PosCursor {
        PosCursor { store, pos, log }
    }

    fn logged<T>(&self, what: &str, result: Result<T, SecretError>) -> Result<T, StoreError> {
        logged(
            &self.log,
            format!("{what} the {} cursor", self.pos.wire()),
            result,
        )
    }
}

/// Kept as text: the cursor as `Instant.toString()` wrote it, the handled
/// payments as their markers' text.
impl CursorStore for PosCursor {
    fn since(&self) -> Result<Option<Rfc3339Instant>, StoreError> {
        let since = self.logged("read", self.store.since(self.pos))?;
        since
            .map(|text| {
                text.parse().map_err(|_| {
                    StoreError(format!(
                        "The saved {} cursor {text:?} isn't a time.",
                        self.pos.display_name()
                    ))
                })
            })
            .transpose()
    }

    fn handled(&self) -> Result<Vec<HandledMarker>, StoreError> {
        let handled = self.logged("read", self.store.handled(self.pos))?;
        Ok(handled.iter().map(|text| text.parse().unwrap()).collect())
    }

    fn save(&self, since: &Rfc3339Instant, handled: &[HandledMarker]) {
        let handled: Vec<String> = handled.iter().map(ToString::to_string).collect();
        let _ = self.logged(
            "keep",
            self.store.save_cursor(self.pos, since.as_str(), &handled),
        );
    }
}

/// The Monero tender's id at each Clover merchant (decision 129). One that
/// can't be read is reported, so no second tender is made in Clover.
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
    fn tender_id(
        &self,
        merchant_id: &CloverMerchantId,
    ) -> Result<Option<CloverTenderId>, StoreError> {
        let id = logged(
            &self.log,
            "read the tender id".into(),
            self.store.tender_id(merchant_id.as_str()),
        )?;
        Ok(id.map(CloverTenderId::from))
    }

    fn save(&self, merchant_id: &CloverMerchantId, tender_id: Option<&CloverTenderId>) {
        let _ = logged(
            &self.log,
            "keep the tender id".into(),
            self.store
                .set_tender_id(merchant_id.as_str(), tender_id.map(CloverTenderId::as_str)),
        );
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
