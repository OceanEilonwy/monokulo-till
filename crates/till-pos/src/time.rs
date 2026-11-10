//! Instants as the points of sale and the stores write them: RFC 3339 in
//! UTC, as Java's `Instant.toString()` wrote them, so cursors and expiries
//! saved by the Android app read back the same.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use jiff::Timestamp;
use jiff::fmt::temporal::DateTimePrinter;
use serde::{Deserialize, Serialize};

/// An instant as a point of sale or a store writes it: RFC 3339 with any
/// offset. It keeps the text it was read from, and writes that back
/// (`Display`, serde), so a sale, a cursor or a journal entry reads and
/// writes the same text whatever it says; one made from a time
/// ([`Rfc3339Instant::from_second`], [`From<Timestamp>`]) is written as
/// Java's `Instant.toString()` wrote it, in UTC (`2026-10-22T16:07:00Z`),
/// which is what Square's `begin_time` and the Android app's stores were
/// given. Two are equal, and ordered, by the instant alone.
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rfc3339Instant {
    at: Timestamp,
    text: Box<str>,
}

/// Text that isn't an RFC 3339 instant.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} isn't an RFC 3339 time")]
pub struct NotAnInstant(pub String);

impl Rfc3339Instant {
    /// The instant `seconds` after the Unix epoch, clamped to what an RFC
    /// 3339 instant can say.
    pub fn from_second(seconds: i64) -> Rfc3339Instant {
        from_seconds(seconds).into()
    }

    /// The instant `millis` after the Unix epoch, clamped likewise.
    pub fn from_millisecond(millis: i64) -> Rfc3339Instant {
        let millis = millis.clamp(
            Timestamp::MIN.as_millisecond(),
            Timestamp::MAX.as_millisecond(),
        );
        Timestamp::from_millisecond(millis)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .into()
    }

    pub fn timestamp(&self) -> Timestamp {
        self.at
    }

    /// Unix seconds, any fraction dropped toward zero.
    pub fn as_second(&self) -> i64 {
        self.at.as_second()
    }

    /// Unix milliseconds, any finer fraction dropped.
    pub fn as_millisecond(&self) -> i64 {
        self.at.as_millisecond()
    }

    /// `seconds` earlier, or this instant when that can't be said.
    pub fn earlier_by(&self, seconds: i64) -> Rfc3339Instant {
        self.at
            .checked_sub(jiff::SignedDuration::from_secs(seconds))
            .map_or_else(|_| self.clone(), Rfc3339Instant::from)
    }

    /// The text, as read or as written.
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl From<Timestamp> for Rfc3339Instant {
    fn from(at: Timestamp) -> Rfc3339Instant {
        Rfc3339Instant {
            at,
            text: format(at).into(),
        }
    }
}

impl FromStr for Rfc3339Instant {
    type Err = NotAnInstant;

    fn from_str(text: &str) -> Result<Rfc3339Instant, NotAnInstant> {
        match parse(text) {
            Some(at) => Ok(Rfc3339Instant {
                at,
                text: text.into(),
            }),
            None => Err(NotAnInstant(text.to_owned())),
        }
    }
}

impl TryFrom<String> for Rfc3339Instant {
    type Error = NotAnInstant;

    fn try_from(text: String) -> Result<Rfc3339Instant, NotAnInstant> {
        text.parse()
    }
}

impl From<Rfc3339Instant> for String {
    fn from(instant: Rfc3339Instant) -> String {
        instant.text.into()
    }
}

impl fmt::Display for Rfc3339Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl fmt::Debug for Rfc3339Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rfc3339Instant({})", self.text)
    }
}

impl PartialEq for Rfc3339Instant {
    fn eq(&self, other: &Rfc3339Instant) -> bool {
        self.at == other.at
    }
}

impl Eq for Rfc3339Instant {}

impl PartialOrd for Rfc3339Instant {
    fn partial_cmp(&self, other: &Rfc3339Instant) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rfc3339Instant {
    fn cmp(&self, other: &Rfc3339Instant) -> Ordering {
        self.at.cmp(&other.at)
    }
}

impl Hash for Rfc3339Instant {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.at.hash(state);
    }
}

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
    fn an_instant_keeps_the_text_it_was_read_from() {
        for text in [
            "2026-10-22T16:07:00.000Z",
            "2026-10-22T09:07:00-07:00",
            "2026-10-22T16:07:00Z",
        ] {
            let at: Rfc3339Instant = text.parse().unwrap();
            assert_eq!(at.to_string(), text);
            assert_eq!(serde_json::to_string(&at).unwrap(), format!("\"{text}\""));
            assert_eq!(at, Rfc3339Instant::from_second(1_792_685_220));
        }
        assert_eq!(
            Rfc3339Instant::from_second(1_761_600_000).to_string(),
            "2025-10-27T21:20:00Z",
            "made from a time, as Java wrote it"
        );
        assert_eq!(
            Rfc3339Instant::from_millisecond(1_761_600_000_512).to_string(),
            "2025-10-27T21:20:00.512Z"
        );
        assert!("yesterday".parse::<Rfc3339Instant>().is_err());
        assert!(serde_json::from_str::<Rfc3339Instant>("\"yesterday\"").is_err());
        let at: Rfc3339Instant = "2026-10-22T16:07:00Z".parse().unwrap();
        assert_eq!(at.earlier_by(120).to_string(), "2026-10-22T16:05:00Z");
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
