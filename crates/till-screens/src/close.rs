//! The close sheet, "Is the customer paying?" (boards PhoneClose and
//! PadClose, the design's CloseAsk; decisions 62 and 70 of android.md;
//! Android: `SalePage.kt`'s `Closing`, `ui/CloseSheet.kt`'s words). The
//! same on every device.

use monokulo_client::{MoneyFormat, PointOfSale, SalePhase};
use till_sales::{Standing, TillSale};

use crate::time::DeviceClock;
use crate::view::{Ring, ring};
use crate::{pos_text, sale_text};

/// The close sheet's three answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CloseChoice {
    RefundIt,
    CloseForNow,
    KeepUp,
}

impl CloseChoice {
    pub const ALL: [CloseChoice; 3] = [
        CloseChoice::RefundIt,
        CloseChoice::CloseForNow,
        CloseChoice::KeepUp,
    ];
}

/// Where the cashier lands after an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CloseThen {
    /// Stay on the sale, which shows its page now (Refunded, or Closed when
    /// the refund failed).
    Sale,
    Shift,
    /// Nothing happens: the sheet goes and the code stays up.
    Stay,
}

/// What an answer does: close the sale, refund it, and where the cashier
/// lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CloseOutcome {
    pub close: bool,
    pub refund: bool,
    pub then: CloseThen,
}

/// The answers offered: all three for this till's sale; for another
/// till's, or one rung up at a point of sale this till has changed from
/// (`elsewhere`), "No: close it" and "Yes" (this till can't refund it, so
/// "No" and "Not sure" would be the same); none once it's refunded or its
/// code is down, where the sale screen has no X.
pub fn choices(sale: &TillSale, elsewhere: bool) -> Vec<CloseChoice> {
    if !matches!(sale.standing(), Standing::Live(_)) {
        Vec::new()
    } else if sale.found.is_none() || elsewhere {
        vec![CloseChoice::CloseForNow, CloseChoice::KeepUp]
    } else {
        CloseChoice::ALL.to_vec()
    }
}

/// Whether the sheet's answer still stands for `sale` as it is now: only
/// while its code is up. Paid (or ended) since the cashier tapped, the
/// answer is dropped, so a paid sale is never closed or refunded by it.
pub fn applies(sale: Option<&TillSale>) -> bool {
    sale.is_some_and(TillSale::live)
}

pub fn outcome(choice: CloseChoice) -> CloseOutcome {
    match choice {
        CloseChoice::RefundIt => CloseOutcome {
            close: true,
            refund: true,
            then: CloseThen::Sale,
        },
        CloseChoice::CloseForNow => CloseOutcome {
            close: true,
            refund: false,
            then: CloseThen::Shift,
        },
        CloseChoice::KeepUp => CloseOutcome {
            close: false,
            refund: false,
            then: CloseThen::Stay,
        },
    }
}

/// An answer's words for a sale of `pos`: another till's sale's "No" only
/// closes it.
pub fn title(choice: CloseChoice, sale: &TillSale, pos: Option<PointOfSale>) -> String {
    match choice {
        CloseChoice::RefundIt => format!("No: {}", pos_text::refund_it_in(pos)),
        CloseChoice::CloseForNow if sale.found.is_none() => "No: close it".into(),
        CloseChoice::CloseForNow => "Not sure: close it for now".into(),
        CloseChoice::KeepUp => "Yes: keep the code up".into(),
    }
}

/// Whether "No" on a sale of `pos` leaves the refund to the cashier there,
/// with the refund sheet: `pos` isn't the point of sale the till's `source`
/// watches (the cashier changed it since, or there's none), or `source`'s
/// API can't refund (Clover, decision 130 of android.md).
pub fn refunds_there(pos: Option<PointOfSale>, source: Option<PointOfSale>) -> bool {
    pos != source
        || match source {
            Some(PointOfSale::Square) => false,
            Some(PointOfSale::Clover) | None => true,
        }
}

/// The line under an answer, with `seconds_left` on the code for "Yes";
/// `pos` is the sale's point of sale, `source` the one the till's source
/// watches ([`refunds_there`]).
pub fn detail(
    choice: CloseChoice,
    sale: &TillSale,
    pos: Option<PointOfSale>,
    source: impl Into<Option<PointOfSale>>,
    seconds_left: i64,
) -> String {
    match choice {
        CloseChoice::RefundIt if refunds_there(pos, source.into()) => format!(
            "Till closes it and shows you how to refund it in {}.",
            pos_text::name(pos)
        ),
        CloseChoice::RefundIt => "Till refunds the sale and closes it.".into(),
        CloseChoice::CloseForNow if sale.found.is_none() => format!(
            "{} A late payment still shows here.",
            pos_text::refund_on_other_till(
                pos,
                sale.device_name
                    .as_deref()
                    .unwrap_or("the till that rang it up")
            )
        ),
        CloseChoice::CloseForNow => {
            "It stays in the shift. A late payment still counts, and you can refund it there."
                .into()
        }
        CloseChoice::KeepUp => match sale.phase {
            SalePhase::Waiting | SalePhase::Partial => {
                format!("{} left to pay.", sale_text::clock(seconds_left))
            }
            _ => "The payment is being confirmed.".into(),
        },
    }
}

/// "Sale #bP9m · $13.88 · Square has it as Monero", or "… · From Back till"
/// for another till's sale.
pub fn subtitle(sale: &TillSale, pos: Option<PointOfSale>, money: &dyn MoneyFormat) -> String {
    let whose = if sale.found.is_some() {
        format!("{} has it as Monero", pos_text::name_first(pos))
    } else {
        sale_text::place(sale, None)
    };
    format!(
        "{} · {} · {whose}",
        sale_text::title(sale),
        sale_text::amount(sale, money)
    )
}

/// The sheet's question, the same for every sale.
pub const QUESTION: &str = "Is the customer paying?";

/// What the close sheet shows.
#[derive(Clone, Debug)]
pub struct CloseInput {
    pub sale: TillSale,
    /// The till's point of sale now.
    pub till: Option<PointOfSale>,
    /// The point of sale the till's source watches
    /// ([`till_sales::RunnerState`]'s), which says whether the till can
    /// refund the sale itself.
    pub source: Option<PointOfSale>,
}

/// One answer, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CloseAnswer {
    pub choice: CloseChoice,
    pub title: String,
    pub detail: String,
    /// Where the cashier lands after it ([`outcome`]): the platform moves
    /// its stage from where the sheet was asked, the engine moves the screen.
    pub then: CloseThen,
}

/// The close sheet, ready to draw. No answers: the sale has no X (paid,
/// ended or closed meanwhile), so the sheet goes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CloseView {
    pub question: String,
    pub subtitle: String,
    pub answers: Vec<CloseAnswer>,
    /// The ring on "Yes".
    pub ring: Ring,
    /// "Yes" counts the code down: draw it again in a second.
    pub ticks: bool,
}

/// The close sheet at the clock's now, amounts written by `money`.
pub fn close(input: &CloseInput, clock: DeviceClock, money: &dyn MoneyFormat) -> CloseView {
    let sale = &input.sale;
    let pos = pos_text::of(sale, input.till);
    let elsewhere = pos_text::elsewhere(sale, input.till);
    let seconds = sale_text::seconds_left(sale.expires_at, clock.now);
    let answers: Vec<CloseAnswer> = choices(sale, elsewhere)
        .into_iter()
        .map(|choice| CloseAnswer {
            choice,
            title: title(choice, sale, pos),
            detail: detail(choice, sale, pos, input.source, seconds),
            then: outcome(choice).then,
        })
        .collect();
    CloseView {
        question: QUESTION.into(),
        subtitle: subtitle(sale, pos, money),
        ticks: !answers.is_empty() && sale.phase.counts_down(),
        ring: ring(sale),
        answers,
    }
}

/// Ported from Android's `ShiftListTest`
/// (`the close sheet's answers close, refund and go where the design says`),
/// with the sheet itself after it.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{la, money};
    use crate::shift::tests::{at, fixture_shift, one, sale_of};

    const SQUARE: Option<PointOfSale> = Some(PointOfSale::Square);

    #[test]
    fn the_close_sheets_answers_close_refund_and_go_where_the_design_says() {
        assert_eq!(
            outcome(CloseChoice::RefundIt),
            CloseOutcome {
                close: true,
                refund: true,
                then: CloseThen::Sale
            }
        );
        assert_eq!(
            outcome(CloseChoice::CloseForNow),
            CloseOutcome {
                close: true,
                refund: false,
                then: CloseThen::Shift
            }
        );
        assert_eq!(
            outcome(CloseChoice::KeepUp),
            CloseOutcome {
                close: false,
                refund: false,
                then: CloseThen::Stay
            }
        );
        let w = sale_of("w").build();
        assert_eq!(choices(&w, false), CloseChoice::ALL);
        let theirs = sale_of("t").theirs().build();
        assert_eq!(
            choices(&theirs, false),
            [CloseChoice::CloseForNow, CloseChoice::KeepUp],
            "this till can't refund another's"
        );
        assert_eq!(
            title(CloseChoice::CloseForNow, &theirs, SQUARE),
            "No: close it"
        );
        assert_eq!(
            detail(CloseChoice::CloseForNow, &theirs, SQUARE, SQUARE, 0),
            "Refund it in Square on Back till. A late payment still shows here."
        );
        assert_eq!(
            title(CloseChoice::CloseForNow, &w, SQUARE),
            "Not sure: close it for now"
        );
        assert_eq!(
            detail(CloseChoice::KeepUp, &w, SQUARE, SQUARE, 892),
            "14:52 left to pay."
        );
        assert!(
            choices(&one(&fixture_shift(), "refunded"), false).is_empty(),
            "nothing to ask: no X"
        );
        assert!(choices(&sale_of("w").closed_at(1).build(), false).is_empty());
        assert!(applies(Some(&w)), "the code is still up: the answer stands");
        assert!(
            !applies(Some(&sale_of("p").phase(SalePhase::Paid).build())),
            "paid as the cashier tapped: the answer is dropped"
        );
        assert!(!applies(Some(&sale_of("w").closed_at(1).build())));
        assert!(!applies(None), "gone from the shift");
        let amount = sale_text::amount(&w, &money);
        assert_eq!(
            subtitle(&w, SQUARE, &money),
            format!("Sale #w · {amount} · Square has it as Monero")
        );
        assert_eq!(
            subtitle(&theirs, SQUARE, &money),
            format!("Sale #t · {amount} · From Back till")
        );
    }

    #[test]
    fn the_sheet_asks_with_the_answers_this_till_can_give() {
        let sale = TillSale {
            expires_at: at() + 892,
            ..sale_of("bP9m").build()
        };
        let view = close(
            &CloseInput {
                sale,
                till: SQUARE,
                source: SQUARE,
            },
            la(at()),
            &money,
        );
        assert_eq!(view.question, "Is the customer paying?");
        assert_eq!(
            view.subtitle,
            "Sale #bP9m · $8.40 · Square has it as Monero"
        );
        let words: Vec<(&str, &str)> = view
            .answers
            .iter()
            .map(|a| (a.title.as_str(), a.detail.as_str()))
            .collect();
        assert_eq!(
            words,
            [
                (
                    "No: refund it in Square",
                    "Till refunds the sale and closes it."
                ),
                (
                    "Not sure: close it for now",
                    "It stays in the shift. A late payment still counts, and you can refund it there."
                ),
                ("Yes: keep the code up", "14:52 left to pay."),
            ]
        );
        assert_eq!(
            view.answers.iter().map(|a| a.then).collect::<Vec<_>>(),
            [CloseThen::Sale, CloseThen::Shift, CloseThen::Stay],
            "each answer says where it lands, so the platform's stage moves from the right place"
        );
        assert!(view.ticks);
    }

    #[test]
    fn another_tills_sale_offers_close_and_keep_and_a_confirming_one_says_so() {
        let theirs = TillSale {
            phase: SalePhase::Confirming,
            ..sale_of("t").theirs().build()
        };
        let view = close(
            &CloseInput {
                sale: theirs,
                till: SQUARE,
                source: SQUARE,
            },
            la(at()),
            &money,
        );
        let words: Vec<(&str, &str)> = view
            .answers
            .iter()
            .map(|a| (a.title.as_str(), a.detail.as_str()))
            .collect();
        assert_eq!(
            words,
            [
                (
                    "No: close it",
                    "Refund it in Square on Back till. A late payment still shows here."
                ),
                ("Yes: keep the code up", "The payment is being confirmed."),
            ]
        );
        assert!(!view.ticks);
        let paid = close(
            &CloseInput {
                sale: sale_of("p").phase(SalePhase::Paid).build(),
                till: SQUARE,
                source: SQUARE,
            },
            la(at()),
            &money,
        );
        assert!(paid.answers.is_empty(), "paid meanwhile: nothing to ask");
    }
}
