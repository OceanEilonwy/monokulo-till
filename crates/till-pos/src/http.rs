//! Addresses and answers, as both points of sale's clients use them.

use monokulo_client::PointOfSale;
use serde::de::DeserializeOwned;

use crate::PosError;

/// `text` percent-encoded for a path segment or a query component: every
/// byte but RFC 3986's unreserved ones (letters, digits, `-._~`) escaped,
/// so a space is `%20`, as OkHttp wrote it, never `+`.
pub(crate) fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `base` followed by `segments`, each encoded as one path segment, then
/// `query`'s pairs in order (a name may repeat).
pub(crate) fn url(base: &str, segments: &[&str], query: &[(&str, &str)]) -> String {
    let mut out = base.trim_end_matches('/').to_owned();
    for segment in segments {
        out.push('/');
        out.push_str(&encode(segment));
    }
    for (i, (name, value)) in query.iter().enumerate() {
        out.push(if i == 0 { '?' } else { '&' });
        out.push_str(&encode(name));
        out.push('=');
        out.push_str(&encode(value));
    }
    out
}

/// A point of sale's answer, whatever its status.
pub(crate) struct Answer {
    pub status: u16,
    pub body: String,
    /// The `Retry-After` header, as sent.
    #[cfg_attr(not(feature = "clover"), allow(dead_code))]
    pub retry_after: Option<String>,
}

impl Answer {
    /// A `2xx`.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Sends `request` and reads the whole answer. Only a failure to reach the
/// point of sale or to read its answer is an error here.
pub(crate) async fn send(request: reqwest::RequestBuilder) -> Result<Answer, PosError> {
    let response = request.send().await.map_err(unreachable)?;
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body = response.text().await.map_err(unreachable)?;
    Ok(Answer {
        status,
        body,
        retry_after,
    })
}

fn unreachable(e: reqwest::Error) -> PosError {
    PosError::Unreachable(e.to_string())
}

/// `body` read as `T`, unknown fields passed over.
pub(crate) fn read<T: DeserializeOwned>(pos: PointOfSale, body: &str) -> Result<T, PosError> {
    serde_json::from_str(body).map_err(|e| {
        PosError::Unreadable(format!(
            "{}'s answer couldn't be read: {e}",
            pos.display_name()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components_are_encoded_as_okhttp_did() {
        assert_eq!(
            encode("PAYMENTS_READ ORDERS_READ"),
            "PAYMENTS_READ%20ORDERS_READ"
        );
        assert_eq!(
            encode("https://till.monokulo.com/square"),
            "https%3A%2F%2Ftill.monokulo.com%2Fsquare"
        );
        assert_eq!(encode("createdTime>=1"), "createdTime%3E%3D1");
        assert_eq!(encode("é"), "%C3%A9");
        assert_eq!(
            url("http://h/", &["v3", "a/b"], &[("f", "x y"), ("f", "z")]),
            "http://h/v3/a%2Fb?f=x%20y&f=z"
        );
    }
}
