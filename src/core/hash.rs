#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::error::CoreError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hash32(pub [u8; 32]);

impl Hash32 {
    pub const ZERO: Self = Self([0u8; 32]);

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

pub fn sha256(data: &[u8]) -> Hash32 {
    let digest = Sha256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Hash32(out)
}

pub fn sha256_pair(left: &Hash32, right: &Hash32) -> Hash32 {
    let mut preimage = [0u8; 64];
    preimage[..32].copy_from_slice(left.as_bytes());
    preimage[32..].copy_from_slice(right.as_bytes());
    sha256(&preimage)
}

pub fn leading_zero_bits(hash: &Hash32) -> u32 {
    let mut bits = 0u32;
    for byte in hash.as_bytes() {
        if *byte == 0 {
            bits += 8;
            continue;
        }

        bits += byte.leading_zeros();
        return bits;
    }
    bits
}

pub fn meets_difficulty(hash: &Hash32, difficulty_bits: u32) -> Result<bool, CoreError> {
    if difficulty_bits > 256 {
        return Err(CoreError::InvalidDifficulty(difficulty_bits));
    }

    let full_bytes = (difficulty_bits / 8) as usize;
    let remaining_bits = (difficulty_bits % 8) as usize;

    if hash.as_bytes()[..full_bytes].iter().any(|byte| *byte != 0) {
        return Ok(false);
    }

    if remaining_bits == 0 {
        return Ok(true);
    }

    let mask = 0xFFu8 << (8 - remaining_bits);
    Ok((hash.as_bytes()[full_bytes] & mask) == 0)
}

#[cfg(test)]
mod tests {
    use crate::core::error::CoreError;
    use crate::core::hash::{leading_zero_bits, meets_difficulty, sha256, Hash32};

    #[test]
    fn sha256_is_deterministic() {
        let input = b"rustchain deterministic hash input";
        let first = sha256(input);
        let second = sha256(input);

        assert_eq!(first, second);
    }

    #[test]
    fn leading_zero_bit_count_works() {
        let hash = Hash32([
            0x00,
            0x00,
            0b0001_1111,
            0xAA,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ]);
        assert_eq!(leading_zero_bits(&hash), 19);
    }

    #[test]
    fn difficulty_check_accepts_and_rejects_correctly() -> Result<(), CoreError> {
        let hash = Hash32([
            0x00,
            0b0000_1111,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
        ]);

        assert!(meets_difficulty(&hash, 12)?);
        assert!(!meets_difficulty(&hash, 13)?);
        Ok(())
    }

    #[test]
    fn invalid_difficulty_is_rejected() {
        let hash = Hash32::ZERO;
        let result = meets_difficulty(&hash, 257);

        assert!(matches!(result, Err(CoreError::InvalidDifficulty(257))));
    }
}
