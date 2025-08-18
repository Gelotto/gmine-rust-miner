use anyhow::Result;
use secp256k1::{Message, SecretKey, Secp256k1};
use tiny_keccak::{Hasher, Keccak};

/// Transaction signer for Injective blockchain
/// Handles Ethereum-style signing with recovery ID
pub struct TransactionSigner {
    secp: Secp256k1<secp256k1::All>,
}

impl TransactionSigner {
    pub fn new() -> Self {
        Self {
            secp: Secp256k1::new(),
        }
    }
    
    /// Sign transaction bytes with a private key
    /// Returns 65-byte signature (64 bytes + 1 byte recovery ID)
    pub fn sign_transaction(
        &self,
        tx_bytes: &[u8],
        private_key: &SecretKey,
    ) -> Result<Vec<u8>> {
        // Hash the transaction bytes with Keccak256 (Injective uses ethsecp256k1)
        let mut hasher = Keccak::v256();
        let mut hash = [0u8; 32];
        hasher.update(tx_bytes);
        hasher.finalize(&mut hash);
        
        // Create a secp256k1 message from the hash
        let message = Message::from_digest_slice(&hash)?;
        
        // Sign with recoverable signature (for Ethereum compatibility)
        let recoverable_sig = self.secp.sign_ecdsa_recoverable(&message, private_key);
        let (recovery_id, signature) = recoverable_sig.serialize_compact();
        
        // Combine signature (64 bytes) + recovery_id (1 byte) = 65 bytes total
        // For Ethereum/Injective compatibility, recovery ID must be 27 or 28
        // The internal recovery_id is 0-3, but Ethereum only uses the parity (0 or 1)
        // v = 27 + (recovery_id % 2)
        let mut sig_bytes = Vec::with_capacity(65);
        sig_bytes.extend_from_slice(&signature);
        sig_bytes.push((recovery_id.to_i32() % 2) as u8 + 27);
        
        Ok(sig_bytes)
    }
    
    /// Sign a message directly (without hashing)
    /// Used for signing pre-hashed data
    pub fn sign_message(
        &self,
        message_hash: &[u8; 32],
        private_key: &SecretKey,
    ) -> Result<Vec<u8>> {
        // Create message from pre-computed hash
        let message = Message::from_digest_slice(message_hash)?;
        
        // Sign with recoverable signature
        let recoverable_sig = self.secp.sign_ecdsa_recoverable(&message, private_key);
        let (recovery_id, signature) = recoverable_sig.serialize_compact();
        
        // Return 65-byte signature with Ethereum-compatible recovery ID
        // v = 27 + (recovery_id % 2) to ensure v is only 27 or 28
        let mut sig_bytes = Vec::with_capacity(65);
        sig_bytes.extend_from_slice(&signature);
        sig_bytes.push((recovery_id.to_i32() % 2) as u8 + 27);
        
        Ok(sig_bytes)
    }
}

impl Default for TransactionSigner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::wallet::InjectiveWallet;
    
    #[test]
    fn test_transaction_signing() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = InjectiveWallet::from_mnemonic_no_passphrase(mnemonic).unwrap();
        
        let signer = TransactionSigner::new();
        let tx_bytes = b"test transaction data";
        
        // Sign the transaction - get private key from wallet
        let private_key = wallet.private_key().unwrap();
        let signature = signer.sign_transaction(tx_bytes, &private_key).unwrap();
        
        // Signature should be 65 bytes (64 + 1 recovery ID)
        assert_eq!(signature.len(), 65);
        
        // Sign same data again - should be deterministic
        let signature2 = signer.sign_transaction(tx_bytes, &private_key).unwrap();
        assert_eq!(signature, signature2);
    }
    
    #[test]
    fn test_message_signing() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = InjectiveWallet::from_mnemonic_no_passphrase(mnemonic).unwrap();
        
        let signer = TransactionSigner::new();
        let message_hash = [0x42u8; 32]; // Test hash
        
        // Sign the message - get private key from wallet
        let private_key = wallet.private_key().unwrap();
        let signature = signer.sign_message(&message_hash, &private_key).unwrap();
        
        // Signature should be 65 bytes
        assert_eq!(signature.len(), 65);
    }
}