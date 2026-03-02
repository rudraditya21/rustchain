#![allow(dead_code)]

use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;

use crate::core::transaction::SignedTransactionPayload;
use crate::crypto::error::CryptoError;
use crate::crypto::signature::{
    derive_address, sign_transaction_payload, signing_key_from_secret, verify_transaction_payload,
    verifying_key_bytes, PublicKeyBytes, SecretKeyBytes, SignatureBytes,
};

pub struct Wallet {
    signing_key: SigningKey,
}

impl Wallet {
    pub fn generate() -> Self {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        Self { signing_key }
    }

    pub fn from_secret_key(secret_key: SecretKeyBytes) -> Self {
        let signing_key = signing_key_from_secret(&secret_key);
        Self { signing_key }
    }

    pub fn from_secret_key_hex(secret_key_hex: &str) -> Result<Self, CryptoError> {
        let secret_key = SecretKeyBytes::from_hex(secret_key_hex)?;
        Ok(Self::from_secret_key(secret_key))
    }

    pub fn secret_key_bytes(&self) -> SecretKeyBytes {
        SecretKeyBytes(self.signing_key.to_bytes())
    }

    pub fn public_key_bytes(&self) -> PublicKeyBytes {
        verifying_key_bytes(&self.signing_key)
    }

    pub fn secret_key_hex(&self) -> String {
        self.secret_key_bytes().to_hex()
    }

    pub fn public_key_hex(&self) -> String {
        self.public_key_bytes().to_hex()
    }

    pub fn address(&self) -> String {
        derive_address(&self.public_key_bytes())
    }

    pub fn sign_payload(&self, payload: &SignedTransactionPayload) -> SignatureBytes {
        sign_transaction_payload(&self.signing_key, payload)
    }

    pub fn verify_payload(
        &self,
        payload: &SignedTransactionPayload,
        signature: &SignatureBytes,
    ) -> Result<bool, CryptoError> {
        verify_transaction_payload(&self.public_key_bytes(), payload, signature)
    }
}

#[cfg(test)]
mod tests {
    use crate::core::transaction::SignedTransactionPayload;
    use crate::crypto::error::CryptoError;
    use crate::crypto::wallet::Wallet;

    fn payload() -> SignedTransactionPayload {
        SignedTransactionPayload {
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 12,
            fee: 1,
            nonce: 99,
        }
    }

    #[test]
    fn wallet_key_serialization_roundtrip() -> Result<(), CryptoError> {
        let wallet = Wallet::generate();
        let secret_hex = wallet.secret_key_hex();
        let imported = Wallet::from_secret_key_hex(&secret_hex)?;

        assert_eq!(wallet.secret_key_hex(), imported.secret_key_hex());
        assert_eq!(wallet.public_key_hex(), imported.public_key_hex());
        assert_eq!(wallet.address(), imported.address());
        Ok(())
    }

    #[test]
    fn wallet_sign_and_verify() -> Result<(), CryptoError> {
        let wallet = Wallet::generate();
        let signature = wallet.sign_payload(&payload());

        assert!(wallet.verify_payload(&payload(), &signature)?);
        Ok(())
    }
}
