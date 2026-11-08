//! Till's points of sale (docs/design.md §1): signing in to Square and
//! Clover with PKCE, finding the Monero sales they take, refunding them, and
//! turning their orders into ORF receipts.
//!
//! Ported from the Android app's `sales` module (docs/ios-plan.md §3,
//! phase 2). Nothing here knows about a platform or about UniFFI.
//!
//! - [`SaleSource`]: what the till asks of a point of sale, with
//!   [`Refunds`] saying who refunds a sale never paid in Monero.
//! - `square` and `clover`, each behind the cargo feature of the same name
//!   (both on by default): the sign-in, the API, the source and the
//!   receipts.
//! - [`PosTokens`], [`TokenStore`] and [`CursorStore`] (and Clover's
//!   `TenderStore`): what the app keeps for a point of sale, behind traits
//!   it implements. [`TokenLock`] is held around every refresh, so two
//!   processes sharing one sign-in (the iOS app and its notification
//!   extension) never both spend a single-use refresh token. A store that
//!   can't be read says so with a [`StoreError`], and nothing is saved over
//!   what it couldn't read.
//! - [`PosError`]: every failure, with what the till shows for it.
//!
//! Requests go through a `reqwest::Client` the caller makes, and the time
//! comes from a [`Clock`], so tests can hold it still.

#[cfg(feature = "clover")]
pub mod clover;
mod error;
#[cfg(any(feature = "square", feature = "clover"))]
mod http;
mod money;
mod pkce;
#[cfg(any(feature = "square", feature = "clover"))]
mod receipts;
mod source;
#[cfg(feature = "square")]
pub mod square;
pub mod time;
mod tokens;

pub use error::PosError;
pub use money::decimal_amount;
pub use monokulo_client::{Clock, PointOfSale};
pub use pkce::Pkce;
pub use source::{
    CursorStore, FoundSale, RefundByTill, RefundInPos, Refunds, SaleOrder, SaleSource,
};
pub use tokens::{NoTokenLock, PosTokens, StoreError, TokenLock, TokenStore};
