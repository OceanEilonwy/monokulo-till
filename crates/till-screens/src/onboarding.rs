//! Setting up a till, step by step (boards Onb*, PadOnb*, PhonePos;
//! Android's `ui/Onboarding.kt`): welcome, connecting a store and checking
//! it, the point of sale and its sign-in with the test sale, Square's
//! locations, alerts, Android's bubble, and ready.
//!
//! What's here is what state chooses: the step's number, iPad's step rail,
//! the buttons and whether they can be pressed, the store's facts, the
//! point of sale's card, and every sentence that names the point of sale.
//! The headings and leads written once ("Connect a store", "Hear every new
//! sale", the dashboard's steps) stay in the platform's screens.

use monokulo_client::{MoneyFormat, PointOfSale, amounts};
use till_pos::square::SquareLocation;

use crate::pos_text;
use crate::settings::{StoreNow, connected, pos_choices};
use crate::view::{Device, Fact, PosChoice, PosMark, Press, capitalised, mark, on_off};

/// How many steps setting up takes, on every device.
pub const STEPS: u8 = 6;

/// Where Clover's Monero payment type stands, as step 4's Clover card and
/// Settings show it (decision 129).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CloverTender {
    /// Not known yet: signing in, or looking.
    Looking,
    /// Found or added: the cashier rings sales up with it.
    #[default]
    Ready,
    /// Clover didn't let the till add it: the cashier adds it in Clover,
    /// then Look again.
    AddInClover,
}

/// Why a connect code or link wasn't taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanProblem {
    /// Paste, with nothing to paste.
    NothingToPaste,
    /// Pasted text that isn't a connect link.
    NotALink,
    /// The camera saw a code that isn't a connect code.
    NotACode,
}

/// Step 4's state: the point of sale chosen, and its sign-in.
#[derive(Clone, Debug, Default)]
pub struct PosSetup {
    /// `None` until the cashier taps one (acceptance T1.1).
    pub chosen: Option<PointOfSale>,
    /// Square: the location chosen, once signed in.
    pub location: Option<SquareLocation>,
    /// Clover: the merchant's name, once signed in.
    pub merchant: Option<String>,
    pub signed_in: bool,
    pub tender: CloverTender,
    pub busy: bool,
    pub problem: Option<String>,
    /// The test sale's amount, written as the till writes amounts, once
    /// one has come in.
    pub test_sale: Option<String>,
    /// Square has more than one location to choose from.
    pub can_change_location: bool,
    /// Onboarding's step 4; false when Settings' "Change the point of sale"
    /// opened it, with Back and no test sale.
    pub setup: bool,
}

impl PosSetup {
    /// Signed in and watching a Square location or a Clover merchant: the
    /// test sale can come in.
    pub fn connected(&self) -> bool {
        connected(
            self.chosen,
            self.signed_in,
            self.location.is_some(),
            self.merchant.is_some(),
        )
    }
}

/// The step the till is on, with what that step shows.
#[derive(Clone, Debug)]
pub enum Step {
    /// Step 1.
    Welcome,
    /// Step 2: the camera looks for the store's connect code, or the link
    /// is pasted.
    Scan { problem: Option<ScanProblem> },
    /// Step 3, before: Monokulo names the store only once the code is
    /// spent, so this shows the server (decision 22 of android.md).
    Confirm {
        host: String,
        encrypted: bool,
        device_name: String,
        busy: bool,
        problem: Option<String>,
    },
    /// Step 3, after: the store, its wallet and this device, read just now
    /// (never saved: decision 50), to check before going on.
    Connected {
        host: String,
        encrypted: bool,
        store: StoreNow,
    },
    /// Step 4.
    PointOfSale(PosSetup),
    /// Step 4, when Square has more than one location.
    Locations { locations: Vec<SquareLocation> },
    /// Step 5: alerts. `place` is the till's location or merchant, for the
    /// sample alert; `currency` the store's.
    Alerts {
        place: Option<String>,
        currency: String,
        allowed: bool,
    },
    /// Step 5 on Android: the bubble over other apps.
    Float { allowed: bool },
    /// Step 6: what's set up. `location` is the location or merchant.
    Ready {
        store: String,
        location: Option<String>,
        alerts: bool,
        bubble: bool,
    },
}

/// What the onboarding screen shows.
#[derive(Clone, Debug)]
pub struct OnboardingInput {
    pub step: Step,
    /// The point of sale chosen so far: the rail and the last steps name it.
    pub pos: Option<PointOfSale>,
}

/// A step, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct OnboardingView {
    /// Under the buttons: "Step 4 of 6".
    pub step_label: Option<String>,
    /// iPad: every step, beside the page.
    pub rail: Vec<RailStep>,
    pub page: OnboardingPage,
}

/// A step on iPad's rail.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RailStep {
    pub number: u8,
    pub label: String,
    pub state: RailState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RailState {
    Done,
    Current,
    Todo,
}

/// Each step's own record.
// A view record is built once per change and crosses UniFFI by value,
// which can't box a variant.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum OnboardingPage {
    Welcome {
        /// The first feature: every point of sale the till knows.
        feature: String,
    },
    Scan {
        problem: Option<String>,
    },
    Confirm {
        host: String,
        encrypted: bool,
        facts: Vec<Fact>,
        connect: Press,
        cancel: Press,
        problem: Option<String>,
    },
    Connected {
        /// "Connected to The Bean Room", or "Connected" before the name.
        title: String,
        host: String,
        encrypted: bool,
        problem: Option<String>,
        facts: Vec<Fact>,
        /// "Continue" once the store is read, or "Try again" after a
        /// failed read; Retry when `retry`.
        next: Press,
        retry: bool,
        footnote: String,
    },
    PointOfSale {
        view: PosStep,
    },
    Locations {
        lead: String,
        locations: Vec<LocationRow>,
        /// No active locations: "None · Square has no active locations".
        none: Option<Fact>,
    },
    Alerts {
        sale: SampleAlert,
        paid: SampleAlert,
        /// "Continue" once allowed, else "Turn on alerts".
        next: Press,
        not_now: Option<Press>,
    },
    Float {
        heading: String,
        lead: String,
        allowed: bool,
        /// "Continue" once allowed, else "Open settings".
        next: Press,
        notifications_only: Option<Press>,
    },
    Ready {
        heading: String,
        lead: String,
        checks: Vec<Check>,
    },
}

/// Step 4: the choice, then the chosen point of sale's card.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PosStep {
    /// Opened from Settings: Back, and Done when connected.
    pub back: bool,
    pub choices: Vec<PosChoice>,
    pub card: Option<PosCard>,
    /// "Done", "Continue" (once the test sale is in), or the sign-in.
    pub next: Option<Press>,
    /// "Skip the test", or "Not now: only show other tills' sales".
    pub skip: Option<Press>,
}

/// The chosen point of sale's card: its sign-in, its location or merchant,
/// and what to do at the point of sale itself.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PosCard {
    pub mark: PosMark,
    /// "Connect Clover".
    pub heading: String,
    pub lead: String,
    /// "Clover account".
    pub account_title: String,
    pub account: Account,
    /// Square's location, once connected; tap to change it when
    /// `change_location`.
    pub place: Option<Fact>,
    pub change_location: bool,
    pub permissions: String,
    pub problem: Option<String>,
    /// "In Square Point of Sale".
    pub at_the_till: String,
    pub steps_title: String,
    /// Numbered from 1, `**bold**` for what to tap.
    pub steps: Vec<String>,
    /// Clover, the payment type to add by hand: "Look again", drawn after
    /// the first step.
    pub look_again: Option<Press>,
    pub test_sale: Option<TestSale>,
    /// Clover: how a sale never paid is refunded.
    pub footnote: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Account {
    /// "Account · Not signed in".
    NotSignedIn { row: Fact },
    /// A tick, "Connected" and who.
    Connected { who: String },
}

/// The test sale's row: waiting (the loading mark), or found (a tick).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TestSale {
    pub found: bool,
    pub text: String,
}

/// One of Square's locations to choose.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LocationRow {
    pub id: String,
    pub name: String,
    pub currency: String,
    /// "Choose Front Counter".
    pub choose: String,
}

/// An alert as it will look, in the step's picture.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SampleAlert {
    pub title: String,
    pub text: Option<String>,
    pub time: String,
}

/// A row of the Ready step's checks.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Check {
    pub name: String,
    pub value: String,
    pub done: bool,
}

/// The step on `device`, amounts written by `money`.
pub fn onboarding(
    input: &OnboardingInput,
    device: Device,
    money: &dyn MoneyFormat,
) -> OnboardingView {
    let pos = input.pos;
    let (number, labelled, page) = match &input.step {
        Step::Welcome => (
            Some(1),
            true,
            OnboardingPage::Welcome {
                feature: pos_text::welcome_feature(),
            },
        ),
        Step::Scan { problem } => (
            Some(2),
            true,
            OnboardingPage::Scan {
                problem: problem.map(|p| scan_problem(p).to_owned()),
            },
        ),
        Step::Confirm {
            host,
            encrypted,
            device_name,
            busy,
            problem,
        } => (
            Some(3),
            true,
            OnboardingPage::Confirm {
                host: host.clone(),
                encrypted: *encrypted,
                facts: vec![
                    Fact::new("Server", host),
                    Fact::new("Device name", device_name),
                ],
                connect: Press::new(if *busy { "Connecting…" } else { "Connect" }).enabled(!busy),
                cancel: Press::new("Cancel").enabled(!busy),
                problem: problem.clone(),
            },
        ),
        Step::Connected {
            host,
            encrypted,
            store,
        } => (Some(3), true, connected_page(host, *encrypted, store)),
        Step::PointOfSale(setup) => (
            setup.setup.then_some(4),
            setup.setup,
            OnboardingPage::PointOfSale {
                view: pos_step(setup, device),
            },
        ),
        Step::Locations { locations } => (
            Some(4),
            true,
            OnboardingPage::Locations {
                lead: pos_text::locations_lead(),
                locations: locations
                    .iter()
                    .map(|l| LocationRow {
                        id: l.id.clone(),
                        name: l.name.clone(),
                        currency: l.currency.clone(),
                        choose: format!("Choose {}", l.name),
                    })
                    .collect(),
                none: locations
                    .is_empty()
                    .then(|| Fact::new("None", pos_text::no_locations())),
            },
        ),
        Step::Alerts {
            place,
            currency,
            allowed,
        } => (
            Some(5),
            true,
            OnboardingPage::Alerts {
                sale: SampleAlert {
                    title: format!(
                        "New Monero sale · {}",
                        amounts::fiat("24.50", currency, money)
                    ),
                    text: Some(
                        place
                            .iter()
                            .map(String::as_str)
                            .chain(["Tap Show code for the customer."])
                            .collect::<Vec<_>>()
                            .join(". "),
                    ),
                    time: "now".into(),
                },
                paid: SampleAlert {
                    title: format!("Paid · {}", amounts::fiat("8.00", currency, money)),
                    text: None,
                    time: "4m ago".into(),
                },
                next: Press::new(if *allowed {
                    "Continue"
                } else {
                    "Turn on alerts"
                }),
                not_now: (!allowed).then(|| Press::new("Not now")),
            },
        ),
        Step::Float { allowed } => (
            None,
            false,
            OnboardingPage::Float {
                heading: pos_text::float_heading(pos),
                lead: pos_text::float_lead(pos),
                allowed: *allowed,
                next: Press::new(if *allowed {
                    "Continue"
                } else {
                    "Open settings"
                }),
                notifications_only: (!allowed).then(|| Press::new("Use notifications only")),
            },
        ),
        Step::Ready {
            store,
            location,
            alerts,
            bubble,
        } => (
            Some(STEPS),
            false,
            ready_page(pos, device, store, location.as_deref(), *alerts, *bubble),
        ),
    };
    OnboardingView {
        step_label: number
            .filter(|_| labelled)
            .map(|n| format!("Step {n} of {STEPS}")),
        rail: match (device, number) {
            (Device::Ipad, Some(current)) => rail(pos, current),
            _ => Vec::new(),
        },
        page,
    }
}

fn scan_problem(problem: ScanProblem) -> &'static str {
    match problem {
        ScanProblem::NothingToPaste => {
            "There's nothing to paste. Copy the connect link on the dashboard first."
        }
        ScanProblem::NotALink => "That isn't a store's connect link.",
        ScanProblem::NotACode => "That code isn't a store's connect code.",
    }
}

/// What Till can do with a store, said before connecting and after.
pub const CAN_DO: &str = "Till can see this store's orders and create payment codes. It can't spend from or change the wallet. Only connect a server you run or trust.";

fn connected_page(host: &str, encrypted: bool, now: &StoreNow) -> OnboardingPage {
    let store = now.store.as_ref();
    let reading = if now.reading { "Reading…" } else { "—" };
    let mut facts = vec![Fact::new("Store", store.map_or(reading, |s| &s.name))];
    if let Some(wallet) = store.and_then(|s| s.wallet.as_ref()) {
        facts.push(Fact::new("Payments go to", &wallet.name));
        facts.push(Fact::new("Wallet address", wallet.address_short()).mono());
        facts.push(Fact::new("Network", capitalised(&wallet.network)));
    }
    facts.push(Fact::new(
        "Device name",
        now.device.as_ref().map_or(reading, |d| &d.device_name),
    ));
    let retry = store.is_none() && !now.reading;
    OnboardingPage::Connected {
        title: store.map_or_else(
            || "Connected".into(),
            |s| format!("Connected to {}", s.name),
        ),
        host: host.into(),
        encrypted,
        problem: now.problem.clone(),
        facts,
        next: if retry {
            Press::new("Try again")
        } else {
            Press::new("Continue").enabled(store.is_some())
        },
        retry,
        footnote: format!("Check this is the store you meant: payments go to its wallet. {CAN_DO}"),
    }
}

const NOT_NOW: &str = "Not now: only show other tills' sales";

fn pos_step(s: &PosSetup, device: Device) -> PosStep {
    let connected = s.connected();
    let (next, skip) = match s.chosen {
        _ if connected && !s.setup => (Some(Press::new("Done")), None),
        _ if connected => (
            Some(Press::new("Continue").enabled(s.test_sale.is_some())),
            s.test_sale.is_none().then(|| Press::new("Skip the test")),
        ),
        Some(chosen) => (
            Some(
                Press::new(if s.busy {
                    pos_text::SIGNING_IN.to_owned()
                } else {
                    pos_text::sign_in(Some(chosen))
                })
                .enabled(!s.busy),
            ),
            Some(Press::new(NOT_NOW).enabled(!s.busy)),
        ),
        None => (None, Some(Press::new(NOT_NOW).enabled(!s.busy))),
    };
    PosStep {
        back: !s.setup,
        choices: pos_choices(s.chosen, !s.busy, device),
        card: s.chosen.map(|pos| pos_card(pos, s)),
        next,
        skip,
    }
}

fn pos_card(pos: PointOfSale, s: &PosSetup) -> PosCard {
    let connected = s.connected();
    let not_signed_in = Account::NotSignedIn {
        row: Fact::new("Account", "Not signed in"),
    };
    let (account, place) = match (pos, connected) {
        (PointOfSale::Square, true) => {
            let location = s.location.as_ref().expect("connected to Square");
            (
                Account::Connected {
                    who: location
                        .business_name
                        .clone()
                        .unwrap_or_else(|| location.name.clone()),
                },
                Some(Fact::new(pos_text::place_row(Some(pos)), &location.name)),
            )
        }
        (PointOfSale::Clover, true) => (
            Account::Connected {
                who: s.merchant.clone().unwrap_or_default(),
            },
            None,
        ),
        (_, false) => (not_signed_in, None),
    };
    let add_by_hand =
        pos == PointOfSale::Clover && connected && s.tender == CloverTender::AddInClover;
    let (steps_title, steps) = match pos {
        PointOfSale::Square => (
            pos_text::SQUARE_STEPS_TITLE,
            pos_text::SQUARE_STEPS.map(String::from).to_vec(),
        ),
        PointOfSale::Clover => {
            let first = if add_by_hand {
                pos_text::CLOVER_ADD_TENDER.to_owned()
            } else {
                pos_text::clover_tender_step(connected && s.tender == CloverTender::Ready)
            };
            let mut steps = vec![first];
            steps.extend(pos_text::CLOVER_STEPS.map(String::from));
            (pos_text::CLOVER_STEPS_TITLE, steps)
        }
    };
    // No sale can be paid in Monero until Clover has the payment type.
    let test_sale = (connected && s.setup && !add_by_hand).then(|| match &s.test_sale {
        None => TestSale {
            found: false,
            text: pos_text::test_sale_waiting(pos),
        },
        Some(amount) => TestSale {
            found: true,
            text: pos_text::test_sale_found(pos, amount),
        },
    });
    PosCard {
        mark: mark(pos),
        heading: pos_text::connect(pos),
        lead: pos_text::lead(pos),
        account_title: pos_text::account(pos),
        account,
        change_location: place.is_some() && s.can_change_location,
        place,
        permissions: pos_text::permissions(pos),
        problem: s.problem.clone(),
        at_the_till: pos_text::at_the_till(pos).into(),
        steps_title: steps_title.into(),
        steps,
        look_again: add_by_hand.then(|| Press::new(pos_text::LOOK_AGAIN).enabled(!s.busy)),
        test_sale,
        footnote: (pos == PointOfSale::Clover).then(|| pos_text::CLOVER_REFUND_NOTE.into()),
    }
}

fn ready_page(
    pos: Option<PointOfSale>,
    device: Device,
    store: &str,
    location: Option<&str>,
    alerts: bool,
    bubble: bool,
) -> OnboardingPage {
    let (heading, lead) = match device {
        Device::Android => ("You're ready".to_owned(), pos_text::ready_lead(pos, bubble)),
        Device::Iphone => (
            "You're ready".to_owned(),
            format!(
                "{} Till waits in the Dynamic Island and opens when a sale comes in.",
                pos_text::ready_start(pos)
            ),
        ),
        // Step 6 on iPad puts Till in Slide Over, beside Square on the same
        // iPad, or over whatever it shows beside a Clover device.
        Device::Ipad => match pos {
            Some(PointOfSale::Square) => (
                "Keep Till next to Square".to_owned(),
                "Put Till in Slide Over so it floats on top of Square while you work.".to_owned(),
            ),
            Some(PointOfSale::Clover) | None => (
                "Keep Till in Slide Over".to_owned(),
                "Put Till in Slide Over so it floats on top of other apps while you work."
                    .to_owned(),
            ),
        },
    };
    let mut checks = vec![
        Check {
            name: "Store".into(),
            value: store.into(),
            done: true,
        },
        Check {
            name: pos_text::check_name(pos).into(),
            value: location.unwrap_or("Not connected").into(),
            done: location.is_some(),
        },
        Check {
            name: "Alerts".into(),
            value: on_off(alerts).into(),
            done: alerts,
        },
    ];
    if device == Device::Android {
        checks.push(Check {
            name: "Bubble".into(),
            value: if bubble { "On" } else { "Notifications only" }.into(),
            done: bubble,
        });
    }
    OnboardingPage::Ready {
        heading,
        lead,
        checks,
    }
}

/// iPad's step rail, with `current` lit.
fn rail(pos: Option<PointOfSale>, current: u8) -> Vec<RailStep> {
    let labels = [
        "Welcome",
        "Connect a store",
        "Confirm",
        pos_text::check_name(pos),
        "Alerts",
        "Slide Over",
    ];
    labels
        .into_iter()
        .zip(1..)
        .map(|(label, number)| RailStep {
            number,
            label: label.into(),
            state: match number.cmp(&current) {
                std::cmp::Ordering::Less => RailState::Done,
                std::cmp::Ordering::Equal => RailState::Current,
                std::cmp::Ordering::Greater => RailState::Todo,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::Choice;
    use monokulo_client::{AppConnection, Store, Wallet};

    /// Dollars as the US writes them.
    fn usd(minor: i64, currency: &str) -> String {
        assert_eq!(currency, "USD");
        format!("${}.{:02}", minor / 100, minor % 100)
    }

    fn show(step: Step, pos: Option<PointOfSale>, device: Device) -> OnboardingView {
        onboarding(&OnboardingInput { step, pos }, device, &usd)
    }

    fn page(step: Step, pos: Option<PointOfSale>, device: Device) -> OnboardingPage {
        show(step, pos, device).page
    }

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

    fn setup(chosen: Option<PointOfSale>) -> PosSetup {
        PosSetup {
            chosen,
            setup: true,
            ..PosSetup::default()
        }
    }

    fn square_connected() -> PosSetup {
        PosSetup {
            signed_in: true,
            location: Some(location()),
            can_change_location: true,
            ..setup(Some(PointOfSale::Square))
        }
    }

    fn clover_connected(tender: CloverTender) -> PosSetup {
        PosSetup {
            signed_in: true,
            merchant: Some("The Bean Room".into()),
            tender,
            ..setup(Some(PointOfSale::Clover))
        }
    }

    fn pos_step_of(s: PosSetup, device: Device) -> PosStep {
        let chosen = s.chosen;
        match page(Step::PointOfSale(s), chosen, device) {
            OnboardingPage::PointOfSale { view } => view,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn welcome_names_every_point_of_sale_and_is_step_1_of_6() {
        assert_eq!(
            show(Step::Welcome, None, Device::Android),
            OnboardingView {
                step_label: Some("Step 1 of 6".into()),
                rail: vec![],
                page: OnboardingPage::Welcome {
                    feature:
                        "Ring up in Square or Clover as usual and choose Monero as the payment."
                            .into()
                },
            }
        );
    }

    #[test]
    fn scanning_says_why_a_code_or_link_wasnt_taken() {
        let problem = |p| match page(Step::Scan { problem: Some(p) }, None, Device::Iphone) {
            OnboardingPage::Scan { problem } => problem.unwrap(),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            problem(ScanProblem::NothingToPaste),
            "There's nothing to paste. Copy the connect link on the dashboard first."
        );
        assert_eq!(
            problem(ScanProblem::NotALink),
            "That isn't a store's connect link."
        );
        assert_eq!(
            problem(ScanProblem::NotACode),
            "That code isn't a store's connect code."
        );
        assert_eq!(
            show(Step::Scan { problem: None }, None, Device::Android)
                .step_label
                .as_deref(),
            Some("Step 2 of 6")
        );
    }

    #[test]
    fn confirming_shows_the_server_and_this_device_and_waits_while_connecting() {
        let confirm = |busy| {
            page(
                Step::Confirm {
                    host: "pay.yourshop.com".into(),
                    encrypted: true,
                    device_name: "Front Counter iPad".into(),
                    busy,
                    problem: None,
                },
                None,
                Device::Ipad,
            )
        };
        assert_eq!(
            confirm(false),
            OnboardingPage::Confirm {
                host: "pay.yourshop.com".into(),
                encrypted: true,
                facts: vec![
                    Fact::new("Server", "pay.yourshop.com"),
                    Fact::new("Device name", "Front Counter iPad"),
                ],
                connect: Press::new("Connect"),
                cancel: Press::new("Cancel"),
                problem: None,
            }
        );
        let OnboardingPage::Confirm {
            connect, cancel, ..
        } = confirm(true)
        else {
            panic!()
        };
        assert_eq!(connect, Press::new("Connecting…").enabled(false));
        assert!(!cancel.enabled);
    }

    #[test]
    fn connected_shows_the_store_and_its_wallet_to_check_or_tries_again() {
        let read = StoreNow {
            store: Some(Store {
                name: "The Bean Room".into(),
                base_currency: "USD".into(),
                wallet: Some(Wallet {
                    name: "Shop wallet".into(),
                    network: "stagenet".into(),
                    address: "4A8xSAMPLEADDRESS000000000000000000009fQ2".into(),
                }),
            }),
            device: Some(AppConnection {
                app_id: "a".into(),
                device_name: "Front Counter iPad".into(),
                platform: "ios".into(),
                connected_at: 1,
            }),
            ..StoreNow::default()
        };
        let connected = |store: StoreNow| {
            page(
                Step::Connected {
                    host: "pay.yourshop.com".into(),
                    encrypted: true,
                    store,
                },
                None,
                Device::Android,
            )
        };
        assert_eq!(
            connected(read),
            OnboardingPage::Connected {
                title: "Connected to The Bean Room".into(),
                host: "pay.yourshop.com".into(),
                encrypted: true,
                problem: None,
                facts: vec![
                    Fact::new("Store", "The Bean Room"),
                    Fact::new("Payments go to", "Shop wallet"),
                    Fact::new("Wallet address", "4A8xSAMP…00009fQ2").mono(),
                    Fact::new("Network", "Stagenet"),
                    Fact::new("Device name", "Front Counter iPad"),
                ],
                next: Press::new("Continue"),
                retry: false,
                footnote: "Check this is the store you meant: payments go to its wallet. Till can see this store's orders and create payment codes. It can't spend from or change the wallet. Only connect a server you run or trust.".into(),
            }
        );
        let OnboardingPage::Connected {
            title,
            facts,
            next,
            retry,
            ..
        } = connected(StoreNow {
            reading: true,
            ..StoreNow::default()
        })
        else {
            panic!()
        };
        assert_eq!(title, "Connected");
        assert_eq!(
            facts,
            [
                Fact::new("Store", "Reading…"),
                Fact::new("Device name", "Reading…")
            ]
        );
        assert_eq!(next, Press::new("Continue").enabled(false));
        assert!(!retry);
        let OnboardingPage::Connected {
            next,
            retry,
            problem,
            ..
        } = connected(StoreNow {
            problem: Some("Couldn't reach the store.".into()),
            ..StoreNow::default()
        })
        else {
            panic!()
        };
        assert_eq!(next, Press::new("Try again"));
        assert!(retry);
        assert_eq!(problem.as_deref(), Some("Couldn't reach the store."));
    }

    #[test]
    fn step_4_offers_the_choice_and_nothing_else_until_a_tap() {
        let view = pos_step_of(setup(None), Device::Android);
        assert_eq!(view.card, None);
        assert_eq!(view.next, None);
        assert_eq!(
            view.skip,
            Some(Press::new("Not now: only show other tills' sales"))
        );
        assert!(!view.back);
        assert_eq!(
            view.choices
                .iter()
                .map(|c| c.choice.clone())
                .collect::<Vec<_>>(),
            [
                Choice {
                    label: "Square".into(),
                    detail: Some("Square Point of Sale on a phone, tablet or Register".into()),
                    selected: false,
                    enabled: true,
                },
                Choice {
                    label: "Clover".into(),
                    detail: Some("A Clover device".into()),
                    selected: false,
                    enabled: true,
                },
            ]
        );
    }

    #[test]
    fn squares_card_before_the_sign_in_word_for_word() {
        let view = pos_step_of(setup(Some(PointOfSale::Square)), Device::Android);
        assert_eq!(view.next, Some(Press::new("Sign in to Square")));
        assert_eq!(
            view.skip,
            Some(Press::new("Not now: only show other tills' sales"))
        );
        assert_eq!(
            view.card,
            Some(PosCard {
                mark: PosMark::Square,
                heading: "Connect Square".into(),
                lead: "Till checks Square for each Monero payment you take, so it can show the code straight away.".into(),
                account_title: "Square account".into(),
                account: Account::NotSignedIn {
                    row: Fact::new("Account", "Not signed in")
                },
                place: None,
                change_location: false,
                permissions: "Till can read payments, orders and locations, and refund a Monero sale that's never paid. Your Square sign-in stays on this device: Monokulo never sees it, or what was bought.".into(),
                problem: None,
                at_the_till: "In Square Point of Sale".into(),
                steps_title: "Add \"Monero\" as a payment type".into(),
                steps: vec![
                    "Open **Settings › Checkout › Payment types**".into(),
                    "Under Other, add a type called **Monero**".into(),
                    "Ring up a sale and tap Monero to pay".into(),
                ],
                look_again: None,
                test_sale: None,
                footnote: None,
            })
        );
        let busy = pos_step_of(
            PosSetup {
                busy: true,
                ..setup(Some(PointOfSale::Square))
            },
            Device::Android,
        );
        assert_eq!(busy.next, Some(Press::new("Signing in…").enabled(false)));
        assert!(!busy.skip.unwrap().enabled);
        assert!(busy.choices.iter().all(|c| !c.choice.enabled));
    }

    #[test]
    fn square_connected_waits_for_the_test_sale_then_continues() {
        let waiting = pos_step_of(square_connected(), Device::Android);
        assert_eq!(waiting.next, Some(Press::new("Continue").enabled(false)));
        assert_eq!(waiting.skip, Some(Press::new("Skip the test")));
        let card = waiting.card.unwrap();
        assert_eq!(
            card.account,
            Account::Connected {
                who: "The Bean Room Ltd".into()
            }
        );
        assert_eq!(card.place, Some(Fact::new("Location", "Front Counter")));
        assert!(card.change_location);
        assert_eq!(
            card.test_sale,
            Some(TestSale {
                found: false,
                text: "Waiting for a test sale… Any amount, you can refund it after.".into()
            })
        );

        let found = pos_step_of(
            PosSetup {
                test_sale: Some("$1.00".into()),
                ..square_connected()
            },
            Device::Android,
        );
        assert_eq!(found.next, Some(Press::new("Continue")));
        assert_eq!(found.skip, None);
        assert_eq!(
            found.card.unwrap().test_sale,
            Some(TestSale {
                found: true,
                text: "Test sale found: $1.00. Refund it in Square when you like.".into()
            })
        );
    }

    #[test]
    fn opened_from_settings_it_has_back_and_done_and_no_test_sale() {
        let view = show(
            Step::PointOfSale(PosSetup {
                setup: false,
                ..square_connected()
            }),
            Some(PointOfSale::Square),
            Device::Android,
        );
        assert_eq!(view.step_label, None);
        let OnboardingPage::PointOfSale { view } = view.page else {
            panic!()
        };
        assert!(view.back);
        assert_eq!(view.next, Some(Press::new("Done")));
        assert_eq!(view.skip, None);
        assert_eq!(view.card.unwrap().test_sale, None);
    }

    #[test]
    fn clovers_card_names_the_payment_type_the_till_adds_and_its_refunds() {
        let before = pos_step_of(setup(Some(PointOfSale::Clover)), Device::Android)
            .card
            .unwrap();
        assert_eq!(before.heading, "Connect Clover");
        assert_eq!(before.account_title, "Clover account");
        assert_eq!(before.at_the_till, "On your Clover device");
        assert_eq!(before.steps_title, "Take Monero in Clover Register");
        assert_eq!(
            before.steps,
            [
                "Till adds a payment type called **Monero** for you",
                "Open **Clover Register** on the Station, Mini, Flex or Go",
                "Ring up a sale, tap **Pay**, then choose **Monero**",
            ]
        );
        assert_eq!(
            before.footnote.as_deref(),
            Some(
                "A sale never paid in Monero is refunded in Clover's Orders app; Till shows you which."
            )
        );

        let ready = pos_step_of(clover_connected(CloverTender::Ready), Device::Android)
            .card
            .unwrap();
        assert_eq!(
            ready.account,
            Account::Connected {
                who: "The Bean Room".into()
            }
        );
        assert_eq!(ready.place, None);
        assert_eq!(
            ready.steps[0],
            "Till added a payment type called **Monero** for you"
        );
        assert_eq!(
            ready.test_sale,
            Some(TestSale {
                found: false,
                text:
                    "Waiting for a test sale… Any amount; refund it in Clover's Orders app after."
                        .into()
            })
        );

        let looking = pos_step_of(clover_connected(CloverTender::Looking), Device::Android)
            .card
            .unwrap();
        assert_eq!(
            looking.steps[0],
            "Till adds a payment type called **Monero** for you"
        );
    }

    #[test]
    fn clover_refusing_the_payment_type_asks_for_it_by_hand_with_no_test_sale() {
        let card = pos_step_of(clover_connected(CloverTender::AddInClover), Device::Android)
            .card
            .unwrap();
        assert_eq!(
            card.steps,
            [
                "Add a payment type called **Monero** in Clover (Setup › Payments, or ask whoever manages your Clover account), then tap **Look again**",
                "Open **Clover Register** on the Station, Mini, Flex or Go",
                "Ring up a sale, tap **Pay**, then choose **Monero**",
            ]
        );
        assert_eq!(card.look_again, Some(Press::new("Look again")));
        assert_eq!(card.test_sale, None);
    }

    #[test]
    fn on_ios_clovers_choice_says_the_device_sits_beside_it() {
        let detail = |device| {
            pos_step_of(setup(None), device).choices[1]
                .choice
                .detail
                .clone()
                .unwrap()
        };
        assert_eq!(detail(Device::Android), "A Clover device");
        assert_eq!(
            detail(Device::Iphone),
            "A Clover device. This iPhone sits beside it and shows the code."
        );
        assert_eq!(
            detail(Device::Ipad),
            "A Clover device. This iPad sits beside it and shows the code."
        );
    }

    #[test]
    fn squares_locations_are_chosen_by_name() {
        assert_eq!(
            page(
                Step::Locations {
                    locations: vec![location()]
                },
                Some(PointOfSale::Square),
                Device::Android
            ),
            OnboardingPage::Locations {
                lead: "Till shows the Monero sales Square takes there.".into(),
                locations: vec![LocationRow {
                    id: "L1".into(),
                    name: "Front Counter".into(),
                    currency: "USD".into(),
                    choose: "Choose Front Counter".into(),
                }],
                none: None,
            }
        );
        let OnboardingPage::Locations { none, .. } = page(
            Step::Locations { locations: vec![] },
            Some(PointOfSale::Square),
            Device::Android,
        ) else {
            panic!()
        };
        assert_eq!(
            none,
            Some(Fact::new("None", "Square has no active locations"))
        );
    }

    #[test]
    fn alerts_show_a_sample_in_the_stores_money_at_the_tills_place() {
        let alerts = |place: Option<&str>, allowed| {
            page(
                Step::Alerts {
                    place: place.map(str::to_owned),
                    currency: "USD".into(),
                    allowed,
                },
                Some(PointOfSale::Square),
                Device::Iphone,
            )
        };
        assert_eq!(
            alerts(Some("Front Counter"), false),
            OnboardingPage::Alerts {
                sale: SampleAlert {
                    title: "New Monero sale · $24.50".into(),
                    text: Some("Front Counter. Tap Show code for the customer.".into()),
                    time: "now".into(),
                },
                paid: SampleAlert {
                    title: "Paid · $8.00".into(),
                    text: None,
                    time: "4m ago".into(),
                },
                next: Press::new("Turn on alerts"),
                not_now: Some(Press::new("Not now")),
            }
        );
        let OnboardingPage::Alerts {
            sale,
            next,
            not_now,
            ..
        } = alerts(None, true)
        else {
            panic!()
        };
        assert_eq!(
            sale.text.as_deref(),
            Some("Tap Show code for the customer.")
        );
        assert_eq!(next, Press::new("Continue"));
        assert_eq!(not_now, None);
    }

    #[test]
    fn androids_bubble_step_floats_over_square_or_other_apps() {
        assert_eq!(
            show(Step::Float { allowed: false }, Some(PointOfSale::Clover), Device::Android),
            OnboardingView {
                step_label: None,
                rail: vec![],
                page: OnboardingPage::Float {
                    heading: "Float over other apps".into(),
                    lead: "Till puts a small bubble over other apps. It's faded while there's nothing to do and lights up when a Monero sale comes in. Tap it to show the code.".into(),
                    allowed: false,
                    next: Press::new("Open settings"),
                    notifications_only: Some(Press::new("Use notifications only")),
                },
            }
        );
        let OnboardingPage::Float {
            heading,
            next,
            notifications_only,
            ..
        } = page(
            Step::Float { allowed: true },
            Some(PointOfSale::Square),
            Device::Android,
        )
        else {
            panic!()
        };
        assert_eq!(heading, "Float over Square");
        assert_eq!(next, Press::new("Continue"));
        assert_eq!(notifications_only, None);
    }

    fn ready(device: Device, pos: Option<PointOfSale>, location: Option<&str>) -> OnboardingView {
        show(
            Step::Ready {
                store: "The Bean Room".into(),
                location: location.map(str::to_owned),
                alerts: true,
                bubble: false,
            },
            pos,
            device,
        )
    }

    #[test]
    fn ready_on_android_checks_the_store_point_of_sale_alerts_and_bubble() {
        let view = ready(
            Device::Android,
            Some(PointOfSale::Square),
            Some("Front Counter"),
        );
        assert_eq!(view.step_label, None);
        assert_eq!(
            view.page,
            OnboardingPage::Ready {
                heading: "You're ready".into(),
                lead: "Start Till and switch to Square. Till waits in its notification and alerts you when a sale comes in.".into(),
                checks: vec![
                    Check { name: "Store".into(), value: "The Bean Room".into(), done: true },
                    Check { name: "Square".into(), value: "Front Counter".into(), done: true },
                    Check { name: "Alerts".into(), value: "On".into(), done: true },
                    Check { name: "Bubble".into(), value: "Notifications only".into(), done: false },
                ],
            }
        );
        let OnboardingPage::Ready { checks, .. } = ready(Device::Android, None, None).page else {
            panic!()
        };
        assert_eq!(
            checks[1],
            Check {
                name: "Point of sale".into(),
                value: "Not connected".into(),
                done: false
            }
        );
    }

    #[test]
    fn ready_on_an_iphone_waits_in_the_dynamic_island_with_no_bubble() {
        assert_eq!(
            ready(Device::Iphone, Some(PointOfSale::Clover), Some("The Bean Room")).page,
            OnboardingPage::Ready {
                heading: "You're ready".into(),
                lead: "Start Till and ring up on your Clover device. Till waits in the Dynamic Island and opens when a sale comes in.".into(),
                checks: vec![
                    Check { name: "Store".into(), value: "The Bean Room".into(), done: true },
                    Check { name: "Clover".into(), value: "The Bean Room".into(), done: true },
                    Check { name: "Alerts".into(), value: "On".into(), done: true },
                ],
            }
        );
    }

    #[test]
    fn on_an_ipad_step_6_puts_till_in_slide_over_and_the_rail_follows_the_steps() {
        let view = ready(
            Device::Ipad,
            Some(PointOfSale::Square),
            Some("Front Counter"),
        );
        let OnboardingPage::Ready { heading, lead, .. } = &view.page else {
            panic!()
        };
        assert_eq!(heading, "Keep Till next to Square");
        assert_eq!(
            lead,
            "Put Till in Slide Over so it floats on top of Square while you work."
        );
        assert!(view.rail.iter().take(5).all(|s| s.state == RailState::Done));
        assert_eq!(view.rail[5].state, RailState::Current);

        let OnboardingPage::Ready { heading, lead, .. } =
            ready(Device::Ipad, Some(PointOfSale::Clover), None).page
        else {
            panic!()
        };
        assert_eq!(heading, "Keep Till in Slide Over");
        assert_eq!(
            lead,
            "Put Till in Slide Over so it floats on top of other apps while you work."
        );

        let rail = show(Step::Scan { problem: None }, None, Device::Ipad).rail;
        let rail: Vec<_> = rail
            .iter()
            .map(|s| (s.number, s.label.as_str(), s.state))
            .collect();
        assert_eq!(
            rail,
            [
                (1, "Welcome", RailState::Done),
                (2, "Connect a store", RailState::Current),
                (3, "Confirm", RailState::Todo),
                (4, "Point of sale", RailState::Todo),
                (5, "Alerts", RailState::Todo),
                (6, "Slide Over", RailState::Todo),
            ]
        );
        let square = show(Step::Welcome, Some(PointOfSale::Square), Device::Ipad).rail;
        assert_eq!(square[3].label, "Square");
        assert_eq!(show(Step::Welcome, None, Device::Iphone).rail, []);
    }
}
