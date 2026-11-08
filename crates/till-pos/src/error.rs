//! What can go wrong at a point of sale (Android: `PosException`,
//! `SquareException`, `CloverException`).

use monokulo_client::PointOfSale;

/// A point of sale's failure. The till catches this one type for every
/// point of sale (decision 131 in docs/decisions/android.md).
///
/// [`PosError::Answered`] is the Kotlin `PosException` a point of sale's
/// own subclass threw (`SquareException`, `CloverException`), and
/// [`PosError::NotAsked`] a bare `PosException`: their messages are what
/// the till shows as they are. The other variants are what Kotlin threw as
/// other exceptions (an `IOException`, a `SerializationException`), which
/// the till shows after "Couldn't reach the POS: ".
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PosError {
    /// The point of sale answered with an error, or its sign-in is gone and
    /// must be done again (`sign_in_again`).
    #[error("{message}")]
    Answered {
        /// Which point of sale said it.
        pos: PointOfSale,
        /// The HTTP status, or 0 when nothing was asked (a sign-in the app
        /// refused before asking, an answer for the wrong merchant).
        status: u16,
        /// In words the till shows.
        message: String,
        /// The sign-in has ended: the cashier must sign in again.
        sign_in_again: bool,
        /// How long the point of sale asked the till to wait, in seconds:
        /// a Clover `429`'s `Retry-After`.
        retry_after_seconds: Option<u64>,
    },
    /// The till refused before asking any point of sale, in words the till
    /// shows ("Sign in to Square first"): the Kotlin `PosException(0, …)`
    /// that named no point of sale. Its status is 0.
    #[error("{0}")]
    NotAsked(String),
    /// The point of sale couldn't be reached: no network, a TLS failure, a
    /// timeout.
    #[error("{0}")]
    Unreachable(String),
    /// The point of sale answered something this crate couldn't read.
    #[error("{0}")]
    Unreadable(String),
    /// The lock held around a refresh ([`crate::TokenLock`]) couldn't be
    /// taken.
    #[error("couldn't lock the sign-in: {0}")]
    Lock(String),
    /// What the app keeps for the point of sale (its sign-in, cursor or
    /// tender id) couldn't be read: never taken for "signed out" or "none
    /// kept", and nothing is saved over it.
    #[error("couldn't read what this till keeps: {0}")]
    Store(#[from] crate::StoreError),
}

impl PosError {
    /// An answer from `pos` that leaves the sign-in as it is.
    pub fn answered(pos: PointOfSale, status: u16, message: impl Into<String>) -> PosError {
        PosError::Answered {
            pos,
            status,
            message: message.into(),
            sign_in_again: false,
            retry_after_seconds: None,
        }
    }

    /// An answer from `pos` saying its sign-in has ended.
    pub fn sign_in(pos: PointOfSale, status: u16, message: impl Into<String>) -> PosError {
        PosError::Answered {
            pos,
            status,
            message: message.into(),
            sign_in_again: true,
            retry_after_seconds: None,
        }
    }

    /// Whether the cashier must sign in again.
    pub fn sign_in_again(&self) -> bool {
        matches!(
            self,
            PosError::Answered {
                sign_in_again: true,
                ..
            }
        )
    }

    /// The point of sale that answered (the Kotlin exception's class:
    /// `SquareException`, `CloverException`); `None` for the other
    /// variants.
    pub fn pos(&self) -> Option<PointOfSale> {
        match self {
            PosError::Answered { pos, .. } => Some(*pos),
            _ => None,
        }
    }

    /// The HTTP status of an [`PosError::Answered`] (0 when nothing was
    /// asked), and 0 for [`PosError::NotAsked`]; `None` for the other
    /// variants.
    pub fn status(&self) -> Option<u16> {
        match self {
            PosError::Answered { status, .. } => Some(*status),
            PosError::NotAsked(_) => Some(0),
            _ => None,
        }
    }

    /// How long the point of sale asked the till to wait, in seconds.
    pub fn retry_after_seconds(&self) -> Option<u64> {
        match self {
            PosError::Answered {
                retry_after_seconds,
                ..
            } => *retry_after_seconds,
            _ => None,
        }
    }

    /// Whether this is an answer with a `4xx` status other than `429`: the
    /// point of sale refused, rather than being busy or unreachable.
    #[cfg(feature = "clover")]
    pub(crate) fn is_refusal(&self) -> bool {
        matches!(self.status(), Some(400..=499)) && self.status() != Some(429)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_before_asking_names_no_point_of_sale() {
        let refused = PosError::NotAsked("Sign in to Square first".into());
        assert_eq!(refused.to_string(), "Sign in to Square first");
        assert_eq!(refused.pos(), None);
        assert_eq!(refused.status(), Some(0));
        assert!(!refused.sign_in_again());
        assert_eq!(refused.retry_after_seconds(), None);
    }
}
