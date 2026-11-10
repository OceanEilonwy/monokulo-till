//! Every word on screen that names the point of sale (acceptance D5,
//! decision 136 of android.md; Android: `PosText.kt`): Square's and
//! Clover's from one exhaustive `match` each, so a point of sale added later
//! doesn't compile until it has its words, and no screen spells "Square"
//! itself. `None` is a till that runs without one (or hasn't chosen yet):
//! "your point of sale".
//!
//! The words that read a sale's point of sale ([`of`], [`elsewhere`],
//! [`place_of`]) take a [`TillSale`]. The reasons a Wake-ups status carries
//! are [`till_bell::pos_words`]'.

use monokulo_client::PointOfSale;
use till_bell::pos_words;
use till_sales::TillSale;

pub use crate::view::mark;

/// Whether `pos`'s mark is drawn in the theme's ink: Square's is one
/// colour, black or white; Clover's keeps its green.
pub fn mark_in_ink(pos: PointOfSale) -> bool {
    match pos {
        PointOfSale::Square => true,
        PointOfSale::Clover => false,
    }
}

/// "Square", "Clover", or "your point of sale" with none.
pub fn name(pos: Option<PointOfSale>) -> &'static str {
    pos.map_or("your point of sale", PointOfSale::display_name)
}

/// [`name`] at the start of a sentence: "Your point of sale" with none.
pub fn name_first(pos: Option<PointOfSale>) -> String {
    crate::view::capitalised(name(pos))
}

/// What the till watches at the point of sale: a Square location, a Clover
/// merchant (its only location, acceptance D1).
pub fn place(pos: Option<PointOfSale>) -> &'static str {
    pos_words::place(pos)
}

/// [`place`] as a row's name: "Location", "Merchant".
pub fn place_row(pos: Option<PointOfSale>) -> String {
    crate::view::capitalised(place(pos))
}

/// "Square location", "Clover merchant".
pub fn place_at(pos: Option<PointOfSale>) -> String {
    match pos {
        Some(pos) => format!("{} {}", pos.display_name(), place(Some(pos))),
        None => place(None).into(),
    }
}

/// "Square sign-in", "point-of-sale sign-in".
pub fn sign_in_of(pos: Option<PointOfSale>) -> String {
    format!(
        "{} sign-in",
        pos.map_or("point-of-sale", PointOfSale::display_name)
    )
}

pub fn sign_in(pos: Option<PointOfSale>) -> String {
    format!("Sign in to {}", name(pos))
}

pub fn sign_in_again(pos: Option<PointOfSale>) -> String {
    format!("Sign in to {} again", name(pos))
}

pub fn sign_out(pos: PointOfSale) -> String {
    format!("Sign out of {}", pos.display_name())
}

/// On the sign-in button while it waits.
pub const SIGNING_IN: &str = "Signing in…";

/// Settings' section and onboarding step 4's section.
pub const POINT_OF_SALE: &str = "Point of sale";
pub const CONNECT_HEADING: &str = "Connect your point of sale";
pub const CHANGE: &str = "Change the point of sale";

/// The choice rows' details (acceptance T1.1).
pub fn choice_detail(pos: PointOfSale) -> &'static str {
    match pos {
        PointOfSale::Square => "Square Point of Sale on a phone, tablet or Register",
        PointOfSale::Clover => "A Clover device",
    }
}

/// The Welcome screen's first feature: every point of sale the till knows.
pub fn welcome_feature() -> String {
    let names: Vec<_> = PointOfSale::ALL.iter().map(|p| p.display_name()).collect();
    format!(
        "Ring up in {} as usual and choose Monero as the payment.",
        names.join(" or ")
    )
}

// Step 4's card, for the point of sale chosen.

pub fn connect(pos: PointOfSale) -> String {
    format!("Connect {}", pos.display_name())
}

pub fn lead(pos: PointOfSale) -> String {
    format!(
        "Till checks {} for each Monero payment you take, so it can show the code straight away.",
        pos.display_name()
    )
}

pub fn account(pos: PointOfSale) -> String {
    format!("{} account", pos.display_name())
}

/// What the till may do at the point of sale, and that its sign-in stays
/// here (acceptance T2.6).
pub fn permissions(pos: PointOfSale) -> String {
    let can = match pos {
        PointOfSale::Square => {
            "Till can read payments, orders and locations, and refund a Monero sale that's never paid."
        }
        PointOfSale::Clover => "Till can read payments and orders, and add a Monero payment type.",
    };
    format!(
        "{can} Your {} sign-in stays on this device: Monokulo never sees it, or what was bought.",
        pos.display_name()
    )
}

/// The section over the steps at the point of sale itself.
pub fn at_the_till(pos: PointOfSale) -> &'static str {
    match pos {
        PointOfSale::Square => "In Square Point of Sale",
        PointOfSale::Clover => "On your Clover device",
    }
}

/// The title of Square's steps card.
pub const SQUARE_STEPS_TITLE: &str = "Add \"Monero\" as a payment type";

/// Square's steps, `**bold**` for what to tap.
pub const SQUARE_STEPS: [&str; 3] = [
    "Open **Settings › Checkout › Payment types**",
    "Under Other, add a type called **Monero**",
    "Ring up a sale and tap Monero to pay",
];

/// The title of Clover's steps card, as Square's names what its steps do.
pub const CLOVER_STEPS_TITLE: &str = "Take Monero in Clover Register";

/// Clover's first step, once the till has the Monero payment type
/// (`added`) or before it has looked.
pub fn clover_tender_step(added: bool) -> String {
    format!(
        "Till {} a payment type called **Monero** for you",
        if added { "added" } else { "adds" }
    )
}

/// Clover's steps after the payment type (acceptance T3.3), numbered from 2.
pub const CLOVER_STEPS: [&str; 2] = [
    "Open **Clover Register** on the Station, Mini, Flex or Go",
    "Ring up a sale, tap **Pay**, then choose **Monero**",
];

/// Clover refused to let the till add the payment type (acceptance T3.2,
/// decision 129).
pub const CLOVER_ADD_TENDER: &str = "Add a payment type called **Monero** in Clover (Setup › Payments, or ask whoever manages your Clover account), then tap **Look again**";
pub const LOOK_AGAIN: &str = "Look again";

/// Settings' payment type row while the Monero payment type isn't at `pos`
/// yet (decision 129).
pub fn tender_missing(pos: PointOfSale) -> String {
    format!("Not in {} yet", pos.display_name())
}

/// How a sale never paid is refunded, said before the first one is
/// (decision 130).
pub const CLOVER_REFUND_NOTE: &str =
    "A sale never paid in Monero is refunded in Clover's Orders app; Till shows you which.";

/// Step 4's test sale row while it waits: Clover's names where the refund
/// is made, as its card's footnote does.
pub fn test_sale_waiting(pos: PointOfSale) -> String {
    let after = match pos {
        PointOfSale::Square => "Any amount, you can refund it after.".to_owned(),
        PointOfSale::Clover => format!("Any amount; refund it in {} after.", refund_where(pos)),
    };
    format!("Waiting for a test sale… {after}")
}

pub fn test_sale_found(pos: PointOfSale, amount: &str) -> String {
    let at = match pos {
        PointOfSale::Square => pos.display_name(),
        PointOfSale::Clover => refund_where(pos),
    };
    format!("Test sale found: {amount}. Refund it in {at} when you like.")
}

// Square's locations, a step of its own.

pub fn locations_lead() -> String {
    format!(
        "Till shows the Monero sales {} takes there.",
        PointOfSale::Square.display_name()
    )
}

pub fn no_locations() -> String {
    format!(
        "{} has no active locations",
        PointOfSale::Square.display_name()
    )
}

// Steps 5 and 6.

/// The bubble floats over the point-of-sale app on this phone; beside a
/// Clover device, over whatever the phone shows.
pub fn float_over(pos: Option<PointOfSale>) -> &'static str {
    match pos {
        Some(PointOfSale::Square) => "Square",
        Some(PointOfSale::Clover) | None => "other apps",
    }
}

pub fn float_heading(pos: Option<PointOfSale>) -> String {
    format!("Float over {}", float_over(pos))
}

pub fn float_lead(pos: Option<PointOfSale>) -> String {
    format!(
        "Till puts a small bubble over {}. It's faded while there's nothing to do and lights up when a Monero sale comes in. Tap it to show the code.",
        float_over(pos)
    )
}

/// The Ready step's first sentence: where the cashier goes once Till starts.
pub fn ready_start(pos: Option<PointOfSale>) -> &'static str {
    match pos {
        Some(PointOfSale::Square) => "Start Till and switch to Square.",
        Some(PointOfSale::Clover) => "Start Till and ring up on your Clover device.",
        None => "Start Till.",
    }
}

pub fn ready_lead(pos: Option<PointOfSale>, bubble: bool) -> String {
    let wait = if bubble {
        format!(
            "Till waits in a faded bubble over {} and lights up when a sale comes in.",
            float_over(pos)
        )
    } else {
        "Till waits in its notification and alerts you when a sale comes in.".to_owned()
    };
    format!("{} {wait}", ready_start(pos))
}

/// The Ready screen's check row and Settings' fact: the point of sale's name.
pub fn check_name(pos: Option<PointOfSale>) -> &'static str {
    pos.map_or(POINT_OF_SALE, PointOfSale::display_name)
}

// Settings.

/// Under the signed-in facts.
pub fn signed_in_footnote(pos: PointOfSale) -> String {
    format!(
        "Sales paid with this payment type at this {} show up in Till.",
        place(Some(pos))
    )
}

/// "this iPad", "this iPhone", "this phone".
pub fn this_device(device: crate::view::Device) -> &'static str {
    match device {
        crate::view::Device::Ipad => "this iPad",
        crate::view::Device::Iphone => "this iPhone",
        crate::view::Device::Android => "this phone",
    }
}

/// Step 4's way past while there's no point of sale (board OnbPos, core.md
/// decision 121).
pub const SET_UP_LATER: &str = "Set up later";

/// Under "Set up later", naming `device`.
pub fn set_up_later_footnote(device: crate::view::Device) -> String {
    let this = this_device(device);
    format!(
        "Without a point of sale, {this} can't find new sales itself. It still shows sales that another till finds."
    )
}

/// Under the sign-in row, or the choice rows.
pub fn without_footnote(pos: Option<PointOfSale>) -> String {
    format!(
        "Without {}, Till shows the sales other tills make, without receipts.",
        pos.map_or("a point of sale", PointOfSale::display_name)
    )
}

pub fn bubble_footnote(pos: Option<PointOfSale>) -> String {
    format!(
        "A faded bubble floats over {} and lights up when a sale comes in. Drag it to either side.",
        float_over(pos)
    )
}

/// The debug build's field for a stand-in on the developer's machine
/// (acceptance T6.5).
pub fn stand_in(pos: PointOfSale) -> String {
    format!("Stand-in {} address", pos.display_name())
}

/// The Wake-ups status card's pace row.
pub fn checking(pos: Option<PointOfSale>) -> String {
    format!("Checking {}", name(pos))
}

// A sale's words, by the point of sale it was rung up at.

/// The point of sale `sale` was rung up at: the one that found it
/// ([`till_pos::FoundSale::point_of_sale`]), so it keeps that name after the
/// till's changes; another till's sale is named by the till's own, since
/// Monokulo doesn't say which point of sale took it (decision 141 of
/// android.md).
pub fn of(sale: &TillSale, till: Option<PointOfSale>) -> Option<PointOfSale> {
    sale.found
        .as_ref()
        .map(|found| found.point_of_sale)
        .or(till)
}

/// Whether this till rang `sale` up at a point of sale it has since changed
/// from (decision 141 of android.md).
pub fn elsewhere(sale: &TillSale, till: Option<PointOfSale>) -> bool {
    sale.found.is_some() && of(sale, till) != till
}

/// Where `sale` was rung up, by name: the till's `location` (its Square
/// location or Clover merchant) for a sale of the point of sale it watches
/// now, or another till's; for one rung up at the point of sale it changed
/// from, that point of sale's name, since the old location or merchant went
/// with its sign-in ("Clover · this till", never the new "Front Counter").
pub fn place_of<'a>(
    sale: &TillSale,
    till: Option<PointOfSale>,
    location: Option<&'a str>,
) -> Option<&'a str> {
    if elsewhere(sale, till) {
        of(sale, till).map(PointOfSale::display_name)
    } else {
        location
    }
}

pub fn refunded_in(pos: Option<PointOfSale>) -> String {
    format!("Refunded in {}", name(pos))
}

pub fn refund_it_in(pos: Option<PointOfSale>) -> String {
    format!("refund it in {}", name(pos))
}

pub fn refund_button(pos: Option<PointOfSale>, amount: &str) -> String {
    format!("Refund {amount} in {}", name(pos))
}

/// Another till's sale: its point of sale is on that till.
pub fn refund_on_other_till(pos: Option<PointOfSale>, till: &str) -> String {
    format!("Refund it in {} on {till}.", name(pos))
}

pub fn has_it_as_monero(pos: Option<PointOfSale>) -> String {
    format!("{} has this sale as Monero", name_first(pos))
}

/// What the till built a receipt from: Square's receipt, Clover's order
/// (Clover prints no receipt number, acceptance D7); a receipt kept after
/// signing out of either, just that.
pub fn receipt_label(pos: Option<PointOfSale>, receipt_id: &str) -> String {
    match pos {
        Some(PointOfSale::Square) => format!("Square receipt {receipt_id}"),
        Some(PointOfSale::Clover) => format!("Clover order {receipt_id}"),
        None => format!("Receipt {receipt_id}"),
    }
}

/// The button that returns to the point-of-sale app on this device, "Return
/// to Square" (core.md decision 224; "Back to" before).
pub fn back_to(pos: Option<PointOfSale>) -> String {
    format!(
        "Return to {}",
        pos.map_or("the till", PointOfSale::display_name)
    )
}

/// The shift's own footing, and the empty shift's hint.
pub fn waiting_hint(pos: Option<PointOfSale>) -> &'static str {
    match pos {
        Some(PointOfSale::Square) => {
            "The code appears here, and the bubble over Square lights up, when you tap Monero in Square."
        }
        Some(PointOfSale::Clover) => {
            "The code appears here, and the bubble lights up, when you choose Monero on the Clover device."
        }
        None => {
            "The code appears here, and the bubble lights up, when another till rings up a Monero sale."
        }
    }
}

pub fn start_hint(pos: Option<PointOfSale>) -> String {
    format!(
        "Start watching and Till checks {} for Monero sales until you stop it.",
        name(pos)
    )
}

/// The refund sheet's title (acceptance T4.1), for the point of sale that
/// took the sale.
pub fn refund_sheet_title(pos: PointOfSale) -> String {
    format!("Refund in {}", pos.display_name())
}

/// Where at `pos` the cashier refunds a sale.
pub fn refund_where(pos: PointOfSale) -> &'static str {
    match pos {
        PointOfSale::Square => "Square's Transactions",
        PointOfSale::Clover => "Clover's Orders app",
    }
}

/// What to refund, and where: the order by its id or title, when it was
/// rung up, the amount. `asks`: the till can ask `pos` whether it's done;
/// else (the till has changed its point of sale since) the cashier's word
/// is all it has (decision 138).
pub fn refund_sheet_body(
    pos: PointOfSale,
    asks: bool,
    order: Option<&str>,
    time: Option<&str>,
    amount: &str,
) -> String {
    let order = order.map(|o| format!("order {o}"));
    let what: Vec<&str> = [order.as_deref(), time, Some(amount)]
        .into_iter()
        .flatten()
        .collect();
    let then = if asks {
        "Till will notice when it's done.".to_owned()
    } else {
        format!("Tap {REFUNDED_IT} once it's done.")
    };
    format!(
        "This sale wasn't paid in Monero. Refund it in {}: {}, paid with Monero. {then}",
        refund_where(pos),
        what.join(", ")
    )
}

/// "I've refunded it" asked, and the point of sale doesn't show it.
pub fn refund_not_seen(pos: PointOfSale) -> String {
    format!(
        "{} doesn't show a refund yet. Finish it in {}, then tap again.",
        pos.display_name(),
        refund_where(pos)
    )
}

pub const REFUNDED_IT: &str = "I've refunded it";
pub const NOT_YET: &str = "Not yet";
pub const LOOKING: &str = "Looking…";

/// `text` without its `**bold**` marks, for a line drawn plain.
pub fn plain(text: &str) -> String {
    text.replace("**", "")
}

/// Ported from Android's `PosTextTest`, test for test, with "Monokular"
/// now "Till" (acceptance D5, T2.6, T3.2, T4.1; decision 136).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::PosMark;

    const SQUARE: PointOfSale = PointOfSale::Square;
    const CLOVER: PointOfSale = PointOfSale::Clover;

    fn each<T>(f: impl Fn(PointOfSale) -> T) -> Vec<T> {
        PointOfSale::ALL.into_iter().map(f).collect()
    }

    #[test]
    fn step_4_and_settings_name_the_point_of_sale_chosen() {
        assert_eq!(
            welcome_feature(),
            "Ring up in Square or Clover as usual and choose Monero as the payment."
        );
        assert_eq!(
            choice_detail(SQUARE),
            "Square Point of Sale on a phone, tablet or Register"
        );
        assert_eq!(choice_detail(CLOVER), "A Clover device");
        assert_eq!(connect(CLOVER), "Connect Clover");
        assert_eq!(
            lead(CLOVER),
            "Till checks Clover for each Monero payment you take, so it can show the code straight away."
        );
        assert_eq!(
            permissions(CLOVER),
            "Till can read payments and orders, and add a Monero payment type. Your Clover sign-in stays on this device: Monokulo never sees it, or what was bought."
        );
        assert_eq!(
            permissions(SQUARE),
            "Till can read payments, orders and locations, and refund a Monero sale that's never paid. Your Square sign-in stays on this device: Monokulo never sees it, or what was bought."
        );
        assert_eq!(at_the_till(CLOVER), "On your Clover device");
        assert_eq!(at_the_till(SQUARE), "In Square Point of Sale");
        assert_eq!(tender_missing(CLOVER), "Not in Clover yet");
        assert_eq!(
            each(|p| sign_in(Some(p))),
            ["Sign in to Square", "Sign in to Clover"]
        );
        assert_eq!(each(sign_out), ["Sign out of Square", "Sign out of Clover"]);
        assert_eq!(each(|p| place_row(Some(p))), ["Location", "Merchant"]);
        assert_eq!(
            each(stand_in),
            ["Stand-in Square address", "Stand-in Clover address"]
        );
        assert_eq!(
            signed_in_footnote(CLOVER),
            "Sales paid with this payment type at this merchant show up in Till."
        );
        assert_eq!(
            without_footnote(Some(CLOVER)),
            "Without Clover, Till shows the sales other tills make, without receipts."
        );
        assert_eq!(
            without_footnote(None),
            "Without a point of sale, Till shows the sales other tills make, without receipts."
        );
    }

    #[test]
    fn clovers_steps_card_reads_as_squares_with_its_test_sale_refunded_in_the_orders_app() {
        assert_eq!(CLOVER_STEPS_TITLE, "Take Monero in Clover Register");
        let mut steps = vec![clover_tender_step(true)];
        steps.extend(CLOVER_STEPS.map(String::from));
        assert_eq!(
            steps,
            [
                "Till added a payment type called **Monero** for you",
                "Open **Clover Register** on the Station, Mini, Flex or Go",
                "Ring up a sale, tap **Pay**, then choose **Monero**",
            ]
        );
        assert_eq!(
            clover_tender_step(false),
            "Till adds a payment type called **Monero** for you",
            "before the sign-in, or while the till looks, it hasn't added it yet"
        );
        assert_eq!(
            test_sale_waiting(CLOVER),
            "Waiting for a test sale… Any amount; refund it in Clover's Orders app after."
        );
        assert_eq!(
            test_sale_found(CLOVER, "$1.00"),
            "Test sale found: $1.00. Refund it in Clover's Orders app when you like."
        );
        assert_eq!(
            test_sale_waiting(SQUARE),
            "Waiting for a test sale… Any amount, you can refund it after."
        );
        assert_eq!(
            test_sale_found(SQUARE, "$1.00"),
            "Test sale found: $1.00. Refund it in Square when you like."
        );
    }

    #[test]
    fn each_point_of_sale_has_its_own_mark_squares_in_the_themes_ink_and_clovers_in_its_green() {
        assert_eq!(each(mark), [PosMark::Square, PosMark::Clover]);
        assert_eq!(each(mark_in_ink), [true, false]);
    }

    #[test]
    fn the_bubble_floats_over_squares_app_and_over_other_apps_beside_a_clover_device() {
        assert_eq!(float_heading(Some(SQUARE)), "Float over Square");
        assert_eq!(float_heading(Some(CLOVER)), "Float over other apps");
        assert_eq!(
            ready_lead(Some(CLOVER), true),
            "Start Till and ring up on your Clover device. Till waits in a faded bubble over other apps and lights up when a sale comes in."
        );
        assert_eq!(
            ready_lead(Some(SQUARE), false),
            "Start Till and switch to Square. Till waits in its notification and alerts you when a sale comes in."
        );
    }

    #[test]
    fn the_refund_sheet_says_what_to_refund_where_and_what_the_till_saw() {
        assert_eq!(refund_sheet_title(CLOVER), "Refund in Clover");
        assert_eq!(refund_sheet_title(SQUARE), "Refund in Square");
        assert_eq!(
            refund_sheet_body(CLOVER, true, Some("7F3KQ2MA"), Some("4:07 PM"), "$13.88"),
            "This sale wasn't paid in Monero. Refund it in Clover's Orders app: order 7F3KQ2MA, 4:07 PM, $13.88, paid with Monero. Till will notice when it's done."
        );
        assert_eq!(
            refund_sheet_body(CLOVER, true, None, None, "$13.88"),
            "This sale wasn't paid in Monero. Refund it in Clover's Orders app: $13.88, paid with Monero. Till will notice when it's done."
        );
        assert_eq!(
            refund_sheet_body(SQUARE, false, Some("bP9m"), None, "$13.88"),
            "This sale wasn't paid in Monero. Refund it in Square's Transactions: order bP9m, $13.88, paid with Monero. Tap I've refunded it once it's done.",
            "a sale of the point of sale the till changed from: only the cashier's word"
        );
        assert_eq!(
            refund_not_seen(CLOVER),
            "Clover doesn't show a refund yet. Finish it in Clover's Orders app, then tap again."
        );
    }

    #[test]
    fn a_receipt_is_squares_receipt_or_clovers_order() {
        assert_eq!(receipt_label(Some(SQUARE), "bP9m"), "Square receipt bP9m");
        assert_eq!(
            receipt_label(Some(CLOVER), "7F3KQ2MA"),
            "Clover order 7F3KQ2MA"
        );
        assert_eq!(receipt_label(None, "bP9m"), "Receipt bP9m");
        assert_eq!(back_to(Some(SQUARE)), "Return to Square");
    }
}
