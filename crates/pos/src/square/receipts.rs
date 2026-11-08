//! A Square order as a receipt (Android: `SquareReceipts.kt`).

use orf_codec::{LineItem, Merchant, Payment, Receipt, ReceiptInfo, ReceiptType, Tax, TaxLine};
use serde_json::{Map, json};

use super::api::{self, SquareLocation, SquareOrder};
use crate::FoundSale;
use crate::money::{major, minor_digits};
use crate::receipts::{Decimal, one_line, receipt_timestamp, take_last};

/// `order` as an Open Receipt Format receipt (docs/design.md §4), built on
/// the till: what was bought never leaves it except in the code
/// (decision 17 in docs/decisions/android.md).
///
/// Each line's unit price is the one that makes quantity × price the line's
/// gross total when there is one in whole minor units, else its list price.
/// Taxes with a percentage and an amount are listed; a tip and a discount go
/// in `extensions`. The time is the order's, in the location's zone.
///
/// `None` when the order's time can't be read.
pub fn receipt(
    order: &SquareOrder,
    location: &SquareLocation,
    sale: &FoundSale,
) -> Option<Receipt> {
    let digits = minor_digits(&order.total_money.currency);
    let line_items = order
        .line_items
        .iter()
        .map(|line| {
            let quantity = Decimal::parse(&line.quantity).unwrap_or(Decimal::ONE);
            let gross = line
                .gross_sales_money
                .as_ref()
                .map(|m| m.amount)
                .or_else(|| {
                    line.base_price_money
                        .as_ref()
                        .map(|m| m.amount.wrapping_mul(quantity.trunc()))
                });
            let total = gross.unwrap_or(0);
            let unit = exact_unit(total, quantity)
                .map(|unit| major(unit, digits))
                .or_else(|| {
                    line.base_price_money
                        .as_ref()
                        .map(|m| major(m.amount, digits))
                })
                .unwrap_or_else(|| major(total, digits));
            LineItem {
                line_item_id: None,
                description: describe(line),
                quantity: quantity.to_f64(),
                unit_price: unit,
                net_amount: None,
                line_total: major(total, digits),
            }
        })
        .collect();
    let tax_lines: Vec<TaxLine> = order
        .taxes
        .iter()
        .filter_map(|tax| {
            let rate = Decimal::parse(tax.percentage.as_deref()?)?;
            let applied = tax.applied_money.as_ref()?;
            Some(TaxLine {
                rate: rate.shifted(2).to_f64(),
                base_amount: None,
                tax_amount: major(applied.amount, digits),
                jurisdiction: None,
            })
        })
        .collect();
    let mut extensions = Map::new();
    for (key, money) in [
        ("tip", &order.total_tip_money),
        ("discount", &order.total_discount_money),
    ] {
        if let Some(money) = money.as_ref().filter(|m| m.amount > 0) {
            extensions.insert(key.into(), json!({"amount": major(money.amount, digits)}));
        }
    }
    let created = crate::time::parse(&order.created_at)?;
    Some(Receipt {
        orf_version: "0.1.0".into(),
        receipt: ReceiptInfo {
            receipt_id: sale
                .reference
                .clone()
                .unwrap_or_else(|| take_last(&order.id, 8)),
            issue_timestamp: receipt_timestamp(created, location.timezone.as_deref()),
            receipt_type: ReceiptType::Sale,
            status: None,
        },
        merchant: Merchant {
            merchant_id: None,
            legal_name: one_line(location.business_name.as_deref().unwrap_or(&location.name)),
            display_name: (location.business_name.as_deref() != Some(location.name.as_str()))
                .then(|| one_line(&location.name)),
            country_code: location.country.clone(),
            tax_identifiers: None,
        },
        customer: None,
        line_items,
        tax: (!tax_lines.is_empty()).then_some(Tax {
            tax_scheme: None,
            tax_lines: Some(tax_lines),
        }),
        payment: Some(Payment {
            payment_method: Some("monero".into()),
            provider: None,
            authorization_code: None,
        }),
        total: major(order.total_money.amount, digits),
        currency: order.total_money.currency.clone(),
        trust: None,
        extensions: (!extensions.is_empty()).then_some(extensions),
    })
}

/// The unit price, in minor units, that makes `quantity` × it exactly
/// `total`; `None` when no whole number of minor units does (or the
/// quantity is zero).
fn exact_unit(total: i64, quantity: Decimal) -> Option<i64> {
    if quantity.units == 0 {
        return None;
    }
    let scaled = i128::from(total).checked_mul(10i128.checked_pow(quantity.scale)?)?;
    if scaled % quantity.units != 0 {
        return None;
    }
    i64::try_from(scaled / quantity.units).ok()
}

/// "Flat white (Large), Oat milk": the item, its variation unless it's
/// Square's default "Regular", and its modifiers, on one line.
fn describe(line: &api::LineItem) -> String {
    let name = line
        .name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or("Item");
    let mut parts = vec![match line
        .variation_name
        .as_deref()
        .filter(|v| !v.trim().is_empty() && *v != "Regular")
    {
        Some(variation) => format!("{name} ({variation})"),
        None => name.to_owned(),
    }];
    parts.extend(
        line.modifiers
            .iter()
            .filter_map(|m| m.name.as_deref())
            .filter(|n| !n.trim().is_empty())
            .map(str::to_owned),
    );
    one_line(&parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).unwrap()
    }

    #[test]
    fn a_unit_price_is_exact_or_none() {
        assert_eq!(exact_unit(900, d("2")), Some(450));
        assert_eq!(exact_unit(1000, d("3")), None);
        assert_eq!(exact_unit(375, d("1.5")), Some(250));
        assert_eq!(exact_unit(100, d("0")), None, "Kotlin divided by zero here");
    }
}
