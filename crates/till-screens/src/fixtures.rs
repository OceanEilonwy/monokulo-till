//! What the tests build sales, receipts, money and clocks from, as the
//! Android tests' helpers did.

use monokulo_client::SalePhase;
use orf_codec::Receipt;
use serde_json::json;
use till_pos::FoundSale;
use till_sales::{PaymentCode, TillSale};

use crate::time::DeviceClock;

/// A sale with every optional part left out: the tests' own helpers fill
/// in what they need.
pub(crate) fn till_sale(id: &str, amount: &str, currency: &str, xmr: &str) -> TillSale {
    TillSale {
        order_id: id.into(),
        amount: amount.into(),
        currency: currency.into(),
        reference: None,
        code: PaymentCode::plain("monero:4abc"),
        phase: SalePhase::Waiting,
        expires_at: 0,
        xmr: xmr.into(),
        address: "4abc".into(),
        found: None,
        receipt: None,
        customer_name: None,
        device_name: None,
        url: None,
        settled: false,
        created_at: 0,
        seen_at: None,
        paid_at: None,
        ended_at: None,
        closed_at: None,
        refunded_at: None,
        refund_pending_at: None,
        confirmations: None,
        confirmations_required: None,
        offered: false,
        kept_as_cash_at: None,
        paid_in_cash_at: None,
        extras: Default::default(),
    }
}

/// The point of sale's sale, as Kotlin's `FoundSale(id, amount, currency,
/// reference, orderId, createdAt, deviceId)`.
pub(crate) fn found(
    id: &str,
    amount_minor: i64,
    currency: &str,
    reference: Option<&str>,
    order_id: &str,
    created_at: &str,
) -> FoundSale {
    FoundSale {
        id: id.into(),
        amount_minor,
        currency: currency.into(),
        reference: reference.map(Into::into),
        order_id: Some(order_id.into()),
        created_at: created_at.into(),
        updated_at: None,
        status: None,
        device_id: None,
        device_name: None,
        cash: false,
        title: None,
        point_of_sale: monokulo_client::PointOfSale::Square,
    }
}

/// A receipt from The Bean Room: each line a description, quantity, unit
/// price and line total; each tax line a rate and an amount.
pub(crate) fn receipt(
    id: &str,
    issued: &str,
    lines: &[(&str, f64, f64, f64)],
    tax: &[(f64, f64)],
    total: f64,
    currency: &str,
) -> Receipt {
    let lines: Vec<_> = lines
        .iter()
        .map(|(description, quantity, unit_price, line_total)| {
            json!({"description": description, "quantity": quantity, "unit_price": unit_price, "line_total": line_total})
        })
        .collect();
    let mut value = json!({
        "orf_version": "0.1.0",
        "receipt": {"receipt_id": id, "issue_timestamp": issued, "receipt_type": "sale"},
        "merchant": {"legal_name": "The Bean Room", "country_code": "US"},
        "line_items": lines,
        "total": total,
        "currency": currency,
    });
    if !tax.is_empty() {
        let tax: Vec<_> = tax
            .iter()
            .map(|(rate, amount)| json!({"rate": rate, "tax_amount": amount}))
            .collect();
        value["tax"] = json!({ "tax_lines": tax });
    }
    serde_json::from_value(value).unwrap()
}

/// Money as Android's `Locale.US` writes it, for the currencies the tests
/// use.
pub(crate) fn money(minor: i64, currency: &str) -> String {
    match currency {
        "USD" => format!("${}.{:02}", minor / 100, minor % 100),
        "JPY" => format!("¥{minor}"),
        other => panic!("no test format for {other}"),
    }
}

/// Los Angeles in October (UTC−7) on a 12-hour clock, at `now`: the
/// Android tests' `Locale.US` and `America/Los_Angeles`.
pub(crate) fn la(now: i64) -> DeviceClock {
    DeviceClock {
        now,
        utc_offset: -7 * 3600,
        hours_24: false,
    }
}

/// UTC on a 24-hour clock: the Android tests' `Locale.UK` and `UTC`.
pub(crate) fn uk(now: i64) -> DeviceClock {
    DeviceClock {
        now,
        utc_offset: 0,
        hours_24: true,
    }
}

/// Unix seconds of an RFC 3339 time.
pub(crate) fn instant(text: &str) -> i64 {
    text.parse::<jiff::Timestamp>().unwrap().as_second()
}
