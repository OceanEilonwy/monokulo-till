//! Setting up a till, step by step (boards Onb*, PadOnb*, PhonePos;
//! Android's `ui/Onboarding.kt`): welcome, connecting a store and checking
//! it, the point of sale and its sign-in with the test sale, Square's
//! locations, alerts, Android's bubble, and ready.
//!
//! What's here is what state chooses: the step's number, iPad's step rail,
//! the buttons and whether they can be pressed, the store's facts, the
//! point of sale's card, and every sentence that names the point of sale.
//! The headings and leads written once ("Connect a store", "Hear every new
//! sale", the dashboard's steps) are here too, for iOS (core.md decision
//! 130); Android keeps its own copies of them for now.

use monokulo_client::{MoneyFormat, PointOfSale, amounts};
use till_pos::square::SquareLocation;

use crate::pos_text;
use crate::settings::{ShowSales, StoreNow, connected, pos_choices, show_sales_choices};
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
    /// Past O1: the cashier chose the point of sale and went on to sign in.
    pub confirmed: bool,
}

/// C4's state: the test sale and the device it came from.
#[derive(Clone, Debug)]
pub struct CounterSetup {
    /// The test sale's amount, as the till writes amounts.
    pub amount: String,
    /// "#t0Ab", when the point of sale numbered it.
    pub number: Option<String>,
    /// The device's name ("Front iPad").
    pub device: String,
    /// The location ("Front Counter").
    pub place: Option<String>,
    pub every_device: bool,
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
    /// C4, after the test sale named its device.
    Counter(CounterSetup),
    /// O4 on iPad: how a new sale shows up. `place` names the sample
    /// alert's sale amount's currency, as Alerts' does.
    ShowSales { chosen: ShowSales, currency: String },
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
    /// Android, under the buttons: "Step 4 of 6". Never on iOS (core.md
    /// decision 121): the iPad's rail shows the step, the iPhone's
    /// [`Self::progress`] bar.
    pub step_label: Option<String>,
    /// iPad: every step, beside the page.
    pub rail: Vec<RailStep>,
    pub page: OnboardingPage,
    /// iPhone: how far through setup, for a thin progress bar; none on
    /// Welcome.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub progress: Option<StepProgress>,
    /// The page's heading, on every page ("Which point of sale do you
    /// use?"), and its lead (core.md decision 130). Android keeps its own
    /// for the pages whose record has none.
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub heading: String,
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub lead: String,
    /// The label over the page's card: "This device will take payments
    /// for", "Locations", "Point of sale".
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub section: String,
    /// Under the card, where the page's record has no footnote of its own:
    /// what Till can do with a store, before connecting.
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub footnote: String,
    /// The page's own button where its record has none: "Get started",
    /// "Start Till".
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub start: Option<Press>,
    /// After connecting: "Wrong store: disconnect".
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub disconnect: Option<Press>,
    /// Welcome's three features.
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    pub features: Vec<Feature>,
    /// Step 2's words around the camera.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub scan: Option<ScanWords>,
}

/// Which of Welcome's features, for its symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum FeatureMark {
    /// "Sees Monero sales".
    Sales,
    /// "Shows a payment code".
    Code,
    /// "Confirms the money arrived".
    Confirmed,
}

/// One of Welcome's features.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Feature {
    pub mark: FeatureMark,
    pub title: String,
    pub text: String,
}

/// Step 2's words around the camera (board OnbStore).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ScanWords {
    /// The dashboard's numbered steps, `**bold**` for what to tap.
    pub steps: Vec<String>,
    /// "Dashboard open on this iPad? Tap **Copy the connect link** there,
    /// then paste it here."
    pub paste_hint: String,
    /// "Paste the link".
    pub paste: String,
    /// Over the camera while it looks: "Looking for a connect code…".
    pub looking: String,
    /// In place of the camera until it's allowed.
    pub camera_ask: String,
    /// "Allow the camera".
    pub allow_camera: String,
}

/// How far through setup, as a bar draws it: step `value` of `total`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct StepProgress {
    pub value: u8,
    pub total: u8,
}

/// Where setup is, as iOS counts it (core.md decision 121): the rail's
/// steps on iPad, the progress bar's on iPhone, which has no "How sales
/// show up".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Phase {
    Welcome,
    Store,
    PointOfSale,
    Alerts,
    ShowSales,
    Ready,
}

impl Phase {
    /// The rail's names, in order (board OnbRail).
    const RAIL: [(Phase, &'static str); 6] = [
        (Phase::Welcome, "Welcome"),
        (Phase::Store, "Connect your store"),
        (Phase::PointOfSale, "Point of sale"),
        (Phase::Alerts, "Alerts"),
        (Phase::ShowSales, "How sales show up"),
        (Phase::Ready, "Ready"),
    ];

    /// The iPhone's bar: none on Welcome, and no "How sales show up".
    fn progress(self) -> Option<StepProgress> {
        let steps = [
            Phase::Store,
            Phase::PointOfSale,
            Phase::Alerts,
            Phase::Ready,
        ];
        let at = steps.iter().position(|p| *p == self)?;
        Some(StepProgress {
            value: at as u8 + 1,
            total: steps.len() as u8,
        })
    }
}

/// Step 4 split in three (O1–O3, core.md decision 121): choosing the point
/// of sale, signing in, then the test sale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PosStage {
    /// "Which point of sale do you use?": the choice, "Continue with
    /// Square" and "Set up later".
    Choose,
    /// "Sign in to Square": the card, the sign-in and Back.
    SignIn,
    /// "Try a test sale": the steps at the point of sale, and the status
    /// bar beside "Skip the test" and Continue.
    TestSale,
}

/// O3's status bar: "Waiting for the test sale…" / "Continue unlocks when
/// it arrives", or the sale found.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TestStatus {
    pub found: bool,
    pub title: String,
    pub detail: String,
}

/// C4: which sales alert here, after the test sale named its device
/// (board CounterLearn, core.md decision 113).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CounterStep {
    /// "Test sale found · $1.00 · Sale #t0Ab".
    pub found: String,
    /// "Which sales should alert this iPad?".
    pub heading: String,
    /// "The test sale was rung up on Front iPad."
    pub lead: String,
    /// "Front iPad".
    pub device: String,
    /// "Only Front iPad", "Every sale at Front Counter":
    /// [`crate::view::Choice`]s for `Action::SetAlertFor { every_device }`,
    /// false then true, which the platforms lead with a device's icon and
    /// a shop's.
    pub choices: Vec<crate::view::Choice>,
    pub footnote: String,
    pub next: Press,
}

/// O4: how a new sale shows up (board OnbShow, core.md decision 120).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ShowSalesStep {
    pub heading: String,
    pub lead: String,
    pub choices: Vec<crate::settings::ShowSalesChoice>,
    /// "On iPadOS 26" and how, while side by side is chosen.
    pub side_by_side: Option<Fact>,
    /// "Send a test alert", and the alert it posts.
    pub test_alert: Press,
    pub sample: crate::notification::SaleAlert,
    pub next: Press,
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
    /// C4: which sales alert here.
    Counter {
        view: CounterStep,
    },
    /// O4 (iPad): how a new sale shows up.
    ShowSales {
        view: ShowSalesStep,
    },
}

/// Step 4: the choice, then the chosen point of sale's card.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PosStep {
    /// Opened from Settings, or past choosing: Back, and Done when
    /// connected from Settings.
    pub back: bool,
    pub choices: Vec<PosChoice>,
    pub card: Option<PosCard>,
    /// "Continue with Square", the sign-in, "Continue" (once the test sale
    /// is in), or "Done".
    pub next: Option<Press>,
    /// "Skip the test", or "Set up later".
    pub skip: Option<Press>,
    /// Which of O1–O3 this is, in setup; `None` from Settings, which shows
    /// the choice and the card together.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub stage: Option<PosStage>,
    /// Under "Set up later": what a till with no point of sale can do.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub skip_footnote: Option<String>,
    /// O3's status bar.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub status: Option<TestStatus>,
    /// O3's line over its heading: "Connected to The Bean Room · Front
    /// Counter".
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub connected: Option<String>,
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
    /// "Connected", beside who, once signed in.
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub connected: String,
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
                        id: l.id.to_string(),
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
        Step::Counter(c) => (
            Some(4),
            false,
            OnboardingPage::Counter {
                view: counter_step(c, device),
            },
        ),
        Step::ShowSales { chosen, currency } => (
            None,
            false,
            OnboardingPage::ShowSales {
                view: show_sales_step(*chosen, currency, pos, money),
            },
        ),
    };
    let phase = phase(&input.step);
    let words = words(&page, device);
    OnboardingView {
        heading: words.heading,
        lead: words.lead,
        section: words.section,
        footnote: words.footnote,
        start: words.start,
        disconnect: words.disconnect,
        features: words.features,
        scan: words.scan,
        // iOS has its rail and its bar instead (decision 121).
        step_label: number
            .filter(|_| labelled && device == Device::Android)
            .map(|n| format!("Step {n} of {STEPS}")),
        rail: match (device, phase) {
            (Device::Ipad, Some(current)) => rail(current),
            _ => Vec::new(),
        },
        progress: phase
            .filter(|_| device == Device::Iphone)
            .and_then(Phase::progress),
        page,
    }
}

/// Where `step` is in setup as iOS counts it; `None` for step 4 opened from
/// Settings and Android's bubble, which aren't steps there.
fn phase(step: &Step) -> Option<Phase> {
    Some(match step {
        Step::Welcome => Phase::Welcome,
        Step::Scan { .. } | Step::Confirm { .. } | Step::Connected { .. } => Phase::Store,
        Step::PointOfSale(setup) if !setup.setup => return None,
        Step::PointOfSale(_) | Step::Locations { .. } | Step::Counter(_) => Phase::PointOfSale,
        Step::Alerts { .. } => Phase::Alerts,
        Step::Float { .. } => return None,
        Step::ShowSales { .. } => Phase::ShowSales,
        Step::Ready { .. } => Phase::Ready,
    })
}

/// The words a page's record doesn't carry (core.md decision 130).
#[derive(Default)]
struct Words {
    heading: String,
    lead: String,
    section: String,
    footnote: String,
    start: Option<Press>,
    disconnect: Option<Press>,
    features: Vec<Feature>,
    scan: Option<ScanWords>,
}

fn words(page: &OnboardingPage, device: Device) -> Words {
    let said = |heading: &str, lead: &str| Words {
        heading: heading.into(),
        lead: lead.into(),
        ..Words::default()
    };
    match page {
        OnboardingPage::Welcome { feature } => Words {
            start: Some(Press::new("Get started")),
            features: vec![
                Feature {
                    mark: FeatureMark::Sales,
                    title: "Sees Monero sales".into(),
                    text: feature.clone(),
                },
                Feature {
                    mark: FeatureMark::Code,
                    title: "Shows a payment code".into(),
                    text: "Turn the screen round and the customer scans with their wallet.".into(),
                },
                Feature {
                    mark: FeatureMark::Confirmed,
                    title: "Confirms the money arrived".into(),
                    text: "Till tells you when the payment lands, and flags any that don't.".into(),
                },
            ],
            ..said(
                "Monokulo Till",
                "Take Monero at your till. Till sees each new sale and shows your customer a payment code.",
            )
        },
        OnboardingPage::Scan { .. } => Words {
            scan: Some(ScanWords {
                steps: vec![
                    "In your Monokulo dashboard, open the store's **Settings › Connections**"
                        .into(),
                    "Under **Apps**, choose **Connect an app**, then **Show the code**".into(),
                    "Point this camera at the code it shows".into(),
                ],
                paste_hint: format!(
                    "Dashboard open on {}? Tap **Copy the connect link** there, then paste it here.",
                    pos_text::this_device(device)
                ),
                paste: "Paste the link".into(),
                looking: "Looking for a connect code…".into(),
                camera_ask: "Allow the camera to scan the code, or paste the link below.".into(),
                allow_camera: "Allow the camera".into(),
            }),
            ..said(
                "Connect a store",
                "Till connects from your Monokulo dashboard, wherever you host it.",
            )
        },
        OnboardingPage::Confirm { .. } => Words {
            section: "This device will take payments for".into(),
            footnote: CAN_DO.into(),
            ..said("Connect to this store?", "")
        },
        OnboardingPage::Connected { title, .. } => Words {
            section: "This device takes payments for".into(),
            disconnect: Some(Press::new("Wrong store: disconnect")),
            ..said(title, "")
        },
        OnboardingPage::PointOfSale { view } => {
            let name = view.card.as_ref().map(|card| match card.mark {
                PosMark::Square => PointOfSale::Square,
                PosMark::Clover => PointOfSale::Clover,
            });
            match (view.stage, name) {
                (Some(PosStage::Choose), _) => said(
                    "Which point of sale do you use?",
                    "Till signs in to it to see each Monero sale the moment you ring it up.",
                ),
                (Some(PosStage::SignIn), Some(pos)) => said(
                    &pos_text::sign_in(Some(pos)),
                    &format!(
                        "{}'s own page opens. Sign in as the business owner and allow Till.",
                        pos.display_name()
                    ),
                ),
                (Some(PosStage::TestSale), Some(pos)) => said(
                    "Try a test sale",
                    &format!(
                        "This checks that {} tells Till about Monero sales. It takes about a minute.",
                        pos.display_name()
                    ),
                ),
                _ => Words {
                    section: pos_text::POINT_OF_SALE.into(),
                    ..said(pos_text::CONNECT_HEADING, "")
                },
            }
        }
        OnboardingPage::Locations { lead, .. } => Words {
            section: "Locations".into(),
            ..said("Which location is this till at?", lead)
        },
        OnboardingPage::Alerts { .. } => said(
            "Hear every new sale",
            "When Till is in the background, an alert tells you a customer is ready to pay.",
        ),
        OnboardingPage::Float { heading, lead, .. } => said(heading, lead),
        OnboardingPage::Ready { heading, lead, .. } => Words {
            start: Some(Press::new("Start Till")),
            ..said(heading, lead)
        },
        OnboardingPage::Counter { view } => said(&view.heading, &view.lead),
        OnboardingPage::ShowSales { view } => said(&view.heading, &view.lead),
    }
}

fn counter_step(c: &CounterSetup, device: Device) -> CounterStep {
    let mut found = vec!["Test sale found".to_owned(), c.amount.clone()];
    found.extend(c.number.as_ref().map(|n| format!("Sale {n}")));
    let place = c.place.as_deref().unwrap_or("this location");
    let this = pos_text::this_device(device);
    CounterStep {
        found: found.join(" · "),
        heading: format!("Which sales should alert {this}?"),
        // One fact, then two plain choices (owner, round 4): the device
        // and the location are in the choices themselves.
        lead: format!("The test sale was rung up on {}.", c.device),
        device: c.device.clone(),
        choices: vec![
            crate::view::Choice {
                label: format!("Only {}", c.device),
                detail: Some("Recommended. Other counters' sales still show, quietly.".into()),
                selected: !c.every_device,
                enabled: true,
            },
            crate::view::Choice {
                label: format!("Every sale at {place}"),
                detail: Some("One till for the whole location.".into()),
                selected: c.every_device,
                enabled: true,
            },
        ],
        footnote: "Change this any time in Settings.".into(),
        next: Press::new("Continue"),
    }
}

fn show_sales_step(
    chosen: ShowSales,
    currency: &str,
    pos: Option<PointOfSale>,
    money: &dyn MoneyFormat,
) -> ShowSalesStep {
    ShowSalesStep {
        heading: "How should a new sale show up?".into(),
        lead: format!(
            "Keep using {} as normal. You can change this any time in Settings.",
            pos_text::name_first(pos)
        ),
        choices: show_sales_choices(chosen, pos),
        side_by_side: (chosen == ShowSales::SideBySide)
            .then(|| Fact::new("On iPadOS 26", crate::settings::SIDE_BY_SIDE_HOW)),
        test_alert: Press::new("Send a test alert"),
        sample: crate::notification::SaleAlert {
            order_id: "test".into(),
            title: format!("Sam · {}", amounts::fiat("24.50", currency, money)),
            body: "Monero sale. Tap to show Sam the code.".into(),
        },
        next: Press::new("Continue"),
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
            Press::new("Continue")
                .enabled(store.is_some())
                .busy(store.is_none() && now.reading)
        },
        retry,
        footnote: format!("Check this is the store you meant: payments go to its wallet. {CAN_DO}"),
    }
}

/// Which of O1–O3 `s` is on, in setup (decision 121).
fn stage(s: &PosSetup) -> Option<PosStage> {
    if !s.setup {
        None
    } else if s.connected() {
        Some(PosStage::TestSale)
    } else if s.confirmed && s.chosen.is_some() {
        Some(PosStage::SignIn)
    } else {
        Some(PosStage::Choose)
    }
}

fn pos_step(s: &PosSetup, device: Device) -> PosStep {
    let connected = s.connected();
    let stage = stage(s);
    let later = || Press::new(pos_text::SET_UP_LATER).enabled(!s.busy);
    let (next, skip) = match (s.chosen, stage) {
        _ if connected && !s.setup => (Some(Press::new("Done")), None),
        _ if connected => (
            Some(Press::new("Continue").enabled(s.test_sale.is_some())),
            s.test_sale.is_none().then(|| Press::new("Skip the test")),
        ),
        // O1: the choice, then on to sign in.
        (Some(chosen), Some(PosStage::Choose)) => (
            Some(Press::new(format!("Continue with {}", chosen.display_name())).enabled(!s.busy)),
            Some(later()),
        ),
        (Some(chosen), _) => (
            Some(
                Press::new(if s.busy {
                    pos_text::SIGNING_IN.to_owned()
                } else {
                    pos_text::sign_in(Some(chosen))
                })
                .enabled(!s.busy)
                .busy(s.busy),
            ),
            // O2 goes back to the choice instead.
            (stage != Some(PosStage::SignIn)).then(later),
        ),
        (None, _) => (None, Some(later())),
    };
    let status = (stage == Some(PosStage::TestSale)).then(|| match &s.test_sale {
        None => TestStatus {
            found: false,
            title: "Waiting for the test sale…".into(),
            detail: "Continue unlocks when it arrives.".into(),
        },
        Some(amount) => TestStatus {
            found: true,
            title: format!("Test sale found · {amount}"),
            detail: "Continue when you're ready.".into(),
        },
    });
    PosStep {
        back: !s.setup || stage == Some(PosStage::SignIn),
        choices: pos_choices(s.chosen, !s.busy, device),
        // O1 is the choice alone; the card follows it.
        card: s
            .chosen
            .filter(|_| stage != Some(PosStage::Choose))
            .map(|pos| pos_card(pos, s)),
        next,
        skip_footnote: skip
            .as_ref()
            .filter(|p| p.label == pos_text::SET_UP_LATER)
            .map(|_| pos_text::set_up_later_footnote(device)),
        skip,
        stage,
        status,
        connected: (stage == Some(PosStage::TestSale))
            .then(|| connected_line(s))
            .flatten(),
    }
}

/// O3's line: "Connected to The Bean Room · Front Counter", the location
/// said once when it's the business's name.
fn connected_line(s: &PosSetup) -> Option<String> {
    let (who, place) = match s.chosen? {
        PointOfSale::Square => {
            let location = s.location.as_ref()?;
            (
                location
                    .business_name
                    .clone()
                    .unwrap_or_else(|| location.name.clone()),
                Some(location.name.clone()),
            )
        }
        PointOfSale::Clover => (s.merchant.clone()?, None),
    };
    let mut parts = vec![format!("Connected to {who}")];
    parts.extend(place.filter(|place| !place.is_empty() && *place != who));
    Some(parts.join(" · "))
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
        connected: "Connected".into(),
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
        // How a sale shows up was chosen the step before (O4, decision
        // 120), in place of the old Slide Over step.
        Device::Ipad => (
            "You're ready".to_owned(),
            format!(
                "{} Till shows each new sale the way you chose.",
                pos_text::ready_start(pos)
            ),
        ),
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
fn rail(current: Phase) -> Vec<RailStep> {
    Phase::RAIL
        .into_iter()
        .zip(1..)
        .map(|((phase, label), number)| RailStep {
            number,
            label: label.into(),
            state: match phase.cmp(&current) {
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
            confirmed: true,
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
        let view = show(Step::Welcome, None, Device::Android);
        assert_eq!(view.step_label.as_deref(), Some("Step 1 of 6"));
        assert_eq!((view.rail, view.progress), (vec![], None));
        assert_eq!(
            view.page,
            OnboardingPage::Welcome {
                feature: "Ring up in Square or Clover as usual and choose Monero as the payment."
                    .into()
            }
        );
        // Decision 130: the words written once are the engine's too.
        assert_eq!(view.heading, "Monokulo Till");
        assert_eq!(
            view.lead,
            "Take Monero at your till. Till sees each new sale and shows your customer a payment code."
        );
        assert_eq!(view.start, Some(Press::new("Get started")));
        assert_eq!(
            view.features
                .iter()
                .map(|f| (f.mark, f.title.as_str()))
                .collect::<Vec<_>>(),
            [
                (FeatureMark::Sales, "Sees Monero sales"),
                (FeatureMark::Code, "Shows a payment code"),
                (FeatureMark::Confirmed, "Confirms the money arrived"),
            ]
        );
        assert_eq!(
            view.features[0].text,
            "Ring up in Square or Clover as usual and choose Monero as the payment."
        );
    }

    /// Decision 130: every page's heading, and the words around its card.
    #[test]
    fn each_page_carries_its_heading_lead_and_section() {
        let scan = show(Step::Scan { problem: None }, None, Device::Ipad);
        assert_eq!(scan.heading, "Connect a store");
        let words = scan.scan.unwrap();
        assert_eq!(
            words.paste_hint,
            "Dashboard open on this iPad? Tap **Copy the connect link** there, then paste it here."
        );
        assert_eq!(words.steps.len(), 3);
        let confirm = show(
            Step::Confirm {
                host: "shop.example".into(),
                encrypted: true,
                device_name: "Front iPad".into(),
                busy: false,
                problem: None,
            },
            None,
            Device::Ipad,
        );
        assert_eq!(
            (
                confirm.heading.as_str(),
                confirm.section.as_str(),
                confirm.footnote.as_str()
            ),
            (
                "Connect to this store?",
                "This device will take payments for",
                CAN_DO
            )
        );
        let locations = show(Step::Locations { locations: vec![] }, None, Device::Ipad);
        assert_eq!(locations.heading, "Which location is this till at?");
        assert_eq!(locations.section, "Locations");
        let alerts = show(
            Step::Alerts {
                place: None,
                currency: "USD".into(),
                allowed: false,
            },
            None,
            Device::Ipad,
        );
        assert_eq!(alerts.heading, "Hear every new sale");
        let ready = ready(
            Device::Ipad,
            Some(PointOfSale::Square),
            Some("Front Counter"),
        );
        assert_eq!(ready.heading, "You're ready");
        assert_eq!(ready.start, Some(Press::new("Start Till")));
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
        assert_eq!(
            next,
            Press::new("Continue").enabled(false).busy(true),
            "Continue shows the read going"
        );
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
        assert_eq!(view.skip, Some(Press::new("Set up later")));
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
        assert_eq!(view.skip, None);
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
                connected: "Connected".into(),
            })
        );
        let busy = pos_step_of(
            PosSetup {
                busy: true,
                ..setup(Some(PointOfSale::Square))
            },
            Device::Android,
        );
        assert_eq!(
            busy.next,
            Some(Press::new("Signing in…").enabled(false).busy(true))
        );
        assert_eq!(busy.skip, None, "O2 goes back with Back");
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
        let view = show(
            Step::Float { allowed: false },
            Some(PointOfSale::Clover),
            Device::Android,
        );
        assert_eq!(
            (view.step_label, view.rail, view.progress),
            (None, vec![], None)
        );
        assert_eq!(
            view.page,
            OnboardingPage::Float {
                heading: "Float over other apps".into(),
                lead: "Till puts a small bubble over other apps. It's faded while there's nothing to do and lights up when a Monero sale comes in. Tap it to show the code.".into(),
                allowed: false,
                next: Press::new("Open settings"),
                notifications_only: Some(Press::new("Use notifications only")),
            }
        );
        assert_eq!(view.heading, "Float over other apps");
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
    fn ipad_has_the_rail_alone_and_iphone_a_progress_bar_with_none_on_welcome() {
        let rail = show(Step::Scan { problem: None }, None, Device::Ipad);
        assert_eq!(rail.step_label, None, "no \"Step N of 6\" on iPad");
        assert_eq!(rail.progress, None);
        let names: Vec<_> = rail
            .rail
            .iter()
            .map(|s| (s.number, s.label.as_str(), s.state))
            .collect();
        assert_eq!(
            names,
            [
                (1, "Welcome", RailState::Done),
                (2, "Connect your store", RailState::Current),
                (3, "Point of sale", RailState::Todo),
                (4, "Alerts", RailState::Todo),
                (5, "How sales show up", RailState::Todo),
                (6, "Ready", RailState::Todo),
            ]
        );
        let ready = ready(
            Device::Ipad,
            Some(PointOfSale::Square),
            Some("Front Counter"),
        );
        assert!(
            ready
                .rail
                .iter()
                .take(5)
                .all(|s| s.state == RailState::Done)
        );
        assert_eq!(ready.rail[5].state, RailState::Current);
        let OnboardingPage::Ready { heading, lead, .. } = &ready.page else {
            panic!()
        };
        assert_eq!(heading, "You're ready");
        assert_eq!(
            lead,
            "Start Till and switch to Square. Till shows each new sale the way you chose."
        );
        let show_step = show(
            Step::ShowSales {
                chosen: ShowSales::Alert,
                currency: "USD".into(),
            },
            Some(PointOfSale::Square),
            Device::Ipad,
        );
        assert_eq!(show_step.rail[4].state, RailState::Current);

        let iphone = |step| show(step, Some(PointOfSale::Square), Device::Iphone);
        assert_eq!(iphone(Step::Welcome).progress, None, "none on Welcome");
        assert_eq!(iphone(Step::Welcome).step_label, None);
        assert_eq!(
            iphone(Step::Scan { problem: None }).progress,
            Some(StepProgress { value: 1, total: 4 })
        );
        assert_eq!(
            iphone(Step::PointOfSale(square_connected())).progress,
            Some(StepProgress { value: 2, total: 4 })
        );
        assert_eq!(
            iphone(Step::Ready {
                store: "The Bean Room".into(),
                location: None,
                alerts: true,
                bubble: false,
            })
            .progress,
            Some(StepProgress { value: 4, total: 4 })
        );
        assert_eq!(iphone(Step::Welcome).rail, []);
        assert_eq!(
            show(Step::Welcome, None, Device::Android)
                .step_label
                .as_deref(),
            Some("Step 1 of 6"),
            "Android keeps its words"
        );
    }

    #[test]
    fn step_4_goes_choose_then_sign_in_then_test_sale() {
        let choose = pos_step_of(
            PosSetup {
                confirmed: false,
                ..setup(Some(PointOfSale::Square))
            },
            Device::Ipad,
        );
        assert_eq!(choose.stage, Some(PosStage::Choose));
        assert_eq!(choose.card, None, "O1 is the choice alone");
        assert_eq!(choose.next, Some(Press::new("Continue with Square")));
        assert_eq!(choose.skip, Some(Press::new("Set up later")));
        assert_eq!(
            choose.skip_footnote.as_deref(),
            Some(
                "Without a point of sale, this iPad can't find new sales itself. It still shows sales that another till finds."
            )
        );
        assert!(!choose.back);

        let sign_in = pos_step_of(setup(Some(PointOfSale::Square)), Device::Ipad);
        assert_eq!(sign_in.stage, Some(PosStage::SignIn));
        assert!(sign_in.card.is_some() && sign_in.back);
        assert_eq!(sign_in.next, Some(Press::new("Sign in to Square")));
        assert_eq!(sign_in.skip, None, "Back returns to the choice");
        let busy = pos_step_of(
            PosSetup {
                busy: true,
                ..setup(Some(PointOfSale::Square))
            },
            Device::Ipad,
        );
        assert!(
            busy.next.unwrap().busy,
            "the button shows the sign-in going"
        );

        let test = pos_step_of(square_connected(), Device::Ipad);
        assert_eq!(test.stage, Some(PosStage::TestSale));
        assert_eq!(
            test.status,
            Some(TestStatus {
                found: false,
                title: "Waiting for the test sale…".into(),
                detail: "Continue unlocks when it arrives.".into(),
            })
        );
        assert_eq!(test.next, Some(Press::new("Continue").enabled(false)));
        assert_eq!(test.skip, Some(Press::new("Skip the test")));
        let found = pos_step_of(
            PosSetup {
                test_sale: Some("$1.00".into()),
                ..square_connected()
            },
            Device::Ipad,
        );
        assert!(found.status.unwrap().found);
        assert_eq!(found.next, Some(Press::new("Continue")));

        let settings = pos_step_of(
            PosSetup {
                setup: false,
                ..setup(Some(PointOfSale::Square))
            },
            Device::Ipad,
        );
        assert_eq!(settings.stage, None, "Settings shows choice and card");
        assert!(settings.card.is_some());
    }

    /// Decision 130: O1–O3's headings and leads, as the boards word them,
    /// and O3's connected line.
    #[test]
    fn step_4s_stages_carry_their_headings_and_leads() {
        let words = |s: PosSetup| {
            let chosen = s.chosen;
            let view = show(Step::PointOfSale(s), chosen, Device::Ipad);
            (view.heading, view.lead, view.section)
        };
        assert_eq!(
            words(PosSetup {
                confirmed: false,
                ..setup(Some(PointOfSale::Square))
            }),
            (
                "Which point of sale do you use?".into(),
                "Till signs in to it to see each Monero sale the moment you ring it up.".into(),
                String::new()
            )
        );
        assert_eq!(
            words(setup(Some(PointOfSale::Clover))),
            (
                "Sign in to Clover".into(),
                "Clover's own page opens. Sign in as the business owner and allow Till.".into(),
                String::new()
            )
        );
        assert_eq!(
            words(square_connected()),
            (
                "Try a test sale".into(),
                "This checks that Square tells Till about Monero sales. It takes about a minute."
                    .into(),
                String::new()
            )
        );
        assert_eq!(
            words(PosSetup {
                setup: false,
                ..setup(Some(PointOfSale::Square))
            }),
            (
                "Connect your point of sale".into(),
                String::new(),
                "Point of sale".into()
            )
        );
        assert_eq!(
            pos_step_of(square_connected(), Device::Ipad)
                .connected
                .as_deref(),
            Some("Connected to The Bean Room Ltd · Front Counter")
        );
        assert_eq!(
            pos_step_of(clover_connected(CloverTender::Ready), Device::Ipad)
                .connected
                .as_deref(),
            Some("Connected to The Bean Room")
        );
        assert_eq!(
            pos_step_of(setup(Some(PointOfSale::Square)), Device::Ipad).connected,
            None,
            "only O3 has it"
        );
    }

    #[test]
    fn after_the_test_sale_c4_names_its_device_and_asks_which_sales_alert() {
        let view = match page(
            Step::Counter(CounterSetup {
                amount: "$1.00".into(),
                number: Some("#t0Ab".into()),
                device: "Front iPad".into(),
                place: Some("Front Counter".into()),
                every_device: false,
            }),
            Some(PointOfSale::Square),
            Device::Ipad,
        ) {
            OnboardingPage::Counter { view } => view,
            other => panic!("{other:?}"),
        };
        assert_eq!(view.found, "Test sale found · $1.00 · Sale #t0Ab");
        assert_eq!(view.heading, "Which sales should alert this iPad?");
        assert_eq!(view.lead, "The test sale was rung up on Front iPad.");
        assert_eq!(
            view.choices
                .iter()
                .map(|c| (c.label.as_str(), c.selected))
                .collect::<Vec<_>>(),
            [
                ("Only Front iPad", true),
                ("Every sale at Front Counter", false),
            ]
        );
        assert_eq!(view.footnote, "Change this any time in Settings.");
    }

    #[test]
    fn o4_asks_how_a_new_sale_shows_up_and_offers_a_test_alert() {
        let view = match page(
            Step::ShowSales {
                chosen: ShowSales::SideBySide,
                currency: "USD".into(),
            },
            Some(PointOfSale::Square),
            Device::Ipad,
        ) {
            OnboardingPage::ShowSales { view } => view,
            other => panic!("{other:?}"),
        };
        assert_eq!(view.heading, "How should a new sale show up?");
        assert_eq!(
            view.lead,
            "Keep using Square as normal. You can change this any time in Settings."
        );
        assert_eq!(
            view.choices
                .iter()
                .map(|c| (c.mode, c.choice.label.as_str(), c.choice.selected))
                .collect::<Vec<_>>(),
            [
                (ShowSales::Alert, "An alert (recommended)", false),
                (ShowSales::SideBySide, "Side by side", true),
            ]
        );
        assert_eq!(view.side_by_side.unwrap().label, "On iPadOS 26");
        assert_eq!(view.test_alert, Press::new("Send a test alert"));
        assert_eq!(view.sample.title, "Sam · $24.50");
        assert_eq!(view.sample.body, "Monero sale. Tap to show Sam the code.");
    }
}
