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
    };
    onboarding::onboarding(
        &OnboardingInput {
            step,
            pos: Some(PointOfSale::Square),
        },
        device,
        &Money(platform),
    )
}

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
    let input = SettingsInput {
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

    const STEPS: [PreviewStep; 13] = [
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
    fn settings_show_the_sample_store_on_square() {
        let view = preview_settings(Device::Ipad, 1_000);
        assert_eq!(view.store.expect("a store section").name, "The Bean Room");
        let wake_ups = preview_wake_ups(Device::Iphone, 1_000);
        assert!(wake_ups.monokulo_bell.selected);
    }
}
