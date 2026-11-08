//! Which sales' changes call for a notification (Android: `TillService`'s
//! `PhaseChanges`), and amounts in the device's locale. What the
//! notifications say is till-screens' `notification`.

use std::collections::HashMap;
use std::sync::Arc;

use monokulo_client::{MoneyFormat, SalePhase};
use till_sales::TillSale;

use crate::Platform;

/// Amounts written by the platform, in the device's locale (decision 9).
pub(crate) struct Money(pub Arc<dyn Platform>);

impl MoneyFormat for Money {
    fn format(&self, minor: i64, currency: &str) -> String {
        self.0.format_money(minor, currency.to_owned())
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
}
