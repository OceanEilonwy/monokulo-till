//! The persisted model against what `Stores.kt` writes (docs/ios-plan.md §3,
//! "Migration of saved state"): every key, with a value of the shape the
//! Kotlin's setter and serializer give it, read into the model and written
//! back byte for byte.

use std::sync::Arc;

use monokulo_client::{Connection, PointOfSale};
use till_bell::{BellAddress, WakeUpChoice, WakeUpStore};
use till_pos::{CursorStore, TokenStore};

use super::*;
use crate::testing::FakePlatform;

/// A sale as the Kotlin app saved it (till-sales' `THEIRS`).
const SHIFT: &str = concat!(
    r#"[{"orderId":"ord_2","amount":"5.00","currency":"USD","#,
    r#""code":{"uri":"monero:8Cy?tx_amount=0.03","hasReceipt":false},"#,
    r#""phase":"Paid","expiresAt":99,"xmr":"0.03","address":"8Cy","deviceName":"Back","#,
    r#""url":"https://pay.example/monokulo/api/v1/orders/ord_2","settled":true,"createdAt":300,"#,
    r#""seenAt":310,"paidAt":320,"confirmations":10,"confirmationsRequired":10}]"#
);

const REGISTRATION: &str = r#"{"bell":{"url":"https://bell.monokulo.com"},"registration_id":"br_1","vapid_public_key":"BOS-key","subscription":{"endpoint":"https://ntfy.sh/up1","p256dh":"BKey","auth":"auth"},"distributor":"io.heckel.ntfy","point_of_sale":"clover","environment":"europe"}"#;

/// Every key `Stores.kt` writes, each as its setter writes it, with values
/// none of which is the default (a default is often written as nothing).
/// The serializers: `Connection.save` (kotlinx, `explicitNulls = false`,
/// defaults left out); `SquareLocation`, `CloverMerchant`, `PosTokens`,
/// `BellAddress`, `BellRegistration` and `WakeUpChoice` (`@Serializable`,
/// snake_case `@SerialName`s, the sealed choice's `type`); `PkceState`
/// (camelCase, no `@SerialName`); `SetSerializer(String)` for the handled
/// ids; `TillSale` for the shift; and `toString()` for the rest.
const KOTLIN: &[(&str, &str)] = &[
    (
        "connection",
        r#"{"app_key":"ak_1","api":"https://pay.example/api/v1","onion_api":"http://abcdef.onion/api/v1","tor":true}"#,
    ),
    (
        "square.location",
        r#"{"id":"L1","name":"Front counter","business_name":"Café Lumen","country":"US","currency":"USD","timezone":"America/New_York","status":"ACTIVE"}"#,
    ),
    ("pos.kind", "clover"),
    ("pos.skipped", "true"),
    ("shift.on", "false"),
    ("shift.started", "1761300000"),
    ("setup.done", "true"),
    ("bubble.on", "false"),
    ("bubble.left", "true"),
    ("bubble.y", "412"),
    (
        "square.pending",
        r#"{"verifier":"v-86","state":"s-43","applicationId":"clv-bell","pointOfSale":"clover"}"#,
    ),
    ("square.application", "sq0idp-bell"),
    ("clover.application", "clv-bell"),
    (
        "wakeups.choice",
        r#"{"type":"own","bell":{"url":"https://bell.example.org"}}"#,
    ),
    ("wakeups.own", r#"{"url":"https://bell.example.org"}"#),
    ("wakeups.registration", REGISTRATION),
    ("debug.square.base", "http://10.0.2.2:8081"),
    ("debug.clover.base", "http://10.0.2.2:8082"),
    (
        "clover.merchant",
        r#"{"id":"MID1","name":"Lumen","currency":"EUR","timezone":"Europe/Paris","country":"FR"}"#,
    ),
    (
        "square.tokens",
        r#"{"access_token":"EAAA1","expires_at":"2026-11-30T00:00:00Z","refresh_token":"r1","merchant_id":"M1"}"#,
    ),
    (
        "clover.tokens",
        r#"{"access_token":"cl1","expires_at":"2026-11-08T12:00:00Z","merchant_id":"MID1"}"#,
    ),
    ("square.since", "2026-10-22T16:07:00.512Z"),
    ("square.handled", r#"["pay_1","pay_2"]"#),
    ("clover.since", "2026-10-22T16:00:00Z"),
    ("clover.handled", r#"["CP1"]"#),
    ("clover.tender.MID1", "TND1"),
    ("shift.sales", SHIFT),
];

fn store(platform: &Arc<FakePlatform>) -> Store {
    Store::new(platform.clone())
}

fn quiet() -> Log {
    Arc::new(|_| {})
}

#[test]
fn every_key_stores_kt_writes_reads_into_the_model() {
    let platform = FakePlatform::with(KOTLIN);
    let model = store(&platform).load().unwrap();
    assert_eq!(
        model.connection,
        Some(Connection {
            app_key: "ak_1".into(),
            api: "https://pay.example/api/v1".into(),
            onion_api: Some("http://abcdef.onion/api/v1".into()),
            tor: true,
        })
    );
    assert_eq!(model.location.as_ref().unwrap().name, "Front counter");
    assert_eq!(model.point_of_sale, Some(PointOfSale::Clover));
    assert!(model.without_pos);
    assert!(!model.shift_on);
    assert_eq!(
        model.shift_started_at,
        Some(till_types::UnixSeconds(1_761_300_000))
    );
    assert!(model.setup_done);
    assert!(!model.bubble_on && model.bubble_left);
    assert_eq!(model.bubble_y, Some(412));
    let pending = model.pending_sign_in.as_ref().unwrap();
    assert_eq!(pending.point_of_sale, PointOfSale::Clover);
    assert_eq!(pending.pkce.verifier.expose_str(), "v-86");
    assert_eq!(pending.pkce.state.as_str(), "s-43");
    assert_eq!(pending.application_id.as_deref(), Some("clv-bell"));
    assert_eq!(model.square_application_id.as_deref(), Some("sq0idp-bell"));
    assert_eq!(model.clover_application_id.as_deref(), Some("clv-bell"));
    let bell = BellAddress::new("https://bell.example.org").unwrap();
    assert_eq!(
        model.wake_up_choice,
        WakeUpChoice::OwnBell { bell: bell.clone() }
    );
    assert_eq!(model.own_bell, Some(bell));
    assert_eq!(
        model
            .registration
            .as_ref()
            .unwrap()
            .point_of_sale
            .as_deref(),
        Some("clover")
    );
    assert_eq!(
        model.square_base_url.as_deref(),
        Some("http://10.0.2.2:8081")
    );
    assert_eq!(
        model.clover_base_url.as_deref(),
        Some("http://10.0.2.2:8082")
    );
    assert_eq!(model.clover_merchant.as_ref().unwrap().id.as_str(), "MID1");
    assert_eq!(model.place_name().as_deref(), Some("Lumen"));

    let log = quiet();
    let s = store(&platform);
    let square = PosTokenStore::new(s.clone(), PointOfSale::Square, log.clone());
    assert_eq!(
        square
            .load()
            .unwrap()
            .unwrap()
            .refresh_token
            .as_ref()
            .map(|t| t.expose_str()),
        Some("r1")
    );
    let clover = PosTokenStore::new(s.clone(), PointOfSale::Clover, log.clone());
    assert_eq!(clover.load().unwrap().unwrap().refresh_token, None);
    let cursor = PosCursor::new(s.clone(), PointOfSale::Square, log.clone());
    assert_eq!(
        cursor.since().unwrap().as_ref().map(|t| t.as_str()),
        Some("2026-10-22T16:07:00.512Z")
    );
    let handled: Vec<String> = cursor
        .handled()
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(handled, ["pay_1", "pay_2"]);
    assert_eq!(
        Tenders::new(s.clone(), log.clone())
            .tender_id(&mid1())
            .unwrap()
            .as_ref()
            .map(|t| t.as_str()),
        Some("TND1")
    );
    let shift = s.shift_sales().unwrap();
    assert_eq!(shift.len(), 1);
    assert_eq!(shift[0].order_id, "ord_2");
}

#[test]
fn the_model_writes_every_key_back_as_stores_kt_wrote_it() {
    let kotlin = FakePlatform::with(KOTLIN);
    let read = store(&kotlin);
    let model = read.load().unwrap();

    let rust = Arc::new(FakePlatform::default());
    let write = store(&rust);
    write.save(&Model::default(), &model).unwrap();
    let log = quiet();
    for pos in PointOfSale::ALL {
        PosTokenStore::new(write.clone(), pos, log.clone()).save(
            PosTokenStore::new(read.clone(), pos, log.clone())
                .load()
                .unwrap(),
        );
        let from = PosCursor::new(read.clone(), pos, log.clone());
        PosCursor::new(write.clone(), pos, log.clone())
            .save(&from.since().unwrap().unwrap(), &from.handled().unwrap());
    }
    Tenders::new(write.clone(), log.clone()).save(&mid1(), Some(&"TND1".into()));
    write.set_shift_sales(&read.shift_sales().unwrap()).unwrap();

    assert_eq!(rust.all(), kotlin.all(), "byte for byte, and no key more");
}

#[test]
fn a_till_from_before_there_was_a_choice_reads_as_square() {
    let tokens =
        r#"{"access_token":"EAAA1","expires_at":"2026-11-30T00:00:00Z","merchant_id":"M1"}"#;
    let platform = FakePlatform::with(&[
        ("square.tokens", tokens),
        ("square.skipped", "true"),
        ("square.pending", r#"{"verifier":"v","state":"s"}"#),
    ]);
    let s = store(&platform);
    let model = s.load().unwrap();
    assert_eq!(model.point_of_sale, Some(PointOfSale::Square));
    assert!(model.without_pos, "square.skipped still counts");
    assert_eq!(
        model.pending_sign_in.unwrap().point_of_sale,
        PointOfSale::Square
    );
    s.set_without_pos(false).unwrap();
    assert_eq!(platform.get("square.skipped"), None, "the old key goes");
    assert!(!s.without_pos().unwrap());
}

#[test]
fn what_doesnt_read_is_none_as_the_kotlin_made_it() {
    let platform = FakePlatform::with(&[
        ("connection", "{not json"),
        ("pos.kind", "toast"),
        ("shift.started", "soon"),
        (
            "square.pending",
            r#"{"verifier":"v","state":"s","pointOfSale":"toast"}"#,
        ),
        ("wakeups.choice", r#"{"type":"carrier-pigeon"}"#),
        ("shift.sales", "[{"),
    ]);
    platform
        .secrets
        .lock()
        .unwrap()
        .insert("square.location".into(), vec![0xff, 0xfe]);
    let s = store(&platform);
    let model = s.load().unwrap();
    assert_eq!(model.connection, None);
    assert_eq!(model.point_of_sale, None, "not Square: one was chosen");
    assert_eq!(model.shift_started_at, None);
    assert_eq!(model.pending_sign_in, None);
    assert_eq!(model.wake_up_choice, WakeUpChoice::TimerOnly);
    assert_eq!(model.location, None);
    assert!(s.shift_sales().unwrap().is_empty());
}

#[test]
fn defaults_are_kept_as_the_setters_keep_them() {
    let platform = Arc::new(FakePlatform::default());
    let s = store(&platform);
    let mut on = Model {
        setup_done: true,
        without_pos: true,
        wake_up_choice: WakeUpChoice::MonokuloBell,
        square_base_url: Some("http://stand-in".into()),
        ..Model::default()
    };
    s.save(&Model::default(), &on).unwrap();
    assert_eq!(
        platform.get("wakeups.choice").as_deref(),
        Some(r#"{"type":"monokulo"}"#)
    );
    let off = Model {
        shift_on: false,
        setup_done: false,
        without_pos: false,
        wake_up_choice: WakeUpChoice::TimerOnly,
        square_base_url: Some("  ".into()),
        ..on.clone()
    };
    s.save(&on, &off).unwrap();
    assert_eq!(platform.get("shift.on").as_deref(), Some("false"));
    for removed in [
        "setup.done",
        "pos.skipped",
        "wakeups.choice",
        "debug.square.base",
    ] {
        assert_eq!(platform.get(removed), None, "{removed}");
    }
    on.shift_on = true;
    s.save(&off, &on).unwrap();
    assert_eq!(platform.get("shift.on").as_deref(), Some("true"));

    s.set_shift_sales(&[]).unwrap();
    assert_eq!(
        platform.get("shift.sales"),
        None,
        "an empty shift is nothing"
    );
    s.save_cursor(PointOfSale::Clover, "2026-10-22T16:00:00Z", &[])
        .unwrap();
    assert_eq!(platform.get("clover.handled").as_deref(), Some("[]"));
}

#[test]
fn counters_cash_and_how_sales_show_up_are_kept_and_the_device_goes_with_the_store() {
    let platform = Arc::new(FakePlatform::default());
    let s = store(&platform);
    let fresh = s.load().unwrap();
    assert_eq!(fresh.counter_device, None);
    assert!(!fresh.alert_every, "this device's alerts, once known");
    assert!(fresh.show_others && fresh.cash_as_monero, "absent is on");
    assert_eq!(fresh.show_sales, crate::ShowSales::Alert);
    let set = Model {
        counter_device: Some(CounterKept {
            id: "dev_front".into(),
            name: Some("Front iPad".into()),
            learned_at: till_types::UnixSeconds(1_762_700_000),
        }),
        alert_every: true,
        show_others: false,
        cash_as_monero: false,
        show_sales: crate::ShowSales::SideBySide,
        ..Model::default()
    };
    s.save(&Model::default(), &set).unwrap();
    assert_eq!(
        platform.get("counter.device").as_deref(),
        Some(r#"{"id":"dev_front","name":"Front iPad","learnedAt":1762700000}"#)
    );
    assert_eq!(platform.get("counter.every").as_deref(), Some("true"));
    assert_eq!(platform.get("counter.others").as_deref(), Some("false"));
    assert_eq!(platform.get("cash.monero").as_deref(), Some("false"));
    assert_eq!(platform.get("sales.show").as_deref(), Some("side"));
    assert_eq!(s.load().unwrap(), set);
    let side = Model {
        show_sales: crate::ShowSales::Alert,
        ..set.clone()
    };
    s.save(&set, &side).unwrap();
    assert_eq!(platform.get("sales.show"), None, "the alert is the default");

    let forgotten = s.forget_all(&side).unwrap();
    assert_eq!(forgotten.counter_device, None, "the device is the store's");
    assert!(!forgotten.alert_every);
    assert!(!forgotten.show_others && !forgotten.cash_as_monero);
    assert_eq!(forgotten.show_sales, crate::ShowSales::Alert);
    assert_eq!(platform.get("counter.device"), None);
}

/// The keys only the iOS app writes, each as it writes it and none the
/// default: Settings › Dynamic Island's two switches.
const IOS: &[(&str, &str)] = &[("island.activity", "false"), ("island.code", "true")];

#[test]
fn the_islands_switches_read_and_write_back_and_are_absent_on_android() {
    let android = store(&FakePlatform::with(KOTLIN)).load().unwrap();
    assert!(
        android.live_activity,
        "absent is on; the engine reads it on iPhone only"
    );
    assert!(!android.code_in_island, "absent is off");

    let ios = FakePlatform::with(IOS);
    let model = store(&ios).load().unwrap();
    assert!(!model.live_activity);
    assert!(model.code_in_island);
    let rust = Arc::new(FakePlatform::default());
    store(&rust).save(&Model::default(), &model).unwrap();
    assert_eq!(rust.all(), ios.all(), "byte for byte, and no key more");

    let back = Model {
        live_activity: true,
        code_in_island: false,
        ..model.clone()
    };
    store(&rust).save(&model, &back).unwrap();
    assert_eq!(rust.get("island.activity").as_deref(), Some("true"));
    assert_eq!(rust.get("island.code"), None, "off is nothing");

    let kept = Kept::load(store(&ios), quiet()).unwrap();
    kept.forget_all().unwrap();
    assert_eq!(
        ios.all(),
        FakePlatform::with(IOS).all(),
        "the switches stay"
    );
}

#[test]
fn forgetting_everything_forgets_what_stores_kt_forgot() {
    let platform = FakePlatform::with(KOTLIN);
    let kept = Kept::load(store(&platform), quiet()).unwrap();
    kept.forget_all().unwrap();
    let left: Vec<String> = platform.all().into_keys().collect();
    assert_eq!(
        left,
        [
            "bubble.left",
            "bubble.on",
            "bubble.y",
            "debug.clover.base",
            "debug.square.base",
            "shift.on"
        ]
    );
    let model = kept.model();
    assert!(!model.shift_on && !model.bubble_on, "the switches stay");
    assert_eq!(model.connection, None);
}

#[test]
fn a_store_that_cant_answer_is_an_error_never_none() {
    let platform = FakePlatform::with(KOTLIN);
    platform.fail(true);
    assert!(store(&platform).load().is_err());
    assert!(Kept::load(store(&platform), quiet()).is_err());

    platform.fail(false);
    let kept = Kept::load(store(&platform), quiet()).unwrap();
    platform.fail(true);
    assert!(kept.update(|m| m.connection = None).is_err());
    assert!(
        kept.model().connection.is_some(),
        "a refused write leaves the model as it was"
    );
    platform.fail(false);
    assert!(platform.get("connection").is_some());
}

/// Decisions 17 and 47: what the leaf crates read through each time says
/// when it couldn't be read, logged, rather than "none" (till-sales then
/// never saves over the shift, and till-pos never over the cursor or the
/// tender id).
#[test]
fn what_is_read_through_says_when_it_couldnt_be_read() {
    let platform = FakePlatform::with(KOTLIN);
    let lines = Arc::new(Mutex::new(Vec::new()));
    let log: Log = {
        let lines = lines.clone();
        Arc::new(move |line| lines.lock().unwrap().push(line))
    };
    let s = store(&platform);
    let cursor = PosCursor::new(s.clone(), PointOfSale::Square, log.clone());
    let tenders = Tenders::new(s.clone(), log);
    platform.fail(true);
    assert!(s.shift_sales().is_err());
    assert!(cursor.since().is_err());
    assert!(cursor.handled().is_err());
    assert!(tenders.tender_id(&mid1()).is_err());
    assert_eq!(lines.lock().unwrap().len(), 3, "each logged");
    platform.fail(false);
    assert_eq!(s.shift_sales().unwrap().len(), 1, "and all still there");
    assert_eq!(
        tenders.tender_id(&mid1()).unwrap().map(|t| t.0),
        Some("TND1".to_owned())
    );
}

#[test]
fn wake_ups_keep_the_change_in_memory_when_the_store_refuses() {
    let platform = Arc::new(FakePlatform::default());
    let kept = Arc::new(Kept::load(store(&platform), quiet()).unwrap());
    let wake_ups = WakeUpsKept(kept.clone());
    wake_ups.set_choice(WakeUpChoice::MonokuloBell);
    assert_eq!(
        platform.get("wakeups.choice").as_deref(),
        Some(r#"{"type":"monokulo"}"#)
    );
    platform.fail(true);
    wake_ups.set_choice(WakeUpChoice::TimerOnly);
    assert_eq!(wake_ups.choice(), WakeUpChoice::TimerOnly);
}

#[test]
fn a_sign_in_that_cant_be_read_is_an_error_never_signed_out() {
    let platform = FakePlatform::with(KOTLIN);
    let lines = Arc::new(Mutex::new(Vec::new()));
    let log: Log = {
        let lines = lines.clone();
        Arc::new(move |line| lines.lock().unwrap().push(line))
    };
    let tokens = PosTokenStore::new(store(&platform), PointOfSale::Square, log);
    platform.fail(true);
    assert!(tokens.load().is_err());
    assert_eq!(lines.lock().unwrap().len(), 1);
    platform.fail(false);
    assert!(
        tokens.load().unwrap().is_some(),
        "and nothing was saved over it"
    );
}

fn mid1() -> till_pos::clover::CloverMerchantId {
    till_pos::clover::CloverMerchantId::new("MID1").unwrap()
}
