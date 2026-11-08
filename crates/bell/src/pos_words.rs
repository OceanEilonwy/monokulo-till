//! The words Wake-ups' statuses give a point of sale, in the cashier's words
//! (acceptance T5.2; Android: `push/PosWords.kt`). Each is one exhaustive
//! `match`, so a point of sale added later doesn't compile until it has
//! them. The screens' own words are elsewhere; these are the reasons a
//! [`WakeUpStatus`](crate::WakeUpStatus) carries.

use monokulo_client::PointOfSale;

/// The Bell runs nothing for `point_of_sale`: refused for good.
pub fn not_rung(point_of_sale: PointOfSale) -> String {
    format!("it doesn't ring {} tills", point_of_sale.display_name())
}

/// The Bell rings `point_of_sale`, but not in the till's `environment`:
/// refused for good ("it doesn't ring Square sandbox tills", "it doesn't
/// ring Clover tills in Europe").
pub fn not_rung_in(point_of_sale: PointOfSale, environment: &str) -> String {
    format!("it doesn't ring {}", tills(point_of_sale, environment))
}

/// `point_of_sale`'s tills in `environment`; one the point of sale doesn't
/// have goes by its wire name.
fn tills(point_of_sale: PointOfSale, environment: &str) -> String {
    match point_of_sale {
        PointOfSale::Square => match environment {
            "production" => "Square production tills".into(),
            "sandbox" => "Square sandbox tills".into(),
            _ => format!("Square {environment} tills"),
        },
        PointOfSale::Clover => match environment {
            "north_america" => "Clover tills in North America".into(),
            "europe" => "Clover tills in Europe".into(),
            "latin_america" => "Clover tills in Latin America".into(),
            "sandbox" => "Clover sandbox tills".into(),
            _ => format!("Clover {environment} tills"),
        },
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
