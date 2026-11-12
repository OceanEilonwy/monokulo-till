//! What can go wrong asking a payment backend.

use monokulo_client::{MonokuloError, problems};

/// A call to a payment backend that didn't succeed.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// What the store's Monokulo answered, or why it couldn't be asked.
    #[error(transparent)]
    Monokulo(#[from] MonokuloError),
    /// The backend can't do what was asked: "make a new code for a sale".
    #[error("This till's payments can't {0}.")]
    Unsupported(&'static str),
    /// Anything else, in the backend's own words.
    #[error("{0}")]
    Other(String),
}

impl BackendError {
    /// The HTTP status it carries, as [`MonokuloError::status`] gives it; 0
    /// for none.
    pub fn status(&self) -> u16 {
        match self {
            BackendError::Monokulo(e) => e.status(),
            _ => 0,
        }
    }

    /// The problem's name (`rate-limited`), when the backend answered with
    /// one.
    pub fn problem(&self) -> Option<&str> {
        match self {
            BackendError::Monokulo(e) => e.problem(),
            _ => None,
        }
    }

    /// Whether the backend was answering, rather than not reached
    /// ([`MonokuloError::answered`]): what it said is shown as it is
    /// ([`BackendError::say`]), and a failure to reach it after "Couldn't
    /// reach the store: ".
    pub fn answered(&self) -> bool {
        match self {
            BackendError::Monokulo(e) => e.answered(),
            BackendError::Unsupported(_) => true,
            BackendError::Other(_) => false,
        }
    }

    /// The till's link no longer works (Monokulo answered 401): stop asking.
    pub fn disconnected(&self) -> bool {
        self.status() == 401
    }

    /// What the till says about it: what Monokulo's problem means and who
    /// can fix it ([`problems::say`]), else its own words.
    pub fn say(&self) -> String {
        match self {
            BackendError::Monokulo(e) => problems::say(e),
            e => e.to_string(),
        }
    }

    /// The Monokulo error underneath, for a caller that tells Monokulo's
    /// failures apart.
    pub fn monokulo(&self) -> Option<&MonokuloError> {
        match self {
            BackendError::Monokulo(e) => Some(e),
            _ => None,
        }
    }
}
