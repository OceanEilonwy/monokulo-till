//! A receipt's numbers written as the Android app's `BigDecimal`s wrote
//! them: a double read by its shortest digits (`BigDecimal.valueOf`), then
//! rounded half up or stripped of its trailing zeros. Exact, in integers.

/// A decimal number: `digits` × 10^-`scale`, negative when `negative`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Decimal {
    negative: bool,
    digits: i128,
    scale: u32,
}

impl Decimal {
    /// `value` by its shortest digits, as Java's `BigDecimal.valueOf` reads
    /// a double. `None` for a value too large to count, or not a number.
    pub(crate) fn of(value: f64) -> Option<Decimal> {
        if !value.is_finite() {
            return None;
        }
        // Rust writes a double's shortest round-trip digits, never in
        // scientific notation.
        let text = format!("{value}");
        let (negative, rest) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.as_str()),
        };
        let (whole, frac) = rest.split_once('.').unwrap_or((rest, ""));
        let mut digits: i128 = 0;
        for b in whole.bytes().chain(frac.bytes()) {
            digits = digits.checked_mul(10)?.checked_add(i128::from(b - b'0'))?;
        }
        Some(Decimal {
            negative: negative && digits != 0,
            digits,
            scale: u32::try_from(frac.len()).ok()?,
        })
    }

    /// Rounded to `scale` digits after the point, half away from zero
    /// (Java's `HALF_UP`).
    pub(crate) fn round(self, scale: u32) -> Option<Decimal> {
        let digits = if scale >= self.scale {
            self.digits
                .checked_mul(10i128.checked_pow(scale - self.scale)?)?
        } else {
            let by = 10i128.checked_pow(self.scale - scale)?;
            let (q, r) = (self.digits / by, self.digits % by);
            if r * 2 >= by { q + 1 } else { q }
        };
        Some(Decimal {
            negative: self.negative && digits != 0,
            digits,
            scale,
        })
    }

    /// Times 10^`places` (Java's `movePointRight`).
    pub(crate) fn times_ten_to(self, places: u32) -> Option<Decimal> {
        Some(if self.scale >= places {
            Decimal {
                scale: self.scale - places,
                ..self
            }
        } else {
            Decimal {
                digits: self
                    .digits
                    .checked_mul(10i128.checked_pow(places - self.scale)?)?,
                scale: 0,
                ..self
            }
        })
    }

    /// Without trailing zeros after the point.
    pub(crate) fn stripped(mut self) -> Decimal {
        while self.scale > 0 && self.digits % 10 == 0 {
            self.digits /= 10;
            self.scale -= 1;
        }
        self
    }

    fn signed(self) -> i128 {
        if self.negative {
            -self.digits
        } else {
            self.digits
        }
    }

    /// The sum of `self` and `other`, exactly.
    pub(crate) fn plus(self, other: Decimal) -> Option<Decimal> {
        let scale = self.scale.max(other.scale);
        let (a, b) = (self.round(scale)?, other.round(scale)?);
        let sum = a.signed().checked_add(b.signed())?;
        Some(Decimal {
            negative: sum < 0,
            digits: sum.abs(),
            scale,
        })
    }

    /// As a double again.
    pub(crate) fn to_f64(self) -> f64 {
        self.plain().parse().unwrap_or(0.0)
    }

    /// Written out in full, with `scale` digits after the point (Java's
    /// `toPlainString`).
    pub(crate) fn plain(self) -> String {
        let digits = self.digits.to_string();
        let scale = self.scale as usize;
        let body = if scale == 0 {
            digits
        } else if digits.len() > scale {
            let (whole, frac) = digits.split_at(digits.len() - scale);
            format!("{whole}.{frac}")
        } else {
            format!("0.{}{digits}", "0".repeat(scale - digits.len()))
        };
        if self.negative {
            format!("-{body}")
        } else {
            body
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(value: f64) -> Decimal {
        Decimal::of(value).unwrap()
    }

    #[test]
    fn doubles_round_and_strip_as_big_decimals_do() {
        assert_eq!(d(9.0).round(2).unwrap().plain(), "9.00");
        assert_eq!(d(1125.0).round(0).unwrap().plain(), "1125");
        assert_eq!(d(0.005).round(2).unwrap().plain(), "0.01");
        assert_eq!(d(-1.5).round(0).unwrap().plain(), "-2");
        assert_eq!(d(1.5).stripped().plain(), "1.5");
        assert_eq!(d(2.0).stripped().plain(), "2");
        assert_eq!(d(0.1).times_ten_to(2).unwrap().plain(), "10");
        assert_eq!(d(0.08875).times_ten_to(2).unwrap().plain(), "8.875");
        assert_eq!(d(0.5).plus(d(0.63)).unwrap().plain(), "1.13");
        assert!(Decimal::of(f64::NAN).is_none());
        assert!(Decimal::of(1e300).is_none());
    }
}
