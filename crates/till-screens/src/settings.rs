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

use crate::counters::Counters;
use crate::onboarding::CloverTender;
use crate::pos_text;
use crate::time::DeviceClock;
use crate::view::{Device, Fact, PosChoice, PosMark, Press, Tone, capitalised, mark};
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
    /// Counters, cash and how a new sale shows up.
    pub choices: SaleChoices,
}

/// How a new sale shows up on iOS (O4 and Settings; core.md decision 120):
/// an alert over the point of sale, or Till and the point of sale side by
/// side. The floating Picture in Picture window was dropped (core.md
/// decision 225). The engine keeps the
/// choice; the platform does the showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ShowSales {
    #[default]
    Alert,
    SideBySide,
}

impl ShowSales {
    pub const ALL: [ShowSales; 2] = [ShowSales::Alert, ShowSales::SideBySide];

    /// The choice as it is kept, by the platform's store and in the
    /// ledger's settings alike: `side`, or nothing for the default alert
    /// (decision 214).
    pub fn wire(self) -> Option<&'static str> {
        match self {
            ShowSales::Alert => None,
            ShowSales::SideBySide => Some("side"),
        }
    }

    /// The choice `kept` says. The ledger's settings once held the variant's
    /// own name (`SideBySide`, `Alert`), which reads as the same choice;
    /// anything else is the alert, the dropped floating window (`pip`,
    /// `FloatingWindow`) among them (decision 225).
    pub fn from_wire(kept: Option<&str>) -> ShowSales {
        match kept {
            Some("side" | "SideBySide") => ShowSales::SideBySide,
            _ => ShowSales::Alert,
        }
    }

    /// The choice's words (board OnbShow).
    pub fn label(self) -> &'static str {
        match self {
            ShowSales::Alert => "An alert (recommended)",
            ShowSales::SideBySide => "Side by side",
        }
    }

    /// The line under it, naming the point of sale.
    pub fn detail(self, pos: Option<PointOfSale>) -> String {
        let name = pos_text::name(pos);
        match self {
            ShowSales::Alert => format!(
                "A time-sensitive alert over {name}. Tap it and the code fills the screen; Return to {name} when it's paid."
            ),
            ShowSales::SideBySide => {
                format!("Till and {name} in two windows, arranged by hand.")
            }
        }
    }
}

/// One way a new sale can show up, as a radio row.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ShowSalesChoice {
    pub mode: ShowSales,
    pub choice: crate::view::Choice,
}

/// The ways a new sale can show up, `chosen` ticked.
pub fn show_sales_choices(chosen: ShowSales, pos: Option<PointOfSale>) -> Vec<ShowSalesChoice> {
    ShowSales::ALL
        .into_iter()
        .map(|mode| ShowSalesChoice {
            mode,
            choice: crate::view::Choice {
                label: mode.label().into(),
                detail: Some(mode.detail(pos)),
                selected: mode == chosen,
                enabled: true,
            },
        })
        .collect()
}

/// Side by side on iPadOS 26, under the choice once it's ticked.
pub const SIDE_BY_SIDE_HOW: &str = "Turn on Windowed Apps in Settings › Multitasking & Gestures. Then touch and hold the green button at the top left of Till's window and choose a layout beside Square.";

/// What the cashier chose about counters, cash and how a sale shows up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaleChoices {
    pub counters: Counters,
    /// The devices at the location by name, as this shift's sales named
    /// them ("Front iPad, Bar iPad, Kiosk").
    pub devices: Vec<String>,
    /// When the test sale named this counter's device.
    pub learned_at: Option<i64>,
    /// "Find this device again" is waiting for the test sale.
    pub finding_device: bool,
    pub cash_as_monero: bool,
    pub show_sales: ShowSales,
}

impl Default for SaleChoices {
    fn default() -> SaleChoices {
        SaleChoices {
            counters: Counters::default(),
            devices: Vec::new(),
            learned_at: None,
            finding_device: false,
            cash_as_monero: true,
            show_sales: ShowSales::Alert,
        }
    }
}

/// Settings › Alert for (board CounterSettings, C5).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CountersSection {
    /// "This device: Front iPad", "Learned from the test sale, 9 Nov";
    /// `None` until a test sale names it.
    pub this_device: Option<crate::view::Choice>,
    /// "Every device at Front Counter", naming the devices seen.
    pub every_device: crate::view::Choice,
    /// "Find this device again with a test sale", or "Waiting for the test
    /// sale…" while it waits (tap again to stop).
    pub find_again: Press,
    pub footnote: String,
    /// "Show their sales in the list".
    pub show_others: bool,
    pub show_others_footnote: String,
    /// "Alert for", over the device choice (core.md decision 130).
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub title: String,
    /// "Other counters", over [`Self::show_others`].
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub others_title: String,
    /// "Show their sales in the list".
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub show_others_label: String,
}

/// Settings › Cash in Square (board CashSettings, K4): one switch, on by
/// default, and a footer saying what it does either way (core.md decision
/// 182).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CashSection {
    /// Cash sales treated as Monero: each alerts and gets a code.
    pub on: bool,
    /// "Treat cash sales as Monero".
    pub label: String,
    /// What the switch does as it stands, and how Square records them.
    pub footnote: String,
    /// "Cash in Square".
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub title: String,
}

/// iOS's alerts, as a status row and a way to the system's settings: iOS
/// decides, so there's no switch that would snap back (review finding I15).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct AlertsStatus {
    /// "New sales and payments · On", or "Off" in warning.
    pub row: Fact,
    /// "Open Notification Settings".
    pub open_settings: String,
    pub footnote: String,
}

/// Settings › How sales show up (iPad).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ShowSalesSection {
    pub choices: Vec<ShowSalesChoice>,
    /// How to set up side by side, while it's chosen.
    pub side_by_side: Option<String>,
    /// "How sales show up".
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub title: String,
}

/// What "Watch for sales" says on iOS, where the app has no notification of
/// its own to keep.
pub const IOS_SHIFT: &str = "While it's on, Till watches for sales and the shift's totals run from when it started, or from 4 AM if that's later. Turn it off at the end of the day.";

/// Settings, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SettingsView {
    pub store: Option<StoreSection>,
    /// Android, while the store has an onion address.
    pub tor: Option<TorSection>,
    pub point_of_sale: PosSection,
    /// "Watch for sales": on every device (iOS too since the ledger,
    /// docs/ledger.md §9).
    pub shift: Option<SwitchSection>,
    pub alerts: SwitchSection,
    /// Android: "Float over other apps".
    pub bubble: Option<SwitchSection>,
    pub wake_ups: WakeUpsPlace,
    /// iPhone.
    pub island: Option<IslandSection>,
    /// Debug builds: a stand-in's address for each point of sale.
    pub stand_ins: Vec<StandIn>,
    /// Signed in to Square or Clover: which device's sales alert, and
    /// whether other counters' show in the list (C5; Clover's since core.md
    /// decision 142).
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub counters: Option<CountersSection>,
    /// Square, signed in: cash sales (K4).
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub cash: Option<CashSection>,
    /// iOS: alerts as iOS allows them, in place of [`Self::alerts`]'s
    /// switch.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub alerts_status: Option<AlertsStatus>,
    /// iPad: how a new sale shows up.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub show_sales: Option<ShowSalesSection>,
    /// The page's and its sections' titles, and Disconnect's words (core.md
    /// decision 130): "Settings", "Store", "Point of sale", "Notifications",
    /// "Wake-ups", "Developer (debug builds)".
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub title: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub store_title: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub pos_title: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub alerts_title: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub wake_ups_title: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub developer_title: String,
    /// "Disconnect this device".
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub disconnect: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub disconnect_footnote: String,
    /// The question before it: "Disconnect this device?", answered
    /// "Disconnect".
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub disconnect_ask: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub disconnect_confirm: String,
}

/// "9 Nov": a day on the device's clock.
fn day(at: i64, clock: DeviceClock) -> String {
    let local = at + i64::from(clock.utc_offset);
    let days = local.div_euclid(86_400);
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("{d} {}", MONTHS[(m - 1) as usize])
}

fn counters_section(
    c: &SaleChoices,
    pos: PointOfSale,
    place: Option<&str>,
    device: Device,
    clock: DeviceClock,
) -> CountersSection {
    let counters = &c.counters;
    let this = counters.device.as_ref();
    let name = this.map(|d| {
        d.name
            .clone()
            .unwrap_or_else(|| crate::counters::device_name(&d.id, &[], counters))
    });
    let at = place.unwrap_or("this location");
    CountersSection {
        this_device: name.map(|name| crate::view::Choice {
            label: format!("This device: {name}"),
            detail: c
                .learned_at
                .map(|t| format!("Learned from the test sale, {}", day(t, clock))),
            selected: !counters.every_device,
            enabled: true,
        }),
        every_device: crate::view::Choice {
            label: format!("Every device at {at}"),
            detail: (!c.devices.is_empty()).then(|| c.devices.join(", ")),
            selected: counters.every_device || this.is_none(),
            enabled: true,
        },
        find_again: Press::new(if c.finding_device {
            "Waiting for the test sale…".to_owned()
        } else if this.is_some() {
            "Find this device again with a test sale".to_owned()
        } else {
            "Find this device with a test sale".to_owned()
        }),
        footnote: if c.finding_device {
            match pos {
                PointOfSale::Square => format!(
                    "Ring up any small sale on {}'s Square and choose Monero. Tap again to stop waiting.",
                    pos_text::this_device(device)
                ),
                PointOfSale::Clover => "Ring up any small sale on the Clover device at this counter and choose Monero. Tap again to stop waiting.".into(),
            }
        } else if this.is_some() {
            format!(
                "{name} names the device that took each payment. Use \"Find again\" if this counter's {name} moves to another device.",
                name = pos.display_name()
            )
        } else {
            "The test sale was skipped, so every device's sales alert here. A test sale names this counter's device.".into()
        },
        show_others: counters.show_others,
        show_others_footnote:
            "They never alert here. Open one to show its code if a customer comes to this counter."
                .into(),
        title: "Alert for".into(),
        others_title: "Other counters".into(),
        show_others_label: "Show their sales in the list".into(),
    }
}

fn cash_section(c: &SaleChoices) -> CashSection {
    CashSection {
        on: c.cash_as_monero,
        label: "Treat cash sales as Monero".into(),
        footnote: if c.cash_as_monero {
            "Cash sales alert and show a code straight away. If the customer pays real cash, tap Paid in cash on the sale. Square still records them as Cash."
        } else {
            "Cash sales show quietly with Take it in Monero, and nothing happens unless you tap it. Square still records them as Cash."
        }
        .into(),
        title: "Cash in Square".into(),
    }
}

/// iOS's alerts row (board DesignSettings).
pub fn alerts_status(allowed: bool) -> AlertsStatus {
    AlertsStatus {
        row: if allowed {
            Fact::new("New sales and payments", "On")
        } else {
            Fact::new("New sales and payments", "Off").toned(Tone::Warn)
        },
        open_settings: "Open Notification Settings".into(),
        footnote:
            "iOS decides whether Till may alert you. Turn them on there and this updates by itself."
                .into(),
    }
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
    /// "Dynamic Island", over the switches "Live Activity" and "Show code
    /// in island", and its footnote (core.md decision 130).
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub title: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub live_activity_label: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub code_label: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub footnote: String,
    /// Asked before the code goes in the island: "Show the code in the
    /// island?", why, and "Show code".
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub ask_title: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub ask: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub ask_confirm: String,
}

impl IslandSection {
    /// The section with these switches, and its words.
    pub fn new(live_activity: bool, code_in_island: bool) -> IslandSection {
        IslandSection {
            live_activity,
            code_in_island,
            title: "Dynamic Island".into(),
            live_activity_label: "Live Activity".into(),
            code_label: "Show code in island".into(),
            footnote: "The island's code is the one Till shows, with what was bought, and it shows on the lock screen too. Live Activities end after 8 hours: start Till again each shift.".into(),
            ask_title: "Show the code in the island?".into(),
            ask: "Each waiting sale's code, with what was bought, will show in the Dynamic Island and on the lock screen, where anyone near this iPhone can see it.".into(),
            ask_confirm: "Show code".into(),
        }
    }
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
    // Square names the device that took each payment and has a cash
    // tender; Clover's payments are read by their Monero tender alone.
    let square_connected = pos == Some(PointOfSale::Square) && input.pos.connected();
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
        shift: Some(SwitchSection {
            on: input.shift_on,
            footnote: if android {
                wake_ups_text::shift(&input.wake_ups.status, pos)
            } else {
                IOS_SHIFT.to_owned()
            },
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
        island: (device == Device::Iphone)
            .then(|| IslandSection::new(input.live_activity, input.code_in_island)),
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
        counters: pos.filter(|_| input.pos.connected()).map(|pos| {
            let place = match pos {
                PointOfSale::Square => input.pos.location.as_ref().map(|l| l.name.as_str()),
                PointOfSale::Clover => input.pos.merchant.as_deref(),
            };
            counters_section(&input.choices, pos, place, device, clock)
        }),
        cash: square_connected.then(|| cash_section(&input.choices)),
        alerts_status: device.is_ios().then(|| alerts_status(input.alerts)),
        show_sales: (device == Device::Ipad).then(|| ShowSalesSection {
            choices: show_sales_choices(input.choices.show_sales, pos),
            side_by_side: (input.choices.show_sales == ShowSales::SideBySide)
                .then(|| SIDE_BY_SIDE_HOW.into()),
            title: "How sales show up".into(),
        }),
        title: "Settings".into(),
        store_title: "Store".into(),
        pos_title: pos_text::POINT_OF_SALE.into(),
        alerts_title: "Notifications".into(),
        wake_ups_title: "Wake-ups".into(),
        developer_title: "Developer (debug builds)".into(),
        disconnect: "Disconnect this device".into(),
        disconnect_footnote: DISCONNECT_FOOTNOTE.into(),
        disconnect_ask: "Disconnect this device?".into(),
        disconnect_confirm: "Disconnect".into(),
    }
}

/// Under Disconnect.
pub const DISCONNECT_FOOTNOTE: &str = "Removes Till from the store's Apps: its key stops working at once. Payments already made aren't affected.";

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
    let at_onion = tor::is_onion(connection.api.as_str());
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
                Tone::Plain
            } else {
                Tone::Muted
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_sales_is_kept_as_its_wire_word_and_reads_the_old_names_back() {
        for mode in ShowSales::ALL {
            assert_eq!(ShowSales::from_wire(mode.wire()), mode);
        }
        assert_eq!(ShowSales::SideBySide.wire(), Some("side"));
        assert_eq!(ShowSales::Alert.wire(), None);
        // The dropped floating window reads back as the alert.
        assert_eq!(ShowSales::from_wire(Some("pip")), ShowSales::Alert);
        // What the ledger's settings were given before (its Debug).
        assert_eq!(
            ShowSales::from_wire(Some("FloatingWindow")),
            ShowSales::Alert
        );
        assert_eq!(
            ShowSales::from_wire(Some("SideBySide")),
            ShowSales::SideBySide
        );
        assert_eq!(ShowSales::from_wire(Some("Alert")), ShowSales::Alert);
        assert_eq!(ShowSales::from_wire(Some("nonsense")), ShowSales::Alert);
    }
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
            choices: SaleChoices::default(),
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
    fn clover_settings_say_which_device_alerts_by_clovers_names_and_have_no_cash() {
        use crate::counters::{CounterDevice, Counters};
        let clover = SettingsInput {
            pos: PosState {
                chosen: Some(PointOfSale::Clover),
                signed_in: true,
                location: None,
                merchant: Some("The Bean Room".into()),
                tender_name: "Monero".into(),
                tender: CloverTender::Ready,
            },
            choices: SaleChoices {
                counters: Counters {
                    device: Some(CounterDevice {
                        id: "DEV_FRONT".into(),
                        name: None,
                    }),
                    names: [("DEV_FRONT".into(), "Front counter".to_owned())].into(),
                    ..Counters::default()
                },
                devices: vec!["Front counter".into()],
                learned_at: None,
                finding_device: false,
                cash_as_monero: true,
                show_sales: ShowSales::Alert,
            },
            ..input()
        };
        let view = settings(&clover, Device::Iphone, CLOCK);
        let counters = view
            .counters
            .expect("Clover's payments name their device too");
        assert_eq!(
            counters.this_device.unwrap().label,
            "This device: Front counter",
            "Clover's own name for it, not \"Device ONT\""
        );
        assert_eq!(counters.every_device.label, "Every device at The Bean Room");
        assert_eq!(
            counters.footnote,
            "Clover names the device that took each payment. Use \"Find again\" if this counter's Clover moves to another device."
        );
        assert_eq!(view.cash, None, "Clover's cash isn't read");
        let finding = SettingsInput {
            choices: SaleChoices {
                finding_device: true,
                ..clover.choices.clone()
            },
            ..clover
        };
        assert_eq!(
            settings(&finding, Device::Iphone, CLOCK)
                .counters
                .unwrap()
                .footnote,
            "Ring up any small sale on the Clover device at this counter and choose Monero. Tap again to stop waiting."
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
    fn an_iphone_has_a_wake_ups_row_the_dynamic_island_and_the_shift_and_no_bubble_or_tor() {
        let onion = SettingsInput {
            connection: Some(Connection {
                onion_api: Some("http://abc.onion".into()),
                ..Connection::new("key", "https://pay.yourshop.com")
            }),
            ..input()
        };
        let view = settings(&onion, Device::Iphone, CLOCK);
        assert_eq!(
            view.shift,
            Some(SwitchSection {
                on: true,
                footnote: IOS_SHIFT.into()
            }),
            "Stop and Start watching on iOS too (docs/ledger.md §9)"
        );
        assert_eq!(view.bubble, None);
        assert_eq!(view.tor, None, "no Orbot on iOS");
        assert_eq!(
            view.alerts.footnote,
            "Alerts are off. Till still shows new sales in the Dynamic Island, but won't make a sound or show a banner when you're in another app."
        );
        assert_eq!(
            view.wake_ups,
            WakeUpsPlace::Row {
                row: Fact::new("Wake-ups", "Connected").toned(Tone::Plain)
            }
        );
        let island = view.island.unwrap();
        assert!(island.live_activity && !island.code_in_island);
        // Decision 130: the words that were Swift's.
        assert_eq!(island.title, "Dynamic Island");
        assert_eq!(
            (island.ask_title.as_str(), island.ask_confirm.as_str()),
            ("Show the code in the island?", "Show code")
        );
        assert_eq!(view.title, "Settings");
        assert_eq!(view.disconnect_ask, "Disconnect this device?");

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

    #[test]
    fn square_settings_say_which_device_alerts_how_cash_is_taken_and_how_sales_show_up() {
        use crate::counters::{CounterDevice, Counters};
        let square = SettingsInput {
            choices: SaleChoices {
                counters: Counters {
                    device: Some(CounterDevice {
                        id: "dev_front".into(),
                        name: Some("Front iPad".into()),
                    }),
                    every_device: false,
                    show_others: true,
                    names: Default::default(),
                },
                devices: vec!["Front iPad".into(), "Bar iPad".into(), "Kiosk".into()],
                // 9 Nov 2026, 10:00 UTC.
                learned_at: Some(1_762_682_400),
                finding_device: false,
                cash_as_monero: true,
                show_sales: ShowSales::SideBySide,
            },
            alerts: true,
            ..input()
        };
        let view = settings(&square, Device::Ipad, CLOCK);
        let counters = view.counters.expect("Square names devices");
        let this = counters.this_device.unwrap();
        assert_eq!(this.label, "This device: Front iPad");
        assert_eq!(
            this.detail.as_deref(),
            Some("Learned from the test sale, 9 Nov")
        );
        assert!(this.selected);
        assert_eq!(counters.every_device.label, "Every device at Front Counter");
        assert_eq!(
            counters.every_device.detail.as_deref(),
            Some("Front iPad, Bar iPad, Kiosk")
        );
        assert!(!counters.every_device.selected);
        assert_eq!(
            counters.find_again.label,
            "Find this device again with a test sale"
        );
        assert!(counters.show_others);
        // Decision 130: the section titles and the switch's label.
        assert_eq!(
            (
                counters.title.as_str(),
                counters.others_title.as_str(),
                counters.show_others_label.as_str()
            ),
            (
                "Alert for",
                "Other counters",
                "Show their sales in the list"
            )
        );
        // Decision 182: one switch, on by default, with a footer.
        let cash = view.cash.expect("Square has a cash tender");
        assert!(cash.on);
        assert_eq!(cash.label, "Treat cash sales as Monero");
        assert_eq!(cash.title, "Cash in Square");
        assert!(
            cash.footnote.contains("tap Paid in cash"),
            "{}",
            cash.footnote
        );
        let offered = settings(
            &SettingsInput {
                choices: SaleChoices {
                    cash_as_monero: false,
                    ..square.choices.clone()
                },
                ..square.clone()
            },
            Device::Ipad,
            CLOCK,
        )
        .cash
        .unwrap();
        assert!(!offered.on);
        assert!(
            offered.footnote.contains("Take it in Monero"),
            "{}",
            offered.footnote
        );
        assert_eq!(view.alerts_title, "Notifications");
        assert_eq!(view.show_sales.as_ref().unwrap().title, "How sales show up");
        let alerts = view.alerts_status.expect("iOS's status row");
        assert_eq!(alerts.row, Fact::new("New sales and payments", "On"));
        assert_eq!(alerts.open_settings, "Open Notification Settings");
        let show = view.show_sales.expect("iPad");
        assert!(show.choices[1].choice.selected);
        assert!(show.side_by_side.is_some());

        let off = settings(
            &SettingsInput {
                alerts: false,
                ..square.clone()
            },
            Device::Iphone,
            CLOCK,
        );
        assert_eq!(off.alerts_status.unwrap().row.tone, Tone::Warn);
        assert_eq!(off.show_sales, None, "iPad only");
        let android = settings(&square, Device::Android, CLOCK);
        assert_eq!(android.alerts_status, None, "Android keeps its switch");
        assert!(android.counters.is_some());

        let skipped = SettingsInput {
            choices: SaleChoices {
                finding_device: true,
                ..SaleChoices::default()
            },
            ..input()
        };
        let counters = settings(&skipped, Device::Ipad, CLOCK).counters.unwrap();
        assert_eq!(counters.this_device, None);
        assert!(
            counters.every_device.selected,
            "the test skipped: every device"
        );
        assert_eq!(counters.find_again.label, "Waiting for the test sale…");
    }
}
