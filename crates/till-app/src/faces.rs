//! What a notification says about a sale, as plain data, and which sales'
//! changes call for one (Android: `TillService`'s `PhaseChanges` and the
//! `Face`s of `notify/SaleNotifications.kt`).
//!
//! The words are a stand-in until till-screens' `notification` module
//! lands: the alert is docs/ios-plan.md's "Monero sale · $24.50 · Tap to
//! show code", and a paid or unpaid sale says so with its amount.

use std::collections::HashMap;
use std::sync::Arc;

use monokulo_client::{MoneyFormat, SalePhase, amounts};
use till_sales::TillSale;

use crate::Platform;

/// Amounts written by the platform, in the device's locale (decision 9).
pub(crate) struct Money(pub Arc<dyn Platform>);

impl MoneyFormat for Money {
    fn format(&self, minor: i64, currency: &str) -> String {
        self.0.format_money(minor, currency.to_owned())
    }
}

/// A notification about one sale, for the platform to post.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NotificationFace {
    /// The sale's order id: a later face for the same sale replaces it.
    pub id: String,
    pub title: String,
    pub text: String,
    /// The lock screen's text: the amount and the phase, never an item.
    pub public_text: String,
    /// Heads-up and time-sensitive: a new sale to show the customer.
    pub alert: bool,
    /// Without sound or buzz: the app is on screen.
    pub quiet: bool,
}

fn amount(sale: &TillSale, money: &dyn MoneyFormat) -> String {
    amounts::fiat(&sale.amount, &sale.currency, money)
}

/// A new sale whose code is up.
pub(crate) fn new_sale(sale: &TillSale, money: &dyn MoneyFormat, quiet: bool) -> NotificationFace {
    let text = format!("{} · Tap to show code", amount(sale, money));
    NotificationFace {
        id: sale.order_id.clone(),
        title: "Monero sale".into(),
        public_text: text.clone(),
        text,
        alert: true,
        quiet,
    }
}

/// A payment landed.
pub(crate) fn paid(sale: &TillSale, money: &dyn MoneyFormat, quiet: bool) -> NotificationFace {
    let text = format!("{} · {} XMR received", amount(sale, money), sale.xmr);
    NotificationFace {
        id: sale.order_id.clone(),
        title: "Paid · Monero sale".into(),
        public_text: text.clone(),
        text,
        alert: false,
        quiet,
    }
}

/// The code ran out, or the payment was cancelled or double-spent.
pub(crate) fn unpaid(sale: &TillSale, money: &dyn MoneyFormat, quiet: bool) -> NotificationFace {
    let why = match sale.phase {
        SalePhase::DoubleSpend => "Double spend",
        SalePhase::Cancelled => "Cancelled",
        _ => "Not paid",
    };
    let text = format!("{} · {why}", amount(sale, money));
    NotificationFace {
        id: sale.order_id.clone(),
        title: format!("{why} · Monero sale"),
        public_text: text.clone(),
        text,
        alert: false,
        quiet,
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

    fn money() -> impl MoneyFormat {
        |minor: i64, currency: &str| format!("{currency} {minor}")
    }

    #[test]
    fn a_new_sale_says_what_the_plan_says() {
        let face = new_sale(&sale("ord_1", "Waiting"), &money(), false);
        assert_eq!(face.title, "Monero sale");
        assert_eq!(face.text, "USD 2450 · Tap to show code");
        assert!(face.alert);
        assert_eq!(
            paid(&sale("ord_1", "Paid"), &money(), true).title,
            "Paid · Monero sale"
        );
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
