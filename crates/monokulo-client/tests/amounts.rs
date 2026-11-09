//! Ported from the Android app's `AmountsTest`, the parts that don't depend
//! on a locale. Which symbol an amount carries is `amounts::written`'s now,
//! tested in `tests/money.rs` (core.md decision 99); the tests of the
//! narrow symbol, the dollar clashes, a currency at home, letters in a
//! symbol and a currency with no symbol are there. Here a stand-in formatter
//! shows what reaches it: minor units and an upper-case ISO 4217 code.

use monokulo_client::amounts;

fn fiat(amount: &str, currency: &str) -> String {
    amounts::fiat(amount, currency, &|minor: i64, code: &str| {
        format!("<{minor} {code}>")
    })
}

#[test]
fn an_amount_is_written_the_way_the_tills_locale_writes_money() {
    assert_eq!(fiat("24.50", "USD"), "<2450 USD>");
    assert_eq!(fiat("8", "eur"), "<800 EUR>");
    assert_eq!(fiat("1200", "JPY"), "<1200 JPY>");
}

#[test]
fn fraction_digits_follow_the_currency() {
    assert_eq!(fiat("1200.6", "JPY"), "<1201 JPY>");
    assert_eq!(fiat("0.1", "USD"), "<10 USD>");
}

#[test]
fn an_amount_that_wont_parse_is_shown_as_it_came() {
    assert_eq!(fiat("4.5", "XMR"), "4.5 XMR");
    assert_eq!(fiat("lots", "XYZ"), "lots XYZ");
    assert_eq!(fiat("12", "???"), "12 ???");
    assert_eq!(fiat("4.5", "xmr"), "4.5 XMR");
    assert_eq!(fiat("lots", "USD"), "lots USD");
}
