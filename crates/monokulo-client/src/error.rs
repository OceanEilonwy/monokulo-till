//! What can go wrong talking to a store's Monokulo (Android:
//! `MonokuloException` in `core/MonokuloApi.kt`).

/// A call to Monokulo that didn't succeed.
///
/// The Android app had one exception with a status, a message and the
/// problem's name; the cases it told apart by status 0 or 403 are variants
/// here, and [`MonokuloError::status`] still gives the number a caller
/// checks (401 means the store disconnected this device).
#[derive(Debug, thiserror::Error)]
pub enum MonokuloError {
    /// Monokulo answered with a failure: its status and what its problem
    /// (`application/problem+json`, RFC 9457) said, its `detail`, else its
    /// `title`, else `HTTP <status>`.
    #[error("{detail}")]
    Refused {
        status: u16,
        detail: String,
        /// The problem's name, the end of its `type` (`invalid-connect-code`),
        /// when it was one.
        problem: Option<String>,
        /// Seconds to wait before asking again, for `rate-limited`: the
        /// problem's `retry_after`, else the `Retry-After` header.
        retry_after: Option<u64>,
    },
    /// The entry offers no link or form for what was asked: the store's
    /// Monokulo doesn't let this key do it. Nothing was sent.
    #[error("The store's API doesn't let this device {0}.")]
    NotAllowed(&'static str),
    /// The handshake's answer named no API.
    #[error("The store's answer didn't say where its API is.")]
    NoApi,
    /// A link or a kept address that isn't an http or https URL.
    #[error("{0} isn't a web address")]
    NotAnAddress(String),
    /// A success whose body isn't what Monokulo's API answers.
    #[error("the store's answer couldn't be read: {0}")]
    Unreadable(#[from] serde_json::Error),
    /// The request didn't get an answer: no connection, a timeout, TLS.
    #[error("{0}")]
    Network(#[from] reqwest::Error),
    /// The store ended its event stream.
    #[error("the store closed its event stream")]
    StreamClosed,
    /// The event stream said nothing, not even its keep-alive comment, for
    /// this many seconds: the connection is dead.
    #[error("the event stream was silent for {0} seconds")]
    StreamSilent(u64),
}

impl MonokuloError {
    /// The HTTP status as the Android app had it: Monokulo's for
    /// [`MonokuloError::Refused`], 403 for [`MonokuloError::NotAllowed`], and
    /// 0 when there was no answer to take one from.
    pub fn status(&self) -> u16 {
        match self {
            MonokuloError::Refused { status, .. } => *status,
            MonokuloError::NotAllowed(_) => 403,
            _ => 0,
        }
    }

    /// The problem's name (`rate-limited`), when Monokulo answered with one.
    pub fn problem(&self) -> Option<&str> {
        match self {
            MonokuloError::Refused { problem, .. } => problem.as_deref(),
            _ => None,
        }
    }

    /// Seconds to wait before asking again, when Monokulo said.
    pub fn retry_after(&self) -> Option<u64> {
        match self {
            MonokuloError::Refused { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// Whether Monokulo was answering: its refusal, or this client's own
    /// words about what it answered or how its event stream ended. These
    /// are what the Kotlin client threw as a `MonokuloException`, which the
    /// till shows as they are (through `problems::say`). A network failure,
    /// an unreadable answer and a bad address were other exceptions, shown
    /// after "Couldn't reach the store: ".
    pub fn answered(&self) -> bool {
        match self {
            MonokuloError::Refused { .. }
            | MonokuloError::NotAllowed(_)
            | MonokuloError::NoApi
            | MonokuloError::StreamClosed
            | MonokuloError::StreamSilent(_) => true,
            MonokuloError::NotAnAddress(_)
            | MonokuloError::Unreadable(_)
            | MonokuloError::Network(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monokulo_answered_what_kotlin_threw_as_a_monokulo_exception() {
        let refused = MonokuloError::Refused {
            status: 422,
            detail: "That code has expired.".into(),
            problem: Some("invalid-connect-code".into()),
            retry_after: None,
        };
        assert!(refused.answered());
        assert!(MonokuloError::NotAllowed("refund").answered());
        assert!(MonokuloError::NoApi.answered());
        assert!(MonokuloError::StreamClosed.answered());
        assert!(MonokuloError::StreamSilent(60).answered());

        assert!(!MonokuloError::NotAnAddress("ftp://shop".into()).answered());
        let unreadable = serde_json::from_str::<u8>("{").unwrap_err();
        assert!(!MonokuloError::Unreadable(unreadable).answered());
    }
}
