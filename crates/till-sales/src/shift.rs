//! The wake-up loop (Android: `Shift.kt`).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::{Instant, sleep_until};

use crate::Till;

/// Why the till checked the POS: what [`Shift::run`] tells its
/// `on_checked`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WakeCause {
    /// The shift starting: the first check, for whatever came in while
    /// nothing watched.
    Started,
    /// The shift's timer, at its pace.
    Timer,
    /// A ding from the Monokulo Bell (docs/design.md §3).
    Ding,
    /// The app came to the front.
    Opened,
}

/// How often to check without a wake-up, read on every tick.
pub type Interval = Arc<dyn Fn() -> Duration + Send + Sync>;

/// Told after every check: why it ran, and how many sales it found that no
/// check had found before.
pub type OnChecked = Arc<dyn Fn(WakeCause, usize) + Send + Sync>;

/// What makes the till check the POS (docs/design.md §3): a timer during a
/// shift, and anything that calls [`Shift::wake`] (the app opening, a ding
/// from a Monokulo Bell). Every wake-up does the same idempotent check.
pub struct Shift {
    till: Arc<Till>,
    interval: Interval,
    on_checked: OnChecked,
    /// One wake-up kept while a check runs, the latest: ten dings in a
    /// second make at most two checks (acceptance A8.37). With `woken`, a
    /// conflated channel.
    wakeup: Mutex<Option<WakeCause>>,
    woken: Notify,
}

impl Shift {
    /// The longest the timer waits before reading the interval again: the
    /// shortest pace.
    const TICK: Duration = Duration::from_secs(2);

    /// A shift checking `till` every `interval` (2 s alone, 30 s with a
    /// Bell; read on every tick, so a change takes effect within one), and
    /// telling `on_checked` after each check.
    pub fn new(till: Arc<Till>, interval: Interval, on_checked: OnChecked) -> Shift {
        Shift {
            till,
            interval,
            on_checked,
            wakeup: Mutex::new(None),
            woken: Notify::new(),
        }
    }

    /// The till this shift checks.
    pub fn till(&self) -> &Arc<Till> {
        &self.till
    }

    /// Check now, rather than at the next tick. While a check runs, the
    /// latest wake-up is kept for when it ends.
    pub fn wake(&self, cause: WakeCause) {
        *self.wakeup.lock().unwrap_or_else(|e| e.into_inner()) = Some(cause);
        self.woken.notify_one();
    }

    /// Checks until dropped: at once ([`WakeCause::Started`]), then on each
    /// wake-up or tick of the timer.
    pub async fn run(&self) {
        let mut cause = WakeCause::Started;
        loop {
            let found = self.till.check().await;
            (self.on_checked)(cause, found);
            cause = self.next().await;
        }
    }

    /// Waits for a wake-up or the timer. The wait goes in ticks of
    /// [`Shift::TICK`] at most, the interval read again at each, so the
    /// pace dropping from 30 s to 2 s mid-wait checks within a tick rather
    /// than at the end of the 30 s (acceptance A5.24). Counted in waits,
    /// not read off a clock, so tokio's paused clock drives it in tests. A
    /// wake-up landing as a tick ends is taken or left for the next wait,
    /// never taken and dropped.
    async fn next(&self) -> WakeCause {
        let mut waited = Duration::ZERO;
        loop {
            let interval = (self.interval)();
            if interval <= waited {
                return WakeCause::Timer;
            }
            let step = (interval - waited).min(Shift::TICK);
            let tick = Instant::now() + step;
            loop {
                tokio::select! {
                    biased;
                    () = self.woken.notified() => {
                        let taken = self.wakeup.lock().unwrap_or_else(|e| e.into_inner()).take();
                        if let Some(cause) = taken {
                            return cause;
                        }
                        // A notification whose wake-up an earlier one took: wait on.
                    }
                    () = sleep_until(tick) => break,
                }
            }
            waited += step;
        }
    }
}

#[cfg(test)]
mod tests {
    //! Ported from the Android app's `ShiftTest`, on tokio's paused clock:
    //! a sleep in the test is Kotlin's `advanceTimeBy`.

    use std::sync::atomic::{AtomicU64, Ordering};

    use async_trait::async_trait;
    use monokulo_client::{AppIdentity, Connection, MonokuloApi, PointOfSale, SystemClock};
    use till_pos::{FoundSale, PosError, RefundByTill, Refunds, SaleOrder, SaleSource};
    use tokio::task::JoinHandle;

    use super::*;

    /// A POS with nothing new that takes `takes` to say so.
    struct SlowSource {
        takes: Duration,
    }

    #[async_trait]
    impl SaleSource for SlowSource {
        fn point_of_sale(&self) -> PointOfSale {
            PointOfSale::Square
        }
        async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError> {
            if !self.takes.is_zero() {
                tokio::time::sleep(self.takes).await;
            }
            Ok(Vec::new())
        }
        async fn order(&self, _: &FoundSale) -> Result<SaleOrder, PosError> {
            Ok(SaleOrder::default())
        }
        fn refunds(&self) -> Refunds<'_> {
            Refunds::ByTill(self)
        }
        fn handled(&self, _: &FoundSale) {}
    }

    #[async_trait]
    impl RefundByTill for SlowSource {
        async fn refund(&self, _: &FoundSale) -> Result<Option<till_pos::ListedRefund>, PosError> {
            Ok(None)
        }
    }

    /// A till over `source`: with no sales found, Monokulo is never asked.
    fn till(source: SlowSource) -> Arc<Till> {
        let monokulo = MonokuloApi::new(
            reqwest::Client::new(),
            AppIdentity {
                slug: "till".into(),
                name: "Till".into(),
                version: "0.1.0".into(),
                platform: "android".into(),
                device_name: "Pixel".into(),
            },
            Arc::new(SystemClock),
        );
        Arc::new(Till::new(
            Arc::new(source),
            monokulo,
            Connection::new("ak_1", "http://127.0.0.1:1/api/v1"),
            Arc::new(crate::Books::new(
                crate::books::testing::TestHost::new(&tempfile::tempdir().unwrap().keep()),
                None,
            )),
        ))
    }

    type Checks = Arc<Mutex<Vec<(u128, WakeCause)>>>;

    /// Runs a shift in the background, recording each check's virtual time
    /// (ms) and cause.
    async fn shift(source: SlowSource, interval: Interval) -> (Arc<Shift>, Checks, JoinHandle<()>) {
        let base = Instant::now();
        let checks: Checks = Arc::default();
        let on_checked: OnChecked = {
            let checks = checks.clone();
            Arc::new(move |cause, _| {
                checks
                    .lock()
                    .unwrap()
                    .push((base.elapsed().as_millis(), cause))
            })
        };
        let shift = Arc::new(Shift::new(till(source), interval, on_checked));
        let running = tokio::spawn({
            let shift = shift.clone();
            async move { shift.run().await }
        });
        tokio::task::yield_now().await;
        (shift, checks, running)
    }

    fn quick() -> SlowSource {
        SlowSource {
            takes: Duration::ZERO,
        }
    }

    fn every(seconds: u64) -> Interval {
        Arc::new(move || Duration::from_secs(seconds))
    }

    async fn advance(ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    fn recorded(checks: &Checks) -> Vec<(u128, WakeCause)> {
        checks.lock().unwrap().clone()
    }

    #[tokio::test(start_paused = true)]
    async fn the_first_check_is_the_shift_starting_then_the_timer_at_its_pace() {
        let (_, checks, running) = shift(quick(), every(2)).await;
        advance(4_001).await;
        assert_eq!(
            recorded(&checks),
            [
                (0, WakeCause::Started),
                (2_000, WakeCause::Timer),
                (4_000, WakeCause::Timer)
            ]
        );
        running.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn a_pace_that_flips_from_2_s_to_30_s_and_back_mid_run_takes_effect_on_the_next_tick_without_a_restart()
     {
        let pace = Arc::new(AtomicU64::new(2));
        let interval: Interval = {
            let pace = pace.clone();
            Arc::new(move || Duration::from_secs(pace.load(Ordering::SeqCst)))
        };
        let (_, checks, running) = shift(quick(), interval).await;
        advance(5_000).await;
        pace.store(30, Ordering::SeqCst); // the Bell connected, during the wait that ends at 6 s
        advance(36_000).await; // to 41 s
        assert_eq!(
            recorded(&checks).iter().map(|c| c.0).collect::<Vec<_>>(),
            [0, 2_000, 4_000, 34_000],
            "30 s from the last check, not another 2 s tick"
        );
        pace.store(2, Ordering::SeqCst); // the Bell's wake-ups stopped, 7 s into a 30 s wait
        advance(1_001).await; // to 42 s: the next tick
        assert_eq!(
            recorded(&checks).last(),
            Some(&(42_000, WakeCause::Timer)),
            "back at 2 s within a tick, not at 64 s"
        );
        advance(2_000).await;
        assert_eq!(recorded(&checks).last().map(|c| c.0), Some(44_000));
        running.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn a_wake_checks_at_once_and_says_why_and_the_timer_starts_again_from_it() {
        let (shift, checks, running) = shift(quick(), every(30)).await;
        advance(10_000).await;
        shift.wake(WakeCause::Ding);
        tokio::task::yield_now().await;
        advance(5_000).await;
        shift.wake(WakeCause::Opened);
        tokio::task::yield_now().await;
        advance(30_001).await;
        assert_eq!(
            recorded(&checks),
            [
                (0, WakeCause::Started),
                (10_000, WakeCause::Ding),
                (15_000, WakeCause::Opened),
                (45_000, WakeCause::Timer)
            ]
        );
        running.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn ten_dings_in_a_second_make_at_most_two_checks_one_in_flight_and_one_queued() {
        let slow = SlowSource {
            takes: Duration::from_millis(300),
        };
        let (shift, checks, running) = shift(slow, every(30)).await;
        advance(1_000).await;
        for _ in 0..10 {
            shift.wake(WakeCause::Ding);
            advance(10).await; // all ten inside the first one's check
        }
        advance(10_000).await;
        assert_eq!(
            recorded(&checks).iter().map(|c| c.1).collect::<Vec<_>>(),
            [WakeCause::Started, WakeCause::Ding, WakeCause::Ding]
        );
        running.abort();
    }
}
