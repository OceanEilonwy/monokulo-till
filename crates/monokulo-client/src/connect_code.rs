//! Connect codes (Android: `core/ConnectCode.kt`).

use reqwest::Url;

use crate::tor;

/// A connect code from a store's Monokulo dashboard (docs/design.md §2):
/// `https://pay.yourshop.com/apps/connect#token=ct_…`, scanned from its QR
/// code or pasted.
///
/// The link is on the store's own Monokulo, so the app learns where to
/// connect from it, and the token travels in the fragment, which a browser
/// never sends to a server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectCode {
    /// Where the token is redeemed: the link without its `#` part,
    /// `https://host[:port][/prefix]/apps/connect`. A Monokulo served under
    /// a path (`https://example.com/monokulo`) keeps it here.
    pub connect_url: String,
    pub token: String,
}

const ROUTE: &str = "/apps/connect";

/// `ct_` and 64 lowercase hex digits.
fn is_token(text: &str) -> bool {
    text.strip_prefix("ct_").is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

impl ConnectCode {
    /// The code in `text`, or `None` for anything that isn't one. Plain
    /// `http` is only taken for an onion address, or with `allow_cleartext`
    /// (debug builds, for a Monokulo on the developer's machine).
    pub fn parse(text: &str, allow_cleartext: bool) -> Option<ConnectCode> {
        let url = Url::parse(text.trim()).ok()?;
        let host = url.host_str()?.to_ascii_lowercase();
        let secure = match url.scheme() {
            "https" => true,
            "http" => host.ends_with(".onion") || allow_cleartext,
            _ => false,
        };
        let user_info = !url.username().is_empty() || url.password().is_some();
        if !secure || user_info || url.query().is_some() {
            return None;
        }
        // `/apps/connect`, after the Monokulo's own path when it's served under one.
        let path = url.path();
        if !path.ends_with(ROUTE) {
            return None;
        }
        let token = url
            .fragment()?
            .split('&')
            .find_map(|part| part.strip_prefix("token="))
            .filter(|token| is_token(token))?;
        let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
        Some(ConnectCode {
            connect_url: format!("{}://{host}{port}{path}", url.scheme()),
            token: token.to_owned(),
        })
    }

    /// The host shown on the confirm screen, with its port when it has one.
    pub fn host(&self) -> String {
        Url::parse(&self.connect_url)
            .ok()
            .and_then(|url| {
                let host = url.host_str()?.to_owned();
                Some(match url.port() {
                    Some(port) => format!("{host}:{port}"),
                    None => host,
                })
            })
            .unwrap_or_default()
    }

    /// Whether the way there is encrypted: https, or Tor to an onion address.
    pub fn encrypted(&self) -> bool {
        self.connect_url.starts_with("https://") || tor::is_onion(&self.connect_url)
    }
}
