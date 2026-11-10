//! What every point of sale's receipts are made with (Android:
//! `Receipts.kt`, and the `BigDecimal` arithmetic `SquareReceipts` and
//! `CloverReceipts` did).

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// `text` on one line: runs of whitespace (Java's `\s`: space, tab, line
/// feed, vertical tab, form feed, carriage return) become one space, and
/// none is left at either end. The codec refuses line breaks in text.
pub(crate) fn one_line(text: &str) -> String {
    text.split([' ', '\t', '\n', '\u{b}', '\u{c}', '\r'])
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `at` in whole seconds, in the merchant's own time zone (UTC when it isn't
/// known or isn't one the device knows), as RFC 3339 with its offset:
/// `2026-10-22T09:06:41-07:00`, or `…Z` in UTC. The codec refuses
/// fractions of a second.
pub(crate) fn receipt_timestamp(at: Timestamp, timezone: Option<&str>) -> String {
    let zone = timezone
        .and_then(|name| TimeZone::get(name).ok())
        .unwrap_or(TimeZone::UTC);
    orf_codec::issue_timestamp(at, zone.to_offset(at))
}

/// The last `n` characters of `text`: an order's id cut to a receipt id.
pub(crate) fn take_last(text: &str, n: usize) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(n)).collect()
}

#[cfg(feature = "clover")]
/// `n` ÷ `d` rounded to the nearest whole number, halves away from zero
/// (Java's `RoundingMode.HALF_UP`). `d` must be positive.
pub(crate) fn div_half_up(n: i128, d: i128) -> i128 {
    let (q, r) = (n / d, n % d);
    if 2 * r.abs() >= d { q + n.signum() } else { q }
}

#[cfg(feature = "square")]
/// An exact decimal: `units` × 10^-`scale`. Square's quantities ("2",
/// "1.5") and tax percentages ("8.875") are read into one, so a receipt's
/// arithmetic is exact, as it was with `BigDecimal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Decimal {
    pub units: i128,
    pub scale: u32,
}

#[cfg(feature = "square")]
impl Decimal {
    pub const ONE: Decimal = Decimal { units: 1, scale: 0 };

    /// The most digits after the point kept: far beyond any quantity or
    /// rate, and well inside what `units` holds.
    const MAX_SCALE: u32 = 18;

    /// `text` as a decimal, as `BigDecimal(String)` reads one: a sign, digits
    /// with at most one point among or around them, then an exponent
    /// (`1.5`, `-2`, `.5`, `1E+2`). `None` for anything else, or for one too
    /// long to hold.
    pub fn parse(text: &str) -> Option<Decimal> {
        let (mantissa, exponent) = match text.find(['e', 'E']) {
            Some(at) => (&text[..at], text[at + 1..].parse::<i32>().ok()?),
            None => (text, 0),
        };
        let (negative, digits) = match mantissa.as_bytes().first()? {
            b'-' => (true, &mantissa[1..]),
            b'+' => (false, &mantissa[1..]),
            _ => (false, mantissa),
        };
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty() && fraction.is_empty()
            || !whole
                .bytes()
                .chain(fraction.bytes())
                .all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let mut units: i128 = 0;
        for b in whole.bytes().chain(fraction.bytes()) {
            units = units.checked_mul(10)?.checked_add(i128::from(b - b'0'))?;
        }
        let mut scale = fraction.len() as i64 - i64::from(exponent);
        if scale < 0 {
            units = units.checked_mul(10i128.checked_pow(u32::try_from(-scale).ok()?)?)?;
            scale = 0;
        }
        let mut scale = u32::try_from(scale).ok()?;
        // Trailing zeros past the point say nothing: drop them, so a long
        // "2.000…" still fits.
        while scale > 0 && units % 10 == 0 {
            units /= 10;
            scale -= 1;
        }
        if scale > Self::MAX_SCALE {
            return None;
        }
        Some(Decimal {
            units: if negative { -units } else { units },
            scale,
        })
    }

    /// The nearest double, as `BigDecimal.toDouble()` gives it.
    pub fn to_f64(self) -> f64 {
        crate::money::scaled(self.units, self.scale)
    }

    /// The whole part, the fraction dropped (`BigDecimal.toLong()`).
    pub fn trunc(self) -> i64 {
        let whole = self.units / 10i128.pow(self.scale);
        whole.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }

    /// This, moved `places` digits to the right of the point: a percentage
    /// as a fraction is `shifted(2)`.
    pub fn shifted(self, places: u32) -> Decimal {
        Decimal {
            units: self.units,
            scale: self.scale + places,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_joins_java_whitespace_only() {
        assert_eq!(one_line("  Ana\nLópez "), "Ana López");
        assert_eq!(one_line("a\t\r\n\u{b}\u{c}b"), "a b");
        assert_eq!(one_line("a\u{a0}b"), "a\u{a0}b", "Java's \\s is ASCII only");
        assert_eq!(one_line(" \n "), "");
    }

    #[test]
    fn receipt_times_are_whole_seconds_in_the_merchants_zone() {
        let at: Timestamp = "2026-10-22T16:06:41.877Z".parse().unwrap();
        assert_eq!(
            receipt_timestamp(at, Some("America/Los_Angeles")),
            "2026-10-22T09:06:41-07:00"
        );
        assert_eq!(receipt_timestamp(at, None), "2026-10-22T16:06:41Z");
        assert_eq!(
            receipt_timestamp(at, Some("Mars/Olympus_Mons")),
            "2026-10-22T16:06:41Z",
            "a zone the device doesn't know is UTC"
        );
        assert_eq!(
            receipt_timestamp(at, Some("Asia/Kolkata")),
            "2026-10-22T21:36:41+05:30"
        );
    }

    #[cfg(feature = "square")]
    #[test]
    fn decimals_read_as_big_decimal_reads_them() {
        let d = |s| Decimal::parse(s);
        assert_eq!(d("2"), Some(Decimal { units: 2, scale: 0 }));
        assert_eq!(
            d("1.50"),
            Some(Decimal {
                units: 15,
                scale: 1
            })
        );
        assert_eq!(
            d("-.5"),
            Some(Decimal {
                units: -5,
                scale: 1
            })
        );
        assert_eq!(d("5."), Some(Decimal { units: 5, scale: 0 }));
        assert_eq!(
            d("1E+2"),
            Some(Decimal {
                units: 100,
                scale: 0
            })
        );
        assert_eq!(
            d("8.875"),
            Some(Decimal {
                units: 8875,
                scale: 3
            })
        );
        for bad in ["", ".", "-", "1.2.3", "1,5", "x", "1e", " 1", "NaN"] {
            assert_eq!(d(bad), None, "{bad:?}");
        }
        assert_eq!(d("2.5").unwrap().trunc(), 2);
        assert_eq!(d("-2.5").unwrap().trunc(), -2);
        assert_eq!(d("8.875").unwrap().to_f64(), 8.875);
    }

    #[cfg(feature = "clover")]
    #[test]
    fn halves_round_away_from_zero() {
        assert_eq!(div_half_up(5, 2), 3);
        assert_eq!(div_half_up(-5, 2), -3);
        assert_eq!(div_half_up(4, 3), 1);
        assert_eq!(div_half_up(888_888, 100), 8889);
    }

    #[test]
    fn take_last_counts_characters() {
        assert_eq!(take_last("ORDER_CL_1", 8), "DER_CL_1");
        assert_eq!(take_last("abc", 8), "abc");
    }
}
