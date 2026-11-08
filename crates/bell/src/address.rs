//! Where a Bell is (Android: `push/BellAddress.kt`).

use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;

/// Where a Monokular Bell is (docs/design.md §3): `https://bell.example`,
/// with a path when the Bell is served under one. Made by
/// [`BellAddress::parse`], which refuses anything else (decision 98), so a
/// kept one is always a Bell the app would talk to. A Bell is reached
/// directly, never over Tor: an onion address is refused.
///
/// Kept as `{"url": "…"}`, as the Android app keeps it; reading one back
/// checks it again, so a kept address that isn't one fails to read.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Kept", into = "Kept")]
pub struct BellAddress {
    url: String,
}

/// A string that isn't a Bell address, given to [`BellAddress::new`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("not a Bell address: {0}")]
pub struct NotABellAddress(pub String);

impl BellAddress {
    /// What the field says under a refused address (acceptance A1.5).
    pub const REFUSED: &'static str = "Enter the Bell's https address, such as bell.example.org";
    /// What the field says under an onion address.
    pub const ONION: &'static str =
        "A Bell can't be an onion address: the Bell is never reached over Tor";

    /// The address `url` as it is, if it is one (plain http allowed, as a
    /// debug build allows it). [`BellAddress::parse`] is what reads what a
    /// cashier typed; this is for an address already made by it.
    pub fn new(url: impl Into<String>) -> Result<BellAddress, NotABellAddress> {
        let url = url.into();
        match BellAddress::problem(&url, true) {
            None => Ok(BellAddress { url }),
            Some(_) => Err(NotABellAddress(url)),
        }
    }

    /// The Bell address `text` names, or `None` when it isn't one
    /// ([`BellAddress::problem`] says why). A missing scheme means https; a
    /// trailing slash is dropped. Plain http is taken only when
    /// `allow_http` (a debug build: a Bell on the developer's machine).
    pub fn parse(text: &str, allow_http: bool) -> Option<BellAddress> {
        if BellAddress::problem(text, allow_http).is_some() {
            return None;
        }
        let parsed = parsed(text)?;
        Some(BellAddress {
            url: parsed.as_str().trim_end_matches('/').to_owned(),
        })
    }

    /// Why `text` isn't a Bell address, in the cashier's words, or `None`
    /// when it is one.
    pub fn problem(text: &str, allow_http: bool) -> Option<&'static str> {
        let Some(parsed) = parsed(text) else {
            return Some(BellAddress::REFUSED);
        };
        let host = parsed.host_str().unwrap_or_default().to_lowercase();
        if host.ends_with(".onion") {
            return Some(BellAddress::ONION);
        }
        if parsed.scheme() == "http" && !allow_http {
            return Some(BellAddress::REFUSED);
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Some(BellAddress::REFUSED);
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Some(BellAddress::REFUSED);
        }
        None
    }

    /// The address in full: `https://example.org/bell`.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The host (with its port when it isn't the scheme's own) and the path,
    /// with no scheme: what the screens show.
    pub fn display(&self) -> String {
        let Ok(parsed) = Url::parse(&self.url) else {
            return self.url.clone();
        };
        let host = parsed.host_str().unwrap_or_default();
        // The url crate leaves out a port that is the scheme's own.
        let host = match parsed.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_owned(),
        };
        host + parsed.path().trim_end_matches('/')
    }

    /// The route's address: the Bell's address followed by `path`
    /// (`/v1/info`).
    pub fn route(&self, path: &str) -> String {
        format!("{}{path}", self.url)
    }
}

impl fmt::Display for BellAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url)
    }
}

/// `text` as a web address, https when it names no scheme; `None` for
/// nothing, or for anything but an http or https address.
fn parsed(text: &str) -> Option<Url> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let text = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    let url = Url::parse(&text).ok()?;
    let web =
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some_and(|h| !h.is_empty());
    web.then_some(url)
}

#[derive(Serialize, Deserialize)]
struct Kept {
    url: String,
}

impl TryFrom<Kept> for BellAddress {
    type Error = NotABellAddress;

    fn try_from(kept: Kept) -> Result<BellAddress, NotABellAddress> {
        BellAddress::new(kept.url)
    }
}

impl From<BellAddress> for Kept {
    fn from(address: BellAddress) -> Kept {
        Kept { url: address.url }
    }
}

/// What Settings › Wake-ups takes as "My own Bell" (acceptance A1.5,
/// decision 98). Ported from `BellAddressTest`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::WakeUpChoice;

    fn url(text: &str) -> Option<String> {
        BellAddress::parse(text, false).map(|a| a.url().to_owned())
    }

    #[test]
    fn a_bare_host_is_https_and_a_trailing_slash_goes() {
        assert_eq!(
            url(" bell.example.org/ ").as_deref(),
            Some("https://bell.example.org")
        );
        assert_eq!(
            url("https://bell.example.org").as_deref(),
            Some("https://bell.example.org")
        );
    }

    #[test]
    fn a_path_and_a_port_are_kept() {
        assert_eq!(
            url("https://example.org/bell/").as_deref(),
            Some("https://example.org/bell")
        );
        assert_eq!(
            url("bell.example.org:8445").as_deref(),
            Some("https://bell.example.org:8445")
        );
    }

    #[test]
    fn it_is_shown_as_host_and_path_with_a_port_only_when_it_isnt_the_schemes() {
        let display = |text: &str, http: bool| BellAddress::parse(text, http).map(|a| a.display());
        assert_eq!(
            display("https://bell.example.org:443/", false).as_deref(),
            Some("bell.example.org")
        );
        assert_eq!(
            display("https://bell.example.org:8445", false).as_deref(),
            Some("bell.example.org:8445")
        );
        assert_eq!(
            display("https://example.org/bell/", false).as_deref(),
            Some("example.org/bell")
        );
        assert_eq!(
            display("http://10.0.2.2:8445", true).as_deref(),
            Some("10.0.2.2:8445")
        );
    }

    #[test]
    fn plain_http_is_for_debug_builds_only() {
        assert_eq!(BellAddress::parse("http://bell.example.org", false), None);
        assert_eq!(
            BellAddress::problem("http://bell.example.org", false),
            Some(BellAddress::REFUSED)
        );
        assert_eq!(
            BellAddress::parse("http://10.0.2.2:8445", true)
                .map(|a| a.url().to_owned())
                .as_deref(),
            Some("http://10.0.2.2:8445"),
        );
    }

    #[test]
    fn an_onion_address_is_refused_with_its_own_words() {
        let onion = "http://pg6mmjiyjmcrsslvykfwnntlaru7p5svn6y2ymmju6nubxndf4pscryd.onion";
        assert_eq!(BellAddress::parse(onion, true), None);
        assert_eq!(BellAddress::problem(onion, false), Some(BellAddress::ONION));
        assert_eq!(
            BellAddress::problem("https://abc.onion/bell", false),
            Some(BellAddress::ONION)
        );
    }

    #[test]
    fn nothing_a_user_name_a_query_or_a_fragment_is_refused() {
        for text in [
            "",
            "   ",
            "https://user:pw@bell.example.org",
            "https://bell.example.org/?x=1",
            "https://bell.example.org/#top",
            "not a url at all",
        ] {
            assert_eq!(BellAddress::parse(text, false), None, "{text}");
            assert_eq!(
                BellAddress::problem(text, false),
                Some(BellAddress::REFUSED),
                "{text}"
            );
        }
    }

    #[test]
    fn routes_hang_off_the_address() {
        assert_eq!(
            BellAddress::parse("https://example.org/bell/", false)
                .map(|a| a.route("/v1/info"))
                .as_deref(),
            Some("https://example.org/bell/v1/info"),
        );
    }

    #[test]
    fn a_kept_choice_restores_and_a_kept_address_that_isnt_one_doesnt() {
        assert_eq!(
            serde_json::from_str::<WakeUpChoice>(
                r#"{"type":"own","bell":{"url":"https://bell.example.org"}}"#
            )
            .unwrap(),
            WakeUpChoice::OwnBell {
                bell: BellAddress::new("https://bell.example.org").unwrap()
            },
        );
        assert_eq!(
            serde_json::from_str::<WakeUpChoice>(r#"{"type":"timer"}"#).unwrap(),
            WakeUpChoice::TimerOnly
        );
        assert!(BellAddress::new("javascript:alert(1)").is_err());
        assert!(BellAddress::new("https://abc.onion").is_err());
        assert!(serde_json::from_str::<BellAddress>(r#"{"url":"https://abc.onion"}"#).is_err());
    }
}
