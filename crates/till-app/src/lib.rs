//! Till's engine (docs/ios-plan.md §2): everything the app does, with no
//! toolkit in it. A platform draws the [`Update`]s published here, carries out
//! the [`Effect`]s, and sends back what the cashier did as [`Action`]s.
//!
//! One task owns the state. [`Engine::send`] never blocks: actions queue on a
//! channel, background work comes back to the task as a message (dropped if
//! the till was connected afresh or forgotten meanwhile), and the task
//! publishes each [`Update`] only when it changed, through the
//! [`EngineListener`].
//!
//! - [`store`]: the persisted model, in `Stores.kt`'s keys and JSON, kept
//!   through [`Platform`]'s secrets.
//! - The engine owns the till: till-sales' `TillRunner` and `LiveStore`,
//!   the points of sale, and till-bell's `BellWakeUps`, whose push side the
//!   platform carries through [`PushPlatform`] and the `Push…` actions.
//! - [`preview_shift`], [`preview_sale`] and [`preview_close`]: the till's
//!   screens from a saved shift, with no engine, for previews and tests.
//! - [`background_check`]: one look for new sales with no engine, for the
//!   iOS Notification Service Extension and Android's ding path.

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

// An order id crosses as till-screens' custom type over String.
#[cfg(feature = "uniffi")]
uniffi::use_remote_type!(till_screens::OrderId);

mod background;
mod engine;
mod ledger;
pub mod lifecycle;
mod phases;
mod preview;
mod preview_sales;
mod preview_setup;
mod push;
mod relay;
mod side;
pub mod store;
#[cfg(test)]
mod testing;
mod web_push;

use monokulo_client::OrderId;
use std::sync::{Arc, OnceLock};

use tokio::sync::mpsc::UnboundedSender;

pub use background::{
    BackgroundCheck, LookStep, background_check, background_check_ding, is_relayed_ding,
};
pub use lifecycle::{BackgroundWork, set_background_work};
use monokulo_client::PointOfSale;
pub use preview::{TillPreview, preview_close, preview_panel, preview_sale, preview_shift};
pub use preview_sales::{
    PreviewPhase, PreviewSale, preview_phase_step, preview_sale_live, preview_sales_json,
};
pub use preview_setup::{PreviewStep, preview_onboarding, preview_settings, preview_wake_ups};
pub use till_screens::Device;
use till_screens::bubble::{BubbleLook, BubbleTap, PanelView};
use till_screens::notification::Notice;
use till_screens::order_text::OrderPageView;
use till_screens::shift::ShiftFilter;
use till_screens::view::Press;
use till_screens::wallet_setup::{PaymentsMode, WalletNetwork, WalletWay};
use till_screens::{
    CloseView, CodeView, LiveActivityState, NotificationView, OnboardingView, SalePageView,
    SaleView, SettingsView, ShiftView, WakeUpsView,
};

/// The version of the core, for the platform's About screen and logs.
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the platform keeps and knows for the engine: secrets (the Android
/// Keystore, or the iOS Keychain in a group the extension shares), and how
/// this device writes amounts of money.
#[cfg_attr(feature = "uniffi", uniffi::export(with_foreign))]
pub trait Platform: Send + Sync {
    /// `None` only when there is no such secret. A store that can't tell says
    /// so with an error: taking that for "none" would replace a saved
    /// connection with nothing.
    fn secret_get(&self, key: String) -> Result<Option<Vec<u8>>, SecretError>;
    fn secret_put(&self, key: String, value: Vec<u8>) -> Result<(), SecretError>;
    fn secret_remove(&self, key: String) -> Result<(), SecretError>;
    /// `minor` units of `currency` (ISO 4217) in the device locale's money
    /// pattern, with `¤` (U+00A4) where the symbol goes: 2450 and "USD" is
    /// "¤24.50" in the US, "24,50 ¤" in Germany (docs/design.md §6). The
    /// engine chooses the symbol (`monokulo_client::amounts::written`,
    /// core.md decision 99), so both platforms write the same one.
    fn format_money(&self, minor: i64, currency: String) -> String;
    /// How the device's locale names `currency`'s symbol, and its own
    /// country's currency.
    fn currency_symbols(&self, currency: String) -> CurrencySymbols;
}

/// What the device's locale knows about a currency's symbol (core.md
/// decision 99).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CurrencySymbols {
    /// The locale's own symbol for it ("$", "R$", "A$"), or the code
    /// itself where the locale writes only the code.
    pub local: String,
    /// International English's (`en-001`): "US$", "A$", "CN¥", or the code.
    pub international: String,
    /// The currency of the device's country; `None` for a locale with no
    /// country.
    pub home: Option<String>,
}

/// The platform's secret store couldn't do what was asked, for now at least.
#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
pub enum SecretError {
    #[error("the secret store is unavailable: {reason}")]
    Unavailable { reason: String },
}

#[cfg(feature = "uniffi")]
impl From<uniffi::UnexpectedUniFFICallbackError> for SecretError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        SecretError::Unavailable { reason: e.reason }
    }
}

/// The device's push subscription for the chosen Bell (till-bell's
/// `PushSubscriptions` and `Distributors`, decision 19): UnifiedPush on
/// Android, APNs through the relay on iOS. Calls come from the engine's
/// tasks and must not block: the platform starts the work and answers
/// with [`Action::PushEndpoint`], [`Action::PushFailed`],
/// [`Action::PushUnregistered`], [`Action::PushReceived`] and
/// [`Action::DistributorsChanged`].
#[cfg_attr(feature = "uniffi", uniffi::export(with_foreign))]
pub trait PushPlatform: Send + Sync {
    /// Subscribe for a Bell whose VAPID public key is `vapid_public_key`
    /// (base64url); subscribing again with another key replaces it.
    fn subscribe(&self, vapid_public_key: String);
    /// End the subscription.
    fn unsubscribe(&self);
    /// Which distributor would carry the dings. iOS has one, always found.
    fn distributors(&self) -> Result<Distributors, PushProblem>;
    /// The cashier's pick from [`Distributors::Choose`].
    fn save_distributor(&self, name: String);
}

/// The distributors on the device, as Wake-ups needs them (decision 91).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Distributors {
    /// The one to use (an Android package name; `apns` on iOS).
    Found { name: String },
    /// Nothing on the device can carry dings.
    None,
    /// Several and no default: the cashier picks one.
    Choose { names: Vec<String> },
}

/// Something on the device failed while asking about distributors.
#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
pub enum PushProblem {
    #[error("the device's push failed: {reason}")]
    Device { reason: String },
}

#[cfg(feature = "uniffi")]
impl From<uniffi::UnexpectedUniFFICallbackError> for PushProblem {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        PushProblem::Device { reason: e.reason }
    }
}

/// Why a push subscription couldn't be made, or ended (till-bell's
/// `PushFailure`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PushFailure {
    NoDistributor,
    Network,
    ActionRequired,
    NoKeys,
    Lost,
    Other,
}

/// Where what the engine publishes goes. Called from the engine's task,
/// never from the platform's UI thread.
#[cfg_attr(feature = "uniffi", uniffi::export(with_foreign))]
pub trait EngineListener: Send + Sync {
    fn on_update(&self, update: Update);
    fn on_effect(&self, effect: Effect);
    /// A line for the system log.
    fn on_log(&self, message: String);
}

/// Which Square the build is for: production's or the sandbox's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum SquareEnvironment {
    Sandbox,
    Production,
}

impl From<SquareEnvironment> for till_pos::SquareEnvironment {
    fn from(environment: SquareEnvironment) -> till_pos::SquareEnvironment {
        match environment {
            SquareEnvironment::Sandbox => till_pos::SquareEnvironment::Sandbox,
            SquareEnvironment::Production => till_pos::SquareEnvironment::Production,
        }
    }
}

/// Which of Clover's platforms the build is for (decision 127 in
/// docs/decisions/android.md): a name a build gets wrong is a compile
/// error in Swift and Kotlin, not an engine that won't start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CloverEnvironment {
    Sandbox,
    NorthAmerica,
    Europe,
    LatinAmerica,
}

impl From<CloverEnvironment> for till_pos::CloverEnvironment {
    fn from(environment: CloverEnvironment) -> till_pos::CloverEnvironment {
        match environment {
            CloverEnvironment::Sandbox => till_pos::CloverEnvironment::Sandbox,
            CloverEnvironment::NorthAmerica => till_pos::CloverEnvironment::NorthAmerica,
            CloverEnvironment::Europe => till_pos::CloverEnvironment::Europe,
            CloverEnvironment::LatinAmerica => till_pos::CloverEnvironment::LatinAmerica,
        }
    }
}

/// Which Square, as the build names it (Android's `BuildConfig`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SquareSetup {
    pub application_id: String,
    /// Where Square's sign-in and API are: production's or the sandbox's.
    pub environment: SquareEnvironment,
    /// Where Square sends the sign-in back: an App Link into the app. The
    /// token exchange names it, as Square requires.
    pub redirect_uri: String,
    /// A stand-in Square, for development; `None` for Square's own.
    pub base_url: Option<String>,
    /// Where the sign-in comes back into the app when that isn't
    /// `redirect_uri` itself: a debug build's `monokulo-till-debug://` link,
    /// which the page at `redirect_uri` forwards to. A redirect to either
    /// is taken (decision 57); `None` takes only `redirect_uri`.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub return_uri: Option<String>,
}

/// Which Clover, as the build names it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CloverSetup {
    pub application_id: String,
    pub environment: CloverEnvironment,
    pub redirect_uri: String,
    pub base_url: Option<String>,
    /// As [`SquareSetup::return_uri`].
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub return_uri: Option<String>,
}

/// The app around the engine: where it keeps things, what it is, and what
/// the build chose.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct EngineConfig {
    /// A directory the app (and on iOS its extension) can write to. The
    /// lock around a point of sale's refresh is a file here.
    pub data_dir: String,
    pub device: Device,
    /// This device, for when the merchant didn't name it ("Pixel 8").
    pub device_name: String,
    pub app_version: String,
    /// A debug build: cleartext connect links, stand-ins, http Bells.
    pub debug: bool,
    /// The device's clock: its offset from UTC now, in seconds, and whether
    /// it keeps a 24-hour clock ([`Action::ClockChanged`] when they change).
    pub utc_offset: i32,
    pub hours_24: bool,
    pub square: SquareSetup,
    pub clover: CloverSetup,
    /// The project's Bell, what "Monokulo Bell" registers with.
    pub bell_url: String,
    /// iOS: the push relay that turns the device's APNs token into a Web
    /// Push endpoint for the Bell (design.md §5, "The relay API"). With it
    /// the engine carries the push side itself: it registers the token
    /// [`Action::PushToken`] hands over, opens what the relay carries, and
    /// never calls the [`PushPlatform`] it was given. `None` on Android,
    /// whose [`PushPlatform`] is UnifiedPush.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub relay_url: Option<String>,
}

impl EngineConfig {
    /// `android` or `ios`, as Monokulo's apps list shows it: the device's.
    pub fn platform(&self) -> &'static str {
        if self.device.is_ios() {
            "ios"
        } else {
            "android"
        }
    }
}

/// A point of sale, as the platform names one to the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Pos {
    Square,
    Clover,
}

impl From<Pos> for PointOfSale {
    fn from(pos: Pos) -> PointOfSale {
        match pos {
            Pos::Square => PointOfSale::Square,
            Pos::Clover => PointOfSale::Clover,
        }
    }
}

/// Settings › Wake-ups' choice, as the platform sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum WakeUps {
    Timer,
    MonokuloBell,
    /// The address as typed; one that isn't a Bell's is ignored (the
    /// screen only offers "Use this Bell" once it is).
    OwnBell {
        address: String,
    },
}

/// The close sheet's answers are `till-screens`' own: one type, so the Swift
/// bindings (one module for every crate) don't declare it twice.
pub use till_screens::close::CloseChoice;
/// The title menu's choices and how a new sale shows up are `till-screens`'
/// own, for the same reason.
pub use till_screens::counters::CounterChoice;
pub use till_screens::settings::ShowSales;

/// Where the app is (Android: `Screen`). Setup follows the design's six
/// onboarding steps.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Screen {
    /// 1: what Till does.
    Welcome,
    /// V1: how Till sees payments, Monokulo or the shop's own wallet
    /// (docs/view-key-mode.md §11.1).
    PaymentsMode,
    /// V2: Connect to Fob, or enter the wallet yourself.
    WalletEntry,
    /// C1: the request's code and pattern, for Fob on a phone.
    FobRequest,
    /// B4 (phone) or C4 (iPad): connected to Fob.
    FobConnected,
    /// D1: scan or paste a wallet code.
    WalletCode,
    /// D2: the address and view key.
    WalletKeys,
    /// D3: a seed phrase, wiped after.
    WalletSeed,
    /// D4: this till's number, node, confirmations and expiry.
    ThisTill,
    /// 2: scan or paste the store's connect code.
    Scan,
    /// 3: connect to the server, then check the store it is.
    Confirm,
    Connected,
    /// 4: the point of sale and its sign-in; Square's locations next.
    PointOfSale,
    Locations,
    /// After the test sale, when it named its device: which sales alert
    /// here (C4, core.md decision 113).
    Counter,
    /// 5: alerts, then (Android) the bubble, or (iPad) how a new sale shows
    /// up (O4, core.md decision 120).
    Alerts,
    Float,
    ShowSales,
    /// 6: start the shift.
    Ready,
    /// The shift: the till's home.
    Till,
    /// One sale: its code while it's live, else its page.
    Sale {
        order_id: OrderId,
    },
    /// A live sale's code enlarged.
    Code {
        order_id: OrderId,
    },
    /// Every line of a live sale's order.
    Order {
        order_id: OrderId,
    },
    Settings,
}

/// What the cashier did, or what the platform saw.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Action {
    /// The app came to the front: publish everything again, read the store,
    /// look for sales now.
    Opened,
    /// The app went to the back.
    Backgrounded,
    /// The device's time zone or clock style changed.
    ClockChanged {
        utc_offset: i32,
        hours_24: bool,
    },
    /// Alerts and (Android) drawing over other apps, as the system allows
    /// them now.
    Permissions {
        alerts: bool,
        bubble: bool,
    },
    /// One step back.
    Back,
    /// The screen's main button: Welcome's start, step 3's "Continue", step
    /// 4's button under the card (the chosen point of sale's sign-in until
    /// the till is connected to it, then "Continue", or "Done" from
    /// Settings), step 5's. The engine decides which from its state, so the
    /// platform sends `Next` whatever the button says.
    Next,
    /// The screen's way past, under its button: step 4's "Not now: only show
    /// other tills' sales" until connected, then "Skip the test"; step 5's
    /// "Not now"; Android's "Use notifications only" (the bubble off).
    Skip,
    /// A connect link opened as an App Link (or from anywhere else that
    /// isn't a paste or a scan).
    ConnectLink {
        url: String,
    },
    /// Step 2's Paste: the clipboard's text, `None` when it held none (step
    /// 2 then says there's nothing to paste; text that isn't a store's
    /// link, that it isn't one).
    PasteConnectLink {
        text: Option<String>,
    },
    /// Text another app shared with Till, whole: the engine finds the
    /// connect link in it (`connect_code::shared_link`, core.md 103), and
    /// does nothing when it holds no link.
    SharedText {
        text: String,
    },
    /// Step 2's camera read a code: one that isn't a store's connect code is
    /// said so.
    ScanConnectCode {
        text: String,
    },
    /// Step 3: redeem the code.
    Connect,
    ChoosePointOfSale {
        pos: Pos,
    },
    /// The chosen point of sale's sign-in, through `application_id` when
    /// that is a Bell's (decision 96): the page opens as
    /// [`Effect::OpenUrl`].
    SignIn {
        application_id: Option<String>,
    },
    /// A point of sale's redirect back into the app.
    SignInReturned {
        url: String,
    },
    ChooseLocation {
        location_id: String,
    },
    ChangeLocation,
    /// "Look again" for Clover's Monero payment type.
    LookAgain,
    /// "Not now": only other tills' sales.
    WithoutPos,
    SignOutOfPos,
    /// Setup's last step, or watching turned on.
    StartShift,
    /// Watching turned off: every live code comes down.
    StopShift,
    /// "New shift" in the shift's header: a new window now (docs/ledger.md
    /// §9), so the total starts again and only the sales that still need
    /// the cashier, and the live ones, stay listed (core.md decision 223).
    NewShift,
    /// The shift's filter chip tapped.
    ChooseFilter {
        filter: ShiftFilter,
    },
    /// The shift's tray folded or unfolded.
    FoldTray {
        folded: bool,
    },
    /// The panel's "Open the shift": every sale's full screen is left for
    /// now, and the shift comes up.
    OpenShift,
    /// Android's bubble panel rose (`up`) or went back into the bubble. While
    /// it is up the panel's countdown is redrawn each second though the app
    /// is in the back (decision 56), and the free bubble doesn't pulse.
    /// Down, after the panel's window has gone (its collapse landed, or it
    /// went without one): that ends a paid moment's green (decision 95).
    PanelShown {
        up: bool,
    },
    /// The bubble was let go against the `left` edge (else the right) at
    /// height `y`, in pixels: kept, and published in [`BubbleView`].
    MoveBubble {
        left: bool,
        y: i32,
    },
    /// The platform's moving stage came to rest on `screen` (Android's
    /// `FollowScreen`): a new sale rises out of the shift only while it
    /// rests there (decision 266 of android.md).
    Landed {
        screen: Screen,
    },
    /// A sale's X or a row's swipe: the close sheet, while the sale has
    /// answers.
    AskClose {
        order_id: OrderId,
    },
    /// The close sheet went without an answer.
    DismissClose,
    OpenSale {
        order_id: OrderId,
    },
    Enlarge {
        order_id: OrderId,
    },
    ShowOrder {
        order_id: OrderId,
    },
    Close {
        order_id: OrderId,
        choice: CloseChoice,
    },
    Refund {
        order_id: OrderId,
    },
    /// The refund sheet's "I've refunded it".
    ConfirmRefund {
        order_id: OrderId,
    },
    /// "Not yet", Back or the scrim.
    DismissRefund,
    /// "Show the code again" on a closed sale.
    Reopen {
        order_id: OrderId,
    },
    CopyAddress {
        order_id: OrderId,
    },
    /// The XMR amount tapped: it is copied.
    CopyAmount {
        order_id: OrderId,
    },
    /// Settings' "Copy wallet address": the store's whole wallet address
    /// is copied (core.md decision 105).
    CopyWalletAddress,
    OpenSettings,
    /// Settings' "Change the point of sale": step 4, back to Settings once
    /// the new sign-in is done.
    OpenPointOfSale,
    /// Read the store again (step 3's "Try again").
    ReadStore,
    /// Debug builds: a stand-in point of sale's address (acceptance T6.5).
    SetBaseUrl {
        pos: Pos,
        url: String,
    },
    SetBubble {
        on: bool,
    },
    SetTor {
        on: bool,
    },
    /// iPhone's Settings › Dynamic Island: the Live Activity each shift.
    SetLiveActivity {
        on: bool,
    },
    /// iPhone's "Show code in island": the plain payment URI goes through
    /// the push relay to the island while a sale waits (design.md §6).
    SetCodeInIsland {
        on: bool,
    },
    /// Whether Orbot's SOCKS proxy answered, as the platform's probe found
    /// it (decision 10): Android probes as the app comes forward and as
    /// step 3 and Settings open. A till connects over Tor only once a probe
    /// said Orbot runs, and Settings says when it doesn't.
    TorProbed {
        running: bool,
    },
    Disconnect,
    /// The store disconnected this device: forget it.
    Forget,
    ChooseWakeUps {
        choice: WakeUps,
    },
    /// Wake-ups' "My own Bell" while its address is being typed:
    /// `picked` when the row was tapped with no address saved (its field
    /// opens, the saved choice stays ticked), and the field as typed so far
    /// (`None` until it's touched). The keyboard is the platform's; the
    /// engine says whether the address is a Bell's. Choosing wake-ups ends
    /// it. The row tapped (`picked`, nothing typed) with an address saved
    /// chooses that Bell at once.
    EditOwnBell {
        picked: bool,
        typed: Option<String>,
    },
    ChooseDistributor {
        name: String,
    },
    /// The platform gave an endpoint, with the subscription's keys.
    PushEndpoint {
        url: String,
        p256dh: Option<String>,
        auth: Option<String>,
    },
    PushFailed {
        failure: PushFailure,
    },
    /// The platform ended the subscription.
    PushUnregistered,
    /// A push arrived, decrypted or not.
    PushReceived {
        content: Vec<u8>,
        decrypted: bool,
    },
    /// An app came or went, which may have been a distributor.
    DistributorsChanged,
    /// C4 and Settings' "Alert for": this counter's device alone, or every
    /// device at the location (core.md decision 113).
    SetAlertFor {
        every_device: bool,
    },
    /// Settings' "Find this device again with a test sale": the next sale
    /// this till finds names this counter's device.
    FindDevice,
    /// Settings' "Show other counters' sales in the list".
    SetShowOtherCounters {
        on: bool,
    },
    /// The shift's title menu: whose sales the list shows.
    ChooseCounter {
        choice: CounterChoice,
    },
    /// Settings' "Treat cash sales as Monero" (core.md decision 116).
    SetCashAsMonero {
        on: bool,
    },
    /// O4 and Settings: how a new sale shows up (core.md decision 120).
    ChooseShowSales {
        mode: ShowSales,
    },
    /// "Take it in Monero" on an offered cash sale: its order and code are
    /// made, and the sale opens on its code.
    TakeInMonero {
        order_id: OrderId,
    },
    /// "Keep as cash" on an offered cash sale.
    KeepAsCash {
        order_id: OrderId,
    },
    /// "Paid in cash" on a cash sale: closed, nothing refunded.
    PaidInCash {
        order_id: OrderId,
    },
    /// "Mark as refunded" on an overpaid sale: the extra went back to the
    /// customer (core.md decision 222).
    ExtraRefunded {
        order_id: OrderId,
    },
    /// "New code" on a sale whose code ran out unpaid (docs/ledger.md §7):
    /// its next order is made, and the sale opens on its code.
    NewCode {
        order_id: OrderId,
    },
    /// iOS's APNs device token, the bytes in hex, each time iOS gives one
    /// (every launch, from `registerForRemoteNotifications`); `None` when
    /// iOS couldn't register for remote notifications. With a
    /// [`EngineConfig::relay_url`] the engine registers it with the relay
    /// while the Bell is chosen; without, it is ignored. A push the relay
    /// carried comes as [`Action::PushReceived`] with its `aes128gcm`
    /// ciphertext and `decrypted` false: the engine opens it.
    PushToken {
        token: Option<String>,
    },
    /// V1: how Till sees payments.
    ChoosePaymentsMode {
        mode: PaymentsMode,
    },
    /// V2's "Connect to Fob": C1, with a new request. On Android the
    /// platform may also open Fob on this device with the request (C1's
    /// `qr`), by an explicit intent, and send Fob's answer back as
    /// [`Action::FobAnswer`].
    ConnectFob,
    /// V2's "Don't have Fob? Get it for Android": Fob's page opens as
    /// [`Effect::OpenUrl`].
    GetFob,
    /// V2: one of the ways to enter the wallet yourself (D1, D2, D3).
    ChooseWalletWay {
        way: WalletWay,
    },
    /// Fob's answer to C1's request: its code scanned ("Scan Fob's code"),
    /// a link back, or (Android, same device) the activity's result.
    FobAnswer {
        text: String,
    },
    /// C4's "Use another wallet": back to V2, what Fob handed over
    /// forgotten.
    UseAnotherWallet,
    /// D1's camera read a code.
    ScanWalletCode {
        text: String,
    },
    /// D1's "Paste a code instead": the clipboard's text, `None` when it
    /// held none.
    PasteWalletCode {
        text: Option<String>,
    },
    /// D2 as typed so far: each change sends both fields, and the engine
    /// says what's wrong as it's typed. The view key is held only until
    /// the wallet is made.
    EditWalletKeys {
        network: WalletNetwork,
        address: String,
        view_key: String,
    },
    /// D3's "25 words" or "16 words (Polyseed)".
    ChooseSeedLength {
        words: u8,
    },
    /// D3's Continue: the words, space separated. The engine takes the view
    /// key from them and wipes them at once; the platform clears its fields
    /// and drops its copy as soon as it has sent them.
    EnterSeed {
        words: String,
    },
    /// D4's till number, − or +.
    StepTillNumber {
        up: bool,
    },
    /// D4's, B4's and C4's Paid after, − or +: Settled Confirmations.
    StepPaidAfter {
        up: bool,
    },
    /// D4's, B4's and C4's "A code runs out after", − or +.
    StepCodeExpiry {
        up: bool,
    },
    /// D4's Change: the node's address, typed or one of `choices`. It's
    /// checked at once.
    SetNode {
        url: String,
    },
}

impl Action {
    /// The action's name, and nothing it carries: what a log line may say of
    /// it. Its fields can hold a sign-in's redirect (with its code), a push
    /// subscription's auth secret, an APNs token or a connect link, none of
    /// which may reach the system log, so the log never prints an action's
    /// `Debug` (decision 213).
    pub fn name(&self) -> &'static str {
        match self {
            Action::Opened => "Opened",
            Action::Backgrounded => "Backgrounded",
            Action::ClockChanged { .. } => "ClockChanged",
            Action::Permissions { .. } => "Permissions",
            Action::Back => "Back",
            Action::Next => "Next",
            Action::Skip => "Skip",
            Action::ConnectLink { .. } => "ConnectLink",
            Action::PasteConnectLink { .. } => "PasteConnectLink",
            Action::SharedText { .. } => "SharedText",
            Action::ScanConnectCode { .. } => "ScanConnectCode",
            Action::Connect => "Connect",
            Action::ChoosePointOfSale { .. } => "ChoosePointOfSale",
            Action::SignIn { .. } => "SignIn",
            Action::SignInReturned { .. } => "SignInReturned",
            Action::ChooseLocation { .. } => "ChooseLocation",
            Action::ChangeLocation => "ChangeLocation",
            Action::LookAgain => "LookAgain",
            Action::WithoutPos => "WithoutPos",
            Action::SignOutOfPos => "SignOutOfPos",
            Action::StartShift => "StartShift",
            Action::StopShift => "StopShift",
            Action::NewShift => "NewShift",
            Action::ChooseFilter { .. } => "ChooseFilter",
            Action::FoldTray { .. } => "FoldTray",
            Action::OpenShift => "OpenShift",
            Action::PanelShown { .. } => "PanelShown",
            Action::MoveBubble { .. } => "MoveBubble",
            Action::Landed { .. } => "Landed",
            Action::AskClose { .. } => "AskClose",
            Action::DismissClose => "DismissClose",
            Action::OpenSale { .. } => "OpenSale",
            Action::Enlarge { .. } => "Enlarge",
            Action::ShowOrder { .. } => "ShowOrder",
            Action::Close { .. } => "Close",
            Action::Refund { .. } => "Refund",
            Action::ConfirmRefund { .. } => "ConfirmRefund",
            Action::DismissRefund => "DismissRefund",
            Action::Reopen { .. } => "Reopen",
            Action::CopyAddress { .. } => "CopyAddress",
            Action::CopyAmount { .. } => "CopyAmount",
            Action::CopyWalletAddress => "CopyWalletAddress",
            Action::OpenSettings => "OpenSettings",
            Action::OpenPointOfSale => "OpenPointOfSale",
            Action::ReadStore => "ReadStore",
            Action::SetBaseUrl { .. } => "SetBaseUrl",
            Action::SetBubble { .. } => "SetBubble",
            Action::SetTor { .. } => "SetTor",
            Action::SetLiveActivity { .. } => "SetLiveActivity",
            Action::SetCodeInIsland { .. } => "SetCodeInIsland",
            Action::TorProbed { .. } => "TorProbed",
            Action::Disconnect => "Disconnect",
            Action::Forget => "Forget",
            Action::ChooseWakeUps { .. } => "ChooseWakeUps",
            Action::EditOwnBell { .. } => "EditOwnBell",
            Action::ChooseDistributor { .. } => "ChooseDistributor",
            Action::PushEndpoint { .. } => "PushEndpoint",
            Action::PushFailed { .. } => "PushFailed",
            Action::PushUnregistered => "PushUnregistered",
            Action::PushReceived { .. } => "PushReceived",
            Action::DistributorsChanged => "DistributorsChanged",
            Action::SetAlertFor { .. } => "SetAlertFor",
            Action::FindDevice => "FindDevice",
            Action::SetShowOtherCounters { .. } => "SetShowOtherCounters",
            Action::ChooseCounter { .. } => "ChooseCounter",
            Action::SetCashAsMonero { .. } => "SetCashAsMonero",
            Action::ChooseShowSales { .. } => "ChooseShowSales",
            Action::TakeInMonero { .. } => "TakeInMonero",
            Action::KeepAsCash { .. } => "KeepAsCash",
            Action::PaidInCash { .. } => "PaidInCash",
            Action::ExtraRefunded { .. } => "ExtraRefunded",
            Action::NewCode { .. } => "NewCode",
            Action::PushToken { .. } => "PushToken",
            Action::ChoosePaymentsMode { .. } => "ChoosePaymentsMode",
            Action::ConnectFob => "ConnectFob",
            Action::GetFob => "GetFob",
            Action::ChooseWalletWay { .. } => "ChooseWalletWay",
            Action::FobAnswer { .. } => "FobAnswer",
            Action::UseAnotherWallet => "UseAnotherWallet",
            Action::ScanWalletCode { .. } => "ScanWalletCode",
            Action::PasteWalletCode { .. } => "PasteWalletCode",
            Action::EditWalletKeys { .. } => "EditWalletKeys",
            Action::ChooseSeedLength { .. } => "ChooseSeedLength",
            Action::EnterSeed { .. } => "EnterSeed",
            Action::StepTillNumber { .. } => "StepTillNumber",
            Action::StepPaidAfter { .. } => "StepPaidAfter",
            Action::StepCodeExpiry { .. } => "StepCodeExpiry",
            Action::SetNode { .. } => "SetNode",
        }
    }
}

/// A slice of what the screens show, sent only when it changed. Each
/// screen's record is till-screens', crossing UniFFI as it is.
// Sent by value across UniFFI, which can't box a variant.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Update {
    /// The core is running.
    Core { version: String },
    /// Where the app is.
    Screen { screen: Screen },
    /// Busy with a network call (buttons wait), and what went wrong last;
    /// and of those, what the platform shows itself (core.md decision
    /// 109): `shown_busy` and `shown_problem` leave out what the screen's
    /// own view already shows (step 3's and step 4's busy button and
    /// problem), so nothing is said twice.
    Work {
        busy: bool,
        problem: Option<String>,
        shown_busy: bool,
        shown_problem: Option<String>,
    },
    /// Android keeps its foreground service while the till runs.
    Service { running: bool },
    /// Onboarding's step, as `till_screens::onboarding` draws it, while
    /// setup is on screen.
    Onboarding { view: OnboardingView },
    /// Settings, as `till_screens::settings` draws it.
    Settings { view: SettingsView },
    /// Settings › Wake-ups, as `till_screens::wake_ups` draws it: Android
    /// draws it inside Settings, iOS on its own screen.
    WakeUps { view: WakeUpsView },
    /// How the screen in the next [`Update::Screen`] came: `by_itself` when
    /// the sales put it there (a new sale rising, a code going down), not
    /// the cashier, so it arrives without growing out of anything (M7);
    /// and where Back would go from it, for a back gesture's preview
    /// (`None` where Back shows none); and whether the system's Back is the
    /// app's there at all (`back_enabled`): everywhere but Welcome, the
    /// shift and step 4 during setup, where Back leaves the app (android.md
    /// decision 77, core.md 98); and which way the change to it goes from
    /// the screen before (core.md 101): `back` toward the shift or back
    /// through setup, and `steps` when both are steps of setup, which slide.
    /// Sent before the screen it describes.
    Nav {
        by_itself: bool,
        back_target: Option<Screen>,
        back_enabled: bool,
        back: bool,
        steps: bool,
    },
    /// The shift, as `till_screens::shift` draws it, once the till is set up.
    Shift { view: ShiftView },
    /// The sale on screen ([`Screen::Sale`], `Code` or `Order`): its live
    /// screen, its page, its enlarged code, and its whole order while the
    /// till has its receipt.
    Sale {
        order_id: OrderId,
        view: SaleView,
        page: SalePageView,
        code: CodeView,
        order: Option<OrderPageView>,
    },
    /// The close sheet over a sale, while one is asked.
    CloseSheet {
        order_id: Option<OrderId>,
        view: Option<CloseView>,
    },
    /// The refund sheet over a sale the cashier refunds at the point of sale.
    RefundSheet { view: Option<RefundSheetView> },
    /// Android's bubble over other apps and its panel.
    Bubble { view: BubbleView },
    /// The notifications, as `till_screens::notification` draws them:
    /// Android's ongoing one, and iOS's alert for a new sale. `alert` (and
    /// an ongoing sale's `alert`) is set in one update only, as the sale
    /// arrives while the app isn't in front: post it then. A later update
    /// without it changes nothing already posted.
    Notification { view: NotificationView },
    /// iPhone's Live Activity, as `till_screens::live_activity` draws it
    /// (decision 40): `None` while there should be none (the switch off in
    /// Settings, no shift on, setup not done). Never sent on iPad or
    /// Android.
    LiveActivity { state: Option<LiveActivityState> },
}

/// The refund sheet (decision 130 of android.md), ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RefundSheetView {
    pub order_id: OrderId,
    /// "Refund in Clover".
    pub title: String,
    /// What to refund, and where.
    pub body: String,
    /// "I've refunded it" asked, and the point of sale doesn't show it yet.
    pub not_seen: Option<String>,
    /// "I've refunded it", or "Looking…" while the point of sale is asked.
    pub refunded: Press,
    /// "Not yet".
    pub not_yet: String,
}

/// Android's bubble and its panel, ready to draw.
///
/// Where the bubble rests (`left`, `y`) is kept by the engine, though only
/// the platform reads it: an upgraded till has it under `Stores.kt`'s
/// `bubble.left` and `bubble.y`, in the store the engine owns and caches
/// (decision 42), which a second writer would leave stale. The platform
/// sends [`Action::MoveBubble`] once a drag lets go, never per frame.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct BubbleView {
    /// The cashier wants it, Android allows it, a shift is on, the till is
    /// running and still connected (the store's 401 takes it down; a failed
    /// poll doesn't, android.md decision 208), and Till's own screen isn't
    /// up.
    pub shown: bool,
    /// The bubble rests against the screen's left edge (else its right).
    pub left: bool,
    /// Its height on screen, in pixels, as last let go; `None` until it has
    /// been moved (Android puts it a third of the way down).
    pub y: Option<i32>,
    /// Green from a payment through the paid dwell, and through the
    /// collapse at its end until the panel has gone (decision 95).
    pub look: BubbleLook,
    /// The sale the bubble and its panel are about.
    pub order_id: Option<OrderId>,
    /// What a screen reader says for the bubble.
    pub label: String,
    /// What a tap does with the panel down, and with it up.
    pub tap: BubbleTap,
    pub tap_label: String,
    pub tap_open: BubbleTap,
    pub tap_open_label: String,
    /// The panel: the sale as its live screen shows it, with its header,
    /// ring, hero, whole order and ended words.
    pub panel: Option<PanelView>,
    /// The free bubble pulses: a code waits for its money and the panel is
    /// down (as [`Action::PanelShown`] last said).
    pub pulses: bool,
    /// A payment landing buzzes the bubble (the app isn't on screen).
    pub buzzes: bool,
}

/// A buzz the platform gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Haptic {
    /// A payment landed while the app is on screen.
    Paid,
}

/// Something only the platform can do.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Effect {
    /// Open `url` in the browser (a point of sale's sign-in page).
    OpenUrl {
        url: String,
    },
    /// Android: post a sale's own notification in the ended group (its
    /// "Paid", or "No payment yet" with how to refund it), replacing one for
    /// the same `order_id`. `paid` gives it the paid look; `timeout_ms`, when
    /// set, is how long it stays (a "Paid" one, core.md decision 106);
    /// `quiet` posts it without sound or banner, the app being on screen.
    /// With two or more showing, their summary is [`ended_summary`]'s.
    Notify {
        order_id: OrderId,
        notice: Notice,
        paid: bool,
        timeout_ms: Option<u64>,
        quiet: bool,
    },
    /// The sale `id` left the shift: what was posted about it goes.
    ClearNotification {
        id: String,
    },
    Haptic {
        haptic: Haptic,
    },
    /// Android: the paid dwell is over with the bubble's panel still up on
    /// the sale just paid and no other code waiting, so the panel collapses
    /// into the bubble. The bubble stays green until the platform says the
    /// panel has gone ([`Action::PanelShown`] with `up: false`).
    LowerPanel,
    /// Android: see whether Orbot is running and say so with
    /// [`Action::TorProbed`]. Sent as the app comes forward and as step 3
    /// or Settings comes up (decision 54, core.md 102); iOS has no Tor and
    /// never gets it.
    ProbeTor,
    /// Put `text` on the clipboard; `kind` says what it is, for the
    /// platform's confirmation ("Address copied") and for marking an
    /// address sensitive, without reading the text.
    CopyText {
        text: String,
        kind: CopyKind,
    },
}

/// What [`Effect::CopyText`] copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CopyKind {
    /// A sale's wallet address ("Copy address").
    Address,
    /// A sale's XMR amount, as its figures ("0.05552"; the amount tapped).
    Amount,
    Other,
}

/// The summary of Android's ended group with `ended` of its notifications
/// showing, and how long it stays: "3 sales ended"; `None` below two,
/// where the platform posts none (core.md decision 106).
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn ended_summary(ended: u32) -> Option<till_screens::notification::EndedSummary> {
    till_screens::notification::ended_summary(ended)
}

/// The Tokio runtime the engine runs on.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("till-core")
            .enable_all()
            .build()
            .expect("start the Tokio runtime")
    })
}

#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct Engine {
    tx: UnboundedSender<engine::Msg>,
    /// The engine's books, once it has made them: what [`Engine::suspend`]
    /// closes from the platform's own thread.
    books: engine::BooksSlot,
}

impl Engine {
    /// Starts the engine's task. The first [`Update`]s follow at once.
    pub fn start(
        config: EngineConfig,
        platform: Arc<dyn Platform>,
        push: Arc<dyn PushPlatform>,
        listener: Arc<dyn EngineListener>,
    ) -> Arc<Engine> {
        Engine::start_with(config, platform, push, listener, engine::Options::default())
    }

    pub(crate) fn start_with(
        config: EngineConfig,
        platform: Arc<dyn Platform>,
        push: Arc<dyn PushPlatform>,
        listener: Arc<dyn EngineListener>,
        options: engine::Options,
    ) -> Arc<Engine> {
        let books = engine::BooksSlot::default();
        let tx = engine::spawn(
            runtime().handle(),
            config,
            platform,
            push,
            listener,
            options,
            books.clone(),
        );
        Arc::new(Engine { tx, books })
    }
}

// Apart, because a constructor's attribute can't depend on the feature.
#[cfg(feature = "uniffi")]
#[uniffi::export]
impl Engine {
    #[uniffi::constructor]
    pub fn new(
        config: EngineConfig,
        platform: Arc<dyn Platform>,
        push: Arc<dyn PushPlatform>,
        listener: Arc<dyn EngineListener>,
    ) -> Arc<Engine> {
        Engine::start(config, platform, push, listener)
    }
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Engine {
    /// Queues `action` for the engine's task. Never blocks.
    pub fn send(&self, action: Action) {
        // The task only ends with the engine, so a send can't fail before then.
        let _ = self.tx.send(engine::Msg::Action(action));
    }
}

#[cfg(test)]
impl Engine {
    /// What `look` sees of the engine's state, once the task gets to it.
    pub(crate) fn peek<T: Send + 'static>(
        &self,
        look: impl FnOnce(&engine::State) -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = self.tx.send(engine::Msg::Peek(Box::new(move |state| {
            let _ = tx.send(look(state));
        })));
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("the engine answers")
    }

    /// Changes the engine's state with `change` on its task, as the end of
    /// one of its timers would; what changed is published after.
    pub(crate) fn poke(&self, change: impl FnOnce(&mut engine::State) + Send + 'static) {
        let _ = self.tx.send(engine::Msg::Poke(Box::new(change)));
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.tx.send(engine::Msg::Stop);
    }
}
