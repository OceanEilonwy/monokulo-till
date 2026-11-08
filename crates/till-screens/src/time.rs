//! Times written as the till writes them. Only the device knows its time
//! zone and whether it keeps a 24-hour clock, so the engine passes those in
//! a [`DeviceClock`] with the time now.

/// The device's clock, as the platform reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DeviceClock {
    /// Unix seconds.
    pub now: i64,
    /// The device's offset from UTC now, in seconds ("+08:00" is 28800).
    pub utc_offset: i32,
    /// "15:58" rather than "3:58 PM".
    pub hours_24: bool,
}

/// The time of day at `at` (Unix seconds) on the device's clock, as
/// Android's short localised time wrote it in English: "3:58 PM", or
/// "15:58" on a 24-hour clock.
pub fn time(at: i64, clock: DeviceClock) -> String {
    let seconds = (at + i64::from(clock.utc_offset)).rem_euclid(86_400);
    let (hour, minute) = (seconds / 3600, seconds % 3600 / 60);
    if clock.hours_24 {
        format!("{hour:02}:{minute:02}")
    } else {
        let half = if hour < 12 { "AM" } else { "PM" };
        let twelve = if hour % 12 == 0 { 12 } else { hour % 12 };
        format!("{twelve}:{minute:02} {half}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(utc_offset: i32, hours_24: bool) -> DeviceClock {
        DeviceClock {
            now: 0,
            utc_offset,
            hours_24,
        }
    }

    #[test]
    fn a_time_is_written_on_the_devices_clock() {
        // 2026-10-28T15:58:00Z
        let ding = 1_793_203_080;
        assert_eq!(time(ding, clock(0, false)), "3:58 PM");
        assert_eq!(time(ding, clock(0, true)), "15:58");
        assert_eq!(time(ding, clock(8 * 3600, false)), "11:58 PM");
        assert_eq!(time(ding, clock(9 * 3600, false)), "12:58 AM");
        assert_eq!(time(ding, clock(9 * 3600, true)), "00:58");
        assert_eq!(time(ding, clock(-16 * 3600, false)), "11:58 PM");
        assert_eq!(time(ding - 4 * 3600, clock(0, false)), "11:58 AM");
        assert_eq!(time(ding - 3 * 3600, clock(0, false)), "12:58 PM");
    }
}
