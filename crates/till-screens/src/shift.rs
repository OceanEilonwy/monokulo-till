//! The shift, the till's home (boards PhoneShift and PadShift, built from
//! ShiftList; Android: `ShiftList.kt` and `ui/Shift.kt`'s logic): the top
//! row, the summary, the filter chips with Needs action's count, and the
//! list under them, its waiting tray first.
//!
//! Where the platforms differ: the idle hint names the bubble only on
//! Android, which has one.

use monokulo_client::{MoneyFormat, PointOfSale, SalePhase, amounts};
use till_sales::{Standing, TillSale};

use crate::close;
use crate::pos_text;
use crate::sale_text;
use crate::time::{DeviceClock, time};
use crate::view::{Device, Press, Tone};

/// The shift's filter chips.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ShiftFilter {
    All,
    NeedsAction,
    Paid,
}

impl ShiftFilter {
    pub const ALL: [ShiftFilter; 3] = [
        ShiftFilter::All,
        ShiftFilter::NeedsAction,
        ShiftFilter::Paid,
    ];

    /// The chip's words.
    pub fn label(self) -> &'static str {
        match self {
            ShiftFilter::All => "All",
            ShiftFilter::NeedsAction => "Needs action",
            ShiftFilter::Paid => "Paid",
        }
    }
}

/// What a shift row opens: the code, for a sale that's live, else the
/// sale's page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RowOpens {
    Code,
    Details,
}

/// The glyph in a row's circle, and on a sale page's hero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum StateIcon {
    Ring,
    Clock,
    Warning,
    Tick,
    Refund,
    Close,
}

/// Which of its corners a shift row rounds: the rows make one card,
/// rounded at its two ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RowEnds {
    Only,
    First,
    Middle,
    Last,
}

/// One item of the shift's list under its chips (the design's A4): the
/// waiting tray's header, a sale's row (in the tray or under Earlier), the
/// tray's fold row, and the Earlier header.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShiftItem<'a> {
    /// "Waiting for payment" over the tray, with how many sales are live.
    TrayHeader { count: usize },
    /// A sale's row, its corners `ends` counted within its section; `tray`
    /// when it sits in the waiting tray.
    Row {
        sale: &'a TillSale,
        ends: RowEnds,
        tray: bool,
    },
    /// The tray's fold: a stack and "+`hidden`" folded, "Show fewer" `open`.
    Fold { hidden: usize, open: bool },
    /// The header over the sales that are no longer live, under the tray.
    Earlier,
}

/// One shift row, in words and roles: what the screen draws for a sale.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ShiftRow {
    /// "Sale #m2Qe · $8.40".
    pub title: String,
    pub sub: String,
    pub tone: Tone,
    /// The title in muted, as a refunded sale's is: nothing more to do.
    pub quiet: bool,
    pub icon: StateIcon,
    pub opens: RowOpens,
}

/// How many live sales the folded tray shows before its fold row.
pub const FOLD_AFTER: usize = 3;

/// The tray's header.
pub const TRAY_HEADER: &str = "Waiting for payment";

/// The header over what is no longer live.
pub const EARLIER: &str = "Earlier";

fn is_live(sale: &TillSale) -> bool {
    matches!(sale.standing(), Standing::Live(_))
}

fn is_paid(sale: &TillSale) -> bool {
    matches!(sale.standing(), Standing::Paid { .. })
}

/// Which sales `filter` shows, newest first as the till keeps them.
pub fn rows(sales: &[TillSale], filter: ShiftFilter) -> Vec<&TillSale> {
    sales
        .iter()
        .filter(|sale| match filter {
            ShiftFilter::All => true,
            ShiftFilter::NeedsAction => sale.needs_action(),
            ShiftFilter::Paid => is_paid(sale),
        })
        .collect()
}

/// The shift's list under its chips (A4): with a live sale under `filter`,
/// the tray's header, every live sale newest first (only the first
/// [`FOLD_AFTER`] and the fold row while `folded` and there are more), then
/// the Earlier header and the rest newest first. With none, [`rows`] as they
/// always read: no header, no fold, no Earlier. Needs action shows the tray
/// whole, with no fold, then the sales ended unpaid; Paid has no live sale,
/// so no header.
pub fn sections(sales: &[TillSale], filter: ShiftFilter, folded: bool) -> Vec<ShiftItem<'_>> {
    let live: Vec<&TillSale> = if filter == ShiftFilter::Paid {
        Vec::new()
    } else {
        sales.iter().filter(|sale| is_live(sale)).collect()
    };
    let rest: Vec<&TillSale> = rows(sales, filter)
        .into_iter()
        .filter(|sale| !is_live(sale))
        .collect();
    if live.is_empty() {
        return section(&rest, false);
    }
    let folds = filter != ShiftFilter::NeedsAction && live.len() > FOLD_AFTER;
    let shown = if folds && folded {
        &live[..FOLD_AFTER]
    } else {
        &live[..]
    };
    let mut items = vec![ShiftItem::TrayHeader { count: live.len() }];
    items.extend(section(shown, true));
    if folds {
        items.push(ShiftItem::Fold {
            hidden: live.len() - FOLD_AFTER,
            open: !folded,
        });
    }
    if !rest.is_empty() {
        items.push(ShiftItem::Earlier);
        items.extend(section(&rest, false));
    }
    items
}

/// Whether `order_id`'s row is one the folded tray hides under `filter`: a
/// live sale after the first [`FOLD_AFTER`].
pub fn folds(sales: &[TillSale], filter: ShiftFilter, order_id: &str) -> bool {
    filter == ShiftFilter::All
        && sales
            .iter()
            .filter(|sale| is_live(sale))
            .position(|sale| sale.order_id == order_id)
            .is_some_and(|at| at >= FOLD_AFTER)
}

fn section<'a>(sales: &[&'a TillSale], tray: bool) -> Vec<ShiftItem<'a>> {
    let last = sales.len().saturating_sub(1);
    sales
        .iter()
        .enumerate()
        .map(|(i, sale)| ShiftItem::Row {
            sale,
            ends: ends(i, last),
            tray,
        })
        .collect()
}

/// The fold's pill: how many more there are folded ("+18"), else "Show
/// fewer".
pub fn fold_label(hidden: usize, open: bool) -> String {
    if open {
        "Show fewer".into()
    } else {
        format!("+{hidden}")
    }
}

fn sales_word(n: usize) -> &'static str {
    if n == 1 { "sale" } else { "sales" }
}

/// What a screen reader says for the tray's header.
pub fn tray_header_said(count: usize) -> String {
    format!("{TRAY_HEADER}, {count} {}", sales_word(count))
}

/// What a screen reader says for the fold row (its state, collapsed or
/// expanded, is said beside it).
pub fn fold_said(hidden: usize, open: bool) -> String {
    if open {
        "Show fewer".into()
    } else {
        format!("Show {hidden} more waiting {}", sales_word(hidden))
    }
}

/// What a new shift keeps of the last one's `sales`: those that still need
/// the cashier (decision 86 of android.md).
pub fn carried(sales: &[TillSale]) -> Vec<TillSale> {
    sales
        .iter()
        .filter(|sale| sale.needs_action())
        .cloned()
        .collect()
}

/// What the Needs action chip counts (decision 60 of android.md).
pub fn needs_action(sales: &[TillSale]) -> usize {
    sales.iter().filter(|sale| sale.needs_action()).count()
}

/// "5 paid · $69.50 · since 9:02 AM", or "Nothing paid yet · since 9:02
/// AM": the sales paid since the shift started summed per currency
/// (decision 72 of android.md). A sale carried over from the last shift,
/// paid after its refund, still needs the cashier but isn't this shift's
/// takings.
pub fn summary(
    sales: &[TillSale],
    started_at: Option<i64>,
    clock: DeviceClock,
    money: &dyn MoneyFormat,
) -> String {
    let paid: Vec<&TillSale> = sales
        .iter()
        .filter(|sale| {
            sale.phase == SalePhase::Paid
                && started_at.is_none_or(|start| sale.paid_at.unwrap_or(start) >= start)
        })
        .collect();
    let mut parts = if paid.is_empty() {
        vec!["Nothing paid yet".to_owned()]
    } else {
        let mut currencies: Vec<&str> = Vec::new();
        for sale in &paid {
            if !currencies.contains(&sale.currency.as_str()) {
                currencies.push(&sale.currency);
            }
        }
        let mut parts = vec![format!("{} paid", paid.len())];
        parts.extend(currencies.into_iter().map(|currency| {
            let those: Vec<&str> = paid
                .iter()
                .filter(|sale| sale.currency == currency)
                .map(|sale| sale.amount.as_str())
                .collect();
            total(&those, currency, money)
        }));
        parts
    };
    if let Some(start) = started_at {
        parts.push(format!("since {}", time(start, clock)));
    }
    parts.join(" · ")
}

/// `amounts` added up in `currency`, as the device writes money; an amount
/// that doesn't read counts as nothing.
fn total(amounts: &[&str], currency: &str, money: &dyn MoneyFormat) -> String {
    let code = currency.to_ascii_uppercase();
    match amounts::exponent(&code) {
        Some(exponent) => {
            let minor: i64 = amounts
                .iter()
                .filter_map(|amount| amounts::minor_units(amount, exponent))
                .sum();
            money.format(minor, &code)
        }
        None => {
            // Not ISO 4217's: summed to the most digits any amount has.
            let digits = amounts
                .iter()
                .map(|amount| amount.split_once('.').map_or(0, |(_, frac)| frac.len()))
                .max()
                .unwrap_or(0) as u32;
            let minor: i64 = amounts
                .iter()
                .filter_map(|amount| amounts::minor_units(amount, digits))
                .sum();
            let scale = 10i64.pow(digits);
            let sum = if digits == 0 {
                minor.to_string()
            } else {
                format!(
                    "{}{}.{:0width$}",
                    if minor < 0 { "-" } else { "" },
                    minor.abs() / scale,
                    minor.abs() % scale,
                    width = digits as usize
                )
            };
            format!("{sum} {code}")
        }
    }
}

/// Where a row goes: a live sale's code, any other sale's page (the
/// design's note s2).
pub fn opens(sale: &TillSale) -> RowOpens {
    if is_live(sale) {
        RowOpens::Code
    } else {
        RowOpens::Details
    }
}

/// The row for `sale` of point of sale `till` (the sale's own,
/// [`pos_text::of`]), with `seconds_left` on its code when it's waiting or
/// part paid.
pub fn row(
    sale: &TillSale,
    till: Option<PointOfSale>,
    seconds_left: i64,
    clock: DeviceClock,
    money: &dyn MoneyFormat,
) -> ShiftRow {
    let at = |when: Option<i64>| {
        when.map(|t| format!(" · {}", time(t, clock)))
            .unwrap_or_default()
    };
    let left = format!(" · {} left", sale_text::clock(seconds_left));
    let other = sale.device_name.as_deref().unwrap_or("another till");
    let standing = sale.standing();
    let (sub, tone, icon) = match standing {
        // The money on its way is the paid green, as its ring is (Board 5);
        // still waiting for it, the link's.
        Standing::Live(phase) => match phase {
            SalePhase::Partial => (format!("Part paid{left}"), Tone::Link, StateIcon::Ring),
            SalePhase::Seen => ("Seen on the network".into(), Tone::Paid, StateIcon::Ring),
            SalePhase::Confirming => (sale_text::confirming(sale), Tone::Paid, StateIcon::Ring),
            _ => (format!("Waiting{left}"), Tone::Link, StateIcon::Ring),
        },
        Standing::Paid {
            late, after_refund, ..
        } => (
            sale_text::status(sale, till) + &at(sale.paid_at),
            if after_refund {
                Tone::Warn
            } else if late {
                Tone::Paid
            } else {
                Tone::Muted
            },
            StateIcon::Tick,
        ),
        Standing::Closed => (
            format!("Closed · not paid · {}", pos_text::refund_it_in(till)),
            Tone::Warn,
            StateIcon::Clock,
        ),
        Standing::Expired => (sale_text::status(sale, till), Tone::Warn, StateIcon::Clock),
        Standing::Reversed => (
            "Payment reversed · don't hand it over".into(),
            Tone::Danger,
            StateIcon::Warning,
        ),
        Standing::Refunded => (
            pos_text::refunded_in(till) + &at(sale.refunded_at),
            Tone::Muted,
            StateIcon::Refund,
        ),
        Standing::RefundInPos => (
            crate::view::capitalised(&pos_text::refund_it_in(till))
                + &order_of(sale)
                + &at(sale.refund_pending_at),
            Tone::Warn,
            StateIcon::Refund,
        ),
        Standing::OtherTill => (
            if sale.phase.is_final() {
                format!("{} · on {other}", sale_text::status(sale, till))
            } else {
                format!("Closed · not paid · refund it on {other}")
            },
            Tone::Muted,
            StateIcon::Close,
        ),
    };
    ShiftRow {
        title: format!(
            "{} · {}",
            sale_text::title(sale),
            sale_text::amount(sale, money)
        ),
        sub,
        tone,
        quiet: standing == Standing::Refunded,
        icon,
        opens: opens(sale),
    }
}

/// " · order 7F3K…": the point of sale's order to refund, by the start of
/// its id, when the till has it.
fn order_of(sale: &TillSale) -> String {
    sale.found
        .as_ref()
        .and_then(|found| found.order_id.as_deref())
        .map(|id| format!(" · order {}…", id.chars().take(4).collect::<String>()))
        .unwrap_or_default()
}

/// The corners row `index` of rows up to `last_index` rounds: the card's
/// top on the first, its bottom on the last.
pub fn ends(index: usize, last_index: usize) -> RowEnds {
    match (index, last_index) {
        (0, 0) => RowEnds::Only,
        (0, _) => RowEnds::First,
        (i, last) if i == last => RowEnds::Last,
        _ => RowEnds::Middle,
    }
}

/// What an empty filter says where its rows would be.
pub fn empty(filter: ShiftFilter) -> &'static str {
    match filter {
        ShiftFilter::All => "No sale right now",
        ShiftFilter::NeedsAction => "Nothing needs action",
        ShiftFilter::Paid => "Nothing paid yet",
    }
}

/// The empty shift's hint while it watches, by the till's point of sale:
/// Android names the bubble; iOS, which has none, the code alone.
pub fn waiting_hint(pos: Option<PointOfSale>, device: Device) -> &'static str {
    if !device.is_ios() {
        return pos_text::waiting_hint(pos);
    }
    match pos {
        Some(PointOfSale::Square) => "The code appears here when you tap Monero in Square.",
        Some(PointOfSale::Clover) => {
            "The code appears here when you choose Monero on the Clover device."
        }
        None => "The code appears here when another till rings up a Monero sale.",
    }
}

/// What the shift shows.
#[derive(Clone, Debug)]
pub struct ShiftInput {
    /// The till's sales, newest first, as [`till_sales::TillState`] keeps
    /// them.
    pub sales: Vec<TillSale>,
    /// The till's point of sale (each sale's words follow its own,
    /// [`pos_text::of`]).
    pub till: Option<PointOfSale>,
    /// The store's name as last read; `None` until the first read answers.
    pub store: Option<String>,
    /// The till's Square location or Clover merchant.
    pub location: Option<String>,
    /// A shift is on.
    pub running: bool,
    /// The store's event stream is up.
    pub live: bool,
    /// The store disconnected this device.
    pub disconnected: bool,
    /// The last thing that went wrong.
    pub problem: Option<String>,
    /// The point of sale's sign-in has ended.
    pub sign_in_again: bool,
    /// When this shift began, if it's known.
    pub started_at: Option<i64>,
    /// The chip chosen.
    pub filter: ShiftFilter,
    /// The tray folded (its default).
    pub folded: bool,
}

/// The top row's state word, in the colour the dot had.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ShiftTop {
    /// The store and the location, said once when they're the same
    /// ("The Bean Room · Front Counter").
    pub place: Option<String>,
    /// "Watching", "Reconnecting…", "Not watching" or "Disconnected".
    pub state: String,
    pub tone: Tone,
    /// What a screen reader says for the row: "The Bean Room, Front
    /// Counter, Watching".
    pub said: String,
}

/// No sale yet, or not watching: the faded mark and what happens next.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ShiftIdle {
    pub title: String,
    pub hint: String,
    /// "Start watching", while no shift is on.
    pub start: Option<Press>,
}

/// One filter chip.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct FilterChip {
    pub filter: ShiftFilter,
    pub label: String,
    /// Needs action's count, when there is something to count.
    pub count: Option<u32>,
    pub selected: bool,
}

/// One item of the list, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ShiftEntry {
    TrayHeader {
        label: String,
        said: String,
    },
    Row {
        order_id: String,
        row: ShiftRow,
        ends: RowEnds,
        tray: bool,
        /// The row swipes to the close sheet: exactly when the sale has the
        /// X (a live sale with answers, [`close::choices`]).
        closes: bool,
    },
    Fold {
        label: String,
        said: String,
        open: bool,
    },
    Earlier {
        label: String,
    },
}

/// The store disconnected this device: nothing else shows.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ShiftDisconnected {
    pub heading: String,
    pub lead: String,
    pub connect_again: Press,
}

/// The shift, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ShiftView {
    pub top: ShiftTop,
    pub disconnected: Option<ShiftDisconnected>,
    pub problem: Option<String>,
    /// "Sign in to Square again", when the sign-in has ended.
    pub sign_in_again: Option<Press>,
    /// Under "This shift": the summary, or "Not watching".
    pub summary: String,
    pub idle: Option<ShiftIdle>,
    /// None until the shift has a sale.
    pub chips: Vec<FilterChip>,
    pub entries: Vec<ShiftEntry>,
    /// What the chosen filter says when it shows nothing.
    pub empty: Option<String>,
    /// A live sale's countdown is on screen: draw it again in a second.
    pub ticks: bool,
}

/// The shift at the clock's now, on `device`, amounts written by `money`.
pub fn shift(
    input: &ShiftInput,
    device: Device,
    clock: DeviceClock,
    money: &dyn MoneyFormat,
) -> ShiftView {
    let (state, tone) = if input.disconnected {
        ("Disconnected", Tone::Danger)
    } else if !input.running {
        ("Not watching", Tone::Muted)
    } else if input.live {
        ("Watching", Tone::Paid)
    } else {
        ("Reconnecting…", Tone::Muted)
    };
    // A Clover merchant is often named as the store is: said once.
    let mut place: Vec<&str> = Vec::new();
    for name in [input.store.as_deref(), input.location.as_deref()]
        .into_iter()
        .flatten()
    {
        if !place.contains(&name) {
            place.push(name);
        }
    }
    let mut said = place.clone();
    said.push(state);
    let top = ShiftTop {
        place: (!place.is_empty()).then(|| place.join(" · ")),
        state: state.into(),
        tone,
        said: said.join(", "),
    };
    if input.disconnected {
        return ShiftView {
            top,
            disconnected: Some(ShiftDisconnected {
                heading: "Disconnected".into(),
                lead: "The store disconnected this device. Connect it again with a new code from the store's settings.".into(),
                connect_again: Press::new("Connect again"),
            }),
            problem: None,
            sign_in_again: None,
            summary: String::new(),
            idle: None,
            chips: Vec::new(),
            entries: Vec::new(),
            empty: None,
            ticks: false,
        };
    }
    let sales = &input.sales;
    let idle = (!input.running || sales.is_empty()).then(|| ShiftIdle {
        title: if input.running {
            "No sale right now"
        } else {
            "Not watching for sales"
        }
        .into(),
        hint: if input.running {
            waiting_hint(input.till, device).into()
        } else {
            pos_text::start_hint(input.till)
        },
        start: (!input.running).then(|| Press::new("Start watching")),
    });
    let count = needs_action(sales);
    let chips = if sales.is_empty() {
        Vec::new()
    } else {
        ShiftFilter::ALL
            .into_iter()
            .map(|filter| FilterChip {
                filter,
                label: filter.label().into(),
                count: (filter == ShiftFilter::NeedsAction && count > 0).then_some(count as u32),
                selected: filter == input.filter,
            })
            .collect()
    };
    let items = sections(sales, input.filter, input.folded);
    let mut ticks = false;
    let entries = items
        .iter()
        .map(|item| match *item {
            ShiftItem::TrayHeader { count } => ShiftEntry::TrayHeader {
                label: TRAY_HEADER.into(),
                said: tray_header_said(count),
            },
            ShiftItem::Earlier => ShiftEntry::Earlier {
                label: EARLIER.into(),
            },
            ShiftItem::Fold { hidden, open } => ShiftEntry::Fold {
                label: fold_label(hidden, open),
                said: fold_said(hidden, open),
                open,
            },
            ShiftItem::Row { sale, ends, tray } => {
                let live = sale.live();
                ticks |= live && sale.phase.counts_down();
                let seconds = if live {
                    sale_text::seconds_left(sale.expires_at, clock.now)
                } else {
                    0
                };
                ShiftEntry::Row {
                    order_id: sale.order_id.clone(),
                    row: row(sale, pos_text::of(sale, input.till), seconds, clock, money),
                    ends,
                    tray,
                    closes: !close::choices(sale, false).is_empty(),
                }
            }
        })
        .collect();
    ShiftView {
        top,
        disconnected: None,
        problem: input.problem.clone(),
        sign_in_again: input
            .sign_in_again
            .then(|| Press::new(pos_text::sign_in_again(input.till))),
        summary: if input.running {
            summary(sales, input.started_at, clock, money)
        } else {
            "Not watching".into()
        },
        idle,
        chips,
        empty: (!sales.is_empty() && items.is_empty()).then(|| empty(input.filter).into()),
        entries,
        ticks,
    }
}

/// Ported from Android's `ShiftListTest`, test for test. Its tests of
/// `TillNav` and `PaidBuzz` are [`crate::till_nav`]'s and
/// [`crate::bubble`]'s; of `SaleHeader` and `SalePage`,
/// [`crate::sale_page`]'s; of `Closing`, [`crate::close`]'s.
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::fixtures::{found, instant, la, money, till_sale};
    use crate::view::Tone;

    /// 3:58 PM in Los Angeles.
    pub(crate) fn at() -> i64 {
        instant("2026-10-22T22:58:00Z")
    }

    /// The Android test's `sale(...)`: $8.40 of this till's, unless told
    /// otherwise.
    pub(crate) struct SaleOf {
        id: String,
        phase: SalePhase,
        mine: bool,
        amount: String,
        currency: String,
        closed_at: Option<i64>,
        refunded_at: Option<i64>,
        paid_at: Option<i64>,
        ended_at: Option<i64>,
        device: Option<String>,
    }

    pub(crate) fn sale_of(id: &str) -> SaleOf {
        SaleOf {
            id: id.into(),
            phase: SalePhase::Waiting,
            mine: true,
            amount: "8.40".into(),
            currency: "USD".into(),
            closed_at: None,
            refunded_at: None,
            paid_at: None,
            ended_at: None,
            device: Some("Back till".into()),
        }
    }

    impl SaleOf {
        pub(crate) fn phase(mut self, phase: SalePhase) -> Self {
            self.phase = phase;
            self
        }
        pub(crate) fn theirs(mut self) -> Self {
            self.mine = false;
            self
        }
        pub(crate) fn device(mut self, device: Option<&str>) -> Self {
            self.device = device.map(Into::into);
            self
        }
        pub(crate) fn amount(mut self, amount: &str) -> Self {
            self.amount = amount.into();
            self
        }
        pub(crate) fn currency(mut self, currency: &str) -> Self {
            self.currency = currency.into();
            self
        }
        pub(crate) fn closed_at(mut self, at: i64) -> Self {
            self.closed_at = Some(at);
            self
        }
        pub(crate) fn refunded_at(mut self, at: i64) -> Self {
            self.refunded_at = Some(at);
            self
        }
        pub(crate) fn paid_at(mut self, at: i64) -> Self {
            self.paid_at = Some(at);
            self
        }
        pub(crate) fn ended_at(mut self, at: i64) -> Self {
            self.ended_at = Some(at);
            self
        }
        pub(crate) fn build(self) -> TillSale {
            let id = self.id.as_str();
            TillSale {
                reference: Some(id.into()),
                code: till_sales::PaymentCode {
                    uri: "monero:4abc?tx_amount=0.1".into(),
                    has_receipt: self.mine,
                    why: None,
                },
                phase: self.phase,
                found: self.mine.then(|| {
                    found(
                        &format!("p_{id}"),
                        840,
                        &self.currency,
                        Some(id),
                        "o",
                        "2026-10-22T22:50:00Z",
                    )
                }),
                device_name: if self.mine { None } else { self.device.clone() },
                created_at: at() - 600,
                paid_at: self.paid_at,
                ended_at: self.ended_at,
                closed_at: self.closed_at,
                refunded_at: self.refunded_at,
                ..till_sale(id, &self.amount, &self.currency, "0.0336")
            }
        }
    }

    /// Acceptance §12's fixtures, newest first.
    pub(crate) fn fixture_shift() -> Vec<TillSale> {
        use SalePhase::*;
        let at = at();
        vec![
            sale_of("waiting").build(),
            sale_of("closedHere").closed_at(at).build(),
            sale_of("closedTheirs").theirs().closed_at(at).build(),
            sale_of("expired").phase(Expired).ended_at(at).build(),
            sale_of("expiredTheirs")
                .phase(Expired)
                .theirs()
                .ended_at(at)
                .build(),
            sale_of("reversed").phase(DoubleSpend).ended_at(at).build(),
            sale_of("refunded")
                .phase(Expired)
                .closed_at(at - 60)
                .refunded_at(at)
                .build(),
            sale_of("paid")
                .phase(Paid)
                .amount("24.50")
                .paid_at(at)
                .build(),
            sale_of("latePaid")
                .phase(Paid)
                .amount("6.00")
                .closed_at(at - 900)
                .paid_at(at)
                .build(),
            sale_of("paidAfterRefund")
                .phase(Paid)
                .amount("11.00")
                .closed_at(at - 900)
                .refunded_at(at - 800)
                .paid_at(at)
                .build(),
            sale_of("cancelled").phase(Cancelled).ended_at(at).build(),
        ]
    }

    pub(crate) fn one(shift: &[TillSale], id: &str) -> TillSale {
        shift.iter().find(|s| s.order_id == id).unwrap().clone()
    }

    fn ids<'a>(sales: impl IntoIterator<Item = &'a TillSale>) -> Vec<&'a str> {
        sales.into_iter().map(|s| s.order_id.as_str()).collect()
    }

    const SQUARE: Option<PointOfSale> = Some(PointOfSale::Square);
    const CLOVER: Option<PointOfSale> = Some(PointOfSale::Clover);

    #[test]
    fn needs_action_counts_what_this_till_can_still_refund_and_a_payment_after_the_refund() {
        let shift = fixture_shift();
        assert_eq!(needs_action(&shift), 5);
        assert_eq!(
            ids(rows(&shift, ShiftFilter::NeedsAction)),
            [
                "closedHere",
                "expired",
                "reversed",
                "paidAfterRefund",
                "cancelled"
            ],
            "never another till's, a refunded, a paid or a waiting sale"
        );
    }

    #[test]
    fn a_new_shift_keeps_the_last_ones_sales_that_still_need_action_and_only_those() {
        let shift = fixture_shift();
        assert_eq!(
            ids(rows(&shift, ShiftFilter::NeedsAction)),
            ids(&carried(&shift))
        );
        assert!(carried(&[sale_of("w").build()]).is_empty());
    }

    #[test]
    fn the_filters_keep_the_shifts_order_newest_first() {
        let shift = fixture_shift();
        assert_eq!(ids(rows(&shift, ShiftFilter::All)), ids(&shift));
        assert_eq!(
            ids(rows(&shift, ShiftFilter::Paid)),
            ["paid", "latePaid", "paidAfterRefund"]
        );
        assert!(rows(&[sale_of("w").build()], ShiftFilter::NeedsAction).is_empty());
        assert_eq!(empty(ShiftFilter::NeedsAction), "Nothing needs action");
        assert_eq!(empty(ShiftFilter::Paid), "Nothing paid yet");
    }

    #[test]
    fn the_summary_counts_paid_sales_sums_them_per_currency_and_says_since_when() {
        use SalePhase::Paid;
        let shift = fixture_shift();
        let started = instant("2026-10-22T16:02:00Z");
        let clock = la(at());
        assert_eq!(
            summary(&shift, Some(started), clock, &money),
            "3 paid · $41.50 · since 9:02 AM"
        );
        assert_eq!(
            summary(&[sale_of("w").build()], Some(started), clock, &money),
            "Nothing paid yet · since 9:02 AM"
        );
        assert_eq!(summary(&[], None, clock, &money), "Nothing paid yet");
        let mixed = [
            sale_of("a").phase(Paid).amount("1.25").build(),
            sale_of("b")
                .phase(Paid)
                .amount("500")
                .currency("JPY")
                .build(),
            sale_of("c").phase(Paid).amount("2.75").build(),
        ];
        assert_eq!(
            summary(&mixed, None, clock, &money),
            "3 paid · $4.00 · ¥500"
        );
        // A sale the last shift carried over, paid after its refund, isn't
        // this shift's takings.
        let carried = sale_of("paidAfterRefund")
            .phase(Paid)
            .amount("11.00")
            .closed_at(started - 900)
            .refunded_at(started - 800)
            .paid_at(started - 60)
            .build();
        let paid = sale_of("paid")
            .phase(Paid)
            .amount("24.50")
            .paid_at(at())
            .build();
        assert_eq!(
            summary(&[carried, paid], Some(started), clock, &money),
            "1 paid · $24.50 · since 9:02 AM"
        );
    }

    #[test]
    fn a_live_sales_row_opens_its_code_every_other_row_its_page() {
        use SalePhase::*;
        let live = [
            sale_of("w").build(),
            sale_of("s").phase(Seen).build(),
            sale_of("c").phase(Confirming).build(),
            sale_of("p").phase(Partial).build(),
            sale_of("t").theirs().build(),
        ];
        assert!(live.iter().all(|s| opens(s) == RowOpens::Code));
        let shift = fixture_shift();
        assert_eq!(shift[1..].len(), 10);
        assert!(shift[1..].iter().all(|s| opens(s) == RowOpens::Details));
    }

    fn triple(
        sale: &TillSale,
        till: Option<PointOfSale>,
        seconds: i64,
    ) -> (String, Tone, StateIcon) {
        let row = row(sale, till, seconds, la(at()), &money);
        (row.sub, row.tone, row.icon)
    }

    fn t(sub: &str, tone: Tone, icon: StateIcon) -> (String, Tone, StateIcon) {
        (sub.into(), tone, icon)
    }

    #[test]
    fn each_row_says_how_its_sale_stands_in_its_states_colour() {
        use SalePhase::*;
        let r = |sale: &TillSale| triple(sale, SQUARE, 552);
        assert_eq!(
            r(&sale_of("w").build()),
            t("Waiting · 9:12 left", Tone::Link, StateIcon::Ring)
        );
        assert_eq!(
            r(&sale_of("w").phase(Seen).build()),
            t("Seen on the network", Tone::Paid, StateIcon::Ring),
            "the money on its way is green"
        );
        assert_eq!(
            r(&sale_of("w").phase(Confirming).build()),
            t("Confirming", Tone::Paid, StateIcon::Ring),
            "until Monokulo says how many"
        );
        let counted = TillSale {
            confirmations: Some(4),
            confirmations_required: Some(10),
            ..sale_of("w").phase(Confirming).build()
        };
        assert_eq!(
            r(&counted),
            t("Confirming · 4 of 10", Tone::Paid, StateIcon::Ring)
        );
        assert_eq!(
            r(&sale_of("w").phase(Partial).build()),
            t("Part paid · 9:12 left", Tone::Link, StateIcon::Ring)
        );
        let shift = fixture_shift();
        let row_of = |id: &str| r(&one(&shift, id));
        assert_eq!(
            row_of("closedHere"),
            t(
                "Closed · not paid · refund it in Square",
                Tone::Warn,
                StateIcon::Clock
            )
        );
        assert_eq!(
            row_of("closedTheirs"),
            t(
                "Closed · not paid · refund it on Back till",
                Tone::Muted,
                StateIcon::Close
            )
        );
        assert_eq!(
            row_of("expired"),
            t("Code expired · not paid", Tone::Warn, StateIcon::Clock)
        );
        assert_eq!(
            row_of("expiredTheirs"),
            t(
                "Code expired · not paid · on Back till",
                Tone::Muted,
                StateIcon::Close
            )
        );
        assert_eq!(
            row_of("cancelled"),
            t("Cancelled · not paid", Tone::Warn, StateIcon::Clock)
        );
        assert_eq!(
            row_of("reversed"),
            t(
                "Payment reversed · don't hand it over",
                Tone::Danger,
                StateIcon::Warning
            )
        );
        assert_eq!(
            row_of("refunded"),
            t(
                "Refunded in Square · 3:58 PM",
                Tone::Muted,
                StateIcon::Refund
            )
        );
        assert_eq!(
            row_of("paid"),
            t("Paid · 3:58 PM", Tone::Muted, StateIcon::Tick)
        );
        assert_eq!(
            row_of("latePaid"),
            t(
                "Paid after you closed it · 3:58 PM",
                Tone::Paid,
                StateIcon::Tick
            )
        );
        assert_eq!(
            row_of("paidAfterRefund"),
            t(
                "Paid after the refund · 3:58 PM",
                Tone::Warn,
                StateIcon::Tick
            )
        );
        assert_eq!(
            r(&sale_of("x")
                .phase(Paid)
                .ended_at(at() - 60)
                .paid_at(at())
                .build()),
            t(
                "Paid after the code expired · 3:58 PM",
                Tone::Paid,
                StateIcon::Tick
            )
        );
        assert_eq!(
            r(&sale_of("x").phase(Expired).theirs().device(None).build()),
            t(
                "Code expired · not paid · on another till",
                Tone::Muted,
                StateIcon::Close
            )
        );
        let refunded = row(&one(&shift, "refunded"), SQUARE, 0, la(at()), &money);
        assert_eq!(
            refunded.title,
            format!(
                "Sale #refunded · {}",
                sale_text::amount(&sale_of("refunded").build(), &money)
            ),
            "the amount as the device writes it"
        );
        assert!(refunded.quiet, "a refunded sale's title is muted too");
        assert!(!row(&sale_of("w").build(), SQUARE, 0, la(at()), &money).quiet);
    }

    #[test]
    fn a_clover_tills_rows_and_pages_name_clover_and_a_sale_to_refund_in_clover_needs_action() {
        use crate::close::{self, CloseChoice};
        use crate::sale_page::{PageAction, page};
        use SalePhase::*;
        let at = at();
        let sub = |sale: &TillSale| triple(sale, CLOVER, 0).0;
        assert_eq!(
            sub(&sale_of("c").closed_at(at).build()),
            "Closed · not paid · refund it in Clover"
        );
        assert_eq!(
            sub(&sale_of("r")
                .phase(Expired)
                .closed_at(at - 60)
                .refunded_at(at)
                .build()),
            "Refunded in Clover · 3:58 PM"
        );
        let pending = TillSale {
            refund_pending_at: Some(at),
            ..sale_of("p").phase(Expired).ended_at(at - 60).build()
        };
        assert_eq!(
            triple(&pending, CLOVER, 0),
            t(
                "Refund it in Clover · order o… · 3:58 PM",
                Tone::Warn,
                StateIcon::Refund
            )
        );
        assert_eq!(
            needs_action(std::slice::from_ref(&pending)),
            1,
            "told to refund it and not yet seen: it needs the cashier"
        );
        assert_eq!(
            sale_text::status(&pending, CLOVER),
            "Not paid · refund it in Clover"
        );
        let p = page(&pending, CLOVER, la(at), false);
        assert_eq!(p.actions, [PageAction::Refund]);
        assert_eq!(
            p.note.as_ref().map(|n| n.text.as_str()),
            Some(
                "Refund this sale in Clover, then tap Refund to say it's done. While the shift is on, Till notices by itself too."
            )
        );
        assert_eq!(p.facts.last().unwrap().label, "Told to refund it");
        assert_eq!(
            page(&sale_of("c").closed_at(at).build(), CLOVER, la(at), false)
                .note
                .map(|n| n.text),
            Some(
                "Clover has this sale as Monero. If the customer isn't paying, refund it. Monokulo keeps watching for a late payment for a while: if one arrives, this sale turns paid.".into()
            )
        );
        let w = sale_of("w").build();
        assert_eq!(
            close::title(CloseChoice::RefundIt, &w, CLOVER),
            "No: refund it in Clover"
        );
        assert_eq!(
            close::detail(CloseChoice::RefundIt, &w, CLOVER, PointOfSale::Clover, 0),
            "Till closes it and shows you how to refund it in Clover."
        );
        assert_eq!(
            close::detail(CloseChoice::RefundIt, &w, SQUARE, PointOfSale::Square, 0),
            "Till refunds the sale and closes it."
        );
        assert_eq!(
            close::detail(CloseChoice::RefundIt, &w, SQUARE, PointOfSale::Clover, 0),
            "Till closes it and shows you how to refund it in Square.",
            "a Square sale on a till now on Clover: refunded in Square, by the cashier"
        );
    }

    #[test]
    fn a_till_with_no_point_of_sale_says_so_in_plain_words() {
        assert_eq!(
            triple(&sale_of("c").closed_at(at()).build(), None, 0).0,
            "Closed · not paid · refund it in your point of sale"
        );
        assert_eq!(
            crate::bubble::panel_ended_line(
                &sale_of("t").phase(SalePhase::Expired).theirs().build(),
                None
            ),
            "Refund it in your point of sale on Back till."
        );
    }

    #[test]
    fn the_rows_make_one_card_rounded_at_its_top_and_bottom() {
        assert_eq!(ends(0, 0), RowEnds::Only, "one row is the whole card");
        assert_eq!(ends(0, 2), RowEnds::First);
        assert_eq!(ends(1, 2), RowEnds::Middle);
        assert_eq!(ends(2, 2), RowEnds::Last);
        assert_eq!(
            ends(1, 1),
            RowEnds::Last,
            "of two, the second is the bottom"
        );
    }

    /// `n` live sales, newest first, then `ended` that aren't: as the till
    /// keeps a busy shift.
    fn busy(n: usize, ended: Option<Vec<TillSale>>) -> Vec<TillSale> {
        let ended = ended.unwrap_or_else(|| {
            vec![
                sale_of("paid").phase(SalePhase::Paid).paid_at(at()).build(),
                sale_of("expired")
                    .phase(SalePhase::Expired)
                    .ended_at(at())
                    .build(),
            ]
        });
        let mut sales: Vec<_> = (0..n)
            .map(|i| sale_of(&format!("w{}", n - i)).build())
            .collect();
        sales.extend(ended);
        sales
    }

    /// The items in words, so a test reads as the list does.
    fn read(items: &[ShiftItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                ShiftItem::TrayHeader { count } => format!("header {count}"),
                ShiftItem::Row { sale, ends, tray } => format!(
                    "{}{} {ends:?}",
                    if *tray { "tray " } else { "" },
                    sale.order_id
                ),
                ShiftItem::Fold { hidden, open } => fold_label(*hidden, *open),
                ShiftItem::Earlier => "earlier".into(),
            })
            .collect()
    }

    #[test]
    fn the_tray_folds_after_three_rows_and_says_how_many_more_wait() {
        assert_eq!(
            read(&sections(&busy(4, None), ShiftFilter::All, true)),
            [
                "header 4",
                "tray w4 First",
                "tray w3 Middle",
                "tray w2 Last",
                "+1",
                "earlier",
                "paid First",
                "expired Last"
            ]
        );
        let sales = busy(21, None);
        let many = sections(&sales, ShiftFilter::All, true);
        assert_eq!(
            many[0],
            ShiftItem::TrayHeader { count: 21 },
            "the header counts every live sale, folded ones too"
        );
        assert_eq!(
            many.iter()
                .filter(|i| matches!(i, ShiftItem::Row { tray: true, .. }))
                .count(),
            3
        );
        let folds: Vec<_> = many
            .iter()
            .filter_map(|i| match i {
                ShiftItem::Fold { hidden, open } => Some(fold_label(*hidden, *open)),
                _ => None,
            })
            .collect();
        assert_eq!(folds, ["+18"]);
    }

    #[test]
    fn the_tray_opens_in_place_and_says_show_fewer() {
        let sales = busy(21, None);
        let open = sections(&sales, ShiftFilter::All, false);
        let tray: Vec<(usize, &TillSale, RowEnds)> = open
            .iter()
            .enumerate()
            .filter_map(|(i, item)| match item {
                ShiftItem::Row {
                    sale,
                    ends,
                    tray: true,
                } => Some((i, *sale, *ends)),
                _ => None,
            })
            .collect();
        let expected: Vec<String> = (1..=21).rev().map(|n| format!("w{n}")).collect();
        assert_eq!(
            tray.iter()
                .map(|(_, s, _)| s.order_id.clone())
                .collect::<Vec<_>>(),
            expected,
            "every live sale, newest first"
        );
        let mut ends_expected = vec![RowEnds::First];
        ends_expected.extend(std::iter::repeat_n(RowEnds::Middle, 19));
        ends_expected.push(RowEnds::Last);
        assert_eq!(
            tray.iter().map(|(_, _, e)| *e).collect::<Vec<_>>(),
            ends_expected
        );
        let (at, fold) = open
            .iter()
            .enumerate()
            .find(|(_, i)| matches!(i, ShiftItem::Fold { .. }))
            .unwrap();
        assert_eq!(
            *fold,
            ShiftItem::Fold {
                hidden: 18,
                open: true
            }
        );
        assert_eq!(fold_label(18, true), "Show fewer");
        assert_eq!(
            tray.last().unwrap().0 + 1,
            at,
            "the fold row closes the tray"
        );
    }

    #[test]
    fn two_waiting_sales_have_no_fold() {
        assert_eq!(
            read(&sections(&busy(2, None), ShiftFilter::All, true)),
            [
                "header 2",
                "tray w2 First",
                "tray w1 Last",
                "earlier",
                "paid First",
                "expired Last"
            ]
        );
        assert_eq!(
            read(&sections(&busy(3, Some(vec![])), ShiftFilter::All, true)),
            [
                "header 3",
                "tray w3 First",
                "tray w2 Middle",
                "tray w1 Last"
            ]
        );
        assert_eq!(
            read(&sections(&busy(1, Some(vec![])), ShiftFilter::All, true)),
            ["header 1", "tray w1 Only"],
            "and no Earlier with nothing under it"
        );
    }

    #[test]
    fn with_no_live_sale_the_list_reads_as_it_always_has() {
        let shift = fixture_shift();
        let ended = &shift[1..];
        for filter in ShiftFilter::ALL {
            let items = sections(ended, filter, true);
            assert!(
                items
                    .iter()
                    .all(|i| matches!(i, ShiftItem::Row { tray: false, .. })),
                "{filter:?}: no header, fold or Earlier"
            );
            let rows = rows(ended, filter);
            let last = rows.len().saturating_sub(1);
            let expected: Vec<ShiftItem> = rows
                .iter()
                .enumerate()
                .map(|(i, sale)| ShiftItem::Row {
                    sale,
                    ends: ends(i, last),
                    tray: false,
                })
                .collect();
            assert_eq!(items, expected);
        }
        assert!(sections(&[], ShiftFilter::All, true).is_empty());
    }

    #[test]
    fn needs_action_shows_the_whole_tray_then_what_ended_unpaid() {
        let sales = busy(5, None);
        let items = read(&sections(&sales, ShiftFilter::NeedsAction, true));
        assert_eq!(
            items,
            [
                "header 5",
                "tray w5 First",
                "tray w4 Middle",
                "tray w3 Middle",
                "tray w2 Middle",
                "tray w1 Last",
                "earlier",
                "expired Only"
            ],
            "unfolded, with no fold row, whatever the fold's state; the paid sale isn't one that needs action"
        );
        assert_eq!(
            items,
            read(&sections(&sales, ShiftFilter::NeedsAction, false))
        );
    }

    #[test]
    fn paid_has_no_tray_and_no_headers() {
        assert_eq!(
            read(&sections(&busy(5, None), ShiftFilter::Paid, true)),
            ["paid Only"]
        );
    }

    #[test]
    fn both_sections_keep_the_shifts_order_newest_first() {
        // The till keeps the shift newest first; a sale paid since lands at
        // the top of Earlier, a new one at the top of the tray.
        let sales = [
            sale_of("new").build(),
            sale_of("justPaid")
                .phase(SalePhase::Paid)
                .paid_at(at())
                .build(),
            sale_of("old").build(),
            sale_of("expired")
                .phase(SalePhase::Expired)
                .ended_at(at())
                .build(),
        ];
        assert_eq!(
            read(&sections(&sales, ShiftFilter::All, true)),
            [
                "header 2",
                "tray new First",
                "tray old Last",
                "earlier",
                "justPaid First",
                "expired Last"
            ]
        );
    }

    #[test]
    fn talk_back_counts_the_tray_and_the_fold_in_words() {
        assert_eq!(tray_header_said(1), "Waiting for payment, 1 sale");
        assert_eq!(tray_header_said(21), "Waiting for payment, 21 sales");
        assert_eq!(fold_said(1, false), "Show 1 more waiting sale");
        assert_eq!(fold_said(18, false), "Show 18 more waiting sales");
        assert_eq!(fold_said(18, true), "Show fewer");
    }

    #[test]
    fn folds_says_which_rows_the_folded_tray_hides() {
        let sales = busy(5, None);
        assert!(!folds(&sales, ShiftFilter::All, "w5"));
        assert!(folds(&sales, ShiftFilter::All, "w2"));
        assert!(!folds(&sales, ShiftFilter::NeedsAction, "w2"));
    }

    // The screen.

    fn input(sales: Vec<TillSale>) -> ShiftInput {
        ShiftInput {
            sales,
            till: SQUARE,
            store: Some("The Bean Room".into()),
            location: Some("Front Counter".into()),
            running: true,
            live: true,
            disconnected: false,
            problem: None,
            sign_in_again: false,
            started_at: Some(instant("2026-10-22T16:02:00Z")),
            filter: ShiftFilter::All,
            folded: true,
        }
    }

    #[test]
    fn the_shift_draws_its_top_row_summary_chips_and_rows() {
        let waiting = TillSale {
            expires_at: at() + 552,
            ..sale_of("waiting").build()
        };
        let mut sales = fixture_shift();
        sales[0] = waiting;
        let view = shift(&input(sales), Device::Iphone, la(at()), &money);
        assert_eq!(
            view.top,
            ShiftTop {
                place: Some("The Bean Room · Front Counter".into()),
                state: "Watching".into(),
                tone: Tone::Paid,
                said: "The Bean Room, Front Counter, Watching".into(),
            }
        );
        assert_eq!(view.summary, "3 paid · $41.50 · since 9:02 AM");
        assert_eq!(view.idle, None);
        assert_eq!(
            view.chips
                .iter()
                .map(|c| (c.label.as_str(), c.count, c.selected))
                .collect::<Vec<_>>(),
            [
                ("All", None, true),
                ("Needs action", Some(5), false),
                ("Paid", None, false)
            ]
        );
        assert!(view.ticks, "the waiting sale counts down");
        assert_eq!(
            view.entries[0],
            ShiftEntry::TrayHeader {
                label: "Waiting for payment".into(),
                said: "Waiting for payment, 1 sale".into()
            }
        );
        let ShiftEntry::Row {
            row, closes, tray, ..
        } = &view.entries[1]
        else {
            panic!("{:?}", view.entries[1]);
        };
        assert_eq!(row.title, "Sale #waiting · $8.40");
        assert_eq!(row.sub, "Waiting · 9:12 left");
        assert!(*closes && *tray);
        assert_eq!(
            view.entries[2],
            ShiftEntry::Earlier {
                label: "Earlier".into()
            }
        );
        let ShiftEntry::Row { closes, .. } = &view.entries[3] else {
            panic!();
        };
        assert!(!closes, "a closed sale has no X to swipe to");
    }

    #[test]
    fn an_empty_shift_says_what_happens_next_and_ios_names_no_bubble() {
        let android = shift(&input(vec![]), Device::Android, la(at()), &money);
        let idle = android.idle.unwrap();
        assert_eq!(idle.title, "No sale right now");
        assert_eq!(
            idle.hint,
            "The code appears here, and the bubble over Square lights up, when you tap Monero in Square."
        );
        assert_eq!(idle.start, None);
        assert!(android.chips.is_empty());
        assert_eq!(android.empty, None);
        let ipad = shift(&input(vec![]), Device::Ipad, la(at()), &money);
        assert_eq!(
            ipad.idle.unwrap().hint,
            "The code appears here when you tap Monero in Square."
        );
        let stopped = ShiftInput {
            running: false,
            ..input(vec![])
        };
        let view = shift(&stopped, Device::Iphone, la(at()), &money);
        assert_eq!(view.top.state, "Not watching");
        assert_eq!(view.summary, "Not watching");
        let idle = view.idle.unwrap();
        assert_eq!(idle.title, "Not watching for sales");
        assert_eq!(
            idle.hint,
            "Start watching and Till checks Square for Monero sales until you stop it."
        );
        assert_eq!(idle.start, Some(Press::new("Start watching")));
    }

    #[test]
    fn a_filter_with_nothing_in_it_says_so_and_a_lost_stream_says_reconnecting() {
        let paid_only = ShiftInput {
            filter: ShiftFilter::NeedsAction,
            live: false,
            ..input(vec![
                sale_of("p").phase(SalePhase::Paid).paid_at(at()).build(),
            ])
        };
        let view = shift(&paid_only, Device::Android, la(at()), &money);
        assert_eq!(view.empty.as_deref(), Some("Nothing needs action"));
        assert_eq!(view.top.state, "Reconnecting…");
        assert_eq!(view.top.tone, Tone::Muted);
        assert!(!view.ticks);
    }

    #[test]
    fn a_disconnected_till_shows_nothing_else_and_an_ended_sign_in_asks_again() {
        let gone = ShiftInput {
            disconnected: true,
            ..input(fixture_shift())
        };
        let view = shift(&gone, Device::Android, la(at()), &money);
        assert_eq!(view.top.state, "Disconnected");
        assert_eq!(view.top.tone, Tone::Danger);
        assert_eq!(
            view.disconnected.unwrap().connect_again,
            Press::new("Connect again")
        );
        assert!(view.entries.is_empty() && view.chips.is_empty());
        let ended = ShiftInput {
            sign_in_again: true,
            problem: Some("Square's sign-in has ended.".into()),
            ..input(vec![])
        };
        let view = shift(&ended, Device::Android, la(at()), &money);
        assert_eq!(
            view.sign_in_again,
            Some(Press::new("Sign in to Square again"))
        );
        assert_eq!(view.problem.as_deref(), Some("Square's sign-in has ended."));
    }

    #[test]
    fn a_clover_merchant_named_as_its_store_is_said_once() {
        let clover = ShiftInput {
            till: CLOVER,
            location: Some("The Bean Room".into()),
            ..input(vec![])
        };
        let view = shift(&clover, Device::Android, la(at()), &money);
        assert_eq!(view.top.place.as_deref(), Some("The Bean Room"));
    }
}
