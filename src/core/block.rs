#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::core::error::CoreError;
use crate::core::hash::{meets_difficulty, sha256, Hash32};
use crate::core::merkle::MerkleTree;
use crate::core::transaction::Transaction;

const BLOCK_HEADER_ENCODING_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockHeader {
    pub version: u32,
    pub previous_block_hash: Hash32,
    pub merkle_root: Hash32,
    pub timestamp_unix: u64,
    pub nonce: u64,
    pub difficulty_bits: u32,
}

impl BlockHeader {
    pub fn encode_for_hash(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(1 + 4 + 32 + 32 + 8 + 8 + 4);
        out.push(BLOCK_HEADER_ENCODING_VERSION);
        out.extend_from_slice(&self.version.to_be_bytes());
        out.extend_from_slice(self.previous_block_hash.as_bytes());
        out.extend_from_slice(self.merkle_root.as_bytes());
        out.extend_from_slice(&self.timestamp_unix.to_be_bytes());
        out.extend_from_slice(&self.nonce.to_be_bytes());
        out.extend_from_slice(&self.difficulty_bits.to_be_bytes());
        out
    }

    pub fn decode_from_hash_bytes(bytes: &[u8]) -> Result<Self, CoreError> {
        let mut cursor = Cursor::new(bytes);

        let encoding_version = cursor.read_u8("block_header.encoding_version")?;
        if encoding_version != BLOCK_HEADER_ENCODING_VERSION {
            return Err(CoreError::InvalidEncodingVersion {
                field: "block_header.encoding_version",
                expected: BLOCK_HEADER_ENCODING_VERSION,
                found: encoding_version,
            });
        }

        let version = cursor.read_u32("block_header.version")?;
        let previous_block_hash = Hash32(cursor.read_hash("block_header.previous_block_hash")?);
        let merkle_root = Hash32(cursor.read_hash("block_header.merkle_root")?);
        let timestamp_unix = cursor.read_u64("block_header.timestamp_unix")?;
        let nonce = cursor.read_u64("block_header.nonce")?;
        let difficulty_bits = cursor.read_u32("block_header.difficulty_bits")?;
        cursor.ensure_consumed()?;

        Ok(Self {
            version,
            previous_block_hash,
            merkle_root,
            timestamp_unix,
            nonce,
            difficulty_bits,
        })
    }

    pub fn block_hash(&self) -> Hash32 {
        sha256(&self.encode_for_hash())
    }

    pub fn meets_pow_difficulty(&self) -> Result<bool, CoreError> {
        meets_difficulty(&self.block_hash(), self.difficulty_bits)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}

impl Block {
    pub fn new(header: BlockHeader, transactions: Vec<Transaction>) -> Self {
        Self {
            header,
            transactions,
        }
    }

    pub fn hash(&self) -> Hash32 {
        self.header.block_hash()
    }

    pub fn computed_merkle_root(&self) -> Hash32 {
        MerkleTree::from_transactions(&self.transactions).root()
    }

    pub fn has_valid_merkle_root(&self) -> bool {
        self.computed_merkle_root() == self.header.merkle_root
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_exact(&mut self, field: &'static str, len: usize) -> Result<&'a [u8], CoreError> {
        let remaining = self.bytes.len().saturating_sub(self.offset);
        if remaining < len {
            return Err(CoreError::UnexpectedEof {
                field,
                needed: len,
                remaining,
            });
        }

        let start = self.offset;
        self.offset += len;
        Ok(&self.bytes[start..start + len])
    }

    fn read_u8(&mut self, field: &'static str) -> Result<u8, CoreError> {
        let bytes = self.read_exact(field, 1)?;
        Ok(bytes[0])
    }

    fn read_u32(&mut self, field: &'static str) -> Result<u32, CoreError> {
        let bytes = self.read_exact(field, 4)?;
        let mut out = [0u8; 4];
        out.copy_from_slice(bytes);
        Ok(u32::from_be_bytes(out))
    }

    fn read_u64(&mut self, field: &'static str) -> Result<u64, CoreError> {
        let bytes = self.read_exact(field, 8)?;
        let mut out = [0u8; 8];
        out.copy_from_slice(bytes);
        Ok(u64::from_be_bytes(out))
    }

    fn read_hash(&mut self, field: &'static str) -> Result<[u8; 32], CoreError> {
        let bytes = self.read_exact(field, 32)?;
        let mut out = [0u8; 32];
        out.copy_from_slice(bytes);
        Ok(out)
    }

    fn ensure_consumed(&self) -> Result<(), CoreError> {
        let remaining = self.bytes.len().saturating_sub(self.offset);
        if remaining == 0 {
            return Ok(());
        }

        Err(CoreError::TrailingBytes(remaining))
    }
}

#[cfg(test)]
mod tests {
    use crate::core::block::{Block, BlockHeader};
    use crate::core::error::CoreError;
    use crate::core::hash::Hash32;
    use crate::core::transaction::Transaction;

    fn sample_transaction(nonce: u64) -> Transaction {
        Transaction {
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 10 + nonce,
            fee: 1,
            nonce,
            signature: vec![1, 2, 3],
        }
    }

    #[test]
    fn block_header_hash_encoding_roundtrip_and_determinism() -> Result<(), CoreError> {
        let header = BlockHeader {
            version: 1,
            previous_block_hash: Hash32([7u8; 32]),
            merkle_root: Hash32([9u8; 32]),
            timestamp_unix: 1_700_000_000,
            nonce: 11,
            difficulty_bits: 4,
        };

        let encoded = header.encode_for_hash();
        let decoded = BlockHeader::decode_from_hash_bytes(&encoded)?;
        assert_eq!(decoded, header);

        let first_hash = header.block_hash();
        let second_hash = header.block_hash();
        assert_eq!(first_hash, second_hash);
        Ok(())
    }

    #[test]
    fn block_merkle_root_validation_matches_transactions() {
        let transactions = vec![
            sample_transaction(1),
            sample_transaction(2),
            sample_transaction(3),
        ];

        let mut block = Block::new(
            BlockHeader {
                version: 1,
                previous_block_hash: Hash32([0u8; 32]),
                merkle_root: Hash32::ZERO,
                timestamp_unix: 1_700_000_010,
                nonce: 0,
                difficulty_bits: 0,
            },
            transactions,
        );

        block.header.merkle_root = block.computed_merkle_root();
        assert!(block.has_valid_merkle_root());

        block.header.merkle_root = Hash32([3u8; 32]);
        assert!(!block.has_valid_merkle_root());
    }

    #[test]
    fn block_header_pow_check_works() -> Result<(), CoreError> {
        let header = BlockHeader {
            version: 1,
            previous_block_hash: Hash32([0u8; 32]),
            merkle_root: Hash32([0u8; 32]),
            timestamp_unix: 0,
            nonce: 0,
            difficulty_bits: 0,
        };

        assert!(header.meets_pow_difficulty()?);
        Ok(())
    }

    #[test]
    fn block_header_decode_rejects_invalid_encoding_version() {
        let header = BlockHeader {
            version: 1,
            previous_block_hash: Hash32([0u8; 32]),
            merkle_root: Hash32([1u8; 32]),
            timestamp_unix: 99,
            nonce: 0,
            difficulty_bits: 1,
        };

        let mut encoded = header.encode_for_hash();
        encoded[0] = 2;

        let decoded = BlockHeader::decode_from_hash_bytes(&encoded);
        assert!(matches!(
            decoded,
            Err(CoreError::InvalidEncodingVersion {
                field: "block_header.encoding_version",
                expected: 1,
                found: 2
            })
        ));
    }
}
