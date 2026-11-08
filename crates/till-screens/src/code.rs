//! The code enlarged for the customer (boards PhoneCode and PadCode, the
//! design's CodeB; Android: `ui/CodeScreen.kt`'s `CodeText`): the amount,
//! the line under it and the code as large as the screen allows. The same
//! on every device. "Scan with any Monero wallet" and "Tap anywhere to
//! shrink", written once, stay in the platform's screen.

use monokulo_client::{MoneyFormat, PointOfSale};
use till_sales::TillSale;

use crate::time::DeviceClock;
use crate::view::{Ring, ring};
use crate::{pos_text, sale_text};

/// "14:52 left" while the code counts down (waiting, or part paid); `None`
/// once a payment is on its way.
pub fn left(sale: &TillSale, seconds_left: i64) -> Option<String> {
    sale.phase
        .counts_down()
        .then(|| format!("{} left", sale_text::clock(seconds_left)))
}

/// "0.05552 XMR · 14:52 left", or "0.05552 XMR · Seen on the network" once
/// seen, for a sale of `pos`.
pub fn under(sale: &TillSale, pos: Option<PointOfSale>, seconds_left: i64) -> String {
    format!(
        "{} · {}",
        sale_text::xmr(sale),
        left(sale, seconds_left).unwrap_or_else(|| sale_text::status(sale, pos))
    )
}

/// What the enlarged code shows.
#[derive(Clone, Debug)]
pub struct CodeInput {
    pub sale: TillSale,
    /// The till's point of sale now.
    pub till: Option<PointOfSale>,
}

/// The enlarged code, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CodeView {
    pub amount: String,
    /// "0.05552 XMR · 14:52 left".
    pub under: String,
    /// The payment URI the code says, with the receipt when it has one.
    pub code: String,
    /// Which of the four steps the ring beside "Scan with any Monero
    /// wallet" is at.
    pub step: u8,
    pub ring: Ring,
    /// The sale is no longer live: the code gives way to the sale's page.
    pub done: bool,
    /// The countdown is on screen: draw it again in a second.
    pub ticks: bool,
}

/// The enlarged code at the clock's now, amounts written by `money`.
pub fn code(input: &CodeInput, clock: DeviceClock, money: &dyn MoneyFormat) -> CodeView {
    let sale = &input.sale;
    let seconds = sale_text::seconds_left(sale.expires_at, clock.now);
    let live = sale.live();
    CodeView {
        amount: sale_text::amount(sale, money),
        under: under(sale, pos_text::of(sale, input.till), seconds),
        code: sale.code.uri.clone(),
        step: sale_text::step(sale.phase),
        ring: ring(sale),
        done: !live,
        ticks: live && sale.phase.counts_down(),
    }
}

/// Ported from Android's `OrderPageTest`
/// (`under the code, the time left until the payment is seen`), then the
/// screen.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{found, la, money, till_sale};
    use monokulo_client::SalePhase;

    fn sale(phase: SalePhase) -> TillSale {
        TillSale {
            reference: Some("k4Rt".into()),
            phase,
            found: Some(found(
                "p1",
                7921,
                "USD",
                Some("k4Rt"),
                "ord",
                "2026-10-22T23:07:00Z",
            )),
            ..till_sale("o1", "79.21", "USD", "0.31684")
        }
    }

    const SQUARE: Option<PointOfSale> = Some(PointOfSale::Square);

    #[test]
    fn under_the_code_the_time_left_until_the_payment_is_seen() {
        use SalePhase::*;
        assert_eq!(left(&sale(Waiting), 892).as_deref(), Some("14:52 left"));
        assert_eq!(left(&sale(Partial), 552).as_deref(), Some("9:12 left"));
        assert_eq!(left(&sale(Seen), 892), None);
        assert_eq!(
            under(&sale(Waiting), SQUARE, 892),
            "0.31684 XMR · 14:52 left"
        );
        assert_eq!(
            under(&sale(Seen), SQUARE, 892),
            "0.31684 XMR · Seen on the network"
        );
        assert_eq!(
            under(&sale(Confirming), SQUARE, 892),
            "0.31684 XMR · Confirming"
        );
    }

    #[test]
    fn the_enlarged_code_shows_the_amount_and_its_line() {
        let waiting = TillSale {
            expires_at: 1_000_892,
            ..sale(SalePhase::Waiting)
        };
        let view = code(
            &CodeInput {
                sale: waiting,
                till: SQUARE,
            },
            la(1_000_000),
            &money,
        );
        assert_eq!(
            view,
            CodeView {
                amount: "$79.21".into(),
                under: "0.31684 XMR · 14:52 left".into(),
                code: "monero:4abc".into(),
                step: 1,
                ring: Ring {
                    step: 1,
                    confirmations: None,
                    required: None,
                },
                done: false,
                ticks: true,
            }
        );
        let paid = code(
            &CodeInput {
                sale: sale(SalePhase::Paid),
                till: SQUARE,
            },
            la(1_000_000),
            &money,
        );
        assert!(paid.done, "paid: the sale's page says so");
        assert_eq!(paid.under, "0.31684 XMR · Paid");
    }
}
