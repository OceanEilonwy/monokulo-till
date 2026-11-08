//! Till's engine (docs/ios-plan.md §2): everything the app does, with no
//! toolkit in it. A platform draws the [`Update`]s published here, carries out
//! the [`Effect`]s, and sends back what the cashier did as [`Action`]s.
//!
//! One task owns the state. [`Engine::send`] never blocks: actions queue on a
//! channel, and the task publishes through the [`EngineListener`].
//!
//! Phase 0 is the shape only: the traits a platform implements, the engine's
//! task, and one action. The screens' records are till-screens' (phase 5a),
//! in [`Update`] already; the engine fills and publishes them in phase 5b.
//!
//! - [`store`]: the persisted model, in `Stores.kt`'s keys and JSON, kept
//!   through [`Platform`]'s secrets.

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

pub mod store;
#[cfg(test)]
mod testing;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, OnceLock};

use till_screens::{OnboardingView, SettingsView, WakeUpsView};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

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

/// Where what the engine publishes goes. Called from the engine's task,
/// never from the platform's UI thread.
#[cfg_attr(feature = "uniffi", uniffi::export(with_foreign))]
pub trait EngineListener: Send + Sync {
    fn on_update(&self, update: Update);
    fn on_effect(&self, effect: Effect);
    /// A line for the system log.
    fn on_log(&self, message: String);
}

/// Where the app keeps things on this device.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct EngineConfig {
    /// A directory the app (and on iOS its extension) can write to.
    pub data_dir: String,
}

/// What the cashier did, or what the platform saw.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Action {
    /// The app came to the front: publish everything again.
    Opened,
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
    /// Onboarding's step, as `till_screens::onboarding` draws it.
    /// Published from phase 5b on, when the engine holds the flow.
    Onboarding { view: OnboardingView },
    /// Settings, as `till_screens::settings` draws it (phase 5b).
    Settings { view: SettingsView },
    /// Settings › Wake-ups, as `till_screens::wake_ups` draws it: Android
    /// draws it inside Settings, iOS on its own screen (phase 5b).
    WakeUps { view: WakeUpsView },
}

/// Something only the platform can do.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Effect {
    /// Open `url` in the browser (a point of sale's sign-in page).
    OpenUrl { url: String },
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
    tx: UnboundedSender<Action>,
}

/// The engine's state, owned by its task.
struct State {
    #[allow(dead_code)] // read from phase 5 on
    config: EngineConfig,
    #[allow(dead_code)]
    platform: Arc<dyn Platform>,
    listener: Arc<dyn EngineListener>,
    published: Option<Update>,
}

impl State {
    fn handle(&mut self, action: Action) {
        match action {
            Action::Opened => {
                self.published = None;
                self.publish(Update::Core {
                    version: CORE_VERSION.to_owned(),
                });
            }
        }
    }

    /// Sends `update` unless it is what was sent last.
    fn publish(&mut self, update: Update) {
        if self.published.as_ref() != Some(&update) {
            self.listener.on_update(update.clone());
            self.published = Some(update);
        }
    }
}

impl Engine {
    /// Starts the engine's task. The first [`Update`]s follow at once.
    pub fn start(
        config: EngineConfig,
        platform: Arc<dyn Platform>,
        listener: Arc<dyn EngineListener>,
    ) -> Arc<Engine> {
        let (tx, mut rx) = unbounded_channel();
        let mut state = State {
            config,
            platform,
            listener,
            published: None,
        };
        runtime().spawn(async move {
            state.handle(Action::Opened);
            while let Some(action) = rx.recv().await {
                let label = format!("{action:?}");
                if catch_unwind(AssertUnwindSafe(|| state.handle(action))).is_err() {
                    state
                        .listener
                        .on_log(format!("till-core: panicked handling {label}"));
                }
            }
        });
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
        listener: Arc<dyn EngineListener>,
    ) -> Arc<Engine> {
        Engine::start(config, platform, listener)
    }
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Engine {
    /// Queues `action` for the engine's task. Never blocks.
    pub fn send(&self, action: Action) {
        // The task only ends with the runtime, so a send can't fail before then.
        let _ = self.tx.send(action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::time::Duration;

    struct Recorder(Mutex<Sender<Update>>);

    impl EngineListener for Recorder {
        fn on_update(&self, update: Update) {
            let _ = self.0.lock().unwrap().send(update);
        }
        fn on_effect(&self, _: Effect) {}
        fn on_log(&self, _: String) {}
    }

    struct NoSecrets;

    impl Platform for NoSecrets {
        fn secret_get(&self, _: String) -> Result<Option<Vec<u8>>, SecretError> {
            Ok(None)
        }
        fn secret_put(&self, _: String, _: Vec<u8>) -> Result<(), SecretError> {
            Ok(())
        }
        fn secret_remove(&self, _: String) -> Result<(), SecretError> {
            Ok(())
        }
        fn format_money(&self, minor: i64, currency: String) -> String {
            format!("{minor} {currency}")
        }
    }

    fn engine() -> (Arc<Engine>, Receiver<Update>) {
        let (tx, rx) = channel();
        let engine = Engine::start(
            EngineConfig {
                data_dir: "/tmp".into(),
            },
            Arc::new(NoSecrets),
            Arc::new(Recorder(Mutex::new(tx))),
        );
        (engine, rx)
    }

    #[test]
    fn a_started_engine_publishes_its_version() {
        let (_engine, rx) = engine();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Update::Core {
                version: CORE_VERSION.to_owned()
            }
        );
    }

    #[test]
    fn opening_the_app_publishes_everything_again() {
        let (engine, rx) = engine();
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        engine.send(Action::Opened);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Update::Core { .. }
        ));
    }
}
