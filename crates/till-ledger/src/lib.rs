//! The till's ledger ([docs/ledger.md](../../../docs/ledger.md)): Till's
//! sales as facts in one encrypted SQLCipher journal, with per-sale
//! projections derived from it, shared by the app and the iOS Notification
//! Service Extension. No network, no platform code.
//!
//! - [`ids`]: identities as newtypes ([`SaleKey`], [`OrderKey`], ...).
//! - [`facts`]: what the sources said and what this device did ([`Fact`]).
//! - [`model`]: [`SharedSale`], [`LocalMarks`] and the pure merges.
//! - [`view`]: [`SaleView`], everything a screen needs about a sale.
//! - [`effects`]: [`diff`], the alerts a change asks for.
//! - [`Ledger`]: the database, the write path, claims, queries,
//!   compaction, retention, rebuilds and the shift's import.
//! - [`shift`]: the shift's window and totals.

pub mod db;
pub mod effects;
pub mod facts;
pub mod ids;
mod import;
mod ledger;
pub mod model;
pub mod shift;
pub mod view;

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

pub use db::{KEY_SECRET, KeyMaterial, SecretStore};
pub use effects::{DiffContext, Effect, diff};
pub use facts::{
    Action, AlertKind, AlertState, Fact, MarkKind, OrderCreated, OrderObserved, OrderRead, Outcome,
    PaymentSeen, Process, Quote, RefundSeen, RefundStatus, ShiftMark, Tender,
};
pub use ids::{DeviceId, OrderId, OrderKey, PaymentId, SaleKey, Scope};
pub use ledger::{
    AlertRow, COMPACT_EVERY_MS, Claim, ClaimPolicy, DEFAULT_RETENTION_DAYS, Imported, JournalRow,
    Ledger, Opened, REDELIVER_AFTER_MS, Recorded, SaleRow,
};
pub use model::{LocalMarks, PaymentPhase, SaleOrder, SharedSale};
pub use shift::{ShiftSummary, ShiftWindow};
pub use view::{Code, SaleView, ViewSettings, may_make_order, view};

/// Why the ledger couldn't do something.
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("file: {0}")]
    Io(#[from] std::io::Error),
    /// The SQLite linked has no SQLCipher (`PRAGMA cipher_version` said
    /// nothing): system SQLite got linked. The ledger refuses to run.
    #[error("the database library has no encryption (SQLCipher missing)")]
    NoCipher,
    /// The key doesn't open the database.
    #[error("the key doesn't open the database")]
    WrongKey,
    /// The secret store couldn't say (before first unlock, say): the round
    /// fails and nothing changes.
    #[error("the database key can't be read: {0}")]
    KeyUnavailable(String),
    #[error("no random bytes: {0}")]
    Random(String),
    /// A newer build made the database.
    #[error("the database is from a newer version (schema {0})")]
    NewerSchema(i64),
    /// A fact changed a projection but identical content was journalled
    /// already: nothing is written.
    #[error("the fact is already in the journal")]
    Conflict,
    #[error("the kept shift can't be read: {0}")]
    Import(String),
}

/// The core's one clock (decision 217): the ledger reads it in
/// milliseconds.
pub use monokulo_client::{Clock, SystemClock, UnixMillis, UnixSeconds};

/// A clock for tests, in milliseconds: it says what it was set to. Clones
/// share the time.
#[derive(Clone, Debug, Default)]
pub struct TestClock(Arc<AtomicI64>);

impl TestClock {
    pub fn at(ms: i64) -> TestClock {
        TestClock(Arc::new(AtomicI64::new(ms)))
    }

    pub fn set(&self, ms: i64) {
        self.0.store(ms, Ordering::SeqCst);
    }

    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now_ms(&self) -> UnixMillis {
        UnixMillis(self.0.load(Ordering::SeqCst))
    }
}
