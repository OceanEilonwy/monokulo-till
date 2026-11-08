//! The compact form of an ORF receipt.
//!
//! ```text
//! payload = header | LEB128(len(numbers)) | numbers | zstd(text, dictionary)
//! header  = format (high 4 bits) | dictionary id (low 4 bits)
//! ```
//!
//! `numbers` holds every number, enum and presence flag, bit-packed with
//! Exp-Golomb codes. `text` holds every string, one per line, in the order
//! the numbers are written; it is compressed on its own with a zstd
//! dictionary in magicless mode (no magic number, checksum, content size or
//! dictionary id: the header names the dictionary).
//!
//! Values that can be worked out from the rest are not sent: a line total of
//! quantity × unit price, a tax amount of base × rate, a total of line totals
//! plus tax plus tip, sequential line item ids "1", "2", …. Each has a flag
//! for when the receipt says otherwise, so every document round-trips exactly.

use std::io;
use std::sync::OnceLock;

use serde_json::{Map, Number, Value};
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};
use zstd::dict::{DecoderDictionary, EncoderDictionary};
use zstd::zstd_safe::{CParameter, DParameter, FrameFormat};

use crate::bits::{Reader, Writer};
use crate::model::*;
use crate::{DecodeError, EncodeError};

/// The format number in the header's high four bits.
pub const FORMAT: u8 = 1;
/// Seconds at 2025-01-01T00:00:00Z: timestamps are stored relative to it.
const EPOCH_2025: i64 = 1_735_689_600;
const CURRENCIES: [&str; 3] = ["USD", "EUR", "GBP"];
const SEP: char = '\n';
const DATETIME: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");
const OFFSET: &[FormatItem<'static>] =
    format_description!("[offset_hour sign:mandatory]:[offset_minute]");

/// zstd's compression level. Above 19 makes no difference to receipts, but
/// costs nothing once the dictionary is prepared.
const LEVEL: i32 = 22;

/// An encoder and decoder bound to one compression dictionary. Making one
/// prepares the dictionary, which takes about 2 ms; encoding and decoding
/// with it then take microseconds, so make it once and keep it.
pub struct Codec<'d> {
    dict_id: u8,
    encoder: EncoderDictionary<'d>,
    decoder: DecoderDictionary<'d>,
}

/// The dictionary shipped with this crate: trained on public domain menus
/// (NYPL), CC0 labels (Wikidata) and generated item names.
pub const DEFAULT_DICTIONARY: &[u8] = include_bytes!("../dict/receipts-open-64k.zdict");

impl Codec<'static> {
    /// The codec for the default dictionary, prepared on first use and shared.
    pub fn shared() -> &'static Codec<'static> {
        static SHARED: OnceLock<Codec<'static>> = OnceLock::new();
        SHARED.get_or_init(|| Codec::prepare(0, DEFAULT_DICTIONARY))
    }
}

impl<'d> Codec<'d> {
    /// A codec with another dictionary, named by `id` (0 to 15) in every
    /// payload. Id 0 is the default dictionary; use others only for tests
    /// and experiments until a format version adds them.
    pub fn with_dictionary(id: u8, dict: &'d [u8]) -> Self {
        assert!(id < 16, "dictionary ids are four bits");
        Codec::prepare(id, dict)
    }

    fn prepare(dict_id: u8, dict: &'d [u8]) -> Self {
        Codec {
            dict_id,
            encoder: EncoderDictionary::new(dict, LEVEL),
            decoder: DecoderDictionary::new(dict),
        }
    }

    pub fn encode(&self, r: &Receipt) -> Result<Vec<u8>, EncodeError> {
        let mut e = Enc {
            bits: Writer::default(),
            text: Vec::new(),
        };
        e.receipt(r)?;
        let numbers = e.bits.finish();
        let text = e.text.join(&SEP.to_string());
        let mut out = vec![(FORMAT << 4) | self.dict_id];
        leb128(&mut out, numbers.len());
        out.extend_from_slice(&numbers);
        out.extend_from_slice(
            &compress(text.as_bytes(), &self.encoder).map_err(EncodeError::Compress)?,
        );
        Ok(out)
    }

    pub fn decode(&self, payload: &[u8]) -> Result<Receipt, DecodeError> {
        let (&header, rest) = payload.split_first().ok_or(DecodeError::Truncated)?;
        if header >> 4 != FORMAT {
            return Err(DecodeError::UnknownFormat(header >> 4));
        }
        if header & 0x0f != self.dict_id {
            return Err(DecodeError::UnknownDictionary(header & 0x0f));
        }
        let (len, used) = read_leb128(rest)?;
        let numbers = rest.get(used..used + len).ok_or(DecodeError::Truncated)?;
        let text =
            decompress(&rest[used + len..], &self.decoder).map_err(DecodeError::Decompress)?;
        let text = String::from_utf8(text).map_err(|_| DecodeError::Corrupt("text isn't UTF-8"))?;
        let mut d = Dec {
            bits: Reader::new(numbers),
            text: text.split(SEP),
        };
        let r = d.receipt()?;
        if d.text.next().is_some() {
            return Err(DecodeError::Corrupt("text left over"));
        }
        Ok(r)
    }
}

fn compress(text: &[u8], dict: &EncoderDictionary) -> io::Result<Vec<u8>> {
    let mut c = zstd::bulk::Compressor::with_prepared_dictionary(dict)?;
    for p in [
        CParameter::Format(FrameFormat::Magicless),
        CParameter::ChecksumFlag(false),
        CParameter::ContentSizeFlag(false),
        CParameter::DictIdFlag(false),
    ] {
        c.set_parameter(p)?;
    }
    c.compress(text)
}

/// Receipts are small; this caps what a malicious payload can make us allocate.
const MAX_TEXT: usize = 1 << 20;

fn decompress(data: &[u8], dict: &DecoderDictionary) -> io::Result<Vec<u8>> {
    let mut d = zstd::bulk::Decompressor::with_prepared_dictionary(dict)?;
    d.set_parameter(DParameter::Format(FrameFormat::Magicless))?;
    d.decompress(data, MAX_TEXT)
}

fn leb128(out: &mut Vec<u8>, mut n: usize) {
    loop {
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn read_leb128(data: &[u8]) -> Result<(usize, usize), DecodeError> {
    let mut n = 0usize;
    for (i, &b) in data.iter().enumerate().take(4) {
        n |= ((b & 0x7f) as usize) << (7 * i);
        if b & 0x80 == 0 {
            return Ok((n, i + 1));
        }
    }
    Err(DecodeError::Truncated)
}

// ---- amounts ----------------------------------------------------------------

/// Digits after the decimal point in a currency's minor unit (ISO 4217).
fn exponent(currency: &str) -> u32 {
    match currency {
        "BIF" | "CLP" | "DJF" | "GNF" | "ISK" | "JPY" | "KMF" | "KRW" | "PYG" | "RWF" | "UGX"
        | "UYI" | "VND" | "VUV" | "XAF" | "XOF" | "XPF" => 0,
        "BHD" | "IQD" | "JOD" | "KWD" | "LYD" | "OMR" | "TND" => 3,
        _ => 2,
    }
}

/// `x` as an integer count of 10^-exp, only if that count gives back exactly `x`.
fn to_units(x: f64, exp: u32) -> Option<i64> {
    let scaled = (x * 10f64.powi(exp as i32)).round();
    if !scaled.is_finite() || scaled.abs() > 9.0e15 {
        return None;
    }
    let n = scaled as i64;
    (from_units(n, exp) == x).then_some(n)
}

fn from_units(n: i64, exp: u32) -> f64 {
    n as f64 / 10f64.powi(exp as i32)
}

/// n / d rounded half away from zero.
fn round_div(n: i128, d: i128) -> i64 {
    let q = (n.abs() + d / 2) / d;
    (if n < 0 { -q } else { q }) as i64
}

fn line_total(qty_milli: i64, price: i64) -> i64 {
    round_div(qty_milli as i128 * price as i128, 1000)
}

fn tax_amount(base: i64, rate_ppm: i64) -> i64 {
    round_div(base as i128 * rate_ppm as i128, 1_000_000)
}

// ---- encoding ---------------------------------------------------------------

struct Enc {
    bits: Writer,
    text: Vec<String>,
}

fn unsupported(field: &str, reason: &'static str) -> EncodeError {
    EncodeError::Unsupported {
        field: field.to_string(),
        reason,
    }
}

impl Enc {
    fn text(&mut self, field: &str, s: &str) -> Result<(), EncodeError> {
        if s.contains(SEP) {
            return Err(unsupported(field, "line breaks in text"));
        }
        self.text.push(s.to_string());
        Ok(())
    }

    fn opt_text(&mut self, field: &str, s: &Option<String>) -> Result<(), EncodeError> {
        self.bits.flag(s.is_some());
        match s {
            Some(s) => self.text(field, s),
            None => Ok(()),
        }
    }

    fn amount(&self, field: &str, x: f64, exp: u32) -> Result<i64, EncodeError> {
        to_units(x, exp).ok_or_else(|| EncodeError::Amount {
            field: field.to_string(),
            value: x,
        })
    }

    fn receipt(&mut self, r: &Receipt) -> Result<(), EncodeError> {
        if r.orf_version != "0.1.0" {
            return Err(unsupported("orf_version", "only ORF 0.1.0"));
        }
        let exp = exponent(&r.currency);

        // Receipt: type, status, time.
        self.bits.put(r.receipt.receipt_type as u64, 2);
        self.bits
            .put(r.receipt.status.map_or(0, |s| s as u64 + 1), 2);
        self.timestamp(&r.receipt.issue_timestamp)?;

        // Merchant, and the receipt id after its name: the dictionary was
        // trained on text that starts with the store, then the reference.
        self.letters("merchant.country_code", &r.merchant.country_code, 2)?;
        self.text("merchant.legal_name", &r.merchant.legal_name)?;
        self.text("receipt.receipt_id", &r.receipt.receipt_id)?;
        self.opt_text("merchant.merchant_id", &r.merchant.merchant_id)?;
        self.opt_text("merchant.display_name", &r.merchant.display_name)?;
        self.tax_ids("merchant.tax_identifiers", &r.merchant.tax_identifiers)?;

        // Currency: the common ones by index, any other by its letters.
        match CURRENCIES.iter().position(|c| *c == r.currency) {
            Some(i) => self.bits.put(i as u64, 2),
            None => {
                self.bits.put(3, 2);
                self.letters("currency", &r.currency, 3)?;
            }
        }

        // Customer.
        self.bits.flag(r.customer.is_some());
        if let Some(c) = &r.customer {
            self.opt_text("customer.customer_id", &c.customer_id)?;
            self.opt_text("customer.legal_name", &c.legal_name)?;
            self.tax_ids("customer.tax_identifiers", &c.tax_identifiers)?;
        }

        // Line items.
        let items = &r.line_items;
        self.bits.eg(items.len() as u64, 2);
        let sequential = items
            .iter()
            .enumerate()
            .all(|(i, it)| it.line_item_id.as_deref() == Some(&(i + 1).to_string()));
        let ids_mode = if items.iter().all(|it| it.line_item_id.is_none()) {
            0
        } else if sequential {
            1
        } else {
            2
        };
        self.bits.put(ids_mode, 2);
        let mut sum_totals = 0i64;
        for (i, it) in items.iter().enumerate() {
            let f = |name: &str| format!("line_items[{i}].{name}");
            if ids_mode == 2 {
                self.opt_text(&f("line_item_id"), &it.line_item_id)?;
            }
            self.text(&f("description"), &it.description)?;
            let qty = self.amount(&f("quantity"), it.quantity, 3)?;
            if qty == 1000 {
                self.bits.flag(false);
            } else {
                self.bits.flag(true);
                if qty % 1000 == 0 {
                    self.bits.flag(false);
                    self.bits.eg_signed(qty / 1000, 1);
                } else {
                    self.bits.flag(true);
                    self.bits.eg_signed(qty, 10);
                }
            }
            let price = self.amount(&f("unit_price"), it.unit_price, exp)?;
            self.bits.eg_signed(price, 8);
            let total = self.amount(&f("line_total"), it.line_total, exp)?;
            let expected = line_total(qty, price);
            let net = it
                .net_amount
                .map(|n| self.amount(&f("net_amount"), n, exp))
                .transpose()?;
            // Most lines are plain: total = quantity × price, no net amount. One bit.
            let plain = total == expected && net.is_none();
            self.bits.flag(!plain);
            if !plain {
                self.bits.flag(total != expected);
                if total != expected {
                    self.bits.eg_signed(total, 8);
                }
                match net {
                    None => self.bits.put(0, 2),
                    Some(n) if n == total => self.bits.put(1, 2),
                    Some(n) => {
                        self.bits.put(2, 2);
                        self.bits.eg_signed(n, 8);
                    }
                }
            }
            sum_totals += total;
        }

        // Tax.
        let mut sum_tax = 0i64;
        self.bits.flag(r.tax.is_some());
        if let Some(tax) = &r.tax {
            self.opt_text("tax.tax_scheme", &tax.tax_scheme)?;
            let lines = tax.tax_lines.as_deref();
            self.bits.eg(lines.map_or(0, |l| l.len() as u64 + 1), 0);
            for (i, line) in lines.unwrap_or_default().iter().enumerate() {
                let f = |name: &str| format!("tax.tax_lines[{i}].{name}");
                let ppm = to_units(line.rate, 6).filter(|p| *p >= 0).ok_or_else(|| {
                    EncodeError::Amount {
                        field: f("rate"),
                        value: line.rate,
                    }
                })?;
                self.bits.eg(ppm as u64, 12);
                let base = match line.base_amount {
                    None => {
                        self.bits.put(0, 2);
                        sum_totals
                    }
                    Some(b) => {
                        let b = self.amount(&f("base_amount"), b, exp)?;
                        if b == sum_totals {
                            self.bits.put(1, 2);
                        } else {
                            self.bits.put(2, 2);
                            self.bits.eg_signed(b, 8);
                        }
                        b
                    }
                };
                let amount = self.amount(&f("tax_amount"), line.tax_amount, exp)?;
                let expected = tax_amount(base, ppm);
                self.bits.flag(amount != expected);
                if amount != expected {
                    self.bits.eg_signed(amount, 6);
                }
                self.opt_text(&f("jurisdiction"), &line.jurisdiction)?;
                sum_tax += amount;
            }
        }

        // Payment and trust: all text.
        self.bits.flag(r.payment.is_some());
        if let Some(p) = &r.payment {
            self.opt_text("payment.payment_method", &p.payment_method)?;
            self.opt_text("payment.provider", &p.provider)?;
            self.opt_text("payment.authorization_code", &p.authorization_code)?;
        }
        self.bits.flag(r.trust.is_some());
        if let Some(t) = &r.trust {
            self.opt_text("trust.hash", &t.hash)?;
            self.opt_text("trust.hash_algorithm", &t.hash_algorithm)?;
            self.opt_text("trust.signature", &t.signature)?;
            self.opt_text("trust.signed_by", &t.signed_by)?;
        }

        // Extensions: a tip's amount as a number, anything else as JSON text.
        let (tip, rest) = split_tip(r.extensions.as_ref(), exp);
        self.bits.flag(tip.is_some());
        if let Some(t) = tip {
            self.bits.eg_signed(t, 9);
        }
        self.bits.put(
            match (&r.extensions, &rest) {
                (None, _) => 0,
                (Some(_), None) => 1,
                (Some(_), Some(_)) => 2,
            },
            2,
        );
        if let Some(rest) = rest {
            let json = serde_json::to_string(&Value::Object(rest)).expect("JSON values serialise");
            self.text("extensions", &json)?;
        }

        // Total.
        let total = self.amount("total", r.total, exp)?;
        let expected = sum_totals + sum_tax + tip.unwrap_or(0);
        self.bits.flag(total != expected);
        if total != expected {
            self.bits.eg_signed(total, 10);
        }
        Ok(())
    }

    fn timestamp(&mut self, s: &str) -> Result<(), EncodeError> {
        let field = "receipt.issue_timestamp";
        let t = OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
            .map_err(|_| unsupported(field, "not an RFC 3339 timestamp"))?;
        let zulu = s.ends_with('Z') || s.ends_with('z');
        if format_timestamp(t, zulu) != s {
            return Err(unsupported(
                field,
                "only whole seconds as YYYY-MM-DDTHH:MM:SS with Z or ±HH:MM",
            ));
        }
        // Minutes since 2025, then the seconds only when there are any.
        let since = t.unix_timestamp() - EPOCH_2025;
        self.bits.eg_signed(since.div_euclid(60), 20);
        let secs = since.rem_euclid(60);
        self.bits.flag(secs != 0);
        if secs != 0 {
            self.bits.put(secs as u64, 6);
        }
        let offset = t.offset().whole_minutes() as i64;
        self.bits.flag(offset == 0);
        if offset == 0 {
            self.bits.flag(zulu);
        } else {
            self.bits.eg_signed(offset, 5);
        }
        Ok(())
    }

    fn letters(&mut self, field: &str, s: &str, n: usize) -> Result<(), EncodeError> {
        if s.len() != n || !s.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(unsupported(field, "not an upper-case ISO code"));
        }
        s.bytes().for_each(|b| self.bits.put((b - b'A') as u64, 5));
        Ok(())
    }

    fn tax_ids(
        &mut self,
        field: &str,
        ids: &Option<Vec<TaxIdentifier>>,
    ) -> Result<(), EncodeError> {
        self.bits
            .eg(ids.as_ref().map_or(0, |v| v.len() as u64 + 1), 0);
        for (i, id) in ids.iter().flatten().enumerate() {
            self.opt_text(&format!("{field}[{i}].scheme"), &id.scheme)?;
            self.opt_text(&format!("{field}[{i}].value"), &id.value)?;
        }
        Ok(())
    }
}

/// The tip amount in minor units, when `extensions.tip.amount` is an exact
/// amount, and whatever remains of the extensions without it.
fn split_tip(
    ext: Option<&Map<String, Value>>,
    exp: u32,
) -> (Option<i64>, Option<Map<String, Value>>) {
    let Some(ext) = ext else { return (None, None) };
    let mut rest = ext.clone();
    // Only a JSON float: an integer written as 2 would come back as 2.0.
    let tip = rest
        .get("tip")
        .and_then(|t| t.get("amount"))
        .filter(|a| a.is_f64())
        .and_then(Value::as_f64)
        .and_then(|a| to_units(a, exp));
    if tip.is_some() {
        let t = rest
            .get_mut("tip")
            .and_then(Value::as_object_mut)
            .expect("checked above");
        t.remove("amount");
        if t.is_empty() {
            rest.remove("tip");
        }
    }
    (tip, (!rest.is_empty()).then_some(rest))
}

fn format_timestamp(t: OffsetDateTime, zulu: bool) -> String {
    let base = t.format(DATETIME).expect("valid format");
    if zulu && t.offset().is_utc() {
        format!("{base}Z")
    } else {
        format!("{base}{}", t.offset().format(OFFSET).expect("valid format"))
    }
}

// ---- decoding ---------------------------------------------------------------

struct Dec<'a> {
    bits: Reader<'a>,
    text: std::str::Split<'a, char>,
}

impl Dec<'_> {
    fn text(&mut self) -> Result<String, DecodeError> {
        self.text
            .next()
            .map(str::to_string)
            .ok_or(DecodeError::Corrupt("text missing"))
    }

    fn opt_text(&mut self) -> Result<Option<String>, DecodeError> {
        Ok(if self.bits.flag()? {
            Some(self.text()?)
        } else {
            None
        })
    }

    fn receipt(&mut self) -> Result<Receipt, DecodeError> {
        let receipt_type = match self.bits.get(2)? {
            0 => ReceiptType::Sale,
            1 => ReceiptType::Refund,
            2 => ReceiptType::Void,
            _ => ReceiptType::Proforma,
        };
        let status = match self.bits.get(2)? {
            0 => None,
            1 => Some(Status::Issued),
            2 => Some(Status::Voided),
            _ => Some(Status::Reissued),
        };
        let issue_timestamp = self.timestamp()?;

        let country_code = self.letters(2)?;
        let legal_name = self.text()?;
        let receipt_id = self.text()?;
        let merchant_id = self.opt_text()?;
        let display_name = self.opt_text()?;
        let tax_identifiers = self.tax_ids()?;

        let currency = match self.bits.get(2)? {
            3 => self.letters(3)?,
            i => CURRENCIES[i as usize].to_string(),
        };
        let exp = exponent(&currency);
        let money = |n: i64| from_units(n, exp);

        let customer = if self.bits.flag()? {
            Some(Customer {
                customer_id: self.opt_text()?,
                legal_name: self.opt_text()?,
                tax_identifiers: self.tax_ids()?,
            })
        } else {
            None
        };

        let n = self.bits.eg(2)? as usize;
        if n > 10_000 {
            return Err(DecodeError::Corrupt("too many line items"));
        }
        let ids_mode = self.bits.get(2)?;
        let mut line_items = Vec::with_capacity(n);
        let mut sum_totals = 0i64;
        for i in 0..n {
            let line_item_id = match ids_mode {
                0 => None,
                1 => Some((i + 1).to_string()),
                _ => self.opt_text()?,
            };
            let description = self.text()?;
            let qty = if !self.bits.flag()? {
                1000
            } else if !self.bits.flag()? {
                self.bits.eg_signed(1)? * 1000
            } else {
                self.bits.eg_signed(10)?
            };
            let price = self.bits.eg_signed(8)?;
            let (total, net_amount) = if !self.bits.flag()? {
                (line_total(qty, price), None)
            } else {
                let total = if self.bits.flag()? {
                    self.bits.eg_signed(8)?
                } else {
                    line_total(qty, price)
                };
                let net = match self.bits.get(2)? {
                    0 => None,
                    1 => Some(money(total)),
                    _ => Some(money(self.bits.eg_signed(8)?)),
                };
                (total, net)
            };
            sum_totals += total;
            line_items.push(LineItem {
                line_item_id,
                description,
                quantity: from_units(qty, 3),
                unit_price: money(price),
                net_amount,
                line_total: money(total),
            });
        }

        let mut sum_tax = 0i64;
        let tax = if self.bits.flag()? {
            let tax_scheme = self.opt_text()?;
            let count = self.bits.eg(0)?;
            let tax_lines = if count == 0 {
                None
            } else {
                let mut lines = Vec::new();
                for _ in 0..count - 1 {
                    let ppm = self.bits.eg(12)? as i64;
                    let (base, base_amount) = match self.bits.get(2)? {
                        0 => (sum_totals, None),
                        1 => (sum_totals, Some(money(sum_totals))),
                        _ => {
                            let b = self.bits.eg_signed(8)?;
                            (b, Some(money(b)))
                        }
                    };
                    let amount = if self.bits.flag()? {
                        self.bits.eg_signed(6)?
                    } else {
                        tax_amount(base, ppm)
                    };
                    sum_tax += amount;
                    lines.push(TaxLine {
                        rate: from_units(ppm, 6),
                        base_amount,
                        tax_amount: money(amount),
                        jurisdiction: self.opt_text()?,
                    });
                }
                Some(lines)
            };
            Some(Tax {
                tax_scheme,
                tax_lines,
            })
        } else {
            None
        };

        let payment = if self.bits.flag()? {
            Some(Payment {
                payment_method: self.opt_text()?,
                provider: self.opt_text()?,
                authorization_code: self.opt_text()?,
            })
        } else {
            None
        };
        let trust = if self.bits.flag()? {
            Some(Trust {
                hash: self.opt_text()?,
                hash_algorithm: self.opt_text()?,
                signature: self.opt_text()?,
                signed_by: self.opt_text()?,
            })
        } else {
            None
        };

        let tip = if self.bits.flag()? {
            Some(self.bits.eg_signed(9)?)
        } else {
            None
        };
        let mut extensions = match self.bits.get(2)? {
            0 => None,
            1 => Some(Map::new()),
            _ => match serde_json::from_str(&self.text()?).map_err(DecodeError::Json)? {
                Value::Object(m) => Some(m),
                _ => return Err(DecodeError::Corrupt("extensions aren't an object")),
            },
        };
        if let Some(t) = tip {
            let ext = extensions.get_or_insert_with(Map::new);
            let entry = ext
                .entry("tip")
                .or_insert_with(|| Value::Object(Map::new()));
            let obj = entry
                .as_object_mut()
                .ok_or(DecodeError::Corrupt("tip isn't an object"))?;
            let amount = Number::from_f64(money(t)).ok_or(DecodeError::Corrupt("tip amount"))?;
            obj.insert("amount".into(), Value::Number(amount));
        }

        let total = if self.bits.flag()? {
            self.bits.eg_signed(10)?
        } else {
            sum_totals + sum_tax + tip.unwrap_or(0)
        };

        Ok(Receipt {
            orf_version: "0.1.0".into(),
            receipt: ReceiptInfo {
                receipt_id,
                issue_timestamp,
                receipt_type,
                status,
            },
            merchant: Merchant {
                merchant_id,
                legal_name,
                display_name,
                country_code,
                tax_identifiers,
            },
            customer,
            line_items,
            tax,
            payment,
            total: money(total),
            currency,
            trust,
            extensions,
        })
    }

    fn timestamp(&mut self) -> Result<String, DecodeError> {
        let minutes = self.bits.eg_signed(20)?;
        let extra = if self.bits.flag()? {
            self.bits.get(6)? as i64
        } else {
            0
        };
        if extra >= 60 {
            return Err(DecodeError::Corrupt("seconds"));
        }
        let secs = minutes
            .checked_mul(60)
            .and_then(|m| m.checked_add(extra + EPOCH_2025))
            .ok_or(DecodeError::Corrupt("timestamp"))?;
        let zero = self.bits.flag()?;
        let (minutes, zulu) = if zero {
            (0, self.bits.flag()?)
        } else {
            (self.bits.eg_signed(5)?, false)
        };
        let offset = i32::try_from(minutes * 60)
            .ok()
            .and_then(|s| UtcOffset::from_whole_seconds(s).ok())
            .ok_or(DecodeError::Corrupt("time zone offset"))?;
        let t = OffsetDateTime::from_unix_timestamp(secs)
            .map_err(|_| DecodeError::Corrupt("timestamp"))?
            .to_offset(offset);
        Ok(format_timestamp(t, zulu))
    }

    fn letters(&mut self, n: usize) -> Result<String, DecodeError> {
        (0..n)
            .map(|_| {
                let v = self.bits.get(5)?;
                if v < 26 {
                    Ok((b'A' + v as u8) as char)
                } else {
                    Err(DecodeError::Corrupt("letter"))
                }
            })
            .collect()
    }

    fn tax_ids(&mut self) -> Result<Option<Vec<TaxIdentifier>>, DecodeError> {
        let count = self.bits.eg(0)?;
        if count == 0 {
            return Ok(None);
        }
        (0..count - 1)
            .map(|_| {
                Ok(TaxIdentifier {
                    scheme: self.opt_text()?,
                    value: self.opt_text()?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_convert_only_when_exact() {
        assert_eq!(to_units(12.99, 2), Some(1299));
        assert_eq!(to_units(0.08875, 6), Some(88_750));
        assert_eq!(to_units(1.24, 3), Some(1240));
        assert_eq!(to_units(1.005, 2), None);
        assert_eq!(to_units(-4.5, 2), Some(-450));
    }

    #[test]
    fn rounding_is_half_away_from_zero() {
        assert_eq!(tax_amount(4800, 88_750), 426);
        assert_eq!(tax_amount(825, 87_500), 72);
        assert_eq!(round_div(-5, 10), -1);
        assert_eq!(line_total(1240, 69), 86);
    }
}
