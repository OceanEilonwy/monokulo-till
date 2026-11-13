//! Setup's steps and Settings drawn with no engine, from a sample store on
//! Square: for the platforms' screenshots of every screen (ios.yml's
//! artifact), which need the engine's own words without a store, a point
//! of sale or a sign-in. Built here, from the same inputs the engine
//! builds, so the screenshots can't drift from what a till shows.

use std::collections::BTreeMap;
use std::sync::Arc;

use monokulo_client::{AppConnection, Connection, PointOfSale, Store, Wallet};
use till_bell::{BellAddress, WakeUpChoice, WakeUpStatus};
use till_pos::square::SquareLocation;
use till_screens::onboarding::{self, CloverTender, CounterSetup, OnboardingInput, PosSetup, Step};
use till_screens::settings::{self, PosState, SaleChoices, SettingsInput, ShowSales, StoreNow};
use till_screens::wake_ups::{self, WakeUpsInput};
use till_screens::wallet_setup::{
    CheckMark, FobConnectedSetup, KeysSetup, LightServerSetup, PaymentsMode, ThisTillSetup,
    ViewKeyIssue, WalletNetwork, WalletStep,
};
use till_screens::{Device, DeviceClock, OnboardingView, SettingsView, WakeUpsView};

use crate::Platform;
use crate::phases::Money;

/// A step of setup, as the screenshots show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PreviewStep {
    Welcome,
    Scan,
    Confirm,
    Connected,
    /// Step 4 before a point of sale is chosen.
    PointOfSale,
    /// Step 4 signed in to Square, before the test sale.
    SignedIn,
    /// Step 4 once the test sale came in.
    TestSale,
    Locations,
    Counter,
    Alerts,
    /// Android: the bubble over other apps.
    Float,
    ShowSales,
    Ready,
    /// V1: how Till sees payments, the shop's own wallet chosen
    /// (screenshot `vk-mode`).
    VkMode,
    /// V2: Connect to Fob, or enter it yourself (`vk-wallet`).
    VkWallet,
    /// C1: the request's code and pattern (`vk-pad-request`).
    VkPadRequest,
    /// B4 on a phone, C4 on iPad: connected to Fob (`vk-connected`,
    /// `vk-pad-connected`).
    VkConnected,
    /// C4 on any device, as an Android tablet draws it
    /// (`EngineConfig::large_screen`; Android's `vk-pad-connected`).
    VkPadConnected,
    /// D1: scan a wallet code (`vk-scan-code`).
    VkScanCode,
    /// D2 with a view key that doesn't match its address (`vk-keys-error`).
    VkKeysError,
    /// D3: a seed phrase (`vk-seed`).
    VkSeed,
    /// D4: this till's number, node, Paid after and expiry
    /// (`vk-this-till`).
    VkThisTill,
    /// Prices, each provider answered (`vk-prices-step`).
    VkPrices,
    /// Prices with Fob's choice picked, one provider not answering
    /// (`vk-prices-fob`).
    VkPricesFob,
    /// V1 with the shop's light wallet server chosen (`vk-light-mode`).
    VkLightMode,
    /// The light wallet server's address, checked (`vk-light-server`).
    VkLightServer,
    /// D4 through a light wallet server: the node checks its proofs
    /// (`vk-light-this-till`).
    VkLightThisTill,
    /// Step 4 with Till's keypad chosen (`pos-keypad-choose`).
    PosKeypadChoose,
    /// Step 4's keypad currency on the shop's own wallet, Australian
    /// dollars chosen (`pos-keypad-currency`).
    PosKeypadCurrency,
    /// The keypad's currencies searched for "dollar"
    /// (`pos-keypad-currency-search`).
    PosKeypadCurrencySearch,
}

const HOST: &str = "pay.thebeanroom.com";

fn location(id: &str, name: &str) -> SquareLocation {
    SquareLocation {
        id: id.into(),
        name: name.into(),
        business_name: Some("The Bean Room Ltd".into()),
        country: "US".into(),
        currency: "USD".into(),
        timezone: None,
        status: Some("ACTIVE".into()),
    }
}

fn store(device_name: &str) -> StoreNow {
    StoreNow {
        store: Some(Store {
            name: "The Bean Room".into(),
            base_currency: "USD".into(),
            wallet: Some(Wallet {
                name: "Shop wallet".into(),
                network: "mainnet".into(),
                address: "4A8xSAMPLEADDRESS000000000000000000009fQ2".into(),
            }),
        }),
        device: Some(AppConnection {
            app_id: "app_1".into(),
            device_name: device_name.into(),
            platform: "ios".into(),
            connected_at: 1,
        }),
        ..StoreNow::default()
    }
}

fn device_name(device: Device) -> &'static str {
    match device {
        Device::Ipad => "Front iPad",
        Device::Iphone => "Sam's iPhone",
        Device::Android => "Front Pixel",
    }
}

fn signed_in() -> PosSetup {
    PosSetup {
        chosen: Some(PointOfSale::Square),
        location: Some(location("L1", "Front Counter")),
        signed_in: true,
        tender: CloverTender::Ready,
        can_change_location: true,
        setup: true,
        confirmed: true,
        ..PosSetup::default()
    }
}

/// Setup's `step` on `device`, as the engine would publish it.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn preview_onboarding(
    step: PreviewStep,
    device: Device,
    platform: Arc<dyn Platform>,
) -> OnboardingView {
    let name = device_name(device);
    let step = match step {
        PreviewStep::Welcome => Step::Welcome,
        PreviewStep::Scan => Step::Scan { problem: None },
        PreviewStep::Confirm => Step::Confirm {
            host: HOST.into(),
            encrypted: true,
            device_name: name.into(),
            busy: false,
            problem: None,
        },
        PreviewStep::Connected => Step::Connected {
            host: HOST.into(),
            encrypted: true,
            store: store(name),
        },
        PreviewStep::PointOfSale => Step::PointOfSale(PosSetup {
            setup: true,
            ..PosSetup::default()
        }),
        PreviewStep::SignedIn => Step::PointOfSale(signed_in()),
        PreviewStep::TestSale => Step::PointOfSale(PosSetup {
            test_sale: Some("$1.00".into()),
            ..signed_in()
        }),
        PreviewStep::Locations => Step::Locations {
            locations: vec![
                location("L1", "Front Counter"),
                location("L2", "Patio Bar"),
                location("L3", "Kiosk"),
            ],
        },
        PreviewStep::Counter => Step::Counter(CounterSetup {
            amount: "$1.00".into(),
            number: Some("#t0Ab".into()),
            device: name.into(),
            place: Some("Front Counter".into()),
            every_device: false,
            this_till_only: false,
        }),
        PreviewStep::Alerts => Step::Alerts {
            place: Some("Front Counter".into()),
            currency: "USD".into(),
            allowed: false,
        },
        PreviewStep::Float => Step::Float { allowed: false },
        PreviewStep::ShowSales => Step::ShowSales {
            chosen: ShowSales::Alert,
            currency: "USD".into(),
        },
        PreviewStep::Ready => Step::Ready {
            store: "The Bean Room".into(),
            location: Some("Front Counter".into()),
            alerts: true,
            bubble: false,
        },
        PreviewStep::PosKeypadChoose => Step::PointOfSale(PosSetup {
            chosen: Some(PointOfSale::Till),
            setup: true,
            this_till_only: true,
            ..PosSetup::default()
        }),
        PreviewStep::PosKeypadCurrency | PreviewStep::PosKeypadCurrencySearch => {
            let query = if step == PreviewStep::PosKeypadCurrencySearch {
                "dollar"
            } else {
                ""
            };
            Step::PointOfSale(PosSetup {
                chosen: Some(PointOfSale::Till),
                setup: true,
                confirmed: true,
                this_till_only: true,
                currencies: keypad_currencies(Some("AUD"), query),
                keypad_currency: Some("AUD".into()),
                picks_currency: true,
                currency_query: query.into(),
                ..PosSetup::default()
            })
        }
        PreviewStep::VkPrices | PreviewStep::VkPricesFob => {
            Step::Prices(prices_input(step == PreviewStep::VkPricesFob))
        }
        step => Step::Wallet(wallet_step(step, name)),
    };
    let keypad = matches!(&step, Step::PointOfSale(s) if s.chosen == Some(PointOfSale::Till));
    let wallet = keypad || matches!(step, Step::Wallet(_) | Step::Prices(_));
    onboarding::onboarding(
        &OnboardingInput {
            step,
            pos: if keypad {
                Some(PointOfSale::Till)
            } else {
                (!wallet).then_some(PointOfSale::Square)
            },
            wallet,
        },
        device,
        &Money(platform),
    )
}

/// The keypad's currencies, as fob-price lists them for `query`, `chosen`
/// ticked.
fn keypad_currencies(
    chosen: Option<&str>,
    query: &str,
) -> Vec<till_screens::onboarding::CurrencyRow> {
    till_wallet::prices::currency_choices(Some("AUD"), query)
        .into_iter()
        .map(|c| till_screens::onboarding::CurrencyRow {
            selected: chosen == Some(c.code.as_str()),
            code: c.code,
            name: c.name,
            detail: c.detail,
            sign: c.sign,
            group: c.group,
        })
        .collect()
}

/// The sample's prices in Australian dollars: Kraken's and CoinGecko's
/// answers, or (from Fob) CoinGecko picked as Fob's choice and Kraken not
/// answering.
fn prices_input(from_fob: bool) -> till_screens::prices::PricesInput {
    use till_screens::prices::{PricesInput, ProviderCheck};
    PricesInput {
        currency: "AUD".into(),
        plural: "Australian dollars".into(),
        provider: if from_fob { "CoinGecko" } else { "Kraken" }.into(),
        providers: till_wallet::prices::provider_names(Some("AUD")),
        checks: Some(vec![
            ProviderCheck {
                name: "Kraken".into(),
                rate: (!from_fob).then(|| "A$845.10".into()),
                failure: from_fob.then(|| "Not answering".into()),
            },
            ProviderCheck {
                name: "CoinGecko".into(),
                rate: Some("A$846.02".into()),
                failure: None,
            },
        ]),
        from_fob,
        busy: false,
        problem: None,
    }
}

/// The view-key boards' sample (the canvas's): Corner Café's wallet in
/// Fob, its till's account 1, Fob's node and Kraken's prices in AUD.
fn wallet_step(step: PreviewStep, device_name: &str) -> WalletStep {
    const NODE: &str = "https://node.example.com:18089";
    match step {
        PreviewStep::VkMode => WalletStep::Mode {
            chosen: Some(PaymentsMode::Wallet),
        },
        PreviewStep::VkLightMode => WalletStep::Mode {
            chosen: Some(PaymentsMode::LightServer),
        },
        PreviewStep::VkLightServer => WalletStep::LightServer(LightServerSetup {
            url: LIGHT_SERVER.into(),
            self_signed: false,
            checks: vec![
                ("Answers in 0.2 s".into(), CheckMark::Passed),
                ("Mainnet".into(), CheckMark::Passed),
                ("Watches subaddresses".into(), CheckMark::Passed),
            ],
            busy: false,
            problem: None,
        }),
        PreviewStep::VkWallet => WalletStep::Entry,
        PreviewStep::VkPadRequest => {
            // A fixed key, so every screenshot draws the same code and
            // pattern.
            let request = till_wallet::FobRequest::with_key(
                &[7; 32],
                [9; 16],
                "",
                device_name,
                till_wallet::Network::Mainnet,
            )
            .expect("the sample request is well formed");
            let pattern = request.pattern();
            WalletStep::FobRequest {
                qr: request.uri(),
                cells: pattern.cells.iter().flatten().copied().collect(),
                ink: pattern.ink,
                paper: pattern.paper,
                problem: None,
                busy: false,
            }
        }
        PreviewStep::VkConnected | PreviewStep::VkPadConnected => WalletStep::FobConnected(FobConnectedSetup {
            wallet_name: "Corner Café".into(),
            label: "Till (Corner Café)".into(),
            account: 1,
            node: Some(NODE.into()),
            prices: Some(("Kraken".into(), "AUD".into())),
            confirmations: 10,
            expiry_minutes: 30,
            busy: false,
            problem: None,
            tablet: step == PreviewStep::VkPadConnected,
        }),
        PreviewStep::VkScanCode => WalletStep::Code { problem: None },
        PreviewStep::VkKeysError => WalletStep::Keys(KeysSetup {
            network: WalletNetwork::Mainnet,
            address: "48EfgX7qQeDf2pA7Lm3sTuVw9cBn5RkHy1ZaJx6NoP8sMd4GeWq2LtYhKv3FcU7iXb1QpRz5DnAe9mKs4Hj2Tg6Vx8Wc9vW2mPqKz".into(),
            view_key_typed: true,
            address_issue: None,
            view_key_issue: Some(ViewKeyIssue::DoesntMatch),
            ok: false,
            busy: false,
        }),
        PreviewStep::VkSeed => WalletStep::Seed {
            words: 25,
            problem: None,
        },
        _ => WalletStep::ThisTill(ThisTillSetup {
            number: Some(1),
            node: Some(NODE.into()),
            checks: vec![
                ("Answers in 0.4 s".into(), CheckMark::Passed),
                ("Mainnet".into(), CheckMark::Passed),
                ("Up to date".into(), CheckMark::Passed),
            ],
            node_choices: till_wallet::setup::default_nodes(till_wallet::Network::Mainnet),
            confirmations: 10,
            expiry_minutes: 30,
            busy: false,
            problem: None,
            light: step == PreviewStep::VkLightThisTill,
        }),
    }
}

/// The sample light wallet server.
const LIGHT_SERVER: &str = "https://lws.example.com:8443";

fn wake_ups_input() -> WakeUpsInput {
    WakeUpsInput {
        choice: WakeUpChoice::MonokuloBell,
        status: WakeUpStatus::Connected { last_ding_at: None },
        monokulo_bell: BellAddress::parse("bell.monokulo.com", false)
            .expect("the project's Bell's address reads"),
        own_bell: None,
        own_picked: false,
        typed: None,
        pos: Some(PointOfSale::Square),
        distributor_names: BTreeMap::new(),
        allow_http: false,
        alerts_allowed: true,
        problem: None,
    }
}

fn clock(now: i64) -> DeviceClock {
    DeviceClock {
        now,
        utc_offset: 0,
        hours_24: false,
    }
}

/// Settings on `device` for a till signed in to Square at Front Counter,
/// alerts allowed, at `now` (Unix seconds).
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn preview_settings(device: Device, now: i64) -> SettingsView {
    settings::settings(&monokulo_settings_input(device), device, clock(now))
}

/// What [`preview_settings`] draws.
fn monokulo_settings_input(device: Device) -> SettingsInput {
    SettingsInput {
        connection: Some(Connection::new("key", format!("https://{HOST}"))),
        store: store(device_name(device)),
        live: true,
        pos: PosState {
            chosen: Some(PointOfSale::Square),
            signed_in: true,
            location: Some(location("L1", "Front Counter")),
            merchant: None,
            tender_name: "Monero".into(),
            tender: CloverTender::Ready,
            keypad_currency: None,
        },
        shift_on: true,
        alerts: true,
        bubble_wanted: false,
        bubble_allowed: false,
        debug: false,
        base_urls: BTreeMap::new(),
        tor_running: None,
        wake_ups: wake_ups_input(),
        live_activity: true,
        code_in_island: false,
        choices: SaleChoices::default(),
        wallet: None,
    }
}

/// Which Settings the screenshots draw besides the Monokulo store's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PreviewSettings {
    /// The sample Monokulo store on Square: [`preview_settings`].
    Monokulo,
    /// A till on Corner Café's wallet, connected through Fob, at Front
    /// Counter (screenshot `vk-settings`).
    VkSettings,
    /// The same, typed in by hand as till 51, with what the wallet warns
    /// of (`vk-settings-warning`).
    VkSettingsWarning,
    /// A till typed in by hand as till 2, through the shop's light wallet
    /// server (`vk-light-settings`).
    VkLightSettings,
    /// The sample Monokulo store with Till's keypad as its point of sale
    /// (`pos-keypad-settings`).
    PosKeypad,
}

/// Settings as `look` says, on `device` at `now`.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn preview_settings_of(look: PreviewSettings, device: Device, now: i64) -> SettingsView {
    use till_screens::wallet_settings::WalletSettingsInput;
    if look == PreviewSettings::Monokulo {
        return preview_settings(device, now);
    }
    if look == PreviewSettings::PosKeypad {
        let mut input = monokulo_settings_input(device);
        input.pos = PosState {
            chosen: Some(PointOfSale::Till),
            keypad_currency: Some("USD".into()),
            ..PosState::default()
        };
        return settings::settings(&input, device, clock(now));
    }
    let warning = look == PreviewSettings::VkSettingsWarning;
    let light = look == PreviewSettings::VkLightSettings;
    let input = SettingsInput {
        connection: None,
        store: StoreNow::default(),
        live: true,
        pos: PosState {
            chosen: Some(PointOfSale::Square),
            signed_in: true,
            location: Some(location("L1", "Front Counter")),
            merchant: None,
            tender_name: "Monero".into(),
            tender: CloverTender::Ready,
            keypad_currency: None,
        },
        shift_on: true,
        alerts: true,
        bubble_wanted: false,
        bubble_allowed: false,
        debug: false,
        base_urls: BTreeMap::new(),
        tor_running: None,
        wake_ups: WakeUpsInput {
            choice: WakeUpChoice::TimerOnly,
            status: WakeUpStatus::Off,
            ..wake_ups_input()
        },
        live_activity: true,
        code_in_island: false,
        choices: SaleChoices::default(),
        wallet: Some(WalletSettingsInput {
            address: "4A8xSAMPLEADDRESS0000000000000000000000000000000000000000000000000000000000000000000000009fQ2".into(),
            network: WalletNetwork::Mainnet,
            node: Some("https://node.example.com:18089".into()),
            light_server: light.then(|| LIGHT_SERVER.into()),
            // A server the shop runs, with its own certificate.
            light_self_signed: light,
            till_number: match look {
                PreviewSettings::VkSettingsWarning => 51,
                PreviewSettings::VkLightSettings => 2,
                _ => 1,
            },
            fob: (!warning && !light)
                .then(|| ("Corner Café".into(), "Till (Corner Café)".into())),
            confirmations: 10,
            expiry_minutes: 30,
            checks: if light {
                vec![
                    ("Answers in 0.2 s".into(), CheckMark::Passed),
                    ("Mainnet".into(), CheckMark::Passed),
                    ("Watches subaddresses".into(), CheckMark::Passed),
                    ("Key 3f9a c41e 07b2 d58c…".into(), CheckMark::Passed),
                ]
            } else {
                vec![
                    ("Answers in 0.4 s".into(), CheckMark::Passed),
                    ("Mainnet".into(), CheckMark::Passed),
                    ("Up to date".into(), CheckMark::Passed),
                ]
            },
            node_choices: till_wallet::setup::default_nodes(till_wallet::Network::Mainnet),
            currency: Some("USD".into()),
            provider: "Kraken".into(),
            providers: till_wallet::prices::provider_names(Some("USD")),
            price_checks: Some(vec![
                till_screens::prices::ProviderCheck {
                    name: "Kraken".into(),
                    rate: Some("$162.40".into()),
                    failure: None,
                },
                till_screens::prices::ProviderCheck {
                    name: "CoinGecko".into(),
                    rate: Some("$162.55".into()),
                    failure: None,
                },
            ]),
            xmr_only: false,
            warnings: if warning {
                vec![
                    "Another till may be using till number 51.".into(),
                    "A payment of an unexpected amount: check it.".into(),
                ]
            } else {
                Vec::new()
            },
            busy: false,
            problem: None,
        }),
    };
    settings::settings(&input, device, clock(now))
}

/// Settings › Wake-ups on `device`, on the project's Bell, at `now`.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn preview_wake_ups(device: Device, now: i64) -> WakeUpsView {
    wake_ups::wake_ups(&wake_ups_input(), device, clock(now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakePlatform;
    use till_screens::onboarding::OnboardingPage;

    const STEPS: [PreviewStep; 25] = [
        PreviewStep::Welcome,
        PreviewStep::Scan,
        PreviewStep::Confirm,
        PreviewStep::Connected,
        PreviewStep::PointOfSale,
        PreviewStep::SignedIn,
        PreviewStep::TestSale,
        PreviewStep::Locations,
        PreviewStep::Counter,
        PreviewStep::Alerts,
        PreviewStep::Float,
        PreviewStep::ShowSales,
        PreviewStep::Ready,
        PreviewStep::VkMode,
        PreviewStep::VkWallet,
        PreviewStep::VkPadRequest,
        PreviewStep::VkConnected,
        PreviewStep::VkPadConnected,
        PreviewStep::VkScanCode,
        PreviewStep::VkKeysError,
        PreviewStep::VkSeed,
        PreviewStep::VkThisTill,
        PreviewStep::VkLightMode,
        PreviewStep::VkLightServer,
        PreviewStep::VkLightThisTill,
    ];

    #[test]
    fn every_step_draws_on_ipad_and_iphone() {
        let platform: Arc<dyn Platform> = Arc::new(FakePlatform::default());
        for device in [Device::Ipad, Device::Iphone, Device::Android] {
            for step in STEPS {
                let view = preview_onboarding(step, device, platform.clone());
                assert!(
                    !view.heading.is_empty() || matches!(view.page, OnboardingPage::Welcome { .. }),
                    "{step:?} on {device:?}"
                );
            }
        }
        assert!(matches!(
            preview_onboarding(PreviewStep::Welcome, Device::Ipad, platform.clone()).page,
            OnboardingPage::Welcome { .. }
        ));
    }

    #[test]
    fn the_view_key_boards_draw_their_own_pages_with_the_designs_words() {
        let platform: Arc<dyn Platform> = Arc::new(FakePlatform::default());
        let view = |step, device| preview_onboarding(step, device, platform.clone());
        assert!(matches!(
            view(PreviewStep::VkMode, Device::Ipad).page,
            OnboardingPage::PaymentsMode { .. }
        ));
        assert_eq!(
            view(PreviewStep::VkWallet, Device::Android).heading,
            "Connect your wallet"
        );
        let OnboardingPage::FobRequest { view: request } =
            view(PreviewStep::VkPadRequest, Device::Ipad).page
        else {
            panic!("C1")
        };
        assert!(request.qr.starts_with("fob://connect?v=1&app=Till"));
        assert_eq!(request.pattern.cells.len(), 25);
        assert_eq!(
            request.qr,
            match view(PreviewStep::VkPadRequest, Device::Ipad).page {
                OnboardingPage::FobRequest { view } => view.qr,
                _ => unreachable!(),
            },
            "the same code every time"
        );
        let pad = view(PreviewStep::VkConnected, Device::Ipad);
        let OnboardingPage::FobConnected { view: connected } = pad.page else {
            panic!("C4")
        };
        assert!(connected.other_wallet.is_some());
        // An Android tablet draws the iPad's C4.
        let tablet = view(PreviewStep::VkPadConnected, Device::Android);
        let OnboardingPage::FobConnected { view: connected } = tablet.page else {
            panic!("C4")
        };
        assert!(connected.other_wallet.is_some() && connected.eyebrow.is_none());
        let phone = view(PreviewStep::VkConnected, Device::Iphone);
        let OnboardingPage::FobConnected { view: connected } = phone.page else {
            panic!("B4")
        };
        assert_eq!(
            connected.eyebrow.as_deref(),
            Some("Set up Till · Your wallet")
        );
        let OnboardingPage::WalletKeys { view: keys } =
            view(PreviewStep::VkKeysError, Device::Ipad).page
        else {
            panic!("D2")
        };
        assert!(
            keys.view_key
                .note
                .unwrap()
                .starts_with("This view key doesn't match")
        );
        assert_eq!(
            view(PreviewStep::VkThisTill, Device::Ipad).heading,
            "This till"
        );
        let rail = view(PreviewStep::VkThisTill, Device::Ipad).rail;
        assert_eq!(rail[3].label, "This till");
        assert_eq!(rail[3].state, till_screens::onboarding::RailState::Current);
    }

    #[test]
    fn the_light_server_boards_say_the_server_sees_every_payment() {
        let platform: Arc<dyn Platform> = Arc::new(FakePlatform::default());
        let view = |step, device| preview_onboarding(step, device, platform.clone());
        let OnboardingPage::PaymentsMode { view: mode } =
            view(PreviewStep::VkLightMode, Device::Ipad).page
        else {
            panic!("V1")
        };
        assert!(mode.choices[2].selected && mode.choices[2].enabled);
        let server = view(PreviewStep::VkLightServer, Device::Ipad);
        assert_eq!(server.heading, "Your light wallet server");
        assert_eq!(
            server.rail[1].state,
            till_screens::onboarding::RailState::Current
        );
        let OnboardingPage::LightServer { view: step } = server.page else {
            panic!("the server's step")
        };
        assert!(step.footnote.contains("sees every payment"));
        assert!(step.next.enabled);
        let android = view(PreviewStep::VkLightServer, Device::Android);
        assert_eq!(
            android
                .step_label
                .as_deref()
                .map(|l| l.starts_with("Step 2 ")),
            Some(true)
        );
        let this_till = view(PreviewStep::VkLightThisTill, Device::Iphone);
        assert!(this_till.lead.contains("checks your server's proofs"));
        let OnboardingPage::ThisTill { view: d4 } = this_till.page else {
            panic!("D4")
        };
        assert_eq!(d4.node.unwrap().label, "Node for proofs");
        let settings = preview_settings_of(PreviewSettings::VkLightSettings, Device::Ipad, 1_000)
            .wallet
            .unwrap();
        assert_eq!(settings.mode.value, "Your light wallet server");
        let row = settings.node.unwrap();
        assert_eq!(row.label, "Light wallet server");
        let switch = row.self_signed.unwrap();
        assert_eq!(switch.label, "Accept its own certificate");
        assert!(switch.selected);
        assert_eq!(settings.till_number.unwrap().value, "2");
    }

    #[test]
    fn wallet_settings_show_the_wallet_in_place_of_a_store() {
        let view = preview_settings_of(PreviewSettings::VkSettings, Device::Ipad, 1_000);
        assert!(view.store.is_none());
        let wallet = view.wallet.expect("a wallet section");
        assert_eq!(
            wallet.fob.unwrap().value,
            "Corner Café · Till (Corner Café)"
        );
        assert!(wallet.till_number.is_none() && wallet.warnings.is_empty());
        assert_eq!(view.disconnect, "Disconnect this wallet");
        let warned =
            preview_settings_of(PreviewSettings::VkSettingsWarning, Device::Android, 1_000)
                .wallet
                .unwrap();
        assert_eq!(warned.warnings.len(), 2);
        assert!(warned.lookahead.is_some());
        assert_eq!(warned.till_number.unwrap().value, "51");
    }

    #[test]
    fn settings_show_the_sample_store_on_square() {
        let view = preview_settings(Device::Ipad, 1_000);
        assert_eq!(view.store.expect("a store section").name, "The Bean Room");
        let wake_ups = preview_wake_ups(Device::Iphone, 1_000);
        assert!(wake_ups.monokulo_bell.selected);
    }
}
