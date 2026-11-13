//! Amounts in a currency's minor unit as a receipt's numbers (Android:
//! `Receipts.kt`).

#[cfg(any(feature = "square", feature = "clover"))]
use till_types::CurrencyCode;

/// How many digits `currency`'s minor unit has, as a receipt counts it:
/// [`CurrencyCode::iso_digits`] (the one ISO 4217 table), and 2 for any
/// other code (XMR too, as orf-codec counts it) or text that isn't a code
/// at all, which orf-codec refuses anyway.
#[cfg(any(feature = "square", feature = "clover"))]
pub(crate) fn minor_digits(currency: &str) -> u32 {
    CurrencyCode::new(currency)
        .ok()
        .and_then(|code| code.iso_digits())
        .unwrap_or(2)
}

/// `minor` with the decimal point `digits` places from the right, as the
/// receipt's number: the double nearest the exact decimal, as Java's
/// `BigDecimal.toDouble()` gives it.
#[cfg(any(feature = "square", feature = "clover"))]
pub(crate) fn major(minor: i64, digits: u32) -> f64 {
    scaled(minor as i128, digits)
}

/// `units` × 10^-`scale` as the nearest double.
#[cfg(any(feature = "square", feature = "clover"))]
pub(crate) fn scaled(units: i128, scale: u32) -> f64 {
    // Rust's float parsing rounds correctly, so going through the decimal
    // text gives the nearest double, where dividing by a power of ten
    // might not.
    format!("{units}e-{scale}").parse().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(feature = "square", feature = "clover"))]
    #[test]
    fn digits_are_the_shared_tables() {
        assert_eq!(minor_digits("KWD"), 3);
        assert_eq!(minor_digits("CLF"), 4);
        assert_eq!(minor_digits("jpy"), 0, "any case");
        assert_eq!(minor_digits("XMR"), 2);
    }

    #[cfg(any(feature = "square", feature = "clover"))]
    #[test]
    fn majors_are_the_nearest_double() {
        assert_eq!(major(1388, 2), 13.88);
        assert_eq!(scaled(8875, 5), 0.08875);
        assert_eq!(major(-125, 2), -1.25);
    }
}
