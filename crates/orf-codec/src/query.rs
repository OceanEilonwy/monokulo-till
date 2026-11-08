//! The receipt as one URI query parameter, `orf=<digits>`.
//!
//! This knows nothing about the URI it goes in. The app adds the parameter
//! to its own payment URI, and a reader that doesn't know it ignores it.

use crate::{DecodeError, EncodeError, Receipt};

/// The query parameter's name.
pub const PARAM: &str = "orf";

/// `orf=<digits>`, to append to a URI's query with `?` or `&`. Digits need
/// no percent-encoding.
pub fn to_param(receipt: &Receipt) -> Result<String, EncodeError> {
    Ok(format!("{PARAM}={}", crate::encode_text(receipt)?))
}

/// `uri` with the receipt added as its `orf` parameter: at the end of the
/// query, before any `#fragment`, with `?` or `&` as needed. A URI that
/// already has an `orf` parameter is refused rather than given a second one.
pub fn append(uri: &str, receipt: &Receipt) -> Result<String, EncodeError> {
    if find(uri).is_some() {
        return Err(EncodeError::HasReceipt);
    }
    let (base, fragment) = match uri.split_once('#') {
        Some((base, fragment)) => (base, Some(fragment)),
        None => (uri, None),
    };
    let sep = match base.split_once('?') {
        None => "?",
        Some((_, "")) => "",
        Some((_, q)) if q.ends_with('&') => "",
        Some(_) => "&",
    };
    let mut out = format!("{base}{sep}{}", to_param(receipt)?);
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    Ok(out)
}

/// The value of the first `orf` parameter in `uri`'s query, if there is one.
/// Only the query is read: the part after the first `?` and before any `#`.
pub fn find(uri: &str) -> Option<&str> {
    let query = uri.split_once('?')?.1;
    let query = query.split_once('#').map_or(query, |(q, _)| q);
    query
        .split('&')
        .find_map(|pair| match pair.split_once('=') {
            Some((PARAM, value)) => Some(value),
            _ => None,
        })
}

/// The receipt in `uri`'s `orf` parameter: `None` when there isn't one, an
/// error when there is one that doesn't decode.
pub fn from_uri(uri: &str) -> Result<Option<Receipt>, DecodeError> {
    find(uri).map(crate::decode_text).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> Receipt {
        serde_json::from_str(r#"{
          "orf_version": "0.1.0",
          "receipt": { "receipt_id": "7", "issue_timestamp": "2026-10-22T12:00:00Z", "receipt_type": "sale" },
          "merchant": { "legal_name": "Corner Shop", "country_code": "US" },
          "line_items": [{ "description": "Tea", "quantity": 1, "unit_price": 2.5, "line_total": 2.5 }],
          "total": 2.5,
          "currency": "USD"
        }"#)
        .unwrap()
    }

    #[test]
    fn appends_with_the_right_separator_and_reads_back() {
        let r = receipt();
        let param = to_param(&r).unwrap();
        for (uri, want) in [
            ("pay:abc", format!("pay:abc?{param}")),
            ("pay:abc?", format!("pay:abc?{param}")),
            ("pay:abc?amount=1", format!("pay:abc?amount=1&{param}")),
            ("pay:abc?amount=1&", format!("pay:abc?amount=1&{param}")),
            (
                "https://x.test/p?a=1#top",
                format!("https://x.test/p?a=1&{param}#top"),
            ),
            (
                "https://x.test/p#top",
                format!("https://x.test/p?{param}#top"),
            ),
        ] {
            let got = append(uri, &r).unwrap();
            assert_eq!(got, want, "{uri}");
            assert_eq!(from_uri(&got).unwrap(), Some(r.clone()), "{uri}");
        }
    }

    #[test]
    fn a_uri_with_a_receipt_already_is_refused() {
        assert!(matches!(
            append("pay:abc?orf=123", &receipt()),
            Err(EncodeError::HasReceipt)
        ));
    }

    #[test]
    fn finds_the_parameter_in_any_uri() {
        assert_eq!(find("pay:abc?amount=1&orf=0123&x=y"), Some("0123"));
        assert_eq!(find("https://example.com/r?orf=42#orf=9"), Some("42"));
        assert_eq!(find("pay:abc?xorf=1&orf_=2"), None);
        assert_eq!(find("pay:abc#orf=1"), None);
        assert_eq!(find("pay:abc"), None);
    }

    #[test]
    fn a_damaged_parameter_is_an_error_and_a_missing_one_is_none() {
        assert!(from_uri("pay:abc?amount=1").unwrap().is_none());
        assert!(from_uri("pay:abc?orf=12x").is_err());
    }
}
