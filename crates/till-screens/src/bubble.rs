//! The bubble over other apps and its panel (Android only: iOS has no
//! bubble), and which surface buzzes when a payment lands (Android:
//! `SaleText.kt`'s `Bubble` and `PaidBuzz`, `overlay/Faces.kt`'s `Panel`
//! words).

use monokulo_client::{MoneyFormat, PointOfSale, SalePhase};
use till_sales::{Standing, TillSale};

use crate::order_text::{OrderPageView, order_page};
use crate::sale::{SaleInput, SaleView};
use crate::time::DeviceClock;
use crate::view::Device;
use crate::{code, pos_text, sale, sale_text};

/// What the bubble shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum BubbleLook {
    Idle,
    Lit,
    Paid,
}

/// What a tap on the bubble does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum BubbleTap {
    /// No sale to show: Till itself comes to the front, as its launcher
    /// icon would bring it.
    OpenApp,
    /// The panel opens on the bubble's sale.
    OpenPanel,
    /// The panel goes back into the bubble.
    LowerPanel,
}

/// Green for a few seconds after a sale is paid, then lit while a sale's
/// code is up, else faded.
pub fn look(sales: &[TillSale], just_paid: bool) -> BubbleLook {
    if just_paid {
        BubbleLook::Paid
    } else if sales.iter().any(TillSale::live) {
        BubbleLook::Lit
    } else {
        BubbleLook::Idle
    }
}

/// Which sale the bubble and its panel are about: the one just paid, while
/// it shows as paid; else the newest whose code is up; else the last one
/// they showed.
pub fn focus(sales: &[TillSale], previous: Option<&str>, just_paid: bool) -> Option<String> {
    let shown = sales
        .iter()
        .find(|sale| Some(sale.order_id.as_str()) == previous);
    if just_paid && shown.is_some_and(|sale| sale.phase == SalePhase::Paid) {
        return previous.map(Into::into);
    }
    sales
        .iter()
        .find(|sale| sale.live())
        .map(|sale| sale.order_id.clone())
        .or_else(|| previous.map(Into::into))
}

/// What a tap does, with `sale` the one the bubble is about ([`focus`])
/// and `panel_open` whether its panel is up or on its way. The panel only
/// shows a sale, so with none the bubble opens Till instead of a panel that
/// would never appear.
pub fn tap(sale: Option<&TillSale>, panel_open: bool) -> BubbleTap {
    match (sale, panel_open) {
        (None, _) => BubbleTap::OpenApp,
        (Some(_), true) => BubbleTap::LowerPanel,
        (Some(_), false) => BubbleTap::OpenPanel,
    }
}

/// What a screen reader says a tap does ("double-tap to …").
pub fn tap_label(tap: BubbleTap) -> &'static str {
    match tap {
        BubbleTap::OpenApp => "Open Till",
        BubbleTap::OpenPanel => "Show the sale",
        BubbleTap::LowerPanel => "Lower to the bubble",
    }
}

/// What a screen reader says for the bubble (`docs/motion.md` §12.5): it
/// no longer shows a sale's step, so its label carries the amount and the
/// step, in the words the shift and the panel use, for `sale`, the one it
/// is about ([`focus`]).
pub fn label(look: BubbleLook, sale: Option<&TillSale>, money: &dyn MoneyFormat) -> String {
    match (look, sale) {
        (BubbleLook::Idle, _) => "Till, nothing waiting".into(),
        (BubbleLook::Paid, None) => "Till, paid".into(),
        (_, None) => "Till, a sale waiting for payment".into(),
        (BubbleLook::Paid, Some(sale)) => format!("Till, {} paid", sale_text::amount(sale, money)),
        (BubbleLook::Lit, Some(sale)) => {
            let words = if sale.live() {
                sale_text::status(sale, None)
            } else {
                "Waiting for payment".into()
            };
            format!(
                "Till, {} {}",
                sale_text::amount(sale, money),
                lower_first(&words)
            )
        }
    }
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The surface that buzzes when a payment lands (`docs/motion.md` §6): the
/// one the cashier is looking at, and it alone, so a payment is one buzz
/// however many surfaces show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum BuzzSurface {
    App,
    Bubble,
}

/// The app when it is on screen (any of its screens), else the bubble.
pub fn buzz_by(app_visible: bool) -> BuzzSurface {
    if app_visible {
        BuzzSurface::App
    } else {
        BuzzSurface::Bubble
    }
}

/// The bubble's side of the rule: whether a payment landing on it buzzes.
pub fn bubble_buzzes(app_visible: bool) -> bool {
    buzz_by(app_visible) == BuzzSurface::Bubble
}

/// The panel header's second line before "Open the shift": the till's
/// location for its own sale, else who took it.
pub fn panel_where(sale: &TillSale, location: Option<&str>) -> String {
    match (&sale.found, location) {
        (Some(_), Some(location)) => location.into(),
        (Some(_), None) => "This till".into(),
        (None, _) => sale_text::place(sale, location),
    }
}

/// The heading of a sale whose code came down while the panel was up
/// (acceptance P15), at a till on `till`.
pub fn panel_ended_heading(sale: &TillSale, till: Option<PointOfSale>) -> String {
    if !sale.refunded() && sale.phase == SalePhase::DoubleSpend {
        "Payment reversed · don't hand it over".into()
    } else {
        sale_text::status(sale, till)
    }
}

/// What the cashier does about it: refund it from the shift, or on the till
/// that took it.
pub fn panel_ended_line(sale: &TillSale, till: Option<PointOfSale>) -> String {
    if sale.refunded() {
        "Nothing more to do.".into()
    } else if sale.found.is_none() {
        pos_text::refund_on_other_till(
            till,
            sale.device_name
                .as_deref()
                .unwrap_or("the till that took it"),
        )
    } else {
        format!("Open the shift to {}.", pos_text::refund_it_in(till))
    }
}

/// Whether the bubble is up at all (android.md decision 208): the cashier
/// wants it (`wanted`: the bubble's switch on and a shift on), Android
/// allows drawing over other apps, the till is running and still
/// connected, and Till itself isn't on screen. A stream or poll that fails
/// leaves `running` and `disconnected` as they were (only the store's 401
/// disconnects the till), so a slow connection never lowers a raised panel.
pub fn shown(
    wanted: bool,
    allowed: bool,
    running: bool,
    disconnected: bool,
    app_visible: bool,
) -> bool {
    wanted && allowed && running && !disconnected && !app_visible
}

/// Whether the bubble pulses: lit for a code still waiting for its money
/// (waiting, or part paid), and only the free bubble with the panel down;
/// the pulse calls the cashier to raise the panel, so once it's up, or the
/// payment is seen, there's nothing to call for.
pub fn pulses(look: BubbleLook, phase: SalePhase, panel_up: bool) -> bool {
    look == BubbleLook::Lit && !panel_up && matches!(phase, SalePhase::Waiting | SalePhase::Partial)
}

/// Where the bubble's paid moment is (`docs/motion.md` M9, android.md
/// decision 158): held for the paid dwell after a payment lands, the bubble
/// green and the panel showing the sale paid; then, with the panel still
/// up on that sale and no other code waiting, the panel collapses into the
/// green bubble, which fades to lit or idle only once the collapse has
/// landed. The engine keeps it; Android only animates the collapse it asks
/// for and says when the panel has gone (decision 95 of core.md).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaidMoment {
    #[default]
    None,
    /// The dwell: green, and the panel holds the paid sale.
    Held,
    /// The dwell, the panel lowered meanwhile (Done, the grabber, a tap
    /// beside it): green to the dwell's end, with no collapse then.
    HeldDown,
    /// The dwell is over and the panel collapses into the still green
    /// bubble.
    Lowering,
}

impl PaidMoment {
    /// Whether the bubble is green and the panel holds the paid sale.
    pub fn paid(self) -> bool {
        self != PaidMoment::None
    }
}

/// What moves a paid moment on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaidEvent {
    /// A payment landed.
    Paid,
    /// The paid dwell went by.
    DwellEnded,
    /// The panel went back into the bubble: its collapse landed, or it went
    /// without one (Android's `Action::PanelShown { up: false }`).
    PanelDown,
}

/// The next paid moment, and whether the panel is to collapse now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaidStep {
    pub moment: PaidMoment,
    pub lower: bool,
}

impl From<PaidMoment> for PaidStep {
    fn from(moment: PaidMoment) -> PaidStep {
        PaidStep {
            moment,
            lower: false,
        }
    }
}

/// The moment after `event`, with `on_panel` whether the panel is up on the
/// sale just paid, and `next_waiting` whether another sale's code is up.
pub fn paid_next(
    moment: PaidMoment,
    event: PaidEvent,
    on_panel: bool,
    next_waiting: bool,
) -> PaidStep {
    use PaidMoment::*;
    match (event, moment) {
        (PaidEvent::Paid, _) => Held.into(),
        (PaidEvent::DwellEnded, Held) if on_panel && !next_waiting => PaidStep {
            moment: Lowering,
            lower: true,
        },
        (PaidEvent::DwellEnded, Held | HeldDown) => None.into(),
        (PaidEvent::PanelDown, Held) => HeldDown.into(),
        (PaidEvent::PanelDown, Lowering) => None.into(),
        (_, moment) => moment.into(),
    }
}

/// What the panel's middle shows (Android's `PanelFace` and the folded
/// panel's hero): the code while the sale is live, else a hero whose look
/// says how it ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PanelHero {
    /// The code: the sale is live.
    Code,
    /// The paid tick (paid late, or after a refund, too).
    Paid,
    /// Refunded in the point of sale, nothing paid.
    Refunded,
    /// The payment was spent twice: the warning.
    Reversed,
    /// Ended unpaid otherwise (its code ran out, the order was cancelled,
    /// or the cashier closed it): the clock.
    Expired,
}

/// `sale`'s hero.
pub fn hero(sale: &TillSale) -> PanelHero {
    match sale.standing() {
        Standing::Live(_) => PanelHero::Code,
        Standing::Paid { .. } => PanelHero::Paid,
        _ if sale.refunded() => PanelHero::Refunded,
        _ if sale.phase == SalePhase::DoubleSpend => PanelHero::Reversed,
        _ => PanelHero::Expired,
    }
}

/// What the bubble's panel shows about its sale.
#[derive(Clone, Debug)]
pub struct PanelInput {
    pub sale: TillSale,
    /// The till's point of sale now.
    pub till: Option<PointOfSale>,
    /// The till's Square location or Clover merchant.
    pub location: Option<String>,
}

/// The bubble's panel, ready to draw: the sale as its live screen shows it,
/// and what the panel draws beside that.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PanelView {
    pub order_id: String,
    /// The sale, as its live screen shows it.
    pub sale: SaleView,
    /// The header's second line: the till's location, or who took it.
    pub where_from: String,
    /// The header's time: when the sale was made ("4:07 PM"); `None` when
    /// that isn't known.
    pub created: Option<String>,
    /// "14:52 left", beside the status, while the code counts down; else
    /// `None`. (The sale's `hint` says more, for the sale screen.)
    pub time_left: Option<String>,
    pub hero: PanelHero,
    /// The whole order, every line: the panel's order page, as the app's
    /// own order page draws it. (The ring, confirmations and all, is the
    /// sale's `ring`.)
    pub order: Option<OrderPageView>,
    /// The code came down unpaid while the panel was up (acceptance P15):
    /// its heading, and what the cashier does about it.
    pub ended_heading: Option<String>,
    pub ended_line: Option<String>,
    /// The countdown is on screen: draw it again in a second.
    pub ticks: bool,
}

/// The panel at the clock's now, on `device`, amounts written by `money`.
pub fn panel(
    input: &PanelInput,
    device: Device,
    clock: DeviceClock,
    money: &dyn MoneyFormat,
) -> PanelView {
    let s = &input.sale;
    let view = sale::sale(
        &SaleInput {
            sale: s.clone(),
            till: input.till,
            location: input.location.clone(),
        },
        device,
        clock,
        money,
    );
    let ended = !s.live() && s.phase != SalePhase::Paid;
    let seconds = sale_text::seconds_left(s.expires_at, clock.now);
    PanelView {
        order_id: s.order_id.clone(),
        where_from: panel_where(s, input.location.as_deref()),
        created: (s.created_at > 0).then(|| sale_text::time_of(s.created_at, clock)),
        time_left: s.live().then(|| code::left(s, seconds)).flatten(),
        hero: hero(s),
        order: order_page(s, pos_text::of(s, input.till), clock),
        ended_heading: ended.then(|| panel_ended_heading(s, input.till)),
        ended_line: ended.then(|| panel_ended_line(s, input.till)),
        ticks: view.ticks,
        sale: view,
    }
}

/// Ported from Android's `SaleTextTest` (the bubble's four) and
/// `ShiftListTest` (`aPaymentBuzzesOnceOnTheSurfaceTheCashierIsLookingAt`).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{found, money, till_sale};

    fn sale(id: &str, phase: SalePhase, closed_at: Option<i64>) -> TillSale {
        TillSale {
            reference: Some("#1042".into()),
            phase,
            found: Some(found(
                "p1",
                2450,
                "USD",
                Some("#1042"),
                "ord",
                "2026-10-23T10:00:00Z",
            )),
            closed_at,
            ..till_sale(id, "24.50", "USD", "0.1421")
        }
    }

    fn of(phase: SalePhase) -> TillSale {
        sale("o1", phase, None)
    }

    #[test]
    fn the_bubble_is_lit_while_a_sales_code_is_up_green_just_after_one_is_paid_else_faded() {
        use SalePhase::*;
        assert_eq!(look(&[], false), BubbleLook::Idle);
        assert_eq!(look(&[of(Seen)], false), BubbleLook::Lit);
        assert_eq!(
            look(&[sale("o1", Waiting, Some(1))], false),
            BubbleLook::Idle,
            "a closed sale's code is down"
        );
        assert_eq!(
            look(&[of(Seen)], true),
            BubbleLook::Paid,
            "paid shows first, even with another sale waiting"
        );
        assert_eq!(look(&[of(Paid)], true), BubbleLook::Paid);
        assert_eq!(look(&[of(Paid)], false), BubbleLook::Idle);
    }

    #[test]
    fn the_bubbles_label_says_the_amount_and_the_step_it_no_longer_shows() {
        use SalePhase::*;
        let amount = sale_text::amount(&of(Waiting), &money);
        let l = |look, sale: Option<TillSale>| label(look, sale.as_ref(), &money);
        assert_eq!(
            l(BubbleLook::Lit, Some(of(Waiting))),
            format!("Till, {amount} waiting for payment")
        );
        assert_eq!(
            l(BubbleLook::Lit, Some(of(Partial))),
            format!("Till, {amount} part paid")
        );
        assert_eq!(
            l(BubbleLook::Lit, Some(of(Seen))),
            format!("Till, {amount} seen on the network")
        );
        assert_eq!(
            l(BubbleLook::Lit, Some(of(Confirming))),
            format!("Till, {amount} confirming")
        );
        assert_eq!(
            l(BubbleLook::Paid, Some(of(Paid))),
            format!("Till, {amount} paid")
        );
        assert_eq!(l(BubbleLook::Idle, Some(of(Paid))), "Till, nothing waiting");
        assert_eq!(l(BubbleLook::Idle, None), "Till, nothing waiting");
    }

    #[test]
    fn a_tap_on_the_bubble_with_no_sale_to_show_opens_till_and_with_one_opens_or_lowers_the_panel()
    {
        // No sale this shift yet: the panel has nothing to show, so the app
        // comes up instead, panel wanted or not.
        assert_eq!(focus(&[], None, false), None);
        assert_eq!(tap(None, false), BubbleTap::OpenApp);
        assert_eq!(tap(None, true), BubbleTap::OpenApp);
        assert_eq!(tap_label(BubbleTap::OpenApp), "Open Till");
        // A sale waiting: the tap toggles the panel, as before.
        let waiting = [of(SalePhase::Waiting)];
        let id = focus(&waiting, None, false);
        let focused = waiting
            .iter()
            .find(|sale| Some(&sale.order_id) == id.as_ref());
        assert_eq!(tap(focused, false), BubbleTap::OpenPanel);
        assert_eq!(tap(focused, true), BubbleTap::LowerPanel);
        // The last sale ended: the bubble is idle but still about it, so the
        // panel shows it ended.
        let ended = sale("o1", SalePhase::Expired, Some(1));
        assert_eq!(tap(Some(&ended), false), BubbleTap::OpenPanel);
        assert_eq!(tap_label(BubbleTap::LowerPanel), "Lower to the bubble");
    }

    #[test]
    fn the_panel_stays_on_a_sale_while_it_shows_as_paid_then_moves_to_the_next_one_waiting() {
        use SalePhase::*;
        let waiting = sale("older", Waiting, None);
        let paid = sale("newer", Paid, None);
        let both = [paid.clone(), waiting.clone()];
        assert_eq!(focus(&both, Some("newer"), true).as_deref(), Some("newer"));
        assert_eq!(focus(&both, Some("newer"), false).as_deref(), Some("older"));
        assert_eq!(
            focus(
                &[sale("newer", Waiting, None), waiting.clone()],
                Some("older"),
                false
            )
            .as_deref(),
            Some("newer"),
            "a newer sale takes over"
        );
        assert_eq!(
            focus(
                &[sale("newer", Waiting, Some(1)), waiting.clone()],
                Some("older"),
                false
            )
            .as_deref(),
            Some("older"),
            "not a closed one"
        );
        assert_eq!(focus(&[], Some("gone"), false).as_deref(), Some("gone"));
    }

    #[test]
    fn a_payment_buzzes_once_on_the_surface_the_cashier_is_looking_at() {
        assert_eq!(
            buzz_by(true),
            BuzzSurface::App,
            "the app on screen: the sale turning paid, or its row"
        );
        assert_eq!(
            buzz_by(false),
            BuzzSurface::Bubble,
            "over Square: the bubble"
        );
        // One surface for any answer, so a payment shown by the sale screen
        // and a bubble still leaving buzzes once.
        for visible in [true, false] {
            let surfaces = [BuzzSurface::App, BuzzSurface::Bubble];
            assert_eq!(
                surfaces.iter().filter(|&&s| s == buzz_by(visible)).count(),
                1
            );
        }
        assert!(bubble_buzzes(false));
        assert!(
            !bubble_buzzes(true),
            "the sale screen in front buzzes instead"
        );
    }
}

/// Ported from Android's `PanelTest`: its words, where the sale is from,
/// the ended sale's rule, the pulse, the hero and whether the overlay is
/// up. Its geometry and motion (the raise, the grabber, the windows'
/// handoff, the collapse) stay with Android's drawing; the paid moment is
/// the engine's now, its three tests below.
#[cfg(test)]
mod panel_tests {
    use super::*;
    use crate::fixtures::{found, instant, la, money, receipt, till_sale};
    use crate::shift::tests::at;

    const SQUARE: Option<PointOfSale> = Some(PointOfSale::Square);

    /// The Kotlin test's `sale(...)`: $79.21, this till's unless `found` is
    /// false, then `device`'s.
    fn sale(phase: SalePhase, found_here: bool, device: Option<&str>, id: &str) -> TillSale {
        TillSale {
            reference: Some("k4Rt".into()),
            phase,
            found: found_here.then(|| {
                found(
                    "p1",
                    7921,
                    "USD",
                    Some("k4Rt"),
                    "ord",
                    "2026-10-22T23:07:00Z",
                )
            }),
            device_name: if found_here {
                None
            } else {
                device.map(Into::into)
            },
            ..till_sale(id, "79.21", "USD", "0.31684")
        }
    }

    fn of(phase: SalePhase) -> TillSale {
        sale(phase, true, Some("Back till"), "o1")
    }

    fn refunded(phase: SalePhase) -> TillSale {
        TillSale {
            refunded_at: Some(9),
            ..of(phase)
        }
    }

    #[test]
    fn the_header_says_where_the_sale_is_from() {
        assert_eq!(
            panel_where(&of(SalePhase::Waiting), Some("Front Counter")),
            "Front Counter"
        );
        assert_eq!(panel_where(&of(SalePhase::Waiting), None), "This till");
        assert_eq!(
            panel_where(
                &sale(SalePhase::Waiting, false, Some("Back till"), "o1"),
                Some("Front Counter")
            ),
            "From Back till"
        );
        assert_eq!(
            panel_where(
                &sale(SalePhase::Waiting, false, None, "o1"),
                Some("Front Counter")
            ),
            "From another till"
        );
    }

    #[test]
    fn a_sale_that_ends_while_the_panel_is_up_sends_the_cashier_to_refund_it() {
        use SalePhase::*;
        let other = |device| sale(Expired, false, device, "o1");
        assert_eq!(
            panel_ended_heading(&of(Expired), SQUARE),
            "Code expired · not paid"
        );
        assert_eq!(
            panel_ended_heading(&of(Cancelled), SQUARE),
            "Cancelled · not paid"
        );
        assert_eq!(
            panel_ended_heading(&of(DoubleSpend), SQUARE),
            "Payment reversed · don't hand it over"
        );
        assert_eq!(
            panel_ended_heading(&sale(DoubleSpend, false, Some("Back till"), "o1"), SQUARE),
            "Payment reversed · don't hand it over"
        );
        assert_eq!(
            panel_ended_line(&of(Expired), SQUARE),
            "Open the shift to refund it in Square."
        );
        assert_eq!(
            panel_ended_line(&other(Some("Back till")), SQUARE),
            "Refund it in Square on Back till."
        );
        assert_eq!(
            panel_ended_line(&other(None), SQUARE),
            "Refund it in Square on the till that took it."
        );
        assert_eq!(
            panel_ended_heading(&refunded(DoubleSpend), SQUARE),
            "Refunded in Square"
        );
        assert_eq!(
            panel_ended_line(&refunded(Expired), SQUARE),
            "Nothing more to do."
        );
    }

    #[test]
    fn the_sale_on_the_panel_shows_paid_before_the_next_one_waiting_comes_up() {
        use SalePhase::*;
        let waiting = [
            sale(Waiting, true, None, "1044"),
            sale(Waiting, true, None, "1043"),
        ];
        assert_eq!(
            focus(&waiting, None, false).as_deref(),
            Some("1044"),
            "the newest is up"
        );
        let paid = [
            sale(Paid, true, None, "1044"),
            sale(Waiting, true, None, "1043"),
        ];
        assert_eq!(
            focus(&paid, Some("1044"), true).as_deref(),
            Some("1044"),
            "held, paid, for a few seconds"
        );
        assert_eq!(
            focus(&paid, Some("1044"), false).as_deref(),
            Some("1043"),
            "then the next one waiting"
        );
    }

    /// Ported from Android's `PanelTest` (the paid moment's three), which
    /// the overlay ran on its own timer before the engine owned it.
    #[test]
    fn a_paid_moment_is_held_for_the_dwell_then_the_panel_collapses_into_the_green_bubble_which_fades_once_it_lands()
     {
        use PaidMoment::*;
        let held = paid_next(None, PaidEvent::Paid, true, false);
        assert_eq!(held, Held.into());
        assert!(held.moment.paid(), "the bubble is green");
        let dwelt = paid_next(Held, PaidEvent::DwellEnded, true, false);
        assert_eq!(
            dwelt,
            PaidStep {
                moment: Lowering,
                lower: true
            },
            "the panel collapses"
        );
        assert!(dwelt.moment.paid(), "into a bubble still green");
        assert_eq!(
            paid_next(Lowering, PaidEvent::PanelDown, false, false),
            None.into()
        );
    }

    #[test]
    fn done_during_the_dwell_keeps_the_bubble_green_to_its_end_without_a_second_collapse() {
        use PaidMoment::*;
        let done = paid_next(Held, PaidEvent::PanelDown, false, false);
        assert_eq!(done, HeldDown.into(), "the cashier's lower is the collapse");
        assert!(done.moment.paid(), "still green till the dwell ends");
        assert_eq!(
            paid_next(HeldDown, PaidEvent::DwellEnded, true, false),
            None.into(),
            "raised again meanwhile: no collapse the cashier didn't ask for"
        );
        assert_eq!(
            paid_next(HeldDown, PaidEvent::PanelDown, false, false),
            HeldDown.into()
        );
    }

    #[test]
    fn with_another_sale_waiting_or_no_panel_up_the_moment_ends_at_the_dwell_without_a_collapse() {
        use PaidMoment::*;
        assert_eq!(
            paid_next(Held, PaidEvent::DwellEnded, true, true),
            None.into(),
            "the next code comes up in the panel"
        );
        assert_eq!(
            paid_next(Held, PaidEvent::DwellEnded, false, false),
            None.into(),
            "the bubble alone: green for the dwell, then it fades"
        );
        assert_eq!(
            paid_next(Lowering, PaidEvent::Paid, false, false),
            Held.into(),
            "another payment mid-collapse: green again"
        );
        assert_eq!(
            paid_next(Lowering, PaidEvent::DwellEnded, false, false),
            Lowering.into(),
            "a late dwell changes nothing"
        );
        assert_eq!(
            paid_next(None, PaidEvent::PanelDown, false, false),
            None.into(),
            "a collapse with nothing paid"
        );
        assert_eq!(
            paid_next(None, PaidEvent::DwellEnded, true, false),
            None.into()
        );
    }

    #[test]
    fn the_free_bubble_pulses_only_while_a_code_waits_for_its_money_and_the_panel_is_down() {
        use SalePhase::*;
        assert!(pulses(BubbleLook::Lit, Waiting, false));
        assert!(
            pulses(BubbleLook::Lit, Partial, false),
            "part paid, the rest still to come"
        );
        assert!(
            !pulses(BubbleLook::Lit, Waiting, true),
            "docked above the panel: nothing to call for"
        );
        assert!(
            !pulses(BubbleLook::Lit, Seen, false),
            "the payment is on its way"
        );
        assert!(!pulses(BubbleLook::Lit, Confirming, false));
        assert!(!pulses(BubbleLook::Paid, Paid, false));
        assert!(!pulses(BubbleLook::Idle, Waiting, false));
    }

    #[test]
    fn the_panel_shows_the_code_while_live_the_tick_once_paid_and_how_it_ended_otherwise() {
        use SalePhase::*;
        assert_eq!(hero(&of(Waiting)), PanelHero::Code);
        assert_eq!(hero(&of(Confirming)), PanelHero::Code);
        assert_eq!(hero(&of(Paid)), PanelHero::Paid);
        assert_eq!(hero(&of(Expired)), PanelHero::Expired);
        assert_eq!(hero(&of(Cancelled)), PanelHero::Expired);
        assert_eq!(
            hero(&TillSale {
                closed_at: Some(1),
                ..of(Waiting)
            }),
            PanelHero::Expired,
            "closed for now, unpaid"
        );
        assert_eq!(hero(&of(DoubleSpend)), PanelHero::Reversed);
        assert_eq!(hero(&refunded(Expired)), PanelHero::Refunded);
        assert_eq!(hero(&refunded(DoubleSpend)), PanelHero::Refunded);
    }

    #[test]
    fn a_failed_poll_or_a_dropped_stream_leaves_the_overlay_up_but_a_disconnect_lowers_it() {
        // wanted, allowed, running, disconnected, app visible
        assert!(shown(true, true, true, false, false));
        assert!(
            !shown(true, true, true, true, false),
            "the store's 401 lowers it"
        );
        assert!(
            !shown(true, true, true, false, true),
            "Till on screen lowers it at once"
        );
        assert!(!shown(true, true, false, false, false));
        assert!(!shown(false, true, true, false, false));
        assert!(!shown(true, false, true, false, false));
    }

    #[test]
    fn the_panel_carries_the_ring_the_time_left_the_hero_and_the_whole_order() {
        let order = receipt(
            "k4Rt",
            "2026-10-22T16:07:00-07:00",
            &[
                ("Flat white", 2.0, 4.5, 9.0),
                ("Oat latte", 1.0, 5.25, 5.25),
                ("Almond croissant", 3.0, 3.75, 11.25),
                ("Banana bread", 1.0, 4.5, 4.5),
                ("Cold brew", 2.0, 5.0, 10.0),
            ],
            &[],
            40.0,
            "USD",
        );
        let waiting = TillSale {
            expires_at: at() + 892,
            created_at: instant("2026-10-22T23:07:00Z"),
            receipt: Some(order),
            ..of(SalePhase::Waiting)
        };
        let input = |sale| PanelInput {
            sale,
            till: SQUARE,
            location: Some("Front Counter".into()),
        };
        let view = panel(&input(waiting.clone()), Device::Android, la(at()), &money);
        assert_eq!(view.order_id, "o1");
        assert_eq!(view.where_from, "Front Counter");
        assert_eq!(view.created.as_deref(), Some("4:07 PM"));
        assert_eq!(view.time_left.as_deref(), Some("14:52 left"));
        assert_eq!(view.sale.hint, "Scan with any Monero wallet · 14:52 left");
        assert_eq!(view.hero, PanelHero::Code);
        let order = view.order.unwrap();
        assert_eq!(order.items, "9 items");
        assert_eq!(order.rows.len(), 5, "every line, not the live card's three");
        assert!(
            view.sale.order.unwrap().more.is_some(),
            "the sale's own card folds"
        );
        assert_eq!((view.ended_heading, view.ended_line), (None, None));
        assert!(view.ticks);

        let confirming = TillSale {
            phase: SalePhase::Confirming,
            confirmations: Some(4),
            confirmations_required: Some(10),
            ..waiting.clone()
        };
        let view = panel(&input(confirming), Device::Android, la(at()), &money);
        assert_eq!(
            (view.sale.ring.confirmations, view.sale.ring.required),
            (Some(4), Some(10))
        );
        assert_eq!(
            view.time_left, None,
            "the payment is on its way: no countdown"
        );
        assert!(!view.ticks);

        let expired = TillSale {
            phase: SalePhase::Expired,
            created_at: 0,
            ..waiting
        };
        let view = panel(&input(expired), Device::Android, la(at()), &money);
        assert_eq!(view.created, None, "no time when it isn't known");
        assert_eq!(view.time_left, None);
        assert_eq!(view.hero, PanelHero::Expired);
        assert_eq!(
            view.ended_heading.as_deref(),
            Some("Code expired · not paid")
        );
        assert_eq!(
            view.ended_line.as_deref(),
            Some("Open the shift to refund it in Square.")
        );
    }
}
