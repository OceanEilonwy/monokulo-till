//! Square's sign-in and API as the till uses them, against a stand-in
//! Square. Ported test for test from the Android app's `SquareTest.kt`.

#![cfg(feature = "square")]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use monokulo_client::FixedClock;
use till_pos::square::{
    SquareApi, SquareAuth, SquareConfig, SquareLocation, SquareOrder, SquareSource,
};
use till_pos::{
    AuthCode, FoundSale, ListedRefund, OAuthRedirect, OAuthState, Pkce, PointOfSale, Refunds,
    SaleSource, SquareEnvironment,
};

const NOW: &str = "2026-10-22T16:10:00Z";

struct Fixture {
    stand: Stand,
    tokens: Arc<MemoryTokens>,
    clock: FixedClock,
    config: SquareConfig,
    auth: Arc<SquareAuth>,
}

impl Fixture {
    async fn start() -> Fixture {
        let stand = Stand::start().await;
        let tokens = Arc::new(MemoryTokens::default());
        let clock = FixedClock::at(at(NOW));
        let config = SquareConfig {
            base_url: Some(stand.url()),
            ..SquareConfig::new("sq0idp-app", false, "https://till.monokulo.com/square")
        };
        let auth = Arc::new(SquareAuth::new(
            reqwest::Client::new(),
            config.clone(),
            tokens.clone(),
            Arc::new(clock.clone()),
        ));
        Fixture {
            stand,
            tokens,
            clock,
            config,
            auth,
        }
    }

    fn source(&self, cursor: Arc<MemoryCursor>) -> SquareSource {
        self.tokens
            .set(Some(tokens("EAAA1", "2026-11-21T16:10:00Z", "r1", "M1")));
        let api = SquareApi::new(
            reqwest::Client::new(),
            self.config.clone(),
            self.auth.clone(),
        );
        let location = SquareLocation {
            id: "L1".into(),
            name: "Front Counter".into(),
            business_name: Some("The Bean Room".into()),
            country: "US".into(),
            currency: "USD".into(),
            timezone: Some("America/Los_Angeles".into()),
            status: None,
        };
        SquareSource::new(
            api,
            self.config.clone(),
            location,
            cursor,
            Arc::new(self.clock.clone()),
        )
    }
}

#[tokio::test]
async fn the_environment_is_the_applications_as_a_bell_names_it() {
    let f = Fixture::start().await;
    assert_eq!(f.config.environment(), SquareEnvironment::Production);
    let sandbox = SquareConfig {
        application_id: "sandbox-sq0idb-dev".into(),
        ..f.config.clone()
    };
    assert_eq!(sandbox.environment(), SquareEnvironment::Sandbox);
    assert_eq!(
        SquareEnvironment::of_application("sq0idp-sandbox-lookalike"),
        SquareEnvironment::Production
    );
}

#[test]
fn the_pkce_challenge_is_rfc_7636s() {
    let pkce = Pkce {
        verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into(),
        state: "s".into(),
    };
    assert_eq!(
        pkce.challenge(),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[tokio::test]
async fn signing_in_asks_for_the_tills_scopes_and_keeps_the_tokens() {
    let f = Fixture::start().await;
    let pkce = Pkce::create();
    let url = f.auth.authorize_url(&pkce);
    assert!(url.contains("client_id=sq0idp-app"), "{url}");
    assert!(
        url.contains(&format!("code_challenge={}", pkce.challenge())),
        "{url}"
    );
    assert!(
        url.contains("scope=PAYMENTS_READ%20ORDERS_READ%20MERCHANT_PROFILE_READ%20PAYMENTS_WRITE"),
        "{url}"
    );
    assert!(
        url.contains("redirect_uri=https%3A%2F%2Ftill.monokulo.com%2Fsquare"),
        "{url}"
    );
    assert!(url.contains("session=false"), "{url}");

    // Another attempt's state is refused before Square is asked.
    let refused = f
        .auth
        .finish(&pkce, &redirect(Some("other"), Some("code"), None))
        .await
        .unwrap_err();
    assert_eq!(refused.pos(), Some(PointOfSale::Square));
    f.stand.enqueue(json(
        r#"{"access_token":"EAAA1","expires_at":"2026-11-21T16:10:00Z","refresh_token":"r1","merchant_id":"M1"}"#,
    ));
    f.auth
        .finish(
            &pkce,
            &redirect(Some(pkce.state.as_str()), Some("code1"), None),
        )
        .await
        .unwrap();
    assert_eq!(f.tokens.get().unwrap().access_token.expose_str(), "EAAA1");
    let sent = body_text(&f.stand.take_request().await);
    assert!(
        sent.contains(&format!(
            r#""code_verifier":"{}""#,
            pkce.verifier.expose_str()
        )) && !sent.contains("secret"),
        "{sent}"
    );
}

#[tokio::test]
async fn the_sandboxs_sign_in_leaves_out_session_which_it_doesnt_support() {
    let f = Fixture::start().await;
    let sandbox = SquareAuth::new(
        reqwest::Client::new(),
        SquareConfig {
            sandbox: true,
            ..f.config.clone()
        },
        f.tokens.clone(),
        Arc::new(f.clock.clone()),
    );
    let url = sandbox.authorize_url(&Pkce::create());
    assert!(!url.contains("session="), "{url}");
    assert!(url.contains("code_challenge_method=S256"), "{url}");
}

#[tokio::test]
async fn a_token_about_to_expire_is_refreshed_and_the_new_refresh_token_kept() {
    let f = Fixture::start().await;
    f.tokens
        .set(Some(tokens("old", "2026-10-22T20:00:00Z", "r1", "M1")));
    f.stand.enqueue(json(
        r#"{"access_token":"new","expires_at":"2026-11-21T16:10:00Z","refresh_token":"r2","merchant_id":"M1"}"#,
    ));
    assert_eq!(f.auth.access_token().await.unwrap().expose_str(), "new");
    assert_eq!(
        f.tokens
            .get()
            .unwrap()
            .refresh_token
            .as_ref()
            .map(|t| t.expose_str()),
        Some("r2")
    );
    assert!(body_text(&f.stand.take_request().await).contains(r#""refresh_token":"r1""#));
    // Good for a while now: no request.
    assert_eq!(f.auth.access_token().await.unwrap().expose_str(), "new");
    assert_eq!(f.stand.request_count().await, 1);
}

#[tokio::test]
async fn a_refused_refresh_means_signing_in_again() {
    let f = Fixture::start().await;
    f.tokens
        .set(Some(tokens("old", "2026-10-22T20:00:00Z", "r1", "M1")));
    f.stand.enqueue(status(
        401,
        r#"{"errors":[{"code":"UNAUTHORIZED","detail":"Refresh token expired"}]}"#,
    ));
    let e = f.auth.access_token().await.unwrap_err();
    assert!(e.sign_in_again());
    assert_eq!(f.tokens.get(), None);
}

#[tokio::test]
async fn only_completed_monero_tenders_are_sales_and_handled_ones_arent_given_again() {
    let f = Fixture::start().await;
    let cursor = MemoryCursor::since(Some("2026-10-22T16:05:00Z"));
    let source = f.source(cursor.clone());
    f.stand.enqueue(json(&fixture("square/payments.json")));
    let sales = source.new_sales().await.unwrap();
    assert_eq!(
        sales.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["pay_monero", "pay_cash"],
        "Monero and completed cash, not card, other tenders or open payments"
    );
    let sale = &sales[0];
    assert_eq!(sale.amount.minor, 1388);
    assert_eq!(sale.reference.as_deref(), Some("bP9m"));
    assert_eq!(
        sale.device_id.as_ref().map(|d| d.as_str()),
        Some("dev_front")
    );
    assert_eq!(
        sale.device_name.as_deref(),
        Some("Front iPad"),
        "on one line, trimmed"
    );
    assert!(!sale.cash);
    assert_eq!(
        sale.updated_at.as_ref().map(|t| t.as_str()),
        Some("2026-10-22T16:07:01.004Z")
    );
    assert_eq!(sale.status.as_ref().map(|s| s.as_str()), Some("COMPLETED"));
    let cash = &sales[1];
    assert!(cash.cash);
    assert_eq!(cash.amount.minor, 2450);
    assert_eq!(cash.device_id.as_ref().map(|d| d.as_str()), Some("dev_bar"));
    assert_eq!(cash.device_name.as_deref(), Some("Bar iPad"));
    let request = f.stand.take_request().await;
    assert_eq!(query(&request, "location_id").as_deref(), Some("L1"));
    assert_eq!(
        query(&request, "begin_time").as_deref(),
        Some("2026-10-22T16:03:00Z"),
        "two minutes of overlap"
    );
    assert_eq!(
        header(&request, "Square-Version").as_deref(),
        Some(SquareConfig::API_VERSION)
    );

    source.handled(sale);
    assert_eq!(
        cursor.handled_ids(),
        ["pay_monero@2026-10-22T16:07:01.004Z"],
        "as it was when handled"
    );
    assert_eq!(
        cursor.since_value().as_deref(),
        Some("2026-10-22T16:07:00.512Z")
    );
    source.handled(cash);
    f.stand.enqueue(json(&fixture("square/payments.json")));
    assert!(source.new_sales().await.unwrap().is_empty());

    // Square changes the payment (a refund, say): it's given again.
    f.stand.enqueue(json(
        &fixture("square/payments.json")
            .replace("2026-10-22T16:07:01.004Z", "2026-10-22T16:12:30.250Z"),
    ));
    let again = source.new_sales().await.unwrap();
    assert_eq!(
        again
            .iter()
            .map(|s| (s.id.as_str(), s.updated_at.as_ref().map(|t| t.as_str())))
            .collect::<Vec<_>>(),
        [("pay_monero", Some("2026-10-22T16:12:30.250Z"))]
    );
}

/// Refunds made anywhere at the location (the till, Square's own app, the
/// Dashboard) are listed from a time, page by page, with `PAYMENTS_READ`.
#[tokio::test]
async fn the_locations_refunds_are_listed_page_by_page() {
    let f = Fixture::start().await;
    let source = f.source(MemoryCursor::since(Some("2026-10-22T16:05:00Z")));
    f.stand.enqueue(json(
        r#"{"refunds":[{"id":"rf_1","status":"COMPLETED","amount_money":{"amount":1388,"currency":"USD"},
            "payment_id":"pay_monero","order_id":"ord_sq_1","location_id":"L1",
            "created_at":"2026-10-22T16:08:00Z","updated_at":"2026-10-22T16:08:02Z"}],"cursor":"c2"}"#,
    ));
    f.stand.enqueue(json(
        r#"{"refunds":[{"id":"rf_2","status":"PENDING","amount_money":{"amount":2450,"currency":"USD"},
            "payment_id":"pay_cash","created_at":"2026-10-22T16:09:00Z"}]}"#,
    ));
    let listed = source
        .refunds_since(&"2026-10-22T16:00:00Z".parse().unwrap())
        .await
        .unwrap()
        .expect("Square lists refunds");
    assert_eq!(
        listed,
        [
            ListedRefund {
                id: "rf_1".into(),
                payment_id: "pay_monero".into(),
                amount_minor: 1388,
                status: "COMPLETED".into(),
                created_at: Some("2026-10-22T16:08:00Z".parse().unwrap()),
            },
            ListedRefund {
                id: "rf_2".into(),
                payment_id: "pay_cash".into(),
                amount_minor: 2450,
                status: "PENDING".into(),
                created_at: Some("2026-10-22T16:09:00Z".parse().unwrap()),
            },
        ]
    );
    let first = f.stand.take_request().await;
    assert_eq!(first.url.path(), "/v2/refunds");
    assert_eq!(query(&first, "location_id").as_deref(), Some("L1"));
    assert_eq!(
        query(&first, "begin_time").as_deref(),
        Some("2026-10-22T16:00:00Z")
    );
    assert_eq!(query(&first, "cursor"), None);
    assert_eq!(
        header(&first, "Square-Version").as_deref(),
        Some(SquareConfig::API_VERSION)
    );
    let second = f.stand.take_request().await;
    assert_eq!(query(&second, "cursor").as_deref(), Some("c2"));
    assert_eq!(
        query(&second, "begin_time").as_deref(),
        Some("2026-10-22T16:00:00Z")
    );
}

#[tokio::test]
async fn a_sales_receipt_is_made_from_its_order_on_the_till_and_fits_in_the_code() {
    let f = Fixture::start().await;
    let source = f.source(MemoryCursor::since(Some("2026-10-22T16:05:00Z")));
    f.stand.enqueue(json(&fixture("square/payments.json")));
    let sale = source.new_sales().await.unwrap().remove(0);
    f.stand.enqueue(json(&fixture("square/order.json")));
    let receipt = source.order(&sale).await.unwrap().receipt.unwrap();
    f.stand.take_request().await;
    assert_eq!(
        f.stand.take_request().await.url.path(),
        "/v2/orders/ord_sq_1"
    );
    assert_eq!(receipt.receipt.receipt_id, "bP9m");
    assert_eq!(receipt.receipt.issue_timestamp, "2026-10-22T09:06:41-07:00");
    assert_eq!(receipt.merchant.legal_name, "The Bean Room");
    assert_eq!(
        receipt.merchant.display_name.as_deref(),
        Some("Front Counter")
    );
    assert_eq!(
        receipt
            .line_items
            .iter()
            .map(|l| l.description.as_str())
            .collect::<Vec<_>>(),
        ["Flat white, Oat milk", "Almond croissant warmed (Large)"]
    );
    let flat_white = &receipt.line_items[0];
    assert_eq!(
        flat_white.unit_price, 4.5,
        "the price that makes 2 × it the line's total"
    );
    assert_eq!(flat_white.line_total, 9.0);
    assert_eq!(
        receipt.tax.as_ref().unwrap().tax_lines.as_ref().unwrap()[0].rate,
        0.08875
    );
    assert_eq!(
        receipt
            .tax
            .as_ref()
            .unwrap()
            .tax_lines
            .as_ref()
            .unwrap()
            .len(),
        1
    );

    let uri = orf_codec::query::append("monero:8Bx?tx_amount=0.1", &receipt)
        .expect("the receipt fits in the code");
    let decoded = orf_codec::query::from_uri(&uri).unwrap().unwrap();
    assert!(
        serde_json::to_string(&decoded)
            .unwrap()
            .contains("Almond croissant warmed (Large)")
    );
}

#[tokio::test]
async fn a_sales_order_names_whom_its_for_when_a_pickup_names_them_from_the_one_read() {
    let f = Fixture::start().await;
    let source = f.source(MemoryCursor::since(Some("2026-10-22T16:05:00Z")));
    f.stand.enqueue(json(&fixture("square/payments.json")));
    let sale = source.new_sales().await.unwrap().remove(0);
    f.stand.enqueue(json(&fixture("square/order-pickup.json")));
    let read = source.order(&sale).await.unwrap();
    assert_eq!(
        read.customer_name.as_deref(),
        Some("Ana López"),
        "on one line, trimmed"
    );
    let receipt = read.receipt.expect("a receipt");
    let uri = orf_codec::query::append("monero:8Bx?tx_amount=0.1", &receipt).unwrap();
    assert!(!uri.contains("Ana"), "the name is never in the code");
    let decoded = orf_codec::query::from_uri(&uri).unwrap().unwrap();
    assert!(!serde_json::to_string(&decoded).unwrap().contains("Ana"));
    assert_eq!(
        f.stand.request_count().await,
        2,
        "the payments and the order: no Customers lookup"
    );
    f.stand.enqueue(json(&fixture("square/order.json")));
    assert_eq!(
        source.order(&sale).await.unwrap().customer_name,
        None,
        "an order with no fulfillment names no one"
    );
}

#[test]
fn a_recipients_name_is_read_defensively_and_a_strange_shape_never_costs_the_receipt() {
    let name = |fulfillments: &str| {
        serde_json::from_str::<SquareOrder>(&format!(
            r#"{{"id":"o","location_id":"L1","created_at":"2026-10-22T16:06:41Z","total_money":{{"amount":1,"currency":"USD"}},"fulfillments":{fulfillments}}}"#
        ))
        .unwrap()
        .customer_name()
    };
    assert_eq!(
        name(r#"[{"delivery_details":{"recipient":{"display_name":"Sam"}}}]"#).as_deref(),
        Some("Sam")
    );
    assert_eq!(
        name(r#"[{"shipment_details":{"recipient":{"display_name":"Kai"}}}]"#).as_deref(),
        Some("Kai")
    );
    assert_eq!(
        name(r#"[{"pickup_details":{"recipient":{"display_name":" "}}},{"pickup_details":{"recipient":{"display_name":"Lee"}}}]"#)
            .as_deref(),
        Some("Lee"),
        "a blank name is skipped"
    );
    assert_eq!(
        name(&format!(
            r#"[{{"pickup_details":{{"recipient":{{"display_name":"{}"}}}}}}]"#,
            "x".repeat(500)
        )),
        Some("x".repeat(80))
    );
    assert_eq!(
        name(r#"[{"pickup_details":{"recipient":{"display_name":42}}}]"#),
        None,
        "a number isn't a name"
    );
    assert_eq!(name(r#"[{"pickup_details":{"recipient":"Ana"}}]"#), None);
    assert_eq!(name(r#"["Ana", null, 3]"#), None);
    assert_eq!(name(r#"{"pickup_details":{}}"#), None);
    assert_eq!(name("null"), None);
}

#[test]
fn the_name_typed_at_the_counter_names_the_order_when_no_recipient_does() {
    let name = |fields: &str| {
        serde_json::from_str::<SquareOrder>(&format!(
            r#"{{"id":"o","location_id":"L1","created_at":"2026-10-22T16:06:41Z","total_money":{{"amount":1,"currency":"USD"}}{fields}}}"#
        ))
        .unwrap()
        .customer_name()
    };
    assert_eq!(
        name(r##","ticket_name":"  Sam\n""##).as_deref(),
        Some("Sam"),
        "on one line, trimmed"
    );
    assert_eq!(
        name(
            r#","ticket_name":"Sam","fulfillments":[{"pickup_details":{"recipient":{"display_name":"Ana López"}}}]"#
        )
        .as_deref(),
        Some("Ana López"),
        "a named recipient comes first (decision 112)"
    );
    assert_eq!(
        name(
            r#","ticket_name":"Sam","fulfillments":[{"pickup_details":{"recipient":{"display_name":" "}}}]"#
        )
        .as_deref(),
        Some("Sam"),
        "a blank recipient falls back to the ticket"
    );
    assert_eq!(name(r##","ticket_name":"   ""##), None);
    assert_eq!(name(""), None);
}

#[tokio::test]
async fn a_refund_is_the_whole_sale_once() {
    let f = Fixture::start().await;
    let source = f.source(MemoryCursor::since(Some("2026-10-22T16:05:00Z")));
    f.stand.enqueue(json(
        r#"{"refund":{"id":"r","status":"PENDING","amount_money":{"amount":1388,"currency":"USD"},"payment_id":"pay_monero","created_at":"2026-10-22T16:10:00Z"}}"#,
    ));
    let Refunds::ByTill(refunds) = source.refunds() else {
        panic!("the till refunds a Square sale");
    };
    let made = refunds
        .refund(&FoundSale {
            id: "pay_monero".into(),
            amount: till_types::Money::new(1388, "USD".parse().unwrap()),
            reference: Some("bP9m".into()),
            order_id: Some("ord_sq_1".into()),
            created_at: "2026-10-22T16:07:00Z".parse().unwrap(),
            updated_at: None,
            status: None,
            device_id: None,
            device_name: None,
            cash: false,
            title: None,
            point_of_sale: PointOfSale::Square,
        })
        .await
        .unwrap()
        .expect("Square answers with the refund");
    assert_eq!((made.id.as_str(), made.status.as_str()), ("r", "PENDING"));
    let body = body_text(&f.stand.take_request().await);
    assert!(
        body.contains(r#""payment_id":"pay_monero""#) && body.contains(r#""amount":1388"#),
        "{body}"
    );
    assert!(
        body.contains(r#""idempotency_key":"till-refund-pay_monero""#),
        "{body}"
    );
}

#[test]
fn amounts_are_decimals_in_the_currencys_own_digits() {
    let decimal =
        |minor, currency: &str| till_types::Money::new(minor, currency.parse().unwrap()).decimal();
    assert_eq!(decimal(2450, "USD"), "24.50");
    assert_eq!(decimal(1200, "JPY"), "1200");
    assert_eq!(decimal(5, "EUR"), "0.05");
}

// Not in SquareTest.kt: the lock docs/ios-plan.md §5 asks for.

#[tokio::test]
async fn two_refreshes_at_once_sharing_a_lock_make_one_refresh_request() {
    let f = Fixture::start().await;
    f.tokens
        .set(Some(tokens("old", "2026-10-22T20:00:00Z", "r1", "M1")));
    f.stand.enqueue(
        json(r#"{"access_token":"new","expires_at":"2026-11-21T16:10:00Z","refresh_token":"r2","merchant_id":"M1"}"#)
            .set_delay(Duration::from_millis(200)),
    );
    // The app and its extension: two sign-ins over one store, one lock.
    let lock = Arc::new(SharedLock::default());
    let sign_in = || {
        SquareAuth::new(
            reqwest::Client::new(),
            f.config.clone(),
            f.tokens.clone(),
            Arc::new(f.clock.clone()),
        )
        .with_lock(lock.clone())
    };
    let (app, extension) = (sign_in(), sign_in());
    let (a, b) = tokio::join!(app.access_token(), extension.access_token());
    assert_eq!((a.unwrap(), b.unwrap()), ("new".into(), "new".into()));
    assert_eq!(f.stand.request_count().await, 1, "r1 spent once");
    assert_eq!(
        f.tokens
            .get()
            .unwrap()
            .refresh_token
            .as_ref()
            .map(|t| t.expose_str()),
        Some("r2")
    );
}

/// Decision 17: a sign-in the store couldn't read is an error, never
/// "signed out": Square isn't asked, nothing is saved over it, and the
/// cashier isn't sent to sign in again.
#[tokio::test]
async fn a_sign_in_that_cant_be_read_is_an_error_never_signed_out() {
    let f = Fixture::start().await;
    let store = Arc::new(Unreadable::default());
    let auth = SquareAuth::new(
        reqwest::Client::new(),
        f.config.clone(),
        store.clone(),
        Arc::new(f.clock.clone()),
    );
    assert!(auth.signed_in().is_err());
    let e = auth.access_token().await.unwrap_err();
    assert!(matches!(e, till_pos::PosError::Store(_)), "{e:?}");
    assert!(!e.sign_in_again());
    assert_eq!(store.saves(), 0);
    assert_eq!(f.stand.request_count().await, 0);
}

/// A cursor the store couldn't read isn't started afresh over the kept one,
/// and a sale handled meanwhile leaves it alone.
#[tokio::test]
async fn a_cursor_that_cant_be_read_is_an_error_and_never_saved_over() {
    let f = Fixture::start().await;
    f.source(MemoryCursor::since(None));
    let cursor = Arc::new(Unreadable::default());
    let api = SquareApi::new(reqwest::Client::new(), f.config.clone(), f.auth.clone());
    let location = SquareLocation {
        id: "L1".into(),
        name: "Front Counter".into(),
        business_name: None,
        country: "US".into(),
        currency: "USD".into(),
        timezone: None,
        status: None,
    };
    let source = SquareSource::new(
        api,
        f.config.clone(),
        location,
        cursor.clone(),
        Arc::new(f.clock.clone()),
    );
    let e = source.new_sales().await.unwrap_err();
    assert!(matches!(e, till_pos::PosError::Store(_)), "{e:?}");
    source.handled(&FoundSale {
        id: "pay_1".into(),
        amount: till_types::Money::new(1388, "USD".parse().unwrap()),
        reference: None,
        order_id: None,
        created_at: "2026-10-22T16:07:00Z".parse().unwrap(),
        updated_at: None,
        status: None,
        device_id: None,
        device_name: None,
        cash: false,
        title: None,
        point_of_sale: PointOfSale::Square,
    });
    assert_eq!(cursor.saves(), 0);
    assert_eq!(f.stand.request_count().await, 0);
}

/// A Square redirect saying `state`, `code` and `error`.
fn redirect(state: Option<&str>, code: Option<&str>, error: Option<&str>) -> OAuthRedirect {
    OAuthRedirect {
        state: state.map(OAuthState::new),
        code: code.map(AuthCode::new),
        error: error.map(String::from),
        merchant_id: None,
    }
}
