#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::core::error::CoreError;
use crate::core::hash::{sha256, Hash32};

const TX_PAYLOAD_ENCODING_VERSION: u8 = 1;
const TX_ENCODING_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedTransactionPayload {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub fee: u64,
    pub nonce: u64,
}

impl SignedTransactionPayload {
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(TX_PAYLOAD_ENCODING_VERSION);
        write_len_prefixed_bytes(&mut out, self.from.as_bytes());
        write_len_prefixed_bytes(&mut out, self.to.as_bytes());
        out.extend_from_slice(&self.amount.to_be_bytes());
        out.extend_from_slice(&self.fee.to_be_bytes());
        out.extend_from_slice(&self.nonce.to_be_bytes());
        out
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, CoreError> {
        let mut cursor = Cursor::new(bytes);

        let version = cursor.read_u8("payload.version")?;
        if version != TX_PAYLOAD_ENCODING_VERSION {
            return Err(CoreError::InvalidEncodingVersion {
                field: "payload.version",
                expected: TX_PAYLOAD_ENCODING_VERSION,
                found: version,
            });
        }

        let from = cursor.read_string("payload.from")?;
        let to = cursor.read_string("payload.to")?;
        let amount = cursor.read_u64("payload.amount")?;
        let fee = cursor.read_u64("payload.fee")?;
        let nonce = cursor.read_u64("payload.nonce")?;
        cursor.ensure_consumed()?;

        Ok(Self {
            from,
            to,
            amount,
            fee,
            nonce,
        })
    }

    pub fn hash(&self) -> Hash32 {
        sha256(&self.encode_canonical())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transaction {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub fee: u64,
    pub nonce: u64,
    pub signature: Vec<u8>,
}

impl Transaction {
    pub fn signing_payload(&self) -> SignedTransactionPayload {
        SignedTransactionPayload {
            from: self.from.clone(),
            to: self.to.clone(),
            amount: self.amount,
            fee: self.fee,
            nonce: self.nonce,
        }
    }

    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(TX_ENCODING_VERSION);
        write_len_prefixed_bytes(&mut out, self.from.as_bytes());
        write_len_prefixed_bytes(&mut out, self.to.as_bytes());
        out.extend_from_slice(&self.amount.to_be_bytes());
        out.extend_from_slice(&self.fee.to_be_bytes());
        out.extend_from_slice(&self.nonce.to_be_bytes());
        write_len_prefixed_bytes(&mut out, &self.signature);
        out
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, CoreError> {
        let mut cursor = Cursor::new(bytes);

        let version = cursor.read_u8("transaction.version")?;
        if version != TX_ENCODING_VERSION {
            return Err(CoreError::InvalidEncodingVersion {
                field: "transaction.version",
                expected: TX_ENCODING_VERSION,
                found: version,
            });
        }

        let from = cursor.read_string("transaction.from")?;
        let to = cursor.read_string("transaction.to")?;
        let amount = cursor.read_u64("transaction.amount")?;
        let fee = cursor.read_u64("transaction.fee")?;
        let nonce = cursor.read_u64("transaction.nonce")?;
        let signature = cursor.read_vec("transaction.signature")?;
        cursor.ensure_consumed()?;

        Ok(Self {
            from,
            to,
            amount,
            fee,
            nonce,
            signature,
        })
    }

    pub fn tx_hash(&self) -> Hash32 {
        sha256(&self.encode_canonical())
    }
}

fn write_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = bytes.len() as u32;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
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

    fn read_vec(&mut self, field: &'static str) -> Result<Vec<u8>, CoreError> {
        let len = self.read_u32(field)? as usize;
        let bytes = self.read_exact(field, len)?;
        Ok(bytes.to_vec())
    }

    fn read_string(&mut self, field: &'static str) -> Result<String, CoreError> {
        let bytes = self.read_vec(field)?;
        String::from_utf8(bytes).map_err(|_| CoreError::InvalidUtf8(field))
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
    use crate::core::error::CoreError;
    use crate::core::transaction::{SignedTransactionPayload, Transaction};

    #[test]
    fn signed_payload_roundtrip_and_hash_are_deterministic() -> Result<(), CoreError> {
        let payload = SignedTransactionPayload {
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 42,
            fee: 1,
            nonce: 8,
        };

        let first_encoded = payload.encode_canonical();
        let second_encoded = payload.encode_canonical();
        assert_eq!(first_encoded, second_encoded);

        let decoded = SignedTransactionPayload::decode_canonical(&first_encoded)?;
        assert_eq!(decoded, payload);
        assert_eq!(payload.hash(), decoded.hash());
        Ok(())
    }

    #[test]
    fn transaction_roundtrip_and_hash_are_deterministic() -> Result<(), CoreError> {
        let tx = Transaction {
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 100,
            fee: 2,
            nonce: 9,
            signature: vec![1, 2, 3, 4],
        };

        let first_hash = tx.tx_hash();
        let second_hash = tx.tx_hash();
        assert_eq!(first_hash, second_hash);

        let encoded = tx.encode_canonical();
        let decoded = Transaction::decode_canonical(&encoded)?;
        assert_eq!(decoded, tx);
        assert_eq!(decoded.tx_hash(), first_hash);

        let signing_payload = tx.signing_payload();
        assert_eq!(signing_payload.from, tx.from);
        assert_eq!(signing_payload.to, tx.to);
        assert_eq!(signing_payload.amount, tx.amount);
        assert_eq!(signing_payload.fee, tx.fee);
        assert_eq!(signing_payload.nonce, tx.nonce);
        Ok(())
    }

    #[test]
    fn transaction_decode_rejects_trailing_bytes() {
        let tx = Transaction {
            from: "a".to_string(),
            to: "b".to_string(),
            amount: 1,
            fee: 0,
            nonce: 0,
            signature: vec![],
        };

        let mut encoded = tx.encode_canonical();
        encoded.push(0xAA);

        let decoded = Transaction::decode_canonical(&encoded);
        assert!(matches!(decoded, Err(CoreError::TrailingBytes(1))));
    }

    #[test]
    fn payload_decode_rejects_invalid_version() {
        let payload = SignedTransactionPayload {
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 1,
            fee: 0,
            nonce: 1,
        };

        let mut encoded = payload.encode_canonical();
        encoded[0] = 200;

        let decoded = SignedTransactionPayload::decode_canonical(&encoded);
        assert!(matches!(
            decoded,
            Err(CoreError::InvalidEncodingVersion {
                field: "payload.version",
                expected: 1,
                found: 200
            })
        ));
    }
}
