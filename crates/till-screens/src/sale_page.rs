//! A sale's page when it isn't live (boards DetPaid, DetLatePaid,
//! DetClosed, DetExpired, DetReversed, DetRefunded, DetOtherTill and
//! PadDetail, built from SaleDetail; Android: `SalePage.kt` and
//! `ui/SaleDetail.kt`'s words): what each says, from where the sale stands.
//! The same on every device.

use monokulo_client::{MoneyFormat, PointOfSale, SalePhase};
use till_sales::{Standing, TillSale};

use crate::order_text::{OrderCard, order_card};
use crate::shift::StateIcon;
use crate::time::{DeviceClock, time};
use crate::view::{Fact, Press};
use crate::{pos_text, sale_text};

/// The colour roles behind a page's hero glyph and its note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Ground {
    Paid,
    Warn,
    Danger,
    Chip,
    Card,
}

/// A page's note: a few sentences on a card, on [`Ground::Danger`] for a
/// reversed payment.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PageNote {
    pub text: String,
    pub ground: Ground,
}

impl PageNote {
    fn card(text: impl Into<String>) -> PageNote {
        PageNote {
            text: text.into(),
            ground: Ground::Card,
        }
    }
}

/// What a page offers: refund the point of sale's sale, put the code back
/// up, make a sale whose code ran out a new one (docs/ledger.md §7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PageAction {
    Refund,
    ShowCodeAgain,
    NewCode,
}

/// A sale's page (the design's row 2b), in words and roles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SalePageModel {
    pub icon: StateIcon,
    pub ground: Ground,
    pub heading: String,
    /// The heading in warn: paid after the refund, which needs the cashier.
    pub heading_warn: bool,
    /// The amount in muted: refunded, so it no longer counts.
    pub amount_muted: bool,
    pub line: String,
    pub note: Option<PageNote>,
    /// Whether the page shows what was bought: never for another till's
    /// sale, which this till has no receipt for.
    pub order: bool,
    pub facts: Vec<Fact>,
    pub actions: Vec<PageAction>,
}

/// A sale screen's top, on a till on `till` (decision 141 of android.md).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaleHeader {
    /// The point of sale that names the sale's words and receipt
    /// ([`pos_text::of`]).
    pub pos: Option<PointOfSale>,
    /// The till has changed from it since.
    pub elsewhere: bool,
    /// The bar's second line: "Front Counter · this till", "Clover · this
    /// till" for a sale rung up at the point of sale it changed from.
    pub place: String,
    /// Whether the sale offers "Back to Square": only on a till on Square,
    /// and never for a sale another point of sale took (decision 135), on
    /// every device (core.md decision 94).
    pub back_to_pos: bool,
}

/// `sale`'s header on a till on `till` at `location`.
pub fn header(sale: &TillSale, till: Option<PointOfSale>, location: Option<&str>) -> SaleHeader {
    let elsewhere = pos_text::elsewhere(sale, till);
    SaleHeader {
        pos: pos_text::of(sale, till),
        elsewhere,
        place: sale_text::place(sale, pos_text::place_of(sale, till, location)),
        back_to_pos: !elsewhere && till == Some(PointOfSale::Square),
    }
}

/// What each page says, from where the sale stands. Times the app doesn't
/// know aren't written (decision 71 of android.md). `pos` is the sale's
/// point of sale. A sale whose order has a title at the point of sale
/// (Clover's "Table 4") shows it first among the facts. `elsewhere`: the
/// sale was rung up at a point of sale this till has since changed from, so
/// the till can't refund it or see its refund, and says to refund it there;
/// one already told to refund it there keeps Refund, for the cashier's word
/// that it's done.
pub fn page(
    sale: &TillSale,
    pos: Option<PointOfSale>,
    clock: DeviceClock,
    elsewhere: bool,
) -> SalePageModel {
    let name = pos_text::name(pos);
    let status = sale_text::status(sale, pos);
    let has_it = pos_text::has_it_as_monero(pos);
    let at = |t: i64| time(t, clock);
    let fact = |label: &str, t: Option<i64>| {
        t.filter(|&t| t > 0)
            .map(|t| Fact::new(label, time(t, clock)))
    };
    let xmr = sale_text::xmr(sale);
    let rung_up = fact("Rung up", Some(sale.created_at));
    let ended = fact(ended_label(sale.phase), sale.ended_at);
    let watching = if sale.settled {
        "Monokulo has stopped watching it."
    } else {
        "Monokulo keeps watching for a late payment for a while: if one arrives, this sale turns paid."
    };
    let refunded_because = || {
        if sale.closed_at.is_some() {
            fact("Closed: customer not paying", sale.closed_at)
        } else {
            fact(ended_label(sale.phase), sale.ended_at)
        }
    };
    let facts = |list: Vec<Option<Fact>>| list.into_iter().flatten().collect::<Vec<_>>();
    let model = |icon, ground, line: String, note, facts| SalePageModel {
        icon,
        ground,
        heading: status.clone(),
        heading_warn: false,
        amount_muted: false,
        line,
        note,
        order: true,
        facts,
        actions: Vec::new(),
    };
    let standing = sale.standing();
    let model = match standing {
        Standing::Paid { late, after_refund } => {
            let closed = sale.closed_at.is_some();
            let note = if after_refund {
                Some(format!(
                    "{xmr} arrived after the {name} sale was refunded. Ring the sale up in {name} again, or return the payment."
                ))
            } else if closed {
                Some(format!(
                    "The customer paid {}after the sale was closed. {}",
                    after(sale.closed_at, sale.paid_at),
                    nothing_to_do(pos)
                ))
            } else if late {
                Some(format!(
                    "The customer paid {}after the code ran out. {}",
                    after(sale.ended_at, sale.paid_at),
                    nothing_to_do(pos)
                ))
            } else {
                None
            };
            let list = if after_refund {
                vec![
                    rung_up,
                    refunded_because(),
                    fact(&pos_text::refunded_in(pos), sale.refunded_at),
                    fact("Paid", sale.paid_at),
                ]
            } else if closed {
                vec![
                    rung_up,
                    fact("Closed", sale.closed_at),
                    fact("Paid", sale.paid_at),
                ]
            } else if late {
                vec![rung_up, ended, fact("Paid", sale.paid_at)]
            } else {
                vec![
                    rung_up,
                    fact("Seen on the network", sale.seen_at),
                    fact("Paid", sale.paid_at),
                ]
            };
            let line = format!(
                "{xmr} received{}",
                sale.paid_at
                    .map(|t| format!(" · {}", at(t)))
                    .unwrap_or_default()
            );
            SalePageModel {
                heading_warn: after_refund,
                ..model(
                    StateIcon::Tick,
                    Ground::Paid,
                    line,
                    note.map(PageNote::card),
                    facts(list),
                )
            }
        }
        Standing::Closed => {
            let then = if sale.settled {
                "Monokulo has stopped watching it.".to_owned()
            } else if sale.expires_at > clock.now {
                format!(
                    "The code is still live until {}, and Monokulo keeps watching for a late payment after that: if one arrives, this sale turns paid.",
                    at(sale.expires_at)
                )
            } else {
                watching.to_owned()
            };
            SalePageModel {
                actions: vec![PageAction::Refund, PageAction::ShowCodeAgain],
                ..model(
                    StateIcon::Clock,
                    Ground::Warn,
                    xmr.clone(),
                    Some(PageNote::card(format!(
                        "{has_it}. If the customer isn't paying, refund it. {then}"
                    ))),
                    facts(vec![rung_up, fact("Closed", sale.closed_at)]),
                )
            }
        }
        Standing::Expired => {
            let note = if sale.phase == SalePhase::Cancelled {
                format!(
                    "The order was cancelled in Monokulo. {has_it}: refund it if the customer isn't paying."
                )
            } else {
                format!(
                    "The code ran out at {} with no payment. {has_it}: refund it if the customer isn't paying. {watching}",
                    at(sale.expires_at)
                )
            };
            SalePageModel {
                actions: vec![PageAction::Refund],
                ..model(
                    StateIcon::Clock,
                    Ground::Warn,
                    xmr.clone(),
                    Some(PageNote::card(note)),
                    facts(vec![rung_up, ended]),
                )
            }
        }
        Standing::Reversed => SalePageModel {
            actions: vec![PageAction::Refund],
            ..model(
                StateIcon::Warning,
                Ground::Danger,
                format!("{xmr} · no longer counts"),
                Some(PageNote {
                    text: format!(
                        "The payment was spent twice, so it doesn't count. Don't hand anything over. {has_it}: refund it there."
                    ),
                    ground: Ground::Danger,
                }),
                facts(vec![
                    rung_up,
                    fact("Seen on the network", sale.seen_at),
                    ended,
                ]),
            )
        },
        Standing::Refunded => {
            let mut line: Vec<String> = sale
                .refunded_at
                .map(|t| format!("Refunded {}", at(t)))
                .into_iter()
                .collect();
            line.push("nothing was paid".into());
            SalePageModel {
                amount_muted: true,
                ..model(
                    StateIcon::Refund,
                    Ground::Chip,
                    line.join(" · "),
                    Some(PageNote::card(
                        "Nothing more to do. If the customer pays after all, Till tells you straight away and this sale shows Paid after the refund.",
                    )),
                    facts(vec![
                        rung_up,
                        refunded_because(),
                        fact(&pos_text::refunded_in(pos), sale.refunded_at),
                    ]),
                )
            }
        }
        Standing::RefundInPos => SalePageModel {
            actions: vec![PageAction::Refund],
            ..model(
                StateIcon::Refund,
                Ground::Warn,
                format!("{xmr} · not paid"),
                Some(PageNote::card(format!(
                    "Refund this sale in {name}, then tap Refund to say it's done. While the shift is on, Till notices by itself too."
                ))),
                facts(vec![
                    rung_up,
                    refunded_because(),
                    fact("Told to refund it", sale.refund_pending_at),
                ]),
            )
        },
        Standing::OtherTill => {
            let other = sale.device_name.as_deref();
            let who = match other {
                Some(other) => format!("{other} rang this sale up."),
                None => "Another till rang this sale up.".into(),
            };
            SalePageModel {
                order: false,
                ..model(
                    StateIcon::Close,
                    Ground::Chip,
                    format!("{xmr} · not paid"),
                    Some(PageNote::card(format!(
                        "{who} Its {name} sale and what was bought stay on that till, so refund it there if the customer isn't paying. A late payment still shows here."
                    ))),
                    facts(vec![
                        other.map(|other| Fact::new("Till", other)),
                        rung_up,
                        if sale.phase.is_final() {
                            ended
                        } else {
                            fact("Closed here", sale.closed_at)
                        },
                    ]),
                )
            }
        }
        Standing::CashOffer { kept } => {
            let who = sale_text::customer(sale).unwrap_or("the customer");
            let note = if kept {
                format!(
                    "Kept as cash in {name}. If {who} would rather pay in Monero after all, make a payment code for this sale."
                )
            } else {
                format!(
                    "Rung up as Cash. If {who} would rather pay in Monero, make a payment code for this sale."
                )
            };
            model(
                StateIcon::Cash,
                Ground::Chip,
                format!("Cash in {name}"),
                Some(PageNote::card(note)),
                facts(vec![rung_up, fact("Kept as cash", sale.kept_as_cash_at)]),
            )
        }
        Standing::PaidInCash => model(
            StateIcon::Cash,
            Ground::Chip,
            format!("Cash in {name}"),
            Some(PageNote::card(format!(
                "The customer paid in cash. {} already has it as cash, so there's nothing to refund.",
                pos_text::name_first(pos)
            ))),
            facts(vec![rung_up, fact("Paid in cash", sale.paid_in_cash_at)]),
        ),
        // A live sale shows its code, not a page; this is only what it
        // would say.
        Standing::Live(_) => model(
            StateIcon::Ring,
            Ground::Chip,
            xmr.clone(),
            None,
            facts(vec![rung_up, fact("Seen on the network", sale.seen_at)]),
        ),
    };
    let rung_up_there =
        format!("This sale was rung up in {name} before this till changed its point of sale.");
    let mut here = if !elsewhere || !model.actions.contains(&PageAction::Refund) {
        model
    } else if standing == Standing::RefundInPos {
        // Already told to refund it there: Refund opens the sheet, whose
        // "I've refunded it" takes the cashier's word.
        SalePageModel {
            note: Some(PageNote::card(format!(
                "{rung_up_there} Refund it in {name}, then tap Refund to say it's done."
            ))),
            ..model
        }
    } else {
        SalePageModel {
            actions: model
                .actions
                .iter()
                .copied()
                .filter(|&a| a != PageAction::Refund)
                .collect(),
            note: Some(PageNote::card(format!(
                "{rung_up_there} If the customer isn't paying, refund it in {name}."
            ))),
            ..model
        }
    };
    if sale.cash() && here.actions.contains(&PageAction::Refund) {
        // Never refunded through the point of sale (core.md decision 116).
        here.actions.retain(|&a| a != PageAction::Refund);
        here.note = Some(PageNote::card(format!(
            "{} has it as cash. If the customer paid in real cash, tap Paid in cash. If they aren't paying, take the sale back in {name}.",
            pos_text::name_first(pos)
        )));
    }
    if let Some(title) = sale.found.as_ref().and_then(|found| found.title.as_deref()) {
        here.facts.insert(0, Fact::new("Order", title));
    }
    ledger_lines(sale, &mut here);
    here
}

/// What the ledger adds to a page (docs/ledger.md §5, §7): New code on a
/// sale whose code ran out unpaid, when the store's Monokulo can renew its
/// order (core.md decision 171), and why its code has no receipt.
fn ledger_lines(sale: &TillSale, here: &mut SalePageModel) {
    if sale.may_new_code() && !here.actions.contains(&PageAction::NewCode) {
        here.actions.push(PageAction::NewCode);
    }
    if !sale.code.has_receipt
        && !sale.code.uri.is_empty()
        && let Some(why) = &sale.code.why
    {
        here.facts
            .push(Fact::new("No receipt in the code", why.clone()));
    }
}

/// What a cash sale offers (core.md decision 116).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CashAction {
    /// Make the offered cash sale's order and code now.
    TakeInMonero,
    /// Leave the offered cash sale as cash.
    KeepAsCash,
    /// The customer paid real cash: close it, with nothing refunded.
    PaidInCash,
}

/// A cash sale's button.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CashButton {
    pub action: CashAction,
    pub press: Press,
}

/// "Take it in Monero".
pub const TAKE_IN_MONERO: &str = "Take it in Monero";
/// "Keep as cash".
pub const KEEP_AS_CASH: &str = "Keep as cash";
/// "Paid in cash".
pub const PAID_IN_CASH: &str = "Paid in cash";

/// The cash buttons for `sale`: Take it in Monero and Keep as cash while
/// it's offered (Take it alone once kept), Paid in cash on a cash sale
/// whose code is up or ended unpaid; none for any other sale. `busy`
/// while its order is being made.
pub fn cash_buttons(sale: &TillSale, busy: bool) -> Vec<CashButton> {
    let button = |action, label: &str| CashButton {
        action,
        press: Press::new(label).enabled(!busy),
    };
    match sale.standing() {
        Standing::CashOffer { kept: false } => vec![
            CashButton {
                action: CashAction::TakeInMonero,
                press: Press::new(if busy {
                    "Making the code…"
                } else {
                    TAKE_IN_MONERO
                })
                .enabled(!busy),
            },
            button(CashAction::KeepAsCash, KEEP_AS_CASH),
        ],
        Standing::CashOffer { kept: true } => vec![CashButton {
            action: CashAction::TakeInMonero,
            press: Press::new(if busy {
                "Making the code…"
            } else {
                TAKE_IN_MONERO
            })
            .enabled(!busy),
        }],
        Standing::Live(_) | Standing::Closed | Standing::Expired | Standing::Reversed
            if sale.cash() =>
        {
            vec![button(CashAction::PaidInCash, PAID_IN_CASH)]
        }
        _ => Vec::new(),
    }
}

/// "15 minutes ", "1 minute ", "under a minute " from `from` to `to`: how
/// late a payment came. Empty when either time isn't known.
pub fn after(from: Option<i64>, to: Option<i64>) -> String {
    let (Some(from), Some(to)) = (from, to) else {
        return String::new();
    };
    match (to - from).max(0) / 60 {
        0 => "under a minute ".into(),
        1 => "1 minute ".into(),
        minutes => format!("{minutes} minutes "),
    }
}

fn ended_label(phase: SalePhase) -> &'static str {
    match phase {
        SalePhase::Cancelled => "Cancelled",
        SalePhase::DoubleSpend => "Reversed",
        _ => "Code expired",
    }
}

fn nothing_to_do(pos: Option<PointOfSale>) -> String {
    format!(
        "{} already has it as Monero, so there's nothing to do. Hand over the order if they're still here.",
        pos_text::name_first(pos)
    )
}

/// What a sale's page shows.
#[derive(Clone, Debug)]
pub struct SalePageInput {
    pub sale: TillSale,
    /// The till's point of sale now.
    pub till: Option<PointOfSale>,
    /// The till's Square location or Clover merchant.
    pub location: Option<String>,
    /// Why the point of sale refused this sale's refund, if it did
    /// ([`till_sales::TillState::refund_problems`]).
    pub refund_problem: Option<String>,
    /// A refund is under way.
    pub refunding: bool,
    /// An offered cash sale's order is being made ("Take it in Monero").
    pub taking: bool,
}

/// A button on the page: the action, its words, and whether it can be
/// pressed now.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PageButton {
    pub action: PageAction,
    pub press: Press,
}

/// A sale's page, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SalePageView {
    /// The bar: whom it's for, or "Sale #bP9m".
    pub title: String,
    /// "3:48 PM · Front Counter · this till", led by the sale's number when
    /// the title is the customer's name.
    pub title_line: String,
    pub icon: StateIcon,
    pub ground: Ground,
    pub heading: String,
    pub heading_warn: bool,
    pub amount: String,
    pub amount_muted: bool,
    pub line: String,
    pub note: Option<PageNote>,
    /// What was bought, every line: this till's own sale with a receipt.
    pub order: Option<OrderCard>,
    pub facts: Vec<Fact>,
    pub refund_problem: Option<String>,
    pub buttons: Vec<PageButton>,
    /// A cash sale's buttons ([`cash_buttons`]), after [`Self::buttons`].
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    pub cash: Vec<CashButton>,
    /// Paid's way back to the point of sale on iOS, "Back to Square", on a
    /// till on Square for its own sales (empty otherwise), shown where the
    /// platform can open it (docs/ipad-redesign.md decision 1, core.md 94).
    #[cfg_attr(feature = "uniffi", uniffi(default = ""))]
    pub back_to: String,
}

/// `input`'s page at the clock's now, amounts written by `money`.
pub fn sale_page(
    input: &SalePageInput,
    clock: DeviceClock,
    money: &dyn MoneyFormat,
) -> SalePageView {
    let sale = &input.sale;
    let header = header(sale, input.till, input.location.as_deref());
    let model = page(sale, header.pos, clock, header.elsewhere);
    // The time before the place, so a narrow bar's ellipsis takes "this
    // till" first, not the time.
    let subtitle = if sale.created_at > 0 {
        format!("{} · {}", time(sale.created_at, clock), header.place)
    } else {
        header.place.clone()
    };
    let amount = sale_text::amount(sale, money);
    let buttons = model
        .actions
        .iter()
        .map(|&action| PageButton {
            action,
            press: match action {
                PageAction::Refund => Press::new(pos_text::refund_button(header.pos, &amount))
                    .enabled(!input.refunding),
                PageAction::ShowCodeAgain => Press::new("Show the code again"),
                PageAction::NewCode => Press::new("New code").enabled(!input.taking),
            },
        })
        .collect();
    SalePageView {
        title: sale_text::heading(sale),
        title_line: sale_text::heading_line(sale, &subtitle),
        icon: model.icon,
        ground: model.ground,
        heading: model.heading,
        heading_warn: model.heading_warn,
        amount,
        amount_muted: model.amount_muted,
        line: model.line,
        note: model.note,
        order: if model.order {
            sale.receipt
                .as_ref()
                .map(|receipt| order_card(receipt, header.pos, false))
        } else {
            None
        },
        facts: model.facts,
        refund_problem: model
            .actions
            .contains(&PageAction::Refund)
            .then(|| input.refund_problem.clone())
            .flatten(),
        buttons,
        cash: cash_buttons(sale, input.taking),
        back_to: if header.back_to_pos {
            pos_text::back_to(header.pos)
        } else {
            String::new()
        },
    }
}

/// Ported from Android's `ShiftListTest`, whose tests cover `SalePage` and
/// `SaleHeader` (Android has no test class of their own), test for test;
/// then the page itself.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::close::{self, CloseChoice};
    use crate::fixtures::{la, money, receipt};
    use crate::shift::tests::{at, fixture_shift, one, sale_of};

    const SQUARE: Option<PointOfSale> = Some(PointOfSale::Square);
    const CLOVER: Option<PointOfSale> = Some(PointOfSale::Clover);

    fn labels(model: &SalePageModel) -> Vec<&str> {
        model.facts.iter().map(|f| f.label.as_str()).collect()
    }

    fn note(model: &SalePageModel) -> &str {
        model.note.as_ref().map_or("", |n| n.text.as_str())
    }

    fn clover_found(sale: TillSale) -> TillSale {
        let mut sale = sale;
        sale.found.as_mut().unwrap().point_of_sale = PointOfSale::Clover;
        sale
    }

    #[test]
    fn why_a_sales_code_has_no_receipt_shows_on_its_page() {
        let mut sale = sale_of("o2").phase(SalePhase::Paid).paid_at(at()).build();
        sale.code = till_sales::PaymentCode {
            uri: "monero:4abc".into(),
            has_receipt: false,
            why: Some("This till isn't signed in to the point of sale.".into()),
        };
        let model = page(&sale, SQUARE, la(at()), false);
        assert!(model.facts.contains(&Fact::new(
            "No receipt in the code",
            "This till isn't signed in to the point of sale."
        )));
        assert!(!model.heading_warn);
    }

    #[test]
    fn an_orders_title_at_the_point_of_sale_is_the_pages_first_fact() {
        let mut titled = sale_of("t").closed_at(at()).build();
        titled.found.as_mut().unwrap().title = Some("Table 4".into());
        let model = page(&titled, CLOVER, la(at()), false);
        assert_eq!(model.facts[0], Fact::new("Order", "Table 4"));
        assert_eq!(
            labels(&page(
                &sale_of("u").closed_at(at()).build(),
                CLOVER,
                la(at()),
                false
            )),
            ["Rung up", "Closed"]
        );
    }

    #[test]
    fn a_clover_sale_on_a_till_changed_to_square_keeps_clovers_name_on_its_screens_read_back_from_the_saved_shift_too()
     {
        let clover = clover_found(sale_of("c").build());
        // The shift as the store saves and reads it again (a reload, a
        // relaunch).
        let json = till_sales::shift_json::encode(std::slice::from_ref(&clover));
        let saved = till_sales::shift_json::decode(&json).unwrap().remove(0);
        for shown in [clover, saved] {
            let h = header(&shown, SQUARE, Some("Front Counter"));
            assert_eq!(h.pos, CLOVER, "the receipt is Clover's order");
            assert!(h.elsewhere);
            assert_eq!(
                h.place, "Clover · this till",
                "not the Square location the till watches now"
            );
            assert!(!h.back_to_pos, "no Back to Square for a Clover sale");
            assert_eq!(
                pos_text::receipt_label(h.pos, "T5XWHYQG"),
                "Clover order T5XWHYQG"
            );
            let closed = TillSale {
                closed_at: Some(at()),
                ..shown.clone()
            };
            let model = page(&closed, h.pos, la(at()), h.elsewhere);
            assert_eq!(model.actions, [PageAction::ShowCodeAgain]);
            assert!(close::subtitle(&shown, h.pos, &money).ends_with(" · Clover has it as Monero"));
        }
        assert_eq!(
            header(&sale_of("s").build(), SQUARE, Some("Front Counter")),
            SaleHeader {
                pos: SQUARE,
                elsewhere: false,
                place: "Front Counter · this till".into(),
                back_to_pos: true
            }
        );
        assert_eq!(
            header(
                &sale_of("t").theirs().build(),
                SQUARE,
                Some("Front Counter")
            ),
            SaleHeader {
                pos: SQUARE,
                elsewhere: false,
                place: "From Back till".into(),
                back_to_pos: true
            }
        );
        assert!(
            !header(&sale_of("s").build(), CLOVER, Some("The Bean Room")).back_to_pos,
            "Clover's device is its own: no Back to"
        );
    }

    #[test]
    fn a_sale_found_before_the_point_of_sale_changed_keeps_the_name_of_the_one_that_took_it() {
        let at = at();
        let before = clover_found(sale_of("old").closed_at(at).refunded_at(at).build());
        let after_change = sale_of("new").closed_at(at).refunded_at(at).build();
        assert_eq!(pos_text::of(&before, SQUARE), CLOVER);
        assert_eq!(
            pos_text::of(&after_change, CLOVER),
            SQUARE,
            "found at Square: Square's, whatever the till's now"
        );
        assert_eq!(
            pos_text::of(&sale_of("theirs").theirs().build(), CLOVER),
            CLOVER,
            "another till's: the till's own"
        );
        assert_eq!(
            pos_text::of(&sale_of("theirs").theirs().build(), None),
            None
        );
        assert_eq!(
            sale_text::status(&before, pos_text::of(&before, SQUARE)),
            "Refunded in Clover"
        );
        let closed = page(&sale_of("c").closed_at(at).build(), CLOVER, la(at), true);
        assert_eq!(
            closed.actions,
            [PageAction::ShowCodeAgain],
            "this till can't refund it, or see its refund"
        );
        assert_eq!(
            note(&closed),
            "This sale was rung up in Clover before this till changed its point of sale. If the customer isn't paying, refund it in Clover."
        );
        let told = TillSale {
            refund_pending_at: Some(at),
            ..sale_of("t").phase(SalePhase::Expired).ended_at(at).build()
        };
        let pending_there = page(&told, CLOVER, la(at), true);
        assert_eq!(
            pending_there.actions,
            [PageAction::Refund],
            "already told to refund it there: Refund takes the cashier's word"
        );
        assert_eq!(
            note(&pending_there),
            "This sale was rung up in Clover before this till changed its point of sale. Refund it in Clover, then tap Refund to say it's done."
        );
        assert_eq!(
            close::choices(&sale_of("w").build(), true),
            [CloseChoice::CloseForNow, CloseChoice::KeepUp]
        );
        assert_eq!(
            close::choices(&sale_of("w").build(), false),
            CloseChoice::ALL
        );
    }

    #[test]
    fn a_late_payment_says_how_late_in_words() {
        assert_eq!(after(Some(100), Some(159)), "under a minute ");
        assert_eq!(after(Some(100), Some(160)), "1 minute ");
        assert_eq!(after(Some(100), Some(100 + 15 * 60 + 30)), "15 minutes ");
        assert_eq!(after(None, Some(100)), "");
        let late = page(&one(&fixture_shift(), "latePaid"), SQUARE, la(at()), false);
        assert_eq!(
            note(&late),
            "The customer paid 15 minutes after the sale was closed. Square already has it as Monero, so there's nothing to do. Hand over the order if they're still here."
        );
        assert_eq!(labels(&late), ["Rung up", "Closed", "Paid"]);
        let expired_then_paid = page(
            &sale_of("x")
                .phase(SalePhase::Paid)
                .ended_at(at() - 60)
                .paid_at(at())
                .build(),
            SQUARE,
            la(at()),
            false,
        );
        assert_eq!(expired_then_paid.heading, "Paid after the code expired");
        assert!(
            note(&expired_then_paid)
                .starts_with("The customer paid 1 minute after the code ran out.")
        );
        assert_eq!(
            labels(&expired_then_paid),
            ["Rung up", "Code expired", "Paid"]
        );
    }

    #[test]
    fn each_page_says_what_the_design_says_without_times_the_app_doesnt_know() {
        let shift = fixture_shift();
        let at = at();
        let of = |id: &str| page(&one(&shift, id), SQUARE, la(at), false);
        let paid = of("paid");
        assert_eq!(paid.heading, "Paid");
        assert_eq!(paid.line, "0.0336 XMR received · 3:58 PM");
        assert_eq!(paid.note, None);
        assert_eq!(
            paid.facts,
            [
                Fact::new("Rung up", "3:48 PM"),
                Fact::new("Paid", "3:58 PM")
            ],
            "no seen row when it wasn't seen; no confirmations"
        );
        assert!(paid.actions.is_empty());

        let closed = of("closedHere");
        assert_eq!(
            closed.actions,
            [PageAction::Refund, PageAction::ShowCodeAgain]
        );
        assert_eq!(
            note(&closed),
            "Square has this sale as Monero. If the customer isn't paying, refund it. Monokulo keeps watching for a late payment for a while: if one arrives, this sale turns paid."
        );
        let still_live = page(
            &TillSale {
                expires_at: at + 1440,
                ..sale_of("c").closed_at(at).build()
            },
            SQUARE,
            la(at),
            false,
        );
        assert!(
            note(&still_live)
                .contains("The code is still live until 4:22 PM, and Monokulo keeps watching")
        );
        let mut renewable = sale_of("c").phase(SalePhase::Expired).ended_at(at).build();
        renewable.extras.renewable = true;
        let settled = page(
            &TillSale {
                settled: true,
                ..renewable.clone()
            },
            SQUARE,
            la(at),
            false,
        );
        assert!(note(&settled).ends_with("Monokulo has stopped watching it."));
        assert_eq!(
            settled.actions,
            [PageAction::Refund, PageAction::NewCode],
            "an expired code can't be shown again, but it can have a new one (docs/ledger.md §7)"
        );
        let older_monokulo = page(
            &TillSale {
                settled: true,
                ..sale_of("c").phase(SalePhase::Expired).ended_at(at).build()
            },
            SQUARE,
            la(at),
            false,
        );
        assert_eq!(
            older_monokulo.actions,
            [PageAction::Refund],
            "a Monokulo that can't renew the order gets no New code, never a second order (core.md 171)"
        );

        let reversed = of("reversed");
        assert_eq!(reversed.note.as_ref().unwrap().ground, Ground::Danger);
        assert_eq!(reversed.line, "0.0336 XMR · no longer counts");

        let refunded = of("refunded");
        assert_eq!(refunded.line, "Refunded 3:58 PM · nothing was paid");
        assert!(refunded.amount_muted);
        assert_eq!(
            labels(&refunded),
            [
                "Rung up",
                "Closed: customer not paying",
                "Refunded in Square"
            ]
        );

        let after_refund = of("paidAfterRefund");
        assert_eq!(after_refund.heading, "Paid after the refund");
        assert!(after_refund.heading_warn);
        assert_eq!(
            labels(&after_refund),
            [
                "Rung up",
                "Closed: customer not paying",
                "Refunded in Square",
                "Paid"
            ]
        );

        let theirs = of("expiredTheirs");
        assert!(!theirs.order, "what was bought stays on that till");
        assert!(theirs.actions.is_empty());
        assert_eq!(
            theirs.facts,
            [
                Fact::new("Till", "Back till"),
                Fact::new("Rung up", "3:48 PM"),
                Fact::new("Code expired", "3:58 PM")
            ]
        );
        assert!(note(&theirs).starts_with("Back till rang this sale up."));
        assert_eq!(
            labels(&of("closedTheirs")),
            ["Till", "Rung up", "Closed here"]
        );

        let cancelled = of("cancelled");
        assert_eq!(
            note(&cancelled),
            "The order was cancelled in Monokulo. Square has this sale as Monero: refund it if the customer isn't paying."
        );
        assert_eq!(cancelled.facts.last().unwrap().label, "Cancelled");
    }

    // The page itself.

    fn input(sale: TillSale) -> SalePageInput {
        SalePageInput {
            sale,
            till: SQUARE,
            location: Some("Front Counter".into()),
            refund_problem: None,
            refunding: false,
            taking: false,
        }
    }

    #[test]
    fn paid_offers_back_to_square_only_on_a_till_on_square_for_its_own_sale() {
        let at = at();
        let sale = sale_of("paidHere").build();
        assert_eq!(
            sale_page(&input(sale.clone()), la(at), &money).back_to,
            "Back to Square"
        );
        let clover_till = SalePageInput {
            till: CLOVER,
            ..input(sale)
        };
        assert_eq!(
            sale_page(&clover_till, la(at), &money).back_to,
            "",
            "Clover runs on its own device: nothing to go back to"
        );
    }

    #[test]
    fn the_closed_page_offers_the_refund_and_the_code_again_under_its_order() {
        let at = at();
        let sale = TillSale {
            receipt: Some(receipt(
                "closedHere",
                "2026-10-22T15:48:00-07:00",
                &[("Flat white", 2.0, 4.2, 8.4)],
                &[],
                8.4,
                "USD",
            )),
            ..sale_of("closedHere").closed_at(at).build()
        };
        let view = sale_page(&input(sale), la(at), &money);
        assert_eq!(view.title, "Sale #closedHere");
        assert_eq!(view.title_line, "3:48 PM · Front Counter · this till");
        assert_eq!(view.heading, "Closed · not paid");
        assert_eq!(view.icon, StateIcon::Clock);
        assert_eq!(view.ground, Ground::Warn);
        assert_eq!(view.amount, "$8.40");
        let order = view.order.unwrap();
        assert_eq!(
            order.summary,
            "Square receipt closedHere · 2 items · 8.40 USD"
        );
        assert_eq!(order.lines[0].label, "2 × Flat white");
        let presses: Vec<&str> = view
            .buttons
            .iter()
            .map(|b| b.press.label.as_str())
            .collect();
        assert_eq!(presses, ["Refund $8.40 in Square", "Show the code again"]);
    }

    #[test]
    fn a_refund_under_way_holds_its_button_and_a_refused_one_says_why() {
        let page = sale_page(
            &SalePageInput {
                refunding: true,
                taking: false,
                refund_problem: Some("Square refused the refund.".into()),
                ..input(
                    sale_of("e")
                        .phase(SalePhase::Expired)
                        .ended_at(at())
                        .build(),
                )
            },
            la(at()),
            &money,
        );
        assert!(!page.buttons[0].press.enabled);
        assert_eq!(
            page.refund_problem.as_deref(),
            Some("Square refused the refund.")
        );
        let paid = sale_page(
            &SalePageInput {
                refund_problem: Some("old".into()),
                ..input(sale_of("p").phase(SalePhase::Paid).paid_at(at()).build())
            },
            la(at()),
            &money,
        );
        assert_eq!(paid.refund_problem, None, "no Refund, no word on it");
        assert!(paid.buttons.is_empty());
    }

    #[test]
    fn another_tills_page_has_no_order_and_a_named_customer_leads_the_bar() {
        let theirs = sale_page(
            &input(sale_of("t").phase(SalePhase::Expired).theirs().build()),
            la(at()),
            &money,
        );
        assert_eq!(theirs.order, None);
        assert_eq!(theirs.title_line, "3:48 PM · From Back till");
        let named = sale_page(
            &input(TillSale {
                customer_name: Some("Ana López".into()),
                ..sale_of("m2Qe").phase(SalePhase::Paid).paid_at(at()).build()
            }),
            la(at()),
            &money,
        );
        assert_eq!(named.title, "Ana López");
        assert_eq!(
            named.title_line,
            "Sale #m2Qe · 3:48 PM · Front Counter · this till"
        );
    }

    fn cash(sale: TillSale) -> TillSale {
        let mut sale = sale;
        sale.found.as_mut().unwrap().cash = true;
        sale
    }

    #[test]
    fn an_offered_cash_sale_offers_take_it_in_monero_and_keep_as_cash() {
        let offered = TillSale {
            offered: true,
            customer_name: Some("Sam".into()),
            ..cash(sale_of("q8Lw").build())
        };
        let view = sale_page(&input(offered.clone()), la(at()), &money);
        assert_eq!(view.title, "Sam");
        assert_eq!(view.heading, "Cash in Square");
        assert_eq!(
            view.note.unwrap().text,
            "Rung up as Cash. If Sam would rather pay in Monero, make a payment code for this sale."
        );
        assert!(view.buttons.is_empty());
        let labels = |buttons: &[CashButton]| {
            buttons
                .iter()
                .map(|b| (b.action, b.press.label.clone(), b.press.enabled))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            labels(&view.cash),
            [
                (CashAction::TakeInMonero, "Take it in Monero".into(), true),
                (CashAction::KeepAsCash, "Keep as cash".into(), true),
            ]
        );
        let taking = sale_page(
            &SalePageInput {
                taking: true,
                ..input(offered.clone())
            },
            la(at()),
            &money,
        );
        assert_eq!(
            labels(&taking.cash)[0],
            (CashAction::TakeInMonero, "Making the code…".into(), false)
        );
        let kept = TillSale {
            kept_as_cash_at: Some(at()),
            ..offered
        };
        assert_eq!(
            labels(&cash_buttons(&kept, false)),
            [(CashAction::TakeInMonero, "Take it in Monero".into(), true)]
        );
    }

    #[test]
    fn a_cash_sale_is_never_refunded_and_offers_paid_in_cash() {
        let closed = cash(sale_of("c").closed_at(at()).build());
        let view = sale_page(&input(closed.clone()), la(at()), &money);
        assert!(
            view.buttons.iter().all(|b| b.action != PageAction::Refund),
            "{:?}",
            view.buttons
        );
        assert_eq!(view.cash[0].action, CashAction::PaidInCash);
        assert!(
            view.note
                .unwrap()
                .text
                .starts_with("Square has it as cash.")
        );
        let paid_in_cash = TillSale {
            paid_in_cash_at: Some(at()),
            ..closed
        };
        let view = sale_page(&input(paid_in_cash), la(at()), &money);
        assert_eq!(view.heading, "Paid in cash");
        assert!(view.cash.is_empty() && view.buttons.is_empty());
        assert!(
            cash_buttons(&sale_of("m").build(), false).is_empty(),
            "not cash"
        );
    }
}
