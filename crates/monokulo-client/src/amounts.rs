//! Amounts as people read them (Android: `core/Amounts.kt`).
//!
//! The device writes money, so `$24.50` follows its locale exactly as the
//! Android app always has (docs/design.md §6, decision 34) without the core
//! carrying locale data. What is left here is what doesn't depend on a
//! locale: a currency's minor units, and reading Monokulo's decimal strings
//! into them.

/// How the device writes an amount of money: `minor` units of `currency`
/// (an ISO 4217 code, upper case), so 2450 and "USD" is "$24.50" in the US.
/// The engine adapts the platform's `format_money` to this.
pub trait MoneyFormat {
    fn format(&self, minor: i64, currency: &str) -> String;
}

impl<F: Fn(i64, &str) -> String> MoneyFormat for F {
    fn format(&self, minor: i64, currency: &str) -> String {
        self(minor, currency)
    }
}

/// `amount` (a decimal string, as Monokulo sends it) in `currency`, as
/// `format` writes it, rounded half to even to the currency's minor unit.
/// A currency that isn't ISO 4217's ("XMR") or an amount that won't parse
/// is shown as it came, followed by the code in upper case: "4.5 XMR".
pub fn fiat(amount: &str, currency: &str, format: &dyn MoneyFormat) -> String {
    let code = currency.to_ascii_uppercase();
    match exponent(&code).and_then(|exp| minor_units(amount, exp)) {
        Some(minor) => format.format(minor, &code),
        None => format!("{amount} {code}"),
    }
}

/// How many digits `code`'s minor unit has (2 for USD, 0 for JPY, 3 for
/// KWD), or `None` for a code that isn't on ISO 4217's list. Codes with no
/// minor unit, the metals and funds, have 0. Only upper case counts.
pub fn exponent(code: &str) -> Option<u32> {
    if ZERO.contains(&code) {
        Some(0)
    } else if THREE.contains(&code) {
        Some(3)
    } else if FOUR.contains(&code) {
        Some(4)
    } else if TWO.contains(&code) {
        Some(2)
    } else {
        None
    }
}

/// `amount`, a decimal string ("24.50", "-3", "1200.6", "1.5E2"), as a
/// whole number of minor units with `exponent` digits, rounded half to even
/// as Java's number formats round: "1200.6" with 0 is 1201, "0.125" with 2
/// is 12. `None` for anything else, or an amount too large to count.
pub fn minor_units(amount: &str, exponent: u32) -> Option<i64> {
    let (negative, rest) = match amount.as_bytes().first()? {
        b'-' => (true, &amount[1..]),
        b'+' => (false, &amount[1..]),
        _ => (false, amount),
    };
    let (mantissa, power) = match rest.find(['e', 'E']) {
        Some(at) => (&rest[..at], rest[at + 1..].parse::<i32>().ok()?),
        None => (rest, 0),
    };
    let (whole, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    if !whole
        .bytes()
        .chain(frac.bytes())
        .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let digits: Vec<u8> = whole
        .bytes()
        .chain(frac.bytes())
        .map(|b| b - b'0')
        .collect();
    // Where the point falls in `digits` once scaled to minor units.
    let point = whole.len() as i64 + power as i64 + exponent as i64;
    let mut minor: i64 = 0;
    for i in 0..point.max(0) {
        let digit = digits.get(i as usize).copied().unwrap_or(0);
        minor = minor.checked_mul(10)?.checked_add(digit as i64)?;
    }
    // Half to even on what's dropped.
    let dropped: &[u8] = if point < 0 {
        // Below the first digit, all of it is smaller than half a unit.
        &[]
    } else {
        digits.get(point as usize..).unwrap_or(&[])
    };
    if let Some((&first, rest)) = dropped.split_first() {
        let above_half = first > 5 || (first == 5 && rest.iter().any(|&d| d != 0));
        let half = first == 5 && rest.iter().all(|&d| d == 0);
        if above_half || (half && minor % 2 == 1) {
            minor = minor.checked_add(1)?;
        }
    }
    Some(if negative { -minor } else { minor })
}

/// ISO 4217's current codes with no minor unit, and those it lists as N.A.
/// (the metals, funds and test codes), which Java gives -1 and Android
/// wrote with none.
const ZERO: &[&str] = &[
    "BIF", "CLP", "DJF", "GNF", "ISK", "JPY", "KMF", "KRW", "PYG", "RWF", "UGX", "UYI", "VND",
    "VUV", "XAF", "XOF", "XPF", // N.A.
    "XAG", "XAU", "XBA", "XBB", "XBC", "XBD", "XDR", "XPD", "XPT", "XSU", "XTS", "XUA", "XXX",
];

const THREE: &[&str] = &["BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND"];

const FOUR: &[&str] = &["CLF", "UYW"];

const TWO: &[&str] = &[
    "AED", "AFN", "ALL", "AMD", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM", "BBD", "BDT", "BGN",
    "BMD", "BND", "BOB", "BOV", "BRL", "BSD", "BTN", "BWP", "BYN", "BZD", "CAD", "CDF", "CHE",
    "CHF", "CHW", "CNY", "COP", "COU", "CRC", "CUP", "CVE", "CZK", "DKK", "DOP", "DZD", "EGP",
    "ERN", "ETB", "EUR", "FJD", "FKP", "GBP", "GEL", "GHS", "GIP", "GMD", "GTQ", "GYD", "HKD",
    "HNL", "HTG", "HUF", "IDR", "ILS", "INR", "IRR", "JMD", "KES", "KGS", "KHR", "KPW", "KYD",
    "KZT", "LAK", "LBP", "LKR", "LRD", "LSL", "MAD", "MDL", "MGA", "MKD", "MMK", "MNT", "MOP",
    "MRU", "MUR", "MVR", "MWK", "MXN", "MXV", "MYR", "MZN", "NAD", "NGN", "NIO", "NOK", "NPR",
    "NZD", "PAB", "PEN", "PGK", "PHP", "PKR", "PLN", "QAR", "RON", "RSD", "RUB", "SAR", "SBD",
    "SCR", "SDG", "SEK", "SGD", "SHP", "SLE", "SOS", "SRD", "SSP", "STN", "SVC", "SYP", "SZL",
    "THB", "TJS", "TMT", "TOP", "TRY", "TTD", "TWD", "TZS", "UAH", "USD", "USN", "UYU", "UZS",
    "VED", "VES", "WST", "XCD", "XCG", "YER", "ZAR", "ZMW", "ZWG",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_strings_read_as_minor_units() {
        assert_eq!(minor_units("24.50", 2), Some(2450));
        assert_eq!(minor_units("8", 2), Some(800));
        assert_eq!(minor_units("-3.1", 2), Some(-310));
        assert_eq!(minor_units(".5", 2), Some(50));
        assert_eq!(minor_units("1.5E2", 2), Some(15000));
        assert_eq!(minor_units("0.125", 2), Some(12), "half to even, down");
        assert_eq!(minor_units("0.135", 2), Some(14), "half to even, up");
        assert_eq!(minor_units("0.1251", 2), Some(13));
        assert_eq!(minor_units("0.004", 2), Some(0));
        assert_eq!(minor_units("1e-5", 2), Some(0));
        assert_eq!(minor_units("1.234", 3), Some(1234));
        for bad in [
            "",
            ".",
            "-",
            "lots",
            "1,5",
            "1.2.3",
            " 1",
            "1e",
            "99999999999999999999",
        ] {
            assert_eq!(minor_units(bad, 2), None, "{bad:?}");
        }
    }

    #[test]
    fn exponents_follow_iso_4217() {
        assert_eq!(exponent("USD"), Some(2));
        assert_eq!(exponent("JPY"), Some(0));
        assert_eq!(exponent("KWD"), Some(3));
        assert_eq!(exponent("CLF"), Some(4));
        assert_eq!(exponent("XAU"), Some(0));
        assert_eq!(exponent("XMR"), None);
        assert_eq!(exponent("usd"), None);
    }
}
