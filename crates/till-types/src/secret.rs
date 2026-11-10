use std::fmt;

use serde::{Deserialize, Serialize};

/// A credential (an access token, an auth secret, an OAuth code) that must
/// never reach a log. `Debug` prints `Secret(..)`; the value is only
/// reached through [`Secret::expose`]. It serializes as the bare value
/// (`#[serde(transparent)]`), so stored JSON doesn't change.
#[derive(Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret<T = String>(T);

impl<T> Secret<T> {
    pub fn new(value: T) -> Secret<T> {
        Secret(value)
    }

    /// The value itself: for the one place that sends or stores it.
    pub fn expose(&self) -> &T {
        &self.0
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl Secret<String> {
    pub fn expose_str(&self) -> &str {
        &self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

impl From<String> for Secret<String> {
    fn from(value: String) -> Self {
        Secret(value)
    }
}

impl From<&str> for Secret<String> {
    fn from(value: &str) -> Self {
        Secret(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_the_value() {
        let secret = Secret::from("hunter2");
        assert_eq!(format!("{secret:?}"), "Secret(..)");
        assert_eq!(format!("{:?}", Some(&secret)), "Some(Secret(..))");
    }

    #[test]
    fn serializes_as_the_bare_value() {
        let secret = Secret::from("abc");
        assert_eq!(serde_json::to_string(&secret).unwrap(), "\"abc\"");
        let back: Secret = serde_json::from_str("\"abc\"").unwrap();
        assert_eq!(back.expose_str(), "abc");
    }
}
