//! Which till screen the sales lead to as they change (decisions 70 and
//! 266 of android.md), and which sales changed phase since the last look
//! (Android: `SaleText.kt`'s `TillNav` and `PhaseChanges`). The engine
//! holds what the last look saw and asks these after every change.

use std::collections::{BTreeSet, HashMap};

use monokulo_client::SalePhase;
use till_sales::{Standing, TillSale};

/// Where the till is, as far as the sales move it: the rest of the app's
/// places (Settings, onboarding) are `Other`, which the sales never leave.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum TillPlace {
    /// The shift, the till's home.
    Shift,
    /// One sale: its code while it's live, else its page.
    Sale {
        order_id: String,
    },
    /// A live sale's code enlarged.
    Code {
        order_id: String,
    },
    /// What a live sale's customer bought, every line.
    Order {
        order_id: String,
    },
    Other,
}

/// The place after the sales changed on `place`: the shift, while it rests
/// on screen (`resting_on_shift`), brings up a live sale that is new since
/// the last look (`live_before`) and not `left`; a sale that came while the
/// cashier was elsewhere waits in the shift's tray and never comes up
/// later, and a closed or ended one never comes up on its own (decision
/// 266). A sale on screen stays whatever it becomes; the enlarged code and
/// the order page are for a live sale only, so they give way to the sale's
/// page once it's paid or ends.
pub fn follow(
    place: &TillPlace,
    sales: &[TillSale],
    left: &BTreeSet<String>,
    live_before: &BTreeSet<String>,
    resting_on_shift: bool,
) -> TillPlace {
    match place {
        TillPlace::Shift if resting_on_shift => sales
            .iter()
            .find(|sale| {
                sale.live()
                    && !left.contains(&sale.order_id)
                    && !live_before.contains(&sale.order_id)
            })
            .map_or(TillPlace::Shift, |sale| TillPlace::Sale {
                order_id: sale.order_id.clone(),
            }),
        TillPlace::Code { order_id } | TillPlace::Order { order_id }
            if !live_or_unknown(sales, order_id) =>
        {
            TillPlace::Sale {
                order_id: order_id.clone(),
            }
        }
        _ => place.clone(),
    }
}

/// The sale on `place` if it was live at the last look and is paid now: it
/// was paid while the customer watched, so it steps back to the shift after
/// a few seconds. Opened from the shift once paid, it stays.
pub fn paid_on_screen(
    place: &TillPlace,
    sales: &[TillSale],
    live_before: &BTreeSet<String>,
) -> Option<String> {
    let TillPlace::Sale { order_id } = place else {
        return None;
    };
    let sale = sales.iter().find(|sale| &sale.order_id == order_id)?;
    (live_before.contains(order_id) && matches!(sale.standing(), Standing::Paid { .. }))
        .then(|| order_id.clone())
}

/// Whether a payment landed since the last look: a sale that was live then
/// is paid now.
pub fn payment_landed(sales: &[TillSale], live_before: &BTreeSet<String>) -> bool {
    sales.iter().any(|sale| {
        live_before.contains(&sale.order_id) && matches!(sale.standing(), Standing::Paid { .. })
    })
}

/// The live sales' ids: what the next look's `live_before` is.
pub fn live_ids(sales: &[TillSale]) -> BTreeSet<String> {
    sales
        .iter()
        .filter(|sale| sale.live())
        .map(|sale| sale.order_id.clone())
        .collect()
}

fn live_or_unknown(sales: &[TillSale], order_id: &str) -> bool {
    sales
        .iter()
        .find(|sale| sale.order_id == order_id)
        .is_none_or(TillSale::live)
}

/// Which sales changed phase since the last look: what the bubble and the
/// notifications react to ("Paid" for a few seconds, an alert on a new
/// sale).
#[derive(Debug, Default)]
pub struct PhaseChanges {
    last: HashMap<String, SalePhase>,
    started: bool,
}

impl PhaseChanges {
    /// The sales in `sales` that are new or in a new phase. The first look
    /// reports nothing: those sales aren't news.
    pub fn next(&mut self, sales: &[TillSale]) -> Vec<TillSale> {
        let changed = if self.started {
            sales
                .iter()
                .filter(|sale| self.last.get(&sale.order_id) != Some(&sale.phase))
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        self.last = sales
            .iter()
            .map(|sale| (sale.order_id.clone(), sale.phase))
            .collect();
        self.started = true;
        changed
    }
}

/// Ported from Android's `SaleTextTest` (phase changes) and `ShiftListTest`
/// (the five on what the till brings up), test for test.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::shift::tests::{fixture_shift, sale_of as sale};

    fn ids(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    fn at_sale(id: &str) -> TillPlace {
        TillPlace::Sale {
            order_id: id.into(),
        }
    }

    fn at_code(id: &str) -> TillPlace {
        TillPlace::Code {
            order_id: id.into(),
        }
    }

    fn at_order(id: &str) -> TillPlace {
        TillPlace::Order {
            order_id: id.into(),
        }
    }

    #[test]
    fn phase_changes_report_new_sales_and_new_phases_but_not_what_was_there_at_the_start() {
        let mut changes = PhaseChanges::default();
        let old = sale("old").build();
        assert!(changes.next(std::slice::from_ref(&old)).is_empty());
        assert!(changes.next(std::slice::from_ref(&old)).is_empty());
        let fresh = sale("new").build();
        assert_eq!(
            changes.next(&[fresh.clone(), old.clone()]),
            std::slice::from_ref(&fresh)
        );
        let paid = TillSale {
            phase: SalePhase::Paid,
            ..fresh
        };
        assert_eq!(
            changes.next(&[paid.clone(), old.clone()]),
            std::slice::from_ref(&paid)
        );
        assert!(changes.next(&[paid, old]).is_empty());
    }

    #[test]
    fn the_till_brings_up_a_new_live_sale_keeps_a_sale_on_screen_and_steps_back_from_one_paid_on_it()
     {
        let waiting = sale("w").build();
        let none = BTreeSet::new();
        let on_shift = |sales: &[TillSale], left: &BTreeSet<String>| {
            follow(&TillPlace::Shift, sales, left, &none, true)
        };
        assert_eq!(
            on_shift(std::slice::from_ref(&waiting), &none),
            at_sale("w")
        );
        assert_eq!(
            on_shift(std::slice::from_ref(&waiting), &ids(&["w"])),
            TillPlace::Shift,
            "the cashier left it"
        );
        assert_eq!(
            on_shift(&[sale("w2").build(), waiting.clone()], &ids(&["w"])),
            at_sale("w2"),
            "the newest they haven't left"
        );
        let shift = fixture_shift();
        let reversed: Vec<_> = shift.iter().rev().cloned().collect();
        assert_eq!(
            on_shift(&reversed, &none),
            at_sale("waiting"),
            "past the paid and ended ones"
        );
        assert_eq!(
            on_shift(&shift[1..], &none),
            TillPlace::Shift,
            "closed and ended sales never come up on their own"
        );
        let mut with_waiting = vec![waiting.clone()];
        with_waiting.extend(shift.iter().cloned());
        assert_eq!(
            follow(&at_sale("paid"), &with_waiting, &none, &none, true),
            at_sale("paid"),
            "a sale on screen stays"
        );
        assert_eq!(
            follow(
                &at_code("w"),
                std::slice::from_ref(&waiting),
                &none,
                &none,
                true
            ),
            at_code("w")
        );
        assert_eq!(
            follow(&at_code("paid"), &shift, &none, &none, false),
            at_sale("paid"),
            "the enlarged code gives way to the page"
        );
        assert_eq!(
            follow(&at_order("expired"), &shift, &none, &none, false),
            at_sale("expired")
        );
        assert_eq!(
            follow(
                &TillPlace::Other,
                std::slice::from_ref(&waiting),
                &none,
                &none,
                true
            ),
            TillPlace::Other
        );
        let paid = TillSale {
            phase: SalePhase::Paid,
            ..waiting
        };
        let paid = std::slice::from_ref(&paid);
        assert_eq!(
            paid_on_screen(&at_sale("w"), paid, &ids(&["w"])).as_deref(),
            Some("w")
        );
        assert_eq!(
            paid_on_screen(&at_sale("w"), paid, &none),
            None,
            "opened from the shift once paid: it stays"
        );
        assert_eq!(paid_on_screen(&TillPlace::Shift, paid, &ids(&["w"])), None);
    }

    #[test]
    fn a_payment_lands_when_a_sale_that_was_live_is_paid() {
        let paid = sale("w").phase(SalePhase::Paid).paid_at(1).build();
        assert!(payment_landed(std::slice::from_ref(&paid), &ids(&["w"])));
        assert!(
            !payment_landed(std::slice::from_ref(&paid), &BTreeSet::new()),
            "a sale already paid at the last look: nothing new"
        );
        assert!(
            !payment_landed(&[sale("w").build()], &ids(&["w"])),
            "still waiting"
        );
        assert!(
            !payment_landed(
                &[sale("w").phase(SalePhase::Expired).ended_at(1).build()],
                &ids(&["w"])
            ),
            "ended unpaid is no payment"
        );
    }

    /// Decision 266: Back always lands on the shift; only a sale that
    /// arrives while the shift rests on screen rises.
    #[test]
    fn a_sale_rises_only_if_it_arrives_while_the_shift_rests() {
        let sales = [sale("new").build(), sale("old").build()];
        let none = BTreeSet::new();
        assert_eq!(
            follow(&TillPlace::Shift, &sales, &none, &ids(&["old"]), true),
            at_sale("new"),
            "new since the last look, on the resting shift"
        );
        assert_eq!(
            follow(
                &TillPlace::Shift,
                &sales,
                &none,
                &ids(&["new", "old"]),
                true
            ),
            TillPlace::Shift,
            "seen at the last look (it came while the cashier was elsewhere): it waits in the tray"
        );
        assert_eq!(
            follow(&TillPlace::Shift, &sales, &none, &ids(&["old"]), false),
            TillPlace::Shift,
            "arriving under a Back still landing: into the tray, not over the move"
        );
        assert_eq!(
            follow(
                &TillPlace::Shift,
                &sales,
                &ids(&["new"]),
                &ids(&["old"]),
                true
            ),
            TillPlace::Shift,
            "a sale the cashier left never rises"
        );
    }

    /// What the model does with a sale that arrives on another screen: at
    /// the next look it is known, so it never rises later.
    #[test]
    fn a_sale_that_arrived_elsewhere_never_rises_later() {
        let sales = [sale("b").build(), sale("a").build()];
        let left = ids(&["a"]);
        let mut live_before: BTreeSet<String>;
        let look = |place: TillPlace, resting: bool, live_before: &mut BTreeSet<String>| {
            let next = follow(&place, &sales, &left, live_before, resting);
            *live_before = live_ids(&sales);
            next
        };
        for elsewhere in [at_sale("a"), TillPlace::Other, at_code("a"), at_order("a")] {
            live_before = ids(&["a"]);
            assert_eq!(
                look(elsewhere.clone(), true, &mut live_before),
                elsewhere,
                "on {elsewhere:?} b doesn't come up"
            );
            // Back on the shift, at rest, with another update: b was seen,
            // so it stays in the tray.
            assert_eq!(
                look(TillPlace::Shift, true, &mut live_before),
                TillPlace::Shift,
                "after {elsewhere:?}"
            );
        }
        live_before = ids(&["a"]);
        assert_eq!(
            look(TillPlace::Shift, false, &mut live_before),
            TillPlace::Shift,
            "mid-Back"
        );
        assert_eq!(
            look(TillPlace::Shift, true, &mut live_before),
            TillPlace::Shift,
            "and not on landing either"
        );
    }
}
