//! A bit writer and reader with Exp-Golomb codes: small numbers cost few
//! bits and any number can be written.

use crate::DecodeError;

#[derive(Default)]
pub struct Writer {
    out: Vec<u8>,
    acc: u8,
    n: u32,
}

impl Writer {
    pub fn put(&mut self, value: u64, width: u32) {
        for i in (0..width).rev() {
            self.acc = (self.acc << 1) | ((value >> i) & 1) as u8;
            self.n += 1;
            if self.n == 8 {
                self.out.push(self.acc);
                self.acc = 0;
                self.n = 0;
            }
        }
    }

    pub fn flag(&mut self, b: bool) {
        self.put(b as u64, 1);
    }

    /// Exp-Golomb of order `k`, for an unsigned value.
    pub fn eg(&mut self, value: u64, k: u32) {
        let v = value as u128 + (1u128 << k);
        let len = 128 - v.leading_zeros();
        self.put(0, len - 1 - k);
        for i in (0..len).rev() {
            self.put(((v >> i) & 1) as u64, 1);
        }
    }

    /// Exp-Golomb of order `k` after zigzag, for a signed value.
    pub fn eg_signed(&mut self, value: i64, k: u32) {
        self.eg(((value << 1) ^ (value >> 63)) as u64, k);
    }

    pub fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            let pad = 8 - self.n;
            self.put(0, pad);
        }
        self.out
    }
}

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    fn bit(&mut self) -> Result<u64, DecodeError> {
        let byte = self.data.get(self.pos / 8).ok_or(DecodeError::Truncated)?;
        self.pos += 1;
        Ok(((byte >> (7 - (self.pos - 1) % 8)) & 1) as u64)
    }

    pub fn get(&mut self, width: u32) -> Result<u64, DecodeError> {
        let mut v = 0;
        for _ in 0..width {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }

    pub fn flag(&mut self) -> Result<bool, DecodeError> {
        Ok(self.bit()? == 1)
    }

    pub fn eg(&mut self, k: u32) -> Result<u64, DecodeError> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 64 {
                return Err(DecodeError::Corrupt("number too long"));
            }
        }
        let rest = self.get(zeros + k)?;
        let v = (1u128 << (zeros + k)) | rest as u128;
        u64::try_from(v - (1u128 << k)).map_err(|_| DecodeError::Corrupt("number too large"))
    }

    pub fn eg_signed(&mut self, k: u32) -> Result<i64, DecodeError> {
        let z = self.eg(k)?;
        Ok(((z >> 1) as i64) ^ -((z & 1) as i64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_round_trip_at_every_order() {
        let values = [
            0u64,
            1,
            2,
            7,
            255,
            256,
            1_000_000,
            u32::MAX as u64,
            u64::MAX - 1,
        ];
        let signed = [0i64, 1, -1, 450, -450, i64::MIN / 2, i64::MAX / 2];
        for k in [0, 1, 8, 20] {
            let mut w = Writer::default();
            values.iter().for_each(|&v| w.eg(v, k));
            signed.iter().for_each(|&v| w.eg_signed(v, k));
            w.put(0b101, 3);
            let bytes = w.finish();
            let mut r = Reader::new(&bytes);
            for &v in &values {
                assert_eq!(r.eg(k).unwrap(), v);
            }
            for &v in &signed {
                assert_eq!(r.eg_signed(k).unwrap(), v);
            }
            assert_eq!(r.get(3).unwrap(), 0b101);
        }
    }

    #[test]
    fn reading_past_the_end_is_an_error() {
        let mut r = Reader::new(&[0]);
        assert!(matches!(r.eg(0), Err(DecodeError::Truncated)));
    }
}
