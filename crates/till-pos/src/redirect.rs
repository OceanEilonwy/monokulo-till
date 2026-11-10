//! A point of sale's sign-in redirect, read once (OAuth 2.0's authorization
//! response, RFC 6749 §4.1.2).

use reqwest::Url;
use till_types::Secret;

till_types::text_id!(
    /// An OAuth `state`: what a sign-in attempt sends, and what its redirect
    /// must echo.
    OAuthState
);

/// An authorization code: spent once on the sign-in's tokens, and never
/// logged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthCode(pub Secret);

impl AuthCode {
    pub fn new(code: impl Into<String>) -> AuthCode {
        AuthCode(Secret::new(code.into()))
    }
}

/// What a sign-in's redirect said: its `state`, `code` and `error`, and
/// Clover's `merchant_id`, each `None` when it didn't say it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OAuthRedirect {
    pub state: Option<OAuthState>,
    pub code: Option<AuthCode>,
    pub error: Option<String>,
    pub merchant_id: Option<String>,
}

impl OAuthRedirect {
    /// `url`'s query, read once: the first of each parameter.
    pub fn parse(url: &Url) -> OAuthRedirect {
        let query = |name: &str| {
            url.query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
        };
        OAuthRedirect {
            state: query("state").map(OAuthState),
            code: query("code").map(AuthCode::new),
            error: query("error"),
            merchant_id: query("merchant_id"),
        }
    }

    /// The code, when the redirect has one that isn't blank.
    pub fn code(&self) -> Option<&AuthCode> {
        self.code
            .as_ref()
            .filter(|code| !code.0.expose_str().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_redirect_is_read_once_and_its_code_never_prints() {
        let url = Url::parse(
            "https://till.example/clover?merchant_id=M1&code=c0de-SECRET&state=s1&state=s2",
        )
        .unwrap();
        let redirect = OAuthRedirect::parse(&url);
        assert_eq!(redirect.state, Some(OAuthState::new("s1")), "the first");
        assert_eq!(redirect.code(), Some(&AuthCode::new("c0de-SECRET")));
        assert_eq!(redirect.merchant_id.as_deref(), Some("M1"));
        assert_eq!(redirect.error, None);
        assert!(!format!("{redirect:?}").contains("c0de-SECRET"));
        let blank = OAuthRedirect::parse(&Url::parse("https://till.example/square?code=").unwrap());
        assert_eq!(blank.code(), None);
    }
}
