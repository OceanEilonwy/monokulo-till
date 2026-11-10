//! A Clover order as a receipt (Android: `CloverReceipts.kt`).

use orf_codec::{LineItem, Merchant, Payment, Receipt, ReceiptInfo, ReceiptType, Tax, TaxLine};
use serde_json::{Map, json};

use super::{CloverMerchant, CloverOrder, OrderLineItem};
use crate::FoundSale;
use crate::money::{major, minor_digits, scaled};
use crate::receipts::{div_half_up, one_line, receipt_timestamp, take_last};

/// The most elements Clover returns of an expanded collection, such as an
/// order's `lineItems`.
pub const EXPANDED_MAX: usize = 100;

/// A tax rate to a thousandth of a percent: 8.875% is 0.08875.
const RATE_DIGITS: u32 = 5;

/// `order` as an Open Receipt Format receipt (docs/design.md §4), built on
/// the till (decision 134 in docs/decisions/android.md).
///
/// Clover repeats a line item once per unit, so equal ones are counted onto
/// one line; an item sold by measure is one line of its `unitQty`
/// thousandths. The order carries its total, tax included, but its tax only
/// per line behind another expand: the receipt shows one tax line, the
/// total less the lines, as Clover's paper receipt does. A total below the
/// lines is a discount, in `extensions`, with no tax line.
///
/// `None` when the merchant's country isn't known (a receipt must name
/// it), or when the order has [`EXPANDED_MAX`] line items or more. Clover
/// may have cut such a list, and the total less the lines would then print
/// the lost items' money as tax. A receipt that lists only some of what was
/// bought, under a total its lines don't add up to, is a wrong receipt;
/// the plain code still takes the payment, and the merchant's own receipt
/// stays whole.
pub fn receipt(
    order: &CloverOrder,
    merchant: &CloverMerchant,
    sale: &FoundSale,
) -> Option<Receipt> {
    let country = merchant.country.as_ref()?;
    if order.line_items.elements.len() >= EXPANDED_MAX {
        return None;
    }
    let currency = order.currency.as_ref().unwrap_or(&merchant.currency);
    let digits = minor_digits(currency);
    let lines = lines(&order.line_items.elements);
    let sum: i64 = lines.iter().map(|l| l.total_minor).sum();
    let rest = order.total - sum;
    let tax = (rest > 0 && sum > 0).then(|| Tax {
        tax_scheme: None,
        tax_lines: Some(vec![TaxLine {
            rate: scaled(
                div_half_up(i128::from(rest) * 10i128.pow(RATE_DIGITS), i128::from(sum)),
                RATE_DIGITS,
            ),
            base_amount: None,
            tax_amount: major(rest, digits),
            jurisdiction: None,
        }]),
    });
    // Less than its lines: a discount the order took off.
    let discount = (rest < 0).then(|| {
        let mut extensions = Map::new();
        extensions.insert("discount".into(), json!({"amount": major(-rest, digits)}));
        extensions
    });
    Some(Receipt {
        orf_version: orf_codec::ORF_VERSION.into(),
        receipt: ReceiptInfo {
            receipt_id: sale
                .reference
                .clone()
                .unwrap_or_else(|| take_last(&order.id, 8)),
            issue_timestamp: receipt_timestamp(
                crate::time::from_millis(order.created_time),
                merchant.timezone.as_deref(),
            ),
            receipt_type: ReceiptType::Sale,
            status: None,
        },
        merchant: Merchant {
            merchant_id: None,
            legal_name: one_line(&merchant.name),
            display_name: None,
            country_code: country.clone(),
            tax_identifiers: None,
        },
        customer: None,
        line_items: lines
            .into_iter()
            .map(|line| LineItem {
                line_item_id: None,
                description: line.description,
                quantity: line.quantity,
                unit_price: major(line.price_minor, digits),
                net_amount: None,
                line_total: major(line.total_minor, digits),
            })
            .collect(),
        tax,
        payment: Some(Payment {
            payment_method: Some("monero".into()),
            provider: None,
            authorization_code: None,
        }),
        total: major(order.total, digits),
        currency: currency.clone(),
        trust: None,
        extensions: discount,
    })
}

/// One receipt line, in minor units.
#[derive(Debug, PartialEq)]
struct Line {
    description: String,
    quantity: f64,
    price_minor: i64,
    total_minor: i64,
}

/// What makes two lines one: the same item at the same price with the same
/// note, or, for a measured item, its own id.
#[derive(PartialEq)]
enum Key<'a> {
    Measured(&'a str),
    Counted(Option<&'a str>, i64, Option<&'a str>),
}

/// Equal per-unit items counted onto one line, in the order they were rung
/// up; a measured item a line of its own.
fn lines(items: &[OrderLineItem]) -> Vec<Line> {
    let mut counted: Vec<(Key<'_>, Line)> = Vec::new();
    for item in items {
        let description = describe(item);
        let (key, line) = match item.unit_qty {
            Some(thousandths) => (
                Key::Measured(&item.id),
                Line {
                    description,
                    quantity: scaled(i128::from(thousandths), 3),
                    price_minor: item.price,
                    total_minor: div_half_up(i128::from(item.price) * i128::from(thousandths), 1000)
                        .clamp(i128::from(i64::MIN), i128::from(i64::MAX))
                        as i64,
                },
            ),
            None => (
                Key::Counted(item.name.as_deref(), item.price, item.note.as_deref()),
                Line {
                    description,
                    quantity: 1.0,
                    price_minor: item.price,
                    total_minor: item.price,
                },
            ),
        };
        match counted.iter_mut().find(|(k, _)| *k == key) {
            Some((Key::Counted(..), same)) => {
                same.quantity += 1.0;
                same.total_minor += item.price;
            }
            // A measured item's id seen again: the later one stands, where
            // the first stood.
            Some((Key::Measured(_), same)) => *same = line,
            None => counted.push((key, line)),
        }
    }
    counted.into_iter().map(|(_, line)| line).collect()
}

/// "Latte, extra hot": the item and the cashier's note on it, on one line.
fn describe(item: &OrderLineItem) -> String {
    let name = item
        .name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or("Item");
    match item.note.as_deref().filter(|n| !n.trim().is_empty()) {
        Some(note) => one_line(&format!("{name}, {note}")),
        None => one_line(name),
    }
}
