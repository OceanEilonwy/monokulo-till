//! Amounts as people read them (Android: `core/Amounts.kt`).
//!
//! The device's locale writes the number, so `$24.50` follows it exactly as
//! the Android app always has (docs/design.md §6, decision 34) without the
//! core carrying locale data. What is here is what doesn't depend on locale
//! data: a currency's minor units, reading Monokulo's decimal strings into
//! them (by till-types' one ISO 4217 table), and which symbol an amount
//! carries ([`written`], core.md decision
//! 99), so iOS and Android print amounts identically.

use till_types::{CurrencyCode, Price};

/// How the device writes an amount of money: `minor` units of `currency`
/// (an ISO 4217 code, upper case), so 2450 and "USD" is "$24.50" in the US.
/// The engine writes it with [`written`], from the platform's locale data.
pub trait MoneyFormat {
    fn format(&self, minor: i64, currency: &str) -> String;
}

impl<F: Fn(i64, &str) -> String> MoneyFormat for F {
    fn format(&self, minor: i64, currency: &str) -> String {
        self(minor, currency)
    }
}

/// The sign a locale's money pattern leaves for the currency symbol, which
/// [`written`] fills in: "¤24.50" in the US, "8,00 ¤" in Germany.
pub const SIGN: char = '\u{a4}';

/// What the device's locale knows about a currency, as the platform looks
/// it up (decision 9 of core.md, amended by 99): the locale data stays with
/// the platform, the choice of symbol is [`written`]'s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbols {
    /// How the device's locale writes the currency's symbol: "$", "R$",
    /// "A$", or the code itself where it has none ("USD" in Australia).
    pub local: String,
    /// How international English (`en-001`) writes it, whose symbols name
    /// the country: "US$", "A$", "CN¥"; the code where it has none ("CHF").
    pub international: String,
    /// The currency of the device's own country, `None` for a locale with
    /// no country.
    pub home: Option<String>,
}

/// The device's locale, as [`written`] needs it.
pub trait MoneyLocale {
    /// `minor` units of `currency` in the locale's money pattern, with
    /// [`SIGN`] where the symbol goes: "¤24.50", "8,00 ¤", "¤ 4,50".
    fn pattern(&self, minor: i64, currency: &str) -> String;
    fn symbols(&self, currency: &str) -> Symbols;
}

/// Symbols that put country letters before a shared sign: "US$" is "$",
/// "CN¥" is "¥".
const PREFIXED: &[&str] = &[
    "US$", "A$", "AU$", "CA$", "HK$", "NZ$", "MX$", "NT$", "CN¥", "JP¥",
];

/// `minor` units of `currency` (ISO 4217, upper case) the way `locale`
/// writes money, with the currency's narrow symbol: 2450 USD is "$24.50" in
/// the US and in Britain alike, since a store takes one currency. The
/// symbol says which dollar only when the till's own currency would read
/// the same: USD on an Australian till is "US$24.50". A currency with no
/// symbol ("CHF") follows the number after a space. (Android's
/// `engine/Money.kt` until decision 99, so iOS writes amounts the same.)
pub fn written(minor: i64, currency: &str, locale: &dyn MoneyLocale) -> String {
    let symbols = locale.symbols(currency);
    let pattern = locale.pattern(minor, currency);
    let Some(narrow) = narrow(currency, &symbols) else {
        let number: String = pattern.chars().filter(|&c| c != SIGN).collect();
        return format!(
            "{} {currency}",
            number.trim_matches(|c: char| c.is_whitespace() || c == '\u{a0}' || c == '\u{202f}')
        );
    };
    let symbol = if clashes(currency, &narrow, &symbols, locale) {
        distinct(&symbols)
    } else {
        narrow
    };
    pattern.replace(SIGN, &symbol)
}

/// `code`'s narrow symbol: the locale's own ("R$" in Brazil, "kr" in
/// Sweden), or the international one where the locale writes only the
/// code, with country letters taken off the shared signs ("US$" to "$").
/// `None` when neither has a symbol ("CHF").
fn narrow(code: &str, symbols: &Symbols) -> Option<String> {
    let symbol = [&symbols.local, &symbols.international]
        .into_iter()
        .find(|symbol| symbol.as_str() != code)?;
    Some(if PREFIXED.contains(&symbol.as_str()) {
        symbol.chars().filter(|c| !c.is_ascii_uppercase()).collect()
    } else {
        symbol.clone()
    })
}

/// Whether the till's own currency is another one written with the same
/// narrow symbol.
fn clashes(code: &str, narrow: &str, symbols: &Symbols, locale: &dyn MoneyLocale) -> bool {
    match symbols.home.as_deref() {
        Some(home) if home != code => {
            self::narrow(home, &locale.symbols(home)).as_deref() == Some(narrow)
        }
        _ => false,
    }
}

/// A symbol that tells the currency apart: the locale's own when it has one
/// with letters and a sign ("A$" in the US, "AU$" in Germany), else the
/// international one ("US$" in Australia, which writes USD as just the
/// code).
fn distinct(symbols: &Symbols) -> String {
    let local = &symbols.local;
    let letters = local.chars().any(|c| c.is_ascii_alphabetic());
    let sign = local.chars().any(|c| !c.is_ascii_alphabetic());
    if letters && sign {
        local.clone()
    } else {
        symbols.international.clone()
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

/// `price` as `format` writes it, from the money it was read as
/// ([`Price::money`]): what [`fiat`] writes of its texts, without reading
/// them again. A currency that isn't ISO 4217's, or a price that isn't
/// money, is shown as it came, followed by the code in upper case.
pub fn price(price: &Price, format: &dyn MoneyFormat) -> String {
    match price.money().filter(|m| m.currency.iso_digits().is_some()) {
        Some(m) => format.format(m.minor, m.currency.as_str()),
        None => {
            let code = price.currency().to_ascii_uppercase();
            // XMR in piconero's twelve digits, without the zeros after them.
            let amount = if code == "XMR" && price.amount().contains('.') {
                price
                    .amount()
                    .trim_end_matches('0')
                    .trim_end_matches('.')
            } else {
                price.amount()
            };
            format!("{amount} {code}")
        }
    }
}

/// How many digits `code`'s minor unit has (2 for USD, 0 for JPY, 3 for
/// KWD, any case), or `None` for a code that isn't on ISO 4217's list
/// ([`CurrencyCode::iso_digits`], the one table).
pub fn exponent(code: &str) -> Option<u32> {
    CurrencyCode::new(code).ok()?.iso_digits()
}

pub use till_types::money::minor_units;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_xmr_price_is_shown_without_its_trailing_zeros() {
        let format = |minor: i64, code: &str| format!("{minor} {code}");
        assert_eq!(price(&Price::new("1.500000000000", "XMR"), &format), "1.5 XMR");
        assert_eq!(price(&Price::new("2.000000000000", "XMR"), &format), "2 XMR");
        assert_eq!(price(&Price::new("2", "XMR"), &format), "2 XMR");
    }

    #[test]
    fn exponents_follow_iso_4217() {
        assert_eq!(exponent("USD"), Some(2));
        assert_eq!(exponent("JPY"), Some(0));
        assert_eq!(exponent("KWD"), Some(3));
        assert_eq!(exponent("CLF"), Some(4));
        assert_eq!(exponent("XAU"), Some(0));
        assert_eq!(exponent("XMR"), None);
        assert_eq!(
            exponent("usd"),
            Some(2),
            "any case, as every table reads it"
        );
        assert_eq!(exponent("dollars"), None);
    }
}
