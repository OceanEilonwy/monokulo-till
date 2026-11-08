//! Bytes as decimal digits: 7 bytes to 17 digits, 2.43 digits a byte, close
//! to the 2.41 optimum. Digits need no escaping in a URI, and a QR code packs
//! them at 3.33 bits each, tighter than any letter-based text encoding.

use crate::DecodeError;

/// Digits for a final group of 1 to 7 bytes: ceil(bytes × log10(256)).
/// Each count is distinct, so the last group's length says how many bytes it holds.
const DIGITS: [usize; 8] = [0, 3, 5, 8, 10, 13, 15, 17];

pub fn to_digits(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 5 / 2 + 3);
    for chunk in data.chunks(7) {
        let n = chunk.iter().fold(0u64, |acc, &b| (acc << 8) | b as u64);
        s.push_str(&format!("{n:0width$}", width = DIGITS[chunk.len()]));
    }
    s
}

pub fn from_digits(s: &str) -> Result<Vec<u8>, DecodeError> {
    if !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(DecodeError::Digits);
    }
    let mut out = Vec::with_capacity(s.len() * 2 / 5 + 1);
    let mut rest = s;
    while !rest.is_empty() {
        let take = rest.len().min(17);
        let bytes = DIGITS
            .iter()
            .position(|&d| d == take)
            .ok_or(DecodeError::Digits)?;
        let n: u64 = rest[..take].parse().map_err(|_| DecodeError::Digits)?;
        if bytes < 8 && n >> (8 * bytes) != 0 {
            return Err(DecodeError::Digits);
        }
        for i in (0..bytes).rev() {
            out.push((n >> (8 * i)) as u8);
        }
        rest = &rest[take..];
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_length_round_trips() {
        for len in 0..40 {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 255) as u8).collect();
            assert_eq!(
                from_digits(&to_digits(&data)).unwrap(),
                data,
                "length {len}"
            );
        }
        let max = vec![0xff; 21];
        assert_eq!(from_digits(&to_digits(&max)).unwrap(), max);
    }

    #[test]
    fn malformed_digits_are_refused() {
        assert!(from_digits("12a").is_err());
        assert!(from_digits("1234").is_err(), "4 digits isn't a valid group");
        assert!(from_digits("999").is_err(), "999 doesn't fit in one byte");
    }
}
