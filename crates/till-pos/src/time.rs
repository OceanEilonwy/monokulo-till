//! Instants as the points of sale and the stores write them: RFC 3339 in
//! UTC, as Java's `Instant.toString()` wrote them, so cursors and expiries
//! saved by the Android app read back the same.

use jiff::Timestamp;
use jiff::fmt::temporal::DateTimePrinter;

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
}
