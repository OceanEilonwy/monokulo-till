//! The till's screens (Android: `AppViewModel`'s `follow`, `backTarget` and
//! what `MainActivity` and the overlay read from the runner): where the
//! sales move the till, the paid sale's step back, and the shift, sale,
//! close, refund, bubble and notification records the engine publishes.

use std::time::Duration;

use monokulo_client::SalePhase;
use till_sales::TillSale;
use till_screens::bubble;
use till_screens::close::{self, CloseInput};
use till_screens::code::{self, CodeInput};
use till_screens::notification::{self, NotificationInput};
use till_screens::order_text::order_card;
use till_screens::pos_text;
use till_screens::sale::{self, SaleInput};
use till_screens::sale_page::{self, SalePageInput};
use till_screens::sale_text;
use till_screens::shift::{self, ShiftInput};
use till_screens::till_nav::{self, TillPlace};
use till_screens::view::Press;
use till_screens::{CloseView, Device, NotificationView, ShiftView};

use super::{Parts, State, apply};
use crate::faces::Money;
use crate::{BubbleView, RefundSheetView, Screen};

/// How long a sale paid on screen stays before it steps back to the shift,
/// and how long the bubble and the ongoing notification show it as paid
/// (Android's `Motion.Dwell.PAID_MS`).
pub(crate) const PAID_DWELL: Duration = Duration::from_millis(5_000);

/// How often a countdown on screen is drawn again (decision 37).
pub(crate) const TICK: Duration = Duration::from_secs(1);

/// The till's place for `screen`, as the sales move it.
pub(crate) fn place(screen: &Screen) -> TillPlace {
    match screen {
        Screen::Till => TillPlace::Shift,
        Screen::Sale { order_id } => TillPlace::Sale {
            order_id: order_id.clone(),
        },
        Screen::Code { order_id } => TillPlace::Code {
            order_id: order_id.clone(),
        },
        Screen::Order { order_id } => TillPlace::Order {
            order_id: order_id.clone(),
        },
        _ => TillPlace::Other,
    }
}

/// The screen for `place`; `None` for the places the sales never reach.
pub(crate) fn screen(place: TillPlace) -> Option<Screen> {
    Some(match place {
        TillPlace::Shift => Screen::Till,
        TillPlace::Sale { order_id } => Screen::Sale { order_id },
        TillPlace::Code { order_id } => Screen::Code { order_id },
        TillPlace::Order { order_id } => Screen::Order { order_id },
        TillPlace::Other => return None,
    })
}

/// The sale a screen shows, if it shows one.
pub(crate) fn order_id(screen: &Screen) -> Option<&str> {
    match screen {
        Screen::Sale { order_id } | Screen::Code { order_id } | Screen::Order { order_id } => {
            Some(order_id)
        }
        _ => None,
    }
}

impl State {
    fn sales(&self) -> Vec<TillSale> {
        self.parts
            .as_ref()
            .map(|p| p.runner.state().till.sales)
            .unwrap_or_default()
    }

    /// Where `place` goes as the sales stand now, on a stage that isn't
    /// resting (nothing new rises).
    fn followed(&self, from: &Screen) -> Screen {
        let next = till_nav::follow(
            &place(from),
            &self.sales(),
            &self.left,
            &self.live_before,
            false,
        );
        screen(next).unwrap_or_else(|| from.clone())
    }

    /// The sales changed (or may have): a new one comes up out of a shift
    /// at rest, a code or order whose sale isn't live gives way to its page,
    /// and a sale paid on screen steps back after [`PAID_DWELL`].
    pub(super) fn follow_sales(&mut self) {
        let sales = self.sales();
        let here = place(&self.screen);
        if here != TillPlace::Other {
            let resting = self.resting_on.as_ref() == Some(&Screen::Till);
            let next = till_nav::follow(&here, &sales, &self.left, &self.live_before, resting);
            if next != here
                && let Some(next) = screen(next.clone())
            {
                // Put there by the sales, not the cashier (M7).
                self.by_itself = true;
                self.resting_on = None;
                self.screen = next;
            }
            if let Some(paid) = till_nav::paid_on_screen(&next, &sales, &self.live_before) {
                self.step_back_after_paid(paid);
            }
        }
        self.live_before = till_nav::live_ids(&sales);
    }

    fn step_back_after_paid(&mut self, order_id: String) {
        self.step_back += 1;
        let token = self.step_back;
        self.task(async move {
            tokio::time::sleep(PAID_DWELL).await;
            apply(move |s| {
                let still = matches!(&s.screen, Screen::Sale { order_id: o } if *o == order_id);
                if s.step_back == token && still {
                    s.leave(order_id);
                }
            })
        });
    }

    /// A payment landed on `order_id`: the bubble and the ongoing
    /// notification show it as paid for [`PAID_DWELL`].
    pub(super) fn just_paid(&mut self, order_id: String) {
        self.just_paid = Some(order_id);
        self.just_paid_token += 1;
        let token = self.just_paid_token;
        self.task(async move {
            tokio::time::sleep(PAID_DWELL).await;
            apply(move |s| {
                if s.just_paid_token == token {
                    s.just_paid = None;
                }
            })
        });
    }

    /// Where Back would go from the screen up, without going: what a back
    /// gesture previews. `None` where Back shows no preview.
    pub(super) fn back_target(&self) -> Option<Screen> {
        match &self.screen {
            Screen::Sale { .. } => Some(Screen::Till),
            Screen::Code { order_id } | Screen::Order { order_id } => Some(Screen::Sale {
                order_id: order_id.clone(),
            }),
            Screen::Settings => Some(
                self.before_settings
                    .as_ref()
                    .filter(|_| self.setup_done())
                    .map(|before| self.followed(before))
                    .unwrap_or_else(|| self.home()),
            ),
            _ => None,
        }
    }

    /// Back from Settings, to the sale or the shift it was opened from: what
    /// arrived meanwhile waits in the tray, and only a code or order whose
    /// sale is down gives way.
    pub(super) fn settings_back(&mut self) {
        match self.before_settings.take() {
            Some(before) if self.setup_done() => {
                let to = self.followed(&before);
                self.go(to);
            }
            _ => self.go_home(),
        }
    }

    /// Asks again in a second while a countdown is on screen (decision 37).
    pub(super) fn tick_if(&mut self, ticks: bool) {
        if !ticks || !self.visible || self.ticking {
            return;
        }
        self.ticking = true;
        self.task(async move {
            tokio::time::sleep(TICK).await;
            apply(|s| s.ticking = false)
        });
    }

    pub(super) fn shift_view(&self, p: &Parts) -> ShiftView {
        let model = p.side.model();
        let runner = p.runner.state();
        let input = ShiftInput {
            sales: runner.till.sales,
            till: model.point_of_sale,
            store: p.live.now().store.map(|s| s.name),
            location: model.place_name(),
            running: runner.running,
            live: runner.live,
            disconnected: runner.disconnected,
            problem: runner.till.problem.or(runner.stream_problem),
            sign_in_again: runner.till.sign_in_again,
            started_at: model.shift_started_at,
            filter: self.filter,
            folded: self.folded,
        };
        shift::shift(
            &input,
            self.config.device,
            self.device_clock(),
            &Money(self.platform.clone()),
        )
    }

    /// The sale on screen, as each of its screens draws it.
    pub(super) fn sale_update(&self, p: &Parts) -> Option<crate::Update> {
        let order_id = order_id(&self.screen)?;
        let runner = p.runner.state();
        let sale = runner.till.sale(order_id)?.clone();
        let model = p.side.model();
        let (till, location) = (model.point_of_sale, model.place_name());
        let (clock, money) = (self.device_clock(), Money(self.platform.clone()));
        let view = sale::sale(
            &SaleInput {
                sale: sale.clone(),
                till,
                location: location.clone(),
            },
            self.config.device,
            clock,
            &money,
        );
        let page = sale_page::sale_page(
            &SalePageInput {
                sale: sale.clone(),
                till,
                location,
                refund_problem: runner.till.refund_problems.get(order_id).cloned(),
                refunding: self.refunding.contains(order_id),
            },
            clock,
            &money,
        );
        let code = code::code(
            &CodeInput {
                sale: sale.clone(),
                till,
            },
            clock,
            &money,
        );
        let order = sale
            .receipt
            .as_ref()
            .map(|receipt| order_card(receipt, pos_text::of(&sale, till), false));
        Some(crate::Update::Sale {
            order_id: order_id.to_owned(),
            view,
            page,
            code,
            order,
        })
    }

    /// The close sheet for the sale asked about, while it has answers.
    pub(super) fn close_view(&self, p: &Parts) -> Option<CloseView> {
        let order_id = self.close_sheet.as_deref()?;
        let runner = p.runner.state();
        let sale = runner.till.sale(order_id)?.clone();
        let view = close::close(
            &CloseInput {
                sale,
                till: p.side.model().point_of_sale,
                source: Some(runner.source),
            },
            self.device_clock(),
            &Money(self.platform.clone()),
        );
        (!view.answers.is_empty()).then_some(view)
    }

    pub(super) fn refund_sheet_view(&self, p: &Parts) -> Option<RefundSheetView> {
        let sheet = self.refund_sheet.as_ref()?;
        let sale = p.runner.state().till.sale(&sheet.order_id)?.clone();
        let money = Money(self.platform.clone());
        let order = sale
            .found
            .as_ref()
            .and_then(|found| found.title.clone().or(found.order_id.clone()));
        let time =
            (sale.created_at > 0).then(|| sale_text::time_of(sale.created_at, self.device_clock()));
        Some(RefundSheetView {
            order_id: sheet.order_id.clone(),
            title: pos_text::refund_sheet_title(sheet.pos),
            body: pos_text::refund_sheet_body(
                sheet.pos,
                sheet.asks,
                order.as_deref(),
                time.as_deref(),
                &sale_text::amount(&sale, &money),
            ),
            not_seen: sheet.not_seen.then(|| pos_text::refund_not_seen(sheet.pos)),
            refunded: if sheet.checking {
                Press::new(pos_text::LOOKING).enabled(false)
            } else {
                Press::new(pos_text::REFUNDED_IT)
            },
            not_yet: pos_text::NOT_YET.into(),
        })
    }

    /// The bubble: about the sale just paid while it shows so, else the
    /// newest live one, else the one it showed last.
    pub(super) fn bubble_view(&mut self) -> Option<BubbleView> {
        let p = self.parts.as_ref()?;
        let model = p.side.model();
        let runner = p.runner.state();
        let sales = &runner.till.sales;
        let just_paid = self.just_paid.is_some();
        let focus = bubble::focus(sales, self.bubble_focus.as_deref(), just_paid);
        let sale = focus
            .as_deref()
            .and_then(|id| sales.iter().find(|s| s.order_id == id));
        let money = Money(self.platform.clone());
        let look = bubble::look(sales, just_paid);
        let till = model.point_of_sale;
        let location = model.place_name();
        let tap = bubble::tap(sale, false);
        let tap_open = bubble::tap(sale, true);
        let view = BubbleView {
            shown: model.bubble_on
                && self.bubble_allowed
                && model.shift_on
                && runner.running
                && !self.visible,
            look,
            order_id: focus.clone(),
            label: bubble::label(look, sale, &money),
            tap,
            tap_label: bubble::tap_label(tap).into(),
            tap_open,
            tap_open_label: bubble::tap_label(tap_open).into(),
            panel: sale.map(|sale| {
                sale::sale(
                    &SaleInput {
                        sale: sale.clone(),
                        till,
                        location: location.clone(),
                    },
                    self.config.device,
                    self.device_clock(),
                    &money,
                )
            }),
            panel_where: sale.map(|sale| bubble::panel_where(sale, location.as_deref())),
            ended_heading: sale
                .filter(|sale| !sale.live() && sale.phase != SalePhase::Paid)
                .map(|sale| bubble::panel_ended_heading(sale, till)),
            ended_line: sale
                .filter(|sale| !sale.live() && sale.phase != SalePhase::Paid)
                .map(|sale| bubble::panel_ended_line(sale, till)),
            buzzes: bubble::bubble_buzzes(self.visible),
        };
        self.bubble_focus = focus;
        Some(view)
    }

    pub(super) fn notification_view(&self, p: &Parts) -> NotificationView {
        let model = p.side.model();
        notification::notification(
            &NotificationInput {
                sales: p.runner.state().till.sales,
                just_paid: self.just_paid.clone(),
                alert_for: None,
                store: p.live.now().store.map(|s| s.name),
                location: model.place_name(),
            },
            self.config.device,
            &Money(self.platform.clone()),
        )
    }

    /// The bubble is Android's alone.
    pub(super) fn has_bubble(&self) -> bool {
        self.config.device == Device::Android
    }
}
