//! Which sales' changes call for a notification (Android: `TillService`'s
//! `PhaseChanges`), and amounts in the device's locale. What the
//! notifications say is till-screens' `notification`.

use std::collections::HashMap;
use std::sync::Arc;

use monokulo_client::amounts::{self, MoneyLocale, Symbols};
use monokulo_client::{MoneyFormat, SalePhase};
use till_sales::TillSale;

use crate::Platform;
use crate::alerted::Alerted;

/// Amounts in the device's locale (decision 9): its pattern and symbols
/// from the platform, which symbol to use from `amounts::written`
/// (decision 99).
pub(crate) struct Money(pub Arc<dyn Platform>);

impl MoneyLocale for Money {
    fn pattern(&self, minor: i64, currency: &str) -> String {
        self.0.format_money(minor, currency.to_owned())
    }

    fn symbols(&self, currency: &str) -> Symbols {
        let s = self.0.currency_symbols(currency.to_owned());
        Symbols {
            local: s.local,
            international: s.international,
            home: s.home,
        }
    }
}

impl MoneyFormat for Money {
    fn format(&self, minor: i64, currency: &str) -> String {
        amounts::written(minor, currency, self)
    }
}

/// What changed about a sale since the last look.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    /// New, its code up.
    New,
    Paid,
    Unpaid,
    /// Gone from the shift.
    Gone,
}

/// The sales' phases at the last look (Android: `PhaseChanges`).
#[derive(Default)]
pub(crate) struct Phases {
    seen: Option<HashMap<String, SalePhase>>,
}

impl Phases {
    /// What changed in `sales`. The first look only learns them: a sale
    /// already there when the till started was told of then.
    pub fn next(&mut self, sales: &[TillSale]) -> Vec<(String, Change)> {
        let now: HashMap<String, SalePhase> = sales
            .iter()
            .map(|sale| (sale.order_id.clone(), sale.phase))
            .collect();
        let Some(before) = self.seen.replace(now.clone()) else {
            return Vec::new();
        };
        let mut changes = Vec::new();
        for sale in sales {
            let was = before.get(&sale.order_id);
            if was == Some(&sale.phase) {
                continue;
            }
            match sale.phase {
                SalePhase::Paid => changes.push((sale.order_id.clone(), Change::Paid)),
                SalePhase::Expired | SalePhase::Cancelled | SalePhase::DoubleSpend => {
                    if was.is_some() {
                        changes.push((sale.order_id.clone(), Change::Unpaid));
                    }
                }
                SalePhase::Waiting if was.is_none() => {
                    changes.push((sale.order_id.clone(), Change::New));
                }
                _ => {}
            }
        }
        for order_id in before.keys() {
            if !now.contains_key(order_id) {
                changes.push((order_id.clone(), Change::Gone));
            }
        }
        changes
    }
}

/// Which sales are due their new-sale alert (core.md decision 123): a
/// live sale that isn't another counter's, which no one has alerted here
/// yet. Unlike [`Phases`], it doesn't ask whether the sale is new to the
/// till: a sale that arrived quiet (another till announced it first) is
/// due once this till's own point of sale finds it on this counter's
/// device, and a sale already alerted (by iOS's extension, say, or before
/// a restart) never is again.
#[derive(Default)]
pub(crate) struct NewSales {
    learned: bool,
}

impl NewSales {
    /// The sales in `sales` (newest first) claimed for their alert now,
    /// newest first, `quiet` saying which are another counter's. The first
    /// look only learns: the sales already there when the till started are
    /// claimed without an alert, as they were told of then.
    pub fn next(
        &mut self,
        sales: &[TillSale],
        quiet: impl Fn(&TillSale) -> bool,
        alerted: &Alerted,
    ) -> Vec<String> {
        let due: Vec<String> = sales
            .iter()
            .filter(|sale| sale.live() && !quiet(sale))
            .map(|sale| sale.order_id.clone())
            .collect();
        let claimed = alerted.claim(&due);
        if !self.learned {
            self.learned = true;
            return Vec::new();
        }
        claimed
    }
}

#[cfg(test)]
mod tests {
    use till_sales::shift_json;

    use super::*;

    fn sale(order_id: &str, phase: &str) -> TillSale {
        let json = format!(
            r#"[{{"orderId":"{order_id}","amount":"24.50","currency":"USD","code":{{"uri":"monero:8C","hasReceipt":false}},"phase":"{phase}","expiresAt":0,"xmr":"0.1","address":"8C"}}]"#
        );
        shift_json::decode(&json).unwrap().remove(0)
    }

    #[test]
    fn changes_are_told_once_and_the_first_look_only_learns() {
        let mut phases = Phases::default();
        assert!(phases.next(&[sale("old", "Waiting")]).is_empty());
        assert_eq!(
            phases.next(&[sale("new", "Waiting"), sale("old", "Waiting")]),
            [("new".to_owned(), Change::New)]
        );
        assert_eq!(
            phases.next(&[sale("new", "Paid"), sale("old", "Expired")]),
            [
                ("new".to_owned(), Change::Paid),
                ("old".to_owned(), Change::Unpaid)
            ]
        );
        assert!(
            phases
                .next(&[sale("new", "Paid"), sale("old", "Expired")])
                .is_empty()
        );
        assert_eq!(
            phases.next(&[sale("new", "Paid")]),
            [("old".to_owned(), Change::Gone)]
        );
    }

    /// This counter's sale as the two paths bring it: another till's
    /// announcement (no `found`, so quiet while this till has a device), and
    /// this till's own point of sale, which names this counter's device.
    fn announced() -> TillSale {
        TillSale {
            device_name: Some("Bar iPad".into()),
            ..sale("ord_1", "Waiting")
        }
    }

    fn found_here(phase: &str) -> TillSale {
        let found = r#"{"id":"pay_1","amountMinor":2450,"currency":"USD","createdAt":"2026-10-22T16:07:00Z","deviceId":"dev_front"}"#;
        TillSale {
            found: Some(serde_json::from_str(found).unwrap()),
            ..sale("ord_1", phase)
        }
    }

    fn front_counter() -> till_screens::counters::Counters {
        till_screens::counters::Counters {
            device: Some(till_screens::counters::CounterDevice {
                id: "dev_front".into(),
                name: Some("Front iPad".into()),
            }),
            every_device: false,
            show_others: true,
            names: Default::default(),
        }
    }

    fn alerted() -> Alerted {
        let platform = std::sync::Arc::new(crate::testing::FakePlatform::default());
        let dir = std::env::temp_dir().to_string_lossy().into_owned();
        Alerted::new(crate::store::Store::new(platform), &dir)
    }

    /// The alerts each look of `looks` claims, after a first look at no
    /// sales.
    fn alerts(looks: &[Vec<TillSale>], record: &Alerted) -> Vec<Vec<String>> {
        let counters = front_counter();
        let mut new_sales = NewSales::default();
        assert!(
            new_sales
                .next(&[], |s| counters.quiet(s), record)
                .is_empty()
        );
        looks
            .iter()
            .map(|sales| new_sales.next(sales, |s| counters.quiet(s), record))
            .collect()
    }

    #[test]
    fn this_counters_sale_alerts_once_when_another_till_announced_it_first() {
        // Monokulo's event first, quiet; then this till's own point of
        // sale finds it on this counter's device; then the event stream
        // moves it on.
        let looks = [
            vec![announced()],
            vec![found_here("Waiting")],
            vec![found_here("Seen")],
        ];
        assert_eq!(
            alerts(&looks, &alerted()),
            [vec![], vec!["ord_1".to_owned()], vec![]]
        );
    }

    #[test]
    fn this_counters_sale_alerts_once_when_this_till_found_it_first() {
        // This till's point of sale first; then Monokulo's event for the
        // same order, which keeps what this till found.
        let looks = [
            vec![found_here("Waiting")],
            vec![found_here("Waiting")],
            vec![found_here("Seen")],
        ];
        assert_eq!(
            alerts(&looks, &alerted()),
            [vec!["ord_1".to_owned()], vec![], vec![]]
        );
    }

    #[test]
    fn a_sale_alerted_elsewhere_or_before_a_restart_never_alerts_again() {
        let record = alerted();
        assert_eq!(
            alerts(&[vec![found_here("Waiting")]], &record),
            [vec!["ord_1".to_owned()]]
        );
        // A new engine (a restart, or iOS's extension) over the same record,
        // seeing the sale arrive after its first look.
        assert_eq!(
            alerts(&[vec![found_here("Waiting")]], &record),
            [Vec::<String>::new()]
        );
    }

    #[test]
    fn the_first_look_learns_but_leaves_a_quiet_sale_due() {
        let counters = front_counter();
        let record = alerted();
        let mut new_sales = NewSales::default();
        let other = TillSale {
            order_id: "ord_2".into(),
            ..announced()
        };
        let first = [other.clone(), found_here("Waiting")];
        assert!(
            new_sales
                .next(&first, |s| counters.quiet(s), &record)
                .is_empty()
        );
        assert!(
            new_sales
                .next(&first, |s| counters.quiet(s), &record)
                .is_empty()
        );
        // The quiet one, found here after all, is still due.
        let found = TillSale {
            order_id: "ord_2".into(),
            ..found_here("Waiting")
        };
        assert_eq!(
            new_sales.next(
                &[found, found_here("Waiting")],
                |s| counters.quiet(s),
                &record
            ),
            ["ord_2".to_owned()]
        );
    }
}
