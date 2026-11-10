//! Wake-ups from a Monokulo Bell (docs/design.md §3): its address, its API,
//! registering a till with it and keeping that registration in line with the
//! till's choice, and which pushes count as dings.
//!
//! Ported from the Android app's `push` module (docs/ios-plan.md §3,
//! phase 4). Nothing here knows about a platform or about UniFFI.
//!
//! - [`BellAddress`]: where a Bell is, as the cashier types it.
//! - [`BellApi`]: the Bell's three routes, behind [`BellClient`].
//! - [`BellWakeUps`]: the registration machine. It takes the platform's
//!   push subscription ([`PushSubscriptions`], [`Distributors`]), the
//!   point of sale's sign-in ([`PosSide`]) and a [`WakeUpStore`], and
//!   publishes a [`WakeUpStatus`] the shift paces its timer by.
//! - [`is_ding`], [`endpoint`] and [`PushFeed`]: what the platform's push
//!   callbacks mean.
//! - [`pos_words`]: the reasons a status gives, in the cashier's words.

mod address;
mod api;
mod model;
pub mod pos_words;
mod push;
mod wake_ups;

pub use address::{BellAddress, NotABellAddress};
pub use api::{
    BELL_TIMEOUT, BellApi, BellClient, BellError, BellProblem, BellRegistered, BellSlug,
    DeviceError,
};
pub use model::{BellInfo, BellRegistration, PosSignIn, WakeUpChoice, WakeUpStatus, WakeUpStore};
pub use push::{
    PushFailure, PushFeed, PushState, PushSubscription, PushSubscriptions, endpoint, is_ding,
};
pub use wake_ups::{
    BellWakeUps, Distributor, Distributors, FIRST_RETRY, LAST_RETRY, MISSED_AFTER, PUSH_NO_KEYS,
    PUSH_OFFLINE, PUSH_REFUSED, PUSH_SILENT, PosSide, SUBSCRIBE_WAIT, TILL_FAILED, UNREGISTER_WAIT,
    about, problem_status, push_failed_status, unreachable_reason,
};
