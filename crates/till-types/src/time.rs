use serde::{Deserialize, Serialize};

/// A Unix time in whole seconds: how Monokulo, the points of sale and the
/// platforms give a time, and how the core's sales and screens hold one.
/// Serialized as the bare number, so kept JSON doesn't change.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct UnixSeconds(pub i64);

/// A Unix time in milliseconds: how the ledger keeps a time (its facts and
/// columns). Serialized as the bare number, and kept in SQLite as an
/// INTEGER.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct UnixMillis(pub i64);

impl UnixSeconds {
    /// The same time in milliseconds.
    pub fn to_millis(self) -> UnixMillis {
        UnixMillis(self.0.saturating_mul(1000))
    }

    /// `seconds` later (earlier when negative).
    pub fn plus(self, seconds: i64) -> UnixSeconds {
        UnixSeconds(self.0.saturating_add(seconds))
    }
}

impl UnixMillis {
    /// The whole second this falls in: rounded down, before 1970 too.
    pub fn to_seconds(self) -> UnixSeconds {
        UnixSeconds(self.0.div_euclid(1000))
    }

    /// `millis` later (earlier when negative).
    pub fn plus(self, millis: i64) -> UnixMillis {
        UnixMillis(self.0.saturating_add(millis))
    }
}

impl From<UnixSeconds> for UnixMillis {
    fn from(seconds: UnixSeconds) -> UnixMillis {
        seconds.to_millis()
    }
}

#[cfg(feature = "rusqlite")]
mod sql {
    use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};

    use super::UnixMillis;

    impl ToSql for UnixMillis {
        fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
            self.0.to_sql()
        }
    }

    impl FromSql for UnixMillis {
        fn column_result(value: ValueRef<'_>) -> FromSqlResult<UnixMillis> {
            i64::column_result(value).map(UnixMillis)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_and_millis_convert_only_on_purpose() {
        assert_eq!(
            UnixSeconds(1_794_247_200).to_millis(),
            UnixMillis(1_794_247_200_000)
        );
        assert_eq!(
            UnixMillis(1_794_247_200_999).to_seconds(),
            UnixSeconds(1_794_247_200)
        );
        assert_eq!(
            UnixMillis(-1).to_seconds(),
            UnixSeconds(-1),
            "down, before 1970"
        );
        assert_eq!(UnixMillis::from(UnixSeconds(2)), UnixMillis(2000));
        assert_eq!(UnixSeconds(i64::MAX).to_millis(), UnixMillis(i64::MAX));
        assert_eq!(UnixSeconds(10).plus(-3), UnixSeconds(7));
    }

    #[test]
    fn they_serialize_as_the_bare_number() {
        assert_eq!(serde_json::to_string(&UnixSeconds(5)).unwrap(), "5");
        assert_eq!(serde_json::to_string(&UnixMillis(5000)).unwrap(), "5000");
        let back: UnixMillis = serde_json::from_str("5000").unwrap();
        assert_eq!(back, UnixMillis(5000));
    }
}
