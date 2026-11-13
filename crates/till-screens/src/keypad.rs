//! "Till", the keypad point of sale (docs/view-key-mode.md §7), modelled on
//! Monokulo's web POS (`apps/pos` in mokulo): keys 1–9, C, 0 and ⌫, the
//! amount over them, an optional reference, and Charge. In the shop's
//! currency the digits fill from the right in its minor units (1, 2, 5 is
//! 1.25); with no fiat currency (XMR only) there's a decimal point key and
//! up to 12 decimals. The sales put aside for the next customer stay live
//! and show over the keys, each opening its code.

use monokulo_client::{MoneyFormat, OrderId};
use till_sales::TillSale;
use till_types::{CurrencyCode, Money};

use crate::view::{Device, Press};
use crate::{sale_text, time::DeviceClock};

/// The most whole digits an amount has, as the web POS allows.
pub const WHOLE_DIGITS: usize = 9;
/// The most digits after the point in XMR.
pub const XMR_DECIMALS: usize = 12;

/// A key on the keypad, or its hardware keyboard's (Escape is Clear,
/// Backspace is ⌫).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum KeypadKey {
    Digit { digit: u8 },
    /// XMR only: the decimal point.
    Point,
    Clear,
    Backspace,
}

/// Whether amounts are typed in XMR, with a decimal point.
fn xmr(currency: CurrencyCode) -> bool {
    currency == "XMR"
}

/// What's typed after `key`: in a fiat currency the digits of its minor
/// units, without leading zeros ("" is zero); in XMR the decimal as typed
/// ("0.5", "12."). A key that would go past the limits changes nothing.
pub fn press(typed: &str, key: KeypadKey, currency: CurrencyCode) -> String {
    match key {
        KeypadKey::Clear => String::new(),
        KeypadKey::Backspace => {
            let mut typed = typed.to_owned();
            typed.pop();
            typed
        }
        KeypadKey::Point if xmr(currency) && !typed.contains('.') => {
            if typed.is_empty() {
                "0.".into()
            } else {
                format!("{typed}.")
            }
        }
        KeypadKey::Point => typed.to_owned(),
        KeypadKey::Digit { digit } if digit <= 9 => {
            let d = char::from(b'0' + digit);
            if xmr(currency) {
                match typed.split_once('.') {
                    Some((_, after)) if after.len() >= XMR_DECIMALS => typed.to_owned(),
                    Some(_) => format!("{typed}{d}"),
                    None if typed.len() >= WHOLE_DIGITS => typed.to_owned(),
                    None if typed == "0" || typed.is_empty() => d.to_string(),
                    None => format!("{typed}{d}"),
                }
            } else {
                let most = WHOLE_DIGITS + currency.minor_digits() as usize;
                let next = format!("{typed}{d}");
                let next = next.trim_start_matches('0');
                if next.len() > most {
                    typed.to_owned()
                } else {
                    next.to_owned()
                }
            }
        }
        KeypadKey::Digit { .. } => typed.to_owned(),
    }
}

/// What's typed as money; `None` at zero.
pub fn amount(typed: &str, currency: CurrencyCode) -> Option<Money> {
    let minor = if xmr(currency) {
        let (whole, after) = typed.split_once('.').unwrap_or((typed, ""));
        let digits = currency.minor_digits() as usize;
        let after: String = after.chars().chain(std::iter::repeat('0')).take(digits).collect();
        format!("{whole}{after}").parse::<i64>().ok()?
    } else {
        typed.parse::<i64>().ok()?
    };
    (minor > 0).then(|| Money::new(minor, currency))
}

/// What the keypad is drawn from.
#[derive(Clone, Debug)]
pub struct KeypadInput {
    /// The keypad's currency; `None` until it's known (a Monokulo store not
    /// read yet).
    pub currency: Option<CurrencyCode>,
    pub typed: String,
    pub reference: String,
    /// Charge's request is under way.
    pub busy: bool,
    pub problem: Option<String>,
    /// The till's sales, newest first: the live ones are shown as put aside.
    pub sales: Vec<TillSale>,
    pub clock: DeviceClock,
}

/// A sale put aside for the next customer, over the keys: tap to open its
/// code.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct AsideSale {
    pub order_id: OrderId,
    /// Its reference, else "Sale #a1b2".
    pub label: String,
    pub amount: String,
    /// "Waiting", "Seen", "Confirming · 4 of 10".
    pub status: String,
    /// What a screen reader says: "Open Sam, $8.40, waiting".
    pub said: String,
}

/// A key as drawn: its label, and what a screen reader says.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct KeypadButton {
    pub key: KeypadKey,
    /// "7", "C", "." or "⌫" (drawn as the platform's delete symbol).
    pub label: String,
    pub said: String,
}

/// The keypad, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct KeypadView {
    /// "$1.25", "1,25 €", "0.5 XMR"; zero as the currency writes it.
    pub amount: String,
    /// What a screen reader says of it: "Amount, $1.25".
    pub amount_said: String,
    /// Four rows of three: 1 2 3, 4 5 6, 7 8 9, then C 0 ⌫, or in XMR
    /// . 0 ⌫ with C over the amount.
    pub keys: Vec<KeypadButton>,
    /// XMR only: Clear, beside the amount, since the point takes its key.
    pub clear: Option<KeypadButton>,
    /// "Reference (optional)".
    pub reference_label: String,
    /// "Customer name or note".
    pub reference_placeholder: String,
    pub reference: String,
    pub reference_max: u32,
    /// "Charge $1.25", off at zero; "Making the code…" while it's asked.
    pub charge: Press,
    pub problem: Option<String>,
    /// "Put aside · 2" over the sales put aside; `None` with none.
    pub aside_title: Option<String>,
    pub aside: Vec<AsideSale>,
    /// "Sales": the shift's list.
    pub sales: Press,
    /// "Settings".
    pub settings: String,
}

/// The keypad's keys for `currency`.
fn keys(currency: CurrencyCode) -> (Vec<KeypadButton>, Option<KeypadButton>) {
    let digit = |d: u8| KeypadButton {
        key: KeypadKey::Digit { digit: d },
        label: d.to_string(),
        said: d.to_string(),
    };
    let clear = KeypadButton {
        key: KeypadKey::Clear,
        label: "C".into(),
        said: "Clear".into(),
    };
    let back = KeypadButton {
        key: KeypadKey::Backspace,
        label: "⌫".into(),
        said: "Delete".into(),
    };
    let mut rows: Vec<KeypadButton> = (1..=9).map(digit).collect();
    if xmr(currency) {
        rows.push(KeypadButton {
            key: KeypadKey::Point,
            label: ".".into(),
            said: "Point".into(),
        });
        rows.extend([digit(0), back]);
        (rows, Some(clear))
    } else {
        rows.extend([clear, digit(0), back]);
        (rows, None)
    }
}

/// How `typed` is shown in `currency`.
fn shown(typed: &str, currency: CurrencyCode, money: &dyn MoneyFormat) -> String {
    if xmr(currency) {
        let typed = if typed.is_empty() { "0" } else { typed };
        format!("{typed} XMR")
    } else {
        let minor = typed.parse::<i64>().unwrap_or(0);
        money.format(minor, currency.as_str())
    }
}

/// The keypad, amounts written by `money`.
pub fn keypad(input: &KeypadInput, _device: Device, money: &dyn MoneyFormat) -> KeypadView {
    let currency = input.currency;
    let (amount, keys, clear) = match currency {
        Some(c) => {
            let (keys, clear) = keys(c);
            (shown(&input.typed, c, money), keys, clear)
        }
        None => ("…".to_owned(), keys(CurrencyCode::USD).0, None),
    };
    let to_charge = currency.and_then(|c| self::amount(&input.typed, c));
    let charge = if input.busy {
        Press::new("Making the code…").enabled(false).busy(true)
    } else {
        match &to_charge {
            Some(_) => Press::new(format!("Charge {amount}")),
            None => Press::new("Charge").enabled(false),
        }
    };
    let aside: Vec<AsideSale> = input
        .sales
        .iter()
        .filter(|sale| sale.live())
        .map(|sale| {
            let label = sale_text::heading(sale);
            let amount = sale_text::amount(sale, money);
            let status = sale_text::status(sale, Some(monokulo_client::PointOfSale::Till));
            AsideSale {
                order_id: sale.order_id.clone(),
                said: format!("Open {label}, {amount}, {}", status.to_lowercase()),
                label,
                amount,
                status,
            }
        })
        .collect();
    KeypadView {
        amount_said: format!("Amount, {amount}"),
        amount,
        keys,
        clear,
        reference_label: "Reference (optional)".into(),
        reference_placeholder: "Customer name or note".into(),
        reference: input.reference.clone(),
        reference_max: till_pos::keypad::REFERENCE_MAX as u32,
        charge,
        problem: input.problem.clone(),
        aside_title: (!aside.is_empty()).then(|| format!("Put aside · {}", aside.len())),
        aside,
        sales: Press::new("Sales"),
        settings: "Settings".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usd() -> CurrencyCode {
        CurrencyCode::USD
    }

    fn typed(keys: &[KeypadKey], currency: CurrencyCode) -> String {
        keys.iter().fold(String::new(), |t, k| press(&t, *k, currency))
    }

    fn d(digit: u8) -> KeypadKey {
        KeypadKey::Digit { digit }
    }

    #[test]
    fn fiat_fills_from_the_right_in_minor_units() {
        let t = typed(&[d(1), d(2), d(5)], usd());
        assert_eq!(amount(&t, usd()), Some(Money::new(125, usd())));
        assert_eq!(typed(&[d(0), d(0), d(7)], usd()), "7", "no leading zeros");
        assert_eq!(amount("", usd()), None, "zero charges nothing");
        let yen = CurrencyCode::new("JPY").unwrap();
        assert_eq!(amount(&typed(&[d(5), d(0), d(0)], yen), yen), Some(Money::new(500, yen)));
        let long = typed(&[d(9); 20], usd());
        assert_eq!(long.len(), WHOLE_DIGITS + 2, "nine whole digits and the cents");
        assert_eq!(typed(&[d(1), d(2), KeypadKey::Backspace], usd()), "1");
        assert_eq!(typed(&[d(1), KeypadKey::Clear], usd()), "");
        assert_eq!(typed(&[d(1), KeypadKey::Point], usd()), "1", "no point in fiat");
    }

    #[test]
    fn xmr_has_a_point_and_twelve_decimals() {
        let x = CurrencyCode::new("XMR").unwrap();
        assert_eq!(typed(&[KeypadKey::Point, d(5)], x), "0.5");
        assert_eq!(
            amount("0.5", x),
            Some(Money::new(500_000_000_000, x)),
            "in piconero"
        );
        assert_eq!(typed(&[d(1), KeypadKey::Point, KeypadKey::Point, d(2)], x), "1.2");
        let many = typed(&[[KeypadKey::Point].as_slice(), &[d(1); 15]].concat(), x);
        assert_eq!(many, format!("0.{}", "1".repeat(12)));
        assert_eq!(amount("0.", x), None);
        assert_eq!(typed(&[d(0), d(3)], x), "3");
    }
}
