//! The iPhone's Live Activity (docs/design.md §6; boards Main, PhoneOrder
//! and PhoneOrderQR): its content state, as the app sets it. Monokulo's
//! pushes set the same fields through the relay; the widget draws its own
//! words from them ("Seen", "1/2", "Tap to show code").
//!
//! iPhone's Live Activity, and the iPad's Picture in Picture window when
//! a new sale shows up in a floating window (docs/ipad-redesign.md
//! decision 2): the same state draws both. Android has its ongoing
//! notification ([`crate::notification`]).

use till_sales::TillSale;

use monokulo_client::SalePhase;

use crate::view::Device;

/// The activity's phase: its layout and colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ActivityPhase {
    Idle,
    Waiting,
    Seen,
    Confirming,
    Paid,
    Expired,
}

/// The Live Activity's content state (design.md §6's table). Every sale
/// field is `None` while idle. Swift names them `sale`, `amount`,
/// `currency`, `xmr`, `expiresAt`, `waitingCount` and `paymentUri`; the
/// relay's JSON spells the last `paymentURI`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LiveActivityState {
    pub phase: ActivityPhase,
    /// The order shown, for the tap's `monokulo-till://sale/{id}`. The app
    /// sets it; Monokulo's pushes don't carry it, so after one the tap
    /// opens the app at its shift.
    pub order_id: Option<String>,
    /// "#1042": the sale's label.
    pub sale: Option<String>,
    /// "24.50": formatted by the device with `currency`, as Monokulo sends
    /// it (no server formats money for the device's locale).
    pub amount: Option<String>,
    pub currency: Option<String>,
    /// "0.1421".
    pub xmr: Option<String>,
    /// Unix seconds: the countdown iOS draws itself, and the stale date.
    pub expires_at: Option<i64>,
    /// The till's own sales still waiting for a payment, this one included
    /// while it is: "+1 more" at 2.
    pub waiting_count: u32,
    /// Only with "Show code in island" on (always in the iPad's floating
    /// window), and only while the sale waits for its first payment: the
    /// sale's code, with its receipt when the state is drawn on the device
    /// alone, and the plain payment URI where it would go through the relay
    /// (design.md §4; core.md decision 140).
    pub payment_uri: Option<String>,
    /// Whom the sale is for ("Sam"), which leads the activity (core.md
    /// decision 118). The app sets it on the device; the relay's pushes
    /// never carry it, as names never reach Monokulo.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub name: Option<String>,
}

/// What the Live Activity shows.
#[derive(Clone, Debug)]
pub struct LiveActivityInput {
    /// The till's sales, newest first. Only its own count: the island never
    /// shows another till's sale.
    pub sales: Vec<TillSale>,
    /// The sale just paid, while it shows as paid (the app then idles the
    /// activity).
    pub just_paid: Option<String>,
    /// The sale whose code just ran out (or whose payment was reversed),
    /// while it shows as expired.
    pub just_ended: Option<String>,
    /// Settings › Dynamic Island › "Show code in island" (off by default).
    pub code_in_island: bool,
    /// The code keeps its receipt: the state is drawn on this device alone
    /// (the iPad's floating window, or a Live Activity with no push token
    /// in use), never sent through the relay (core.md decision 140).
    pub receipt: bool,
}

/// `uri` without its receipt: the payment URI as the order gave it.
pub fn plain_uri(uri: &str) -> String {
    let (head, fragment) = match uri.split_once('#') {
        Some((head, fragment)) => (head, Some(fragment)),
        None => (uri, None),
    };
    let mut plain = match head.split_once('?') {
        Some((path, query)) => {
            let kept: Vec<&str> = query
                .split('&')
                .filter(|pair| !pair.starts_with("orf=") && *pair != "orf")
                .collect();
            if kept.is_empty() {
                path.to_owned()
            } else {
                format!("{path}?{}", kept.join("&"))
            }
        }
        None => head.to_owned(),
    };
    if let Some(fragment) = fragment {
        plain.push('#');
        plain.push_str(fragment);
    }
    plain
}

/// "#1042" from the POS's receipt number.
fn label(sale: &TillSale) -> Option<String> {
    let reference = sale.reference.as_deref()?.trim();
    if reference.is_empty() {
        None
    } else if reference.starts_with('#') {
        Some(reference.into())
    } else {
        Some(format!("#{reference}"))
    }
}

/// The content state on `device`: `None` on Android. The engine asks on
/// iPad only while its Picture in Picture window is chosen.
pub fn live_activity(input: &LiveActivityInput, device: Device) -> Option<LiveActivityState> {
    if !device.is_ios() {
        return None;
    }
    let own: Vec<&TillSale> = input
        .sales
        .iter()
        .filter(|sale| sale.found.is_some())
        .collect();
    let waiting_count = own
        .iter()
        .filter(|sale| sale.live() && sale.phase.counts_down())
        .count() as u32;
    let find =
        |id: Option<&str>| id.and_then(|id| own.iter().copied().find(|sale| sale.order_id == id));
    let (phase, shown) = if let Some(paid) =
        find(input.just_paid.as_deref()).filter(|sale| sale.phase == SalePhase::Paid)
    {
        (ActivityPhase::Paid, Some(paid))
    } else if let Some(live) = own.iter().copied().find(|sale| sale.live()) {
        let phase = match live.phase {
            SalePhase::Seen => ActivityPhase::Seen,
            SalePhase::Confirming => ActivityPhase::Confirming,
            _ => ActivityPhase::Waiting,
        };
        (phase, Some(live))
    } else if let Some(ended) = find(input.just_ended.as_deref())
        .filter(|sale| sale.phase != SalePhase::Paid && !sale.live())
    {
        (ActivityPhase::Expired, Some(ended))
    } else {
        (ActivityPhase::Idle, None)
    };
    Some(LiveActivityState {
        phase,
        order_id: shown.map(|sale| sale.order_id.clone()),
        sale: shown.and_then(label),
        amount: shown.map(|sale| sale.amount.clone()),
        currency: shown.map(|sale| sale.currency.clone()),
        xmr: shown.map(|sale| sale.xmr.clone()),
        expires_at: shown.map(|sale| sale.expires_at),
        waiting_count,
        payment_uri: shown
            .filter(|sale| input.code_in_island && sale.live() && sale.phase == SalePhase::Waiting)
            .map(|sale| {
                if input.receipt {
                    sale.code.uri.clone()
                } else {
                    plain_uri(&sale.code.uri)
                }
            }),
        name: shown
            .and_then(crate::sale_text::customer)
            .map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{found, till_sale};

    fn sale(id: &str, phase: SalePhase, mine: bool) -> TillSale {
        TillSale {
            reference: Some("1042".into()),
            code: till_sales::PaymentCode {
                uri: "monero:4abc?tx_amount=0.1421&orf=Q0RF#x".into(),
                has_receipt: true,
                why: None,
            },
            phase,
            expires_at: 1_761_312_000,
            found: mine.then(|| {
                found(
                    "p1",
                    2450,
                    "USD",
                    Some("1042"),
                    "ord",
                    "2026-10-23T10:00:00Z",
                )
            }),
            device_name: (!mine).then(|| "Back till".into()),
            ..till_sale(id, "24.50", "USD", "0.1421")
        }
    }

    fn input(sales: Vec<TillSale>) -> LiveActivityInput {
        LiveActivityInput {
            sales,
            just_paid: None,
            just_ended: None,
            code_in_island: false,
            receipt: false,
        }
    }

    #[test]
    fn idle_is_the_logo_and_nothing_of_a_sale() {
        let state = live_activity(&input(vec![]), Device::Iphone).unwrap();
        assert_eq!(
            state,
            LiveActivityState {
                phase: ActivityPhase::Idle,
                order_id: None,
                sale: None,
                amount: None,
                currency: None,
                xmr: None,
                expires_at: None,
                waiting_count: 0,
                payment_uri: None,
                name: None,
            }
        );
        assert_eq!(
            live_activity(&input(vec![]), Device::Ipad),
            Some(state),
            "the iPad's floating window draws the same state"
        );
        assert_eq!(live_activity(&input(vec![]), Device::Android), None);
    }

    #[test]
    fn a_waiting_sale_carries_its_amount_unformatted_and_the_code_only_when_asked() {
        let sales = vec![
            sale("o2", SalePhase::Waiting, true),
            sale("o1", SalePhase::Waiting, true),
        ];
        let off = live_activity(&input(sales.clone()), Device::Iphone).unwrap();
        assert_eq!(
            off,
            LiveActivityState {
                phase: ActivityPhase::Waiting,
                order_id: Some("o2".into()),
                sale: Some("#1042".into()),
                amount: Some("24.50".into()),
                currency: Some("USD".into()),
                xmr: Some("0.1421".into()),
                expires_at: Some(1_761_312_000),
                waiting_count: 2,
                payment_uri: None,
                name: None,
            }
        );
        let on = live_activity(
            &LiveActivityInput {
                code_in_island: true,
                ..input(sales)
            },
            Device::Iphone,
        )
        .unwrap();
        assert_eq!(
            on.payment_uri.as_deref(),
            Some("monero:4abc?tx_amount=0.1421#x"),
            "the plain URI where it would go through the relay"
        );
        let on_device = live_activity(
            &LiveActivityInput {
                code_in_island: true,
                receipt: true,
                ..input(vec![sale("o2", SalePhase::Waiting, true)])
            },
            Device::Ipad,
        )
        .unwrap();
        assert_eq!(
            on_device.payment_uri.as_deref(),
            Some("monero:4abc?tx_amount=0.1421&orf=Q0RF#x"),
            "drawn on the device alone, the code keeps its receipt"
        );
        let named = TillSale {
            customer_name: Some(" Sam ".into()),
            ..sale("o3", SalePhase::Waiting, true)
        };
        let state = live_activity(&input(vec![named]), Device::Iphone).unwrap();
        assert_eq!(state.name.as_deref(), Some("Sam"), "the name leads it");
    }

    #[test]
    fn a_part_paid_sale_waits_without_its_code_and_seen_and_confirming_follow_the_phase() {
        let on = |phase| {
            live_activity(
                &LiveActivityInput {
                    code_in_island: true,
                    ..input(vec![sale("o1", phase, true)])
                },
                Device::Iphone,
            )
            .unwrap()
        };
        let partial = on(SalePhase::Partial);
        assert_eq!(partial.phase, ActivityPhase::Waiting);
        assert_eq!(
            partial.payment_uri, None,
            "the full amount is no longer owed"
        );
        assert_eq!(on(SalePhase::Seen).phase, ActivityPhase::Seen);
        assert_eq!(on(SalePhase::Seen).waiting_count, 0);
        assert_eq!(on(SalePhase::Confirming).phase, ActivityPhase::Confirming);
        assert_eq!(on(SalePhase::Confirming).payment_uri, None);
    }

    #[test]
    fn paid_and_expired_show_for_their_moment_then_idle() {
        let paid = LiveActivityInput {
            just_paid: Some("o1".into()),
            ..input(vec![sale("o1", SalePhase::Paid, true)])
        };
        assert_eq!(
            live_activity(&paid, Device::Iphone).unwrap().phase,
            ActivityPhase::Paid
        );
        let expired = LiveActivityInput {
            just_ended: Some("o1".into()),
            ..input(vec![sale("o1", SalePhase::Expired, true)])
        };
        let state = live_activity(&expired, Device::Iphone).unwrap();
        assert_eq!(state.phase, ActivityPhase::Expired);
        assert_eq!(state.amount.as_deref(), Some("24.50"));
        assert_eq!(state.order_id.as_deref(), Some("o1"));
        assert_eq!(
            live_activity(
                &input(vec![sale("o1", SalePhase::Paid, true)]),
                Device::Iphone
            )
            .unwrap()
            .phase,
            ActivityPhase::Idle
        );
    }

    #[test]
    fn another_tills_sale_never_reaches_the_island() {
        let state = live_activity(
            &input(vec![sale("theirs", SalePhase::Waiting, false)]),
            Device::Iphone,
        )
        .unwrap();
        assert_eq!(state.phase, ActivityPhase::Idle);
        assert_eq!(state.waiting_count, 0);
    }

    #[test]
    fn the_plain_uri_drops_only_the_receipt() {
        assert_eq!(
            plain_uri("monero:4abc?tx_amount=1&orf=X"),
            "monero:4abc?tx_amount=1"
        );
        assert_eq!(plain_uri("monero:4abc?orf=X"), "monero:4abc");
        assert_eq!(
            plain_uri("monero:4abc?tx_amount=1"),
            "monero:4abc?tx_amount=1"
        );
        assert_eq!(plain_uri("monero:4abc"), "monero:4abc");
    }
}
