//! Currencies and amounts: [`CurrencyCode`], with the one ISO 4217 table of
//! minor-unit digits every crate reads, and [`Money`], an amount in a
//! currency's minor unit that never travels without its currency.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A currency's code: three ASCII letters, kept in upper case ("USD").
/// Reading one trims it and upper-cases it, so `" usd"` is USD; anything
/// that isn't three letters isn't a code. A code needn't be on ISO 4217's
/// list ("XMR"): [`CurrencyCode::iso_digits`] says whether it is.
/// Serialized as its letters.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CurrencyCode([u8; 3]);

/// Text that isn't three letters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotACurrencyCode(pub String);

impl fmt::Display for NotACurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} isn't a currency code", self.0)
    }
}

impl std::error::Error for NotACurrencyCode {}

impl CurrencyCode {
    pub const USD: CurrencyCode = CurrencyCode(*b"USD");
    pub const EUR: CurrencyCode = CurrencyCode(*b"EUR");
    pub const GBP: CurrencyCode = CurrencyCode(*b"GBP");

    /// `text`, trimmed and upper-cased, when that's three ASCII letters.
    pub fn new(text: &str) -> Result<CurrencyCode, NotACurrencyCode> {
        match text.trim().as_bytes() {
            &[a, b, c] if [a, b, c].iter().all(u8::is_ascii_alphabetic) => Ok(CurrencyCode([
                a.to_ascii_uppercase(),
                b.to_ascii_uppercase(),
                c.to_ascii_uppercase(),
            ])),
            _ => Err(NotACurrencyCode(text.to_owned())),
        }
    }

    pub fn as_str(&self) -> &str {
        // Only ASCII letters are ever kept.
        std::str::from_utf8(&self.0).unwrap_or("XXX")
    }

    /// How many digits the currency's minor unit has on ISO 4217's list (2
    /// for USD, 0 for JPY, 3 for KWD, 4 for CLF); `None` for a code that
    /// isn't on it. Codes it lists with no minor unit (N.A.: the metals,
    /// funds and test codes) have 0, as Android wrote them.
    pub fn iso_digits(&self) -> Option<u32> {
        let code = self.as_str();
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

    /// How many digits an amount in the currency is counted in: ISO 4217's
    /// ([`iso_digits`](Self::iso_digits)), else 2, as Java's `Currency`
    /// answered for a code it didn't know. Every amount the till counts in
    /// minor units, and every receipt orf-codec encodes, uses this.
    pub fn minor_digits(&self) -> u32 {
        self.iso_digits().unwrap_or(2)
    }
}

impl fmt::Debug for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CurrencyCode({})", self.as_str())
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CurrencyCode {
    type Err = NotACurrencyCode;

    fn from_str(text: &str) -> Result<CurrencyCode, NotACurrencyCode> {
        CurrencyCode::new(text)
    }
}

impl TryFrom<String> for CurrencyCode {
    type Error = NotACurrencyCode;

    fn try_from(text: String) -> Result<CurrencyCode, NotACurrencyCode> {
        CurrencyCode::new(&text)
    }
}

impl TryFrom<&str> for CurrencyCode {
    type Error = NotACurrencyCode;

    fn try_from(text: &str) -> Result<CurrencyCode, NotACurrencyCode> {
        CurrencyCode::new(text)
    }
}

impl From<CurrencyCode> for String {
    fn from(code: CurrencyCode) -> String {
        code.as_str().to_owned()
    }
}

impl PartialEq<str> for CurrencyCode {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for CurrencyCode {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// An amount in a currency's minor unit: 2450 USD is $24.50.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Money {
    pub minor: i64,
    pub currency: CurrencyCode,
}

impl Money {
    pub fn new(minor: i64, currency: CurrencyCode) -> Money {
        Money { minor, currency }
    }

    /// The amount as a decimal in the currency's own digits: 2450 USD is
    /// "24.50", 1200 JPY is "1200", 5 EUR is "0.05". What Monokulo is asked
    /// for.
    pub fn decimal(&self) -> String {
        decimal(self.minor, self.currency.minor_digits())
    }

    /// `amount`, a decimal string ("24.50", "1.5E2"), in `currency`,
    /// rounded half to even to its minor unit ([`minor_units`]); `None`
    /// when it isn't a number.
    pub fn from_decimal(amount: &str, currency: CurrencyCode) -> Option<Money> {
        minor_units(amount, currency.minor_digits()).map(|minor| Money { minor, currency })
    }
}

/// A price as a service wrote it, a decimal and a currency (Monokulo's
/// `"amount": "24.50", "currency": "USD"`), read once into the [`Money`] it
/// is. It keeps both texts and writes them back as they came, so a kept
/// sale's or a journalled order's JSON doesn't change; a price whose text
/// isn't a decimal in a currency code still reads, with no money
/// (`money()` is `None`), and is shown as it came. Serialized as two
/// fields, `amount` and `currency`, for `#[serde(flatten)]` into the
/// struct it's in.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(from = "PriceText", into = "PriceText")]
pub struct Price {
    amount: String,
    currency: String,
    money: Option<Money>,
}

#[derive(Serialize, Deserialize)]
struct PriceText {
    amount: String,
    currency: String,
}

impl From<PriceText> for Price {
    fn from(text: PriceText) -> Price {
        Price::new(text.amount, text.currency)
    }
}

impl From<Price> for PriceText {
    fn from(price: Price) -> PriceText {
        PriceText {
            amount: price.amount,
            currency: price.currency,
        }
    }
}

impl Price {
    /// `amount` in `currency`, as written: its money when the amount is a
    /// decimal ([`Money::from_decimal`]) and the currency a code.
    pub fn new(amount: impl Into<String>, currency: impl Into<String>) -> Price {
        let (amount, currency) = (amount.into(), currency.into());
        let money = CurrencyCode::new(&currency)
            .ok()
            .and_then(|code| Money::from_decimal(&amount, code));
        Price {
            amount,
            currency,
            money,
        }
    }

    /// `money`, written in its currency's digits ("24.50", "USD").
    pub fn of(money: Money) -> Price {
        Price {
            amount: money.decimal(),
            currency: money.currency.to_string(),
            money: Some(money),
        }
    }

    /// The amount as written ("24.50").
    pub fn amount(&self) -> &str {
        &self.amount
    }

    /// The currency as written ("USD").
    pub fn currency(&self) -> &str {
        &self.currency
    }

    /// The money it is, when its texts read as money.
    pub fn money(&self) -> Option<Money> {
        self.money
    }
}

/// `minor` with the decimal point `digits` places from the right: 5 with 2
/// is "0.05", -5 is "-0.05".
pub fn decimal(minor: i64, digits: u32) -> String {
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

/// [`Money`] as two fields of the struct it's in, `amountMinor` and
/// `currency`, for `#[serde(flatten, with = "till_types::money::camel")]`:
/// the JSON a Kotlin `FoundSale` kept.
pub mod camel {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{CurrencyCode, Money};

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fields {
        amount_minor: i64,
        currency: CurrencyCode,
    }

    pub fn serialize<S: Serializer>(money: &Money, s: S) -> Result<S::Ok, S::Error> {
        Fields {
            amount_minor: money.minor,
            currency: money.currency,
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Money, D::Error> {
        let f = Fields::deserialize(d)?;
        Ok(Money::new(f.amount_minor, f.currency))
    }
}

/// [`Money`] as two fields of the struct it's in, `amount_minor` and
/// `currency`, for `#[serde(flatten, with = "till_types::money::snake")]`:
/// the ledger journal's.
pub mod snake {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{CurrencyCode, Money};

    #[derive(Serialize, Deserialize)]
    struct Fields {
        amount_minor: i64,
        currency: CurrencyCode,
    }

    pub fn serialize<S: Serializer>(money: &Money, s: S) -> Result<S::Ok, S::Error> {
        Fields {
            amount_minor: money.minor,
            currency: money.currency,
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Money, D::Error> {
        let f = Fields::deserialize(d)?;
        Ok(Money::new(f.amount_minor, f.currency))
    }
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

    fn code(text: &str) -> CurrencyCode {
        CurrencyCode::new(text).unwrap()
    }

    #[test]
    fn a_code_is_three_letters_trimmed_and_upper_cased() {
        assert_eq!(code("usd"), CurrencyCode::USD);
        assert_eq!(code(" Eur\n").as_str(), "EUR");
        assert_eq!(code("XMR").as_str(), "XMR");
        for bad in ["", "US", "USDT", "U$D", "12 3", "ÜSD"] {
            assert!(CurrencyCode::new(bad).is_err(), "{bad:?}");
        }
        assert_eq!(serde_json::to_string(&code("jpy")).unwrap(), "\"JPY\"");
        let read: CurrencyCode = serde_json::from_str("\"kwd\"").unwrap();
        assert_eq!(read.as_str(), "KWD");
        assert!(serde_json::from_str::<CurrencyCode>("\"dollars\"").is_err());
    }

    #[test]
    fn digits_follow_iso_4217_and_unknown_codes_count_in_two() {
        for (text, iso, minor) in [
            ("USD", Some(2), 2),
            ("JPY", Some(0), 0),
            ("jpy", Some(0), 0),
            ("KWD", Some(3), 3),
            ("CLF", Some(4), 4),
            ("UYW", Some(4), 4),
            ("XAU", Some(0), 0),
            ("XMR", None, 2),
        ] {
            assert_eq!(code(text).iso_digits(), iso, "{text}");
            assert_eq!(code(text).minor_digits(), minor, "{text}");
        }
    }

    #[test]
    fn money_reads_and_writes_decimals_in_its_currencys_digits() {
        let money = |minor, c| Money::new(minor, code(c));
        assert_eq!(money(2450, "USD").decimal(), "24.50");
        assert_eq!(money(1200, "JPY").decimal(), "1200");
        assert_eq!(money(1000, "KWD").decimal(), "1.000");
        assert_eq!(money(12345, "CLF").decimal(), "1.2345");
        assert_eq!(money(-5, "EUR").decimal(), "-0.05");
        assert_eq!(
            Money::from_decimal("24.5", CurrencyCode::USD),
            Some(money(2450, "USD"))
        );
        assert_eq!(
            Money::from_decimal("1200.6", code("JPY")),
            Some(money(1201, "JPY"))
        );
        assert_eq!(Money::from_decimal("lots", CurrencyCode::USD), None);
    }

    #[test]
    fn decimals_pad_and_sign() {
        assert_eq!(decimal(5, 2), "0.05");
        assert_eq!(decimal(-5, 2), "-0.05");
        assert_eq!(decimal(123_456, 3), "123.456");
        assert_eq!(decimal(0, 2), "0.00");
        assert_eq!(decimal(i64::MIN, 0), "-9223372036854775808");
    }

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
    fn a_price_keeps_its_text_and_reads_its_money() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        #[serde(rename_all = "camelCase")]
        struct Sale {
            order_id: String,
            #[serde(flatten)]
            price: Price,
            reference: Option<String>,
        }
        for (json, money) in [
            (
                r#"{"orderId":"o","amount":"24.5","currency":"usd","reference":null}"#,
                Some(Money::new(2450, CurrencyCode::USD)),
            ),
            (
                r#"{"orderId":"o","amount":"lots","currency":"USD","reference":null}"#,
                None,
            ),
            (
                r#"{"orderId":"o","amount":"1","currency":"dollars","reference":null}"#,
                None,
            ),
        ] {
            let sale: Sale = serde_json::from_str(json).unwrap();
            assert_eq!(sale.price.money(), money, "{json}");
            assert_eq!(serde_json::to_string(&sale).unwrap(), json, "as it came");
        }
        let price = Price::of(Money::new(1200, code("JPY")));
        assert_eq!((price.amount(), price.currency()), ("1200", "JPY"));
    }

    #[test]
    fn money_flattens_into_the_struct_its_in() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        #[serde(rename_all = "camelCase")]
        struct Sale {
            id: String,
            #[serde(flatten, with = "camel")]
            amount: Money,
            reference: Option<String>,
        }
        let json = r#"{"id":"p","amountMinor":1388,"currency":"USD","reference":null}"#;
        let sale: Sale = serde_json::from_str(json).unwrap();
        assert_eq!(sale.amount, Money::new(1388, CurrencyCode::USD));
        assert_eq!(serde_json::to_string(&sale).unwrap(), json);
    }
}
