//! The ledger's tests (docs/ledger.md §15, the crate's part).

use std::sync::Arc;

use monokulo_client::{PointOfSale, SalePhase};
use proptest::prelude::*;
use till_ledger::view::setting;
use till_ledger::{
    AlertKind, AlertState, ClaimPolicy, Fact, Imported, KeyMaterial, Ledger, LedgerError, MarkKind,
    OrderCreated, OrderObserved, OrderRead, PaymentPhase, PaymentSeen, Process, Quote, RefundSeen,
    RefundStatus, SaleKey, Scope, SecretStore, Tender, TestClock, UnixMillis, UnixSeconds,
    ViewSettings, may_make_order,
};

/// 2026-11-09 18:00 UTC.
const NOW: i64 = 1_794_247_200_000;

struct Fixture {
    dir: tempfile::TempDir,
    clock: TestClock,
    scope: Scope,
}

fn key() -> KeyMaterial {
    KeyMaterial {
        key: [7; 32],
        salt: [9; 16],
    }
}

impl Fixture {
    fn new() -> Fixture {
        Fixture {
            dir: tempfile::tempdir().unwrap(),
            clock: TestClock::at(NOW),
            scope: Scope::of("store", Some((PointOfSale::Square, "M1"))),
        }
    }

    fn open_as(&self, name: &str, process: Process) -> Ledger {
        let mut ledger = Ledger::open(
            &self.dir.path().join(name),
            &key(),
            self.scope.clone(),
            process,
            Arc::new(self.clock.clone()),
        )
        .unwrap();
        ledger.set_time_zone(jiff::tz::TimeZone::UTC);
        ledger
    }

    fn open(&self, process: Process) -> Ledger {
        self.open_as("till.db", process)
    }
}

fn sale(id: &str) -> SaleKey {
    SaleKey::pos(PointOfSale::Square, id)
}

fn payment(id: &str, device: &str, tender: Tender, updated_at: i64) -> Fact {
    Fact::PosPaymentSeen(PaymentSeen {
        pos: PointOfSale::Square,
        payment: id.into(),
        amount: till_types::Money::new(2450, "USD".parse().unwrap()),
        tender,
        status: "COMPLETED".into(),
        device_id: Some(device.into()),
        device_name: Some(format!("{device} iPad")),
        receipt_number: Some(format!("#{id}")),
        order_ref: Some(format!("sqo_{id}")),
        title: None,
        created_at: "2026-11-09T17:00:00Z".parse().unwrap(),
        updated_at: UnixMillis(updated_at),
    })
}

fn created(id: &str, order: &str) -> Fact {
    Fact::OrderCreated(OrderCreated {
        sale: sale(id),
        order_id: order.into(),
        url: Some(format!("https://shop.example/api/v1/orders/{order}")),
        payment_uri: format!("monero:8{order}?tx_amount=0.15"),
        address: format!("8{order}"),
        xmr: "0.15".into(),
        price: till_types::Price::new("24.50", "USD"),
        reference: Some(format!("#{id}")),
        expires_at: UnixMillis(NOW).to_seconds().plus(900),
        confirmations_required: Some(10),
        device_name: None,
        renew: Some(format!("https://shop.example/api/v1/orders/{order}")),
    })
}

fn observed(order: &str, phase: SalePhase, is_final: bool) -> Fact {
    Fact::OrderObserved(OrderObserved {
        order_id: order.into(),
        phase,
        confirmations: match phase {
            SalePhase::Confirming => Some(3),
            SalePhase::Paid => Some(10),
            _ => Some(0),
        },
        is_final,
        received: (phase == SalePhase::Partial).then_some(1_000),
        overpaid: false,
        quote: None,
        renew: None,
    })
}

/// When `created`'s code runs out, Unix seconds.
const FIRST_EXPIRY: i64 = NOW / 1000 + 900;

/// `observed`, said under the quote that runs out at `expires_at` for
/// `piconero`.
fn quoted(order: &str, phase: SalePhase, expires_at: i64, piconero: u64) -> Fact {
    let Fact::OrderObserved(o) = observed(order, phase, false) else {
        unreachable!("an observation")
    };
    Fact::OrderObserved(OrderObserved {
        quote: Some(Quote {
            expires_at: UnixSeconds(expires_at),
            piconero,
        }),
        ..o
    })
}

fn read(id: &str, name: &str) -> Fact {
    Fact::PosOrderRead(OrderRead {
        sale: sale(id),
        updated_at: UnixMillis(1),
        receipt: None,
        why: Some("The order has no items.".into()),
        customer_name: Some(name.into()),
    })
}

fn set(name: &str, value: &str, at: i64) -> Fact {
    Fact::SettingChanged {
        name: name.into(),
        value: Some(value.into()),
        at: UnixMillis(at),
    }
}

fn phase(ledger: &Ledger, id: &str) -> Option<PaymentPhase> {
    ledger.sale(&sale(id)).unwrap().and_then(|v| v.payment)
}

/// The shared facts the property tests draw from: two sales, three
/// orders, and observations in every phase.
fn pool() -> Vec<Fact> {
    let mut facts = vec![
        payment("p1", "front", Tender::Monero, 1),
        payment("p1", "front", Tender::Monero, 2),
        payment("p2", "back", Tender::Cash, 1),
        read("p1", "Sam"),
        Fact::PosOrderRead(OrderRead {
            sale: sale("p1"),
            updated_at: UnixMillis(2),
            receipt: None,
            why: None,
            customer_name: Some("Sam B".into()),
        }),
        Fact::PosRefundSeen(RefundSeen {
            sale: sale("p2"),
            refund_id: "r1".into(),
            amount_minor: 2450,
            status: RefundStatus::Pending,
        }),
        Fact::PosRefundSeen(RefundSeen {
            sale: sale("p2"),
            refund_id: "r1".into(),
            amount_minor: 2450,
            status: RefundStatus::Completed,
        }),
        created("p1", "o1"),
        // Two orders for one sale, as only a build before core.md decision
        // 167 made: the later code stands.
        created("p1", "o1b"),
        created("p2", "o2"),
        // A renewal of o2: its first quote's ending, and the new quote.
        quoted("o2", SalePhase::Expired, FIRST_EXPIRY, 150_000_000_000),
        quoted(
            "o2",
            SalePhase::Waiting,
            FIRST_EXPIRY + 1_200,
            160_000_000_000,
        ),
        quoted("o2", SalePhase::Seen, FIRST_EXPIRY + 1_200, 160_000_000_000),
    ];
    for order in ["o1", "o1b", "o2"] {
        for phase in SalePhase::ALL {
            for is_final in [false, true] {
                facts.push(observed(order, phase, is_final));
            }
        }
    }
    facts
}

fn shared_of(ledger: &Ledger) -> Vec<(String, String)> {
    let mut rows: Vec<_> = ledger
        .rows()
        .unwrap()
        .into_iter()
        .map(|r| (r.sale.text(), serde_json::to_string(&r.shared).unwrap()))
        .collect();
    rows.sort();
    rows
}

fn feed(ledger: &mut Ledger, facts: &[Fact], clock: &TestClock) {
    for fact in facts {
        clock.advance(7);
        ledger.record(fact.clone()).unwrap();
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, failure_persistence: None, ..ProptestConfig::default() })]

    /// §15.1 and §15.4: any permutation of a set of shared facts, with
    /// duplicates, merges to the same `SharedSale`, on tills whose settings
    /// differ. §15.2: the incremental projections equal a full replay.
    #[test]
    fn permutations_with_duplicates_merge_alike(
        picks in prop::collection::vec(0usize..64, 1..40),
        shuffle in Just(()).prop_perturb(|_, mut rng| rng.random::<u64>()),
    ) {
        let pool = pool();
        let facts: Vec<Fact> = picks.iter().map(|i| pool[i % pool.len()].clone()).collect();
        let mut shuffled = facts.clone();
        // A Fisher-Yates shuffle from the drawn seed, with every fact twice.
        shuffled.extend(facts.clone());
        let mut state = shuffle | 1;
        for i in (1..shuffled.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let j = (state % (i as u64 + 1)) as usize;
            shuffled.swap(i, j);
        }

        let fx = Fixture::new();
        let mut a = fx.open_as("a.db", Process::App);
        let mut b = fx.open_as("b.db", Process::App);
        b.record(set(setting::DEVICE, "front", 1)).unwrap();
        b.record(set(setting::CASH_AS_MONERO, "false", 2)).unwrap();
        feed(&mut a, &facts, &fx.clock);
        feed(&mut b, &shuffled, &fx.clock);
        prop_assert_eq!(shared_of(&a), shared_of(&b));

        let before = a.dump_projections().unwrap();
        a.rebuild().unwrap();
        prop_assert_eq!(before, a.dump_projections().unwrap());
    }
}

#[test]
fn an_unknown_order_waits_and_one_order_never_changes_another_sale() {
    let fx = Fixture::new();
    let mut ledger = fx.open(Process::App);
    ledger
        .record(payment("p1", "front", Tender::Monero, 1))
        .unwrap();
    ledger.record(created("p1", "o1")).unwrap();
    ledger
        .record(payment("p2", "front", Tender::Monero, 1))
        .unwrap();

    // §15.5: o2's events wait for its order.
    assert!(
        ledger
            .record(observed("o2", SalePhase::Paid, true))
            .unwrap()
            .changed
    );
    assert_eq!(phase(&ledger, "p2"), None);
    assert_eq!(phase(&ledger, "p1"), Some(PaymentPhase::Waiting));
    // A repeat is the same row.
    assert!(
        !ledger
            .record(observed("o2", SalePhase::Paid, true))
            .unwrap()
            .changed
    );

    let made = ledger.record(created("p2", "o2")).unwrap();
    assert!(made.changed);
    assert_eq!(
        phase(&ledger, "p2"),
        Some(PaymentPhase::Paid { overpaid: false })
    );
    assert_eq!(phase(&ledger, "p1"), Some(PaymentPhase::Waiting));

    // o1's observation touches p1 alone.
    ledger
        .record(observed("o1", SalePhase::Seen, false))
        .unwrap();
    assert_eq!(phase(&ledger, "p1"), Some(PaymentPhase::Seen));
    assert_eq!(
        phase(&ledger, "p2"),
        Some(PaymentPhase::Paid { overpaid: false })
    );
    let before = ledger.dump_projections().unwrap();
    ledger.rebuild().unwrap();
    assert_eq!(before, ledger.dump_projections().unwrap());
}

#[test]
fn only_changes_are_journalled() {
    let fx = Fixture::new();
    let mut ledger = fx.open(Process::App);
    ledger
        .record(payment("p1", "front", Tender::Monero, 1))
        .unwrap();
    ledger.record(created("p1", "o1")).unwrap();
    ledger
        .record(observed("o1", SalePhase::Confirming, false))
        .unwrap();
    let rows = ledger.journal().unwrap().len();
    // The 20-second confirmations read saying the same thing writes nothing.
    for _ in 0..5 {
        fx.clock.advance(20_000);
        let r = ledger
            .record(observed("o1", SalePhase::Confirming, false))
            .unwrap();
        assert!(!r.changed);
        assert!(
            !ledger
                .record(observed("o1", SalePhase::Seen, false))
                .unwrap()
                .changed
        );
    }
    assert_eq!(ledger.journal().unwrap().len(), rows);
}

/// §15.3, the iPad of 2026-11-09: an extension round writing older
/// observations while the app has Paid can't make another sale Paid, lower
/// a phase, or alert twice.
#[test]
fn a_stale_extension_round_moves_nothing_backwards() {
    let fx = Fixture::new();
    let mut app = fx.open(Process::App);
    let mut ext = fx.open(Process::Extension);
    app.record(payment("p1", "front", Tender::Monero, 1))
        .unwrap();
    let r = app.record(created("p1", "o1")).unwrap();
    assert_eq!(r.effects_claimed.len(), 1);
    app.posted(&r.effects_claimed[0]).unwrap();
    app.record(payment("p2", "front", Tender::Monero, 1))
        .unwrap();
    let r = app.record(created("p2", "o2")).unwrap();
    app.posted(&r.effects_claimed[0]).unwrap();
    app.record(Fact::Foreground {
        until: UnixMillis(NOW + 60_000),
    })
    .unwrap();
    let paid = app.record(observed("o1", SalePhase::Paid, true)).unwrap();
    assert_eq!(paid.effects_claimed.len(), 1);
    assert_eq!(paid.effects_claimed[0].kind, AlertKind::PaidMoment);

    // The extension read both orders a while ago.
    for fact in [
        payment("p1", "front", Tender::Monero, 1),
        created("p1", "o1"),
        observed("o1", SalePhase::Seen, false),
        observed("o1", SalePhase::Waiting, false),
        payment("p2", "front", Tender::Monero, 1),
        created("p2", "o2"),
        observed("o2", SalePhase::Waiting, false),
    ] {
        let about_p1 = matches!(&fact, Fact::OrderObserved(o) if o.order_id.as_str() == "o1")
            || matches!(&fact, Fact::OrderCreated(o) if o.order_id.as_str() == "o1");
        let r = ext.record(fact).unwrap();
        assert!(r.effects.is_empty());
        if about_p1 {
            assert!(!r.changed, "a stale read of the paid sale writes nothing");
        }
    }
    assert!(
        ext.claim_due(ClaimPolicy::NewestNewSale)
            .unwrap()
            .is_empty()
    );
    assert!(app.claim_due(ClaimPolicy::All).unwrap().is_empty());
    assert_eq!(
        phase(&app, "p1"),
        Some(PaymentPhase::Paid { overpaid: false })
    );
    assert_eq!(phase(&app, "p2"), Some(PaymentPhase::Waiting));
    // Paid again by a late read: no second alert.
    let again = app.record(observed("o1", SalePhase::Paid, false)).unwrap();
    assert!(again.effects.is_empty());
}

/// §15.4: two tills fed the same facts have equal `shared` columns; their
/// effects differ by settings alone.
#[test]
fn two_tills_differ_only_in_effects() {
    let fx = Fixture::new();
    let mut front = fx.open_as("front.db", Process::App);
    let mut back = fx.open_as("back.db", Process::App);
    front.record(set(setting::DEVICE, "front", 1)).unwrap();
    back.record(set(setting::DEVICE, "back", 1)).unwrap();
    let facts = [
        payment("p1", "front", Tender::Monero, 1),
        created("p1", "o1"),
        observed("o1", SalePhase::Paid, true),
    ];
    let mut front_effects = Vec::new();
    let mut back_effects = Vec::new();
    for fact in &facts {
        front_effects.extend(front.record(fact.clone()).unwrap().effects);
    }
    for fact in facts.iter().rev() {
        back_effects.extend(back.record(fact.clone()).unwrap().effects);
    }
    assert_eq!(shared_of(&front), shared_of(&back));
    assert_eq!(
        front_effects.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [AlertKind::NewSale, AlertKind::Paid]
    );
    assert!(back_effects.is_empty());
    let v = back.sale(&sale("p1")).unwrap().unwrap();
    assert!(v.other_counter && !v.alerts_here);
}

/// §15.6: one delivered alert per sale and kind across two connections; a
/// claim whose process died is delivered again, once.
#[test]
fn one_alert_per_sale_and_a_dead_claim_is_redelivered_once() {
    let fx = Fixture::new();
    let mut a = fx.open(Process::App);
    let mut b = fx.open(Process::App);
    a.record(payment("p1", "front", Tender::Monero, 1)).unwrap();
    b.record(payment("p1", "front", Tender::Monero, 1)).unwrap();
    let ra = a.record(created("p1", "o1")).unwrap();
    let rb = b.record(created("p1", "o1")).unwrap();
    assert_eq!(ra.effects_claimed.len(), 1);
    assert_eq!(ra.effects_claimed[0].notification_id, "sq:p1#new_sale");
    assert!(rb.effects_claimed.is_empty() && !rb.changed);

    // `a` dies before posting.
    drop(a);
    fx.clock.advance(10_000);
    assert!(b.claim_due(ClaimPolicy::All).unwrap().is_empty());
    fx.clock.advance(21_000);
    let again = b.claim_due(ClaimPolicy::All).unwrap();
    assert_eq!(again.len(), 1);
    assert!(again[0].redelivery);
    let mut c = fx.open(Process::Extension);
    assert!(c.claim_due(ClaimPolicy::NewestNewSale).unwrap().is_empty());
    b.posted(&again[0]).unwrap();
    fx.clock.advance(60_000);
    assert!(c.claim_due(ClaimPolicy::All).unwrap().is_empty());
    assert_eq!(
        b.alert(&sale("p1"), AlertKind::NewSale)
            .unwrap()
            .unwrap()
            .state,
        AlertState::Posted
    );
}

#[test]
fn the_extension_claims_only_the_newest_new_sale_and_leaves_the_rest() {
    let fx = Fixture::new();
    let mut ext = fx.open(Process::Extension);
    let mut app = fx.open(Process::App);
    for (id, order) in [("p1", "o1"), ("p2", "o2")] {
        ext.record(payment(id, "front", Tender::Monero, 1)).unwrap();
        let r = ext.record(created(id, order)).unwrap();
        assert_eq!(r.effects.len(), 1);
        assert!(r.effects_claimed.is_empty());
    }
    // Same rung time: the later key wins the tie, deterministically.
    let shown = ext.claim_due(ClaimPolicy::NewestNewSale).unwrap();
    assert_eq!(shown.len(), 1);
    let rest = app.claim_due(ClaimPolicy::All).unwrap();
    assert_eq!(rest.len(), 1);
    assert_ne!(rest[0].sale, shown[0].sale);
}

#[test]
fn a_new_sale_paid_before_anyone_showed_it_is_not_shown() {
    let fx = Fixture::new();
    let mut ext = fx.open(Process::Extension);
    let mut app = fx.open(Process::App);
    ext.record(payment("p1", "front", Tender::Monero, 1))
        .unwrap();
    ext.record(created("p1", "o1")).unwrap();
    ext.record(observed("o1", SalePhase::Paid, true)).unwrap();
    let claims = app.claim_due(ClaimPolicy::All).unwrap();
    assert_eq!(
        claims.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [AlertKind::Paid]
    );
    assert_eq!(
        app.alert(&sale("p1"), AlertKind::NewSale)
            .unwrap()
            .unwrap()
            .state,
        AlertState::Posted
    );
}

/// §15.7: a late Paid on an expired, cancelled, partly paid, closed or
/// refunded sale shows as Paid, flagged where needed, and alerts.
#[test]
fn a_late_payment_always_shows_and_alerts() {
    type Before = fn(&mut Ledger);
    let cases: [(&str, Before, bool, bool); 5] = [
        (
            "expired",
            |l| {
                l.record(observed("o1", SalePhase::Expired, false)).unwrap();
            },
            false,
            false,
        ),
        (
            "cancelled",
            |l| {
                l.record(observed("o1", SalePhase::Cancelled, false))
                    .unwrap();
            },
            false,
            false,
        ),
        (
            "partial",
            |l| {
                l.record(observed("o1", SalePhase::Partial, false)).unwrap();
            },
            false,
            false,
        ),
        (
            "closed",
            |l| {
                l.record(Fact::Mark {
                    sale: sale("p1"),
                    mark: MarkKind::Closed,
                    at: UnixMillis(NOW),
                })
                .unwrap();
            },
            true,
            false,
        ),
        (
            "refunded",
            |l| {
                l.record(Fact::PosRefundSeen(RefundSeen {
                    sale: sale("p1"),
                    refund_id: "r".into(),
                    amount_minor: 2450,
                    status: RefundStatus::Completed,
                }))
                .unwrap();
            },
            false,
            true,
        ),
    ];
    for (name, before, after_close, after_refund) in cases {
        let fx = Fixture::new();
        let mut l = fx.open(Process::App);
        l.record(payment("p1", "front", Tender::Monero, 1)).unwrap();
        l.record(created("p1", "o1")).unwrap();
        before(&mut l);
        let r = l.record(observed("o1", SalePhase::Paid, false)).unwrap();
        let v = l.sale(&sale("p1")).unwrap().unwrap();
        assert_eq!(v.phase, SalePhase::Paid, "{name}");
        assert_eq!(v.paid_after_close, after_close, "{name}");
        assert_eq!(v.paid_after_refund, after_refund, "{name}");
        assert_eq!(
            r.effects.iter().map(|e| e.kind).collect::<Vec<_>>(),
            [AlertKind::Paid],
            "{name}"
        );
    }
}

#[test]
fn a_double_spend_stays_unless_monokulo_settles_it_paid() {
    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    l.record(payment("p1", "front", Tender::Monero, 1)).unwrap();
    l.record(created("p1", "o1")).unwrap();
    l.record(observed("o1", SalePhase::DoubleSpend, false))
        .unwrap();
    l.record(observed("o1", SalePhase::Paid, false)).unwrap();
    assert_eq!(phase(&l, "p1"), Some(PaymentPhase::DoubleSpend));
    l.record(observed("o1", SalePhase::Paid, true)).unwrap();
    assert_eq!(
        phase(&l, "p1"),
        Some(PaymentPhase::Paid { overpaid: false })
    );
}

/// Core.md decision 168: paying more than asked is an overpayment: it alerts
/// once, and the cashier saying the extra went back settles it.
#[test]
fn an_overpaid_sale_alerts_once_until_the_extra_is_marked_refunded() {
    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    l.record(payment("p1", "front", Tender::Monero, 1)).unwrap();
    l.record(created("p1", "o1")).unwrap();
    let r = l.record(observed("o1", SalePhase::Paid, false)).unwrap();
    assert!(
        r.effects_claimed
            .iter()
            .all(|c| c.kind != AlertKind::Overpaid),
        "an exact payment is not an overpayment"
    );
    assert!(!l.sale(&sale("p1")).unwrap().unwrap().overpaid);

    let mut over = observed("o1", SalePhase::Paid, true);
    if let Fact::OrderObserved(o) = &mut over {
        o.overpaid = true;
    }
    let r = l.record(over).unwrap();
    let overpaid: Vec<_> = r
        .effects_claimed
        .iter()
        .filter(|c| c.kind == AlertKind::Overpaid)
        .collect();
    assert_eq!(overpaid.len(), 1, "the overpayment alerts");
    let v = l.sale(&sale("p1")).unwrap().unwrap();
    assert!(v.overpaid && v.extra_refunded_at.is_none());

    l.record(Fact::Mark {
        sale: sale("p1"),
        mark: MarkKind::ExtraRefunded,
        at: monokulo_client::UnixMillis(NOW),
    })
    .unwrap();
    let v = l.sale(&sale("p1")).unwrap().unwrap();
    assert!(v.overpaid && v.extra_refunded_at.is_some());
}

/// §15.8: New code renews the sale's one order (core.md decisions 167 to
/// 172): the same order with a new quote, waiting again with its new code;
/// the old quote's ending doesn't end it, whichever arrives first; the
/// renewal alerts nothing; a Paid against either quote shows; and a sale
/// whose order offers no `renew-order` isn't renewable.
#[test]
fn a_new_code_renews_the_one_order_with_a_new_quote() {
    let key = sale("p1").order_key().unwrap().key();
    assert!(key.starts_with("sq:") && !key.contains('#'), "{key}");

    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    l.record(payment("p1", "front", Tender::Monero, 1)).unwrap();
    l.record(created("p1", "o1")).unwrap();
    l.record(observed("o1", SalePhase::Expired, false)).unwrap();
    let v = l.sale(&sale("p1")).unwrap().unwrap();
    assert!(v.renewable && !v.live);
    assert_eq!(v.payment, Some(PaymentPhase::Expired));

    let renewed = FIRST_EXPIRY + 1_200;
    let r = l
        .record(quoted("o1", SalePhase::Waiting, renewed, 160_000_000_000))
        .unwrap();
    assert!(
        r.effects.is_empty(),
        "the cashier asked for it: no new alert"
    );
    let v = l.sale(&sale("p1")).unwrap().unwrap();
    assert_eq!(
        (v.order_id.as_str(), v.payment, v.live, v.expires_at),
        (
            "o1",
            Some(PaymentPhase::Waiting),
            true,
            UnixSeconds(renewed)
        )
    );
    assert_eq!(v.xmr, "0.16");
    assert_eq!(v.code.uri, "monero:8o1?tx_amount=0.16");
    // The first quote's ending, heard late from the stream, changes nothing.
    l.record(quoted(
        "o1",
        SalePhase::Expired,
        FIRST_EXPIRY,
        150_000_000_000,
    ))
    .unwrap();
    assert_eq!(phase(&l, "p1"), Some(PaymentPhase::Waiting));
    // A payment at the old price shows as Monokulo judges it.
    l.record(quoted("o1", SalePhase::Partial, renewed, 160_000_000_000))
        .unwrap();
    assert_eq!(
        phase(&l, "p1"),
        Some(PaymentPhase::Partial { received: 1_000 })
    );
    l.record(quoted("o1", SalePhase::Paid, FIRST_EXPIRY, 150_000_000_000))
        .unwrap();
    assert!(phase(&l, "p1").unwrap().is_paid());

    // The new quote's own ending ends it.
    l.record(payment("p2", "front", Tender::Monero, 1)).unwrap();
    l.record(created("p2", "o2")).unwrap();
    l.record(quoted("o2", SalePhase::Waiting, renewed, 160_000_000_000))
        .unwrap();
    l.record(quoted("o2", SalePhase::Expired, renewed, 160_000_000_000))
        .unwrap();
    assert_eq!(phase(&l, "p2"), Some(PaymentPhase::Expired));

    // An order an older Monokulo made offers no renewal.
    let Fact::OrderCreated(older) = created("p3", "o3") else {
        unreachable!("an order")
    };
    l.record(payment("p3", "front", Tender::Monero, 1)).unwrap();
    l.record(Fact::OrderCreated(OrderCreated {
        renew: None,
        ..older
    }))
    .unwrap();
    assert!(!l.sale(&sale("p3")).unwrap().unwrap().renewable);
}

/// A code this device's clock said ran out counts only for the quote it
/// ran out under: renewed, the code is up again, and when the renewed code
/// runs out by the clock it lapses again.
#[test]
fn a_clock_lapse_belongs_to_the_quote_it_lapsed() {
    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    l.record(payment("p1", "front", Tender::Monero, 1)).unwrap();
    l.record(created("p1", "o1")).unwrap();
    l.record(Fact::Mark {
        sale: sale("p1"),
        mark: MarkKind::ClockLapsed,
        at: UnixSeconds(FIRST_EXPIRY + 1).to_millis(),
    })
    .unwrap();
    assert!(l.sale(&sale("p1")).unwrap().unwrap().clock_lapsed);
    let renewed = FIRST_EXPIRY + 1_200;
    l.record(quoted("o1", SalePhase::Waiting, renewed, 160_000_000_000))
        .unwrap();
    let v = l.sale(&sale("p1")).unwrap().unwrap();
    assert!(v.live && !v.clock_lapsed, "renewed, the code is up again");
    l.record(Fact::Mark {
        sale: sale("p1"),
        mark: MarkKind::ClockLapsed,
        at: UnixSeconds(renewed + 1).to_millis(),
    })
    .unwrap();
    assert!(l.sale(&sale("p1")).unwrap().unwrap().clock_lapsed);
}

/// §15.9: settings never change `shared`; a cash sale gets an order only
/// from a till set to its device.
#[test]
fn settings_never_enter_the_merge() {
    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    l.record(payment("p1", "back", Tender::Cash, 1)).unwrap();
    let before = shared_of(&l);
    let v = l.sale(&sale("p1")).unwrap().unwrap();
    assert!(v.waiting_for_order && !v.offered && v.cash);
    assert!(may_make_order(&v, &l.view_settings().unwrap(), false));

    l.record(set(setting::DEVICE, "front", 2)).unwrap();
    l.record(set(setting::CASH_AS_MONERO, "false", 3)).unwrap();
    assert_eq!(shared_of(&l), before);
    let settings = l.view_settings().unwrap();
    let v = l.sale(&sale("p1")).unwrap().unwrap();
    assert!(v.offered && v.order_id == "cash:p1");
    assert!(!may_make_order(&v, &settings, false));
    assert!(
        may_make_order(&v, &settings, true),
        "the cashier may take it"
    );

    let on = ViewSettings {
        cash_as_monero: true,
        ..settings.clone()
    };
    assert!(
        !may_make_order(&v, &on, false),
        "another counter's cash sale"
    );
    let back = ViewSettings {
        device: Some("back".into()),
        ..on.clone()
    };
    let v = till_ledger::view(
        &sale("p1"),
        &l.row(&sale("p1")).unwrap().unwrap().shared,
        &Default::default(),
        &back,
        UnixMillis(0),
    );
    assert!(may_make_order(&v, &back, false));
}

/// Core.md decision 167: with cash as Monero off, a cash sale is ignored
/// altogether: it is no row in the list, while a Monero sale is.
#[test]
fn with_cash_as_monero_off_cash_sales_are_ignored() {
    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    l.record(payment("cash1", "front", Tender::Cash, 1))
        .unwrap();
    l.record(payment("monero1", "front", Tender::Monero, 2))
        .unwrap();
    assert_eq!(l.sales().unwrap().len(), 2, "cash as Monero is on");

    l.record(set(setting::CASH_AS_MONERO, "false", 3)).unwrap();
    let keys: Vec<_> = l.sales().unwrap().into_iter().map(|v| v.sale).collect();
    assert_eq!(keys, vec![sale("monero1")], "the cash sale is gone");
    assert!(
        l.sale(&sale("cash1")).unwrap().is_some(),
        "still known by its key"
    );
}

const SHIFT: &str = r#"[
 {"orderId":"ord_2","amount":"5.00","currency":"USD","code":{"uri":"monero:8B2?tx_amount=0.03","hasReceipt":false},
  "phase":"Paid","expiresAt":1794248000,"xmr":"0.03","address":"8B2",
  "found":{"id":"pay_2","amountMinor":500,"currency":"USD","createdAt":"2026-11-09T17:30:00Z","deviceId":"front","cash":true},
  "settled":true,"createdAt":1794245400,"seenAt":1794245460,"paidAt":1794245520},
 {"orderId":"ord_1","amount":"13.88","currency":"USD","reference":"bP9m",
  "code":{"uri":"monero:8Bx?tx_amount=0.08&orf=abc","hasReceipt":true},
  "phase":"Expired","expiresAt":1794244000,"xmr":"0.08","address":"8Bx",
  "found":{"id":"pay_1","amountMinor":1388,"currency":"USD","reference":"bP9m","orderId":"sqo_1","createdAt":"2026-11-09T17:00:00Z","deviceId":"front"},
  "receipt":{"receipt":{"receipt_id":"bP9m","issue_timestamp":"2026-11-09T09:00:00-08:00"},"merchant":{"legal_name":"The Bean Room","country_code":"US"},
   "line_items":[{"description":"Flat white","quantity":1.0,"unit_price":13.88,"line_total":13.88}],"tax":{"tax_lines":[]},"total":13.88,"currency":"USD"},
  "customerName":"Sam","createdAt":1794243600,"endedAt":1794244000,"closedAt":1794244100,"refundPendingAt":1794244200},
 {"orderId":"ord_9","amount":"2.00","currency":"USD","code":{"uri":"monero:8B9?tx_amount=0.01","hasReceipt":false},
  "phase":"Waiting","expiresAt":1794248000,"xmr":"0.01","address":"8B9","deviceName":"Back iPad","createdAt":1794246000},
 {"orderId":"cash:pay_3","amount":"7.00","currency":"USD","code":{"uri":"","hasReceipt":false},
  "phase":"Waiting","expiresAt":0,"xmr":"","address":"",
  "found":{"id":"pay_3","amountMinor":700,"currency":"USD","createdAt":"2026-11-09T17:45:00Z","cash":true},
  "offered":true,"keptAsCashAt":1794246400,"createdAt":1794246300}
]"#;

/// §15.10: the kept shift's sales, marks and posted alerts are imported
/// once, with two openers.
#[test]
fn the_kept_shift_is_imported_once() {
    let fx = Fixture::new();
    let mut app = fx.open(Process::App);
    let mut ext = fx.open(Process::Extension);
    let posted = vec![
        "ord_1".to_owned(),
        "ord_2".to_owned(),
        "ord_unknown".to_owned(),
    ];
    let first = ext.import_shift(SHIFT, &posted).unwrap();
    assert_eq!(
        first,
        Imported::Done {
            sales: 4,
            alerts: 2
        }
    );
    assert_eq!(
        app.import_shift(SHIFT, &posted).unwrap(),
        Imported::AlreadyImported
    );
    assert!(app.learning().unwrap());

    let one = app.sale(&sale("pay_1")).unwrap().unwrap();
    assert_eq!(one.phase, SalePhase::Expired);
    assert_eq!(one.closed_at, Some(UnixSeconds(1794244100)));
    assert_eq!(one.refund_pending_at, Some(UnixSeconds(1794244200)));
    assert_eq!(one.customer_name.as_deref(), Some("Sam"));
    assert!(one.receipt.is_some() && one.code.has_receipt);
    assert_eq!(one.created_at, UnixSeconds(1794243600));
    let two = app.sale(&sale("pay_2")).unwrap().unwrap();
    assert_eq!(
        (two.phase, two.paid_at, two.seen_at),
        (
            SalePhase::Paid,
            Some(UnixSeconds(1794245520)),
            Some(UnixSeconds(1794245460))
        )
    );
    assert!(two.settled && two.cash);
    let nine = app.sale(&SaleKey::order("ord_9")).unwrap().unwrap();
    assert_eq!(nine.device_name.as_deref(), Some("Back iPad"));
    assert!(nine.code.why.is_some());
    let three = app.sale(&sale("pay_3")).unwrap().unwrap();
    assert_eq!(three.kept_as_cash_at, Some(UnixSeconds(1794246400)));
    assert_eq!(three.order_id, "cash:pay_3");
    assert_eq!(
        app.alert(&sale("pay_1"), AlertKind::NewSale)
            .unwrap()
            .unwrap()
            .state,
        AlertState::Posted
    );
    assert!(
        app.alert(&SaleKey::order("ord_9"), AlertKind::NewSale)
            .unwrap()
            .is_none()
    );

    let before = app.dump_projections().unwrap();
    app.rebuild().unwrap();
    assert_eq!(before, app.dump_projections().unwrap());
}

#[derive(Default)]
struct Secrets {
    kept: std::sync::Mutex<Option<Vec<u8>>>,
    fail: bool,
}

impl SecretStore for Secrets {
    fn get(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        if self.fail {
            return Err("before first unlock".into());
        }
        Ok(self.kept.lock().unwrap().clone())
    }

    fn put(&self, _: &str, value: Vec<u8>) -> Result<(), String> {
        if self.fail {
            return Err("before first unlock".into());
        }
        *self.kept.lock().unwrap() = Some(value);
        Ok(())
    }
}

/// §15.11, the ledger's part.
#[test]
fn the_key_states() {
    let fx = Fixture::new();
    let clock = Arc::new(fx.clock.clone());
    let failing = Secrets {
        fail: true,
        ..Secrets::default()
    };
    let e = Ledger::open_with_secrets(
        fx.dir.path(),
        &failing,
        fx.scope.clone(),
        Process::App,
        clock.clone(),
    );
    assert!(matches!(e, Err(LedgerError::KeyUnavailable(_))));
    assert!(
        std::fs::read_dir(fx.dir.path()).unwrap().next().is_none(),
        "nothing changed"
    );

    let secrets = Secrets::default();
    let opened = Ledger::open_with_secrets(
        fx.dir.path(),
        &secrets,
        fx.scope.clone(),
        Process::App,
        clock.clone(),
    )
    .unwrap();
    assert!(opened.created && !opened.recovered);
    let mut l = opened.ledger;
    assert!(l.learning().unwrap());
    l.set_learning(false).unwrap();
    l.record(payment("p1", "front", Tender::Monero, 1)).unwrap();
    l.close().unwrap();
    let again = Ledger::open_with_secrets(
        fx.dir.path(),
        &secrets,
        fx.scope.clone(),
        Process::App,
        clock.clone(),
    )
    .unwrap();
    assert!(!again.created && !again.recovered);
    assert!(again.ledger.row(&sale("p1")).unwrap().is_some());
    assert!(!again.ledger.learning().unwrap());
    again.ledger.close().unwrap();

    // The key is lost: the database moves aside, and the new one learns.
    *secrets.kept.lock().unwrap() = None;
    let recovered = Ledger::open_with_secrets(
        fx.dir.path(),
        &secrets,
        fx.scope.clone(),
        Process::App,
        clock,
    )
    .unwrap();
    assert!(recovered.recovered);
    let mut l = recovered.ledger;
    assert!(l.row(&sale("p1")).unwrap().is_none());
    l.record(payment("p1", "front", Tender::Monero, 1)).unwrap();
    let r = l.record(created("p1", "o1")).unwrap();
    assert_eq!(r.effects.len(), 1);
    assert!(r.effects_claimed.is_empty(), "learning: nothing delivered");
    assert_eq!(
        l.alert(&sale("p1"), AlertKind::NewSale)
            .unwrap()
            .unwrap()
            .state,
        AlertState::Posted
    );
    let lost = std::fs::read_dir(fx.dir.path())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("till.db.lost-")
        })
        .count();
    assert_eq!(lost, 1);
}

#[test]
fn a_wrong_key_is_refused_and_the_file_is_encrypted() {
    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    l.record(read("p1", "Sam Customer")).unwrap();
    l.close().unwrap();
    let path = fx.dir.path().join("till.db");
    let bytes = std::fs::read(&path).unwrap();
    let wal = std::fs::read(fx.dir.path().join("till.db-wal")).unwrap_or_default();
    for file in [&bytes, &wal] {
        assert!(!file.windows(12).any(|w| w == b"Sam Customer"));
    }
    let wrong = KeyMaterial {
        key: [1; 32],
        salt: [9; 16],
    };
    let e = Ledger::open(
        &path,
        &wrong,
        fx.scope.clone(),
        Process::App,
        Arc::new(fx.clock.clone()),
    );
    assert!(matches!(e, Err(LedgerError::WrongKey)));
}

/// §15.12: signing out, or changing the store or POS, deletes that scope's
/// rows.
#[test]
fn deleting_a_scope_deletes_its_rows_only() {
    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    l.record(payment("p1", "front", Tender::Monero, 1)).unwrap();
    l.record(created("p1", "o1")).unwrap();
    l.record(observed("o9", SalePhase::Seen, false)).unwrap();
    l.set_cursor("square.payments", "2026-11-09T17:00:00Z")
        .unwrap();
    let old = l.scope().clone();
    let other = Scope::of("store", Some((PointOfSale::Clover, "C1")));
    l.set_scope(other.clone());
    l.record(payment("p2", "front", Tender::Monero, 1)).unwrap();
    l.delete_scope(&old).unwrap();
    assert!(l.row(&sale("p2")).unwrap().is_some());
    l.set_scope(old);
    assert!(l.rows().unwrap().is_empty());
    assert!(l.cursor("square.payments").unwrap().is_none());
    assert!(l.order(&"o1".into()).unwrap().is_none());
    assert!(l.journal().unwrap().iter().all(|r| r.scope == other.0));
}

/// The rows of every scope but the ledger's go at once: the store or the
/// POS changed while the app wasn't running.
#[test]
fn only_the_current_scope_is_kept() {
    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    let current = l.scope().clone();
    for (scope, id) in [
        (Scope::of("store", Some((PointOfSale::Clover, "C1"))), "p1"),
        (Scope::of("old store", None), "p2"),
    ] {
        l.set_scope(scope);
        l.record(payment(id, "front", Tender::Monero, 1)).unwrap();
        l.set_cursor("square.refunds", "2026-11-09T17:00:00Z")
            .unwrap();
    }
    l.set_scope(current.clone());
    l.record(payment("p3", "front", Tender::Monero, 1)).unwrap();
    assert_eq!(l.keep_only_scope().unwrap(), 2);
    assert_eq!(l.keep_only_scope().unwrap(), 0);
    assert!(l.row(&sale("p3")).unwrap().is_some());
    assert!(l.journal().unwrap().iter().all(|r| r.scope == current.0));
}

/// A shift an older build kept, with sales it knew only from Monokulo.
const MONOKULO_ONLY_SHIFT: &str = r#"[
 {"orderId":"ord_7","amount":"24.50","currency":"USD","code":{"uri":"monero:8B7?tx_amount=0.15","hasReceipt":false},
  "phase":"Waiting","expiresAt":1794248000,"xmr":"0.15","address":"8B7","deviceName":"Front iPad",
  "createdAt":1794243600,"closedAt":1794244100},
 {"orderId":"ord_8","amount":"24.50","currency":"USD","code":{"uri":"monero:8B8?tx_amount=0.15","hasReceipt":false},
  "phase":"Paid","expiresAt":1794248000,"xmr":"0.15","address":"8B8","settled":true,
  "createdAt":1794243700,"paidAt":1794243800}
]"#;

/// After an upgrade, a sale the old build kept only from Monokulo is folded
/// into the point-of-sale sale whose order it is, once the till finds it at
/// the POS: one row, with the old row's marks and alerts and what Monokulo
/// said, nothing alerted again, and a replay, compaction and retention
/// that agree.
#[test]
fn a_sale_kept_from_monokulo_alone_is_folded_into_the_pos_sale_that_finds_it() {
    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    assert_eq!(
        l.import_shift(MONOKULO_ONLY_SHIFT, &["ord_7".to_owned()])
            .unwrap(),
        Imported::Done {
            sales: 2,
            alerts: 1
        }
    );
    l.set_learning(false).unwrap();
    let kept7 = SaleKey::order("ord_7");
    assert!(l.row(&kept7).unwrap().is_some());

    let mut claimed = Vec::new();
    for fact in [
        payment("p7", "front", Tender::Monero, 1),
        created("p7", "ord_7"),
        payment("p8", "front", Tender::Monero, 1),
        created("p8", "ord_8"),
    ] {
        fx.clock.advance(1000);
        claimed.extend(l.record(fact).unwrap().effects_claimed);
    }
    assert!(claimed.is_empty(), "nothing alerted again: {claimed:?}");

    let rows = l.rows().unwrap();
    assert_eq!(
        rows.iter().map(|r| r.sale.text()).collect::<Vec<_>>(),
        ["sq:p7", "sq:p8"],
        "the Monokulo-only rows are gone"
    );
    let seven = l.sale(&sale("p7")).unwrap().unwrap();
    assert_eq!(
        seven.closed_at,
        Some(UnixSeconds(1794244100)),
        "its close kept"
    );
    assert!(!seven.live);
    assert_eq!(seven.order_id, "ord_7");
    assert_eq!(
        l.alert(&sale("p7"), AlertKind::NewSale)
            .unwrap()
            .unwrap()
            .state,
        AlertState::Posted
    );
    assert!(l.alert(&kept7, AlertKind::NewSale).unwrap().is_none());
    let eight = l.sale(&sale("p8")).unwrap().unwrap();
    assert_eq!(eight.phase, SalePhase::Paid, "what Monokulo said, kept");
    assert_eq!(eight.paid_at, Some(UnixSeconds(1794243800)));
    assert!(eight.settled);
    assert_eq!(
        l.order(&"ord_7".into()).unwrap(),
        Some(sale("p7")),
        "its order is the POS sale's"
    );
    let kept_prefix = format!("{}:", "mk");
    assert!(
        l.journal().unwrap().iter().all(|r| !r
            .sale_key
            .as_deref()
            .is_some_and(|k| k.starts_with(&kept_prefix))),
        "its journal rows go with the POS sale"
    );

    // A later observation of the order reaches the POS sale.
    l.record(observed("ord_7", SalePhase::Seen, false)).unwrap();
    assert_eq!(phase(&l, "p7"), Some(PaymentPhase::Seen));

    let before = l.dump_projections().unwrap();
    l.rebuild().unwrap();
    assert_eq!(
        before,
        l.dump_projections().unwrap(),
        "a replay folds alike"
    );
    l.compact(true).unwrap();
    let after = l.dump_projections().unwrap();
    l.rebuild().unwrap();
    assert_eq!(after, l.dump_projections().unwrap());
    fx.clock.advance(31 * 24 * 60 * 60 * 1000);
    l.retain().unwrap();
    assert!(l.rows().unwrap().is_empty());
    assert!(
        l.journal().unwrap().iter().all(|r| r.sale_key.is_none()),
        "no row of the folded sale left behind"
    );
}

/// §15.2, after compaction: the projections still equal a replay, and an
/// ended sale before the shift has one journal row.
#[test]
fn compaction_keeps_the_projections() {
    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    // Rung up a week ago.
    let old = |id: &str| {
        let Fact::PosPaymentSeen(mut p) = payment(id, "front", Tender::Monero, 1) else {
            unreachable!()
        };
        p.created_at = "2026-11-02T17:00:00Z".parse().unwrap();
        Fact::PosPaymentSeen(p)
    };
    for fact in [
        old("p1"),
        created("p1", "o1"),
        observed("o1", SalePhase::Seen, false),
        observed("o1", SalePhase::Paid, true),
        old("p2"),
        created("p2", "o2"),
        payment("p3", "front", Tender::Monero, 1),
        created("p3", "o3"),
        observed("o3", SalePhase::Paid, true),
    ] {
        fx.clock.advance(1000);
        let r = l.record(fact).unwrap();
        for c in r.effects_claimed {
            l.posted(&c).unwrap();
        }
    }
    l.record(Fact::Foreground {
        until: UnixMillis(NOW + 1),
    })
    .unwrap();
    l.record(Fact::Foreground {
        until: UnixMillis(NOW + 2),
    })
    .unwrap();
    let before = l.dump_projections().unwrap();
    assert_eq!(l.compact(true).unwrap(), 1, "p1 alone: ended and old");
    assert_eq!(l.compact(false).unwrap(), 0, "at most hourly");
    let p1_rows: Vec<_> = l
        .journal()
        .unwrap()
        .into_iter()
        .filter(|r| r.sale_key.as_deref() == Some("sq:p1"))
        .collect();
    assert_eq!(p1_rows.len(), 1);
    assert_eq!(p1_rows[0].kind, "Snapshot");
    assert_eq!(
        l.journal()
            .unwrap()
            .iter()
            .filter(|r| r.kind == "Foreground")
            .count(),
        1
    );
    let after = l.dump_projections().unwrap();
    // Only p1's applied_seq moved.
    assert_eq!(before.len(), after.len());
    l.rebuild().unwrap();
    assert_eq!(after, l.dump_projections().unwrap());
    // A late fact still applies over the snapshot.
    assert!(
        !l.record(observed("o1", SalePhase::Seen, false))
            .unwrap()
            .changed
    );

    // Retention: 30 days on, every sale goes.
    fx.clock.advance(31 * 24 * 60 * 60 * 1000);
    assert_eq!(l.retain().unwrap(), 3);
    assert!(l.rows().unwrap().is_empty());
}

#[test]
fn the_shift_lists_live_sales_always_and_ended_ones_in_its_window() {
    let fx = Fixture::new();
    let mut l = fx.open(Process::App);
    let at = |id: &str, when: &str, tender| {
        let Fact::PosPaymentSeen(mut p) = payment(id, "front", tender, 1) else {
            unreachable!()
        };
        p.created_at = when.parse().unwrap();
        Fact::PosPaymentSeen(p)
    };
    // Yesterday's live sale and paid sale; today's paid cash sale.
    for fact in [
        at("old_live", "2026-11-08T17:00:00Z", Tender::Monero),
        created("old_live", "o1"),
        at("old_paid", "2026-11-08T17:00:00Z", Tender::Monero),
        created("old_paid", "o2"),
        observed("o2", SalePhase::Paid, true),
        at("today", "2026-11-09T17:00:00Z", Tender::Cash),
        created("today", "o3"),
        observed("o3", SalePhase::Paid, true),
    ] {
        l.record(fact).unwrap();
    }
    let shift = l.shift().unwrap();
    let keys: Vec<_> = shift.sales.iter().map(|v| v.sale.text()).collect();
    assert_eq!(keys, ["sq:today", "sq:old_live"]);
    assert_eq!(shift.paid_count, 1);
    assert_eq!(shift.paid_total.get("USD"), Some(&2450));
    assert_eq!(shift.cash_in_monero_count, 1);

    // Started after today's sale: it leaves the window, the live one stays.
    l.record(Fact::ShiftMarked {
        mark: till_ledger::ShiftMark::Started,
        at: UnixMillis(NOW - 1000),
    })
    .unwrap();
    let shift = l.shift().unwrap();
    assert_eq!(shift.sales.len(), 1);
    assert_eq!(shift.paid_count, 0);
}

/// Two connections writing the same sales at once, on two threads: every
/// write lands, nothing is journalled twice, and both see the same rows.
#[test]
fn two_connections_write_at_once() {
    let fx = Fixture::new();
    let facts = pool();
    let mut handles = Vec::new();
    for (n, process) in [Process::App, Process::Extension].into_iter().enumerate() {
        let mut ledger = fx.open(process);
        let mut facts = facts.clone();
        if n == 1 {
            facts.reverse();
        }
        handles.push(std::thread::spawn(move || {
            for fact in facts {
                ledger.record(fact).unwrap();
            }
            ledger
        }));
    }
    let ledgers: Vec<Ledger> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let mut solo = fx.open_as("solo.db", Process::App);
    for fact in pool() {
        solo.record(fact).unwrap();
    }
    assert_eq!(shared_of(&ledgers[0]), shared_of(&solo));
    assert_eq!(shared_of(&ledgers[1]), shared_of(&solo));
    let new_sales: Vec<_> = ["p1", "p2"]
        .iter()
        .filter_map(|id| ledgers[0].alert(&sale(id), AlertKind::NewSale).unwrap())
        .collect();
    assert!(new_sales.len() <= 2);
}
