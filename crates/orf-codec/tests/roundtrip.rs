//! Round trips of ORF's own example receipts (CC0, from
//! github.com/openreceiptformat/orf-spec at 9f64924) and receipts a till would
//! make, checked against the ORF 0.1 JSON Schema.

use orf_codec::{EncodeError, Receipt, decode, decode_text, encode, encode_text};
use serde_json::{Value, json};

fn schema() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(include_str!("orf/orf-0.1.json")).unwrap();
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .unwrap()
}

fn assert_round_trips(json: &Value) -> usize {
    let receipt: Receipt = serde_json::from_value(json.clone()).expect("a valid ORF document");
    let bytes = encode(&receipt).expect("encodes");
    let back = decode(&bytes).expect("decodes");
    assert_eq!(back, receipt);
    let back_json = serde_json::to_value(&back).unwrap();
    assert!(
        schema().is_valid(&back_json),
        "decoded receipt fails the ORF schema: {back_json:#}"
    );
    assert_eq!(
        decode_text(&encode_text(&receipt).unwrap()).unwrap(),
        receipt
    );
    bytes.len()
}

#[test]
fn orf_examples_round_trip() {
    for (name, text) in [
        ("coffee-shop", include_str!("orf/coffee-shop.json")),
        ("restaurant", include_str!("orf/restaurant.json")),
        ("retail", include_str!("orf/retail.json")),
    ] {
        let json: Value = serde_json::from_str(text).unwrap();
        assert!(schema().is_valid(&json), "{name} should be a valid example");
        let size = assert_round_trips(&json);
        println!("{name}: {} bytes of JSON, {size} bytes encoded", text.len());
    }
}

fn till_receipt() -> Value {
    json!({
        "orf_version": "0.1.0",
        "receipt": { "receipt_id": "#88213", "issue_timestamp": "2026-10-19T17:47:00Z", "receipt_type": "sale" },
        "merchant": { "legal_name": "Greenleaf Market", "country_code": "US" },
        "line_items": [
            { "line_item_id": "1", "description": "Organic bananas 1.24 kg @ 0.69/kg", "quantity": 1.24, "unit_price": 0.69, "line_total": 0.86 },
            { "line_item_id": "2", "description": "Greek yogurt 500 g", "quantity": 2, "unit_price": 4.49, "line_total": 8.98 },
            { "line_item_id": "3", "description": "Coffee beans 1 kg", "quantity": 1, "unit_price": 21.99, "line_total": 21.99 }
        ],
        "tax": { "tax_lines": [ { "rate": 0.0888, "tax_amount": 2.81 } ] },
        "payment": { "payment_method": "monero" },
        "total": 34.64,
        "currency": "USD",
        "extensions": { "tip": { "amount": 1.0 } }
    })
}

#[test]
fn a_till_receipt_round_trips_small() {
    let size = assert_round_trips(&till_receipt());
    assert!(size < 120, "{size} bytes");
}

#[test]
fn values_that_disagree_with_the_derived_ones_are_kept() {
    let mut r = till_receipt();
    r["line_items"][0]["line_total"] = json!(0.85); // rounded down by the till
    r["tax"]["tax_lines"][0]["tax_amount"] = json!(2.80);
    r["total"] = json!(40.0);
    r["line_items"][1]["line_item_id"] = json!("SKU-77");
    r["receipt"]["issue_timestamp"] = json!("2026-10-19T17:47:00+05:45");
    r["extensions"] = json!({ "tip": { "amount": 2, "percentage": 0.1 }, "note": "thanks" });
    assert_round_trips(&r);
}

#[test]
fn other_currencies_and_refunds_round_trip() {
    let mut r = till_receipt();
    r["currency"] = json!("JPY");
    r["receipt"]["receipt_type"] = json!("refund");
    r["receipt"]["status"] = json!("issued");
    r["line_items"] =
        json!([{ "description": "Matcha", "quantity": -1, "unit_price": 980, "line_total": -980 }]);
    r["tax"] = json!({ "tax_scheme": "JP_CT", "tax_lines": [{ "rate": 0.1, "base_amount": -980, "tax_amount": -98, "jurisdiction": "JP" }] });
    r["total"] = json!(-1078);
    r["extensions"] = json!({});
    assert_round_trips(&r);
}

#[test]
fn what_cant_be_represented_is_refused() {
    let refuse = |r: Value| encode(&serde_json::from_value(r).unwrap()).unwrap_err();

    let mut r = till_receipt();
    r["receipt"]["issue_timestamp"] = json!("2026-10-19T17:47:00.25Z");
    assert!(matches!(refuse(r), EncodeError::Unsupported { .. }));

    let mut r = till_receipt();
    r["line_items"][0]["description"] = json!("two\nlines");
    assert!(matches!(refuse(r), EncodeError::Unsupported { .. }));

    let mut r = till_receipt();
    r["line_items"][0]["unit_price"] = json!(0.695);
    assert!(matches!(refuse(r), EncodeError::Amount { .. }));

    let mut r = till_receipt();
    r["merchant"]["shoe_size"] = json!(9);
    assert!(
        serde_json::from_value::<Receipt>(r).is_err(),
        "unknown fields are refused, not dropped"
    );
}

#[test]
fn damaged_input_is_an_error_not_a_panic() {
    let receipt: Receipt = serde_json::from_value(till_receipt()).unwrap();
    let bytes = encode(&receipt).unwrap();
    for cut in 0..bytes.len() {
        let _ = decode(&bytes[..cut]);
    }
    for i in 0..bytes.len() {
        let mut b = bytes.clone();
        b[i] ^= 0x5a;
        let _ = decode(&b);
    }
    assert!(decode_text("12x").is_err());
}
