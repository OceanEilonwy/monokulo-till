//! How a sale is named and described everywhere it's shown: the shift, the
//! sale, its page, the bubble and the notifications (Android:
//! `SaleText.kt`'s `SaleText`).
//!
//! Amounts go through the device's [`MoneyFormat`], times through its
//! [`DeviceClock`], as every screen's do.

use monokulo_client::{MoneyFormat, PointOfSale, SalePhase, amounts};
use orf_codec::LineItem;
use orf_codec::Receipt;
use till_sales::{Standing, TillSale};
use till_types::{CurrencyCode, Money, UnixSeconds};

use crate::decimal::Decimal;
use crate::pos_text;
use crate::time::{DeviceClock, time};

/// "Sale #1042" from the POS's receipt number, or "Monero sale" when the
/// POS gave none.
pub fn title(sale: &TillSale) -> String {
    let reference = sale.reference.as_deref().unwrap_or("").trim();
    // A sale charged on Till's keypad is named by its reference as typed
    // ("Sam", "Table 4"), as Monokulo's web POS names it.
    let keypad = sale
        .found
        .as_ref()
        .is_some_and(|f| f.point_of_sale == PointOfSale::Till);
    if keypad && !reference.is_empty() {
        reference.into()
    } else if reference.is_empty() {
        "Monero sale".into()
    } else if reference.starts_with('#') {
        format!("Sale {reference}")
    } else {
        format!("Sale #{reference}")
    }
}

/// How staff tell the sale apart (core.md decision 118): whom it's for when
/// the POS's order names them ([`TillSale::customer_name`]), else
/// [`title`]. It leads the shift's rows, the sale's page, the alert and the
/// Live Activity, and Android's notifications, their lock-screen text
/// included (decision 124); the sale number moves to the line under it.
pub fn heading(sale: &TillSale) -> String {
    customer(sale).map_or_else(|| title(sale), Into::into)
}

/// The line under [`heading`]: `place`, led by the sale's [`title`] when the
/// heading is the customer's name, so the cashier can still match it to the
/// POS ("Sale #m2Qe · Front Counter · this till").
pub fn heading_line(sale: &TillSale, place: &str) -> String {
    match customer(sale) {
        None => place.into(),
        Some(_) => format!("{} · {place}", title(sale)),
    }
}

/// The line under a sale's title on its own screens (owner, round 4): the
/// sale's number when the title is the customer's name, where it's from
/// only when it's another till's (a till serves one place, so "Front
/// Counter · this till" told the cashier nothing), then `when`: "14:00
/// left" while it waits, the time it was rung up after. "Sale #m2Qe ·
/// 14:00 left", "From Bar iPad · 10:10 PM".
pub fn sale_line(sale: &TillSale, place: &str, when: Option<&str>) -> String {
    let mut parts = Vec::new();
    if customer(sale).is_some() {
        parts.push(title(sale));
    }
    if sale.found.is_none() {
        parts.push(place.to_owned());
    }
    parts.extend(when.map(str::to_owned));
    parts.join(" · ")
}

/// Whom the sale is for, when the POS's order names them.
pub fn customer(sale: &TillSale) -> Option<&str> {
    sale.customer_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

/// "#m2Qe": the sale's number at the POS, when it gave one.
pub fn label(sale: &TillSale) -> Option<String> {
    let reference = sale.reference.as_deref()?.trim();
    if reference.is_empty() {
        None
    } else if reference.starts_with('#') {
        Some(reference.into())
    } else {
        Some(format!("#{reference}"))
    }
}

/// Where it came from: "Front Counter · this till" for this till's, "From
/// Back till" for another's.
pub fn place(sale: &TillSale, location: Option<&str>) -> String {
    match (&sale.found, location, &sale.device_name) {
        (Some(_), Some(location), _) => format!("{location} · this till"),
        (Some(_), None, _) => "This till".into(),
        (None, _, Some(device)) => format!("From {device}"),
        (None, _, None) => "From another till".into(),
    }
}

/// The sale's amount, as the device writes money.
pub fn amount(sale: &TillSale, money: &dyn MoneyFormat) -> String {
    amounts::price(&sale.price, money)
}

/// The exact XMR to pay: rounding it would have the customer underpay.
pub fn xmr(sale: &TillSale) -> String {
    format!("{} XMR", sale.xmr)
}

/// The sale's state in a few words: the shift row's second line and the
/// detail page's heading. Another till's sale says what this till knows
/// (its code ran out, its payment was reversed, it was closed here), never
/// that the other till closed it, which Monokulo doesn't tell us. `till` is
/// the sale's point of sale ([`pos_text::of`]).
pub fn status(sale: &TillSale, till: Option<PointOfSale>) -> String {
    match sale.standing() {
        Standing::Live(phase) => match phase {
            SalePhase::Partial => "Part paid".into(),
            SalePhase::Seen => "Seen on the network".into(),
            SalePhase::Confirming => confirming(sale),
            _ => "Waiting for payment".into(),
        },
        Standing::Paid {
            late, after_refund, ..
        } => {
            let paid = if after_refund {
                "Paid after the refund"
            } else if sale.closed_at.is_some() {
                "Paid after you closed it"
            } else if late {
                "Paid after the code expired"
            } else {
                "Paid"
            };
            if sale.cash() {
                // The point of sale's books say cash (core.md decision 117).
                format!("{paid} in Monero · cash in {}", pos_text::name(till))
            } else {
                paid.into()
            }
        }
        Standing::Overpaid => "Overpaid".into(),
        Standing::CashOffer { kept: false } => format!("Cash in {}", pos_text::name(till)),
        Standing::CashOffer { kept: true } => "Cash · kept as cash".into(),
        Standing::PaidInCash => "Paid in cash".into(),
        Standing::Closed => "Closed · not paid".into(),
        Standing::Expired | Standing::Reversed | Standing::OtherTill => ended(sale.phase).into(),
        Standing::Refunded => pos_text::refunded_in(till),
        Standing::RefundInPos => format!("Not paid · {}", pos_text::refund_it_in(till)),
    }
}

/// "Confirming · 4 of 10": its payment's confirmations of the store's
/// threshold, or "Confirming" until Monokulo says.
pub fn confirming(sale: &TillSale) -> String {
    match (
        sale.confirmations,
        sale.confirmations_required.filter(|&n| n > 0),
    ) {
        (Some(have), Some(need)) => format!("Confirming · {} of {need}", have.min(need)),
        _ => "Confirming".into(),
    }
}

/// How a sale that isn't live ended, or that it was closed here while
/// still waiting.
fn ended(phase: SalePhase) -> &'static str {
    match phase {
        SalePhase::Expired => "Code expired · not paid",
        SalePhase::Cancelled => "Cancelled · not paid",
        SalePhase::DoubleSpend => "Payment reversed",
        _ => "Closed · not paid",
    }
}

/// The status's second line under a live sale's code: what the cashier
/// tells the customer; paid, the XMR it took.
pub fn hint(sale: &TillSale, seconds_left: i64) -> String {
    match sale.phase {
        SalePhase::Waiting => format!("Scan with any Monero wallet · {} left", clock(seconds_left)),
        SalePhase::Partial => format!(
            "Ask the customer to pay the rest to the same code · {} left",
            clock(seconds_left)
        ),
        SalePhase::Paid => format!("{} received", xmr(sale)),
        _ => "The customer can go: it's being confirmed".into(),
    }
}

/// Which of the four steps (Waiting, Seen, Confirming, Paid) it's at, from
/// 1; 0 when it ended unpaid.
pub fn step(phase: SalePhase) -> u8 {
    match phase {
        SalePhase::Waiting | SalePhase::Partial => 1,
        SalePhase::Seen => 2,
        SalePhase::Confirming => 3,
        SalePhase::Paid => 4,
        SalePhase::Expired | SalePhase::Cancelled | SalePhase::DoubleSpend => 0,
    }
}

/// "1:05" from seconds.
pub fn clock(seconds: i64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Seconds from the clock's now until `expires_at`; 0 once it's passed.
pub fn seconds_left(expires_at: UnixSeconds, now: UnixSeconds) -> i64 {
    (expires_at.0 - now.0).max(0)
}

/// "15 items" or "1 item": the receipt's whole quantities added up, and a
/// line sold by measure (2.5 × Granola) one item however much of it, so "2
/// × Flat white, 2.5 × Granola" is "3 items", what the customer takes away
/// (decision 145 of android.md).
pub fn items(receipt: &Receipt) -> String {
    match item_count(receipt) {
        1 => "1 item".into(),
        n => format!("{n} items"),
    }
}

pub fn item_count(receipt: &Receipt) -> i64 {
    receipt
        .line_items
        .iter()
        .map(|item| {
            if item.quantity % 1.0 == 0.0 {
                item.quantity as i64
            } else {
                1
            }
        })
        .sum()
}

/// "2 × Flat white, Oat latte, 3 × Almond croissant, +5 more": the first
/// `show` lines, and how many more there are.
pub fn first_items(receipt: &Receipt, show: usize) -> String {
    let lines = &receipt.line_items;
    let head: Vec<String> = lines.iter().take(show).map(line).collect();
    let head = head.join(", ");
    match lines.len().saturating_sub(show) {
        0 => head,
        more => format!("{head}, +{more} more"),
    }
}

/// "2 × Flat white"; one of something is just its name.
pub fn line(item: &LineItem) -> String {
    if item.quantity == 1.0 {
        item.description.clone()
    } else {
        format!("{} × {}", quantity(item.quantity), item.description)
    }
}

/// "2", or "1.5" for something sold by weight.
pub fn quantity(quantity: f64) -> String {
    Decimal::of(quantity).map_or_else(|| quantity.to_string(), |d| d.stripped().plain())
}

/// A receipt's amount as money: the receipt's JSON number (which orf-codec
/// keeps as a double, its wire format) read back to the decimal it was
/// written as, by its shortest digits as `BigDecimal.valueOf` read it, and
/// rounded half up to the currency's minor unit. Exact for any amount
/// orf-codec encodes (at most 9 × 10^15 minor units, fewer than 16
/// digits). `None` for a currency that isn't a code, or a value that isn't
/// a number or doesn't fit (core.md decision 212).
pub fn receipt_money(value: f64, currency: &str) -> Option<Money> {
    let currency = CurrencyCode::new(currency).ok()?;
    let minor = Decimal::of(value)?
        .round(currency.minor_digits())?
        .units()?;
    Some(Money::new(minor, currency))
}

/// "9.00": a receipt's amount in the currency's minor digits with no
/// symbol, as the design's order lists have it ([`receipt_money`]).
pub fn plain(value: f64, currency: &str) -> String {
    receipt_money(value, currency).map_or_else(
        || {
            let digits = CurrencyCode::new(currency).map_or(2, |c| c.minor_digits());
            format!("{value:.*}", digits as usize)
        },
        |money| money.decimal(),
    )
}

/// `values` added up as money in `currency`, in its minor units; `None`
/// when any isn't money ([`receipt_money`]) or the sum doesn't fit.
pub fn sum_money(values: impl IntoIterator<Item = f64>, currency: &str) -> Option<Money> {
    let mut sum = receipt_money(0.0, currency)?;
    for value in values {
        let money = receipt_money(value, currency)?;
        sum.minor = sum.minor.checked_add(money.minor)?;
    }
    Some(sum)
}

/// The lines a sale's order card lists: every line when there are at most
/// `max`, else the first `max - 1` and how many more there are (the card's
/// "+5 more", which opens the whole order).
pub fn shown_lines(receipt: &Receipt, max: usize) -> (&[LineItem], usize) {
    let lines = receipt.line_items.as_slice();
    if lines.len() <= max {
        (lines, 0)
    } else {
        (&lines[..max - 1], lines.len() - (max - 1))
    }
}

/// "Tax 1.13": the receipt's taxes together, or `None` when it has none.
pub fn tax(receipt: &Receipt) -> Option<String> {
    let lines = receipt.tax.as_ref()?.tax_lines.as_ref()?;
    if lines.is_empty() {
        return None;
    }
    let currency = &receipt.currency;
    let sum = sum_money(lines.iter().map(|line| line.tax_amount), currency).map_or_else(
        || plain(lines.iter().map(|line| line.tax_amount).sum(), currency),
        |sum| sum.decimal(),
    );
    Some(format!("Tax {sum}"))
}

/// "13.88 USD": the receipt's total with the currency's code.
pub fn total(receipt: &Receipt) -> String {
    format!(
        "{} {}",
        plain(receipt.total, &receipt.currency),
        receipt.currency
    )
}

/// "Square receipt bP9m · 15 items · 79.21 USD": a folded order card's one
/// row, whose receipt, how much and for what.
pub fn receipt_summary(receipt: &Receipt, pos: Option<PointOfSale>) -> String {
    format!(
        "{} · {} · {}",
        pos_text::receipt_label(pos, &receipt.receipt.receipt_id),
        items(receipt),
        total(receipt)
    )
}

/// "4:07 PM" (or "16:07"): a time of day on the device's clock.
pub fn time_of(at: UnixSeconds, clock: DeviceClock) -> String {
    time(at, clock)
}

/// Ported from Android's `SaleTextTest`, test for test. The bubble's tests
/// are [`crate::bubble`]'s, `PhaseChanges`' [`crate::till_nav`]'s; the
/// loading mark's keyframes (`glintAlpha`, `coreAlpha`) stay with each
/// platform's drawing of it.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{found, instant, la, money, receipt as make_receipt, till_sale, uk};

    pub(crate) fn sale(
        phase: SalePhase,
        reference: Option<&str>,
        mine: bool,
        device: Option<&str>,
        closed_at: Option<i64>,
        refunded_at: Option<i64>,
    ) -> TillSale {
        TillSale {
            reference: reference.map(Into::into),
            code: till_sales::PaymentCode {
                uri: "monero:4abc?tx_amount=0.1421".into(),
                has_receipt: true,
                why: None,
            },
            phase,
            found: mine.then(|| found("p1", 2450, "USD", reference, "ord", "2026-10-23T10:00:00Z")),
            device_name: device.map(Into::into),
            closed_at: closed_at.map(UnixSeconds),
            refunded_at: refunded_at.map(UnixSeconds),
            ..till_sale("o1", "24.50", "USD", "0.1421")
        }
    }

    fn waiting() -> TillSale {
        sale(SalePhase::Waiting, Some("#1042"), true, None, None, None)
    }

    fn of(reference: Option<&str>) -> TillSale {
        sale(SalePhase::Waiting, reference, true, None, None, None)
    }

    fn phased(phase: SalePhase) -> TillSale {
        sale(phase, Some("#1042"), true, None, None, None)
    }

    fn bean_room() -> Receipt {
        make_receipt(
            "k4Rt",
            "2026-10-22T16:07:00-07:00",
            &[
                ("Flat white", 2.0, 4.5, 9.0),
                ("Oat latte", 1.0, 5.25, 5.25),
                ("Almond croissant", 3.0, 3.75, 11.25),
                ("Banana bread", 1.0, 4.5, 4.5),
                ("House blend beans, 250 g", 1.5, 12.0, 18.0),
            ],
            &[(0.08875, 4.26)],
            52.26,
            "USD",
        )
    }

    const SQUARE: Option<PointOfSale> = Some(PointOfSale::Square);

    #[test]
    fn a_sale_is_named_by_the_poss_receipt_number_else_as_a_monero_sale() {
        assert_eq!(title(&of(Some("#1042"))), "Sale #1042");
        assert_eq!(title(&of(Some("bP9m"))), "Sale #bP9m");
        assert_eq!(title(&of(None)), "Monero sale");
        assert_eq!(title(&of(Some(" "))), "Monero sale");
    }

    #[test]
    fn a_sales_page_is_titled_by_whom_its_for_its_number_kept_on_the_line_under_it() {
        let named = TillSale {
            customer_name: Some(" Ana López ".into()),
            ..of(Some("m2Qe"))
        };
        assert_eq!(heading(&named), "Ana López");
        assert_eq!(
            heading_line(&named, "Front Counter · this till"),
            "Sale #m2Qe · Front Counter · this till"
        );
        assert_eq!(
            title(&named),
            "Sale #m2Qe",
            "the shift, the bubble and notifications keep the number"
        );
        let blank = TillSale {
            customer_name: Some("  ".into()),
            ..of(Some("m2Qe"))
        };
        for unnamed in [of(Some("m2Qe")), blank] {
            assert_eq!(heading(&unnamed), "Sale #m2Qe");
            assert_eq!(
                heading_line(&unnamed, "Front Counter · this till"),
                "Front Counter · this till"
            );
        }
        let ana = TillSale {
            customer_name: Some("Ana".into()),
            ..of(None)
        };
        assert_eq!(heading_line(&ana, "This till"), "Monero sale · This till");
    }

    #[test]
    fn a_sale_says_where_it_came_from() {
        assert_eq!(
            place(&waiting(), Some("Front Counter")),
            "Front Counter · this till"
        );
        assert_eq!(place(&waiting(), None), "This till");
        let theirs = sale(
            SalePhase::Waiting,
            Some("#1042"),
            false,
            Some("Front Counter iPad"),
            None,
            None,
        );
        assert_eq!(
            place(&theirs, Some("Front Counter")),
            "From Front Counter iPad"
        );
        let unnamed = sale(SalePhase::Waiting, Some("#1042"), false, None, None, None);
        assert_eq!(place(&unnamed, None), "From another till");
    }

    #[test]
    fn the_four_steps_follow_the_payment_and_an_unpaid_end_is_none_of_them() {
        use SalePhase::*;
        let steps: Vec<u8> = [
            Waiting,
            Partial,
            Seen,
            Confirming,
            Paid,
            Expired,
            Cancelled,
            DoubleSpend,
        ]
        .into_iter()
        .map(step)
        .collect();
        assert_eq!(steps, [1, 1, 2, 3, 4, 0, 0, 0]);
        assert_eq!(clock(29 * 60 + 5), "29:05");
    }

    #[test]
    fn a_sales_status_follows_where_it_stands_on_this_till() {
        use SalePhase::*;
        let s = |sale: TillSale| status(&sale, SQUARE);
        let mine =
            |phase, closed, refunded| sale(phase, Some("#1042"), true, None, closed, refunded);
        assert_eq!(s(waiting()), "Waiting for payment");
        assert_eq!(s(phased(Seen)), "Seen on the network");
        assert_eq!(
            s(phased(Confirming)),
            "Confirming",
            "until Monokulo says how many"
        );
        let counted = |phase, have, need| TillSale {
            confirmations: Some(have),
            confirmations_required: Some(need),
            ..phased(phase)
        };
        assert_eq!(s(counted(Confirming, 4, 10)), "Confirming · 4 of 10");
        assert_eq!(
            s(counted(Seen, 0, 10)),
            "Seen on the network",
            "seen has no count to give"
        );
        assert_eq!(s(phased(Partial)), "Part paid");
        assert_eq!(s(phased(Paid)), "Paid");
        assert_eq!(s(mine(Paid, Some(1), None)), "Paid after you closed it");
        assert_eq!(
            s(TillSale {
                ended_at: Some(UnixSeconds(1)),
                ..phased(Paid)
            }),
            "Paid after the code expired"
        );
        assert_eq!(
            s(TillSale {
                ended_at: Some(UnixSeconds(2)),
                ..mine(Paid, Some(1), None)
            }),
            "Paid after you closed it",
            "closed, then the code ran out, then paid"
        );
        assert_eq!(
            s(mine(Paid, Some(1), Some(2))),
            "Paid after the refund",
            "a late payment after the Square refund"
        );
        assert_eq!(s(mine(Waiting, Some(1), None)), "Closed · not paid");
        assert_eq!(s(phased(Expired)), "Code expired · not paid");
        assert_eq!(
            s(mine(Expired, Some(1), None)),
            "Code expired · not paid",
            "the code running out outranks the cashier's close"
        );
        assert_eq!(s(phased(Cancelled)), "Cancelled · not paid");
        assert_eq!(s(phased(DoubleSpend)), "Payment reversed");
        assert_eq!(s(mine(Expired, None, Some(2))), "Refunded in Square");
        assert_eq!(s(mine(Cancelled, Some(1), Some(2))), "Refunded in Square");
        assert_eq!(
            s(sale(
                Expired,
                Some("#1042"),
                false,
                Some("Back till"),
                None,
                None
            )),
            "Code expired · not paid",
            "what this till knows; the row adds the till"
        );
        assert_eq!(
            s(sale(DoubleSpend, Some("#1042"), false, None, None, None)),
            "Payment reversed"
        );
        assert_eq!(
            s(sale(Waiting, Some("#1042"), false, None, Some(1), None)),
            "Closed · not paid"
        );
        assert_eq!(
            s(sale(
                Paid,
                Some("#1042"),
                false,
                Some("Back till"),
                None,
                None
            )),
            "Paid",
            "another till's paid sale is just paid"
        );
    }

    #[test]
    fn what_was_bought_is_counted_and_listed_from_the_on_device_receipt() {
        let receipt = bean_room();
        assert_eq!(
            items(&receipt),
            "8 items",
            "whole quantities add up; the 1.5 of beans by weight is one item"
        );
        let mut granola = receipt.clone();
        granola.line_items = vec![
            receipt.line_items[0].clone(),
            make_receipt("k", "x", &[("Granola", 2.5, 2.0, 5.0)], &[], 5.0, "USD").line_items[0]
                .clone(),
        ];
        assert_eq!(
            items(&granola),
            "3 items",
            "2 × Flat white and 2.5 × Granola: never '5 items'"
        );
        let mut one = receipt.clone();
        one.line_items.truncate(1);
        one.line_items[0].quantity = 1.0;
        assert_eq!(items(&one), "1 item");
        assert_eq!(
            first_items(&receipt, 3),
            "2 × Flat white, Oat latte, 3 × Almond croissant, +2 more"
        );
        let mut two = receipt.clone();
        two.line_items.truncate(2);
        assert_eq!(
            first_items(&two, 3),
            "2 × Flat white, Oat latte",
            "no '+0 more'"
        );
        assert_eq!(
            line(receipt.line_items.last().unwrap()),
            "1.5 × House blend beans, 250 g"
        );
        assert_eq!(plain(9.0, "USD"), "9.00");
        assert_eq!(plain(1125.0, "JPY"), "1125");
        assert_eq!(plain(0.1234, "CLF"), "0.1234");
        assert_eq!(
            sum_money([0.1, 0.2], "usd"),
            Some(Money::new(30, CurrencyCode::USD)),
            "exact in minor units, where doubles give 0.30000000000000004"
        );
        let at = instant("2026-10-22T16:07:00Z");
        assert_eq!(
            time_of(UnixSeconds(at), la(0)),
            "9:07 AM",
            "the device's own form"
        );
        assert_eq!(time_of(UnixSeconds(at), uk(0)), "16:07");
    }

    #[test]
    fn a_countdown_is_the_seconds_from_now_to_the_codes_end_and_never_below_none() {
        assert_eq!(
            seconds_left(UnixSeconds(1_000_552), UnixSeconds(1_000_000)),
            552
        );
        assert_eq!(
            seconds_left(UnixSeconds(1_000_000), UnixSeconds(1_000_000)),
            0
        );
        assert_eq!(
            seconds_left(UnixSeconds(1_000_000), UnixSeconds(1_000_009)),
            0,
            "a code past its end shows 0:00, not a negative time"
        );
    }

    #[test]
    fn confirming_counts_up_to_the_stores_threshold_and_no_further() {
        let words = |have, need| {
            confirming(&TillSale {
                confirmations: have,
                confirmations_required: need,
                ..phased(SalePhase::Confirming)
            })
        };
        assert_eq!(words(Some(1), Some(10)), "Confirming · 1 of 10");
        assert_eq!(
            words(Some(12), Some(10)),
            "Confirming · 10 of 10",
            "a read just before it turns paid never says more than it needs"
        );
        assert_eq!(
            words(Some(4), None),
            "Confirming",
            "with no threshold there is nothing to count to"
        );
        assert_eq!(words(None, Some(10)), "Confirming");
        assert_eq!(
            words(Some(0), Some(0)),
            "Confirming",
            "a store that takes no confirmation is paid on sight"
        );
    }

    #[test]
    fn the_amount_is_the_devices_money() {
        assert_eq!(amount(&waiting(), &money), "$24.50");
        assert_eq!(xmr(&waiting()), "0.1421 XMR");
    }
}
