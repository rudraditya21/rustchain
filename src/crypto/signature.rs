#![allow(dead_code)]

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::core::hash::sha256;
use crate::core::transaction::SignedTransactionPayload;
use crate::crypto::error::CryptoError;

pub const SECRET_KEY_LENGTH: usize = 32;
pub const PUBLIC_KEY_LENGTH: usize = 32;
pub const SIGNATURE_LENGTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretKeyBytes(pub [u8; SECRET_KEY_LENGTH]);

impl SecretKeyBytes {
    pub fn as_bytes(&self) -> &[u8; SECRET_KEY_LENGTH] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        encode_hex(self.as_bytes())
    }

    pub fn from_hex(hex: &str) -> Result<Self, CryptoError> {
        decode_hex::<SECRET_KEY_LENGTH>(hex, "secret_key_hex").map(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKeyBytes(pub [u8; PUBLIC_KEY_LENGTH]);

impl PublicKeyBytes {
    pub fn as_bytes(&self) -> &[u8; PUBLIC_KEY_LENGTH] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        encode_hex(self.as_bytes())
    }

    pub fn from_hex(hex: &str) -> Result<Self, CryptoError> {
        decode_hex::<PUBLIC_KEY_LENGTH>(hex, "public_key_hex").map(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureBytes(pub [u8; SIGNATURE_LENGTH]);

impl SignatureBytes {
    pub fn as_bytes(&self) -> &[u8; SIGNATURE_LENGTH] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        encode_hex(self.as_bytes())
    }

    pub fn from_hex(hex: &str) -> Result<Self, CryptoError> {
        decode_hex::<SIGNATURE_LENGTH>(hex, "signature_hex").map(Self)
    }
}

pub fn derive_address(public_key: &PublicKeyBytes) -> String {
    let digest = sha256(public_key.as_bytes());
    let mut address = String::from("rc1");
    address.push_str(&encode_hex(&digest.0[..20]));
    address
}

pub fn signing_key_from_secret(secret: &SecretKeyBytes) -> SigningKey {
    SigningKey::from_bytes(secret.as_bytes())
}

pub fn verifying_key_bytes(signing_key: &SigningKey) -> PublicKeyBytes {
    PublicKeyBytes(signing_key.verifying_key().to_bytes())
}

pub fn verifying_key_from_bytes(public_key: &PublicKeyBytes) -> Result<VerifyingKey, CryptoError> {
    VerifyingKey::from_bytes(public_key.as_bytes()).map_err(|_| CryptoError::PublicKeyParse)
}

pub fn sign_transaction_payload(
    signing_key: &SigningKey,
    payload: &SignedTransactionPayload,
) -> SignatureBytes {
    let message = payload.encode_canonical();
    let signature = signing_key.sign(&message);
    SignatureBytes(signature.to_bytes())
}

pub fn verify_transaction_payload(
    public_key: &PublicKeyBytes,
    payload: &SignedTransactionPayload,
    signature: &SignatureBytes,
) -> Result<bool, CryptoError> {
    let verifying_key = verifying_key_from_bytes(public_key)?;
    let message = payload.encode_canonical();
    let signature = Signature::from_bytes(signature.as_bytes());
    Ok(verifying_key.verify(&message, &signature).is_ok())
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

fn decode_hex<const N: usize>(hex: &str, field: &'static str) -> Result<[u8; N], CryptoError> {
    let raw = hex.as_bytes();
    if raw.len() != N * 2 {
        return Err(CryptoError::HexLengthMismatch {
            field,
            expected: N * 2,
            found: raw.len(),
        });
    }

    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        let hi = decode_nibble(raw[i * 2], field, i * 2)?;
        let lo = decode_nibble(raw[i * 2 + 1], field, i * 2 + 1)?;
        *byte = (hi << 4) | lo;
    }
    Ok(out)
}

fn decode_nibble(byte: u8, field: &'static str, index: usize) -> Result<u8, CryptoError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(CryptoError::HexCharacter {
            field,
            index,
            value: byte as char,
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::core::transaction::SignedTransactionPayload;
    use crate::crypto::error::CryptoError;
    use crate::crypto::signature::{
        derive_address, sign_transaction_payload, signing_key_from_secret,
        verify_transaction_payload, verifying_key_bytes, PublicKeyBytes, SecretKeyBytes,
        SignatureBytes,
    };

    fn payload() -> SignedTransactionPayload {
        SignedTransactionPayload {
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 50,
            fee: 2,
            nonce: 7,
        }
    }

    #[test]
    fn sign_and_verify_success() -> Result<(), CryptoError> {
        let signing_key = signing_key_from_secret(&SecretKeyBytes([7u8; 32]));
        let public_key = verifying_key_bytes(&signing_key);
        let signature = sign_transaction_payload(&signing_key, &payload());

        assert!(verify_transaction_payload(
            &public_key,
            &payload(),
            &signature
        )?);
        Ok(())
    }

    #[test]
    fn tampered_payload_fails_verification() -> Result<(), CryptoError> {
        let signing_key = signing_key_from_secret(&SecretKeyBytes([7u8; 32]));
        let public_key = verifying_key_bytes(&signing_key);
        let signature = sign_transaction_payload(&signing_key, &payload());

        let mut tampered = payload();
        tampered.amount += 1;
        assert!(!verify_transaction_payload(
            &public_key,
            &tampered,
            &signature
        )?);
        Ok(())
    }

    #[test]
    fn wrong_key_fails_verification() -> Result<(), CryptoError> {
        let signer_a = signing_key_from_secret(&SecretKeyBytes([11u8; 32]));
        let signer_b = signing_key_from_secret(&SecretKeyBytes([12u8; 32]));

        let signature = sign_transaction_payload(&signer_a, &payload());
        let wrong_public = verifying_key_bytes(&signer_b);

        assert!(!verify_transaction_payload(
            &wrong_public,
            &payload(),
            &signature
        )?);
        Ok(())
    }

    #[test]
    fn modified_signature_is_rejected() -> Result<(), CryptoError> {
        let signing_key = signing_key_from_secret(&SecretKeyBytes([21u8; 32]));
        let public_key = verifying_key_bytes(&signing_key);
        let mut signature = sign_transaction_payload(&signing_key, &payload());
        signature.0[0] ^= 0x80;

        assert!(!verify_transaction_payload(
            &public_key,
            &payload(),
            &signature
        )?);
        Ok(())
    }

    #[test]
    fn serialization_roundtrip_for_key_and_signature() -> Result<(), CryptoError> {
        let signing_key = signing_key_from_secret(&SecretKeyBytes([3u8; 32]));
        let public_key = verifying_key_bytes(&signing_key);
        let signature = sign_transaction_payload(&signing_key, &payload());

        let secret_hex = SecretKeyBytes([3u8; 32]).to_hex();
        let public_hex = public_key.to_hex();
        let signature_hex = signature.to_hex();

        let secret_roundtrip = SecretKeyBytes::from_hex(&secret_hex)?;
        let public_roundtrip = PublicKeyBytes::from_hex(&public_hex)?;
        let signature_roundtrip = SignatureBytes::from_hex(&signature_hex)?;

        assert_eq!(secret_roundtrip, SecretKeyBytes([3u8; 32]));
        assert_eq!(public_roundtrip, public_key);
        assert_eq!(signature_roundtrip, signature);
        Ok(())
    }

    #[test]
    fn address_derivation_is_deterministic() {
        let key_a = PublicKeyBytes([9u8; 32]);
        let key_b = PublicKeyBytes([10u8; 32]);

        let address_a_1 = derive_address(&key_a);
        let address_a_2 = derive_address(&key_a);
        let address_b = derive_address(&key_b);

        assert_eq!(address_a_1, address_a_2);
        assert_ne!(address_a_1, address_b);
        assert!(address_a_1.starts_with("rc1"));
        assert_eq!(address_a_1.len(), 43);
    }
}
