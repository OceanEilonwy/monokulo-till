//! The sales this till has alerted (core.md decision 123): one new-sale
//! alert per sale per till, whichever path reached the till first and
//! whichever process posted it.
//!
//! The record is kept through [`Platform`](crate::Platform)'s secrets, so on
//! iOS the app and its Notification Service Extension read and write the
//! same one (the shared Keychain group), and it outlives a restart. A claim
//! reads, checks and writes it under an exclusive `flock` on a file in the
//! data directory, as the sign-in's refresh does (decision 13), so two
//! processes, or two extension instances for two dings of one payment,
//! never both win the same sale. What this process has claimed is also
//! held in memory, so a store that can't answer never makes it alert twice.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Mutex;

use till_pos::TokenLock;

use crate::side::FileLock;
use crate::store::{Store, keys};

/// Which sales have been alerted here.
pub(crate) struct Alerted {
    store: Store,
    lock: FileLock,
    claimed: Mutex<BTreeSet<String>>,
}

impl Alerted {
    /// The most order ids remembered, the newest kept: a shift keeps at
    /// most `Till::MAX_SALES` sales, so an older id can't come back.
    pub const KEPT: usize = till_sales::Till::MAX_SALES;

    /// The record in `store`, its lock beside the data in `data_dir`.
    pub fn new(store: Store, data_dir: &str) -> Alerted {
        Alerted {
            store,
            lock: FileLock::new(PathBuf::from(data_dir).join("alerts.lock")),
            claimed: Mutex::new(BTreeSet::new()),
        }
    }

    /// Claims the alert for each of `order_ids`, in order: answers those no
    /// one had claimed before, here or in another process, which are now
    /// claimed. A record the store couldn't read is left as it was (the
    /// claim is held in memory alone), rather than written over.
    pub fn claim(&self, order_ids: &[String]) -> Vec<String> {
        if order_ids.is_empty() {
            return Vec::new();
        }
        let mut claimed = self.claimed.lock().unwrap_or_else(|e| e.into_inner());
        // Without the lock file (no data directory) the claim is still made.
        let locked = self.lock.acquire().is_ok();
        let kept = self.read();
        let mut record = kept.clone().unwrap_or_default();
        let mut won = Vec::new();
        for id in order_ids {
            if claimed.contains(id) || record.contains(id) {
                continue;
            }
            claimed.insert(id.clone());
            record.push(id.clone());
            won.push(id.clone());
        }
        if !won.is_empty() && kept.is_some() {
            let excess = record.len().saturating_sub(Alerted::KEPT);
            record.drain(..excess);
            let json = serde_json::to_string(&record).expect("always JSON");
            let _ = self.store.put(keys::ALERTED, Some(&json));
        }
        if locked {
            self.lock.release();
        }
        won
    }

    /// The record as kept, oldest first; `None` when the store couldn't
    /// answer. Anything unreadable in it is an empty record.
    fn read(&self) -> Option<Vec<String>> {
        let saved = self.store.get(keys::ALERTED).ok()?;
        Some(
            saved
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::testing::FakePlatform;

    fn dir() -> String {
        let dir = std::env::temp_dir().join(format!(
            "till-alerted-{}-{}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace("::", "-")
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().into_owned()
    }

    fn ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn a_sale_is_claimed_once_across_two_processes_and_a_restart() {
        let platform = Arc::new(FakePlatform::default());
        let dir = dir();
        // The app and the extension: two records over one store.
        let app = Alerted::new(Store::new(platform.clone()), &dir);
        let extension = Alerted::new(Store::new(platform.clone()), &dir);
        assert_eq!(extension.claim(&ids(&["o1"])), ids(&["o1"]));
        assert_eq!(app.claim(&ids(&["o1", "o2"])), ids(&["o2"]));
        assert_eq!(extension.claim(&ids(&["o2"])), Vec::<String>::new());
        let restarted = Alerted::new(Store::new(platform.clone()), &dir);
        assert!(restarted.claim(&ids(&["o1", "o2"])).is_empty());
        assert_eq!(
            platform.get("alerts.posted").as_deref(),
            Some(r#"["o1","o2"]"#)
        );
    }

    #[test]
    fn claims_racing_on_threads_give_each_sale_to_one() {
        let platform = Arc::new(FakePlatform::default());
        let dir = dir();
        let won: Vec<Vec<String>> = (0..8)
            .map(|_| {
                let alerted = Alerted::new(Store::new(platform.clone()), &dir);
                std::thread::spawn(move || alerted.claim(&ids(&["o1", "o2", "o3"])))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect();
        let mut all: Vec<String> = won.into_iter().flatten().collect();
        all.sort();
        assert_eq!(all, ids(&["o1", "o2", "o3"]));
    }

    #[test]
    fn a_store_that_cant_answer_is_never_written_over_and_still_claims_once() {
        let platform = Arc::new(FakePlatform::default());
        platform.set("alerts.posted", r#"["o1"]"#);
        let alerted = Alerted::new(Store::new(platform.clone()), &dir());
        platform.fail(true);
        assert_eq!(alerted.claim(&ids(&["o2"])), ids(&["o2"]));
        assert!(alerted.claim(&ids(&["o2"])).is_empty(), "held in memory");
        platform.fail(false);
        assert_eq!(platform.get("alerts.posted").as_deref(), Some(r#"["o1"]"#));
    }

    #[test]
    fn the_record_keeps_the_newest_ids() {
        let platform = Arc::new(FakePlatform::default());
        let old: Vec<String> = (0..Alerted::KEPT).map(|i| format!("old{i}")).collect();
        platform.set("alerts.posted", &serde_json::to_string(&old).unwrap());
        let alerted = Alerted::new(Store::new(platform.clone()), &dir());
        assert_eq!(alerted.claim(&ids(&["new"])), ids(&["new"]));
        let kept: Vec<String> =
            serde_json::from_str(&platform.get("alerts.posted").unwrap()).unwrap();
        assert_eq!(kept.len(), Alerted::KEPT);
        assert_eq!(kept.first().map(String::as_str), Some("old1"));
        assert_eq!(kept.last().map(String::as_str), Some("new"));
    }
}
