//! The time, as the core reads it. The Android code took a `now: () -> Long`
//! lambda (Unix seconds) wherever it needed the time; here that is a
//! [`Clock`], so tests can hold the time still or move it on.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

/// Unix time in whole seconds, as Monokulo and the points of sale give it.
pub trait Clock: Send + Sync {
    fn now(&self) -> i64;
}

/// The device's clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

/// A clock for tests: it says what it was set to until it is moved.
/// Clones share the same time.
#[derive(Clone, Debug, Default)]
pub struct FixedClock(Arc<AtomicI64>);

impl FixedClock {
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
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_clock_moves_only_when_told_and_its_clones_follow() {
        let clock = FixedClock::at(1_761_312_000);
        let other = clock.clone();
        assert_eq!(clock.now(), 1_761_312_000);
        clock.advance(30);
        assert_eq!(other.now(), 1_761_312_030);
        other.set(5);
        assert_eq!(clock.now(), 5);
    }

    #[test]
    fn the_system_clock_is_after_2026() {
        assert!(SystemClock.now() > 1_767_225_600);
    }
}
