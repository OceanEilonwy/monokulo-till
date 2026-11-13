//! The points of sale Till watches (Android: `core/PointOfSale.kt`).

use serde::{Deserialize, Serialize};

/// A point of sale Till watches for Monero sales (docs/design.md §1,
/// "POS sources"), named as the Monokulo Bell names it: a registration's
/// `point_of_sale`, `/v1/info`'s `points_of_sale` keys and a ding's
/// `{"wake":"<pos>"}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointOfSale {
    Square,
    Clover,
    /// Till's own keypad on the device (docs/view-key-mode.md §7): no
    /// sign-in, no webhook, so the Bell never sees it.
    Till,
}

impl PointOfSale {
    /// Every point of sale the till signs in to and a Bell rings for.
    pub const ALL: [PointOfSale; 2] = [PointOfSale::Square, PointOfSale::Clover];

    /// Every point of sale, the keypad too, in the order screens list them.
    pub const EVERY: [PointOfSale; 3] =
        [PointOfSale::Square, PointOfSale::Clover, PointOfSale::Till];

    /// The name on the wire, between the till and a Bell.
    pub fn wire(self) -> &'static str {
        match self {
            PointOfSale::Square => "square",
            PointOfSale::Clover => "clover",
            PointOfSale::Till => "till",
        }
    }

    /// The name on screen: "Sign in to Square", "Refund it in Clover".
    pub fn display_name(self) -> &'static str {
        match self {
            PointOfSale::Square => "Square",
            PointOfSale::Clover => "Clover",
            PointOfSale::Till => "Till",
        }
    }

    /// The point of sale `wire` names, or `None` for one this build doesn't
    /// know. Only the exact lowercase name counts.
    pub fn from_wire(wire: &str) -> Option<PointOfSale> {
        PointOfSale::EVERY
            .into_iter()
            .find(|pos| pos.wire() == wire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_wire_name_reads_back_as_its_point_of_sale() {
        for pos in PointOfSale::ALL {
            assert_eq!(PointOfSale::from_wire(pos.wire()), Some(pos));
        }
        let wires: Vec<_> = PointOfSale::ALL.iter().map(|p| p.wire()).collect();
        assert_eq!(wires, ["square", "clover"]);
        let names: Vec<_> = PointOfSale::ALL.iter().map(|p| p.display_name()).collect();
        assert_eq!(names, ["Square", "Clover"]);
    }

    #[test]
    fn the_keypad_is_till_on_the_wire_and_no_bells() {
        assert_eq!(PointOfSale::from_wire("till"), Some(PointOfSale::Till));
        assert_eq!(PointOfSale::Till.display_name(), "Till");
        assert!(!PointOfSale::ALL.contains(&PointOfSale::Till));
    }

    #[test]
    fn a_name_this_build_does_not_know_or_another_spelling_is_none() {
        assert_eq!(PointOfSale::from_wire("toast"), None);
        assert_eq!(
            PointOfSale::from_wire("Square"),
            None,
            "the wire name is lowercase"
        );
        assert_eq!(PointOfSale::from_wire(" clover"), None);
        assert_eq!(PointOfSale::from_wire(""), None);
    }

    #[test]
    fn serde_uses_the_wire_name() {
        assert_eq!(
            serde_json::to_string(&PointOfSale::Clover).unwrap(),
            r#""clover""#
        );
        assert_eq!(
            serde_json::from_str::<PointOfSale>(r#""square""#).unwrap(),
            PointOfSale::Square
        );
    }
}
