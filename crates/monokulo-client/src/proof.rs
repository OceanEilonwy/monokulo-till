//! Monokulo's proof of work, for `challenge-required` (Android:
//! `core/Proof.kt`): a nonce such that SHA-256 of the challenge followed by
//! the nonce starts with `difficulty` zero bits, sent back as
//! `Monokulo-Proof: <challenge>.<nonce>`.

use sha2::{Digest, Sha256};

/// The `Monokulo-Proof` header's value for `challenge` at `difficulty`: the
/// challenge, a dot, and the first nonce (counting from 0) that passes.
/// It takes about 2^`difficulty` hashes, so call it off the async threads.
pub fn solve(challenge: &str, difficulty: u32) -> String {
    let mut nonce: u64 = 0;
    while zero_bits(&Sha256::digest(format!("{challenge}{nonce}"))) < difficulty {
        nonce += 1;
    }
    format!("{challenge}.{nonce}")
}

/// How many zero bits `hash` starts with.
pub fn zero_bits(hash: &[u8]) -> u32 {
    let mut bits = 0;
    for &byte in hash {
        if byte == 0 {
            bits += 8;
        } else {
            return bits + byte.leading_zeros();
        }
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_bits_counts_across_bytes() {
        assert_eq!(zero_bits(&[0, 0, 0x80]), 16);
        assert_eq!(zero_bits(&[0, 0x0f]), 12);
        assert_eq!(zero_bits(&[0, 0]), 16);
        assert_eq!(zero_bits(&[0xff]), 0);
    }

    #[test]
    fn a_solved_proof_has_the_zero_bits_asked_for() {
        let proof = solve("c0ffee.abc", 12);
        let nonce = proof.strip_prefix("c0ffee.abc.").unwrap();
        assert!(zero_bits(&Sha256::digest(format!("c0ffee.abc{nonce}"))) >= 12);
    }
}
