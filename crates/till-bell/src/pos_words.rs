//! The words Wake-ups' statuses give a point of sale, in the cashier's words
//! (acceptance T5.2; Android: `push/PosWords.kt`). Each is one exhaustive
//! `match`, so a point of sale added later doesn't compile until it has
//! them. The screens' own words are elsewhere; these are the reasons a
//! [`WakeUpStatus`](crate::WakeUpStatus) carries.

use monokulo_client::PointOfSale;
use till_pos::{CloverEnvironment, PosEnvironment, SquareEnvironment};

/// The Bell runs nothing for `point_of_sale`: refused for good.
pub fn not_rung(point_of_sale: PointOfSale) -> String {
    format!("it doesn't ring {} tills", point_of_sale.display_name())
}

/// The Bell rings the till's point of sale, but not in its `environment`:
/// refused for good ("it doesn't ring Square sandbox tills", "it doesn't
/// ring Clover tills in Europe").
pub fn not_rung_in(environment: PosEnvironment) -> String {
    format!("it doesn't ring {}", tills(environment))
}

/// The point of sale's tills in `environment`.
fn tills(environment: PosEnvironment) -> &'static str {
    match environment {
        PosEnvironment::Square(SquareEnvironment::Production) => "Square production tills",
        PosEnvironment::Square(SquareEnvironment::Sandbox) => "Square sandbox tills",
        PosEnvironment::Clover(CloverEnvironment::NorthAmerica) => "Clover tills in North America",
        PosEnvironment::Clover(CloverEnvironment::Europe) => "Clover tills in Europe",
        PosEnvironment::Clover(CloverEnvironment::LatinAmerica) => "Clover tills in Latin America",
        PosEnvironment::Clover(CloverEnvironment::Sandbox) => "Clover sandbox tills",
    }
}

/// The point of sale didn't confirm the place the till registered for
/// (`{pos}-refused`).
pub fn refused(point_of_sale: PointOfSale) -> String {
    let name = point_of_sale.display_name();
    format!(
        "{name} didn't confirm this {}. Sign in to {name} again.",
        place(Some(point_of_sale))
    )
}

/// What a registration is for at `point_of_sale`: a Square location, a
/// Clover merchant (its only location).
pub fn place(point_of_sale: Option<PointOfSale>) -> &'static str {
    match point_of_sale {
        Some(PointOfSale::Square) | None => "location",
        Some(PointOfSale::Clover) => "merchant",
    }
}

/// Whose account another registration of this device is under.
pub fn account(point_of_sale: Option<PointOfSale>) -> String {
    match point_of_sale {
        None => "account".into(),
        Some(pos @ (PointOfSale::Square | PointOfSale::Clover)) => {
            format!("{} account", pos.display_name())
        }
    }
}
