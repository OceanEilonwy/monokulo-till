//! Ported from the Android app's `SaleEventTest`.

use monokulo_client::payment_uri;
use monokulo_client::{Order, SaleEvent, SalePhase};
use reqwest::Url;

#[test]
fn an_event_from_the_stream_decodes_ignoring_fields_it_doesnt_know_its_order_link_resolved() {
    let event = SaleEvent::decode(
        r##"{"event":"sale.paid","order_id":"ord_1","amount":"24.50","currency":"USD",
           "address":"8Bx","xmr_amount_piconero":141900000000,"merchant_order_id":"#1042",
           "expires_at":1761312000,"device_name":"Front counter","at":1761310260,"later":true,
           "_links":{"mk:order":{"href":"/monokulo/api/v1/orders/ord_1"}}}"##,
        &Url::parse("https://example.com/monokulo/api/v1/events").unwrap(),
    )
    .unwrap();
    assert_eq!(
        event.order_url.as_deref(),
        Some("https://example.com/monokulo/api/v1/orders/ord_1")
    );
    assert_eq!(event.phase(), Some(SalePhase::Paid));
    assert_eq!(event.merchant_order_id.as_deref(), Some("#1042"));
    assert!(event.phase().unwrap().is_final());
}

#[test]
fn every_event_name_has_its_phase() {
    for (name, phase) in [
        ("sale.waiting", SalePhase::Waiting),
        ("sale.seen", SalePhase::Seen),
        ("sale.confirming", SalePhase::Confirming),
        ("sale.paid", SalePhase::Paid),
        ("sale.partial", SalePhase::Partial),
        ("sale.expired", SalePhase::Expired),
        ("sale.double_spend", SalePhase::DoubleSpend),
    ] {
        assert_eq!(SalePhase::of_event(name), Some(phase));
    }
    assert_eq!(SalePhase::of_event("ready"), None);
    assert_eq!(
        SalePhase::of_event("sale.cancelled"),
        None,
        "Monokulo never sends one: only the POS cancels, and only its own orders"
    );
}

#[test]
fn a_code_counts_down_until_some_of_its_money_is_seen_and_part_paid_still_counts_down() {
    let counting: Vec<_> = SalePhase::ALL
        .into_iter()
        .filter(|p| p.counts_down())
        .collect();
    assert_eq!(counting, [SalePhase::Waiting, SalePhase::Partial]);
}

#[test]
fn a_status_puts_a_sale_in_its_phase() {
    for (status, phase) in [
        ("pending", SalePhase::Waiting),
        ("unconfirmed", SalePhase::Seen),
        ("confirming", SalePhase::Confirming),
        ("overpaid", SalePhase::Paid),
        ("paid", SalePhase::Paid),
        ("partial", SalePhase::Partial),
        ("expired", SalePhase::Expired),
        ("cancelled", SalePhase::Cancelled),
    ] {
        assert_eq!(Order::phase_of(status), phase, "{status}");
    }
}

#[test]
fn payment_uris_carry_the_amount_in_xmr_without_trailing_zeros() {
    assert_eq!(payment_uri::xmr(80_370_584_829), "0.080370584829");
    assert_eq!(payment_uri::xmr(2_000_000_000_000), "2");
    assert_eq!(payment_uri::xmr(500_000_000_000), "0.5");
    assert_eq!(
        payment_uri::build("8Bx", 500_000_000_000),
        "monero:8Bx?tx_amount=0.5"
    );
}
