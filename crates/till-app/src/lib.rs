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
//! - [`background_check`]: one look for new sales with no engine, for the
//!   iOS Notification Service Extension and Android's ding path.

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

mod background;
mod engine;
mod faces;
mod push;
mod side;
pub mod store;
#[cfg(test)]
mod testing;

use std::sync::{Arc, OnceLock};

use tokio::sync::mpsc::UnboundedSender;

pub use background::{BackgroundCheck, background_check};
pub use faces::NotificationFace;
use monokulo_client::PointOfSale;
pub use till_screens::Device;
use till_screens::{OnboardingView, SettingsView, WakeUpsView};

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
    /// `minor` units of `currency` (ISO 4217) in the device's locale: 2450
    /// and "USD" is "$24.50" in the US (docs/design.md §6).
    fn format_money(&self, minor: i64, currency: String) -> String;
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

/// Which Square, as the build names it (Android's `BuildConfig`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SquareSetup {
    pub application_id: String,
    pub sandbox: bool,
    /// Where Square sends the sign-in back: an App Link into the app.
    pub redirect_uri: String,
    /// A stand-in Square, for development; `None` for Square's own.
    pub base_url: Option<String>,
}

/// Which Clover, as the build names it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CloverSetup {
    pub application_id: String,
    /// `sandbox`, `north_america`, `europe` or `latin_america`.
    pub environment: String,
    pub redirect_uri: String,
    pub base_url: Option<String>,
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
    /// `android` or `ios`, as Monokulo's apps list shows it.
    pub platform: String,
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

/// Where the app is (Android: `Screen`). Setup follows the design's six
/// onboarding steps.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Screen {
    /// 1: what Till does.
    Welcome,
    /// 2: scan or paste the store's connect code.
    Scan,
    /// 3: connect to the server, then check the store it is.
    Confirm,
    Connected,
    /// 4: the point of sale and its sign-in; Square's locations next.
    PointOfSale,
    Locations,
    /// 5: alerts, then (Android) the bubble.
    Alerts,
    Float,
    /// 6: start the shift.
    Ready,
    /// The shift: the till's home.
    Till,
    /// One sale: its code while it's live, else its page.
    Sale {
        order_id: String,
    },
    /// A live sale's code enlarged.
    Code {
        order_id: String,
    },
    /// Every line of a live sale's order.
    Order {
        order_id: String,
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
    /// The screen's way on: Welcome's start, step 3's "Next", step 4's
    /// "Continue", step 5's.
    Next,
    /// A connect link, scanned, pasted or opened as an App Link.
    ConnectLink {
        url: String,
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
    OpenSale {
        order_id: String,
    },
    Enlarge {
        order_id: String,
    },
    ShowOrder {
        order_id: String,
    },
    Close {
        order_id: String,
        choice: CloseChoice,
    },
    Refund {
        order_id: String,
    },
    /// The refund sheet's "I've refunded it".
    ConfirmRefund {
        order_id: String,
    },
    /// "Not yet", Back or the scrim.
    DismissRefund,
    /// "Show the code again" on a closed sale.
    Reopen {
        order_id: String,
    },
    CopyAddress {
        order_id: String,
    },
    OpenSettings,
    SetBubble {
        on: bool,
    },
    SetTor {
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
    /// Busy with a network call (buttons wait), and what went wrong last.
    Work { busy: bool, problem: Option<String> },
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
    /// Post or replace the notification `face.id`.
    Notify {
        face: NotificationFace,
    },
    ClearNotification {
        id: String,
    },
    Haptic {
        haptic: Haptic,
    },
    CopyText {
        text: String,
    },
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
        let tx = engine::spawn(
            runtime().handle(),
            config,
            platform,
            push,
            listener,
            options,
        );
        Arc::new(Engine { tx })
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
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.tx.send(engine::Msg::Stop);
    }
}
