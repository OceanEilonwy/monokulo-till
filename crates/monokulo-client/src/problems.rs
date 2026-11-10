//! What the app says when Monokulo answers one of its routes with a problem
//! (Android: `core/Problems.kt`): one message per problem Monokulo's OpenAPI
//! document lists for the routes the app calls (the handshake, the entry, the
//! store, the app, orders and the event stream), saying what happened and,
//! when someone has something to do, who and what. The contract test checks
//! every listed problem has one here, so a problem added at a re-pin fails
//! there.

use crate::error::MonokuloError;

/// A problem Monokulo answers with, by its name (the end of its `type`,
/// `invalid-connect-code`): one variant per problem its OpenAPI document
/// lists for the routes the app calls, and [`MonokuloProblem::Other`] for
/// any other name, kept as it came.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MonokuloProblem {
    InvalidConnectCode,
    NoPublicAddress,
    Unauthorized,
    Forbidden,
    RateLimited,
    ChallengeRequired,
    Internal,
    InvalidRequest,
    TooLarge,
    UnsupportedMediaType,
    RatesUnavailable,
    EngineUnavailable,
    StoreUnavailable,
    InvalidAmount,
    IdempotencyConflict,
    OrderNotRenewable,
    NotFound,
    TooManyStreams,
    /// A name not listed here.
    Other(String),
}

impl MonokuloProblem {
    const NAMED: [(MonokuloProblem, &'static str); 18] = [
        (MonokuloProblem::InvalidConnectCode, "invalid-connect-code"),
        (MonokuloProblem::NoPublicAddress, "no-public-address"),
        (MonokuloProblem::Unauthorized, "unauthorized"),
        (MonokuloProblem::Forbidden, "forbidden"),
        (MonokuloProblem::RateLimited, "rate-limited"),
        (MonokuloProblem::ChallengeRequired, "challenge-required"),
        (MonokuloProblem::Internal, "internal"),
        (MonokuloProblem::InvalidRequest, "invalid-request"),
        (MonokuloProblem::TooLarge, "too-large"),
        (
            MonokuloProblem::UnsupportedMediaType,
            "unsupported-media-type",
        ),
        (MonokuloProblem::RatesUnavailable, "rates-unavailable"),
        (MonokuloProblem::EngineUnavailable, "engine-unavailable"),
        (MonokuloProblem::StoreUnavailable, "store-unavailable"),
        (MonokuloProblem::InvalidAmount, "invalid-amount"),
        (MonokuloProblem::IdempotencyConflict, "idempotency-conflict"),
        (MonokuloProblem::OrderNotRenewable, "order-not-renewable"),
        (MonokuloProblem::NotFound, "not-found"),
        (MonokuloProblem::TooManyStreams, "too-many-streams"),
    ];

    /// The problem named `slug`.
    pub fn from_slug(slug: &str) -> MonokuloProblem {
        Self::NAMED
            .iter()
            .find(|(_, name)| *name == slug)
            .map_or_else(
                || MonokuloProblem::Other(slug.to_owned()),
                |(p, _)| p.clone(),
            )
    }

    /// The problem's name, as Monokulo wrote it.
    pub fn as_str(&self) -> &str {
        match self {
            MonokuloProblem::Other(slug) => slug,
            known => Self::NAMED
                .iter()
                .find(|(p, _)| p == known)
                .map_or("", |(_, name)| name),
        }
    }
}

impl std::fmt::Display for MonokuloProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The message for `e`, else its own detail for a problem not listed here,
/// else `HTTP <status>`.
pub fn say(e: &MonokuloError) -> String {
    if let Some(said) = e.problem_kind().and_then(|problem| said(problem, e)) {
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
    let problem = MonokuloProblem::from_slug(problem);
    let e = MonokuloError::Refused {
        status: 0,
        detail: String::new(),
        problem: Some(problem.clone()),
        retry_after: None,
    };
    said(&problem, &e).is_some()
}

fn said(problem: &MonokuloProblem, e: &MonokuloError) -> Option<String> {
    use MonokuloProblem as P;
    Some(match problem {
        // The handshake.
        P::InvalidConnectCode => {
            "That code has been used or has expired. Make a new one on the store's Apps card.".into()
        }
        P::NoPublicAddress => "The store's Monokulo has no web address yet, so nothing can connect to it. Ask its admin to set one, then scan the same code again.".into(),
        // Every route.
        P::Unauthorized => {
            "The store disconnected this device. Connect it again from the store's Apps card.".into()
        }
        P::Forbidden => "The store's Monokulo doesn't let this device's key do that. Disconnect it and connect again from the store's Apps card.".into(),
        P::RateLimited => format!(
            "The store is busy and asked for {} seconds' wait before the next try.",
            e.retry_after().unwrap_or(60)
        ),
        P::ChallengeRequired => {
            "The store is busy and asked for a check this device couldn't pass. Try again in a moment.".into()
        }
        P::Internal => "Something went wrong on the store's Monokulo. Try again in a moment; if it keeps happening, its admin can find it in Monokulo's logs.".into(),
        P::InvalidRequest | P::TooLarge | P::UnsupportedMediaType => format!(
            "Monokulo didn't take what this app sent ({e}). Update Till; if it keeps happening, tell its makers."
        ),
        // Making an order: no order was made, so the till asks again with the same key.
        P::RatesUnavailable => "Monokulo has no exchange rate right now, so there's no payment code yet. Trying again shortly.".into(),
        P::EngineUnavailable => {
            "Monokulo's payment engine is busy, so there's no payment code yet. Trying again shortly.".into()
        }
        P::StoreUnavailable => "Monokulo can't read this store's settings right now, so there's no payment code yet. Trying again shortly; if it keeps happening, ask Monokulo's admin to check its database.".into(),
        P::InvalidAmount => format!(
            "Monokulo can't price this sale ({e}), so it has no payment code. Take another payment for it and refund it in the POS."
        ),
        P::IdempotencyConflict => "Monokulo already has a different order for this POS sale, so it made no payment code. Take another payment for it and refund it in the POS.".into(),
        // Renewing an order (New code): Monokulo says why it won't.
        P::OrderNotRenewable => {
            let detail = e.to_string();
            if detail.trim().is_empty() || detail.starts_with("HTTP ") {
                "Monokulo can't make a new code for this sale: a payment may have reached it already.".into()
            } else {
                detail
            }
        }
        // Reading an order, and the event stream.
        P::NotFound => "The store has no such order any more.".into(),
        P::TooManyStreams => "Too many devices are following this store's sales at once. Close Till on one you aren't using, or disconnect it from the store's Apps card.".into(),
        P::Other(_) => return None,
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
            problem: problem.map(MonokuloProblem::from_slug),
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

    #[test]
    fn a_problem_reads_and_writes_its_name() {
        for (problem, name) in MonokuloProblem::NAMED {
            assert_eq!(MonokuloProblem::from_slug(name), problem);
            assert_eq!(problem.as_str(), name);
        }
        let other = MonokuloProblem::from_slug("teapot");
        assert_eq!(other, MonokuloProblem::Other("teapot".into()));
        assert_eq!(other.to_string(), "teapot");
    }
}
