//! The time, as the core reads it. The Android code took a `now: () -> Long`
//! lambda (Unix seconds) wherever it needed the time; here that is a
//! [`Clock`], so tests can hold the time still or move it on.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

pub use till_types::{UnixMillis, UnixSeconds};

/// The time now: the one clock every crate of the core reads, in the unit
/// each wants (decision 194). The ledger keeps milliseconds; Monokulo, the
/// points of sale and the screens whole seconds.
pub trait Clock: Send + Sync {
    /// Now, to the millisecond.
    fn now_ms(&self) -> UnixMillis;

    /// Now, in whole seconds.
    fn now(&self) -> UnixSeconds {
        self.now_ms().to_seconds()
    }
}

/// The device's clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> UnixMillis {
        UnixMillis(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        )
    }
}

/// A clock for tests, in whole seconds: it says what it was set to until
/// it is moved. Clones share the same time.
#[derive(Clone, Debug, Default)]
pub struct FixedClock(Arc<AtomicI64>);

impl FixedClock {
    /// At `now`, Unix seconds.
    pub fn at(now: i64) -> FixedClock {
        FixedClock(Arc::new(AtomicI64::new(now)))
    }

    pub fn set(&self, now: i64) {
        self.0.store(now, Ordering::SeqCst);
    }

    pub fn advance(&self, seconds: i64) {
        self.0.fetch_add(seconds, Ordering::SeqCst);
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> UnixMillis {
        self.now().to_millis()
    }

    fn now(&self) -> UnixSeconds {
        UnixSeconds(self.0.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_clock_moves_only_when_told_and_its_clones_follow() {
        let clock = FixedClock::at(1_761_312_000);
        let other = clock.clone();
        assert_eq!(clock.now(), UnixSeconds(1_761_312_000));
        assert_eq!(clock.now_ms(), UnixMillis(1_761_312_000_000));
        clock.advance(30);
        assert_eq!(other.now(), UnixSeconds(1_761_312_030));
        other.set(5);
        assert_eq!(clock.now(), UnixSeconds(5));
    }

    #[test]
    fn the_system_clock_is_after_2026() {
        assert!(SystemClock.now() > UnixSeconds(1_767_225_600));
    }
}
