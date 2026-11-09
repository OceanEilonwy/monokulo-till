//! A live sale (boards PhoneSale, PadSale and PadFront's words, built from
//! SaleBody; Android: `ui/Till.kt`'s sale screen, `SaleText.kt`): the bar,
//! the amount and the XMR, the code, the status line under it, the order
//! card and the buttons. It stays on screen through Paid, when its code is
//! spent; the sale's page takes over once it's opened again from the shift.
//!
//! Where the platforms differ: Android offers "Back to Square" beside "Copy
//! address" where its decisions say (a till on Square, a sale Square took);
//! iOS can't open Square (docs/ios-plan.md §4), so it has "Copy address"
//! alone.

use monokulo_client::{MoneyFormat, PointOfSale, SalePhase};
use till_sales::TillSale;

use crate::order_text::{OrderCard, order_card};
use crate::sale_page::header;
use crate::time::DeviceClock;
use crate::view::{Device, PaymentProgress, PosMark, Press, Ring, mark, ring};
use crate::{close, pos_text, sale_text};

/// What the sale screen shows.
#[derive(Clone, Debug)]
pub struct SaleInput {
    pub sale: TillSale,
    /// The till's point of sale now.
    pub till: Option<PointOfSale>,
    /// The till's Square location or Clover merchant.
    pub location: Option<String>,
}

/// The sale, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SaleView {
    /// The bar: whom it's for, or "Sale #bP9m".
    pub title: String,
    /// "Front Counter · this till", led by the sale's number when the title
    /// is the customer's name.
    pub title_line: String,
    /// "$13.88".
    pub amount: String,
    /// "0.05552 XMR", which copies the amount when tapped.
    pub xmr: String,
    /// What the code says: the payment URI, with the receipt when it could
    /// be added.
    pub code: String,
    /// What a screen reader says for the code.
    pub code_said: String,
    /// Paid: the code is spent, faded so no one scans it again.
    pub spent: bool,
    /// "Waiting for payment", "Seen on the network", "Confirming · 4 of
    /// 10", "Paid".
    pub status: String,
    /// "Scan with any Monero wallet · 14:52 left".
    pub hint: String,
    /// Which of the four steps the ring is at; 4 with the tick.
    pub step: u8,
    pub ring: Ring,
    /// What was bought, three lines at most, from this till's receipt.
    pub order: Option<OrderCard>,
    /// "Copy address", and what it copies.
    pub copy_address: Press,
    pub address: String,
    /// Android: "Back to Square", and which point of sale it opens.
    pub back_to_pos: Option<BackToPos>,
    /// The bar's X, which asks whether the customer is paying.
    pub closes: bool,
    /// The countdown is on screen: draw it again in a second.
    pub ticks: bool,
    /// How far the payment has come, in the Till widget's thirds, for the
    /// platform's progress bar (core.md decision 119).
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub progress: Option<PaymentProgress>,
    /// A cash sale's "Paid in cash" (core.md decision 116).
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    pub cash: Vec<crate::sale_page::CashButton>,
}

/// Android's way back to the point of sale beside "Copy address": its
/// words, and whose app it opens, so the button opens the point of sale it
/// names.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct BackToPos {
    /// "Back to Square".
    pub press: Press,
    /// The point of sale it names, and opens.
    pub mark: PosMark,
}

impl BackToPos {
    pub fn new(pos: PointOfSale) -> BackToPos {
        BackToPos {
            press: Press::new(pos_text::back_to(Some(pos))),
            mark: mark(pos),
        }
    }
}

/// What a screen reader says for `sale`'s code.
pub fn code_said(sale: &TillSale, money: &dyn MoneyFormat) -> String {
    let mut said = format!("Payment code for {}", sale_text::amount(sale, money));
    if sale.code.has_receipt {
        said.push_str(", with the receipt in it");
    }
    if sale.phase == SalePhase::Paid {
        said.push_str(", paid: no need to scan it");
    }
    said
}

/// The sale at the clock's now, on `device`, amounts written by `money`.
pub fn sale(
    input: &SaleInput,
    device: Device,
    clock: DeviceClock,
    money: &dyn MoneyFormat,
) -> SaleView {
    let sale = &input.sale;
    let header = header(sale, input.till, input.location.as_deref());
    let live = sale.live();
    let seconds = sale_text::seconds_left(sale.expires_at, clock.now);
    SaleView {
        title: sale_text::heading(sale),
        title_line: sale_text::heading_line(sale, &header.place),
        amount: sale_text::amount(sale, money),
        xmr: sale_text::xmr(sale),
        code: sale.code.uri.clone(),
        code_said: code_said(sale, money),
        spent: sale.phase == SalePhase::Paid,
        status: sale_text::status(sale, header.pos),
        hint: sale_text::hint(sale, seconds),
        step: sale_text::step(sale.phase),
        ring: ring(sale),
        order: sale
            .receipt
            .as_ref()
            .map(|receipt| order_card(receipt, header.pos, true)),
        copy_address: Press::new("Copy address"),
        address: sale.address.clone(),
        back_to_pos: header
            .pos
            .filter(|_| header.back_to_pos && !device.is_ios())
            .map(BackToPos::new),
        closes: !close::choices(sale, header.elsewhere).is_empty(),
        ticks: live && sale.phase.counts_down(),
        progress: crate::view::progress(sale),
        cash: crate::sale_page::cash_buttons(sale, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{la, money, receipt};
    use crate::shift::tests::{at, sale_of};

    const SQUARE: Option<PointOfSale> = Some(PointOfSale::Square);

    fn bp9m() -> TillSale {
        TillSale {
            amount: "13.88".into(),
            xmr: "0.05552".into(),
            expires_at: at() + 892,
            receipt: Some(receipt(
                "bP9m",
                "2026-10-22T16:07:00-07:00",
                &[
                    ("Flat white", 2.0, 4.5, 9.0),
                    ("Almond croissant", 1.0, 3.75, 3.75),
                ],
                &[(0.08875, 1.13)],
                13.88,
                "USD",
            )),
            ..sale_of("bP9m").build()
        }
    }

    fn input(sale: TillSale, till: Option<PointOfSale>) -> SaleInput {
        SaleInput {
            sale,
            till,
            location: Some("Front Counter".into()),
        }
    }

    #[test]
    fn a_waiting_sale_shows_its_amount_code_status_and_order_as_the_design_does() {
        let view = sale(&input(bp9m(), SQUARE), Device::Android, la(at()), &money);
        assert_eq!(view.title, "Sale #bP9m");
        assert_eq!(view.title_line, "Front Counter · this till");
        assert_eq!(view.amount, "$13.88");
        assert_eq!(view.xmr, "0.05552 XMR");
        assert_eq!(view.code, "monero:4abc?tx_amount=0.1");
        assert_eq!(
            view.code_said,
            "Payment code for $13.88, with the receipt in it"
        );
        assert_eq!(view.status, "Waiting for payment");
        assert_eq!(view.hint, "Scan with any Monero wallet · 14:52 left");
        assert_eq!(view.step, 1);
        assert!(!view.spent && view.closes && view.ticks);
        let order = view.order.unwrap();
        assert_eq!(order.summary, "Square receipt bP9m · 3 items · 13.88 USD");
        assert_eq!(
            order
                .lines
                .iter()
                .map(|l| (l.label.as_str(), l.price.as_str()))
                .collect::<Vec<_>>(),
            [("2 × Flat white", "9.00"), ("Almond croissant", "3.75")]
        );
        assert_eq!(order.foot, "Tax 1.13");
        assert_eq!(order.total, "13.88 USD");
        assert_eq!(view.copy_address, Press::new("Copy address"));
        assert_eq!(view.address, "4abc");
        assert_eq!(
            view.back_to_pos,
            Some(BackToPos {
                press: Press::new("Back to Square"),
                mark: PosMark::Square,
            })
        );
    }

    #[test]
    fn back_to_the_point_of_sale_opens_the_one_it_names() {
        for pos in PointOfSale::ALL {
            let back = BackToPos::new(pos);
            assert_eq!(back.press.label, format!("Back to {}", pos.display_name()));
            assert_eq!(back.mark, mark(pos), "the button opens {pos:?}, as it says");
        }
        let view = sale(&input(bp9m(), SQUARE), Device::Android, la(at()), &money);
        let back = view.back_to_pos.expect("a Square till offers it");
        assert_eq!(back.mark, PosMark::Square, "Square's app, never another");
    }

    #[test]
    fn ios_has_copy_address_and_no_back_to_square() {
        for device in [Device::Iphone, Device::Ipad] {
            let view = sale(&input(bp9m(), SQUARE), device, la(at()), &money);
            assert_eq!(view.copy_address, Press::new("Copy address"));
            assert_eq!(view.back_to_pos, None, "{device:?} can't open Square");
        }
    }

    #[test]
    fn android_offers_back_to_square_only_on_square_and_for_squares_own_sales() {
        let clover_till = sale(
            &input(bp9m(), Some(PointOfSale::Clover)),
            Device::Android,
            la(at()),
            &money,
        );
        assert_eq!(clover_till.back_to_pos, None, "Clover's device is its own");
        assert_eq!(
            clover_till.title_line, "Square · this till",
            "a Square sale on a till now on Clover"
        );
        assert!(
            clover_till.closes,
            "the sale rung up elsewhere still has its X, with two answers"
        );
    }

    #[test]
    fn a_sale_paid_on_screen_spends_its_code_and_says_what_came_in() {
        let paid = TillSale {
            phase: SalePhase::Paid,
            paid_at: Some(at()),
            ..bp9m()
        };
        let view = sale(&input(paid, SQUARE), Device::Iphone, la(at()), &money);
        assert!(view.spent);
        assert_eq!(view.status, "Paid");
        assert_eq!(view.hint, "0.05552 XMR received");
        assert_eq!(view.step, 4);
        assert_eq!(
            view.code_said,
            "Payment code for $13.88, with the receipt in it, paid: no need to scan it"
        );
        assert!(!view.closes, "no X once paid");
        assert!(!view.ticks);
    }

    #[test]
    fn a_confirming_sale_counts_its_confirmations_and_lets_the_customer_go() {
        let confirming = TillSale {
            phase: SalePhase::Confirming,
            confirmations: Some(4),
            confirmations_required: Some(10),
            ..bp9m()
        };
        let view = sale(&input(confirming, SQUARE), Device::Ipad, la(at()), &money);
        assert_eq!(view.status, "Confirming · 4 of 10");
        assert_eq!(view.hint, "The customer can go: it's being confirmed");
        assert_eq!(view.step, 3);
        assert_eq!(
            view.ring,
            Ring::new(3, Some(4), Some(10)),
            "the ring grows with each confirmation"
        );
        assert!(
            view.ring.spins && !view.ring.green,
            "green is for paid alone"
        );
        assert!((view.ring.fill - (0.12 + 0.88 * 0.4)).abs() < 1e-6);
        assert!(!view.ticks);
    }
}
