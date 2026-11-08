//! The till (docs/design.md §1, §3): the shift's sales and where each stands,
//! making Monokulo orders for the sales a point of sale takes, and the
//! wake-up loop that decides when to look.
//!
//! Ported from the Android app's `Till`, `Standing`, `Shift`, `TillRunner`,
//! `LiveStore` and `PosSwitch` (docs/ios-plan.md §3, phase 3). Nothing here
//! knows about a platform or about UniFFI.
//!
//! - [`Till`]: the state machine. It finds the point of sale's Monero sales,
//!   makes each a Monokulo order whose [`PaymentCode`] carries the receipt,
//!   follows the store's events and reads of its orders, and keeps the
//!   cashier's marks (closed, refunded, told to refund in the POS) as
//!   times. Its [`TillState`] is a `watch` channel.
//! - [`TillSale`] and [`Standing`]: a sale, and where it stands, which every
//!   screen's words follow from.
//! - [`ShiftStore`] and [`shift_json`]: the shift kept between runs, in the
//!   JSON the Android app writes.
//! - [`Shift`]: the wake-up loop, checking on a timer and whenever woken.
//! - [`TillRunner`]: a shift's tasks together (checks, the event stream, the
//!   codes' clock, confirmations), over a [`RunnerSide`] the engine
//!   implements; [`NoSource`] for a till not signed in to a point of sale.
//! - [`LiveStore`]: the store and this device as last read, never saved.
//! - [`PosSwitch`]: the ordered side effects of changing the point of sale.
//!
//! Time comes from a [`monokulo_client::Clock`] and tokio's timers, so tests
//! run on tokio's paused clock.

mod kotlin_json;
mod live_store;
mod pos_switch;
mod runner;
mod sale;
mod shift;
mod standing;
mod till;

pub use kotlin_json::shift_json;
pub use live_store::{LiveStore, OnDisconnected, OnOnionApi, StoreNow};
pub use pos_switch::{PosSwitch, PosSwitchSide};
pub use runner::{NoSource, RunnerSide, RunnerState, TillRunner};
pub use sale::{PaymentCode, TillSale, TillState};
pub use shift::{Interval, OnChecked, Shift, WakeCause};
pub use standing::Standing;
pub use till::{OrderStatus, ShiftStore, Till};
