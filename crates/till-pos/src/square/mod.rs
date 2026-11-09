//! Square (docs/design.md §1): OAuth with PKCE on the device
//! ([`SquareAuth`]), the parts of its API the till uses ([`SquareApi`]),
//! Square as a [`SaleSource`](crate::SaleSource) ([`SquareSource`]), and its
//! orders as receipts ([`receipt`]).

mod api;
mod auth;
mod config;
mod receipts;
mod source;

pub use api::{
    Device, External, LineItem, Modifier, Money, OrderTax, SquareApi, SquareLocation, SquareOrder,
    SquarePayment, SquareRefund,
};
pub use auth::SquareAuth;
pub use config::SquareConfig;
pub use receipts::receipt;
pub use source::SquareSource;

use monokulo_client::PointOfSale;

use crate::PosError;

/// The first error Square gave, from its `errors` array: its `detail`, else
/// its `code`.
fn square_error(body: &str) -> Option<String> {
    let body: serde_json::Value = serde_json::from_str(body).ok()?;
    let first = body.get("errors")?.as_array()?.first()?;
    let said = first
        .get("detail")
        .filter(|v| !v.is_null())
        .or_else(|| first.get("code"))?;
    match said {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(_) | serde_json::Value::Bool(_) => Some(said.to_string()),
        _ => None,
    }
}

/// Square's error for an answer with `status` and `body`.
fn answered(status: u16, body: &str) -> PosError {
    PosError::answered(
        PointOfSale::Square,
        status,
        square_error(body).unwrap_or_else(|| format!("Square said {status}.")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squares_error_is_the_first_detail_else_its_code() {
        assert_eq!(
            square_error(
                r#"{"errors":[{"code":"UNAUTHORIZED","detail":"Refresh token expired"}]}"#
            ),
            Some("Refresh token expired".into())
        );
        assert_eq!(
            square_error(r#"{"errors":[{"code":"NOT_FOUND"}]}"#),
            Some("NOT_FOUND".into())
        );
        assert_eq!(square_error(r#"{"errors":[]}"#), None);
        assert_eq!(square_error("<html>"), None);
        assert_eq!(answered(502, "").to_string(), "Square said 502.");
    }
}
