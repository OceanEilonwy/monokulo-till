//! What the platforms tell the core about the process's life, and the one
//! thing the core asks of them around its writes, for the ledger
//! (docs/ledger.md §11, §14; docs/decisions/core.md 151, 152).
//!
//! Everything about the ledger is decided here in Rust: when to close and
//! reopen it, whether the extension's round changed anything. The platforms
//! only pass on what the OS alone knows ([`Engine::suspend`],
//! [`Engine::resume`], [`Engine::ledger_changed`]) and hold the process
//! awake while a write runs ([`BackgroundWork`], [`hold`]).

use std::sync::{Arc, RwLock};

use crate::Engine;

/// Keeps the process from being suspended while the core writes the ledger:
/// on iOS a background task assertion (`ProcessInfo.performExpiringActivity`,
/// in the app and the Notification Service Extension), since iOS kills a
/// suspended process that holds a lock on a database in the shared
/// container (§11, "0xdead10cc"). Android has no such rule and gives none.
///
/// Called from the core's own threads, never the main one; both calls must
/// return at once.
#[cfg_attr(feature = "uniffi", uniffi::export(with_foreign))]
pub trait BackgroundWork: Send + Sync {
    /// Starts holding the process awake for `reason`; the returned id ends
    /// it through [`BackgroundWork::end`].
    fn begin(&self, reason: String) -> u64;
    /// Ends the hold `id` that [`BackgroundWork::begin`] returned.
    fn end(&self, id: u64);
}

static BACKGROUND_WORK: RwLock<Option<Arc<dyn BackgroundWork>>> = RwLock::new(None);

/// Gives the core the platform's [`BackgroundWork`], once per process,
/// before the engine starts or `background_check` runs. Without one, [`hold`]
/// holds nothing.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn set_background_work(work: Arc<dyn BackgroundWork>) {
    if let Ok(mut slot) = BACKGROUND_WORK.write() {
        *slot = Some(work);
    }
}

/// Holds the process awake until dropped: the ledger takes one around each
/// write transaction, which is short and never spans a suspension.
#[must_use = "the hold ends when dropped"]
pub struct Hold {
    held: Option<(Arc<dyn BackgroundWork>, u64)>,
}

/// Starts a [`Hold`] for `reason`, through the platform's [`BackgroundWork`]
/// if it gave one.
pub fn hold(reason: &str) -> Hold {
    let work = BACKGROUND_WORK.read().ok().and_then(|slot| slot.clone());
    Hold {
        held: work.map(|work| {
            let id = work.begin(reason.to_owned());
            (work, id)
        }),
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        if let Some((work, id)) = self.held.take() {
            work.end(id);
        }
    }
}

// The engine's lifecycle calls (docs/ledger.md §11): the platforms call
// them where the OS tells them; the core decides what they mean.
#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Engine {
    /// The app is going to the background and may be suspended once this
    /// returns: the engine finishes any write under way and closes the
    /// ledger. Blocks until then, so the platform calls it off the main
    /// thread, inside a background hold. A later write reopens the ledger
    /// under its own [`hold`].
    pub fn suspend(&self) {
        let books = self.books.lock().ok().and_then(|slot| slot.clone());
        if let Some(books) = books {
            books.suspend();
        }
        let _ = self.tx.send(crate::engine::Msg::Suspended);
    }

    /// The app is in front again (after [`Engine::suspend`]): the engine
    /// reopens the ledger and re-reads what the extension wrote meanwhile.
    /// Never blocks.
    pub fn resume(&self) {
        let _ = self.tx.send(crate::engine::Msg::LedgerChanged);
    }

    /// Another process (iOS's Notification Service Extension) finished a
    /// round that may have written the ledger: the engine re-reads its
    /// projections, claims what the extension left it, and publishes what
    /// changed. Never blocks.
    pub fn ledger_changed(&self) {
        let _ = self.tx.send(crate::engine::Msg::LedgerChanged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorded(Mutex<Vec<String>>);

    impl BackgroundWork for Recorded {
        fn begin(&self, reason: String) -> u64 {
            let mut calls = self.0.lock().unwrap();
            calls.push(format!("begin {reason}"));
            calls.len() as u64
        }
        fn end(&self, id: u64) {
            self.0.lock().unwrap().push(format!("end {id}"));
        }
    }

    #[test]
    fn a_hold_begins_and_ends_through_the_platform() {
        let work = Arc::new(Recorded::default());
        set_background_work(work.clone());
        {
            let _hold = hold("ledger write");
        }
        assert_eq!(
            *work.0.lock().unwrap(),
            vec!["begin ledger write".to_owned(), "end 1".to_owned()]
        );
    }
}
