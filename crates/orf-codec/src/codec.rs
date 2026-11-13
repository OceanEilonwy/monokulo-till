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
use till_types::CurrencyCode;
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
        let end = used.checked_add(len).ok_or(DecodeError::Truncated)?;
        let numbers = rest.get(used..end).ok_or(DecodeError::Truncated)?;
        let text = decompress(&rest[end..], &self.decoder).map_err(DecodeError::Decompress)?;
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

/// Digits after the decimal point in a currency's minor unit: ISO 4217's
/// ([`CurrencyCode::iso_digits`], the table every crate of the till
/// reads), else 2. XMR is 2 here though the till counts it in piconero:
/// the format's digits are shared with every other reader of a code, so
/// they don't follow the till's count. Text that isn't a code at all has
/// 2; the encoder refuses it as the currency's letters anyway.
fn exponent(currency: &str) -> u32 {
    CurrencyCode::new(currency)
        .ok()
        .and_then(|code| code.iso_digits())
        .unwrap_or(2)
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

/// n / d rounded half away from zero; `None` when that's beyond an i64.
/// `n` is a product of two i64s, so `abs` can't overflow.
fn round_div(n: i128, d: i128) -> Option<i64> {
    let q = (n.abs() + d / 2) / d;
    i64::try_from(if n < 0 { -q } else { q }).ok()
}

/// Quantity × unit price; `None` when it's beyond an i64, which no total
/// that was encoded as worked out can be.
fn line_total(qty_milli: i64, price: i64) -> Option<i64> {
    round_div(qty_milli as i128 * price as i128, 1000)
}

/// Base × rate; `None` as for [`line_total`].
fn tax_amount(base: i64, rate_ppm: i64) -> Option<i64> {
    round_div(base as i128 * rate_ppm as i128, 1_000_000)
}

/// The most line items, tax lines or tax identifiers a receipt may have.
/// Each costs only a few bits, so a payload could otherwise claim millions
/// and have them allocated.
const MAX_ITEMS: u64 = 10_000;

/// `a + b`, or the decoder's error for an amount past an i64.
fn add(a: i64, b: i64) -> Result<i64, DecodeError> {
    a.checked_add(b)
        .ok_or(DecodeError::Corrupt("amounts too large"))
}

/// A count read from the bits, refused past [`MAX_ITEMS`] before anything
/// is made for it.
fn count(n: u64, what: &'static str) -> Result<usize, DecodeError> {
    if n > MAX_ITEMS {
        return Err(DecodeError::Corrupt(what));
    }
    Ok(n as usize)
}

// ---- encoding ---------------------------------------------------------------

struct Enc {
    bits: Writer,
    text: Vec<String>,
}

fn too_large(field: &str) -> EncodeError {
    unsupported(field, "amounts too large to add up")
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
        if r.orf_version != crate::ORF_VERSION {
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
        if items.len() as u64 > MAX_ITEMS {
            return Err(unsupported("line_items", "more than 10,000"));
        }
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
            let expected = line_total(qty, price) == Some(total);
            let net = it
                .net_amount
                .map(|n| self.amount(&f("net_amount"), n, exp))
                .transpose()?;
            // Most lines are plain: total = quantity × price, no net amount. One bit.
            let plain = expected && net.is_none();
            self.bits.flag(!plain);
            if !plain {
                self.bits.flag(!expected);
                if !expected {
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
            sum_totals = sum_totals
                .checked_add(total)
                .ok_or_else(|| too_large("line_items"))?;
        }

        // Tax.
        let mut sum_tax = 0i64;
        self.bits.flag(r.tax.is_some());
        if let Some(tax) = &r.tax {
            self.opt_text("tax.tax_scheme", &tax.tax_scheme)?;
            let lines = tax.tax_lines.as_deref();
            if lines.is_some_and(|l| l.len() as u64 > MAX_ITEMS) {
                return Err(unsupported("tax.tax_lines", "more than 10,000"));
            }
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
                let expected = tax_amount(base, ppm) == Some(amount);
                self.bits.flag(!expected);
                if !expected {
                    self.bits.eg_signed(amount, 6);
                }
                self.opt_text(&f("jurisdiction"), &line.jurisdiction)?;
                sum_tax = sum_tax
                    .checked_add(amount)
                    .ok_or_else(|| too_large("tax.tax_lines"))?;
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
        let expected = sum_totals
            .checked_add(sum_tax)
            .and_then(|s| s.checked_add(tip.unwrap_or(0)));
        self.bits.flag(expected != Some(total));
        if expected != Some(total) {
            self.bits.eg_signed(total, 10);
        }
        Ok(())
    }

    fn timestamp(&mut self, s: &str) -> Result<(), EncodeError> {
        let field = "receipt.issue_timestamp";
        let (t, offset) =
            parse_timestamp(s).ok_or_else(|| unsupported(field, "not an RFC 3339 timestamp"))?;
        let zulu = s.ends_with('Z') || s.ends_with('z');
        if format_timestamp(t, offset, zulu).as_deref() != Some(s) {
            return Err(unsupported(
                field,
                "only whole seconds as YYYY-MM-DDTHH:MM:SS with Z or ±HH:MM",
            ));
        }
        // Minutes since 2025, then the seconds only when there are any.
        let since = t - EPOCH_2025;
        self.bits.eg_signed(since.div_euclid(60), 20);
        let secs = since.rem_euclid(60);
        self.bits.flag(secs != 0);
        if secs != 0 {
            self.bits.put(secs as u64, 6);
        }
        let offset = i64::from(offset / 60);
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
        if ids.as_ref().is_some_and(|v| v.len() as u64 > MAX_ITEMS) {
            return Err(unsupported(field, "more than 10,000"));
        }
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

/// An RFC 3339 timestamp as Unix seconds and its offset in seconds (`Z` is
/// UTC's); `None` when it isn't one. What it accepts beyond the one form the
/// codec encodes is refused by comparing the text with
/// [`format_timestamp`]'s. The arithmetic is the calendar's own, so every
/// local time from year -9999 to 9999 at any offset counts, as it did when
/// the `time` crate read it.
pub(crate) fn parse_timestamp(s: &str) -> Option<(i64, i32)> {
    if !rfc3339_shape(s) {
        return None;
    }
    let pieces = jiff::fmt::temporal::Pieces::parse(s).ok()?;
    let offset = pieces.to_numeric_offset()?.seconds();
    let (date, time) = (pieces.date(), pieces.time()?);
    // A leap second reads as :59; only 23:59:60 can be one.
    let leap = s.get(17..19) == Some("60");
    if leap && (time.hour(), time.minute()) != (23, 59) {
        return None;
    }
    let days = days_from_civil(date.year().into(), date.month().into(), date.day().into());
    let local = days * 86_400
        + i64::from(time.hour()) * 3600
        + i64::from(time.minute()) * 60
        + i64::from(time.second());
    Some((local - i64::from(offset), offset))
}

/// `at`, in whole seconds, at `offset`, as a receipt's `issue_timestamp`:
/// `2026-10-22T09:06:41-07:00`, or `…Z` for a zero offset. The one form the
/// codec encodes; a fraction of a second is dropped toward the past.
pub fn issue_timestamp(at: jiff::Timestamp, offset: jiff::tz::Offset) -> String {
    format_timestamp(at.as_second(), offset.seconds(), true)
        .expect("a jiff timestamp's local time is within years -9999 to 9999")
}

/// Whether `s` is written as RFC 3339 writes a timestamp:
/// `YYYY-MM-DDTHH:MM:SS` (or a space for `T`), any fraction, then `Z` or
/// `±HH:MM` (`T` and `Z` in either case), as the `time` crate read it.
fn rfc3339_shape(s: &str) -> bool {
    let b = s.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let fixed = b.len() >= 20
        && digits(0..4)
        && b[4] == b'-'
        && digits(5..7)
        && b[7] == b'-'
        && digits(8..10)
        && matches!(b[10], b'T' | b't' | b' ')
        && digits(11..13)
        && b[13] == b':'
        && digits(14..16)
        && b[16] == b':'
        && digits(17..19);
    if !fixed {
        return false;
    }
    let mut rest = &b[19..];
    if let Some(fraction) = rest.strip_prefix(b".") {
        let n = fraction.iter().take_while(|d| d.is_ascii_digit()).count();
        if n == 0 {
            return false;
        }
        rest = &fraction[n..];
    }
    match rest {
        [b'Z' | b'z'] => true,
        [b'+' | b'-', h1, h2, b':', m1, m2] => [h1, h2, m1, m2].iter().all(|d| d.is_ascii_digit()),
        _ => false,
    }
}

/// The Unix seconds `secs` at `offset` seconds as `YYYY-MM-DDTHH:MM:SS` and
/// `±HH:MM`, or `Z` for a zero offset when `zulu`: the one form the codec
/// encodes. A fraction of a minute of the offset is dropped. `None` when the
/// local time is outside years -9999 to 9999.
fn format_timestamp(secs: i64, offset: i32, zulu: bool) -> Option<String> {
    let local = secs.checked_add(i64::from(offset))?;
    let (year, month, day) = civil_from_days(local.div_euclid(86_400));
    if !(-9999..=9999).contains(&year) {
        return None;
    }
    let time = local.rem_euclid(86_400);
    let base = format!(
        "{}{:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        if year < 0 { "-" } else { "" },
        year.unsigned_abs(),
        time / 3600,
        time % 3600 / 60,
        time % 60,
    );
    Some(if zulu && offset == 0 {
        format!("{base}Z")
    } else {
        let sign = if offset < 0 { '-' } else { '+' };
        let offset = offset.unsigned_abs();
        format!("{base}{sign}{:02}:{:02}", offset / 3600, offset % 3600 / 60)
    })
}

/// Days from 1970-01-01 to `year`-`month`-`day` in the proleptic Gregorian
/// calendar (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The date `days` after 1970-01-01, as year, month and day
/// ([`days_from_civil`]'s inverse).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
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

        let n = count(self.bits.eg(2)?, "too many line items")?;
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
                self.bits
                    .eg_signed(1)?
                    .checked_mul(1000)
                    .ok_or(DecodeError::Corrupt("quantity"))?
            } else {
                self.bits.eg_signed(10)?
            };
            let price = self.bits.eg_signed(8)?;
            let worked_out = || line_total(qty, price).ok_or(DecodeError::Corrupt("line total"));
            let (total, net_amount) = if !self.bits.flag()? {
                (worked_out()?, None)
            } else {
                let total = if self.bits.flag()? {
                    self.bits.eg_signed(8)?
                } else {
                    worked_out()?
                };
                let net = match self.bits.get(2)? {
                    0 => None,
                    1 => Some(money(total)),
                    _ => Some(money(self.bits.eg_signed(8)?)),
                };
                (total, net)
            };
            sum_totals = add(sum_totals, total)?;
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
            let lines_and_one = self.bits.eg(0)?;
            let tax_lines = if lines_and_one == 0 {
                None
            } else {
                let n = count(lines_and_one - 1, "too many tax lines")?;
                let mut lines = Vec::with_capacity(n);
                for _ in 0..n {
                    let ppm = i64::try_from(self.bits.eg(12)?)
                        .map_err(|_| DecodeError::Corrupt("tax rate"))?;
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
                        tax_amount(base, ppm).ok_or(DecodeError::Corrupt("tax amount"))?
                    };
                    sum_tax = add(sum_tax, amount)?;
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
            add(add(sum_totals, sum_tax)?, tip.unwrap_or(0))?
        };

        Ok(Receipt {
            orf_version: crate::ORF_VERSION.into(),
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
        // An offset `time`'s `UtcOffset` takes: under 26 hours.
        let offset = minutes
            .checked_mul(60)
            .and_then(|s| i32::try_from(s).ok())
            .filter(|s| s.abs() < 26 * 3600)
            .ok_or(DecodeError::Corrupt("time zone offset"))?;
        // An instant in UTC years -9999 to 9999, then the local time in them.
        if format_timestamp(secs, 0, true).is_none() {
            return Err(DecodeError::Corrupt("timestamp"));
        }
        format_timestamp(secs, offset, zulu).ok_or(DecodeError::Corrupt("timestamp"))
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
        let ids_and_one = self.bits.eg(0)?;
        if ids_and_one == 0 {
            return Ok(None);
        }
        (0..count(ids_and_one - 1, "too many tax identifiers")?)
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
        assert_eq!(tax_amount(4800, 88_750), Some(426));
        assert_eq!(tax_amount(825, 87_500), Some(72));
        assert_eq!(round_div(-5, 10), Some(-1));
        assert_eq!(line_total(1240, 69), Some(86));
        assert_eq!(line_total(i64::MAX, i64::MAX), None, "past an i64");
        assert_eq!(line_total(i64::MIN, i64::MAX), None);
        assert_eq!(tax_amount(i64::MAX, 2_000_000), None);
    }

    /// The digits are the till's one ISO 4217 table's: CLF in four, as
    /// Square and Clover amounts are counted, so its receipts encode.
    #[test]
    fn exponents_are_the_shared_tables() {
        for (code, digits) in [("USD", 2), ("JPY", 0), ("KWD", 3), ("CLF", 4), ("UYW", 4)] {
            assert_eq!(exponent(code), digits, "{code}");
            assert_eq!(
                exponent(code),
                CurrencyCode::new(code).unwrap().minor_digits()
            );
        }
        assert_eq!(exponent("XMR"), 2, "a code ISO 4217 doesn't list");
        let receipt: Receipt = serde_json::from_str(
            r#"{
          "orf_version": "0.1.0",
          "receipt": { "receipt_id": "7", "issue_timestamp": "2026-10-22T12:00:00Z", "receipt_type": "sale" },
          "merchant": { "legal_name": "Corner Shop", "country_code": "CL" },
          "line_items": [{ "description": "Te", "quantity": 1, "unit_price": 0.1234, "line_total": 0.1234 }],
          "total": 0.1234,
          "currency": "CLF"
        }"#,
        )
        .unwrap();
        let back = crate::decode(&crate::encode(&receipt).unwrap()).unwrap();
        assert_eq!(back, receipt);
    }

    // ---- hostile payloads ---------------------------------------------------

    /// A payload of `bits` and `text` lines, laid out as [`Codec::encode`]
    /// lays one out, whatever they say.
    fn payload(bits: Writer, text: &[&str]) -> Vec<u8> {
        let numbers = bits.finish();
        let mut out = vec![FORMAT << 4];
        leb128(&mut out, numbers.len());
        out.extend_from_slice(&numbers);
        let text = compress(text.join("\n").as_bytes(), &Codec::shared().encoder).unwrap();
        out.extend_from_slice(&text);
        out
    }

    /// A sale at 2025-01-01T00:00:00Z from a US merchant, up to its tax
    /// identifiers. Its text is "S", "1".
    fn merchant() -> Writer {
        let mut w = Writer::default();
        w.put(0, 2);
        w.put(0, 2);
        w.eg_signed(0, 20);
        w.flag(false);
        w.flag(true);
        w.flag(true);
        w.put((b'U' - b'A') as u64, 5);
        w.put((b'S' - b'A') as u64, 5);
        w.flag(false);
        w.flag(false);
        w
    }

    /// [`merchant`], then no tax identifiers, USD and no customer:
    /// everything before the line items.
    fn head() -> Writer {
        let mut w = merchant();
        w.eg(0, 0);
        w.put(0, 2);
        w.flag(false);
        w
    }

    /// No line items, then a tax section claiming `n` lines.
    fn tax_lines(n: u64) -> Writer {
        let mut w = head();
        w.eg(0, 2);
        w.put(0, 2);
        w.flag(true);
        w.flag(false);
        w.eg(n.saturating_add(1), 0);
        w
    }

    fn corrupt(bits: Writer, text: &[&str]) -> &'static str {
        match crate::decode(&payload(bits, text)) {
            Err(DecodeError::Corrupt(what)) => what,
            other => panic!("not refused as corrupt: {other:?}"),
        }
    }

    /// One line item with no id, described "x", the quantity in
    /// milli-units, its total worked out unless given.
    fn item(w: &mut Writer, qty_milli: i64, price: i64, total: Option<i64>) {
        w.flag(true);
        w.flag(true);
        w.eg_signed(qty_milli, 10);
        w.eg_signed(price, 8);
        w.flag(total.is_some());
        if let Some(total) = total {
            w.flag(true);
            w.eg_signed(total, 8);
            w.put(0, 2);
        }
    }

    /// [`head`], then `n` line items with no ids.
    fn items(n: u64) -> Writer {
        let mut w = head();
        w.eg(n, 2);
        w.put(0, 2);
        w
    }

    #[test]
    fn a_whole_quantity_past_an_i64_is_refused() {
        let mut w = items(1);
        w.flag(true);
        w.flag(false);
        w.eg_signed(i64::MAX / 2, 1);
        assert_eq!(corrupt(w, &["S", "1", "x"]), "quantity");
    }

    #[test]
    fn a_worked_out_line_total_past_an_i64_is_refused() {
        for qty in [i64::MAX, i64::MIN] {
            let mut w = items(1);
            item(&mut w, qty, i64::MAX, None);
            assert_eq!(corrupt(w, &["S", "1", "x"]), "line total", "{qty}");
        }
    }

    #[test]
    fn line_totals_that_add_up_past_an_i64_are_refused() {
        for (first, second) in [(i64::MAX, 1), (i64::MIN, -1)] {
            let mut w = items(2);
            item(&mut w, 1000, 1, Some(first));
            item(&mut w, 1000, 1, Some(second));
            assert_eq!(corrupt(w, &["S", "1", "x", "x"]), "amounts too large");
        }
    }

    #[test]
    fn a_tax_rate_past_an_i64_is_refused() {
        let mut w = tax_lines(1);
        w.eg(u64::MAX, 12);
        assert_eq!(corrupt(w, &["S", "1"]), "tax rate");
    }

    #[test]
    fn a_worked_out_tax_amount_past_an_i64_is_refused() {
        let mut w = tax_lines(1);
        w.eg(i64::MAX as u64, 12);
        w.put(2, 2);
        w.eg_signed(i64::MAX, 8);
        w.flag(false);
        assert_eq!(corrupt(w, &["S", "1"]), "tax amount");
    }

    #[test]
    fn a_total_worked_out_past_an_i64_is_refused() {
        // A line total of i64::MAX and a tip of 1, with no total given.
        let mut w = items(1);
        item(&mut w, 1000, 1, Some(i64::MAX));
        w.flag(false);
        w.flag(false);
        w.flag(false);
        w.flag(true);
        w.eg_signed(1, 9);
        w.put(1, 2);
        w.flag(false);
        assert_eq!(corrupt(w, &["S", "1", "x"]), "amounts too large");
    }

    #[test]
    fn counts_past_the_limit_are_refused_before_anything_is_made() {
        for n in [MAX_ITEMS + 1, u64::MAX - 1] {
            assert_eq!(corrupt(items(n), &["S", "1"]), "too many line items", "{n}");
            assert_eq!(
                corrupt(tax_lines(n), &["S", "1"]),
                "too many tax lines",
                "{n}"
            );
            let mut w = merchant();
            w.eg(n + 1, 0);
            assert_eq!(corrupt(w, &["S", "1"]), "too many tax identifiers", "{n}");
        }
    }

    #[test]
    fn a_number_longer_than_a_u64_is_refused() {
        // The line item count, at order 2, with 63 leading zeros: 65 bits
        // would follow, one more than a u64 holds.
        let mut w = head();
        w.put(0, 63);
        w.put(u64::MAX, 64);
        w.put(u64::MAX, 8);
        assert_eq!(corrupt(w, &["S", "1"]), "number too long");
    }

    #[test]
    fn a_numbers_length_past_the_payload_is_truncated() {
        for bytes in [
            vec![FORMAT << 4, 0xff, 0xff, 0xff, 0x7f],
            vec![FORMAT << 4, 0xff, 0xff, 0xff, 0xff, 0x01],
            vec![FORMAT << 4],
        ] {
            assert!(
                matches!(crate::decode(&bytes), Err(DecodeError::Truncated)),
                "{bytes:?}"
            );
        }
    }

    /// Bytes from a fixed xorshift, so a failure can be run again.
    fn noise(seed: &mut u64, len: usize) -> Vec<u8> {
        (0..len)
            .map(|_| {
                *seed ^= *seed << 13;
                *seed ^= *seed >> 7;
                *seed ^= *seed << 17;
                *seed as u8
            })
            .collect()
    }

    /// Random payloads, and a real one cut short or with bytes changed:
    /// every one decodes or is refused, and none panics (overflow checks
    /// are on in tests).
    #[test]
    fn hostile_payloads_never_panic() {
        let mut seed = 0x9e37_79b9_7f4a_7c15;
        for len in 0..4000 {
            let mut bytes = noise(&mut seed, len % 200);
            if let Some(b) = bytes.first_mut() {
                *b = FORMAT << 4;
            }
            let _ = crate::decode(&bytes);
        }
        let receipt: Receipt = serde_json::from_str(
            r#"{
          "orf_version": "0.1.0",
          "receipt": { "receipt_id": "7", "issue_timestamp": "2026-10-22T12:00:00-07:00", "receipt_type": "sale" },
          "merchant": { "legal_name": "Corner Shop", "country_code": "US", "tax_identifiers": [{ "scheme": "EIN", "value": "1" }] },
          "line_items": [
            { "description": "Tea", "quantity": 2.5, "unit_price": 2.5, "line_total": 6.25 },
            { "description": "Cake", "quantity": 3, "unit_price": 4, "line_total": 11.0, "net_amount": 10.0 }
          ],
          "tax": { "tax_lines": [{ "rate": 0.08875, "tax_amount": 1.51 }] },
          "total": 18.76,
          "currency": "EUR",
          "extensions": { "tip": { "amount": 1.5 } }
        }"#,
        )
        .unwrap();
        let good = crate::encode(&receipt).unwrap();
        assert_eq!(crate::decode(&good).unwrap(), receipt);
        for i in 1..good.len() {
            for flip in [0x01, 0x10, 0x80, 0xff] {
                let mut bad = good.clone();
                bad[i] ^= flip;
                let _ = crate::decode(&bad);
            }
            let _ = crate::decode(&good[..i]);
        }
        for _ in 0..2000 {
            let mut bad = good.clone();
            let at = 1 + noise(&mut seed, 1)[0] as usize % (good.len() - 1);
            for (b, w) in bad[at..].iter_mut().zip(noise(&mut seed, 8)) {
                *b = w;
            }
            let _ = crate::decode(&bad);
        }
    }

    /// A receipt whose amounts add up past an i64 is refused, not wrapped.
    #[test]
    fn amounts_that_add_up_past_an_i64_are_not_encoded() {
        let item = r#"{ "description": "Gold", "quantity": 1, "unit_price": 9.0e13, "line_total": 9.0e13 }"#;
        let items = vec![item; 1100].join(",");
        let receipt: Receipt = serde_json::from_str(&format!(
            r#"{{
          "orf_version": "0.1.0",
          "receipt": {{ "receipt_id": "7", "issue_timestamp": "2026-10-22T12:00:00Z", "receipt_type": "sale" }},
          "merchant": {{ "legal_name": "Mint", "country_code": "US" }},
          "line_items": [{items}],
          "total": 1,
          "currency": "USD"
        }}"#
        ))
        .unwrap();
        assert!(matches!(
            crate::encode(&receipt),
            Err(EncodeError::Unsupported {
                reason: "amounts too large to add up",
                ..
            })
        ));
    }
}
