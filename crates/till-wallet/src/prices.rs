//! What XMR is worth in the sale's currency, for making a code
//! (docs/view-key-mode.md §8): fob-price's providers, held to a stricter
//! freshness than Fob's own. A code is never made from a price older than
//! [`AGED`] (10 minutes): the price is asked for first, and if that fails no
//! code is made.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fob_price::{Provider, Source, Transport};
use monokulo_client::Clock;
use till_types::{Money, Price, PriceUsed, UnixSeconds};

/// The oldest price a code is made from, in seconds: fob-price's `AGED`.
pub const AGED: u64 = fob_price::AGED;

/// How often the price is asked for again while Till is in the foreground,
/// in seconds: fob-price's `REFRESH`, 5 minutes.
pub const REFRESH: u64 = fob_price::REFRESH;

/// The soonest pull to refresh asks again, in seconds: fob-price's
/// `PULL_REFRESH`.
pub const PULL_REFRESH: u64 = fob_price::PULL_REFRESH;

/// Piconero in one XMR.
const ATOMIC_PER_XMR: u128 = 1_000_000_000_000;
/// Millionths: how a provider's price is counted.
const MICRO: u128 = 1_000_000;

/// Where a price comes from: a fob-price provider, or a stand-in in tests.
#[async_trait]
pub trait PriceSource: Send + Sync {
    /// "Kraken".
    fn name(&self) -> String;
    /// Whether it prices XMR in `currency` at all.
    fn prices(&self, currency: &str) -> bool;
    /// One XMR now, in millionths of `currency`.
    async fn current(&self, currency: &str) -> Result<u64, String>;
}

/// A provider of fob-price's, over the internet.
pub struct ProviderSource(pub Provider);

#[async_trait]
impl PriceSource for ProviderSource {
    fn name(&self) -> String {
        self.0.name().to_owned()
    }

    fn prices(&self, currency: &str) -> bool {
        self.0.prices(currency)
    }

    async fn current(&self, currency: &str) -> Result<u64, String> {
        self.0
            .current(currency, Transport::Web)
            .await
            .map_err(|e| fob_wallet::FobError::Price(e).to_string())
    }
}

/// The provider named `name`, else the first that prices `currency`
/// (fob-price's `providers_for`), else Kraken.
pub fn provider_for(name: Option<&str>, currency: Option<&str>) -> Provider {
    name.and_then(Provider::named)
        .filter(|p| currency.is_none_or(|c| p.prices(c)))
        .or_else(|| currency.and_then(|c| fob_price::providers_for(c).first().copied()))
        .unwrap_or_default()
}

/// The providers that price XMR in `currency` (fob-price's
/// `providers_for`), or every provider without a currency, by name: what
/// Settings offers.
pub fn provider_names(currency: Option<&str>) -> Vec<String> {
    let providers = match currency {
        Some(code) => fob_price::providers_for(code),
        None => Provider::ALL.to_vec(),
    };
    providers.iter().map(|p| p.name().to_owned()).collect()
}

/// The name of the provider [`provider_for`] picks.
pub fn provider_name(name: Option<&str>, currency: Option<&str>) -> String {
    provider_for(name, currency).name().to_owned()
}

/// One provider's answer as the Prices page opens (fob-price's `check`):
/// what it says XMR is worth in the currency, written as Fob writes it
/// ("A$845.10"), or why it gave none, in Fob's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderAnswer {
    pub name: String,
    pub rate: Option<String>,
    pub failure: Option<&'static str>,
}

/// Asks each provider that prices `currency` for XMR's price now, all at
/// once, over the web, each given fob-price's `CHECK_TIMEOUT`: the lines of
/// Fob's Price from, in fob-price's order. Only the currency pair is asked
/// for.
pub async fn check_providers(currency: &str, chosen: &str) -> Vec<ProviderAnswer> {
    let providers = fob_price::providers_for(currency);
    let currency = fob_monero::fiat::currency(currency);
    fob_price::check(
        providers,
        &currency,
        Transport::Web,
        chosen,
        fob_price::CHECK_TIMEOUT,
    )
    .await
    .into_iter()
    .map(|c| ProviderAnswer {
        name: c.name.to_owned(),
        rate: c.rate,
        failure: c.failure.map(failure_words),
    })
    .collect()
}

/// Why a provider gave no price, in fob-words' words.
pub fn failure_words(failure: fob_price::Failure) -> &'static str {
    use fob_price::Failure;
    match failure {
        Failure::NotAnswering => "Not answering",
        Failure::Busy => "Busy, try again soon",
        Failure::BadAnswer => "Answered without a price",
        Failure::TorNotYet => "Tor isn't built yet",
        Failure::NotOverTor => "Doesn't answer over Tor",
    }
}

/// "Australian dollars", as Fob names `code` in "Prices in …"; empty for
/// a currency Fob doesn't know.
pub fn currency_plural(code: &str) -> String {
    fob_monero::fiat::currency(code).plural.to_owned()
}

/// One of the keypad's currencies, as fob-price lists them for Fob's
/// "Default currency": XMR first (`code` "XMR", its name "None (XMR
/// only)"), then this device's own, then all of them by name, each group's
/// first row naming it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrencyChoice {
    pub code: String,
    pub name: String,
    /// "AUD · Kraken, CoinGecko", or for XMR "No price is ever looked up".
    pub detail: String,
    pub sign: String,
    /// "This device", "Every currency", or searching "3 of 45".
    pub group: Option<String>,
}

/// What the keypad calls XMR's row, which fob-price calls "Monero (XMR)".
const NO_CURRENCY: &str = "None (XMR only)";

/// The keypad's currencies (fob-price's `currency_list`), `phone` being
/// the device's own currency, when known. A `query` keeps only those it
/// matches by name, code or symbol, whatever the case or accents, as Fob's
/// own search does ("aud", "australian", "dollar", "zloty"). Searching, the
/// device's row is left out, as it's among the rest, and XMR's row stays
/// when the query matches its name here ("none") or Fob's ("monero",
/// "xmr").
pub fn currency_choices(phone: Option<&str>, query: &str) -> Vec<CurrencyChoice> {
    use fob_monero::fiat::currencies::fold;
    use fob_price::{CurrencyHeading, CurrencyLine};
    let mut list = fob_price::currency_list(None, phone, query);
    let folded = fold(query.trim());
    let has_xmr = list
        .groups
        .iter()
        .any(|g| matches!(g.heading, CurrencyHeading::Monero));
    if !has_xmr && !folded.is_empty() && fold(NO_CURRENCY).contains(&folded) {
        // XMR's row, as a blank query lists it first.
        let mut all = fob_price::currency_list(None, None, "");
        all.groups.truncate(1);
        list.groups.splice(0..0, all.groups);
    }
    let mut rows = Vec::new();
    for group in list.groups {
        let heading = match group.heading {
            CurrencyHeading::Monero => None,
            CurrencyHeading::ThisDevice => Some("This device".to_owned()),
            CurrencyHeading::All { .. } => Some("Every currency".to_owned()),
            CurrencyHeading::Found { count, of } => Some(format!("{count} of {of}")),
        };
        for (i, row) in group.rows.into_iter().enumerate() {
            let (code, name, detail) = match (row.code, row.line) {
                (Some(code), CurrencyLine::PricedBy { providers, .. }) => {
                    let detail = format!("{code} · {}", providers.join(", "));
                    (code, row.name, detail)
                }
                (code, _) => (
                    code.unwrap_or_else(|| "XMR".into()),
                    NO_CURRENCY.into(),
                    "No price is ever looked up".into(),
                ),
            };
            rows.push(CurrencyChoice {
                code,
                name,
                detail,
                sign: row.sign,
                group: if i == 0 { heading.clone() } else { None },
            });
        }
    }
    rows
}

/// "Australian dollar", as Fob names `code`; empty for one it doesn't
/// know.
pub fn currency_name(code: &str) -> String {
    fob_monero::fiat::currency(code).name.to_owned()
}

/// The prices asked for so far, by currency, each with when it was given.
pub struct Prices {
    source: Mutex<Arc<dyn PriceSource>>,
    clock: Arc<dyn Clock>,
    known: Mutex<HashMap<String, (u64, i64)>>,
}

/// A fiat amount worked out in XMR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Converted {
    /// Piconero, before the wallet adds its tag.
    pub atomic: u64,
    /// The rate it was worked out at; `None` for an amount already in XMR.
    pub price: Option<PriceUsed>,
}

impl Prices {
    pub fn new(source: Arc<dyn PriceSource>, clock: Arc<dyn Clock>) -> Prices {
        Prices {
            source: Mutex::new(source),
            clock,
            known: Mutex::new(HashMap::new()),
        }
    }

    fn source(&self) -> Arc<dyn PriceSource> {
        self.source
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn source_name(&self) -> String {
        self.source().name()
    }

    /// Prices from `source` from now on; what the last one gave is
    /// forgotten.
    pub fn set_source(&self, source: Arc<dyn PriceSource>) {
        *self.source.lock().unwrap_or_else(|e| e.into_inner()) = source;
        self.known.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// One XMR in `currency`, in millionths, and when it was given: the one
    /// known if it's under [`AGED`] old, else asked for now.
    pub async fn fresh(&self, currency: &str) -> Result<(u64, i64), String> {
        let now = self.clock.now().0;
        let known = self
            .known
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(currency)
            .copied();
        if let Some((rate, at)) = known
            && now.saturating_sub(at) <= AGED as i64
        {
            return Ok((rate, at));
        }
        let source = self.source();
        let rate = source.current(currency).await?;
        if rate == 0 {
            return Err(format!("{} gave no price", source.name()));
        }
        self.known
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(currency.to_owned(), (rate, now));
        Ok((rate, now))
    }

    /// Asks for XMR's price in `currency` now unless the one known is under
    /// `max_age` seconds old: the 5-minute refresh while Till is open
    /// ([`REFRESH`]), and pull to refresh ([`PULL_REFRESH`]). No currency
    /// (XMR only) asks nothing. A failure is kept for the code that needs
    /// the price, which asks again and says so.
    pub async fn keep_fresh(&self, currency: Option<&str>, max_age: u64) -> Result<(), String> {
        let Some(code) = currency.filter(|c| *c != "XMR") else {
            return Ok(());
        };
        if !self.source().prices(code) {
            return Ok(());
        }
        let now = self.clock.now().0;
        let known = self
            .known
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(code)
            .copied();
        if known.is_some_and(|(_, at)| now.saturating_sub(at) < max_age as i64) {
            return Ok(());
        }
        let source = self.source();
        let rate = source.current(code).await?;
        if rate == 0 {
            return Err(format!("{} gave no price", source.name()));
        }
        self.known
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(code.to_owned(), (rate, now));
        Ok(())
    }

    /// `amount` in piconero, rounded to a millionth of an XMR so the
    /// wallet's tag has the digits under it to itself (§5.3). An amount in
    /// XMR is taken as it is; any other is converted at a fresh price, and
    /// refused when none can be had.
    pub async fn convert(&self, amount: &Money) -> Result<Converted, String> {
        let code = amount.currency.as_str();
        if amount.minor <= 0 {
            return Err("There's nothing to pay.".into());
        }
        let minor = amount.minor as u128;
        let digits = amount.currency.minor_digits();
        if code == "XMR" {
            let atomic = minor * 10u128.pow(12u32.saturating_sub(digits));
            return Ok(Converted {
                atomic: round_for_tag(atomic),
                price: None,
            });
        }
        let source = self.source();
        if !source.prices(code) {
            return Err(format!("{} has no price of XMR in {code}.", source.name()));
        }
        let (rate, at) = self.fresh(code).await.map_err(|why| {
            format!(
                "Couldn't get the price of XMR in {code} from {}, so no code was made: {why}",
                source.name()
            )
        })?;
        // minor / 10^digits currency = minor * 10^6 / (rate * 10^digits) XMR.
        let numerator = minor * MICRO * ATOMIC_PER_XMR;
        let denominator = rate as u128 * 10u128.pow(digits);
        let atomic = (numerator + denominator / 2) / denominator;
        Ok(Converted {
            atomic: round_for_tag(atomic),
            price: Some(PriceUsed {
                provider: source.name(),
                rate: Price::new(micro_decimal(rate, digits), code),
                at: UnixSeconds(at),
            }),
        })
    }
}

/// Piconero rounded half up to a millionth of an XMR, and never below one.
fn round_for_tag(atomic: u128) -> u64 {
    let step = fob_wallet::wallet::TAG_MODULUS as u128;
    let rounded = (atomic + step / 2) / step * step;
    u64::try_from(rounded.max(step)).unwrap_or(u64::MAX - u64::MAX % step as u64)
}

/// A price in millionths written with the currency's own digits: 162400000
/// with 2 is "162.40".
fn micro_decimal(micro: u64, digits: u32) -> String {
    let scale = 10u128.pow(6u32.saturating_sub(digits));
    let minor = (micro as u128 + scale / 2) / scale;
    till_types::money::decimal(i64::try_from(minor).unwrap_or(i64::MAX), digits)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
    use till_types::CurrencyCode;

    /// A clock the test moves.
    pub struct TestClock(pub AtomicI64);

    impl Clock for TestClock {
        fn now_ms(&self) -> till_types::UnixMillis {
            till_types::UnixMillis(self.0.load(Ordering::SeqCst) * 1000)
        }
    }

    /// 160 of anything for an XMR, counting how often it's asked; fails
    /// when told to.
    pub struct FixedSource {
        pub asked: AtomicUsize,
        pub failing: std::sync::atomic::AtomicBool,
    }

    impl FixedSource {
        pub fn new() -> FixedSource {
            FixedSource {
                asked: AtomicUsize::new(0),
                failing: false.into(),
            }
        }
    }

    #[async_trait]
    impl PriceSource for FixedSource {
        fn name(&self) -> String {
            "Kraken".into()
        }
        fn prices(&self, currency: &str) -> bool {
            currency != "XYZ"
        }
        async fn current(&self, _: &str) -> Result<u64, String> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            if self.failing.load(Ordering::SeqCst) {
                return Err("no answer".into());
            }
            Ok(160_000_000)
        }
    }

    fn usd(minor: i64) -> Money {
        Money::new(minor, CurrencyCode::USD)
    }

    #[tokio::test]
    async fn a_price_is_used_for_ten_minutes_then_asked_again_and_a_failure_makes_no_code() {
        let source = Arc::new(FixedSource::new());
        let clock = Arc::new(TestClock(AtomicI64::new(1_000)));
        let prices = Prices::new(source.clone(), clock.clone());

        let converted = prices.convert(&usd(2450)).await.unwrap();
        // $24.50 at $160 is 0.153125 XMR.
        assert_eq!(converted.atomic, 153_125_000_000);
        let price = converted.price.unwrap();
        assert_eq!(price.provider, "Kraken");
        assert_eq!(price.rate, Price::new("160.00", "USD"));
        assert_eq!(price.at, UnixSeconds(1_000));

        clock.0.store(1_000 + AGED as i64, Ordering::SeqCst);
        prices.convert(&usd(100)).await.unwrap();
        assert_eq!(source.asked.load(Ordering::SeqCst), 1, "still fresh");

        clock.0.store(1_001 + AGED as i64, Ordering::SeqCst);
        source.failing.store(true, Ordering::SeqCst);
        let refused = prices.convert(&usd(100)).await.unwrap_err();
        assert!(refused.contains("no code was made"), "{refused}");
        assert_eq!(source.asked.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn amounts_round_to_a_millionth_and_xmr_needs_no_price() {
        let source = Arc::new(FixedSource::new());
        let clock = Arc::new(TestClock(AtomicI64::new(0)));
        let prices = Prices::new(source.clone(), clock);
        // 1 cent at $160 is 62.5 µXMR: rounded half up.
        assert_eq!(prices.convert(&usd(1)).await.unwrap().atomic, 63_000_000);
        let yen = Money::new(1200, CurrencyCode::new("JPY").unwrap());
        assert_eq!(
            prices.convert(&yen).await.unwrap().price.unwrap().rate,
            Price::new("160", "JPY")
        );
        let xmr = Money::new(1_500_000_000_000, CurrencyCode::new("XMR").unwrap());
        let converted = prices.convert(&xmr).await.unwrap();
        assert_eq!(
            (converted.atomic, converted.price),
            (1_500_000_000_000, None)
        );
        assert_eq!(source.asked.load(Ordering::SeqCst), 2, "dollars, then yen");
        assert!(prices.convert(&usd(0)).await.is_err());
        let unpriced = Money::new(100, CurrencyCode::new("XYZ").unwrap());
        assert!(prices.convert(&unpriced).await.is_err());
    }

    #[test]
    fn a_provider_is_the_one_named_if_it_prices_the_currency() {
        assert_eq!(
            provider_for(Some("CoinGecko"), Some("USD")),
            Provider::CoinGecko
        );
        assert_eq!(provider_for(None, Some("USD")), Provider::Kraken);
        assert_eq!(provider_for(Some("Nobody"), None), Provider::Kraken);
    }

    #[tokio::test]
    async fn the_price_is_kept_fresh_every_five_minutes_and_xmr_asks_nothing() {
        let source = Arc::new(FixedSource::new());
        let clock = Arc::new(TestClock(AtomicI64::new(1_000)));
        let prices = Prices::new(source.clone(), clock.clone());
        prices.keep_fresh(Some("USD"), REFRESH).await.unwrap();
        assert_eq!(source.asked.load(Ordering::SeqCst), 1);
        clock.0.store(1_000 + REFRESH as i64 - 1, Ordering::SeqCst);
        prices.keep_fresh(Some("USD"), REFRESH).await.unwrap();
        assert_eq!(
            source.asked.load(Ordering::SeqCst),
            1,
            "under 5 minutes old"
        );
        prices.keep_fresh(Some("USD"), PULL_REFRESH).await.unwrap();
        assert_eq!(
            source.asked.load(Ordering::SeqCst),
            2,
            "pulled: a minute is enough"
        );
        prices.convert(&usd(100)).await.unwrap();
        assert_eq!(source.asked.load(Ordering::SeqCst), 2, "the code uses it");
        prices.keep_fresh(None, REFRESH).await.unwrap();
        prices.keep_fresh(Some("XMR"), 0).await.unwrap();
        assert_eq!(source.asked.load(Ordering::SeqCst), 2, "XMR asks nothing");
    }

    #[test]
    fn a_currency_is_named_as_fob_names_it() {
        assert_eq!(currency_plural("AUD"), "Australian dollars");
        assert_eq!(currency_plural("XYZ"), "");
    }

    #[test]
    fn the_keypads_currencies_are_xmr_then_this_devices_then_every_one() {
        let rows = currency_choices(Some("AUD"), "");
        assert_eq!(rows[0].code, "XMR");
        assert_eq!(rows[0].name, "None (XMR only)");
        assert_eq!(rows[1].code, "AUD");
        assert_eq!(rows[1].group.as_deref(), Some("This device"));
        assert_eq!(rows[1].detail, "AUD · Kraken, CoinGecko");
        assert_eq!(rows[2].group.as_deref(), Some("Every currency"));
        assert_eq!(rows.len(), 2 + 45);
        assert_eq!(
            currency_choices(Some("AUD"), "   "),
            rows,
            "blank is no search"
        );
    }

    /// The keypad's currency search: by code, name or a word of it,
    /// whatever the case or accents; the device's row isn't repeated, and
    /// XMR's row stays for what names it.
    #[test]
    fn a_search_keeps_the_currencies_it_matches() {
        let codes = |query: &str| -> Vec<String> {
            currency_choices(Some("AUD"), query)
                .into_iter()
                .map(|r| r.code)
                .collect()
        };
        assert_eq!(codes("aud"), ["AUD", "SAR"], "Saudi riyal has it too");
        assert_eq!(codes("  AUSTRALIAN "), ["AUD"]);
        assert_eq!(codes("Austrálian"), ["AUD"], "accents off");
        assert_eq!(codes("zloty"), ["PLN"]);
        let dollars = codes("dollar");
        assert!(dollars.len() > 3, "{dollars:?}");
        assert!(!dollars.contains(&"XMR".to_owned()));
        assert!(dollars.contains(&"USD".to_owned()));
        assert_eq!(
            dollars.iter().filter(|c| *c == "AUD").count(),
            1,
            "the device's row isn't repeated"
        );
        assert_eq!(codes("none"), ["XMR"]);
        assert_eq!(codes("XMR"), ["XMR"]);
        assert_eq!(codes("monero"), ["XMR"]);
        assert_eq!(codes("qqq"), Vec::<String>::new());

        let rows = currency_choices(Some("AUD"), "dollar");
        assert_eq!(
            rows[0].group,
            Some(format!("{} of 45", rows.len())),
            "the matches say how many of all"
        );
        assert!(rows[1..].iter().all(|r| r.group.is_none()));
        let none = currency_choices(None, "none");
        assert_eq!(none[0].name, "None (XMR only)");
        assert_eq!(none[0].group, None);
    }
}
