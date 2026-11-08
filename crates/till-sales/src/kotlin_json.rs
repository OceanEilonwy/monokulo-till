//! The saved shift's JSON as the Android app writes it: kotlinx.serialization
//! with `explicitNulls = false` and its default of leaving out a property
//! that has its default value, fields in the Kotlin classes' order, and
//! doubles as Java's `Double.toString` writes them. Android keeps writing
//! the shift until phase 5 (docs/decisions/core.md 2), so what either side
//! writes must read in the other, and a shift Rust saves is the same bytes
//! Kotlin would have saved.

use serde::ser::Error as _;
use serde::{Serialize, Serializer};
use serde_json::value::RawValue;
use serde_json::{Map, Value};
use till_pos::{FoundSale, PointOfSale};

use crate::TillSale;

/// A shift's sales as the JSON the Android app saves (`shift.sales` in its
/// store), and back. Fields neither side knows are ignored when reading,
/// as the app's `ignoreUnknownKeys` did.
pub mod shift_json {
    use super::TillSale;

    /// `sales` as the Kotlin app writes them.
    pub fn encode(sales: &[TillSale]) -> String {
        serde_json::to_string(sales).expect("a shift's sales always serialise")
    }

    /// The sales in `saved`, a shift either side wrote.
    pub fn decode(saved: &str) -> Result<Vec<TillSale>, serde_json::Error> {
        serde_json::from_str(saved)
    }
}

/// `value` as Java's `Double.toString` writes it, which is what kotlinx
/// writes for a `Double`: plain decimals with at least one digit after the
/// point from 10⁻³ up to 10⁷ (`2.0`, `0.08875`), else computerized
/// scientific notation (`1.0E7`, `5.0E-4`); the digits are the shortest
/// that read back as the same double. `None` for NaN and the infinities,
/// which kotlinx refuses to write.
pub(crate) fn java_double(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(
            if value.is_sign_negative() {
                "-0.0"
            } else {
                "0.0"
            }
            .into(),
        );
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let abs = value.abs();
    // Rust's `{:e}` gives the shortest digits that round-trip: "1.388e1".
    let scientific = format!("{abs:e}");
    let (mantissa, exponent) = scientific.split_once('e')?;
    let exponent: i32 = exponent.parse().ok()?;
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    if (1e-3..1e7).contains(&abs) {
        if exponent >= 0 {
            let whole_len = exponent as usize + 1;
            let (whole, fraction) = if digits.len() > whole_len {
                (
                    digits[..whole_len].to_owned(),
                    digits[whole_len..].to_owned(),
                )
            } else {
                (format!("{digits:0<whole_len$}"), String::new())
            };
            let fraction = if fraction.is_empty() {
                "0".into()
            } else {
                fraction
            };
            Some(format!("{sign}{whole}.{fraction}"))
        } else {
            let zeros = "0".repeat((-exponent - 1) as usize);
            Some(format!("{sign}0.{zeros}{digits}"))
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        Some(format!("{sign}{first}.{rest}E{exponent}"))
    }
}

/// Writes `value` as Java does ([`java_double`]).
fn double<S: Serializer>(value: &f64, s: S) -> Result<S::Ok, S::Error> {
    match java_double(*value) {
        Some(text) => RawValue::from_string(text)
            .map_err(S::Error::custom)?
            .serialize(s),
        None => s.serialize_f64(*value),
    }
}

/// A JSON value with its non-integer numbers written as Java does: a
/// receipt's `extensions`, which Kotlin built from doubles.
struct JavaValue<'a>(&'a Value);

impl Serialize for JavaValue<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        match self.0 {
            Value::Number(n) if n.is_f64() => double(&n.as_f64().unwrap_or(f64::NAN), s),
            Value::Array(items) => {
                let mut seq = s.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(&JavaValue(item))?;
                }
                seq.end()
            }
            Value::Object(fields) => JavaMap(fields).serialize(s),
            other => other.serialize(s),
        }
    }
}

/// A JSON object written as [`JavaValue`] writes its values.
struct JavaMap<'a>(&'a Map<String, Value>);

impl Serialize for JavaMap<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (key, value) in self.0 {
            map.serialize_entry(key, &JavaValue(value))?;
        }
        map.end()
    }
}

/// A [`FoundSale`] as the Kotlin `FoundSale` wrote it in a saved shift:
/// its point of sale left out when it is Square, the property's default.
/// (till-pos always writes it; both read either.)
pub(crate) fn found_sale<S: Serializer>(
    found: &Option<FoundSale>,
    s: S,
) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Out<'a> {
        id: &'a str,
        amount_minor: i64,
        currency: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        reference: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        order_id: Option<&'a str>,
        created_at: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        device_id: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        point_of_sale: Option<&'static str>,
    }
    let Some(found) = found else {
        return s.serialize_none();
    };
    Out {
        id: &found.id,
        amount_minor: found.amount_minor,
        currency: &found.currency,
        reference: found.reference.as_deref(),
        order_id: found.order_id.as_deref(),
        created_at: &found.created_at,
        device_id: found.device_id.as_deref(),
        title: found.title.as_deref(),
        point_of_sale: (found.point_of_sale != PointOfSale::Square)
            .then(|| found.point_of_sale.display_name()),
    }
    .serialize(s)
}

/// An [`orf_codec::Receipt`] as the Kotlin `receipt.Receipt` wrote it in a
/// saved shift. The Kotlin model has only the parts a till fills in, so
/// the parts it lacks (a customer, trust, ids, net amounts, a tax scheme,
/// a status) are left out; till-pos never fills them. A tax with no lines
/// is written with an empty list, and a payment with no method not at all,
/// as the Kotlin model can't hold either.
pub(crate) mod receipt {
    use orf_codec::{LineItem, Merchant, Payment, Receipt, ReceiptInfo, ReceiptType, Tax, TaxLine};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde_json::{Map, Value};

    use super::{JavaMap, double};

    const ORF_VERSION: &str = "0.1.0";

    #[derive(Serialize)]
    struct Out<'a> {
        #[serde(skip_serializing_if = "is_orf_version")]
        orf_version: &'a str,
        receipt: InfoOut<'a>,
        merchant: MerchantOut<'a>,
        line_items: Vec<LineOut<'a>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tax: Option<TaxOut>,
        #[serde(skip_serializing_if = "Option::is_none")]
        payment: Option<PaymentOut<'a>>,
        #[serde(serialize_with = "double")]
        total: f64,
        currency: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        extensions: Option<JavaMap<'a>>,
    }

    fn is_orf_version(version: &&str) -> bool {
        *version == ORF_VERSION
    }

    #[derive(Serialize)]
    struct InfoOut<'a> {
        receipt_id: &'a str,
        issue_timestamp: &'a str,
        #[serde(skip_serializing_if = "is_sale")]
        receipt_type: &'static str,
    }

    fn is_sale(kind: &&str) -> bool {
        *kind == "sale"
    }

    #[derive(Serialize)]
    struct MerchantOut<'a> {
        legal_name: &'a str,
        country_code: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        display_name: Option<&'a str>,
    }

    #[derive(Serialize)]
    struct LineOut<'a> {
        description: &'a str,
        #[serde(serialize_with = "double")]
        quantity: f64,
        #[serde(serialize_with = "double")]
        unit_price: f64,
        #[serde(serialize_with = "double")]
        line_total: f64,
    }

    #[derive(Serialize)]
    struct TaxOut {
        tax_lines: Vec<TaxLineOut>,
    }

    #[derive(Serialize)]
    struct TaxLineOut {
        #[serde(serialize_with = "double")]
        rate: f64,
        #[serde(serialize_with = "double")]
        tax_amount: f64,
    }

    #[derive(Serialize)]
    struct PaymentOut<'a> {
        payment_method: &'a str,
    }

    pub(crate) fn serialize<S: Serializer>(
        receipt: &Option<Receipt>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let Some(r) = receipt else {
            return s.serialize_none();
        };
        Out {
            orf_version: &r.orf_version,
            receipt: InfoOut {
                receipt_id: &r.receipt.receipt_id,
                issue_timestamp: &r.receipt.issue_timestamp,
                receipt_type: match r.receipt.receipt_type {
                    ReceiptType::Sale => "sale",
                    ReceiptType::Refund => "refund",
                    ReceiptType::Void => "void",
                    ReceiptType::Proforma => "proforma",
                },
            },
            merchant: MerchantOut {
                legal_name: &r.merchant.legal_name,
                country_code: &r.merchant.country_code,
                display_name: r.merchant.display_name.as_deref(),
            },
            line_items: r
                .line_items
                .iter()
                .map(|line| LineOut {
                    description: &line.description,
                    quantity: line.quantity,
                    unit_price: line.unit_price,
                    line_total: line.line_total,
                })
                .collect(),
            tax: r.tax.as_ref().map(|tax| TaxOut {
                tax_lines: tax
                    .tax_lines
                    .iter()
                    .flatten()
                    .map(|line| TaxLineOut {
                        rate: line.rate,
                        tax_amount: line.tax_amount,
                    })
                    .collect(),
            }),
            payment: r.payment.as_ref().and_then(|payment| {
                payment
                    .payment_method
                    .as_deref()
                    .map(|payment_method| PaymentOut { payment_method })
            }),
            total: r.total,
            currency: &r.currency,
            extensions: r.extensions.as_ref().map(JavaMap),
        }
        .serialize(s)
    }

    #[derive(Deserialize)]
    struct In {
        #[serde(default = "orf_version")]
        orf_version: String,
        receipt: InfoIn,
        merchant: MerchantIn,
        line_items: Vec<LineIn>,
        #[serde(default)]
        tax: Option<TaxIn>,
        #[serde(default)]
        payment: Option<PaymentIn>,
        total: f64,
        currency: String,
        #[serde(default)]
        extensions: Option<Map<String, Value>>,
    }

    fn orf_version() -> String {
        ORF_VERSION.into()
    }

    #[derive(Deserialize)]
    struct InfoIn {
        receipt_id: String,
        issue_timestamp: String,
        #[serde(default = "sale")]
        receipt_type: ReceiptType,
    }

    fn sale() -> ReceiptType {
        ReceiptType::Sale
    }

    #[derive(Deserialize)]
    struct MerchantIn {
        legal_name: String,
        country_code: String,
        #[serde(default)]
        display_name: Option<String>,
    }

    #[derive(Deserialize)]
    struct LineIn {
        description: String,
        quantity: f64,
        unit_price: f64,
        line_total: f64,
    }

    #[derive(Deserialize)]
    struct TaxIn {
        tax_lines: Vec<TaxLineIn>,
    }

    #[derive(Deserialize)]
    struct TaxLineIn {
        rate: f64,
        tax_amount: f64,
    }

    #[derive(Deserialize)]
    struct PaymentIn {
        payment_method: String,
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Receipt>, D::Error> {
        let Some(r) = Option::<In>::deserialize(d)? else {
            return Ok(None);
        };
        Ok(Some(Receipt {
            orf_version: r.orf_version,
            receipt: ReceiptInfo {
                receipt_id: r.receipt.receipt_id,
                issue_timestamp: r.receipt.issue_timestamp,
                receipt_type: r.receipt.receipt_type,
                status: None,
            },
            merchant: Merchant {
                merchant_id: None,
                legal_name: r.merchant.legal_name,
                display_name: r.merchant.display_name,
                country_code: r.merchant.country_code,
                tax_identifiers: None,
            },
            customer: None,
            line_items: r
                .line_items
                .into_iter()
                .map(|line| LineItem {
                    line_item_id: None,
                    description: line.description,
                    quantity: line.quantity,
                    unit_price: line.unit_price,
                    net_amount: None,
                    line_total: line.line_total,
                })
                .collect(),
            tax: r.tax.map(|tax| Tax {
                tax_scheme: None,
                tax_lines: Some(
                    tax.tax_lines
                        .into_iter()
                        .map(|line| TaxLine {
                            rate: line.rate,
                            base_amount: None,
                            tax_amount: line.tax_amount,
                            jurisdiction: None,
                        })
                        .collect(),
                ),
            }),
            payment: r.payment.map(|payment| Payment {
                payment_method: Some(payment.payment_method),
                provider: None,
                authorization_code: None,
            }),
            total: r.total,
            currency: r.currency,
            trust: None,
            extensions: r.extensions,
        }))
    }
}

#[cfg(test)]
mod tests {
    use monokulo_client::SalePhase;

    use super::*;
    use crate::Standing;

    /// What the Android app saves for TillTest's sale, made by this till and
    /// closed: its found sale (Square's, so no `pointOfSale`) and receipt
    /// (version and type at their defaults, so left out).
    const OURS: &str = concat!(
        r#"{"orderId":"ord_1","amount":"13.88","currency":"USD","reference":"bP9m","#,
        r#""code":{"uri":"monero:8Bx?tx_amount=0.08&orf=abc","hasReceipt":true},"#,
        r#""phase":"Waiting","expiresAt":1761312000,"xmr":"0.08","address":"8Bx","#,
        r#""found":{"id":"pay_monero","amountMinor":1388,"currency":"USD","reference":"bP9m","orderId":"ord_sq_1","createdAt":"2026-10-22T16:07:00Z","deviceId":"dev_front"},"#,
        r#""receipt":{"receipt":{"receipt_id":"bP9m","issue_timestamp":"2026-10-22T09:07:00-07:00"},"merchant":{"legal_name":"The Bean Room","country_code":"US"},"#,
        r#""line_items":[{"description":"Flat white","quantity":2.0,"unit_price":4.5,"line_total":9.0},{"description":"Croissant","quantity":1.0,"unit_price":3.75,"line_total":3.75}],"#,
        r#""tax":{"tax_lines":[{"rate":0.08875,"tax_amount":1.13}]},"total":13.88,"currency":"USD"},"#,
        r#""url":"https://pay.example/api/v1/orders/ord_1","createdAt":1761149220,"closedAt":50}"#
    );

    /// Another till's sale, paid and settled, with its count.
    const THEIRS: &str = concat!(
        r#"{"orderId":"ord_2","amount":"5.00","currency":"USD","#,
        r#""code":{"uri":"monero:8Cy?tx_amount=0.03","hasReceipt":false},"#,
        r#""phase":"Paid","expiresAt":99,"xmr":"0.03","address":"8Cy","deviceName":"Back","#,
        r#""url":"https://pay.example/monokulo/api/v1/orders/ord_2","settled":true,"createdAt":300,"#,
        r#""seenAt":310,"paidAt":320,"confirmations":10,"confirmationsRequired":10}"#
    );

    /// A Clover sale told to refund there, its receipt with a display name,
    /// a payment and a discount, and a code that couldn't take it.
    const CLOVER: &str = concat!(
        r#"{"orderId":"ord_3","amount":"24.50","currency":"EUR","reference":"T4","#,
        r#""code":{"uri":"monero:8Dz?tx_amount=0.1","hasReceipt":false,"why":"the URI already has an orf parameter"},"#,
        r#""phase":"Expired","expiresAt":1761312000,"xmr":"0.1","address":"8Dz","#,
        r#""found":{"id":"PAY_1","amountMinor":2450,"currency":"EUR","createdAt":"2026-10-22T16:07:00Z","title":"Table 4","pointOfSale":"Clover"},"#,
        r#""receipt":{"receipt":{"receipt_id":"T4","issue_timestamp":"2026-10-22T18:07:00+02:00"},"merchant":{"legal_name":"Café Lumen","country_code":"FR","display_name":"Lumen"},"#,
        r#""line_items":[{"description":"Galette","quantity":1.0,"unit_price":26.0,"line_total":26.0}],"#,
        r#""payment":{"payment_method":"monero"},"total":24.5,"currency":"EUR","extensions":{"discount":{"amount":-1.5}}},"#,
        r#""customerName":"Ana López","createdAt":1761149220,"endedAt":1761312000,"closedAt":1761312005,"refundPendingAt":1761312010}"#
    );

    #[test]
    fn a_shift_the_android_app_saved_reads_and_writes_back_byte_for_byte() {
        let saved = format!("[{OURS},{THEIRS},{CLOVER}]");
        let sales = shift_json::decode(&saved).unwrap();
        let [ours, theirs, clover] = &sales[..] else {
            panic!("three sales: {sales:?}")
        };
        assert_eq!(ours.standing(), Standing::Closed);
        assert_eq!(
            ours.found.as_ref().unwrap().point_of_sale,
            PointOfSale::Square
        );
        let receipt = ours.receipt.as_ref().unwrap();
        assert_eq!(receipt.orf_version, "0.1.0");
        assert_eq!(receipt.receipt.receipt_type, orf_codec::ReceiptType::Sale);
        assert_eq!(receipt.line_items[0].quantity, 2.0);
        assert_eq!(theirs.standing(), Standing::PAID);
        assert_eq!(theirs.phase, SalePhase::Paid);
        assert_eq!(theirs.confirmations, Some(10));
        assert_eq!(clover.standing(), Standing::RefundInPos);
        assert_eq!(
            clover.found.as_ref().unwrap().point_of_sale,
            PointOfSale::Clover
        );
        assert_eq!(clover.customer_name.as_deref(), Some("Ana López"));
        assert_eq!(shift_json::encode(&sales), saved);
    }

    #[test]
    fn fields_neither_side_knows_are_ignored_and_till_pos_found_sales_read_too() {
        let saved = OURS
            .replacen(
                r#""orderId":"ord_1","#,
                r#""orderId":"ord_1","later":{"a":1},"#,
                1,
            )
            .replace(
                r#""deviceId":"dev_front"}"#,
                r#""deviceId":"dev_front","pointOfSale":"Square"}"#,
            );
        let sales = shift_json::decode(&format!("[{saved}]")).unwrap();
        assert_eq!(shift_json::encode(&sales), format!("[{OURS}]"));
    }

    #[test]
    fn doubles_are_written_as_java_writes_them() {
        for (value, java) in [
            (2.0, "2.0"),
            (4.5, "4.5"),
            (13.88, "13.88"),
            (0.08875, "0.08875"),
            (100.0, "100.0"),
            (-1.5, "-1.5"),
            (0.001, "0.001"),
            (9_999_999.0, "9999999.0"),
            (1e7, "1.0E7"),
            (12_345_678.9, "1.23456789E7"),
            (0.0005, "5.0E-4"),
            (1.25e-5, "1.25E-5"),
            (0.0, "0.0"),
            (-0.0, "-0.0"),
        ] {
            assert_eq!(java_double(value).as_deref(), Some(java), "{value}");
        }
        assert_eq!(java_double(f64::NAN), None);
        let big = OURS.replace(r#""total":13.88"#, r#""total":1.0E7"#);
        let sales = shift_json::decode(&format!("[{big}]")).unwrap();
        assert_eq!(sales[0].receipt.as_ref().unwrap().total, 1e7);
        assert_eq!(shift_json::encode(&sales), format!("[{big}]"));
    }
}
