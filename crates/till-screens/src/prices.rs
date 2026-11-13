//! Where a till on the shop's own wallet gets the price of XMR
//! (docs/view-key-mode.md §8): setup's Prices step and Settings' price
//! source, copying Fob's Price from (its Android `PriceFromScreen`, the
//! words of `fob-words/src/prices.rs`). Only the providers that price the
//! currency are offered (fob-price's `providers_for`), each asked for the
//! price as the page opens (fob-price's `check`): what it says XMR is
//! worth, or why it gave none. One that gave none can't be picked.

use crate::view::Press;

/// One provider's line, as fob-price's check left it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderCheck {
    /// "Kraken".
    pub name: String,
    /// What it says XMR is worth: "A$845.10".
    pub rate: Option<String>,
    /// Why it gave no price, in Fob's words: "Not answering".
    pub failure: Option<String>,
}

/// What a price source is drawn from: setup's step and Settings' row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PricesInput {
    /// "AUD".
    pub currency: String,
    /// "Australian dollars", or the code for a currency Fob has no name for.
    pub plural: String,
    /// The provider prices come from now.
    pub provider: String,
    /// The providers that price the currency, in fob-price's order.
    pub providers: Vec<String>,
    /// Each provider's answer; `None` while they're being asked.
    pub checks: Option<Vec<ProviderCheck>>,
    /// Fob shared its price source, and it's still the one chosen.
    pub from_fob: bool,
    pub busy: bool,
    pub problem: Option<String>,
}

/// A provider to pick: its name, what it says XMR is worth (or why it
/// gave nothing, `failed`), and whether it's the one prices come from.
/// Picking one sends `Action::ChoosePriceProvider` with its name.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ProviderRow {
    pub name: String,
    /// "A$845.10", or "Not answering"; `None` while it's asked.
    pub value: Option<String>,
    /// It gave no price: `value` is why, in the warning colour, and it
    /// can't be picked.
    pub failed: bool,
    pub selected: bool,
    pub enabled: bool,
    /// What a screen reader says: "Kraken, A$845.10".
    pub label: String,
}

/// The Prices step (setup, wallet modes with a fiat currency).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PricesStep {
    /// Fob's intro, the currency in bold between `intro_before` and
    /// `intro_after`: "Prices in **Australian dollars**, checked as this
    /// page opens. Tap a provider to pick it."
    pub intro_before: String,
    pub intro_bold: String,
    pub intro_after: String,
    /// "Price from".
    pub section: String,
    /// "Asking each provider…", while they're asked.
    pub asking: Option<String>,
    pub providers: Vec<ProviderRow>,
    /// What a screen reader hears once the answers are in: "Kraken
    /// A$845.10, CoinGecko not answering".
    pub summary: String,
    /// Fob shared its price source: "Kraken is Fob's price source. Pick
    /// another if you like."
    pub from_fob: Option<String>,
    /// What's asked, and how fresh a price must be.
    pub footnote: String,
    /// "Continue".
    pub next: Press,
    pub problem: Option<String>,
}

/// What a request for a price gives away, and how fresh a code's price is.
pub const PRIVACY: &str = "Till asks for the price of XMR, never an amount or a payment. It asks again every 5 minutes while it's open, and never makes a code from a price more than 10 minutes old.";

/// The providers as rows to pick.
pub fn provider_rows(p: &PricesInput) -> Vec<ProviderRow> {
    p.providers
        .iter()
        .map(|name| {
            let check = p
                .checks
                .as_ref()
                .and_then(|checks| checks.iter().find(|c| c.name == *name));
            let failure = check.and_then(|c| c.failure.clone());
            let value = check.and_then(|c| c.rate.clone()).or(failure.clone());
            ProviderRow {
                name: name.clone(),
                label: match &value {
                    Some(value) => format!("{name}, {value}"),
                    None => name.clone(),
                },
                failed: failure.is_some(),
                selected: *name == p.provider,
                // The one in use stays picked even when it didn't answer
                // just now: a code asks it again first.
                enabled: !p.busy && (failure.is_none() || *name == p.provider),
                value,
            }
        })
        .collect()
}

/// "Kraken A$845.10, CoinGecko not answering", as fob-words' summary.
pub fn summary(p: &PricesInput) -> String {
    let Some(checks) = &p.checks else {
        return String::new();
    };
    checks
        .iter()
        .map(|c| match (&c.rate, &c.failure) {
            (Some(rate), _) => format!("{} {rate}", c.name),
            (None, Some(why)) => format!("{} {}", c.name, why.to_lowercase()),
            (None, None) => format!("{} not asked", c.name),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The Prices step.
pub fn prices_step(p: &PricesInput) -> PricesStep {
    PricesStep {
        intro_before: "Prices in ".into(),
        intro_bold: p.plural.clone(),
        intro_after: ", checked as this page opens. Tap a provider to pick it.".into(),
        section: "Price from".into(),
        asking: p
            .checks
            .is_none()
            .then(|| "Asking each provider…".into()),
        providers: provider_rows(p),
        summary: summary(p),
        from_fob: p
            .from_fob
            .then(|| format!("{} is Fob's price source. Pick another if you like.", p.provider)),
        footnote: PRIVACY.into(),
        next: Press::new("Continue").enabled(!p.busy).busy(p.busy),
        problem: p.problem.clone(),
    }
}

/// The plural Fob names `code` by ("Australian dollars"), or the code.
pub fn plural_or_code(plural: &str, code: &str) -> String {
    if plural.is_empty() {
        code.to_owned()
    } else {
        plural.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> PricesInput {
        PricesInput {
            currency: "AUD".into(),
            plural: "Australian dollars".into(),
            provider: "Kraken".into(),
            providers: vec!["Kraken".into(), "CoinGecko".into()],
            checks: None,
            from_fob: false,
            busy: false,
            problem: None,
        }
    }

    #[test]
    fn while_asking_each_provider_is_offered_with_no_price_yet() {
        let step = prices_step(&input());
        assert_eq!(
            format!("{}{}{}", step.intro_before, step.intro_bold, step.intro_after),
            "Prices in Australian dollars, checked as this page opens. Tap a provider to pick it."
        );
        assert_eq!(step.asking.as_deref(), Some("Asking each provider…"));
        assert_eq!(step.providers.len(), 2);
        assert!(step.providers[0].selected && step.providers[0].enabled);
        assert!(step.providers.iter().all(|p| p.value.is_none()));
        assert!(step.from_fob.is_none());
        assert!(step.next.enabled);
    }

    #[test]
    fn a_provider_that_gave_no_price_says_why_and_cant_be_picked() {
        let step = prices_step(&PricesInput {
            checks: Some(vec![
                ProviderCheck {
                    name: "Kraken".into(),
                    rate: Some("A$845.10".into()),
                    failure: None,
                },
                ProviderCheck {
                    name: "CoinGecko".into(),
                    rate: None,
                    failure: Some("Not answering".into()),
                },
            ]),
            ..input()
        });
        assert!(step.asking.is_none());
        let (kraken, gecko) = (&step.providers[0], &step.providers[1]);
        assert_eq!(kraken.value.as_deref(), Some("A$845.10"));
        assert_eq!(kraken.label, "Kraken, A$845.10");
        assert!(!kraken.failed && kraken.enabled);
        assert_eq!(gecko.value.as_deref(), Some("Not answering"));
        assert!(gecko.failed && !gecko.enabled && !gecko.selected);
        assert_eq!(step.summary, "Kraken A$845.10, CoinGecko not answering");
    }

    #[test]
    fn fobs_choice_is_picked_and_said_so() {
        let step = prices_step(&PricesInput {
            provider: "CoinGecko".into(),
            from_fob: true,
            ..input()
        });
        assert!(step.providers[1].selected);
        assert_eq!(
            step.from_fob.as_deref(),
            Some("CoinGecko is Fob's price source. Pick another if you like.")
        );
    }
}
