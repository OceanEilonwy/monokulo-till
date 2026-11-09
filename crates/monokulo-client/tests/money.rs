//! Ported from Android's `engine/MoneyTest` (the six of `core`'s
//! `AmountsTest` that read the JVM's locale data): which symbol an amount
//! carries is `amounts::written`'s now (decision 99 of core.md), and only
//! the locale's pattern and symbols are the platform's. The stand-in
//! locales below give the symbols and patterns the JVM gives for each
//! locale the Kotlin tests used.

use monokulo_client::amounts::{self, MoneyLocale, SIGN, Symbols};

/// A locale as the platform describes it: where the sign goes, its
/// separators, its own currency and how it writes each currency's symbol.
struct Locale {
    home: Option<&'static str>,
    sign_first: bool,
    /// A space between the sign and the number.
    spaced: bool,
    decimal: char,
    group: char,
    local: &'static [(&'static str, &'static str)],
}

/// International English (`en-001`), whose symbols name the country.
const INTERNATIONAL: &[(&str, &str)] = &[
    ("USD", "US$"),
    ("AUD", "A$"),
    ("CAD", "CA$"),
    ("JPY", "JP¥"),
    ("EUR", "€"),
    ("GBP", "£"),
    ("BRL", "R$"),
    ("ZAR", "ZAR"),
    ("SEK", "SEK"),
    ("SGD", "SGD"),
    ("PLN", "PLN"),
    ("CHF", "CHF"),
];

fn lookup(table: &[(&str, &str)], code: &str) -> String {
    table
        .iter()
        .find(|(c, _)| *c == code)
        .map_or(code.to_owned(), |(_, s)| (*s).to_owned())
}

impl MoneyLocale for Locale {
    fn pattern(&self, minor: i64, currency: &str) -> String {
        let digits = amounts::exponent(currency).unwrap_or(2) as usize;
        let text = minor.abs().to_string();
        let text = format!("{text:0>width$}", width = digits + 1);
        let (whole, frac) = text.split_at(text.len() - digits);
        let mut grouped = String::new();
        for (i, c) in whole.chars().enumerate() {
            if i > 0 && (whole.len() - i) % 3 == 0 {
                grouped.push(self.group);
            }
            grouped.push(c);
        }
        let number = if digits == 0 {
            grouped
        } else {
            format!("{grouped}{}{frac}", self.decimal)
        };
        let space = if self.spaced { "\u{a0}" } else { "" };
        if self.sign_first {
            format!("{SIGN}{space}{number}")
        } else {
            format!("{number}{space}{SIGN}")
        }
    }

    fn symbols(&self, currency: &str) -> Symbols {
        Symbols {
            local: lookup(self.local, currency),
            international: lookup(INTERNATIONAL, currency),
            home: self.home.map(Into::into),
        }
    }
}

const fn english(
    home: Option<&'static str>,
    local: &'static [(&'static str, &'static str)],
) -> Locale {
    Locale {
        home,
        sign_first: true,
        spaced: false,
        decimal: '.',
        group: ',',
        local,
    }
}

const fn continental(home: &'static str, local: &'static [(&'static str, &'static str)]) -> Locale {
    Locale {
        home: Some(home),
        sign_first: false,
        spaced: true,
        decimal: ',',
        group: '.',
        local,
    }
}

const US: Locale = english(
    Some("USD"),
    &[
        ("USD", "$"),
        ("JPY", "¥"),
        ("GBP", "£"),
        ("EUR", "€"),
        ("AUD", "A$"),
        ("CAD", "CA$"),
        ("BRL", "R$"),
    ],
);
const UK: Locale = english(
    Some("GBP"),
    &[
        ("GBP", "£"),
        ("EUR", "€"),
        ("USD", "US$"),
        ("JPY", "JP¥"),
        ("AUD", "A$"),
    ],
);
const AUSTRALIA: Locale = english(Some("AUD"), &[("AUD", "$"), ("EUR", "€"), ("JPY", "JPY")]);
const CANADA: Locale = english(Some("CAD"), &[("CAD", "$"), ("USD", "US$"), ("AUD", "A$")]);
const JAPAN: Locale = english(
    Some("JPY"),
    &[("JPY", "￥"), ("USD", "$"), ("EUR", "€"), ("AUD", "A$")],
);
const ENGLISH: Locale = english(None, &[("USD", "$")]);
const SINGAPORE: Locale = english(Some("SGD"), &[("SGD", "$")]);
const SOUTH_AFRICA: Locale = Locale {
    decimal: ',',
    group: '\u{a0}',
    ..english(Some("ZAR"), &[("ZAR", "R")])
};
const BRAZIL: Locale = Locale {
    sign_first: true,
    ..continental("BRL", &[("BRL", "R$")])
};
const GERMANY: Locale = continental(
    "EUR",
    &[("EUR", "€"), ("USD", "$"), ("JPY", "¥"), ("AUD", "AU$")],
);
const SWEDEN: Locale = continental("SEK", &[("SEK", "kr")]);
const POLAND: Locale = continental("PLN", &[("PLN", "zł")]);

fn money(minor: i64, currency: &str, locale: &Locale) -> String {
    amounts::written(minor, currency, locale).replace('\u{a0}', " ")
}

#[test]
fn an_amount_is_written_the_way_the_tills_locale_writes_money() {
    assert_eq!(money(2450, "USD", &US), "$24.50");
    assert_eq!(money(800, "EUR", &UK), "€8.00");
    assert_eq!(money(1200, "JPY", &US), "¥1,200");
    assert_eq!(money(800, "EUR", &GERMANY), "8,00 €");
}

#[test]
fn an_amount_carries_the_narrow_symbol_never_the_currencys_code() {
    assert_eq!(money(450, "USD", &UK), "$4.50");
    assert_eq!(money(1200, "JPY", &UK), "¥1,200");
    assert_eq!(money(1200, "JPY", &AUSTRALIA), "¥1,200");
    assert_eq!(money(800, "EUR", &AUSTRALIA), "€8.00");
    assert_eq!(money(680, "AUD", &AUSTRALIA), "$6.80");
    // Japan writes its own yen full width.
    assert_eq!(money(1200, "JPY", &JAPAN), "￥1,200");
    assert_eq!(money(450, "USD", &JAPAN), "$4.50");
    assert_eq!(money(800, "EUR", &JAPAN), "€8.00");
    assert_eq!(money(450, "USD", &GERMANY), "4,50 $");
    assert_eq!(money(1200, "JPY", &GERMANY), "1.200 ¥");
    assert_eq!(money(320, "GBP", &US), "£3.20");
}

#[test]
fn a_dollar_the_tills_own_dollar_would_be_mistaken_for_says_which_it_is() {
    assert_eq!(money(450, "USD", &AUSTRALIA), "US$4.50");
    assert_eq!(money(680, "AUD", &US), "A$6.80");
    assert_eq!(money(680, "AUD", &CANADA), "A$6.80");
    assert_eq!(money(200, "CAD", &US), "CA$2.00");
    assert_eq!(money(450, "USD", &CANADA), "US$4.50");
}

#[test]
fn a_dollar_no_own_dollar_clashes_with_keeps_the_narrow_symbol() {
    assert_eq!(money(680, "AUD", &UK), "$6.80");
    assert_eq!(money(680, "AUD", &GERMANY), "6,80 $");
    assert_eq!(money(680, "AUD", &JAPAN), "$6.80");
    // A locale with no country has no currency of its own to clash with.
    assert_eq!(money(450, "USD", &ENGLISH), "$4.50");
}

#[test]
fn a_currency_at_home_keeps_the_symbol_its_own_locale_writes() {
    assert_eq!(money(450, "BRL", &BRAZIL), "R$ 4,50");
    assert_eq!(money(450, "ZAR", &SOUTH_AFRICA), "R4,50");
    assert_eq!(money(450, "SEK", &SWEDEN), "4,50 kr");
    assert_eq!(money(450, "SGD", &SINGAPORE), "$4.50");
    assert_eq!(money(450, "PLN", &POLAND), "4,50 zł");
}

#[test]
fn a_symbol_whose_letters_are_the_symbol_keeps_them_abroad() {
    assert_eq!(money(450, "BRL", &US), "R$4.50");
}

#[test]
fn a_currency_with_no_symbol_follows_the_number_after_a_space() {
    assert_eq!(money(450, "CHF", &US), "4.50 CHF");
    assert_eq!(money(450, "CHF", &GERMANY), "4,50 CHF");
}
