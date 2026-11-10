//! A Clover order as the receipt in the code, built on the till. Ported
//! test for test from the Android app's `CloverReceiptsTest.kt`.

#![cfg(feature = "clover")]

mod common;

use std::sync::Arc;

use common::*;
use monokulo_client::FixedClock;
use till_pos::clover::{
    CloverApi, CloverAuth, CloverConfig, CloverEnvironment, CloverMerchant, CloverOrder,
    CloverSource, EXPANDED_MAX, Elements, OrderLineItem, receipt,
};
use till_pos::{FoundSale, PointOfSale, SaleSource};

fn merchant() -> CloverMerchant {
    CloverMerchant {
        id: "MCLOVER00001".into(),
        name: "The Bean Room".into(),
        currency: "USD".parse().unwrap(),
        timezone: Some("America/Los_Angeles".into()),
        country: Some("US".into()),
    }
}

fn sale() -> FoundSale {
    FoundSale {
        id: "PAY_MONERO_1".into(),
        amount: till_types::Money::new(2205, "USD".parse().unwrap()),
        reference: None,
        order_id: Some("ORDER_CL_1".into()),
        created_at: "2026-10-28T08:56:00Z".into(),
        updated_at: None,
        status: None,
        device_id: Some("DEV_FRONT".into()),
        device_name: None,
        cash: false,
        title: None,
        point_of_sale: PointOfSale::Square,
    }
}

/// Clover's JSON as the till reads it, unknown fields passed over.
fn order() -> CloverOrder {
    serde_json::from_str(&fixture("clover/order.json")).unwrap()
}

fn line(id: &str, name: &str, price: i64) -> OrderLineItem {
    OrderLineItem {
        id: id.into(),
        name: Some(name.into()),
        price,
        unit_qty: None,
        note: None,
    }
}

#[tokio::test]
async fn a_sales_receipt_is_read_from_its_order_through_the_source_and_fits_in_the_code() {
    let stand = Stand::start().await;
    let tokens = MemoryTokens::holding(Some(tokens(
        "acc1",
        "2026-10-28T09:30:00Z",
        "ref1",
        "MCLOVER00001",
    )));
    let config = CloverConfig {
        base_url: Some(stand.url()),
        ..CloverConfig::new(
            "CLOVERAPP0001",
            CloverEnvironment::NorthAmerica,
            "https://till.monokulo.com/clover",
        )
    };
    let clock = Arc::new(FixedClock::at(at("2026-10-28T09:00:00Z")));
    let auth = CloverAuth::new(
        reqwest::Client::new(),
        config.clone(),
        tokens,
        clock.clone(),
    );
    let api = CloverApi::new(reqwest::Client::new(), config.clone(), Arc::new(auth));
    let source = CloverSource::new(
        api,
        config,
        merchant(),
        MemoryCursor::since(None),
        Arc::new(MemoryTenders::default()),
        clock,
    );
    stand.enqueue(status(200, &fixture("clover/order.json")));
    let receipt = source.order(&sale()).await.unwrap().receipt.unwrap();
    let request = stand.take_request().await;
    assert_eq!(
        request.url.path(),
        "/v3/merchants/MCLOVER00001/orders/ORDER_CL_1"
    );
    assert_eq!(query(&request, "expand").as_deref(), Some("lineItems"));

    let uri = orf_codec::query::append("monero:8Bx?tx_amount=0.1", &receipt)
        .expect("the receipt fits in the code");
    let decoded =
        serde_json::to_string(&orf_codec::query::from_uri(&uri).unwrap().unwrap()).unwrap();
    assert!(
        decoded.contains("Granola, per 100 g") && decoded.contains("The Bean Room"),
        "{decoded}"
    );
}

#[test]
fn equal_line_items_are_counted_onto_one_line_and_a_measured_one_has_its_decimal_quantity() {
    let receipt = receipt(&order(), &merchant(), &sale()).unwrap();
    let items = &receipt.line_items;
    assert_eq!(
        items
            .iter()
            .map(|l| l.description.as_str())
            .collect::<Vec<_>>(),
        [
            "Flat white",
            "Almond croissant",
            "Flat white, oat milk",
            "Granola, per 100 g"
        ]
    );
    assert_eq!(
        items.iter().map(|l| l.quantity).collect::<Vec<_>>(),
        [2.0, 1.0, 1.0, 2.5]
    );
    assert_eq!(
        items.iter().map(|l| l.unit_price).collect::<Vec<_>>(),
        [4.5, 3.75, 4.5, 1.2]
    );
    assert_eq!(
        items.iter().map(|l| l.line_total).collect::<Vec<_>>(),
        [9.0, 3.75, 4.5, 3.0]
    );
}

#[test]
fn the_tax_is_one_line_the_orders_total_less_its_lines_and_the_total_is_the_orders() {
    let receipt = receipt(&order(), &merchant(), &sale()).unwrap();
    let tax_lines = receipt.tax.unwrap().tax_lines.unwrap();
    assert_eq!(tax_lines.len(), 1);
    assert_eq!(tax_lines[0].tax_amount, 1.8);
    assert_eq!(tax_lines[0].rate, 0.08889, "1.80 on 20.25");
    assert_eq!(receipt.total, 22.05);
    assert_eq!(receipt.currency, "USD");
    assert_eq!(
        receipt.payment.unwrap().payment_method.as_deref(),
        Some("monero")
    );
    assert_eq!(receipt.extensions, None);
}

#[test]
fn the_receipt_names_the_merchant_the_order_and_its_time_in_the_merchants_zone() {
    let receipt = receipt(&order(), &merchant(), &sale()).unwrap();
    assert_eq!(
        receipt.receipt.receipt_id, "DER_CL_1",
        "Clover prints no number on the payment"
    );
    assert_eq!(receipt.receipt.issue_timestamp, "2026-10-28T01:55:58-07:00");
    assert_eq!(receipt.merchant.legal_name, "The Bean Room");
    assert_eq!(receipt.merchant.country_code, "US");
}

#[test]
fn an_order_for_less_than_its_lines_took_a_discount_off_and_has_no_tax() {
    let discounted = CloverOrder {
        total: 1900,
        ..order()
    };
    let receipt = receipt(&discounted, &merchant(), &sale()).unwrap();
    assert_eq!(receipt.tax, None);
    assert_eq!(receipt.total, 19.0);
    assert_eq!(
        serde_json::to_string(&receipt.extensions).unwrap(),
        r#"{"discount":{"amount":1.25}}"#
    );
}

#[test]
fn a_currency_without_cents_keeps_its_own_digits() {
    let yen = CloverOrder {
        id: "ORDER_JP_0001".into(),
        currency: Some("JPY".into()),
        total: 1100,
        created_time: 1793177758000,
        payment_state: None,
        line_items: Elements {
            elements: vec![line("L1", "Matcha", 500), line("L2", "Matcha", 500)],
        },
    };
    let tokyo = CloverMerchant {
        currency: "JPY".parse().unwrap(),
        timezone: Some("Asia/Tokyo".into()),
        country: Some("JP".into()),
        ..merchant()
    };
    let receipt = receipt(&yen, &tokyo, &sale()).unwrap();
    assert_eq!(receipt.line_items.len(), 1);
    assert_eq!(receipt.line_items[0].unit_price, 500.0);
    assert_eq!(receipt.line_items[0].quantity, 2.0);
    let tax_lines = receipt.tax.unwrap().tax_lines.unwrap();
    assert_eq!(tax_lines[0].tax_amount, 100.0);
    assert_eq!(tax_lines[0].rate, 0.1);
}

#[test]
fn an_order_of_as_many_line_items_as_clover_expands_may_be_cut_short_so_it_gets_the_plain_code() {
    // 140 coffees rung up, of which Clover sends the first 100: the other 40's money isn't tax.
    let coffees: Vec<OrderLineItem> = (0..EXPANDED_MAX)
        .map(|i| line(&format!("L{i}"), "Drip coffee", 300))
        .collect();
    let cut = CloverOrder {
        id: "ORDER_BIG_01".into(),
        currency: Some("USD".into()),
        total: 140 * 300,
        created_time: 1793177758000,
        payment_state: None,
        line_items: Elements {
            elements: coffees.clone(),
        },
    };
    assert_eq!(receipt(&cut, &merchant(), &sale()), None);
    let whole = CloverOrder {
        line_items: Elements {
            elements: coffees[..coffees.len() - 1].to_vec(),
        },
        total: 99 * 300,
        ..cut
    };
    let receipt = receipt(&whole, &merchant(), &sale()).unwrap();
    assert_eq!(receipt.line_items.len(), 1);
    assert_eq!(
        receipt.line_items[0].quantity, 99.0,
        "one fewer is the whole order"
    );
}
