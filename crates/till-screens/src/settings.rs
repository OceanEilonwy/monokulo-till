//! Settings (boards PhoneSettings and PadSettings; Android's
//! `ui/Settings.kt`): the store, Tor, the point of sale, the shift, alerts,
//! the bubble, Wake-ups and the debug build's stand-ins.
//!
//! Where the platforms differ: Android has the shift's switch, the bubble,
//! Tor and Wake-ups in place; iOS has no bubble and no Tor, opens Wake-ups
//! from a row of its own, and on iPhone has the Dynamic Island section.
//!
//! The section headings, the switches' labels, "Disconnect this device"
//! and the footnotes no state chooses (the wallet's, Disconnect's, the
//! Dynamic Island's) stay in the platform's screen.

use std::collections::BTreeMap;

use monokulo_client::{AppConnection, Connection, PointOfSale, Store, tor};
use till_bell::WakeUpChoice;
use till_pos::square::SquareLocation;

use crate::onboarding::CloverTender;
use crate::pos_text;
use crate::time::DeviceClock;
use crate::view::{Device, Fact, PosChoice, PosMark, Tone, capitalised, mark};
use crate::wake_ups::{WakeUpsInput, WakeUpsView, wake_ups};
use crate::wake_ups_text;

/// The store as last read: shown while Settings reads it again (Android:
/// `StoreNow`).
#[derive(Clone, Debug, Default)]
pub struct StoreNow {
    pub store: Option<Store>,
    pub device: Option<AppConnection>,
    /// The entry on the store's onion address, while it has one.
    pub onion_api: Option<String>,
    /// A read is under way.
    pub reading: bool,
    /// Why the last read failed, if it did.
    pub problem: Option<String>,
}

impl StoreNow {
    /// What a fact not read yet says: "Reading…" while a read is under way.
    fn unknown(&self) -> &'static str {
        if self.reading { "Reading…" } else { "—" }
    }
}

/// Settings › Point of sale's state (acceptance T1.2).
#[derive(Clone, Debug, Default)]
pub struct PosState {
    /// `None` while none is chosen (or the till runs without one).
    pub chosen: Option<PointOfSale>,
    pub signed_in: bool,
    /// Square's location, while signed in to Square.
    pub location: Option<SquareLocation>,
    /// Clover's merchant name, while signed in to Clover.
    pub merchant: Option<String>,
    pub tender_name: String,
    /// Clover's Monero payment type, while signed in to Clover.
    pub tender: CloverTender,
}

impl PosState {
    /// Signed in and at a location or merchant: the facts show.
    pub fn connected(&self) -> bool {
        connected(
            self.chosen,
            self.signed_in,
            self.location.is_some(),
            self.merchant.is_some(),
        )
    }
}

/// Signed in to `chosen` and at a Square location (`location`) or a Clover
/// merchant (`merchant`): step 4 can go on, and the facts show.
pub fn connected(
    chosen: Option<PointOfSale>,
    signed_in: bool,
    location: bool,
    merchant: bool,
) -> bool {
    signed_in
        && match chosen {
            Some(PointOfSale::Square) => location,
            Some(PointOfSale::Clover) => merchant,
            None => false,
        }
}

/// What Settings shows.
#[derive(Clone, Debug)]
pub struct SettingsInput {
    /// `None` once disconnected: no store section.
    pub connection: Option<Connection>,
    pub store: StoreNow,
    /// The store's event stream is up.
    pub live: bool,
    pub pos: PosState,
    pub shift_on: bool,
    pub alerts: bool,
    /// Android: the bubble wanted, and allowed by Android.
    pub bubble_wanted: bool,
    pub bubble_allowed: bool,
    /// A debug build, with the stand-ins' addresses by point of sale
    /// (acceptance T6.5).
    pub debug: bool,
    pub base_urls: BTreeMap<&'static str, String>,
    /// Orbot's proxy answers; `None` while that's being checked.
    pub tor_running: Option<bool>,
    pub wake_ups: WakeUpsInput,
    /// iPhone: the Live Activity each shift, and the code in the island.
    pub live_activity: bool,
    pub code_in_island: bool,
}

/// Settings, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SettingsView {
    pub store: Option<StoreSection>,
    /// Android, while the store has an onion address.
    pub tor: Option<TorSection>,
    pub point_of_sale: PosSection,
    /// Android: "Watch for sales".
    pub shift: Option<SwitchSection>,
    pub alerts: SwitchSection,
    /// Android: "Float over other apps".
    pub bubble: Option<SwitchSection>,
    pub wake_ups: WakeUpsPlace,
    /// iPhone.
    pub island: Option<IslandSection>,
    /// Debug builds: a stand-in's address for each point of sale.
    pub stand_ins: Vec<StandIn>,
}

/// The store, its wallet and this device.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct StoreSection {
    /// Why the last read failed.
    pub problem: Option<String>,
    /// The store's name, or "Reading…" or "—".
    pub name: String,
    pub host: String,
    /// The green lock beside the host.
    pub encrypted: bool,
    /// "Online" (a green dot) or "Offline".
    pub online: bool,
    pub online_label: String,
    /// "Payments go to", "Wallet address" (monospaced, with Copy),
    /// "Network", "Base currency", "This device".
    pub facts: Vec<Fact>,
    /// The whole address Copy copies, while the wallet is known.
    pub wallet_address: Option<String>,
}

/// Tor through Orbot (decision 55).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TorSection {
    /// Connected at the onion address: "Over Tor · Always", with no switch.
    pub always: bool,
    /// "Reach the store over Tor", unless always.
    pub on: bool,
    pub footnote: String,
}

/// Settings › Point of sale (acceptance T1.2).
// A view record is built once per change and crosses UniFFI by value,
// which can't box a variant.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PosSection {
    /// Signed in: which one and its facts, Sign out and Change the point of
    /// sale.
    SignedIn {
        mark: PosMark,
        mark_in_ink: bool,
        name: String,
        /// Square's account.
        account: Option<Fact>,
        /// The location (tap to change it, `change_location`) or merchant.
        place: Fact,
        change_location: bool,
        /// "Payment type", or Clover's "Not in Clover yet" in warning.
        payment_type: Fact,
        /// Clover, the payment type missing: "Look again".
        look_again: Option<String>,
        sign_out: String,
        change: String,
        footnotes: Vec<String>,
    },
    /// Not signed in: the choice rows and the chosen one's sign-in.
    Choosing {
        choices: Vec<PosChoice>,
        sign_in: Option<String>,
        footnote: String,
    },
}

/// A section that is one switch and its footnote.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SwitchSection {
    pub on: bool,
    pub footnote: String,
}

/// Where Wake-ups is.
// A view record is built once per change and crosses UniFFI by value,
// which can't box a variant.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum WakeUpsPlace {
    /// Android: a section of Settings.
    Inline { view: WakeUpsView },
    /// iOS: a row that opens the Wake-ups screen, saying how sales come.
    Row { row: Fact },
}

/// iPhone's Dynamic Island section.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct IslandSection {
    pub live_activity: bool,
    /// "Show code in island", off by default.
    pub code_in_island: bool,
}

/// A debug build's stand-in for a point of sale.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct StandIn {
    pub point_of_sale: PosMark,
    /// "Stand-in Square address".
    pub label: String,
    pub value: String,
}

/// Settings on `device`.
pub fn settings(input: &SettingsInput, device: Device, clock: DeviceClock) -> SettingsView {
    let pos = input.pos.chosen;
    let android = device == Device::Android;
    SettingsView {
        store: input
            .connection
            .as_ref()
            .map(|c| store_section(c, &input.store, input.live)),
        tor: input
            .connection
            .as_ref()
            .filter(|_| android)
            .and_then(|c| tor_section(c, input.store.onion_api.as_deref(), input.tor_running)),
        point_of_sale: pos_section(&input.pos, device),
        shift: android.then(|| SwitchSection {
            on: input.shift_on,
            footnote: wake_ups_text::shift(&input.wake_ups.status, pos),
        }),
        alerts: SwitchSection {
            on: input.alerts,
            footnote: alerts_footnote(input.alerts, device).into(),
        },
        bubble: android.then(|| SwitchSection {
            on: input.bubble_wanted && input.bubble_allowed,
            footnote: if !input.bubble_allowed {
                "Android hasn't allowed Till to display over other apps, so sales show in the notification only.".into()
            } else if input.bubble_wanted {
                pos_text::bubble_footnote(pos)
            } else {
                "Sales show in the notification only.".into()
            },
        }),
        wake_ups: if android {
            WakeUpsPlace::Inline {
                view: wake_ups(&input.wake_ups, device, clock),
            }
        } else {
            WakeUpsPlace::Row {
                row: wake_ups_row(&input.wake_ups, device),
            }
        },
        island: (device == Device::Iphone).then_some(IslandSection {
            live_activity: input.live_activity,
            code_in_island: input.code_in_island,
        }),
        stand_ins: if input.debug {
            PointOfSale::ALL
                .into_iter()
                .map(|p| StandIn {
                    point_of_sale: mark(p),
                    label: pos_text::stand_in(p),
                    value: input.base_urls.get(p.wire()).cloned().unwrap_or_default(),
                })
                .collect()
        } else {
            Vec::new()
        },
    }
}

fn store_section(connection: &Connection, now: &StoreNow, live: bool) -> StoreSection {
    let unknown = now.unknown();
    let store = now.store.as_ref();
    let wallet = store.and_then(|s| s.wallet.as_ref());
    let mut facts = Vec::new();
    if let Some(wallet) = wallet {
        facts.push(Fact::new("Payments go to", &wallet.name));
        facts.push(Fact::new("Wallet address", wallet.address_short()).mono());
        facts.push(Fact::new("Network", capitalised(&wallet.network)));
    }
    facts.push(Fact::new(
        "Base currency",
        store.map_or(unknown, |s| &s.base_currency),
    ));
    facts.push(Fact::new(
        "This device",
        now.device.as_ref().map_or(unknown, |d| &d.device_name),
    ));
    StoreSection {
        problem: now.problem.clone(),
        name: store.map_or(unknown, |s| &s.name).to_owned(),
        host: connection.host(),
        encrypted: connection.encrypted(),
        online: live,
        online_label: if live { "Online" } else { "Offline" }.into(),
        facts,
        wallet_address: wallet.map(|w| w.address.clone()),
    }
}

/// Tor (decision 55): offered while the store has an onion address, through
/// Orbot. A till that connected at the onion address is on Tor already.
fn tor_section(
    connection: &Connection,
    onion_api: Option<&str>,
    running: Option<bool>,
) -> Option<TorSection> {
    let at_onion = tor::is_onion(&connection.api);
    if !at_onion && connection.onion_api.is_none() && onion_api.is_none() {
        return None;
    }
    let footnote = if connection.over_tor() && running == Some(false) {
        format!(
            "Orbot isn't running, so Till can't reach the store. Open Orbot{}",
            if at_onion { "." } else { ", or turn this off." }
        )
    } else if at_onion {
        "Till connected at the store's onion address, so it reaches the store through Orbot.".into()
    } else if connection.tor {
        "Till reaches the store at its onion address, through Orbot.".into()
    } else {
        "The store has an onion address. Turn this on to reach it over Tor, through the Orbot app."
            .into()
    };
    Some(TorSection {
        always: at_onion,
        on: at_onion || connection.tor,
        footnote,
    })
}

fn pos_section(p: &PosState, device: Device) -> PosSection {
    match p.chosen {
        Some(chosen) if p.connected() => {
            let place_row = pos_text::place_row(Some(chosen));
            let (account, place, change_location) = match chosen {
                PointOfSale::Square => {
                    let location = p.location.as_ref().expect("connected to Square");
                    (
                        Some(Fact::new(
                            "Account",
                            location.business_name.as_deref().unwrap_or("Signed in"),
                        )),
                        Fact::new(place_row, &location.name),
                        true,
                    )
                }
                PointOfSale::Clover => (
                    None,
                    Fact::new(place_row, p.merchant.clone().unwrap_or_default()),
                    false,
                ),
            };
            let missing = chosen == PointOfSale::Clover && p.tender == CloverTender::AddInClover;
            let mut footnotes = Vec::new();
            if missing {
                footnotes.push(pos_text::plain(pos_text::CLOVER_ADD_TENDER));
            }
            footnotes.push(pos_text::signed_in_footnote(chosen));
            PosSection::SignedIn {
                mark: mark(chosen),
                mark_in_ink: pos_text::mark_in_ink(chosen),
                name: chosen.display_name().into(),
                account,
                place,
                change_location,
                payment_type: if missing {
                    Fact::new("Payment type", pos_text::tender_missing(chosen)).toned(Tone::Warn)
                } else {
                    Fact::new("Payment type", &p.tender_name)
                },
                look_again: missing.then(|| pos_text::LOOK_AGAIN.into()),
                sign_out: pos_text::sign_out(chosen),
                change: pos_text::CHANGE.into(),
                footnotes,
            }
        }
        chosen => PosSection::Choosing {
            choices: pos_choices(chosen, true, device),
            sign_in: chosen.map(|c| pos_text::sign_in(Some(c))),
            footnote: pos_text::without_footnote(chosen),
        },
    }
}

/// Square and Clover as one radio group (acceptance T1.1), each led by its
/// mark. On iOS, Clover's says the phone or iPad sits beside the device.
pub(crate) fn pos_choices(
    chosen: Option<PointOfSale>,
    enabled: bool,
    device: Device,
) -> Vec<PosChoice> {
    PointOfSale::ALL
        .into_iter()
        .map(|pos| PosChoice {
            mark: mark(pos),
            mark_in_ink: pos_text::mark_in_ink(pos),
            choice: crate::view::Choice {
                label: pos.display_name().into(),
                detail: Some(choice_detail(pos, device)),
                selected: chosen == Some(pos),
                enabled,
            },
        })
        .collect()
}

/// [`pos_text::choice_detail`], with where an iOS device stands beside a
/// Clover device (board PhonePos).
fn choice_detail(pos: PointOfSale, device: Device) -> String {
    let detail = pos_text::choice_detail(pos);
    match (pos, device) {
        (PointOfSale::Clover, Device::Iphone) => {
            format!("{detail}. This iPhone sits beside it and shows the code.")
        }
        (PointOfSale::Clover, Device::Ipad) => {
            format!("{detail}. This iPad sits beside it and shows the code.")
        }
        _ => detail.into(),
    }
}

/// Under the alerts switch.
fn alerts_footnote(on: bool, device: Device) -> &'static str {
    match (on, device) {
        (true, Device::Android) => {
            "A new sale makes a sound and shows a banner when you're in another app. Change the sound in Android's settings."
        }
        (true, Device::Iphone | Device::Ipad) => {
            "A new sale makes a sound and shows a banner when you're in another app."
        }
        (false, Device::Android) => {
            "Alerts are off. Till still shows new sales in its bubble and here, but won't make a sound or show a banner."
        }
        (false, Device::Iphone) => {
            "Alerts are off. Till still shows new sales in the Dynamic Island, but won't make a sound or show a banner when you're in another app."
        }
        (false, Device::Ipad) => {
            "With alerts off, you won't know about a sale while Till is tucked off the edge. Keep the panel showing."
        }
    }
}

/// iOS's Wake-ups row: the timer alone, or how the chosen Bell stands.
fn wake_ups_row(w: &WakeUpsInput, device: Device) -> Fact {
    match w.choice {
        WakeUpChoice::TimerOnly => Fact::new("Wake-ups", "Timer only"),
        WakeUpChoice::MonokuloBell | WakeUpChoice::OwnBell { .. } => {
            let connected = matches!(w.status, till_bell::WakeUpStatus::Connected { .. });
            Fact::new("Wake-ups", wake_ups_text::word(&w.status, device)).toned(if connected {
                Tone::Paid
            } else {
                Tone::Muted
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::Choice;
    use monokulo_client::Wallet;
    use till_bell::{BellAddress, WakeUpStatus};

    const CLOCK: DeviceClock = DeviceClock {
        now: 1_793_203_080,
        utc_offset: 0,
        hours_24: false,
    };

    fn location() -> SquareLocation {
        SquareLocation {
            id: "L1".into(),
            name: "Front Counter".into(),
            business_name: Some("The Bean Room Ltd".into()),
            country: "US".into(),
            currency: "USD".into(),
            timezone: None,
            status: Some("ACTIVE".into()),
        }
    }

    fn store() -> StoreNow {
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
                device_name: "Sam's iPhone".into(),
                platform: "ios".into(),
                connected_at: 1,
            }),
            ..StoreNow::default()
        }
    }

    fn input() -> SettingsInput {
        SettingsInput {
            connection: Some(Connection::new("key", "https://pay.yourshop.com")),
            store: store(),
            live: true,
            pos: PosState {
                chosen: Some(PointOfSale::Square),
                signed_in: true,
                location: Some(location()),
                merchant: None,
                tender_name: "Monero".into(),
                tender: CloverTender::Ready,
            },
            shift_on: true,
            alerts: false,
            bubble_wanted: true,
            bubble_allowed: true,
            debug: false,
            base_urls: BTreeMap::new(),
            tor_running: None,
            wake_ups: WakeUpsInput {
                choice: WakeUpChoice::MonokuloBell,
                status: WakeUpStatus::Connected { last_ding_at: None },
                monokulo_bell: BellAddress::parse("bell.monokulo.com", false).unwrap(),
                own_bell: None,
                own_picked: false,
                typed: None,
                pos: Some(PointOfSale::Square),
                distributor_names: BTreeMap::new(),
                allow_http: false,
                alerts_allowed: false,
                problem: None,
            },
            live_activity: true,
            code_in_island: false,
        }
    }

    #[test]
    fn the_store_section_shows_the_store_its_wallet_and_this_device() {
        let view = settings(&input(), Device::Iphone, CLOCK);
        assert_eq!(
            view.store,
            Some(StoreSection {
                problem: None,
                name: "The Bean Room".into(),
                host: "pay.yourshop.com".into(),
                encrypted: true,
                online: true,
                online_label: "Online".into(),
                facts: vec![
                    Fact::new("Payments go to", "Shop wallet"),
                    Fact::new("Wallet address", "4A8xSAMP…00009fQ2").mono(),
                    Fact::new("Network", "Mainnet"),
                    Fact::new("Base currency", "USD"),
                    Fact::new("This device", "Sam's iPhone"),
                ],
                wallet_address: Some("4A8xSAMPLEADDRESS000000000000000000009fQ2".into()),
            })
        );
    }

    #[test]
    fn a_store_not_read_yet_says_reading_or_a_dash() {
        let reading = SettingsInput {
            store: StoreNow {
                reading: true,
                ..StoreNow::default()
            },
            live: false,
            ..input()
        };
        let store = settings(&reading, Device::Android, CLOCK).store.unwrap();
        assert_eq!(store.name, "Reading…");
        assert_eq!(store.online_label, "Offline");
        assert_eq!(
            store.facts,
            [
                Fact::new("Base currency", "Reading…"),
                Fact::new("This device", "Reading…")
            ]
        );
        assert_eq!(store.wallet_address, None);
        let failed = SettingsInput {
            store: StoreNow {
                problem: Some("Couldn't reach the store.".into()),
                ..StoreNow::default()
            },
            ..input()
        };
        let store = settings(&failed, Device::Android, CLOCK).store.unwrap();
        assert_eq!(store.name, "—");
        assert_eq!(store.problem.as_deref(), Some("Couldn't reach the store."));
        let gone = SettingsInput {
            connection: None,
            ..input()
        };
        assert_eq!(settings(&gone, Device::Android, CLOCK).store, None);
    }

    #[test]
    fn signed_in_to_square_shows_its_account_location_and_payment_type() {
        assert_eq!(
            settings(&input(), Device::Android, CLOCK).point_of_sale,
            PosSection::SignedIn {
                mark: PosMark::Square,
                mark_in_ink: true,
                name: "Square".into(),
                account: Some(Fact::new("Account", "The Bean Room Ltd")),
                place: Fact::new("Location", "Front Counter"),
                change_location: true,
                payment_type: Fact::new("Payment type", "Monero"),
                look_again: None,
                sign_out: "Sign out of Square".into(),
                change: "Change the point of sale".into(),
                footnotes: vec![
                    "Sales paid with this payment type at this location show up in Till.".into()
                ],
            }
        );
    }

    #[test]
    fn clover_without_its_payment_type_says_how_to_add_it_and_offers_look_again() {
        let clover = SettingsInput {
            pos: PosState {
                chosen: Some(PointOfSale::Clover),
                signed_in: true,
                location: None,
                merchant: Some("The Bean Room".into()),
                tender_name: "Monero".into(),
                tender: CloverTender::AddInClover,
            },
            ..input()
        };
        assert_eq!(
            settings(&clover, Device::Ipad, CLOCK).point_of_sale,
            PosSection::SignedIn {
                mark: PosMark::Clover,
                mark_in_ink: false,
                name: "Clover".into(),
                account: None,
                place: Fact::new("Merchant", "The Bean Room"),
                change_location: false,
                payment_type: Fact::new("Payment type", "Not in Clover yet").toned(Tone::Warn),
                look_again: Some("Look again".into()),
                sign_out: "Sign out of Clover".into(),
                change: "Change the point of sale".into(),
                footnotes: vec![
                    "Add a payment type called Monero in Clover (Setup › Payments, or ask whoever manages your Clover account), then tap Look again".into(),
                    "Sales paid with this payment type at this merchant show up in Till.".into(),
                ],
            }
        );
    }

    #[test]
    fn not_signed_in_shows_the_choices_and_the_chosen_ones_sign_in() {
        let none = SettingsInput {
            pos: PosState::default(),
            ..input()
        };
        let choosing = settings(&none, Device::Android, CLOCK).point_of_sale;
        let PosSection::Choosing {
            choices,
            sign_in,
            footnote,
        } = choosing
        else {
            panic!("{choosing:?}")
        };
        assert_eq!(
            choices,
            [
                PosChoice {
                    mark: PosMark::Square,
                    mark_in_ink: true,
                    choice: Choice {
                        label: "Square".into(),
                        detail: Some("Square Point of Sale on a phone, tablet or Register".into()),
                        selected: false,
                        enabled: true,
                    },
                },
                PosChoice {
                    mark: PosMark::Clover,
                    mark_in_ink: false,
                    choice: Choice {
                        label: "Clover".into(),
                        detail: Some("A Clover device".into()),
                        selected: false,
                        enabled: true,
                    },
                },
            ]
        );
        assert_eq!(sign_in, None);
        assert_eq!(
            footnote,
            "Without a point of sale, Till shows the sales other tills make, without receipts."
        );

        let signed_out = SettingsInput {
            pos: PosState {
                chosen: Some(PointOfSale::Clover),
                ..PosState::default()
            },
            ..input()
        };
        let PosSection::Choosing {
            choices,
            sign_in,
            footnote,
        } = settings(&signed_out, Device::Iphone, CLOCK).point_of_sale
        else {
            panic!()
        };
        assert!(choices[1].choice.selected);
        assert_eq!(
            choices[1].choice.detail.as_deref(),
            Some("A Clover device. This iPhone sits beside it and shows the code.")
        );
        assert_eq!(sign_in.as_deref(), Some("Sign in to Clover"));
        assert_eq!(
            footnote,
            "Without Clover, Till shows the sales other tills make, without receipts."
        );
    }

    #[test]
    fn android_has_the_shift_the_bubble_and_wake_ups_in_place() {
        let view = settings(&input(), Device::Android, CLOCK);
        assert_eq!(
            view.shift,
            Some(SwitchSection {
                on: true,
                footnote: "While it's on, the Bell wakes Till when a sale comes in and it checks Square every 30 seconds between wake-ups. Turn it off at the end of the day.".into(),
            })
        );
        assert_eq!(
            view.alerts,
            SwitchSection {
                on: false,
                footnote: "Alerts are off. Till still shows new sales in its bubble and here, but won't make a sound or show a banner.".into(),
            }
        );
        assert_eq!(
            view.bubble,
            Some(SwitchSection {
                on: true,
                footnote: "A faded bubble floats over Square and lights up when a sale comes in. Drag it to either side.".into(),
            })
        );
        assert_eq!(
            view.wake_ups,
            WakeUpsPlace::Inline {
                view: wake_ups(&input().wake_ups, Device::Android, CLOCK)
            }
        );
        assert_eq!(view.island, None);
        assert_eq!(view.stand_ins, []);

        let alerts_on = settings(
            &SettingsInput {
                alerts: true,
                ..input()
            },
            Device::Android,
            CLOCK,
        );
        assert_eq!(
            alerts_on.alerts.footnote,
            "A new sale makes a sound and shows a banner when you're in another app. Change the sound in Android's settings."
        );
    }

    #[test]
    fn the_bubble_says_why_sales_show_only_in_the_notification() {
        let bubble = |wanted, allowed| {
            settings(
                &SettingsInput {
                    bubble_wanted: wanted,
                    bubble_allowed: allowed,
                    ..input()
                },
                Device::Android,
                CLOCK,
            )
            .bubble
            .unwrap()
        };
        assert_eq!(
            bubble(true, false),
            SwitchSection {
                on: false,
                footnote: "Android hasn't allowed Till to display over other apps, so sales show in the notification only.".into()
            }
        );
        assert_eq!(
            bubble(false, true),
            SwitchSection {
                on: false,
                footnote: "Sales show in the notification only.".into()
            }
        );
    }

    #[test]
    fn an_iphone_has_a_wake_ups_row_and_the_dynamic_island_and_no_shift_bubble_or_tor() {
        let onion = SettingsInput {
            connection: Some(Connection {
                onion_api: Some("http://abc.onion".into()),
                ..Connection::new("key", "https://pay.yourshop.com")
            }),
            ..input()
        };
        let view = settings(&onion, Device::Iphone, CLOCK);
        assert_eq!(view.shift, None);
        assert_eq!(view.bubble, None);
        assert_eq!(view.tor, None, "no Orbot on iOS");
        assert_eq!(
            view.alerts.footnote,
            "Alerts are off. Till still shows new sales in the Dynamic Island, but won't make a sound or show a banner when you're in another app."
        );
        assert_eq!(
            view.wake_ups,
            WakeUpsPlace::Row {
                row: Fact::new("Wake-ups", "Connected").toned(Tone::Paid)
            }
        );
        assert_eq!(
            view.island,
            Some(IslandSection {
                live_activity: true,
                code_in_island: false
            })
        );

        let timer = SettingsInput {
            wake_ups: WakeUpsInput {
                choice: WakeUpChoice::TimerOnly,
                status: WakeUpStatus::Off,
                ..input().wake_ups
            },
            alerts: true,
            ..input()
        };
        let view = settings(&timer, Device::Iphone, CLOCK);
        assert_eq!(
            view.wake_ups,
            WakeUpsPlace::Row {
                row: Fact::new("Wake-ups", "Timer only")
            }
        );
        assert_eq!(
            view.alerts.footnote,
            "A new sale makes a sound and shows a banner when you're in another app."
        );
    }

    #[test]
    fn an_ipad_says_what_alerts_off_costs_in_slide_over_and_has_no_island() {
        let view = settings(&input(), Device::Ipad, CLOCK);
        assert_eq!(
            view.alerts.footnote,
            "With alerts off, you won't know about a sale while Till is tucked off the edge. Keep the panel showing."
        );
        assert_eq!(view.island, None);
        assert!(matches!(view.wake_ups, WakeUpsPlace::Row { .. }));
    }

    #[test]
    fn tor_is_offered_while_the_store_has_an_onion_address() {
        let tor = |connection: Connection, onion_api: Option<&str>, running: Option<bool>| {
            settings(
                &SettingsInput {
                    connection: Some(connection),
                    store: StoreNow {
                        onion_api: onion_api.map(str::to_owned),
                        ..store()
                    },
                    tor_running: running,
                    ..input()
                },
                Device::Android,
                CLOCK,
            )
            .tor
        };
        let clear = Connection::new("key", "https://pay.yourshop.com");
        assert_eq!(tor(clear.clone(), None, None), None);
        assert_eq!(
            tor(clear.clone(), Some("http://abc.onion"), None),
            Some(TorSection {
                always: false,
                on: false,
                footnote: "The store has an onion address. Turn this on to reach it over Tor, through the Orbot app.".into()
            })
        );
        let on = Connection {
            onion_api: Some("http://abc.onion".into()),
            tor: true,
            ..clear
        };
        assert_eq!(
            tor(on.clone(), None, Some(true)).unwrap().footnote,
            "Till reaches the store at its onion address, through Orbot."
        );
        assert_eq!(
            tor(on, None, Some(false)).unwrap().footnote,
            "Orbot isn't running, so Till can't reach the store. Open Orbot, or turn this off."
        );
        let onion = Connection::new("key", "http://abc.onion");
        assert_eq!(
            tor(onion.clone(), None, None),
            Some(TorSection {
                always: true,
                on: true,
                footnote: "Till connected at the store's onion address, so it reaches the store through Orbot.".into()
            })
        );
        assert_eq!(
            tor(onion, None, Some(false)).unwrap().footnote,
            "Orbot isn't running, so Till can't reach the store. Open Orbot."
        );
    }

    #[test]
    fn a_debug_build_has_a_stand_in_for_each_point_of_sale() {
        let mut base_urls = BTreeMap::new();
        base_urls.insert("clover", "http://10.0.2.2:8090".to_owned());
        let view = settings(
            &SettingsInput {
                debug: true,
                base_urls,
                ..input()
            },
            Device::Android,
            CLOCK,
        );
        assert_eq!(
            view.stand_ins,
            [
                StandIn {
                    point_of_sale: PosMark::Square,
                    label: "Stand-in Square address".into(),
                    value: String::new()
                },
                StandIn {
                    point_of_sale: PosMark::Clover,
                    label: "Stand-in Clover address".into(),
                    value: "http://10.0.2.2:8090".into()
                },
            ]
        );
    }
}
