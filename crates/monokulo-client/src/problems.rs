//! What the app says when Monokulo answers one of its routes with a problem
//! (Android: `core/Problems.kt`): one message per problem Monokulo's OpenAPI
//! document lists for the routes the app calls (the handshake, the entry, the
//! store, the app, orders and the event stream), saying what happened and,
//! when someone has something to do, who and what. The contract test checks
//! every listed problem has one here, so a problem added at a re-pin fails
//! there.

use crate::error::MonokuloError;

/// The message for `e`, else its own detail for a problem not listed here,
/// else `HTTP <status>`.
pub fn say(e: &MonokuloError) -> String {
    if let Some(said) = e.problem().and_then(|problem| said(problem, e)) {
        return said;
    }
    let message = e.to_string();
    if message.trim().is_empty() {
        format!("HTTP {}", e.status())
    } else {
        message
    }
}

/// Whether `problem` has a message of its own.
pub fn knows(problem: &str) -> bool {
    let e = MonokuloError::Refused {
        status: 0,
        detail: String::new(),
        problem: Some(problem.to_owned()),
        retry_after: None,
    };
    said(problem, &e).is_some()
}

fn said(problem: &str, e: &MonokuloError) -> Option<String> {
    Some(match problem {
        // The handshake.
        "invalid-connect-code" => {
            "That code has been used or has expired. Make a new one on the store's Apps card.".into()
        }
        "no-public-address" => "The store's Monokulo has no web address yet, so nothing can connect to it. Ask its admin to set one, then scan the same code again.".into(),
        // Every route.
        "unauthorized" => {
            "The store disconnected this device. Connect it again from the store's Apps card.".into()
        }
        "forbidden" => "The store's Monokulo doesn't let this device's key do that. Disconnect it and connect again from the store's Apps card.".into(),
        "rate-limited" => format!(
            "The store is busy and asked for {} seconds' wait before the next try.",
            e.retry_after().unwrap_or(60)
        ),
        "challenge-required" => {
            "The store is busy and asked for a check this device couldn't pass. Try again in a moment.".into()
        }
        "internal" => "Something went wrong on the store's Monokulo. Try again in a moment; if it keeps happening, its admin can find it in Monokulo's logs.".into(),
        "invalid-request" | "too-large" | "unsupported-media-type" => format!(
            "Monokulo didn't take what this app sent ({e}). Update Till; if it keeps happening, tell its makers."
        ),
        // Making an order: no order was made, so the till asks again with the same key.
        "rates-unavailable" => "Monokulo has no exchange rate right now, so there's no payment code yet. Trying again shortly.".into(),
        "engine-unavailable" => {
            "Monokulo's payment engine is busy, so there's no payment code yet. Trying again shortly.".into()
        }
        "store-unavailable" => "Monokulo can't read this store's settings right now, so there's no payment code yet. Trying again shortly; if it keeps happening, ask Monokulo's admin to check its database.".into(),
        "invalid-amount" => format!(
            "Monokulo can't price this sale ({e}), so it has no payment code. Take another payment for it and refund it in the POS."
        ),
        "idempotency-conflict" => "Monokulo already has a different order for this POS sale, so it made no payment code. Take another payment for it and refund it in the POS.".into(),
        // Renewing an order (New code): Monokulo says why it won't.
        "order-not-renewable" => {
            let detail = e.to_string();
            if detail.trim().is_empty() || detail.starts_with("HTTP ") {
                "Monokulo can't make a new code for this sale: a payment may have reached it already.".into()
            } else {
                detail
            }
        }
        // Reading an order, and the event stream.
        "not-found" => "The store has no such order any more.".into(),
        "too-many-streams" => "Too many devices are following this store's sales at once. Close Till on one you aren't using, or disconnect it from the store's Apps card.".into(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(
        status: u16,
        detail: &str,
        problem: Option<&str>,
        retry_after: Option<u64>,
    ) -> MonokuloError {
        MonokuloError::Refused {
            status,
            detail: detail.into(),
            problem: problem.map(Into::into),
            retry_after,
        }
    }

    #[test]
    fn a_known_problem_has_its_words_and_waits_as_long_as_it_says() {
        assert_eq!(
            say(&refused(429, "Slow down.", Some("rate-limited"), Some(30))),
            "The store is busy and asked for 30 seconds' wait before the next try."
        );
        assert_eq!(
            say(&refused(429, "Slow down.", Some("rate-limited"), None)),
            "The store is busy and asked for 60 seconds' wait before the next try."
        );
        assert_eq!(
            say(&refused(
                400,
                "amount: too many digits",
                Some("invalid-request"),
                None
            )),
            "Monokulo didn't take what this app sent (amount: too many digits). Update Till; if it keeps happening, tell its makers."
        );
    }

    #[test]
    fn anything_else_says_its_own_detail_or_its_status() {
        assert_eq!(
            say(&refused(418, "I'm a teapot.", Some("teapot"), None)),
            "I'm a teapot."
        );
        assert_eq!(say(&refused(502, "", None, None)), "HTTP 502");
        assert!(!knows("teapot"));
        assert!(knows("not-found"));
    }
}
