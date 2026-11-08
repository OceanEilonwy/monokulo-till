//! What each of Till's screens shows (docs/ios-plan.md §2, "Screens"): the
//! core's records and the clock in, finished rows and words out. Pure
//! functions: nothing here touches the network, a file or a background
//! task, so Android and iOS draw the same thing.
//!
//! A screen is a function from an input struct (which the engine fills from
//! its state) and the [`Device`] it is drawn on, to a view record with every
//! word already chosen. The platform lays the record out and sends back the
//! screen's actions; headings written once and never chosen by state stay in
//! the platform's own screen.
//!
//! - [`wake_ups`], Settings › Wake-ups;
//! - [`settings`], Settings itself;
//! - [`onboarding`], each step of setting up a till;
//! - [`pos_text`] and [`wake_ups_text`], the words the screens share, which
//!   name the point of sale and say where Wake-ups stand.
//!
//! With the `uniffi` feature every view record crosses to Kotlin and Swift
//! as it is (the package `uniffi.till_screens`), with no mirror types.

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

pub mod onboarding;
pub mod pos_text;
pub mod settings;
pub mod time;
pub mod view;
pub mod wake_ups;
pub mod wake_ups_text;

pub use onboarding::{OnboardingInput, OnboardingView, onboarding};
pub use settings::{SettingsInput, SettingsView, settings};
pub use time::DeviceClock;
pub use view::{Device, Fact, PosMark, Tone};
pub use wake_ups::{WakeUpsInput, WakeUpsView, wake_ups};
