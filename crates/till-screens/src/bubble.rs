//! The bubble over other apps and its panel (Android only: iOS has no
//! bubble), and which surface buzzes when a payment lands (Android:
//! `SaleText.kt`'s `Bubble` and `PaidBuzz`, `overlay/Faces.kt`'s `Panel`
//! words).

use monokulo_client::{MoneyFormat, PointOfSale, SalePhase};
use till_sales::TillSale;

use crate::{pos_text, sale_text};

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
