//! The till's screens drawn with no engine, from a shift written as Android
//! saved it (`till_sales::shift_json`): for the platforms' previews, and for
//! their on-device tests of how a screen is drawn and moves, which need real
//! records without a store, a point of sale or a clock that runs.

use std::sync::Arc;

use till_sales::{TillSale, shift_json};
use till_screens::bubble::{self, PanelInput, PanelView};
use till_screens::close::{self, CloseInput};
use till_screens::code::{self, CodeInput};
use till_screens::order_text::order_page;
use till_screens::pos_text;
use till_screens::sale::{self, SaleInput};
use till_screens::sale_page::{self, SalePageInput};
use till_screens::shift::{self, ShiftFilter, ShiftInput};
use till_screens::{CloseView, Device, DeviceClock, ShiftView};

use crate::phases::Money;
use crate::{Platform, Pos, Update};

/// The till a preview draws (not `Preview`, which SwiftUI declares): its sales, and what the engine would know
/// about it at the clock's `now`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TillPreview {
    /// The shift's sales, newest first, as Android's `Stores.kt` saved them.
    pub sales_json: String,
    /// Unix seconds.
    pub now: i64,
    #[cfg_attr(feature = "uniffi", uniffi(default = 0))]
    pub utc_offset: i32,
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub hours_24: bool,
    /// Android when not said.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub device: Option<Device>,
    /// The till's point of sale.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub till: Option<Pos>,
    /// The point of sale the till's source watches; the till's when not said.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub source: Option<Pos>,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub store: Option<String>,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub location: Option<String>,
    #[cfg_attr(feature = "uniffi", uniffi(default = true))]
    pub running: bool,
    #[cfg_attr(feature = "uniffi", uniffi(default = true))]
    pub live: bool,
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub disconnected: bool,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub problem: Option<String>,
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub sign_in_again: bool,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub started_at: Option<i64>,
    /// The shift's chip; All when not said.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub filter: Option<ShiftFilter>,
    #[cfg_attr(feature = "uniffi", uniffi(default = true))]
    pub folded: bool,
    /// The sale previewed has a refund under way, or one its point of sale
    /// refused with these words.
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub refunding: bool,
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub refund_problem: Option<String>,
}

impl TillPreview {
    fn sales(&self) -> Option<Vec<TillSale>> {
        shift_json::decode(&self.sales_json).ok()
    }

    fn sale(&self, order_id: &str) -> Option<TillSale> {
        self.sales()?.into_iter().find(|s| s.order_id == order_id)
    }

    fn clock(&self) -> DeviceClock {
        DeviceClock {
            now: self.now,
            utc_offset: self.utc_offset,
            hours_24: self.hours_24,
        }
    }

    fn device(&self) -> Device {
        self.device.unwrap_or(Device::Android)
    }
}

/// The shift, as the engine would publish it; `None` when the sales can't
/// be read.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn preview_shift(preview: TillPreview, platform: Arc<dyn Platform>) -> Option<ShiftView> {
    let input = ShiftInput {
        sales: preview.sales()?,
        till: preview.till.map(Into::into),
        store: preview.store.clone(),
        location: preview.location.clone(),
        running: preview.running,
        live: preview.live,
        disconnected: preview.disconnected,
        problem: preview.problem.clone(),
        sign_in_again: preview.sign_in_again,
        started_at: preview.started_at,
        filter: preview.filter.unwrap_or(ShiftFilter::All),
        folded: preview.folded,
        counters: Default::default(),
        counter: None,
        alerts: None,
        bell: false,
        show_sales: Default::default(),
    };
    Some(shift::shift(
        &input,
        preview.device(),
        preview.clock(),
        &Money(platform),
    ))
}

/// The screens of the sale `order_id` ([`Update::Sale`]), as the engine
/// would publish them with it on screen; `None` when there's no such sale.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn preview_sale(
    preview: TillPreview,
    order_id: String,
    platform: Arc<dyn Platform>,
) -> Option<Update> {
    let sale = preview.sale(&order_id)?;
    let till = preview.till.map(Into::into);
    let location = preview.location.clone();
    let (clock, money) = (preview.clock(), Money(platform));
    let view = sale::sale(
        &SaleInput {
            sale: sale.clone(),
            till,
            location: location.clone(),
        },
        preview.device(),
        clock,
        &money,
    );
    let page = sale_page::sale_page(
        &SalePageInput {
            sale: sale.clone(),
            till,
            location,
            refund_problem: preview.refund_problem.clone(),
            refunding: preview.refunding,
            taking: false,
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
    let order = order_page(&sale, pos_text::of(&sale, till), clock);
    Some(Update::Sale {
        order_id,
        view,
        page,
        code,
        order,
    })
}

/// The close sheet over the sale `order_id`, while it has answers.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn preview_close(
    preview: TillPreview,
    order_id: String,
    platform: Arc<dyn Platform>,
) -> Option<CloseView> {
    let sale = preview.sale(&order_id)?;
    let view = close::close(
        &CloseInput {
            sale,
            till: preview.till.map(Into::into),
            source: preview.source.or(preview.till).map(Into::into),
        },
        preview.clock(),
        &Money(platform),
    );
    (!view.answers.is_empty()).then_some(view)
}

/// The bubble's panel about the sale `order_id` (Android only), as the
/// engine publishes it in `BubbleView`.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn preview_panel(
    preview: TillPreview,
    order_id: String,
    platform: Arc<dyn Platform>,
) -> Option<PanelView> {
    let sale = preview.sale(&order_id)?;
    Some(bubble::panel(
        &PanelInput {
            sale,
            till: preview.till.map(Into::into),
            location: preview.location.clone(),
        },
        preview.device.unwrap_or(Device::Android),
        preview.clock(),
        &Money(platform),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakePlatform;

    fn preview(sales_json: &str) -> TillPreview {
        TillPreview {
            sales_json: sales_json.into(),
            now: 1_000,
            utc_offset: 0,
            hours_24: false,
            device: None,
            till: Some(Pos::Square),
            source: None,
            store: Some("The Bean Room".into()),
            location: Some("Front".into()),
            running: true,
            live: true,
            disconnected: false,
            problem: None,
            sign_in_again: false,
            started_at: None,
            filter: None,
            folded: true,
            refunding: false,
            refund_problem: None,
        }
    }

    const SALE: &str = r#"[{"orderId":"a1","amount":"8.40","currency":"USD","reference":"a1","code":{"uri":"monero:4abc?tx_amount=0.0336","hasReceipt":false},"phase":"Waiting","expiresAt":1600,"xmr":"0.0336","address":"4abc","found":{"id":"p_a1","amountMinor":840,"currency":"USD","reference":"a1","orderId":"o","createdAt":"2026-10-22T22:50:00Z"},"createdAt":900}]"#;

    #[test]
    fn a_saved_shift_draws_its_shift_sale_and_close_sheet() {
        let platform = Arc::new(FakePlatform::default());
        let shift = preview_shift(preview(SALE), platform.clone()).expect("the shift reads");
        assert!(shift.entries.iter().any(|entry| matches!(
            entry,
            till_screens::shift::ShiftEntry::Row { order_id, .. } if order_id == "a1"
        )));
        let Some(Update::Sale { view, code, .. }) =
            preview_sale(preview(SALE), "a1".into(), platform.clone())
        else {
            panic!("the sale draws");
        };
        assert_eq!(view.status, "Waiting for payment");
        assert!(view.hint.ends_with("10:00 left"), "{}", view.hint);
        assert!(!code.done);
        let close = preview_close(preview(SALE), "a1".into(), platform.clone()).unwrap();
        assert_eq!(close.answers.len(), 3);
        let panel = preview_panel(preview(SALE), "a1".into(), platform.clone()).unwrap();
        assert_eq!(panel.where_from, "Front");
        assert_eq!(panel.time_left.as_deref(), Some("10:00 left"));
        assert!(preview_sale(preview(SALE), "b2".into(), platform.clone()).is_none());
        assert!(preview_shift(preview("not json"), platform).is_none());
    }
}
