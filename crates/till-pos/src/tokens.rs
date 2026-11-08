//! A point of sale's sign-in, where it is kept, and the lock around its
//! refresh (Android: `PosTokens.kt`; the lock is new, docs/ios-plan.md §5).

#[cfg(any(feature = "square", feature = "clover"))]
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Something the app keeps for the till couldn't be read: the Keychain or
/// the Keystore didn't answer. Never the same as "nothing kept", which a
/// store says with `None` (decision 17 in docs/decisions/core.md).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct StoreError(pub String);

/// The till's own sign-in to its point of sale: one shape for Square's and
/// Clover's, so the app keeps either the same way. The JSON names are
/// Square's wire names, which sign-ins stored before Clover were written
/// with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosTokens {
    pub access_token: String,
    /// RFC 3339. Clover's Unix-seconds expiry is converted to it at sign-in.
    pub expires_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// The merchant the sign-in is for: Square's token answer says it,
    /// Clover's redirect does.
    pub merchant_id: String,
}

/// Where a sign-in is kept, encrypted: one per point of sale. The app
/// implements it (the Android Keystore, the iOS Keychain).
pub trait TokenStore: Send + Sync {
    /// The sign-in, or `None` when there is none. A store that can't tell
    /// says so with an error: taking that for "signed out" would send the
    /// cashier to sign in again, and could forget a good sign-in.
    fn load(&self) -> Result<Option<PosTokens>, StoreError>;
    /// Keeps `tokens` in place of what was kept; `None` forgets it.
    fn save(&self, tokens: Option<PosTokens>);
}

/// A lock that every process sharing a [`TokenStore`] holds around a
/// refresh, so only one of them spends a refresh token.
///
/// Square's and Clover's refresh tokens are single-use. On iOS the app and
/// its Notification Service Extension share one sign-in through the
/// Keychain, and may both find the access token due at the same moment: the
/// one whose refresh lands second would be signed out. So a refresh takes
/// this lock, reads the stored sign-in again, refreshes only if it is still
/// due, keeps the new tokens and lets the lock go. On iOS it is an exclusive
/// `flock` on a file in the App Group container; on Android, where one
/// process holds the sign-in, it is [`NoTokenLock`].
///
/// Within one process each sign-in also takes its own async mutex first,
/// so the lock only ever has one waiter per process.
pub trait TokenLock: Send + Sync {
    /// Waits until this process holds the lock. It may block: it is called
    /// off the async threads.
    fn acquire(&self) -> std::io::Result<()>;
    /// Lets the lock go. Called once after each `acquire` that succeeded,
    /// even when the request that wanted it was dropped meanwhile.
    fn release(&self);
}

/// The lock for a sign-in only one process uses: it never waits.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoTokenLock;

impl TokenLock for NoTokenLock {
    fn acquire(&self) -> std::io::Result<()> {
        Ok(())
    }

    fn release(&self) {}
}

/// A held [`TokenLock`], let go when dropped.
#[cfg(any(feature = "square", feature = "clover"))]
pub(crate) struct Held(Arc<dyn TokenLock>);

#[cfg(any(feature = "square", feature = "clover"))]
impl Drop for Held {
    fn drop(&mut self) {
        self.0.release();
    }
}

/// Waits for `lock` on a blocking thread. If the caller stops waiting, a
/// lock acquired after that is let go at once: the guard is what is sent
/// back, and a guard nobody receives is dropped.
#[cfg(any(feature = "square", feature = "clover"))]
pub(crate) async fn hold(lock: &Arc<dyn TokenLock>) -> Result<Held, crate::PosError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let lock = Arc::clone(lock);
    tokio::task::spawn_blocking(move || {
        let held = lock.acquire().map(|()| Held(Arc::clone(&lock)));
        let _ = sender.send(held);
    });
    match receiver.await {
        Ok(held) => held.map_err(|e| crate::PosError::Lock(e.to_string())),
        Err(_) => Err(crate::PosError::Lock("the lock's thread stopped".into())),
    }
}

/// Shared by every sign-in that doesn't name a lock.
#[cfg(any(feature = "square", feature = "clover"))]
pub(crate) fn no_lock() -> Arc<dyn TokenLock> {
    Arc::new(NoTokenLock)
}
