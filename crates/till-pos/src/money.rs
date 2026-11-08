//! Amounts in a currency's minor unit (Android: `Receipts.kt`).

/// How many digits `currency`'s minor unit has: 2 for USD, 0 for JPY, 3 for
/// KWD (ISO 4217). A currency this table doesn't know has 2, as Java's
/// `Currency` answered for one it didn't know.
pub(crate) fn minor_digits(currency: &str) -> u32 {
    match currency {
        // The currencies orf-codec encodes with no minor unit, and the
        // funds and metals ISO 4217 gives none.
        "BIF" | "CLP" | "DJF" | "GNF" | "ISK" | "JPY" | "KMF" | "KRW" | "PYG" | "RWF" | "UGX"
        | "UYI" | "VND" | "VUV" | "XAF" | "XOF" | "XPF" | "XAG" | "XAU" | "XBA" | "XBB" | "XBC"
        | "XBD" | "XDR" | "XPD" | "XPT" | "XSU" | "XTS" | "XUA" | "XXX" => 0,
        "BHD" | "IQD" | "JOD" | "KWD" | "LYD" | "OMR" | "TND" => 3,
        "CLF" | "UYW" => 4,
        _ => 2,
    }
}

/// `minor` units of `currency` as a decimal string in the currency's own
/// digits: 2450 USD is "24.50", 1200 JPY is "1200", 5 EUR is "0.05". This is
/// the amount the till asks Monokulo for.
pub fn decimal_amount(minor: i64, currency: &str) -> String {
    decimal(minor, minor_digits(currency))
}

/// `minor` with the decimal point `digits` places from the right.
pub(crate) fn decimal(minor: i64, digits: u32) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let magnitude = minor.unsigned_abs().to_string();
    let digits = digits as usize;
    if digits == 0 {
        return format!("{sign}{magnitude}");
    }
    let padded = format!("{magnitude:0>width$}", width = digits + 1);
    let (whole, fraction) = padded.split_at(padded.len() - digits);
    format!("{sign}{whole}.{fraction}")
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

    #[test]
    fn decimals_pad_and_sign() {
        assert_eq!(decimal(5, 2), "0.05");
        assert_eq!(decimal(-5, 2), "-0.05");
        assert_eq!(decimal(123_456, 3), "123.456");
        assert_eq!(decimal(0, 2), "0.00");
        assert_eq!(decimal(i64::MIN, 0), "-9223372036854775808");
        assert_eq!(decimal_amount(1000, "KWD"), "1.000");
    }

    #[cfg(any(feature = "square", feature = "clover"))]
    #[test]
    fn majors_are_the_nearest_double() {
        assert_eq!(major(1388, 2), 13.88);
        assert_eq!(scaled(8875, 5), 0.08875);
        assert_eq!(major(-125, 2), -1.25);
    }
}
