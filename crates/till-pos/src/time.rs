//! Instants as the points of sale and the stores write them: RFC 3339 in
//! UTC, as Java's `Instant.toString()` wrote them, so cursors and expiries
//! saved by the Android app read back the same.

use jiff::Timestamp;
use jiff::fmt::temporal::DateTimePrinter;

/// The Unix seconds of an RFC 3339 instant with any offset (a sale's
/// [`crate::FoundSale::created_at`], a saved cursor), any fraction dropped
/// toward zero (Java's `getEpochSecond` floored, which differs only before
/// 1970); `None` when it isn't one.
pub fn unix_seconds(text: &str) -> Option<i64> {
    parse(text).map(|at| at.as_second())
}

/// The Unix milliseconds of an RFC 3339 instant with any offset (a
/// payment's `updated_at`), any finer fraction dropped; `None` when it
/// isn't one.
pub fn unix_millis(text: &str) -> Option<i64> {
    parse(text).map(|at| at.as_millisecond())
}

/// The instant `seconds` after the Unix epoch, written as Java's
/// `Instant.toString()` wrote it (`2026-10-22T16:07:00Z`), clamped to what
/// an RFC 3339 instant can say.
pub fn instant(seconds: i64) -> String {
    format(from_seconds(seconds))
}

/// An RFC 3339 instant, with any offset; `None` when it isn't one.
pub(crate) fn parse(text: &str) -> Option<Timestamp> {
    text.parse().ok()
}

/// The instant `seconds` after the Unix epoch, clamped to what an RFC 3339
/// instant can say.
pub(crate) fn from_seconds(seconds: i64) -> Timestamp {
    let seconds = seconds.clamp(Timestamp::MIN.as_second(), Timestamp::MAX.as_second());
    Timestamp::from_second(seconds).unwrap_or(Timestamp::UNIX_EPOCH)
}

/// The instant `millis` after the Unix epoch, clamped likewise.
#[cfg(feature = "clover")]
pub(crate) fn from_millis(millis: i64) -> Timestamp {
    let millis = millis.clamp(
        Timestamp::MIN.as_millisecond(),
        Timestamp::MAX.as_millisecond(),
    );
    Timestamp::from_millisecond(millis).unwrap_or(Timestamp::UNIX_EPOCH)
}

/// `at` in UTC, as `Instant.toString()` wrote it: `2026-10-22T16:07:00Z`
/// with no fraction when there is none, else milliseconds, microseconds or
/// nanoseconds, whichever is the fewest that say it exactly
/// (`2026-10-22T16:07:00.512Z`).
pub(crate) fn format(at: Timestamp) -> String {
    let nanos = at.subsec_nanosecond().unsigned_abs();
    let precision = if nanos == 0 {
        0
    } else if nanos.is_multiple_of(1_000_000) {
        3
    } else if nanos.is_multiple_of(1_000) {
        6
    } else {
        9
    };
    DateTimePrinter::new()
        .precision(Some(precision))
        .timestamp_to_string(&at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instants_are_written_as_java_wrote_them() {
        let at = |s: &str| format(parse(s).unwrap());
        assert_eq!(at("2026-10-22T16:07:00.512Z"), "2026-10-22T16:07:00.512Z");
        assert_eq!(at("2026-10-22T16:07:00.5Z"), "2026-10-22T16:07:00.500Z");
        assert_eq!(at("2026-10-22T16:07:00.000Z"), "2026-10-22T16:07:00Z");
        assert_eq!(
            at("2026-10-22T16:07:00.000001Z"),
            "2026-10-22T16:07:00.000001Z"
        );
        assert_eq!(at("2026-10-22T09:07:00-07:00"), "2026-10-22T16:07:00Z");
        assert_eq!(format(from_seconds(1_761_600_000)), "2025-10-27T21:20:00Z");
        assert!(parse("yesterday").is_none());
    }

    #[test]
    fn instants_read_and_write_as_unix_seconds() {
        assert_eq!(unix_seconds("2025-10-27T21:20:00Z"), Some(1_761_600_000));
        assert_eq!(
            unix_seconds("2025-10-27T14:20:00.999-07:00"),
            Some(1_761_600_000)
        );
        assert_eq!(unix_seconds("1969-12-31T23:59:59.5Z"), Some(0));
        assert_eq!(unix_seconds("yesterday"), None);
        assert_eq!(instant(1_761_600_000), "2025-10-27T21:20:00Z");
        assert_eq!(instant(i64::MAX), "9999-12-30T22:00:00Z");
    }
}
