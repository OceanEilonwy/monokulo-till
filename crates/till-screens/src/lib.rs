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
//! - [`shift`], the till's home: its rows, filters and summary;
//! - [`sale`], a live sale; [`close`], its close sheet; [`code`], its code
//!   enlarged; [`sale_page`], a sale's page once it isn't live;
//! - [`notification`], the notifications' faces; [`live_activity`], the
//!   iPhone's Live Activity;
//! - [`wake_ups`], Settings › Wake-ups;
//! - [`settings`], Settings itself;
//! - [`onboarding`], each step of setting up a till;
//! - [`pos_text`], [`sale_text`], [`order_text`] and [`wake_ups_text`], the
//!   words the screens share; [`bubble`] and [`till_nav`], Android's bubble
//!   and where the sales take the till.
//!
//! A screen with a countdown says `ticks`: the engine draws it again a
//! second later while it does.
//!
//! With the `uniffi` feature every view record crosses to Kotlin and Swift
//! as it is (the package `uniffi.till_screens`), with no mirror types.

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

// An order id crosses to Kotlin and Swift as the bare string it was.
#[cfg(feature = "uniffi")]
use monokulo_client::OrderId;
#[cfg(feature = "uniffi")]
uniffi::custom_type!(OrderId, String, {
    remote,
    lower: |id| id.0,
    try_lift: |id| Ok(OrderId(id)),
});

pub mod bubble;
pub mod close;
pub mod code;
pub mod counters;
mod decimal;
#[cfg(test)]
mod fixtures;
pub mod keypad;
pub mod live_activity;
pub mod node_chooser;
pub mod notification;
pub mod onboarding;
pub mod order_text;
pub mod pos_text;
pub mod prices;
pub mod sale;
pub mod sale_page;
pub mod sale_text;
pub mod settings;
pub mod shift;
pub mod till_nav;
pub mod time;
pub mod view;
pub mod wake_ups;
pub mod wake_ups_text;
pub mod wallet_settings;
pub mod wallet_setup;

pub use close::{CloseInput, CloseView, close};
pub use code::{CodeInput, CodeView, code};
pub use live_activity::{LiveActivityInput, LiveActivityState, live_activity};
pub use notification::{NotificationInput, NotificationView, notification};
pub use onboarding::{OnboardingInput, OnboardingView, onboarding};
pub use sale::{SaleInput, SaleView, sale};
pub use sale_page::{SalePageInput, SalePageView, sale_page};
pub use settings::{SettingsInput, SettingsView, settings};
pub use shift::{ShiftInput, ShiftView, shift};
pub use time::DeviceClock;
pub use view::{Device, Fact, PosMark, Tone};
pub use wake_ups::{WakeUpsInput, WakeUpsView, wake_ups};
