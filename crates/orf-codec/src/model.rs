//! The Open Receipt Format 0.1 document, as its JSON Schema describes it
//! (<https://github.com/openreceiptformat/orf-spec>, `schema/orf-0.1.json`).
//!
//! Unknown fields are refused rather than dropped, so a document that decodes
//! is exactly the one that was encoded. Free-form data belongs in
//! `extensions`, which is carried as is.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub orf_version: String,
    pub receipt: ReceiptInfo,
    pub merchant: Merchant,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer: Option<Customer>,
    pub line_items: Vec<LineItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax: Option<Tax>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment: Option<Payment>,
    /// In major units of `currency`, like every amount here.
    pub total: f64,
    /// ISO 4217, e.g. "USD".
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust: Option<Trust>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Map<String, Value>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReceiptInfo {
    pub receipt_id: String,
    /// RFC 3339, whole seconds, e.g. "2024-12-28T09:15:30-08:00".
    pub issue_timestamp: String,
    pub receipt_type: ReceiptType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

impl ReceiptInfo {
    /// When it was issued, read as the codec reads it (RFC 3339 with any
    /// offset, any fraction of a second dropped); `None` when it isn't a
    /// timestamp.
    pub fn issued(&self) -> Option<jiff::Timestamp> {
        let (secs, _) = crate::codec::parse_timestamp(&self.issue_timestamp)?;
        jiff::Timestamp::from_second(secs).ok()
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptType {
    Sale,
    Refund,
    Void,
    Proforma,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Issued,
    Voided,
    Reissued,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Merchant {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merchant_id: Option<String>,
    pub legal_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// ISO 3166-1 alpha-2, e.g. "US".
    pub country_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_identifiers: Option<Vec<TaxIdentifier>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Customer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legal_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_identifiers: Option<Vec<TaxIdentifier>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaxIdentifier {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LineItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_item_id: Option<String>,
    pub description: String,
    pub quantity: f64,
    pub unit_price: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_amount: Option<f64>,
    pub line_total: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tax {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_lines: Option<Vec<TaxLine>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaxLine {
    /// A fraction: 0.08875 is 8.875%.
    pub rate: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_amount: Option<f64>,
    pub tax_amount: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jurisdiction: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Payment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_code: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Trust {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_algorithm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_by: Option<String>,
}
