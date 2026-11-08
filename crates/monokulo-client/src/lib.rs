//! Till's client for a store's own Monokulo (docs/design.md §2): connect
//! codes, the connected-apps HTTP API, the store's event stream, and the
//! small types the rest of the core shares ([`PointOfSale`], [`Clock`]).
//!
//! Ported from the Android app's `core` module (docs/ios-plan.md §3, phase 1).
//! Nothing here knows about a platform or about UniFFI: where the device has
//! to answer (how it writes money, whether Orbot is running), the crate asks
//! through a trait ([`MoneyFormat`], [`tor::TorProbe`]).

pub mod amounts;
pub mod api;
pub mod clock;
pub mod connect_code;
pub mod connection;
pub mod error;
pub mod order;
pub mod payment_uri;
pub mod point_of_sale;
pub mod problems;
pub mod proof;
pub mod sale;
mod sse;
pub mod store;
pub mod tor;

pub use amounts::MoneyFormat;
pub use api::{AppIdentity, MonokuloApi, StreamEvent};
pub use clock::{Clock, FixedClock, SystemClock};
pub use connect_code::ConnectCode;
pub use connection::Connection;
pub use error::MonokuloError;
pub use order::Order;
pub use point_of_sale::PointOfSale;
pub use sale::{SaleEvent, SalePhase};
pub use store::{AppConnection, Store, Wallet};
