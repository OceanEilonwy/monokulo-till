//! Settings › Wake-ups in the cashier's words (acceptance A1.6–A1.8;
//! Android: `WakeUpsText.kt`): every status's word and footnote from one
//! exhaustive `match` each, so a status added later doesn't compile until it
//! has words (D15), naming the till's point of sale [`pos_text`]'s way
//! (acceptance T6.1; `None`: none chosen, "your point of sale").
//!
//! Where iOS differs it is told by the [`Device`]: an iPhone checks only
//! while Till is open, and an iOS device has no UnifiedPush. Its one
//! distributor is APNs, whose wake-ups arrive as alerts, so the statuses
//! that ask for a UnifiedPush app ask for alerts there.

use monokulo_client::PointOfSale;
use till_bell::{BellAddress, WakeUpStatus};
use till_types::UnixSeconds;

use crate::pos_text::{self, name, name_first};
use crate::time::{self, DeviceClock};
use crate::view::Device;

/// The Timer only row's detail.
pub fn timer_detail(pos: Option<PointOfSale>, device: Device) -> String {
    match device {
        Device::Iphone => format!(
            "Checks {} every 2 seconds, only while Till is open",
            name(pos)
        ),
        Device::Android | Device::Ipad => format!("Checks {} every 2 seconds", name(pos)),
    }
}

/// The My own Bell row's detail when no address is saved.
pub const NO_OWN_BELL: &str = "Enter its address";

/// Under the choices while My own Bell is picked and no address is saved
/// yet: nothing has changed until one is used.
pub const NO_OWN_BELL_YET: &str = "Enter the address of the Bell you run, then press Use this Bell. Until then Wake-ups stay as they were.";

/// Under every Bell's status: what the Bell learns, and what it doesn't.
pub fn privacy(pos: Option<PointOfSale>) -> String {
    format!(
        "The Bell learns which {} this till is at and when it takes payments, not what was sold. Your {} is shown to it once, to prove the {}, and never kept.",
        pos_text::place_at(pos),
        pos_text::sign_in_of(pos),
        pos_text::place(pos)
    )
}

/// Under the choices while Timer only is chosen. An iPhone checks only
/// while Till is open, and a Bell is what brings sales in behind it.
pub fn timer_only(pos: Option<PointOfSale>, device: Device) -> String {
    let name = name(pos);
    match device {
        Device::Iphone => format!(
            "Till checks {name} every 2 seconds while it's open. A Bell lets {name} wake the till within a second or two of a sale, so it can check every 30 seconds instead, and brings in sales while Till is in the background."
        ),
        Device::Android | Device::Ipad => format!(
            "Till checks {name} every 2 seconds during a shift. A Bell lets {name} wake the till within a second or two of a sale, so it can check every 30 seconds instead."
        ),
    }
}

/// A Bell's row detail: its address without its scheme, with the scheme
/// only when that isn't https (a debug build's http Bell), so two Bells
/// that differ in nothing else can be told apart (decision 124).
pub fn bell(address: &BellAddress) -> String {
    let display = address.display();
    if address.url() == format!("https://{display}") {
        display
    } else {
        address.url().to_owned()
    }
}

/// The My own Bell row's detail: the saved address, or how to start.
pub fn own_detail(saved: Option<&BellAddress>) -> String {
    saved.map_or_else(|| NO_OWN_BELL.to_owned(), bell)
}

/// What the address field starts with: the saved address as its row shows
/// it, so it reads back as the same address.
pub fn own_field(saved: Option<&BellAddress>) -> String {
    saved.map(bell).unwrap_or_default()
}

/// The status card's Checking row ([`pos_text::checking`]): the pace the
/// shift's timer runs at, on one line at any font scale.
pub fn pace(status: &WakeUpStatus) -> String {
    format!("Every {} s", status.interval().as_secs())
}

/// The status card's Bell row.
pub fn word(status: &WakeUpStatus, device: Device) -> &'static str {
    match status {
        WakeUpStatus::Off => "Off",
        WakeUpStatus::NoPos => "Sign in needed",
        WakeUpStatus::NoDistributor | WakeUpStatus::ChooseDistributor { .. } if device.is_ios() => {
            "Alerts are off"
        }
        WakeUpStatus::NoDistributor => "Needs a UnifiedPush app",
        WakeUpStatus::ChooseDistributor { .. } => "Choose a UnifiedPush app",
        WakeUpStatus::Connecting => "Connecting…",
        WakeUpStatus::Connected { .. } => "Connected",
        WakeUpStatus::MissingWakeups { .. } => "Wake-ups not arriving",
        WakeUpStatus::Unreachable { .. } => "Not connected",
        WakeUpStatus::Refused { .. } => "Not connected",
        WakeUpStatus::SignInNeeded { .. } => "Sign in needed",
    }
}

/// The status card's Last wake-up row, written as the shift writes times;
/// `None` while the status has no ding clock (the row is left out).
pub fn last_wake_up(status: &WakeUpStatus, clock: DeviceClock) -> Option<String> {
    let at = match status {
        WakeUpStatus::Connected { last_ding_at } => last_ding_at,
        WakeUpStatus::MissingWakeups { last_ding_at } => last_ding_at,
        _ => return None,
    };
    Some(at.map_or_else(
        || "None yet".to_owned(),
        |at| time::time(UnixSeconds(at), clock),
    ))
}

/// The footnote under the cards, for a till at `pos`.
pub fn footnote(status: &WakeUpStatus, pos: Option<PointOfSale>, device: Device) -> String {
    match status {
        WakeUpStatus::Off => timer_only(pos, device),
        WakeUpStatus::NoPos => format!(
            "{} to use the Bell. Until then Till shows other tills' sales only.",
            pos_text::sign_in(pos)
        ),
        WakeUpStatus::NoDistributor | WakeUpStatus::ChooseDistributor { .. }
            if device.is_ios() =>
        {
            "The Bell's wake-ups arrive as alerts, and alerts are off for Till in iOS Settings. Turn them on there, then come back here. Until then Till checks every 2 seconds.".into()
        }
        WakeUpStatus::NoDistributor => "Wake-ups need a UnifiedPush app such as ntfy to reach this till. Install one, then come back here. Until then Till checks every 2 seconds.".into(),
        WakeUpStatus::ChooseDistributor { .. } => "Deliver wake-ups through one of the UnifiedPush apps above. Until you choose, Till checks every 2 seconds.".into(),
        WakeUpStatus::Connecting => "Connecting to the Bell…".into(),
        WakeUpStatus::Connected { .. } => format!(
            "{} wakes this till through the Bell within a second or two of a sale. Till still checks every 30 seconds in case a wake-up is lost.",
            name_first(pos)
        ),
        WakeUpStatus::MissingWakeups { .. } => format!(
            "A sale came in without a wake-up from the Bell. Till is checking {} every 2 seconds until the Bell's wake-ups come back.",
            name(pos)
        ),
        WakeUpStatus::Unreachable { reason } => format!(
            "Till couldn't reach the Bell ({reason}). It's checking {} every 2 seconds and will try the Bell again.",
            name(pos)
        ),
        WakeUpStatus::Refused { reason, .. } => format!(
            "The Bell refused this till: {} Till is checking {} every 2 seconds.",
            sentence(reason),
            name(pos)
        ),
        WakeUpStatus::SignInNeeded { point_of_sale, .. } => format!(
            "This Bell uses a different {} application. {} through it to use the Bell.",
            point_of_sale.display_name(),
            pos_text::sign_in(Some(*point_of_sale))
        ),
    }
}

/// The Shift section's footnote, by the pace, for a till at `pos`.
pub fn shift(status: &WakeUpStatus, pos: Option<PointOfSale>) -> String {
    match status {
        WakeUpStatus::Connected { .. } => format!(
            "While it's on, the Bell wakes Till when a sale comes in and it checks {} every 30 seconds between wake-ups. Turn it off at the end of the day.",
            name(pos)
        ),
        WakeUpStatus::Off
        | WakeUpStatus::NoPos
        | WakeUpStatus::NoDistributor
        | WakeUpStatus::ChooseDistributor { .. }
        | WakeUpStatus::Connecting
        | WakeUpStatus::MissingWakeups { .. }
        | WakeUpStatus::Unreachable { .. }
        | WakeUpStatus::Refused { .. }
        | WakeUpStatus::SignInNeeded { .. } => format!(
            "While it's on, Till checks {} every 2 seconds and keeps a notification. Turn it off at the end of the day.",
            name(pos)
        ),
    }
}

/// `reason` ending in one full stop: some refusals are sentences of their
/// own ("… Sign in to Clover again.").
fn sentence(reason: &str) -> String {
    if reason.ends_with('.') {
        reason.to_owned()
    } else {
        format!("{reason}.")
    }
}

/// Ported from Android's `WakeUpsTextTest`, test for test, with "Monokular"
/// now "Till" (acceptance A1.6–A1.8 exactly, D15, for each point of sale,
/// T6.1); then what iOS says differently.
#[cfg(test)]
mod tests {
    use super::*;
    use WakeUpStatus as S;

    const SQUARE: Option<PointOfSale> = Some(PointOfSale::Square);
    const CLOVER: Option<PointOfSale> = Some(PointOfSale::Clover);
    const ANDROID: Device = Device::Android;
    /// 2026-10-28T15:58:00Z.
    const DING: i64 = 1_793_203_080;
    /// Locale.US in UTC, as the Kotlin test wrote times.
    const US_UTC: DeviceClock = DeviceClock {
        now: DING,
        utc_offset: 0,
        hours_24: false,
    };

    fn parse(text: &str) -> Option<BellAddress> {
        BellAddress::parse(text, false)
    }

    fn parse_http(text: &str) -> Option<BellAddress> {
        BellAddress::parse(text, true)
    }

    /// One of every status.
    fn every() -> Vec<WakeUpStatus> {
        vec![
            S::Off,
            S::NoPos,
            S::NoDistributor,
            S::ChooseDistributor {
                packages: vec![
                    "io.heckel.ntfy".into(),
                    "org.unifiedpush.distributor.sunup".into(),
                ],
            },
            S::Connecting,
            S::Connected { last_ding_at: None },
            S::MissingWakeups {
                last_ding_at: Some(DING),
            },
            S::Unreachable {
                reason: "no connection".into(),
            },
            S::Refused {
                reason: "this location already has 32 tills registered".into(),
                retry_at: None,
            },
            S::SignInNeeded {
                point_of_sale: PointOfSale::Square,
                application_id: "sandbox-sq0idb-other".into(),
            },
        ]
    }

    fn footnotes(statuses: &[WakeUpStatus], pos: Option<PointOfSale>) -> Vec<String> {
        statuses.iter().map(|s| footnote(s, pos, ANDROID)).collect()
    }

    #[test]
    fn the_bell_row_says_each_status_in_a_word_or_two() {
        let words: Vec<_> = every().iter().map(|s| word(s, ANDROID)).collect();
        assert_eq!(
            words,
            [
                "Off",
                "Sign in needed",
                "Needs a UnifiedPush app",
                "Choose a UnifiedPush app",
                "Connecting…",
                "Connected",
                "Wake-ups not arriving",
                "Not connected",
                "Not connected",
                "Sign in needed",
            ]
        );
    }

    #[test]
    fn checking_says_30_s_while_connected_and_2_s_otherwise_the_unit_on_the_numbers_line() {
        let two = "Every 2 s";
        let paces: Vec<_> = every().iter().map(pace).collect();
        assert_eq!(
            paces,
            [two, two, two, two, two, "Every 30 s", two, two, two, two]
        );
    }

    #[test]
    fn last_wake_up_shows_only_with_a_ding_clock_as_the_shift_writes_times_or_none_yet() {
        assert_eq!(
            last_wake_up(&S::Connected { last_ding_at: None }, US_UTC).as_deref(),
            Some("None yet")
        );
        assert_eq!(
            last_wake_up(
                &S::Connected {
                    last_ding_at: Some(DING)
                },
                US_UTC
            )
            .as_deref(),
            Some("3:58 PM")
        );
        assert_eq!(
            last_wake_up(
                &S::MissingWakeups {
                    last_ding_at: Some(DING)
                },
                US_UTC
            )
            .as_deref(),
            Some("3:58 PM")
        );
        assert_eq!(
            last_wake_up(
                &S::Connected {
                    last_ding_at: Some(DING)
                },
                US_UTC
            ),
            Some(time::time(UnixSeconds(DING), US_UTC))
        );
        for status in every() {
            if !matches!(status, S::Connected { .. } | S::MissingWakeups { .. }) {
                assert_eq!(
                    last_wake_up(&status, US_UTC),
                    None,
                    "{status:?} has no Last wake-up row"
                );
            }
        }
    }

    #[test]
    fn every_status_has_its_footnote_word_for_word_at_square() {
        assert_eq!(
            footnotes(&every(), SQUARE),
            [
                "Till checks Square every 2 seconds during a shift. A Bell lets Square wake the till within a second or two of a sale, so it can check every 30 seconds instead.",
                "Sign in to Square to use the Bell. Until then Till shows other tills' sales only.",
                "Wake-ups need a UnifiedPush app such as ntfy to reach this till. Install one, then come back here. Until then Till checks every 2 seconds.",
                "Deliver wake-ups through one of the UnifiedPush apps above. Until you choose, Till checks every 2 seconds.",
                "Connecting to the Bell…",
                "Square wakes this till through the Bell within a second or two of a sale. Till still checks every 30 seconds in case a wake-up is lost.",
                "A sale came in without a wake-up from the Bell. Till is checking Square every 2 seconds until the Bell's wake-ups come back.",
                "Till couldn't reach the Bell (no connection). It's checking Square every 2 seconds and will try the Bell again.",
                "The Bell refused this till: this location already has 32 tills registered. Till is checking Square every 2 seconds.",
                "This Bell uses a different Square application. Sign in to Square through it to use the Bell.",
            ]
        );
    }

    #[test]
    fn every_status_has_its_footnote_at_clover_naming_clover_where_squares_names_square() {
        let clover: Vec<_> = every()
            .into_iter()
            .map(|s| match s {
                S::SignInNeeded { .. } => S::SignInNeeded {
                    point_of_sale: PointOfSale::Clover,
                    application_id: "OTHERCLOVERAP".into(),
                },
                s => s,
            })
            .collect();
        assert_eq!(
            footnotes(&clover, CLOVER),
            [
                "Till checks Clover every 2 seconds during a shift. A Bell lets Clover wake the till within a second or two of a sale, so it can check every 30 seconds instead.",
                "Sign in to Clover to use the Bell. Until then Till shows other tills' sales only.",
                "Wake-ups need a UnifiedPush app such as ntfy to reach this till. Install one, then come back here. Until then Till checks every 2 seconds.",
                "Deliver wake-ups through one of the UnifiedPush apps above. Until you choose, Till checks every 2 seconds.",
                "Connecting to the Bell…",
                "Clover wakes this till through the Bell within a second or two of a sale. Till still checks every 30 seconds in case a wake-up is lost.",
                "A sale came in without a wake-up from the Bell. Till is checking Clover every 2 seconds until the Bell's wake-ups come back.",
                "Till couldn't reach the Bell (no connection). It's checking Clover every 2 seconds and will try the Bell again.",
                "The Bell refused this till: this location already has 32 tills registered. Till is checking Clover every 2 seconds.",
                "This Bell uses a different Clover application. Sign in to Clover through it to use the Bell.",
            ]
        );
        assert_eq!(
            footnote(
                &S::Refused {
                    reason: "it doesn't ring Clover tills".into(),
                    retry_at: None
                },
                CLOVER,
                ANDROID
            ),
            "The Bell refused this till: it doesn't ring Clover tills. Till is checking Clover every 2 seconds."
        );
    }

    #[test]
    fn with_no_point_of_sale_chosen_the_words_say_your_point_of_sale() {
        assert_eq!(
            footnote(&S::NoPos, None, ANDROID),
            "Sign in to your point of sale to use the Bell. Until then Till shows other tills' sales only."
        );
        assert_eq!(
            footnote(&S::Connected { last_ding_at: None }, None, ANDROID),
            "Your point of sale wakes this till through the Bell within a second or two of a sale. Till still checks every 30 seconds in case a wake-up is lost."
        );
        assert_eq!(
            timer_detail(None, ANDROID),
            "Checks your point of sale every 2 seconds"
        );
        assert_eq!(
            privacy(None),
            "The Bell learns which location this till is at and when it takes payments, not what was sold. Your point-of-sale sign-in is shown to it once, to prove the location, and never kept."
        );
    }

    #[test]
    fn every_reason_reads_in_its_sentence_and_one_that_is_a_sentence_keeps_one_full_stop() {
        let unreachable: Vec<_> = ["it didn't answer", "it answered 503", "it isn't a Bell"]
            .into_iter()
            .map(|reason| {
                footnote(
                    &S::Unreachable {
                        reason: reason.into(),
                    },
                    SQUARE,
                    ANDROID,
                )
            })
            .collect();
        assert_eq!(
            unreachable,
            [
                "Till couldn't reach the Bell (it didn't answer). It's checking Square every 2 seconds and will try the Bell again.",
                "Till couldn't reach the Bell (it answered 503). It's checking Square every 2 seconds and will try the Bell again.",
                "Till couldn't reach the Bell (it isn't a Bell). It's checking Square every 2 seconds and will try the Bell again.",
            ]
        );
        assert_eq!(
            footnote(
                &S::Refused {
                    reason: "Square didn't confirm this location. Sign in to Square again.".into(),
                    retry_at: None
                },
                SQUARE,
                ANDROID
            ),
            "The Bell refused this till: Square didn't confirm this location. Sign in to Square again. Till is checking Square every 2 seconds."
        );
        assert_eq!(
            footnote(
                &S::Refused {
                    reason:
                        "too many registrations from this network: trying again in about a minute"
                            .into(),
                    retry_at: Some(1)
                },
                SQUARE,
                ANDROID
            ),
            "The Bell refused this till: too many registrations from this network: trying again in about a minute. Till is checking Square every 2 seconds."
        );
        assert_eq!(
            footnote(
                &S::Unreachable {
                    reason: "the UnifiedPush app didn't answer: open it once".into()
                },
                SQUARE,
                ANDROID
            ),
            "Till couldn't reach the Bell (the UnifiedPush app didn't answer: open it once). It's checking Square every 2 seconds and will try the Bell again."
        );
    }

    #[test]
    fn the_shift_footnote_follows_the_pace() {
        let timer = "While it's on, Till checks Square every 2 seconds and keeps a notification. Turn it off at the end of the day.";
        let bell = "While it's on, the Bell wakes Till when a sale comes in and it checks Square every 30 seconds between wake-ups. Turn it off at the end of the day.";
        let expected: Vec<_> = every()
            .iter()
            .map(|s| {
                if matches!(s, S::Connected { .. }) {
                    bell
                } else {
                    timer
                }
            })
            .collect();
        let shifts: Vec<_> = every().iter().map(|s| shift(s, SQUARE)).collect();
        assert_eq!(shifts, expected);
        assert_eq!(
            shift(&S::Off, CLOVER),
            "While it's on, Till checks Clover every 2 seconds and keeps a notification. Turn it off at the end of the day."
        );
        assert_eq!(
            shift(&S::Connected { last_ding_at: None }, CLOVER),
            "While it's on, the Bell wakes Till when a sale comes in and it checks Clover every 30 seconds between wake-ups. Turn it off at the end of the day."
        );
    }

    #[test]
    fn the_choice_rows_details_and_the_privacy_footnote() {
        assert_eq!(
            timer_detail(SQUARE, ANDROID),
            "Checks Square every 2 seconds"
        );
        assert_eq!(
            timer_detail(CLOVER, ANDROID),
            "Checks Clover every 2 seconds"
        );
        assert_eq!(own_detail(None), "Enter its address");
        assert_eq!(
            NO_OWN_BELL_YET,
            "Enter the address of the Bell you run, then press Use this Bell. Until then Wake-ups stay as they were."
        );
        assert_eq!(
            own_detail(parse("bell.thebeanroom.coffee").as_ref()),
            "bell.thebeanroom.coffee"
        );
        assert_eq!(
            own_detail(parse("https://bell.example.org/bell/").as_ref()),
            "bell.example.org/bell"
        );
        // A debug build's http Bell shows its scheme, so it isn't mistaken
        // for the https one of the same host (decision 124).
        assert_eq!(
            own_detail(parse_http("http://10.0.2.2:8445").as_ref()),
            "http://10.0.2.2:8445"
        );
        assert_eq!(
            bell(&parse_http("http://10.0.2.2:8445").unwrap()),
            "http://10.0.2.2:8445"
        );
        assert_eq!(
            bell(&parse("https://bell.monokulo.com").unwrap()),
            "bell.monokulo.com"
        );
        assert_eq!(
            privacy(SQUARE),
            "The Bell learns which Square location this till is at and when it takes payments, not what was sold. Your Square sign-in is shown to it once, to prove the location, and never kept."
        );
        assert_eq!(
            privacy(CLOVER),
            "The Bell learns which Clover merchant this till is at and when it takes payments, not what was sold. Your Clover sign-in is shown to it once, to prove the merchant, and never kept."
        );
    }

    #[test]
    fn the_address_field_starts_with_the_saved_address_its_scheme_only_when_it_isnt_https() {
        assert_eq!(own_field(None), "");
        let https = parse("bell.example.org/bell");
        assert_eq!(own_field(https.as_ref()), "bell.example.org/bell");
        assert_eq!(
            parse(&own_field(https.as_ref())),
            https,
            "reads back as the same address"
        );
        let http = parse_http("http://10.0.2.2:8445");
        assert_eq!(own_field(http.as_ref()), "http://10.0.2.2:8445");
        assert_eq!(parse_http(&own_field(http.as_ref())), http);
    }

    // iOS.

    #[test]
    fn an_iphone_checks_only_while_till_is_open_and_a_bell_brings_sales_in_behind_it() {
        assert_eq!(
            timer_detail(SQUARE, Device::Iphone),
            "Checks Square every 2 seconds, only while Till is open"
        );
        assert_eq!(
            timer_only(SQUARE, Device::Iphone),
            "Till checks Square every 2 seconds while it's open. A Bell lets Square wake the till within a second or two of a sale, so it can check every 30 seconds instead, and brings in sales while Till is in the background."
        );
        assert_eq!(
            footnote(&S::Off, CLOVER, Device::Iphone),
            timer_only(CLOVER, Device::Iphone)
        );
        // An iPad in Slide Over stays open beside the point of sale: Android's words.
        assert_eq!(
            timer_detail(SQUARE, Device::Ipad),
            timer_detail(SQUARE, ANDROID)
        );
        assert_eq!(
            timer_only(SQUARE, Device::Ipad),
            timer_only(SQUARE, ANDROID)
        );
    }

    #[test]
    fn on_ios_wake_ups_arrive_as_alerts_so_no_status_asks_for_a_unifiedpush_app() {
        for device in [Device::Iphone, Device::Ipad] {
            for status in every() {
                assert!(
                    !word(&status, device).contains("UnifiedPush"),
                    "{status:?} on {device:?}"
                );
                if !matches!(status, S::Unreachable { .. }) {
                    // An unreachable Bell's reason is the Bell crate's.
                    assert!(
                        !footnote(&status, SQUARE, device).contains("UnifiedPush"),
                        "{status:?} on {device:?}"
                    );
                }
            }
            assert_eq!(word(&S::NoDistributor, device), "Alerts are off");
            assert_eq!(
                footnote(&S::NoDistributor, SQUARE, device),
                "The Bell's wake-ups arrive as alerts, and alerts are off for Till in iOS Settings. Turn them on there, then come back here. Until then Till checks every 2 seconds."
            );
            // The rest read as Android's.
            for status in every() {
                if !matches!(
                    status,
                    S::Off | S::NoDistributor | S::ChooseDistributor { .. }
                ) {
                    assert_eq!(word(&status, device), word(&status, ANDROID));
                    assert_eq!(
                        footnote(&status, SQUARE, device),
                        footnote(&status, SQUARE, ANDROID)
                    );
                }
            }
        }
    }
}
