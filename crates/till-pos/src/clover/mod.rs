//! Clover (docs/design.md §1): low-trust OAuth with PKCE on the device
//! ([`CloverAuth`]), the parts of its `/v3` API the till uses
//! ([`CloverApi`]), Clover as a [`SaleSource`](crate::SaleSource)
//! ([`CloverSource`]), and its orders as receipts ([`receipt`]).

mod api;
mod auth;
mod config;
mod receipts;
mod source;

pub use api::{
    CloverApi, CloverDevice, CloverMerchant, CloverOrder, CloverPayment, CloverRefund,
    CloverTender, Elements, OrderLineItem, PaymentOrder, PaymentResult, PaymentState, Ref,
};
pub use auth::CloverAuth;
pub use config::{CloverConfig, CloverEnvironment};
pub use receipts::{EXPANDED_MAX, receipt};
pub use source::{CloverSource, TenderState, TenderStore};

/// Clover's `message`, from an error body, when it says one.
fn clover_error(body: &str) -> Option<String> {
    let body: serde_json::Value = serde_json::from_str(body).ok()?;
    let message = match body.get("message")? {
        serde_json::Value::String(s) => s.clone(),
        v @ (serde_json::Value::Number(_) | serde_json::Value::Bool(_)) => v.to_string(),
        _ => return None,
    };
    (!message.trim().is_empty()).then_some(message)
}

/// A `Retry-After` in seconds, the only form Clover sends; `None` when
/// absent or unreadable. A negative one is 0.
fn retry_after(header: Option<&str>) -> Option<u64> {
    let seconds: i64 = header?.trim().parse().ok()?;
    Some(seconds.max(0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clovers_message_and_retry_after_are_read_defensively() {
        assert_eq!(
            clover_error(r#"{"message":"Forbidden"}"#),
            Some("Forbidden".into())
        );
        assert_eq!(clover_error(r#"{"message":"  "}"#), None);
        assert_eq!(clover_error("Bad gateway"), None);
        assert_eq!(retry_after(Some(" 30 ")), Some(30));
        assert_eq!(retry_after(Some("-4")), Some(0));
        assert_eq!(retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")), None);
        assert_eq!(retry_after(None), None);
    }
}
