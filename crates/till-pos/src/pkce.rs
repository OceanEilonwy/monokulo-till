//! PKCE (RFC 7636), as both points of sale sign in with it (Android:
//! `Pkce.kt`).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use till_types::Secret;

/// One sign-in attempt's PKCE verifier and state. The app keeps it while
/// the cashier signs in, and hands it back with the redirect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pkce {
    /// The `code_verifier`, sent only with the code it was made for.
    pub verifier: Secret,
    /// The `state`, which the redirect must echo.
    pub state: String,
}

impl Pkce {
    /// A new attempt: a verifier of 64 random bytes and a state of 32, each
    /// base64url without padding.
    ///
    /// # Panics
    ///
    /// When the operating system has no randomness to give, which no
    /// supported device lacks.
    pub fn create() -> Pkce {
        Pkce {
            verifier: Secret::new(random_string::<64>()),
            state: random_string::<32>(),
        }
    }

    /// The `code_challenge`: base64url(SHA-256(verifier)), no padding.
    pub fn challenge(&self) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.expose_str().as_bytes()))
    }
}

fn random_string<const N: usize>() -> String {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("the operating system's random numbers");
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempts_differ_and_are_url_safe() {
        let (a, b) = (Pkce::create(), Pkce::create());
        assert_ne!(a, b);
        assert_eq!(a.verifier.expose_str().len(), 86);
        assert_eq!(a.state.len(), 43);
        let safe = |s: &str| {
            s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        };
        assert!(safe(a.verifier.expose_str()) && safe(&a.state) && safe(&a.challenge()));
        assert!(!format!("{a:?}").contains(a.verifier.expose_str()));
    }
}
