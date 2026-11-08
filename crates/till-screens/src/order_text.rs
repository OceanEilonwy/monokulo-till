//! What was bought, from this till's own receipt (decision 63 of
//! android.md): the order card on a sale and its page, and the whole
//! order's words (Android: `ui/OrderPage.kt`'s `OrderText`, `ui/Till.kt`'s
//! `OrderCard`).

use monokulo_client::PointOfSale;
use orf_codec::{LineItem, Receipt};
use till_sales::TillSale;

use crate::decimal::Decimal;
use crate::time::{DeviceClock, time};
use crate::{pos_text, sale_text};

/// "8 lines" or "1 line".
pub fn lines(receipt: &Receipt) -> String {
    match receipt.line_items.len() {
        1 => "1 line".into(),
        n => format!("{n} lines"),
    }
}

/// "2 ×", and "1 ×" too: the order's quantity column.
pub fn quantity(item: &LineItem) -> String {
    format!("{} ×", sale_text::quantity(item.quantity))
}

/// "8.875%" from a tax line's fraction (0.08875), to at most three
/// decimals; "10%" from 0.1.
pub fn rate(fraction: f64) -> String {
    let percent = Decimal::of(fraction)
        .and_then(|d| d.times_ten_to(2))
        .and_then(|d| d.round(3))
        .map_or_else(|| format!("{}", fraction * 100.0), |d| d.stripped().plain());
    format!("{percent}%")
}

/// What the lines come to before tip, discount and tax.
pub fn subtotal(receipt: &Receipt) -> f64 {
    let mut sum = Decimal::of(0.0);
    for item in &receipt.line_items {
        sum = sum.and_then(|sum| sum.plus(Decimal::of(item.line_total)?));
    }
    sum.map_or_else(
        || receipt.line_items.iter().map(|item| item.line_total).sum(),
        Decimal::to_f64,
    )
}

/// The receipt's tip and discount (its `extensions`, as the till wrote
/// them), each a name and its amount as the order shows it: "2.00", or
/// "−1.50" off.
pub fn extras(receipt: &Receipt) -> Vec<(String, String)> {
    let Some(extensions) = &receipt.extensions else {
        return Vec::new();
    };
    let amount_of = |key: &str| {
        extensions
            .get(key)?
            .get("amount")?
            .as_f64()
            .filter(|&amount| amount > 0.0)
    };
    let mut extras = Vec::new();
    if let Some(tip) = amount_of("tip") {
        extras.push(("Tip".into(), sale_text::plain(tip, &receipt.currency)));
    }
    if let Some(discount) = amount_of("discount") {
        extras.push((
            "Discount".into(),
            format!("−{}", sale_text::plain(discount, &receipt.currency)),
        ));
    }
    extras
}

/// "79.21 USD": the total with the currency's code.
pub fn total(receipt: &Receipt) -> String {
    sale_text::total(receipt)
}

/// "Sale #k4Rt · Square receipt k4Rt · 4:07 PM": the sale, the point of
/// sale's receipt when it gave one, and when the receipt was issued.
pub fn subtitle(
    sale: &TillSale,
    receipt: &Receipt,
    pos: Option<PointOfSale>,
    clock: DeviceClock,
) -> String {
    let issued = receipt
        .receipt
        .issue_timestamp
        .parse::<jiff::Timestamp>()
        .ok()
        .map(|at| time(at.as_second(), clock));
    let mut parts = vec![sale_text::title(sale)];
    if sale.reference.is_some() {
        parts.push(pos_text::receipt_label(pos, &receipt.receipt.receipt_id));
    }
    parts.extend(issued);
    parts.join(" · ")
}

/// One line of the order card: "2 × Flat white" and "9.00".
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct OrderLine {
    pub label: String,
    pub price: String,
}

/// The order card (decision 63 of android.md). Folded, its one row says
/// whose receipt it is, how many items and the total; unfolded, each line
/// and its price, then the tax and the total. Whether it's folded is the
/// platform's (it opens by itself only when the page has room).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct OrderCard {
    /// "Square receipt bP9m · 15 items · 79.21 USD".
    pub summary: String,
    pub lines: Vec<OrderLine>,
    /// "+5 more", which opens the whole order, on a live sale's card.
    pub more: Option<String>,
    /// "Tax 1.13", or "Total" when the receipt has no tax.
    pub foot: String,
    /// "13.88 USD".
    pub total: String,
}

/// `receipt`'s card, named by `pos`: on a live sale (`live`) a long order
/// shows three lines and "+N more"; a sale's page lists every line.
pub fn order_card(receipt: &Receipt, pos: Option<PointOfSale>, live: bool) -> OrderCard {
    let (shown, more) = if live {
        sale_text::shown_lines(receipt, 4)
    } else {
        (receipt.line_items.as_slice(), 0)
    };
    OrderCard {
        summary: sale_text::receipt_summary(receipt, pos),
        lines: shown
            .iter()
            .map(|item| OrderLine {
                label: sale_text::line(item),
                price: sale_text::plain(item.line_total, &receipt.currency),
            })
            .collect(),
        more: (more > 0).then(|| format!("+{more} more")),
        foot: sale_text::tax(receipt).unwrap_or_else(|| "Total".into()),
        total: sale_text::total(receipt),
    }
}

/// Ported from Android's `OrderPageTest` (its four on the order; the fifth,
/// on the code's line, is [`crate::code`]'s) and `ShiftListTest`
/// (`the order card lists four lines, else three and how many more`).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{found, la, receipt as make_receipt, till_sale, uk};
    use crate::shift::tests::sale_of;
    use monokulo_client::SalePhase;
    use serde_json::json;

    /// The design's order: 15 items on 8 lines.
    fn design() -> Receipt {
        make_receipt(
            "k4Rt",
            "2026-10-22T16:07:00-07:00",
            &[
                ("Flat white", 2.0, 4.5, 9.0),
                ("Oat latte", 1.0, 5.25, 5.25),
                ("Almond croissant", 3.0, 3.75, 11.25),
                ("Banana bread", 1.0, 4.5, 4.5),
                ("Cold brew", 2.0, 5.0, 10.0),
                ("Chai", 1.0, 4.75, 4.75),
                ("Choc chip cookie", 4.0, 2.5, 10.0),
                ("House blend beans, 250 g", 1.0, 18.0, 18.0),
            ],
            &[(0.08875, 6.46)],
            79.21,
            "USD",
        )
    }

    fn sale(reference: Option<&str>) -> TillSale {
        TillSale {
            reference: reference.map(Into::into),
            found: Some(found(
                "p1",
                7921,
                "USD",
                reference,
                "ord",
                "2026-10-22T23:07:00Z",
            )),
            receipt: Some(design()),
            ..till_sale("o1", "79.21", "USD", "0.31684")
        }
    }

    #[test]
    fn the_order_counts_items_and_lines() {
        let receipt = design();
        assert_eq!(sale_text::items(&receipt), "15 items");
        assert_eq!(lines(&receipt), "8 lines");
        let mut one = receipt.clone();
        one.line_items = vec![receipt.line_items[1].clone()];
        assert_eq!(sale_text::items(&one), "1 item");
        assert_eq!(lines(&one), "1 line");
        assert_eq!(
            quantity(&receipt.line_items[1]),
            "1 ×",
            "the order's column says 1 × too"
        );
        let beans = make_receipt("b", "x", &[("Beans", 0.5, 30.0, 15.0)], &[], 15.0, "USD");
        assert_eq!(quantity(&beans.line_items[0]), "0.5 ×");
    }

    #[test]
    fn tax_rates_print_to_three_decimals_at_most() {
        assert_eq!(rate(0.08875), "8.875%");
        assert_eq!(rate(0.1), "10%");
        assert_eq!(rate(0.0825), "8.25%");
        assert_eq!(rate(0.0712345), "7.123%");
    }

    #[test]
    fn the_totals_are_the_receipts() {
        let receipt = design();
        assert_eq!(sale_text::plain(subtotal(&receipt), "USD"), "72.75");
        assert_eq!(total(&receipt), "79.21 USD");
        assert!(extras(&receipt).is_empty());
        let mut tipped = receipt.clone();
        tipped.extensions = Some(
            json!({"tip": {"amount": 2.0}, "discount": {"amount": 1.5}})
                .as_object()
                .unwrap()
                .clone(),
        );
        assert_eq!(
            extras(&tipped),
            [
                ("Tip".to_owned(), "2.00".to_owned()),
                ("Discount".to_owned(), "−1.50".to_owned())
            ]
        );
        let yen = make_receipt(
            "y",
            "x",
            &[("Matcha", 1.0, 1200.0, 1200.0)],
            &[],
            1200.0,
            "JPY",
        );
        assert_eq!(total(&yen), "1200 JPY");
    }

    #[test]
    fn the_subtitle_names_the_sale_its_square_receipt_and_when_it_was_issued() {
        let square = Some(PointOfSale::Square);
        let receipt = design();
        assert_eq!(
            subtitle(&sale(Some("k4Rt")), &receipt, square, la(0)),
            "Sale #k4Rt · Square receipt k4Rt · 4:07 PM"
        );
        // Kotlin's test read 16:07 in Los Angeles on a 24-hour clock.
        let la_24 = DeviceClock {
            hours_24: true,
            ..la(0)
        };
        assert_eq!(
            subtitle(&sale(Some("k4Rt")), &receipt, square, la_24),
            "Sale #k4Rt · Square receipt k4Rt · 16:07"
        );
        assert_eq!(
            subtitle(&sale(None), &receipt, square, la(0)),
            "Monero sale · 4:07 PM",
            "no Square reference, no Square receipt"
        );
        assert_eq!(
            subtitle(&sale(Some("k4Rt")), &receipt, square, uk(0)),
            "Sale #k4Rt · Square receipt k4Rt · 23:07"
        );
    }

    #[test]
    fn the_order_card_lists_four_lines_else_three_and_how_many_more() {
        let receipt = |n: usize| {
            let names: Vec<String> = (0..n).map(|i| format!("Item {i}")).collect();
            let lines: Vec<(&str, f64, f64, f64)> = names
                .iter()
                .map(|name| (name.as_str(), 1.0, 1.0, 1.0))
                .collect();
            make_receipt(
                "k4Rt",
                "2026-10-22T16:07:00-07:00",
                &lines,
                &[],
                n as f64,
                "USD",
            )
        };
        let four = receipt(4);
        let (shown, more) = sale_text::shown_lines(&four, 4);
        assert_eq!((shown.len(), more), (4, 0));
        let eight = receipt(8);
        let (shown, more) = sale_text::shown_lines(&eight, 4);
        assert_eq!((shown.len(), more), (3, 5));
        assert_eq!(
            sale_text::tax(&receipt(1)),
            None,
            "no tax lines: the last row is the total"
        );
        assert_eq!(sale_text::total(&receipt(1)), "1.00 USD");
        assert_eq!(
            sale_text::receipt_summary(&receipt(8), Some(PointOfSale::Square)),
            "Square receipt k4Rt · 8 items · 8.00 USD"
        );
        assert_eq!(
            sale_text::receipt_summary(&receipt(1), Some(PointOfSale::Clover)),
            "Clover order k4Rt · 1 item · 1.00 USD",
            "folded, the card says whose and how much"
        );
        let two = receipt(2);
        let taxed = make_receipt(
            "k4Rt",
            "2026-10-22T16:07:00-07:00",
            &[("Item 0", 1.0, 1.0, 1.0), ("Item 1", 1.0, 1.0, 1.0)],
            &[(0.05, 0.5), (0.08875, 0.63)],
            two.total,
            "USD",
        );
        assert_eq!(sale_text::tax(&taxed).as_deref(), Some("Tax 1.13"));
        assert_eq!(
            sale_text::hint(&sale_of("w").build(), 892),
            "Scan with any Monero wallet · 14:52 left"
        );
        assert_eq!(
            sale_text::hint(&sale_of("w").phase(SalePhase::Partial).build(), 552),
            "Ask the customer to pay the rest to the same code · 9:12 left"
        );
        assert_eq!(
            sale_text::hint(&sale_of("w").phase(SalePhase::Seen).build(), 0),
            "The customer can go: it's being confirmed"
        );
        let paid = sale_of("w").phase(SalePhase::Paid).paid_at(990).build();
        assert_eq!(
            sale_text::hint(&paid, 0),
            format!("{} received", sale_text::xmr(&paid)),
            "paid, the foot says what came in, not the clock"
        );
        // The card itself.
        let card = order_card(&eight, Some(PointOfSale::Square), true);
        assert_eq!(card.lines.len(), 3);
        assert_eq!(card.more.as_deref(), Some("+5 more"));
        assert_eq!(card.foot, "Total");
        assert_eq!(order_card(&eight, None, false).lines.len(), 8);
        assert_eq!(order_card(&eight, None, false).more, None);
    }
}
