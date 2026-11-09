//! Amounts in the device's locale. Which sales' changes call for a
//! notification is the ledger's now (docs/ledger.md §6); what the
//! notifications say is till-screens' `notification`.

use std::sync::Arc;

use monokulo_client::MoneyFormat;
use monokulo_client::amounts::{self, MoneyLocale, Symbols};

use crate::Platform;

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
