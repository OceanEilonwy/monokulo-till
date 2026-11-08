//! Monokular's client for a store's own Monokulo (docs/design.md §2): connect
//! codes, the connected-apps HTTP API, the store's event stream, and the
//! small types the rest of the core shares ([`PointOfSale`], [`Clock`]).
//!
//! Ported from the Android app's `core` module (docs/ios-plan.md §3, phase 1).
//! Nothing here knows about a platform or about UniFFI.

pub mod clock;
pub mod point_of_sale;

pub use clock::{Clock, FixedClock, SystemClock};
pub use point_of_sale::PointOfSale;
