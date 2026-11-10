//! Monero payment URIs: `monero:<address>?tx_amount=<XMR>` (Android:
//! `PaymentUri` in `core/Order.kt`), and [`PaymentUri`]: such a URI as a
//! code carries it, with or without the receipt.

use std::fmt;

use serde::{Deserialize, Serialize};

const PICONERO_PER_XMR: u64 = 1_000_000_000_000;

/// `piconero` as XMR, without trailing zeros: 80370584829 is
/// "0.080370584829", and 2000000000000 is "2".
pub fn xmr(piconero: u64) -> String {
    let whole = piconero / PICONERO_PER_XMR;
    let frac = format!("{:012}", piconero % PICONERO_PER_XMR);
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{frac}")
    }
}

/// The plain payment URI for `piconero` to `address`, which a receipt is
/// appended to.
pub fn build(address: &str, piconero: u64) -> String {
    format!("monero:{address}?tx_amount={}", xmr(piconero))
}

/// A payment URI as a sale's code carries it: the plain one an order gives
/// ([`build`]), or that with the receipt in its `orf` parameter. Serialized
/// as the bare string.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PaymentUri(String);

impl PaymentUri {
    pub fn new(uri: impl Into<String>) -> PaymentUri {
        PaymentUri(uri.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    /// The URI without its receipt: only the `orf` parameter taken out
    /// ([`orf_codec::query::remove`]), every other parameter (`tx_amount`)
    /// and the fragment kept.
    pub fn plain(&self) -> PaymentUri {
        PaymentUri(orf_codec::query::remove(&self.0))
    }
}

impl fmt::Display for PaymentUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for PaymentUri {
    fn from(uri: &str) -> PaymentUri {
        PaymentUri(uri.to_owned())
    }
}

impl From<String> for PaymentUri {
    fn from(uri: String) -> PaymentUri {
        PaymentUri(uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_takes_out_only_the_receipt() {
        let plain = |uri: &str| PaymentUri::from(uri).plain().into_string();
        assert_eq!(
            plain("monero:8Bx?orf=0123&tx_amount=1"),
            "monero:8Bx?tx_amount=1"
        );
        assert_eq!(
            plain("monero:8Bx?tx_amount=1&orf=0123"),
            "monero:8Bx?tx_amount=1"
        );
        assert_eq!(plain("monero:8Bx?tx_amount=1"), "monero:8Bx?tx_amount=1");
        assert_eq!(plain("monero:8Bx?orf=0123#x"), "monero:8Bx#x");
    }
}
