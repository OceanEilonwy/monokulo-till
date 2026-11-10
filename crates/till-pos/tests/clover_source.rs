//! Clover's API through the till's Clover source, against a stand-in
//! Clover. Ported test for test from the Android app's
//! `CloverSourceTest.kt`.

#![cfg(feature = "clover")]

mod common;

use std::sync::Arc;

use common::*;
use monokulo_client::FixedClock;
use till_pos::clover::{
    CloverApi, CloverAuth, CloverConfig, CloverEnvironment, CloverMerchant, CloverSource,
    TenderState,
};
use till_pos::{CursorStore, FoundSale, PointOfSale, Refunds, SaleSource};
use wiremock::ResponseTemplate;

struct Fixture {
    stand: Stand,
    tokens: Arc<MemoryTokens>,
    clock: FixedClock,
    config: CloverConfig,
    api: CloverApi,
    merchant: CloverMerchant,
    tenders: Arc<MemoryTenders>,
}

fn bean_room() -> CloverMerchant {
    CloverMerchant {
        id: till_pos::clover::CloverMerchantId::new("MCLOVER00001").unwrap(),
        name: "The Bean Room".into(),
        currency: "USD".parse().unwrap(),
        timezone: Some("America/Los_Angeles".into()),
        country: Some("US".into()),
    }
}

impl Fixture {
    async fn start() -> Fixture {
        let stand = Stand::start().await;
        let tokens = MemoryTokens::holding(Some(tokens(
            "acc1",
            "2026-10-28T09:30:00Z",
            "ref1",
            "MCLOVER00001",
        )));
        let clock = FixedClock::at(at("2026-10-28T09:00:00Z"));
        let config = CloverConfig {
            user_agent: "Till/0.3.0".into(),
            base_url: Some(stand.url()),
            ..CloverConfig::new(
                "CLOVERAPP0001",
                CloverEnvironment::NorthAmerica,
                "https://till.monokulo.com/clover",
            )
        };
        let auth = CloverAuth::new(
            reqwest::Client::new(),
            config.clone(),
            tokens.clone(),
            Arc::new(clock.clone()),
        );
        let api = CloverApi::new(reqwest::Client::new(), config.clone(), Arc::new(auth));
        Fixture {
            stand,
            tokens,
            clock,
            config,
            api,
            merchant: bean_room(),
            tenders: Arc::new(MemoryTenders::default()),
        }
    }

    fn source(&self) -> CloverSource {
        self.source_with(
            MemoryCursor::since(Some("2026-10-28T08:57:00Z")),
            self.merchant.clone(),
        )
    }

    fn source_with(&self, cursor: Arc<dyn CursorStore>, at: CloverMerchant) -> CloverSource {
        CloverSource::new(
            self.api.clone(),
            self.config.clone(),
            at,
            cursor,
            self.tenders.clone(),
            Arc::new(self.clock.clone()),
        )
    }

    async fn methods(&self) -> Vec<String> {
        let mut methods = Vec::new();
        for _ in 0..self.stand.request_count().await {
            methods.push(self.stand.take_request().await.method.to_string());
        }
        methods
    }
}

fn ids(sales: &[FoundSale]) -> Vec<&str> {
    sales.iter().map(|s| s.id.as_str()).collect()
}

fn sale(order_id: Option<&str>, device: Option<&str>) -> FoundSale {
    FoundSale {
        id: "PAY_MONERO_1".into(),
        amount: till_types::Money::new(2205, "USD".parse().unwrap()),
        reference: None,
        order_id: order_id.map(Into::into),
        created_at: "2026-10-28T08:56:00Z".parse().unwrap(),
        updated_at: None,
        status: None,
        device_id: device.map(Into::into),
        device_name: None,
        cash: false,
        title: None,
        point_of_sale: PointOfSale::Square,
    }
}

#[tokio::test]
async fn the_merchant_is_read_with_its_address_and_properties_and_every_request_names_the_till() {
    let f = Fixture::start().await;
    f.stand.enqueue(json(&fixture("clover/merchant.json")));
    f.stand.enqueue(json(&fixture("clover/properties.json")));
    assert_eq!(f.api.merchant(&f.merchant.id).await.unwrap(), f.merchant);
    let first = f.stand.take_request().await;
    assert_eq!(first.url.path(), "/v3/merchants/MCLOVER00001");
    assert_eq!(query(&first, "expand").as_deref(), Some("address"));
    assert_eq!(
        header(&first, "Authorization").as_deref(),
        Some("Bearer acc1")
    );
    assert_eq!(header(&first, "User-Agent").as_deref(), Some("Till/0.3.0"));
    assert_eq!(
        f.stand.take_request().await.url.path(),
        "/v3/merchants/MCLOVER00001/properties"
    );
}

#[tokio::test]
async fn a_merchant_answer_for_another_merchant_is_refused() {
    let f = Fixture::start().await;
    f.stand
        .enqueue(json(r#"{"id":"MOTHER000001","name":"Elsewhere"}"#));
    let e = f.api.merchant(&f.merchant.id).await.unwrap_err();
    assert_eq!(e.pos(), Some(PointOfSale::Clover));
    assert!(e.sign_in_again());
}

#[test]
fn the_country_comes_from_the_address_else_the_locale() {
    assert_eq!(CloverApi::country(Some("us"), None).as_deref(), Some("US"));
    assert_eq!(
        CloverApi::country(None, Some("fr_FR")).as_deref(),
        Some("FR")
    );
    assert_eq!(
        CloverApi::country(Some(""), Some("es-MX")).as_deref(),
        Some("MX")
    );
    assert_eq!(CloverApi::country(None, Some("en")), None);
    assert_eq!(CloverApi::country(Some("United States"), None), None);
}

#[tokio::test]
async fn the_monero_tender_is_found_by_its_label_whatever_its_case_and_spaces_and_remembered() {
    let f = Fixture::start().await;
    f.stand.enqueue(json(&fixture("clover/tenders.json")));
    let found = f.source().ensure_tender().await.unwrap();
    let TenderState::Ready(tender) = &found else {
        panic!("{found:?}");
    };
    assert_eq!(tender.id.as_str(), "T_MONERO");
    assert_eq!(f.tenders.id("MCLOVER00001").as_deref(), Some("T_MONERO"));
    f.stand.enqueue(json(&fixture("clover/tenders.json")));
    assert_eq!(f.source().ensure_tender().await.unwrap(), found);
    assert_eq!(f.methods().await, ["GET", "GET"], "nothing made");
}

#[tokio::test]
async fn with_no_monero_tender_one_is_made_as_an_external_payment_and_remembered() {
    let f = Fixture::start().await;
    f.stand.enqueue(json(
        r#"{"elements":[{"id":"T_CASH","label":"Cash","labelKey":"com.clover.tender.cash"}]}"#,
    ));
    f.stand.enqueue(json(
        r#"{"id":"T_NEW","label":"Monero","labelKey":"com.clover.tender.external_payment","enabled":true,"visible":true}"#,
    ));
    let made = f.source().ensure_tender().await.unwrap();
    let TenderState::Ready(tender) = &made else {
        panic!("{made:?}");
    };
    assert_eq!(tender.id.as_str(), "T_NEW");
    assert_eq!(f.tenders.id("MCLOVER00001").as_deref(), Some("T_NEW"));
    f.stand.take_request().await;
    let post = f.stand.take_request().await;
    assert_eq!(post.method.as_str(), "POST");
    assert_eq!(post.url.path(), "/v3/merchants/MCLOVER00001/tenders");
    let body = body_json(&post);
    assert_eq!(body["label"], "Monero");
    assert_eq!(body["labelKey"], "com.clover.tender.external_payment");
    assert_eq!(body["opensCashDrawer"], false);
    assert_eq!(body["supportsTipping"], false);
}

#[tokio::test]
async fn a_refused_tender_is_added_in_clover_by_the_cashier_and_the_till_never_makes_it_on_its_own()
{
    let f = Fixture::start().await;
    f.stand.enqueue(json(r#"{"elements":[]}"#));
    f.stand.enqueue(status(
        403,
        r#"{"message":"Forbidden: missing MERCHANT_W"}"#,
    ));
    assert_eq!(
        f.source().ensure_tender().await.unwrap(),
        TenderState::AddInClover("Forbidden: missing MERCHANT_W".into())
    );
    assert_eq!(f.tenders.id("MCLOVER00001"), None);
    // The timer: tenders looked at, never posted, and no payments asked for without one.
    f.stand.enqueue(json(r#"{"elements":[]}"#));
    assert_eq!(f.source().new_sales().await.unwrap(), []);
    assert_eq!(f.methods().await, ["GET", "POST", "GET"]);
    // The cashier added it in Clover: the next look finds it and reads payments.
    f.stand.enqueue(json(&fixture("clover/tenders.json")));
    f.stand.enqueue(json(r#"{"elements":[]}"#));
    f.source().new_sales().await.unwrap();
    assert_eq!(f.tenders.id("MCLOVER00001").as_deref(), Some("T_MONERO"));
}

#[tokio::test]
async fn a_remembered_tender_deleted_in_clover_is_found_again() {
    let f = Fixture::start().await;
    f.tenders.set("MCLOVER00001", "T_GONE");
    f.stand.enqueue(json(&fixture("clover/tenders.json")));
    let TenderState::Ready(tender) = f.source().ensure_tender().await.unwrap() else {
        panic!("found again");
    };
    assert_eq!(tender.id.as_str(), "T_MONERO");
    assert_eq!(f.tenders.id("MCLOVER00001").as_deref(), Some("T_MONERO"));
}

#[tokio::test]
async fn only_successful_monero_payments_not_voided_refunded_or_handled_are_sales_oldest_first() {
    let f = Fixture::start().await;
    f.tenders.set("MCLOVER00001", "T_MONERO");
    let cursor = MemoryCursor::since(Some("2026-10-28T08:57:00Z"));
    let source = f.source_with(cursor.clone(), f.merchant.clone());
    f.stand.enqueue(json(&fixture("clover/payments.json")));
    let sales = source.new_sales().await.unwrap();
    assert_eq!(
        ids(&sales),
        ["PAY_MONERO_0", "PAY_MONERO_1", "PAY_MONERO_2"],
        "a voided refund doesn't count"
    );
    let sale = &sales[1];
    assert_eq!(
        *sale,
        FoundSale {
            id: "PAY_MONERO_1".into(),
            amount: till_types::Money::new(2205, "USD".parse().unwrap()),
            reference: None,
            order_id: Some("ORDER_CL_1".into()),
            created_at: "2026-10-28T08:56:00Z".parse().unwrap(),
            updated_at: None,
            status: None,
            device_id: Some("DEV_FRONT".into()),
            device_name: None,
            cash: false,
            title: Some("Table 4".into()),
            point_of_sale: PointOfSale::Clover,
        }
    );
    assert_eq!(
        sales.iter().map(|s| s.title.as_deref()).collect::<Vec<_>>(),
        [None, Some("Table 4"), None],
        "an order with no title has none"
    );
    let request = f.stand.take_request().await;
    assert_eq!(request.url.path(), "/v3/merchants/MCLOVER00001/payments");
    assert_eq!(
        query_values(&request, "filter"),
        [
            "tender.id=T_MONERO".to_owned(),
            format!("createdTime>={}", millis("2026-10-28T08:55:00Z")),
        ],
        "two minutes of overlap"
    );
    assert_eq!(
        query(&request, "expand").as_deref(),
        Some("tender,order,refunds")
    );
    assert_eq!(
        query(&request, "orderBy").as_deref(),
        Some("createdTime ASC")
    );
    assert_eq!(query(&request, "limit").as_deref(), Some("100"));
    assert_eq!(query(&request, "offset").as_deref(), Some("0"));

    source.handled(sale);
    assert_eq!(cursor.handled_ids(), ["PAY_MONERO_1"]);
    assert_eq!(
        cursor.since_value().as_deref(),
        Some("2026-10-28T08:57:00Z"),
        "never back"
    );
    f.stand.enqueue(json(&fixture("clover/payments.json")));
    assert_eq!(
        ids(&source.new_sales().await.unwrap()),
        ["PAY_MONERO_0", "PAY_MONERO_2"]
    );
}

#[tokio::test]
async fn payments_are_read_page_by_page_until_a_short_one_and_never_further_back_than_clover_allows()
 {
    let f = Fixture::start().await;
    f.tenders.set("MCLOVER00001", "T_MONERO");
    let page = |from: u64, count: u64| {
        let elements: Vec<String> = (from..from + count)
            .map(|i| {
                format!(
                    r#"{{"id":"P{i}","amount":100,"result":"SUCCESS","createdTime":{},"tender":{{"id":"T_MONERO"}}}}"#,
                    1793177760000 + i
                )
            })
            .collect();
        json(&format!(r#"{{"elements":[{}]}}"#, elements.join(",")))
    };
    f.stand.enqueue(page(0, 100));
    f.stand.enqueue(page(100, 3));
    let sales = f
        .source_with(
            MemoryCursor::since(Some("2026-01-01T00:00:00Z")),
            f.merchant.clone(),
        )
        .new_sales()
        .await
        .unwrap();
    assert_eq!(sales.len(), 103);
    let first = f.stand.take_request().await;
    assert_eq!(
        query_values(&first, "filter")[1],
        format!(
            "createdTime>={}",
            (at("2026-10-28T09:00:00Z") - 89 * 24 * 60 * 60) * 1000
        ),
        "Clover filters payments 90 days back at most"
    );
    assert_eq!(
        query(&f.stand.take_request().await, "offset").as_deref(),
        Some("100")
    );
    assert_eq!(f.stand.request_count().await, 2);
}

#[tokio::test]
async fn a_first_look_starts_watching_from_now() {
    let f = Fixture::start().await;
    f.tenders.set("MCLOVER00001", "T_MONERO");
    let cursor = MemoryCursor::since(None);
    f.stand.enqueue(json(r#"{"elements":[]}"#));
    f.source_with(cursor.clone(), f.merchant.clone())
        .new_sales()
        .await
        .unwrap();
    assert_eq!(
        cursor.since_value().as_deref(),
        Some("2026-10-28T09:00:00Z")
    );
}

#[tokio::test]
async fn clover_asking_the_till_to_slow_down_holds_the_next_look_for_its_retry_after() {
    let f = Fixture::start().await;
    f.tenders.set("MCLOVER00001", "T_MONERO");
    let source = f.source();
    f.stand
        .enqueue(ResponseTemplate::new(429).insert_header("Retry-After", "30"));
    let e = source.new_sales().await.unwrap_err();
    assert_eq!(e.to_string(), "Clover asked the till to slow down.");
    assert_eq!(e.retry_after_seconds(), Some(30));
    f.clock.advance(29);
    assert_eq!(source.new_sales().await.unwrap(), []);
    assert_eq!(f.stand.request_count().await, 1, "nothing asked while held");
    f.clock.advance(1);
    f.stand.enqueue(json(r#"{"elements":[]}"#));
    source.new_sales().await.unwrap();
    assert_eq!(f.stand.request_count().await, 2);
}

#[tokio::test]
async fn a_429_without_retry_after_holds_for_a_second() {
    let f = Fixture::start().await;
    f.tenders.set("MCLOVER00001", "T_MONERO");
    f.stand.enqueue(ResponseTemplate::new(429));
    let e = f.source().new_sales().await.unwrap_err();
    assert_eq!(e.retry_after_seconds(), Some(1));
}

#[tokio::test]
async fn a_token_clover_ended_early_is_refreshed_and_the_request_asked_again() {
    let f = Fixture::start().await;
    f.tenders.set("MCLOVER00001", "T_MONERO");
    f.stand
        .enqueue(status(401, r#"{"message":"401 Unauthorized"}"#));
    f.stand.enqueue(json(
        r#"{"access_token":"acc2","access_token_expiration":1793179800,"refresh_token":"ref2"}"#,
    ));
    f.stand.enqueue(json(r#"{"elements":[]}"#));
    assert_eq!(f.source().new_sales().await.unwrap(), []);
    let kept = f.tokens.get().unwrap();
    assert_eq!(kept.access_token.expose_str(), "acc2");
    assert_eq!(
        kept.refresh_token.as_ref().map(|t| t.expose_str()),
        Some("ref2")
    );
    assert_eq!(
        header(&f.stand.take_request().await, "Authorization").as_deref(),
        Some("Bearer acc1")
    );
    assert_eq!(f.stand.take_request().await.url.path(), "/oauth/v2/refresh");
    assert_eq!(
        header(&f.stand.take_request().await, "Authorization").as_deref(),
        Some("Bearer acc2")
    );
}

#[tokio::test]
async fn a_fresh_token_refused_too_means_signing_in_again_with_no_refresh_on_the_next_pass() {
    let f = Fixture::start().await;
    f.tenders.set("MCLOVER00001", "T_MONERO");
    f.stand.enqueue(ResponseTemplate::new(401));
    f.stand.enqueue(json(
        r#"{"access_token":"acc2","access_token_expiration":1793179800,"refresh_token":"ref2"}"#,
    ));
    f.stand.enqueue(ResponseTemplate::new(401));
    let e = f.source().new_sales().await.unwrap_err();
    assert!(
        e.sign_in_again(),
        "the till catches one type for every point of sale"
    );
    assert_eq!(f.tokens.get(), None);
    assert!(f.source().new_sales().await.unwrap_err().sign_in_again());
    assert_eq!(f.stand.request_count().await, 3);
}

#[tokio::test]
async fn a_refresh_refused_after_an_early_end_signs_out() {
    let f = Fixture::start().await;
    f.tenders.set("MCLOVER00001", "T_MONERO");
    f.stand.enqueue(ResponseTemplate::new(401));
    f.stand
        .enqueue(status(401, r#"{"message":"Refresh token revoked"}"#));
    let e = f.source().new_sales().await.unwrap_err();
    assert_eq!(e.to_string(), "Clover's sign-in has ended. Sign in again.");
    assert_eq!(f.tokens.get(), None);
}

#[tokio::test]
async fn a_sale_is_refunded_when_its_refunds_reach_its_amount_it_was_voided_or_its_order_reads_refunded()
 {
    let f = Fixture::start().await;
    let source = f.source();
    let Refunds::InPos(refunds) = source.refunds() else {
        panic!("the cashier refunds a Clover sale");
    };
    let sale = sale(Some("ORDER_CL_1"), Some("DEV_FRONT"));

    f.stand
        .enqueue(json(&fixture("clover/payment-refunded.json")));
    assert!(
        refunds.refunded(&sale).await.unwrap(),
        "1000 + 1205, the voided one left out"
    );
    let request = f.stand.take_request().await;
    assert_eq!(
        request.url.path(),
        "/v3/merchants/MCLOVER00001/payments/PAY_MONERO_1"
    );
    assert_eq!(
        query(&request, "expand").as_deref(),
        Some("tender,order,refunds")
    );

    let partly = r#"{"id":"PAY_MONERO_1","amount":2205,"result":"SUCCESS","createdTime":1793177760000,"order":{"id":"ORDER_CL_1"},
        "refunds":{"elements":[{"id":"R","amount":1000}]}}"#;
    f.stand.enqueue(json(partly));
    f.stand.enqueue(json(&fixture("clover/order.json")));
    assert!(
        !refunds.refunded(&sale).await.unwrap(),
        "a part refund isn't the sale refunded"
    );
    f.stand.enqueue(json(partly));
    f.stand.enqueue(json(
        &fixture("clover/order.json")
            .replace(r#""paymentState": "PAID""#, r#""paymentState": "REFUNDED""#),
    ));
    assert!(refunds.refunded(&sale).await.unwrap());

    f.stand.enqueue(json(
        r#"{"id":"PAY_MONERO_1","amount":2205,"result":"VOIDED","voided":true,"createdTime":1793177760000}"#,
    ));
    assert!(
        refunds.refunded(&sale).await.unwrap(),
        "voided in Clover is undone too"
    );
}

#[tokio::test]
async fn a_merchant_whose_country_isnt_known_gets_the_plain_code() {
    let f = Fixture::start().await;
    let sale = sale(Some("ORDER_CL_1"), None);
    let nowhere = CloverMerchant {
        country: None,
        ..f.merchant.clone()
    };
    let cursor = MemoryCursor::since(Some("2026-10-28T08:57:00Z"));
    assert_eq!(
        f.source_with(cursor, nowhere)
            .order(&sale)
            .await
            .unwrap()
            .receipt,
        None
    );
    let no_order = FoundSale {
        order_id: None,
        ..sale
    };
    assert_eq!(f.source().order(&no_order).await.unwrap().receipt, None);
    assert_eq!(f.stand.request_count().await, 0);
}

/// A remembered tender id the store couldn't read is an error: Clover's
/// tenders are listed, but none is made, found again or saved over it.
#[tokio::test]
async fn a_tender_id_that_cant_be_read_makes_no_tender_and_saves_nothing() {
    let f = Fixture::start().await;
    let tenders = Arc::new(Unreadable::default());
    let source = CloverSource::new(
        f.api.clone(),
        f.config.clone(),
        f.merchant.clone(),
        MemoryCursor::since(Some("2026-10-28T08:57:00Z")),
        tenders.clone(),
        Arc::new(f.clock.clone()),
    );
    f.stand.enqueue(json(r#"{"elements":[]}"#));
    let e = source.ensure_tender().await.unwrap_err();
    assert!(matches!(e, till_pos::PosError::Store(_)), "{e:?}");
    let e = source.new_sales().await.unwrap_err();
    assert!(matches!(e, till_pos::PosError::Store(_)), "{e:?}");
    assert_eq!(tenders.saves(), 0);
    assert_eq!(f.methods().await, ["GET"], "nothing made");
}

/// A cursor the store couldn't read isn't started afresh over the kept one.
#[tokio::test]
async fn a_cursor_that_cant_be_read_is_an_error_and_never_saved_over() {
    let f = Fixture::start().await;
    f.tenders.set("MCLOVER00001", "T_MONERO");
    let cursor = Arc::new(Unreadable::default());
    let source = f.source_with(cursor.clone(), f.merchant.clone());
    let e = source.new_sales().await.unwrap_err();
    assert!(matches!(e, till_pos::PosError::Store(_)), "{e:?}");
    source.handled(&sale(None, None));
    assert_eq!(cursor.saves(), 0);
    assert_eq!(f.stand.request_count().await, 0);
}

#[tokio::test]
async fn the_merchants_devices_are_read_for_their_names() {
    let f = Fixture::start().await;
    f.stand.enqueue(json(
        r#"{"elements":[
            {"id":"DEV_FRONT","name":"  Front   counter ","model":"Clover Mini","serial":"C030UQ71040123"},
            {"id":"DEV_BACK","model":"Clover Flex","serial":"C042UQ0004F2A"},
            {"id":"DEV_BARE"}
        ]}"#,
    ));
    let devices = f.api.devices(&f.merchant.id).await.unwrap();
    assert_eq!(
        devices.iter().map(|d| d.label()).collect::<Vec<_>>(),
        [
            Some("Front counter".to_owned()),
            Some("Clover Flex 4F2A".to_owned()),
            None,
        ],
        "the name, else the model and the serial's end, else none"
    );
    let request = f.stand.take_request().await;
    assert_eq!(request.url.path(), "/v3/merchants/MCLOVER00001/devices");
    assert_eq!(query(&request, "limit").as_deref(), Some("1000"));
}
