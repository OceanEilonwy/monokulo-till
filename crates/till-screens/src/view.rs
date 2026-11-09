//! The parts the screens' records are made of.

use monokulo_client::PointOfSale;
use till_sales::TillSale;

use crate::sale_text;

/// What a screen is drawn on, where its words differ: iOS has no
/// UnifiedPush (its wake-ups arrive as alert pushes through the relay), no
/// bubble and no Tor, and the iPhone has the Dynamic Island.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Device {
    Android,
    Iphone,
    Ipad,
}

impl Device {
    pub fn is_ios(self) -> bool {
        matches!(self, Device::Iphone | Device::Ipad)
    }
}

/// The colour role of a value or a row that's a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Tone {
    /// The ink, or the secondary ink for a fact's value.
    Plain,
    /// Quieter than plain: a status that isn't good news yet.
    Muted,
    /// The paid green: connected.
    Paid,
    /// Something to do before it works.
    Warn,
    /// A row that does something.
    Link,
    /// A row that undoes something: Disconnect.
    Danger,
}

/// A row with a name and its value: "Payments go to · Shop wallet".
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Fact {
    pub label: String,
    pub value: String,
    pub tone: Tone,
    /// Set in the monospaced face: a wallet address.
    pub mono: bool,
}

impl Fact {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Fact {
        Fact {
            label: label.into(),
            value: value.into(),
            tone: Tone::Plain,
            mono: false,
        }
    }

    pub fn toned(self, tone: Tone) -> Fact {
        Fact { tone, ..self }
    }

    pub fn mono(self) -> Fact {
        Fact { mono: true, ..self }
    }
}

/// A point of sale's mark beside its name, each brand's own art.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PosMark {
    Square,
    Clover,
}

/// One row of a radio group: a point of sale, a way wake-ups come.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Choice {
    pub label: String,
    pub detail: Option<String>,
    pub selected: bool,
    pub enabled: bool,
}

/// A point of sale as a choice row, led by its mark on a tile.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PosChoice {
    pub mark: PosMark,
    /// Whether the mark is drawn in the theme's ink (Square's), or keeps its
    /// own colour (Clover's green).
    pub mark_in_ink: bool,
    pub choice: Choice,
}

/// A button: its words, and whether it can be pressed now.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Press {
    pub label: String,
    pub enabled: bool,
    /// Work it started is under way: the button shows progress in itself
    /// (review finding I6), as Continue does while the store is read.
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub busy: bool,
}

impl Press {
    pub fn new(label: impl Into<String>) -> Press {
        Press {
            label: label.into(),
            enabled: true,
            busy: false,
        }
    }

    pub fn enabled(self, enabled: bool) -> Press {
        Press { enabled, ..self }
    }

    pub fn busy(self, busy: bool) -> Press {
        Press { busy, ..self }
    }
}

/// A sale's phase ring: which of the four steps it's at ([`sale_text::step`]),
/// and, seen or confirming, its payment's confirmations of those the store
/// asks for; and, from those, how much of it the arc fills, whether it
/// turns and whether it's green (core.md decision 100). How the arc sweeps
/// and turns is the platform's drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Ring {
    pub step: u8,
    pub confirmations: Option<u32>,
    pub required: Option<u32>,
    /// How much of the ring the arc fills, 0 to 1, from the top, clockwise:
    /// a quarter while it waits, the whole ring paid. Seen and confirming,
    /// the payment's way to the store's threshold: [`SEEN_FILL`] seen, and
    /// the rest shared out over the confirmations asked for. With no count
    /// from Monokulo, a quarter a step.
    pub fill: f32,
    /// The arc turns: the payment is on the network, seen (step 2) or
    /// confirming (3), and not yet paid.
    pub spins: bool,
    /// The arc is the paid green: only once it's paid (core.md decision
    /// 119); seen and confirming it's the accent, as while it waits.
    pub green: bool,
}

/// How far a sale's payment has come, as the Till widget counts it and the
/// platform's thin progress bar draws it (core.md decision 119): `value` of
/// `total` thirds, waiting 0, seen 1, confirming 2, paid 3, with its words
/// ("Seen on the network") and the count beside them ("1 of 3").
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PaymentProgress {
    pub value: u8,
    pub total: u8,
    pub label: String,
    pub count: String,
    /// What a screen reader says: "Seen on the network, 1 of 3".
    pub said: String,
}

/// `sale`'s [`PaymentProgress`]; one that ended unpaid has none to show.
pub fn progress(sale: &TillSale) -> Option<PaymentProgress> {
    let (value, label) = match sale.phase {
        monokulo_client::SalePhase::Waiting => (0, "Waiting for payment"),
        monokulo_client::SalePhase::Partial => (0, "Part paid"),
        monokulo_client::SalePhase::Seen => (1, "Seen on the network"),
        monokulo_client::SalePhase::Confirming => (2, "Confirming"),
        monokulo_client::SalePhase::Paid => (3, "Paid"),
        _ => return None,
    };
    const TOTAL: u8 = 3;
    let count = format!("{value} of {TOTAL}");
    Some(PaymentProgress {
        value,
        total: TOTAL,
        said: format!("{label}, {count}"),
        label: label.into(),
        count,
    })
}

impl Eq for Ring {}

/// How much of the ring a seen payment with no confirmation yet fills.
pub const SEEN_FILL: f32 = 0.12;

impl Ring {
    /// The ring at `step` with `confirmations` of `required`.
    pub fn new(step: u8, confirmations: Option<u32>, required: Option<u32>) -> Ring {
        let spins = step == 2 || step == 3;
        let fill = match required.filter(|&r| spins && r > 0) {
            Some(required) => {
                let have = confirmations.unwrap_or(0).min(required);
                SEEN_FILL + (1.0 - SEEN_FILL) * have as f32 / required as f32
            }
            None => f32::from(step) / 4.0,
        };
        Ring {
            step,
            confirmations,
            required,
            fill,
            spins,
            green: step == 4,
        }
    }
}

/// The ring at `step` with `confirmations` of `required`, for a platform
/// that draws one without a sale (a row's badge, a preview).
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn ring_of(step: u8, confirmations: Option<u32>, required: Option<u32>) -> Ring {
    Ring::new(step, confirmations, required)
}

/// `sale`'s ring.
pub fn ring(sale: &TillSale) -> Ring {
    Ring::new(
        sale_text::step(sale.phase),
        sale.confirmations,
        sale.confirmations_required,
    )
}

/// `pos`'s mark.
pub fn mark(pos: PointOfSale) -> PosMark {
    match pos {
        PointOfSale::Square => PosMark::Square,
        PointOfSale::Clover => PosMark::Clover,
    }
}

/// "On" or "Off".
pub(crate) fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

/// The first letter in upper case: "mainnet" is "Mainnet".
pub(crate) fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Ported from Android's `PhaseRingTest`.
#[cfg(test)]
mod ring_tests {
    use super::*;

    const WAITING: u8 = 1;
    const SEEN: u8 = 2;
    const CONFIRMING: u8 = 3;
    const PAID: u8 = 4;
    const ENDED: u8 = 0;

    fn fill(step: u8, have: Option<u32>, required: Option<u32>) -> f32 {
        Ring::new(step, have, required).fill
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn with_no_count_from_monokulo_the_ring_fills_a_quarter_a_step_and_is_empty_once_a_sale_ended_unpaid()
     {
        assert_eq!(fill(WAITING, None, None), 0.25);
        assert_eq!(fill(SEEN, None, None), 0.5);
        assert_eq!(fill(CONFIRMING, None, None), 0.75);
        assert_eq!(fill(PAID, None, None), 1.0, "paid closes the ring");
        assert_eq!(fill(ENDED, None, None), 0.0);
    }

    #[test]
    fn seen_and_confirming_the_arc_is_the_payments_way_to_the_stores_threshold() {
        assert!(
            close(fill(SEEN, Some(0), Some(10)), 0.12),
            "seen: a short arc"
        );
        assert!(close(fill(SEEN, None, Some(10)), 0.12));
        assert!(close(fill(CONFIRMING, Some(1), Some(10)), 0.12 + 0.088));
        assert!(close(
            fill(CONFIRMING, Some(4), Some(10)),
            0.12 + 0.88 * 0.4
        ));
        assert!(close(
            fill(CONFIRMING, Some(8), Some(10)),
            0.12 + 0.88 * 0.8
        ));
        assert!(
            close(fill(CONFIRMING, Some(12), Some(10)), 1.0),
            "never past the full ring"
        );
        assert!(
            close(fill(CONFIRMING, Some(1), Some(3)), 0.12 + 0.88 / 3.0),
            "a store asking three grows in thirds"
        );
        assert_eq!(
            fill(CONFIRMING, Some(0), Some(0)),
            0.75,
            "no threshold to share out: by step"
        );
        assert_eq!(
            fill(WAITING, Some(0), Some(10)),
            0.25,
            "waiting stays the still quarter"
        );
        assert_eq!(fill(PAID, Some(4), Some(10)), 1.0, "paid closes it");
    }

    #[test]
    fn the_arc_spins_once_the_payment_is_seen_and_turns_green_only_paid() {
        for step in [SEEN, CONFIRMING] {
            let ring = Ring::new(step, None, None);
            assert!(
                ring.spins && !ring.green,
                "step {step}: green is for paid alone"
            );
        }
        let paid = Ring::new(PAID, None, None);
        assert!(paid.green && !paid.spins, "paid stops");
        for step in [WAITING, ENDED] {
            let ring = Ring::new(step, None, None);
            assert!(!ring.spins && !ring.green, "step {step} stands still");
        }
    }

    #[test]
    fn progress_counts_the_widgets_thirds_with_its_words() {
        use monokulo_client::SalePhase::*;
        let of = |phase| {
            progress(&TillSale {
                phase,
                ..till_sales_fixture()
            })
        };
        let seen = of(Seen).unwrap();
        assert_eq!(
            (
                seen.value,
                seen.total,
                seen.label.as_str(),
                seen.count.as_str()
            ),
            (1, 3, "Seen on the network", "1 of 3")
        );
        assert_eq!(seen.said, "Seen on the network, 1 of 3");
        assert_eq!(of(Waiting).unwrap().value, 0);
        assert_eq!(of(Partial).unwrap().value, 0);
        assert_eq!(of(Confirming).unwrap().value, 2);
        let paid = of(Paid).unwrap();
        assert_eq!((paid.value, paid.label.as_str()), (3, "Paid"));
        assert_eq!(of(Expired), None);
    }

    fn till_sales_fixture() -> TillSale {
        crate::fixtures::till_sale("o", "1.00", "USD", "0.01")
    }
}
