//! What the till's notifications say (Android's shade and chips, board
//! PadTucked; Android: `notify/SaleNotifications.kt`'s `Face`, `Ongoing`
//! and the ended notices' words).
//!
//! Where the platforms differ: Android has one ongoing notification for the
//! shift, which becomes the current sale (a Live Update, its chip the ring
//! alone), and alerts by making it a heads-up. iOS has no ongoing
//! notification (the iPhone's Live Activity is [`crate::live_activity`]);
//! a new sale is a time-sensitive alert, whose words differ on iPad (swipe
//! Till in from Slide Over) and iPhone (tap to open Till).
//!
//! Whom a sale is for, and on the lock screen what was bought, are never on
//! a notification.

use monokulo_client::{MoneyFormat, PointOfSale, SalePhase};
use till_sales::TillSale;

use crate::view::{Device, capitalised};
use crate::{pos_text, sale_text};

/// The notification's small icon, the status bar chip's: the ring filling
/// a quarter a step, or the tick once paid; the mark when there's no sale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum FaceIcon {
    Mark,
    RingQuarter,
    RingHalf,
    RingThreeQuarters,
    Tick,
}

/// The notification's colour: the accent, or the paid green.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum FaceTint {
    Accent,
    Paid,
}

/// What the ongoing notification says about a sale (the design's Shade and
/// Chips boards), worked out from the sale alone.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Face {
    pub icon: FaceIcon,
    pub tint: FaceTint,
    pub title: String,
    /// "$13.88 · 3 items · waiting for payment".
    pub text: String,
    /// What was bought ("2 × Flat white, Almond croissant"): the private
    /// version only, `None` without a receipt.
    pub detail: Option<String>,
    /// The lock screen's text: the amount and the phase, never an item.
    pub public_text: String,
    /// How many of the four steps are filled.
    pub step: u8,
    /// The customer still has to scan: "Full screen" beside "Show code",
    /// else "Copy address".
    pub full_screen: bool,
}

/// A sale whose code is up, in `location` (the till's Square location or
/// Clover merchant).
pub fn face(sale: &TillSale, location: Option<&str>, money: &dyn MoneyFormat) -> Face {
    let (icon, title, phrase) = match sale.phase {
        SalePhase::Seen => (
            FaceIcon::RingHalf,
            "Seen on the network".to_owned(),
            "waiting for confirmation",
        ),
        SalePhase::Confirming => (
            FaceIcon::RingThreeQuarters,
            "Confirming".to_owned(),
            "nearly there",
        ),
        SalePhase::Partial => (
            FaceIcon::RingQuarter,
            format!("Part paid · {}", sale_text::place(sale, location)),
            "ask the customer to pay the rest",
        ),
        _ => (
            FaceIcon::RingQuarter,
            format!("New Monero sale · {}", where_from(sale, location)),
            "waiting for payment",
        ),
    };
    let amount = sale_text::amount(sale, money);
    let receipt = sale.receipt.as_ref();
    let mut text = vec![amount.clone()];
    text.extend(receipt.map(sale_text::items));
    text.push(phrase.into());
    Face {
        icon,
        tint: FaceTint::Accent,
        title,
        text: text.join(" · "),
        // Every line: Android ellipsises what doesn't fit.
        detail: receipt
            .filter(|r| !r.line_items.is_empty())
            .map(|r| sale_text::first_items(r, r.line_items.len())),
        public_text: format!("{amount} · {phrase}"),
        step: sale_text::step(sale.phase),
        full_screen: matches!(sale.phase, SalePhase::Waiting | SalePhase::Partial),
    }
}

/// A new sale's place on a notification: the till's location, else where
/// the sale came from.
fn where_from(sale: &TillSale, location: Option<&str>) -> String {
    location.map_or_else(|| sale_text::place(sale, location), Into::into)
}

/// The few seconds after `sale` is paid: a green tick in the chip.
pub fn paid_face(sale: &TillSale, money: &dyn MoneyFormat) -> Face {
    let text = format!(
        "{} · {} received",
        sale_text::amount(sale, money),
        sale_text::xmr(sale)
    );
    Face {
        icon: FaceIcon::Tick,
        tint: FaceTint::Paid,
        title: format!("Paid · {}", sale_text::title(sale)),
        text: text.clone(),
        detail: None,
        public_text: text,
        step: 4,
        full_screen: false,
    }
}

/// What the shift's one ongoing notification is about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ongoing<'a> {
    /// No sale's code is up: what's being watched.
    Watching,
    /// The newest sale whose code is up.
    Sale(&'a TillSale),
    /// A sale just paid, for a few seconds.
    Paid(&'a TillSale),
}

/// The sale `just_paid` while it shows as paid, else the newest live sale,
/// else watching. A sale closed, expired or reversed is no longer live, so
/// the chip goes with it at once.
pub fn ongoing<'a>(sales: &'a [TillSale], just_paid: Option<&str>) -> Ongoing<'a> {
    if let Some(paid) = sales
        .iter()
        .find(|sale| Some(sale.order_id.as_str()) == just_paid && sale.phase == SalePhase::Paid)
    {
        return Ongoing::Paid(paid);
    }
    sales
        .iter()
        .find(|sale| sale.live())
        .map_or(Ongoing::Watching, Ongoing::Sale)
}

/// The ongoing notification while no sale's code is up.
pub const WATCHING_TITLE: &str = "Watching for Monero sales";

/// A short notification of its own: a sale that ended, or the group's
/// summary.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Notice {
    pub title: String,
    pub text: Option<String>,
}

/// Replaces the sale's progress once it's paid.
pub fn paid_notice(sale: &TillSale, money: &dyn MoneyFormat) -> Notice {
    Notice {
        title: format!("Paid · {}", sale_text::title(sale)),
        text: Some(format!(
            "{} · {} received",
            sale_text::amount(sale, money),
            sale_text::xmr(sale)
        )),
    }
}

/// A sale that ended without its money, at a till on `till`: open it to
/// refund the point of sale's sale.
pub fn unpaid_notice(
    sale: &TillSale,
    till: Option<PointOfSale>,
    money: &dyn MoneyFormat,
) -> Notice {
    let pos = till;
    let what = if sale.found.is_some() {
        format!(
            "{} if the customer isn't paying.",
            capitalised(&pos_text::refund_it_in(pos))
        )
    } else {
        pos_text::refund_on_other_till(pos, "the till that took it")
    };
    Notice {
        title: format!(
            "{} · {}",
            sale_text::status(sale, pos),
            sale_text::title(sale)
        ),
        text: Some(format!("{}. {what}", sale_text::amount(sale, money))),
    }
}

/// The ended group's summary, once there are two.
pub fn ended_summary(ended: u32) -> Notice {
    Notice {
        title: format!("{ended} sales ended"),
        text: None,
    }
}

/// A button on a sale's notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum NoticeAction {
    ShowCode,
    FullScreen,
    CopyAddress,
}

impl NoticeAction {
    pub fn label(self) -> &'static str {
        match self {
            NoticeAction::ShowCode => "Show code",
            NoticeAction::FullScreen => "Full screen",
            NoticeAction::CopyAddress => "Copy address",
        }
    }
}

/// A notification button: what it does and its words.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NoticeButton {
    pub action: NoticeAction,
    pub label: String,
}

impl From<NoticeAction> for NoticeButton {
    fn from(action: NoticeAction) -> NoticeButton {
        NoticeButton {
            action,
            label: action.label().into(),
        }
    }
}

/// Android's ongoing notification, ready to post.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum OngoingFace {
    /// "Watching for Monero sales", and "The Bean Room · Front Counter".
    Watching { title: String, text: String },
    Sale {
        order_id: String,
        face: Face,
        /// The sale's title, the notification's sub-text.
        sub_text: String,
        /// A heads-up: the sale is new.
        alert: bool,
        buttons: Vec<NoticeButton>,
    },
    Paid {
        order_id: String,
        face: Face,
        sub_text: String,
    },
}

/// iOS's alert for a new sale: time-sensitive, so it shows through Focus.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SaleAlert {
    pub order_id: String,
    /// "New Monero sale · $24.50".
    pub title: String,
    /// "Front Counter. Swipe in Till, or tap here, to show the payment
    /// code."
    pub body: String,
}

/// What the notifications show.
#[derive(Clone, Debug)]
pub struct NotificationInput {
    /// The till's sales, newest first.
    pub sales: Vec<TillSale>,
    /// The sale just paid, for the few seconds it shows as paid.
    pub just_paid: Option<String>,
    /// The sale to alert about: new since the last look
    /// ([`crate::till_nav::PhaseChanges`]), while the app isn't in front.
    pub alert_for: Option<String>,
    /// The store's name as last read.
    pub store: Option<String>,
    /// The till's Square location or Clover merchant.
    pub location: Option<String>,
}

/// The notifications, ready to post.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NotificationView {
    /// Android's ongoing notification.
    pub ongoing: Option<OngoingFace>,
    /// iOS's alert for a new sale.
    pub alert: Option<SaleAlert>,
}

/// The notifications on `device`, amounts written by `money`.
pub fn notification(
    input: &NotificationInput,
    device: Device,
    money: &dyn MoneyFormat,
) -> NotificationView {
    let location = input.location.as_deref();
    if device.is_ios() {
        let alert = input
            .alert_for
            .as_deref()
            .and_then(|id| input.sales.iter().find(|sale| sale.order_id == id))
            .filter(|sale| sale.live())
            .map(|sale| {
                let to_show = if device == Device::Ipad {
                    "Swipe in Till, or tap here, to show the payment code."
                } else {
                    "Tap here to show the payment code."
                };
                SaleAlert {
                    order_id: sale.order_id.clone(),
                    title: format!("New Monero sale · {}", sale_text::amount(sale, money)),
                    body: format!("{}. {to_show}", where_from(sale, location)),
                }
            });
        return NotificationView {
            ongoing: None,
            alert,
        };
    }
    let ongoing = match ongoing(&input.sales, input.just_paid.as_deref()) {
        Ongoing::Watching => {
            let place: Vec<&str> = [input.store.as_deref().filter(|s| !s.is_empty()), location]
                .into_iter()
                .flatten()
                .collect();
            OngoingFace::Watching {
                title: WATCHING_TITLE.into(),
                text: place.join(" · "),
            }
        }
        Ongoing::Sale(sale) => {
            let face = face(sale, location, money);
            let second = if face.full_screen {
                NoticeAction::FullScreen
            } else {
                NoticeAction::CopyAddress
            };
            OngoingFace::Sale {
                order_id: sale.order_id.clone(),
                sub_text: sale_text::title(sale),
                alert: input.alert_for.as_deref() == Some(sale.order_id.as_str()),
                buttons: vec![NoticeAction::ShowCode.into(), second.into()],
                face,
            }
        }
        Ongoing::Paid(sale) => OngoingFace::Paid {
            order_id: sale.order_id.clone(),
            face: paid_face(sale, money),
            sub_text: sale_text::title(sale),
        },
    };
    NotificationView {
        ongoing: Some(ongoing),
        alert: None,
    }
}

/// Ported from Android's `NotificationFaceTest`, test for test (the icon's
/// drawable and colour int are [`FaceIcon`] and [`FaceTint`] here); then
/// the ended notices and each device's notifications.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{found, money, receipt as make_receipt, till_sale};
    use orf_codec::Receipt;

    fn bean_room() -> Receipt {
        make_receipt(
            "bP9m",
            "2026-10-22T16:07:00-07:00",
            &[
                ("Flat white", 2.0, 4.5, 9.0),
                ("Almond croissant", 1.0, 3.75, 3.75),
            ],
            &[],
            13.88,
            "USD",
        )
    }

    fn sale(id: &str, phase: SalePhase, receipt: Option<Receipt>, mine: bool) -> TillSale {
        TillSale {
            reference: Some("bP9m".into()),
            code: till_sales::PaymentCode {
                uri: "monero:4abc?tx_amount=0.05552".into(),
                has_receipt: receipt.is_some(),
                why: None,
            },
            phase,
            found: mine.then(|| {
                found(
                    "p1",
                    1388,
                    "USD",
                    Some("bP9m"),
                    "ord",
                    "2026-10-22T23:07:00Z",
                )
            }),
            receipt,
            device_name: (!mine).then(|| "Back till".into()),
            ..till_sale(id, "13.88", "USD", "0.05552")
        }
    }

    fn of(phase: SalePhase) -> TillSale {
        sale("o1", phase, Some(bean_room()), true)
    }

    fn amount() -> String {
        sale_text::amount(&of(SalePhase::Waiting), &money)
    }

    #[test]
    fn a_new_sale_says_what_was_bought_on_the_private_version_only() {
        let face = face(&of(SalePhase::Waiting), Some("Front Counter"), &money);
        let amount = amount();
        assert_eq!(face.icon, FaceIcon::RingQuarter);
        assert_eq!(face.tint, FaceTint::Accent);
        assert_eq!(face.title, "New Monero sale · Front Counter");
        assert_eq!(
            face.text,
            format!("{amount} · 3 items · waiting for payment")
        );
        assert_eq!(
            face.detail.as_deref(),
            Some("2 × Flat white, Almond croissant")
        );
        assert_eq!(
            face.public_text,
            format!("{amount} · waiting for payment"),
            "the lock screen never shows an item or a count"
        );
        assert_eq!(face.step, 1);
        assert!(face.full_screen);
    }

    #[test]
    fn the_chips_ring_fills_a_quarter_a_step() {
        let amount = amount();
        assert_eq!(
            face(&of(SalePhase::Partial), None, &money).icon,
            FaceIcon::RingQuarter
        );
        let seen = face(&of(SalePhase::Seen), None, &money);
        assert_eq!(seen.icon, FaceIcon::RingHalf);
        assert_eq!(seen.title, "Seen on the network");
        assert_eq!(
            seen.text,
            format!("{amount} · 3 items · waiting for confirmation")
        );
        assert!(!seen.full_screen, "once seen the customer has scanned");
        let confirming = face(&of(SalePhase::Confirming), None, &money);
        assert_eq!(confirming.icon, FaceIcon::RingThreeQuarters);
        assert_eq!(
            confirming.text,
            format!("{amount} · 3 items · nearly there")
        );
        assert_eq!(confirming.step, 3);
    }

    #[test]
    fn another_tills_sale_has_no_items_to_tell() {
        let theirs = sale("o1", SalePhase::Waiting, None, false);
        let f = face(&theirs, Some("Front Counter"), &money);
        assert_eq!(f.text, format!("{} · waiting for payment", amount()));
        assert_eq!(f.detail, None);
        assert_eq!(f.title, "New Monero sale · Front Counter");
        assert_eq!(
            face(&theirs, None, &money).title,
            "New Monero sale · From Back till"
        );
    }

    #[test]
    fn a_long_order_lists_every_line_and_lets_android_cut_it_short() {
        let mut long = bean_room();
        let chai = make_receipt("c", "x", &[("Chai", 1.0, 4.75, 4.75)], &[], 4.75, "USD");
        for _ in 0..4 {
            long.line_items.push(chai.line_items[0].clone());
        }
        assert_eq!(
            face(
                &sale("o1", SalePhase::Waiting, Some(long), true),
                None,
                &money
            )
            .detail
            .as_deref(),
            Some("2 × Flat white, Almond croissant, Chai, Chai, Chai, Chai")
        );
    }

    #[test]
    fn whom_a_sale_is_for_is_never_on_a_notification_private_version_or_public() {
        let named = TillSale {
            customer_name: Some("Ana López".into()),
            ..of(SalePhase::Waiting)
        };
        let paid = TillSale {
            phase: SalePhase::Paid,
            ..named.clone()
        };
        for f in [
            face(&named, Some("Front Counter"), &money),
            paid_face(&paid, &money),
        ] {
            let words = [
                Some(&f.title),
                Some(&f.text),
                f.detail.as_ref(),
                Some(&f.public_text),
            ];
            assert!(
                !words.into_iter().flatten().any(|w| w.contains("Ana")),
                "{f:?}"
            );
        }
        assert_eq!(paid_face(&paid, &money).title, "Paid · Sale #bP9m");
        let alert = notification(
            &NotificationInput {
                sales: vec![named],
                just_paid: None,
                alert_for: Some("o1".into()),
                store: None,
                location: Some("Front Counter".into()),
            },
            Device::Ipad,
            &money,
        )
        .alert
        .unwrap();
        assert!(!alert.title.contains("Ana") && !alert.body.contains("Ana"));
    }

    #[test]
    fn paid_is_a_green_tick_and_the_amount_received() {
        let f = paid_face(&of(SalePhase::Paid), &money);
        assert_eq!(f.icon, FaceIcon::Tick);
        assert_eq!(f.tint, FaceTint::Paid);
        assert_eq!(f.title, "Paid · Sale #bP9m");
        assert_eq!(f.text, format!("{} · 0.05552 XMR received", amount()));
        assert_eq!(f.detail, None);
        assert_eq!(f.step, 4);
        assert!(!f.full_screen);
    }

    #[test]
    fn the_ongoing_notification_follows_the_newest_live_sale_a_sale_just_paid_or_watching() {
        let waiting = sale("o2", SalePhase::Waiting, Some(bean_room()), true);
        let paid = sale("o1", SalePhase::Paid, Some(bean_room()), true);
        let both = [waiting.clone(), paid.clone()];
        assert_eq!(
            ongoing(&both, Some("o1")),
            Ongoing::Paid(&paid),
            "the tick first, for a few seconds"
        );
        assert_eq!(
            ongoing(&both, None),
            Ongoing::Sale(&waiting),
            "then the sale still waiting"
        );
        assert_eq!(
            ongoing(std::slice::from_ref(&paid), None),
            Ongoing::Watching,
            "then back to watching"
        );
        let closed = TillSale {
            closed_at: Some(5),
            ..of(SalePhase::Waiting)
        };
        assert_eq!(
            ongoing(&[closed], None),
            Ongoing::Watching,
            "a closed sale drops the chip at once"
        );
        for phase in [
            SalePhase::Expired,
            SalePhase::Cancelled,
            SalePhase::DoubleSpend,
        ] {
            assert_eq!(
                ongoing(&[of(phase)], Some("o1")),
                Ongoing::Watching,
                "{phase:?}: no chip, and no tick"
            );
        }
    }

    // The ended notices, and each device's notifications.

    #[test]
    fn an_ended_sale_says_how_it_ended_and_what_to_do() {
        assert_eq!(
            paid_notice(&of(SalePhase::Paid), &money),
            Notice {
                title: "Paid · Sale #bP9m".into(),
                text: Some("$13.88 · 0.05552 XMR received".into()),
            }
        );
        let square = Some(PointOfSale::Square);
        assert_eq!(
            unpaid_notice(&of(SalePhase::Expired), square, &money),
            Notice {
                title: "Code expired · not paid · Sale #bP9m".into(),
                text: Some("$13.88. Refund it in Square if the customer isn't paying.".into()),
            }
        );
        assert_eq!(
            unpaid_notice(&sale("o1", SalePhase::Expired, None, false), square, &money)
                .text
                .as_deref(),
            Some("$13.88. Refund it in Square on the till that took it.")
        );
        assert_eq!(ended_summary(3).title, "3 sales ended");
    }

    fn input(sales: Vec<TillSale>, alert_for: Option<&str>) -> NotificationInput {
        NotificationInput {
            sales,
            just_paid: None,
            alert_for: alert_for.map(Into::into),
            store: Some("The Bean Room".into()),
            location: Some("Front Counter".into()),
        }
    }

    #[test]
    fn android_keeps_one_ongoing_notification_that_becomes_the_sale() {
        let watching = notification(&input(vec![], None), Device::Android, &money);
        assert_eq!(
            watching.ongoing,
            Some(OngoingFace::Watching {
                title: "Watching for Monero sales".into(),
                text: "The Bean Room · Front Counter".into(),
            })
        );
        assert_eq!(watching.alert, None);
        let new = notification(
            &input(vec![of(SalePhase::Waiting)], Some("o1")),
            Device::Android,
            &money,
        );
        let Some(OngoingFace::Sale {
            alert,
            buttons,
            sub_text,
            face,
            ..
        }) = new.ongoing
        else {
            panic!("{new:?}");
        };
        assert!(alert);
        assert_eq!(sub_text, "Sale #bP9m");
        assert_eq!(face.title, "New Monero sale · Front Counter");
        assert_eq!(
            buttons.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(),
            ["Show code", "Full screen"]
        );
        let seen = notification(
            &input(vec![of(SalePhase::Seen)], None),
            Device::Android,
            &money,
        );
        let Some(OngoingFace::Sale { alert, buttons, .. }) = seen.ongoing else {
            panic!();
        };
        assert!(!alert);
        assert_eq!(buttons[1].label, "Copy address");
        let paid = notification(
            &NotificationInput {
                just_paid: Some("o1".into()),
                ..input(vec![of(SalePhase::Paid)], None)
            },
            Device::Android,
            &money,
        );
        assert!(matches!(paid.ongoing, Some(OngoingFace::Paid { .. })));
    }

    #[test]
    fn ios_alerts_a_new_sale_in_its_own_words_on_ipad_and_iphone() {
        let sales = vec![TillSale {
            amount: "24.50".into(),
            ..of(SalePhase::Waiting)
        }];
        let ipad = notification(&input(sales.clone(), Some("o1")), Device::Ipad, &money);
        assert_eq!(
            ipad.ongoing, None,
            "the Live Activity is iOS's ongoing face"
        );
        assert_eq!(
            ipad.alert,
            Some(SaleAlert {
                order_id: "o1".into(),
                title: "New Monero sale · $24.50".into(),
                body: "Front Counter. Swipe in Till, or tap here, to show the payment code.".into(),
            })
        );
        let iphone = notification(&input(sales.clone(), Some("o1")), Device::Iphone, &money);
        assert_eq!(
            iphone.alert.unwrap().body,
            "Front Counter. Tap here to show the payment code."
        );
        assert_eq!(
            notification(&input(sales, None), Device::Ipad, &money).alert,
            None,
            "nothing new, nothing to say"
        );
        let theirs = notification(
            &NotificationInput {
                location: None,
                ..input(
                    vec![sale("o1", SalePhase::Waiting, None, false)],
                    Some("o1"),
                )
            },
            Device::Ipad,
            &money,
        );
        assert_eq!(
            theirs.alert.unwrap().body,
            "From Back till. Swipe in Till, or tap here, to show the payment code."
        );
    }
}
