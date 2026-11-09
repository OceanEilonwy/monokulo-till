//! Settings › Wake-ups (acceptance A1, decision 99 of android.md; boards
//! PhoneWakeUps, Android's `WakeUpsSection` in `ui/Settings.kt`): the three
//! choices as one radio group, "My own Bell" with its address field, then,
//! with a Bell chosen, its status card, the status's footnote and what the
//! Bell learns. On Android it is a section of Settings; on iOS a screen of
//! its own, under Settings' Wake-ups row, with a note that its wake-ups are
//! alerts.
//!
//! The screen's headings ("Wake-ups", "Bell", "Last wake-up", "Bell
//! address", "Use this Bell", "Get ntfy") are the platform's: no state
//! chooses them.

use std::collections::BTreeMap;

use monokulo_client::PointOfSale;
use till_bell::{BellAddress, WakeUpChoice, WakeUpStatus};

use crate::pos_text;
use crate::time::DeviceClock;
use crate::view::{Choice, Device, Fact, PosMark, Press, Tone, mark};
use crate::wake_ups_text as text;

/// What the engine knows that Wake-ups shows.
#[derive(Clone, Debug)]
pub struct WakeUpsInput {
    pub choice: WakeUpChoice,
    pub status: WakeUpStatus,
    /// The project's Bell: the Monokulo Bell row's detail is its address.
    pub monokulo_bell: BellAddress,
    /// The last "My own Bell" address entered, kept through Timer only.
    pub own_bell: Option<BellAddress>,
    /// My own Bell tapped with no address saved yet: its field opens, but
    /// the tick and the status card stay with the saved choice until "Use
    /// this Bell" is pressed (decisions 103, 124).
    pub own_picked: bool,
    /// The address field as typed so far; `None` until it's touched, when
    /// it holds [`text::own_field`] of the saved address.
    pub typed: Option<String>,
    /// The till's point of sale; `None` while none is chosen.
    pub pos: Option<PointOfSale>,
    /// The installed UnifiedPush apps' names, by package (Android).
    pub distributor_names: BTreeMap<String, String>,
    /// A debug build: an http Bell is taken (decision 98).
    pub allow_http: bool,
    /// iOS: alerts are allowed for Till, which the Bell's wake-ups need.
    pub alerts_allowed: bool,
    /// What went wrong on the way back here, such as the point of sale's
    /// words on a failed sign-in through a Bell (decision 105).
    pub problem: Option<String>,
}

/// Wake-ups, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WakeUpsView {
    pub problem: Option<String>,
    /// "Timer only", "Monokulo Bell", "My own Bell", in that order.
    pub timer: Choice,
    pub monokulo_bell: Choice,
    pub own_bell: Choice,
    /// My own Bell's address field, under its row while it's picked.
    pub own_field: Option<OwnBellField>,
    /// The footnotes under the choices.
    pub notes: Vec<String>,
    /// With a Bell chosen: how it stands.
    pub status: Option<StatusCard>,
    /// iOS, with a Bell chosen: its wake-ups are alerts.
    pub alerts: Option<AlertsNote>,
}

/// My own Bell's address, prefilled with the saved one: "Use this Bell" only
/// for an address that differs and parses, and why one doesn't under the
/// field (acceptance A1.4–A1.5).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct OwnBellField {
    /// What the field starts with.
    pub initial: String,
    /// Why what's typed isn't a Bell's address.
    pub problem: Option<String>,
    pub use_this: Press,
}

/// A chosen Bell's status card.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct StatusCard {
    /// "Checking Square · Every 30 s".
    pub checking: Fact,
    /// "Bell · Connected", green while connected.
    pub bell: Fact,
    pub last_wake_up: Option<Fact>,
    pub action: Option<StatusAction>,
    pub footnote: String,
    /// What the Bell learns.
    pub privacy: String,
}

/// What the cashier can do about a status, as a row of its card.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum StatusAction {
    /// Android, no UnifiedPush app: "Get ntfy".
    GetDistributor,
    /// Android, several and no default: one row each, by name.
    ChooseDistributor { apps: Vec<Distributor> },
    /// iOS, alerts off: open Till's page in iOS Settings.
    OpenAlertSettings,
    /// The Bell hears the point of sale through another application: sign
    /// in through that one.
    SignIn {
        label: String,
        point_of_sale: PosMark,
        application_id: String,
    },
}

/// A UnifiedPush app to pick.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Distributor {
    pub package: String,
    pub name: String,
}

/// iOS: the Bell's wake-ups arrive as alerts.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct AlertsNote {
    /// "New sale alerts · Allowed", or "· Off".
    pub alerts: Fact,
    pub footnote: String,
}

pub const ALERTS_NOTE: &str = "iOS delays quiet wake-ups, so the Bell's are alerts. With alerts off in iOS Settings, Till stays on the timer.";

/// Wake-ups on `device`, at the device's `clock`.
pub fn wake_ups(input: &WakeUpsInput, device: Device, clock: DeviceClock) -> WakeUpsView {
    let pos = input.pos;
    let own_shown = input.own_picked || matches!(input.choice, WakeUpChoice::OwnBell { .. });
    let choice = |label: &str, selected: bool, detail: String| Choice {
        label: label.into(),
        detail: Some(detail),
        selected,
        enabled: true,
    };

    let mut notes = Vec::new();
    if input.own_picked {
        notes.push(text::NO_OWN_BELL_YET.to_owned());
    }
    let timer_only = input.choice == WakeUpChoice::TimerOnly;
    if timer_only && !input.own_picked {
        notes.push(text::timer_only(pos, device));
    }

    WakeUpsView {
        problem: input.problem.clone(),
        timer: choice("Timer only", timer_only, text::timer_detail(pos, device)),
        monokulo_bell: choice(
            "Monokulo Bell",
            input.choice == WakeUpChoice::MonokuloBell,
            text::bell(&input.monokulo_bell),
        ),
        own_bell: choice(
            "My own Bell",
            matches!(input.choice, WakeUpChoice::OwnBell { .. }),
            text::own_detail(input.own_bell.as_ref()),
        ),
        own_field: own_shown.then(|| own_field(input)),
        notes,
        status: (!timer_only).then(|| status_card(input, device, clock)),
        alerts: (device.is_ios() && !timer_only).then(|| AlertsNote {
            alerts: if input.alerts_allowed {
                Fact::new("New sale alerts", "Allowed")
            } else {
                Fact::new("New sale alerts", "Off").toned(Tone::Warn)
            },
            footnote: ALERTS_NOTE.into(),
        }),
    }
}

fn own_field(input: &WakeUpsInput) -> OwnBellField {
    let initial = text::own_field(input.own_bell.as_ref());
    let typed = input.typed.clone().unwrap_or_else(|| initial.clone());
    let parsed = BellAddress::parse(&typed, input.allow_http);
    let usable = parsed.is_some() && parsed.as_ref() != input.own_bell.as_ref();
    let problem = if typed.trim().is_empty() {
        None
    } else {
        BellAddress::problem(&typed, input.allow_http).map(str::to_owned)
    };
    OwnBellField {
        initial,
        problem,
        use_this: Press::new("Use this Bell").enabled(usable),
    }
}

fn status_card(input: &WakeUpsInput, device: Device, clock: DeviceClock) -> StatusCard {
    let status = &input.status;
    let pos = input.pos;
    let action = match status {
        WakeUpStatus::NoDistributor | WakeUpStatus::ChooseDistributor { .. } if device.is_ios() => {
            Some(StatusAction::OpenAlertSettings)
        }
        WakeUpStatus::NoDistributor => Some(StatusAction::GetDistributor),
        WakeUpStatus::ChooseDistributor { packages } => Some(StatusAction::ChooseDistributor {
            apps: packages
                .iter()
                .map(|package| Distributor {
                    package: package.clone(),
                    name: input
                        .distributor_names
                        .get(package)
                        .cloned()
                        .unwrap_or_else(|| package.clone()),
                })
                .collect(),
        }),
        WakeUpStatus::SignInNeeded {
            point_of_sale,
            application_id,
        } => Some(StatusAction::SignIn {
            label: pos_text::sign_in(Some(*point_of_sale)),
            point_of_sale: mark(*point_of_sale),
            application_id: application_id.clone(),
        }),
        _ => None,
    };
    let connected = matches!(status, WakeUpStatus::Connected { .. });
    StatusCard {
        checking: Fact::new(pos_text::checking(pos), text::pace(status)),
        bell: Fact::new("Bell", text::word(status, device)).toned(if connected {
            Tone::Plain
        } else {
            Tone::Muted
        }),
        last_wake_up: text::last_wake_up(status, clock).map(|at| Fact::new("Last wake-up", at)),
        action,
        footnote: text::footnote(status, pos, device),
        privacy: text::privacy(pos),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DING: i64 = 1_793_203_080;
    const CLOCK: DeviceClock = DeviceClock {
        now: DING + 60,
        utc_offset: 0,
        hours_24: false,
    };

    fn input() -> WakeUpsInput {
        WakeUpsInput {
            choice: WakeUpChoice::TimerOnly,
            status: WakeUpStatus::Off,
            monokulo_bell: BellAddress::parse("https://bell.monokulo.com", false).unwrap(),
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

    fn choice(label: &str, detail: &str, selected: bool) -> Choice {
        Choice {
            label: label.into(),
            detail: Some(detail.into()),
            selected,
            enabled: true,
        }
    }

    #[test]
    fn timer_only_shows_the_three_choices_and_says_what_a_bell_would_add() {
        let view = wake_ups(&input(), Device::Android, CLOCK);
        assert_eq!(
            view,
            WakeUpsView {
                problem: None,
                timer: choice("Timer only", "Checks Square every 2 seconds", true),
                monokulo_bell: choice("Monokulo Bell", "bell.monokulo.com", false),
                own_bell: choice("My own Bell", "Enter its address", false),
                own_field: None,
                notes: vec![
                    "Till checks Square every 2 seconds during a shift. A Bell lets Square wake the till within a second or two of a sale, so it can check every 30 seconds instead.".into()
                ],
                status: None,
                alerts: None,
            }
        );
    }

    #[test]
    fn a_connected_bell_shows_its_status_card_word_for_word() {
        let view = wake_ups(
            &WakeUpsInput {
                choice: WakeUpChoice::MonokuloBell,
                status: WakeUpStatus::Connected {
                    last_ding_at: Some(DING),
                },
                ..input()
            },
            Device::Android,
            CLOCK,
        );
        assert!(view.monokulo_bell.selected && !view.timer.selected);
        assert!(view.notes.is_empty());
        assert_eq!(
            view.status,
            Some(StatusCard {
                checking: Fact::new("Checking Square", "Every 30 s"),
                bell: Fact::new("Bell", "Connected").toned(Tone::Plain),
                last_wake_up: Some(Fact::new("Last wake-up", "3:58 PM")),
                action: None,
                footnote: "Square wakes this till through the Bell within a second or two of a sale. Till still checks every 30 seconds in case a wake-up is lost.".into(),
                privacy: "The Bell learns which Square location this till is at and when it takes payments, not what was sold. Your Square sign-in is shown to it once, to prove the location, and never kept.".into(),
            })
        );
        assert_eq!(view.alerts, None, "Android has no alerts note");
    }

    #[test]
    fn my_own_bell_picked_with_no_address_opens_its_field_and_keeps_the_saved_choice() {
        let view = wake_ups(
            &WakeUpsInput {
                own_picked: true,
                ..input()
            },
            Device::Android,
            CLOCK,
        );
        assert!(view.timer.selected && !view.own_bell.selected);
        assert_eq!(
            view.notes,
            [
                "Enter the address of the Bell you run, then press Use this Bell. Until then Wake-ups stay as they were."
            ]
        );
        assert_eq!(
            view.own_field,
            Some(OwnBellField {
                initial: String::new(),
                problem: None,
                use_this: Press::new("Use this Bell").enabled(false),
            })
        );

        let typed = |text: &str| {
            wake_ups(
                &WakeUpsInput {
                    own_picked: true,
                    typed: Some(text.into()),
                    ..input()
                },
                Device::Android,
                CLOCK,
            )
            .own_field
            .unwrap()
        };
        assert!(typed("bell.thebeanroom.coffee").use_this.enabled);
        let http = typed("http://bell.thebeanroom.coffee");
        assert!(!http.use_this.enabled);
        assert_eq!(
            http.problem.as_deref(),
            BellAddress::problem("http://bell.thebeanroom.coffee", false)
        );
        assert!(http.problem.is_some());
    }

    #[test]
    fn the_saved_own_bell_fills_its_field_and_using_it_again_is_offered_only_for_another() {
        let saved = BellAddress::parse("bell.thebeanroom.coffee", false).unwrap();
        let own = WakeUpsInput {
            choice: WakeUpChoice::OwnBell {
                bell: saved.clone(),
            },
            status: WakeUpStatus::Connecting,
            own_bell: Some(saved),
            ..input()
        };
        let view = wake_ups(&own, Device::Android, CLOCK);
        assert!(view.own_bell.selected);
        assert_eq!(
            view.own_bell.detail.as_deref(),
            Some("bell.thebeanroom.coffee")
        );
        let field = view.own_field.unwrap();
        assert_eq!(field.initial, "bell.thebeanroom.coffee");
        assert!(!field.use_this.enabled, "the same address again");
        let status = view.status.unwrap();
        assert_eq!(
            status.bell,
            Fact::new("Bell", "Connecting…").toned(Tone::Muted)
        );
        assert_eq!(status.last_wake_up, None);
        assert_eq!(status.footnote, "Connecting to the Bell…");
    }

    #[test]
    fn each_status_offers_what_can_be_done_about_it() {
        let action = |status: WakeUpStatus, device: Device| {
            let mut names = BTreeMap::new();
            names.insert("io.heckel.ntfy".to_owned(), "ntfy".to_owned());
            wake_ups(
                &WakeUpsInput {
                    choice: WakeUpChoice::MonokuloBell,
                    status,
                    distributor_names: names,
                    ..input()
                },
                device,
                CLOCK,
            )
            .status
            .unwrap()
            .action
        };
        assert_eq!(
            action(WakeUpStatus::NoDistributor, Device::Android),
            Some(StatusAction::GetDistributor)
        );
        assert_eq!(
            action(
                WakeUpStatus::ChooseDistributor {
                    packages: vec!["io.heckel.ntfy".into(), "org.example.up".into()]
                },
                Device::Android
            ),
            Some(StatusAction::ChooseDistributor {
                apps: vec![
                    Distributor {
                        package: "io.heckel.ntfy".into(),
                        name: "ntfy".into()
                    },
                    Distributor {
                        package: "org.example.up".into(),
                        name: "org.example.up".into()
                    },
                ]
            })
        );
        assert_eq!(
            action(
                WakeUpStatus::SignInNeeded {
                    point_of_sale: PointOfSale::Clover,
                    application_id: "OTHERCLOVERAP".into()
                },
                Device::Iphone
            ),
            Some(StatusAction::SignIn {
                label: "Sign in to Clover".into(),
                point_of_sale: PosMark::Clover,
                application_id: "OTHERCLOVERAP".into(),
            })
        );
        assert_eq!(
            action(WakeUpStatus::NoDistributor, Device::Iphone),
            Some(StatusAction::OpenAlertSettings)
        );
        assert_eq!(action(WakeUpStatus::Connecting, Device::Ipad), None);
    }

    #[test]
    fn on_an_iphone_the_timer_runs_while_till_is_open_and_the_bells_wake_ups_are_alerts() {
        let timer = wake_ups(&input(), Device::Iphone, CLOCK);
        assert_eq!(
            timer.timer.detail.as_deref(),
            Some("Checks Square every 2 seconds, only while Till is open")
        );
        assert_eq!(
            timer.notes,
            [
                "Till checks Square every 2 seconds while it's open. A Bell lets Square wake the till within a second or two of a sale, so it can check every 30 seconds instead, and brings in sales while Till is in the background."
            ]
        );
        assert_eq!(timer.alerts, None, "the timer needs no alerts");

        let off = wake_ups(
            &WakeUpsInput {
                choice: WakeUpChoice::MonokuloBell,
                status: WakeUpStatus::NoDistributor,
                alerts_allowed: false,
                ..input()
            },
            Device::Iphone,
            CLOCK,
        );
        assert_eq!(
            off.alerts,
            Some(AlertsNote {
                alerts: Fact::new("New sale alerts", "Off").toned(Tone::Warn),
                footnote: "iOS delays quiet wake-ups, so the Bell's are alerts. With alerts off in iOS Settings, Till stays on the timer.".into(),
            })
        );
        let status = off.status.unwrap();
        assert_eq!(
            status.bell,
            Fact::new("Bell", "Alerts are off").toned(Tone::Muted)
        );
        assert_eq!(
            status.footnote,
            "The Bell's wake-ups arrive as alerts, and alerts are off for Till in iOS Settings. Turn them on there, then come back here. Until then Till checks every 2 seconds."
        );

        let allowed = wake_ups(
            &WakeUpsInput {
                choice: WakeUpChoice::MonokuloBell,
                status: WakeUpStatus::Connected { last_ding_at: None },
                ..input()
            },
            Device::Ipad,
            CLOCK,
        );
        assert_eq!(
            allowed.alerts.unwrap().alerts,
            Fact::new("New sale alerts", "Allowed")
        );
        assert_eq!(
            allowed.timer.detail.as_deref(),
            Some("Checks Square every 2 seconds"),
            "an iPad in Slide Over stays open"
        );
    }
}
