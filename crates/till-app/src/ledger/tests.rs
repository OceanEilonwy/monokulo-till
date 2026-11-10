//! The ledger as the app and iOS's extension open it over one platform
//! and one data directory (docs/ledger.md §15, the engine's part): the
//! §1 race, scope deletion, the import made once.

use std::sync::{Arc, Mutex};

use monokulo_client::{FixedClock, PointOfSale, SalePhase};
use till_ledger::{
    AlertKind, Claim, ClaimPolicy, Fact, OrderCreated, OrderId, OrderObserved, PaymentId,
    PaymentSeen, Process, Tender,
};

use crate::side::Side;
use crate::store::{Kept, Store};
use crate::testing::FakePlatform;
use crate::{CloverSetup, Device, EngineConfig, SquareSetup};

fn config(data_dir: &str) -> EngineConfig {
    EngineConfig {
        data_dir: data_dir.to_owned(),
        device: Device::Ipad,
        device_name: "iPad".into(),
        app_version: "0.1.0".into(),
        debug: false,
        utc_offset: 0,
        hours_24: false,
        square: SquareSetup {
            application_id: "sq0idp-till".into(),
            environment: crate::SquareEnvironment::Production,
            redirect_uri: "https://till.monokulo.com/square".into(),
            base_url: None,
            return_uri: None,
        },
        clover: CloverSetup {
            application_id: "clv-till".into(),
            environment: crate::CloverEnvironment::Sandbox,
            redirect_uri: "https://till.monokulo.com/clover".into(),
            base_url: None,
            return_uri: None,
        },
        bell_url: "https://bell.monokulo.com".into(),
        relay_url: None,
    }
}

/// A till signed in to Square at location `L1`, connected to a store.
fn platform() -> Arc<FakePlatform> {
    FakePlatform::with(&[
        ("setup.done", "true"),
        ("pos.kind", "square"),
        (
            "square.location",
            r#"{"id":"L1","name":"Front counter","country":"US","currency":"USD"}"#,
        ),
        (
            "connection",
            r#"{"app_key":"ak_1","api":"https://pay.example/api/v1"}"#,
        ),
    ])
}

/// The app's or the extension's side over `platform` and `dir`, the app's
/// claims collected.
fn side(
    platform: &Arc<FakePlatform>,
    dir: &tempfile::TempDir,
    process: Process,
) -> (Side, Arc<Mutex<Vec<Claim>>>) {
    let kept = Arc::new(Kept::load(Store::new(platform.clone()), Arc::new(|_| {})).unwrap());
    let side = Side::new(
        Arc::new(config(&dir.path().to_string_lossy())),
        kept,
        platform.clone(),
        process,
        reqwest::Client::new(),
        Arc::new(FixedClock::at(1_761_149_280)),
    );
    let claimed = Arc::new(Mutex::new(Vec::new()));
    let into = claimed.clone();
    side.ledger()
        .deliver_to(Arc::new(move |claims| into.lock().unwrap().extend(claims)));
    side.books().refresh();
    side.books().with(|l| l.set_learning(false)).unwrap();
    (side, claimed)
}

fn location(id: &str) -> till_pos::square::SquareLocation {
    serde_json::from_value(serde_json::json!({
        "id": id, "name": "Counter", "country": "US", "currency": "USD"
    }))
    .unwrap()
}

fn payment(id: &str) -> Fact {
    Fact::PosPaymentSeen(PaymentSeen {
        pos: PointOfSale::Square,
        payment: PaymentId::new(id),
        amount: till_pos::Money::new(1388, till_pos::CurrencyCode::USD),
        tender: Tender::Monero,
        status: "COMPLETED".into(),
        device_id: None,
        device_name: None,
        receipt_number: Some("bP9m".into()),
        order_ref: None,
        title: None,
        created_at: "2026-10-22T16:07:00Z".parse().unwrap(),
        updated_at: till_types::UnixMillis(1_761_149_220_000),
    })
}

fn order(payment: &str, order: &str) -> Fact {
    Fact::OrderCreated(OrderCreated {
        sale: till_ledger::SaleKey::pos(PointOfSale::Square, payment),
        order_id: OrderId::new(order),
        url: None,
        payment_uri: "monero:8Bx?tx_amount=0.08".into(),
        address: "8Bx".into(),
        xmr: "0.08".into(),
        price: till_pos::Price::new("13.88", "USD"),
        reference: Some("bP9m".into()),
        expires_at: till_types::UnixSeconds(1_893_456_000),
        confirmations_required: None,
        device_name: None,
        renew: None,
    })
}

fn observed(order: &str, phase: SalePhase) -> Fact {
    Fact::OrderObserved(OrderObserved {
        order_id: OrderId::new(order),
        phase,
        confirmations: None,
        is_final: false,
        received: None,
        overpaid: false,
        quote: None,
        renew: None,
    })
}

fn kinds(claims: &Mutex<Vec<Claim>>, kind: AlertKind) -> usize {
    claims
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.kind == kind)
        .count()
}

/// The owner's iPad on 2026-11-09 (docs/ledger.md §1): while the app has
/// one sale paid, the extension's round writes older observations of it
/// and of another sale. No second sale turns paid, the paid one stays paid,
/// and nothing is alerted twice.
#[test]
fn an_extension_writing_stale_facts_never_pays_another_sale_or_alerts_twice() {
    let platform = platform();
    let dir = tempfile::tempdir().unwrap();
    let (app, claimed) = side(&platform, &dir, Process::App);
    let (extension, _) = side(&platform, &dir, Process::Extension);
    for fact in [
        payment("p1"),
        order("p1", "o1"),
        payment("p2"),
        order("p2", "o2"),
    ] {
        app.books().record(fact);
    }
    assert_eq!(kinds(&claimed, AlertKind::NewSale), 2);
    app.books().record(observed("o1", SalePhase::Paid));
    let paid = kinds(&claimed, AlertKind::Paid) + kinds(&claimed, AlertKind::PaidMoment);
    assert_eq!(paid, 1);

    // The extension's round, from before it heard of the payment.
    for fact in [
        payment("p1"),
        order("p1", "o1"),
        observed("o1", SalePhase::Waiting),
        observed("o1", SalePhase::Seen),
        payment("p2"),
        order("p2", "o2"),
        observed("o2", SalePhase::Waiting),
    ] {
        extension.books().record(fact);
    }
    assert!(
        extension
            .books()
            .claim_due(ClaimPolicy::NewestNewSale)
            .is_empty(),
        "nothing new for the extension to show"
    );
    extension.books().suspend();

    // The app hears the extension's round (the Darwin notification).
    app.books().refresh();
    let due = app.books().claim_due(ClaimPolicy::All);
    assert!(due.is_empty(), "{due:?}");
    let state = app.books().state();
    let phase = |order: &str| {
        state
            .sales
            .iter()
            .find(|s| s.order_id == order)
            .map(|s| s.phase)
    };
    assert_eq!(phase("o1"), Some(SalePhase::Paid), "still paid");
    assert_eq!(phase("o2"), Some(SalePhase::Waiting), "never paid");
    let paid = kinds(&claimed, AlertKind::Paid) + kinds(&claimed, AlertKind::PaidMoment);
    assert_eq!(paid, 1, "one paid alert");
    assert_eq!(
        kinds(&claimed, AlertKind::NewSale),
        2,
        "one new-sale alert each"
    );
}

/// Signing out of the point of sale, or changing its account, deletes the
/// old scope's rows (docs/ledger.md §4).
#[test]
fn another_point_of_sale_account_deletes_the_old_scopes_sales() {
    let platform = platform();
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = side(&platform, &dir, Process::App);
    app.books().record(payment("p1"));
    app.books().record(order("p1", "o1"));
    assert_eq!(app.books().state().sales.len(), 1);
    app.kept
        .update(|m| m.location = Some(location("L2")))
        .unwrap();
    app.books().rescope();
    assert!(app.books().state().sales.is_empty(), "the new scope's");
    app.kept
        .update(|m| m.location = Some(location("L1")))
        .unwrap();
    app.books().rescope();
    assert!(
        app.books().state().sales.is_empty(),
        "the old scope's rows were deleted"
    );
}

/// Decision 214: how a new sale shows up is kept in the ledger as the
/// platform's store keeps it, and a ledger that was given the variant's
/// name before has it replaced.
#[test]
fn show_sales_is_kept_in_the_ledger_as_its_wire_word() {
    let platform = platform();
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = side(&platform, &dir, Process::App);
    let settings = || app.books().with(|ledger| ledger.settings()).unwrap();
    // As the engine kept it before.
    app.books().keep_settings(
        &[("sales.show", Some("FloatingWindow".to_owned()))],
        till_types::UnixMillis(1_000),
    );
    app.kept
        .update(|m| m.show_sales = crate::ShowSales::FloatingWindow)
        .unwrap();
    app.books().keep_settings(
        &app.kept.model().ledger_settings(),
        till_types::UnixMillis(2_000),
    );
    assert_eq!(
        settings().get("sales.show").map(String::as_str),
        Some("pip")
    );
    assert_eq!(
        crate::ShowSales::from_wire(Some("FloatingWindow")),
        crate::ShowSales::FloatingWindow,
        "the old word reads as the same choice"
    );
    app.kept
        .update(|m| m.show_sales = crate::ShowSales::Alert)
        .unwrap();
    app.books().keep_settings(
        &app.kept.model().ledger_settings(),
        till_types::UnixMillis(3_000),
    );
    assert_eq!(settings().get("sales.show"), None);
    assert_eq!(
        settings().get("counter.others").map(String::as_str),
        Some("true")
    );
}

/// A scope that changed while the app wasn't running (another location
/// chosen, say) has its old rows deleted as the app next opens the ledger,
/// keeping only the current scope's; the extension leaves that to the app.
#[test]
fn the_app_deletes_old_scopes_rows_at_its_next_open() {
    let platform = platform();
    let dir = tempfile::tempdir().unwrap();
    {
        let (app, _) = side(&platform, &dir, Process::App);
        app.books().record(payment("p1"));
        app.books().record(order("p1", "o1"));
        assert_eq!(app.books().state().sales.len(), 1);
        app.books().suspend();
    }
    let at =
        |id: &str| format!(r#"{{"id":"{id}","name":"Counter","country":"US","currency":"USD"}}"#);
    platform.set("square.location", &at("L2"));
    {
        let (extension, _) = side(&platform, &dir, Process::Extension);
        extension.books().suspend();
    }
    platform.set("square.location", &at("L1"));
    {
        let (app, _) = side(&platform, &dir, Process::App);
        assert_eq!(
            app.books().state().sales.len(),
            1,
            "the extension deleted nothing"
        );
        app.books().suspend();
    }
    platform.set("square.location", &at("L2"));
    {
        let (app, _) = side(&platform, &dir, Process::App);
        assert!(app.books().state().sales.is_empty(), "the new scope's");
        app.books().suspend();
    }
    platform.set("square.location", &at("L1"));
    let (app, _) = side(&platform, &dir, Process::App);
    assert!(
        app.books().state().sales.is_empty(),
        "the old scope's rows were deleted at the open"
    );
}

/// An older build's kept shift and alert record become the ledger's once,
/// whichever process opens it first, and are deleted (docs/ledger.md §10).
#[test]
fn the_kept_shift_is_imported_once_by_whichever_process_opens_first() {
    let platform = platform();
    platform.set(
        "shift.sales",
        r#"[{"orderId":"ord_1","amount":"13.88","currency":"USD","reference":"bP9m","code":{"uri":"monero:8Bx?tx_amount=0.08","hasReceipt":false},"phase":"Paid","expiresAt":1893456000,"xmr":"0.08","address":"8Bx","found":{"id":"pay_monero","amountMinor":1388,"currency":"USD","reference":"bP9m","createdAt":"2026-10-22T16:07:00Z"}}]"#,
    );
    platform.set("alerts.posted", r#"["ord_1"]"#);
    let dir = tempfile::tempdir().unwrap();
    let (extension, _) = side(&platform, &dir, Process::Extension);
    let (app, claimed) = side(&platform, &dir, Process::App);
    assert_eq!(platform.get("shift.sales"), None);
    assert_eq!(platform.get("alerts.posted"), None);
    let imports = app
        .books()
        .with(|l| l.journal())
        .unwrap()
        .iter()
        .filter(|row| row.kind == "Imported")
        .count();
    assert_eq!(imports, 1);
    let sales = app.books().state().sales;
    assert_eq!(sales.len(), 1);
    assert_eq!(sales[0].phase, SalePhase::Paid);
    assert_eq!(extension.books().state().sales.len(), 1);
    // A later look at the same sale tells no one anything.
    app.books().record(order("pay_monero", "ord_1"));
    assert!(claimed.lock().unwrap().is_empty(), "{:?}", claimed.lock());
}

/// Decision 216: the scope is the kept API address's text, as it was
/// before the address had a type, so no kept ledger's rows are deleted as
/// changed (decision 166).
#[test]
fn the_scope_of_a_kept_connection_is_as_it_was() {
    let platform = platform();
    let kept = Kept::load(Store::new(platform), Arc::new(|_| {})).unwrap();
    let model = kept.model();
    assert_eq!(
        crate::ledger::scope_of(&model),
        till_ledger::Scope::of(
            "https://pay.example/api/v1",
            Some((PointOfSale::Square, "L1"))
        )
    );
    assert_eq!(
        crate::ledger::scope_of(&model).as_str(),
        "38d75479b20222a134f0839f65bd110a"
    );
}
