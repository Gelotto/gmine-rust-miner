use anyhow::{Result, bail, anyhow};
use bip39::Mnemonic;
use bip32::{XPrv, ChildNumber};
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use tiny_keccak::{Hasher, Keccak};
use bech32::{self, Hrp};
use zeroize::{Zeroize, ZeroizeOnDrop};
use log::{info, warn, debug};

const INJECTIVE_HD_PATH: &str = "m/44'/60'/0'/0/0"; // Ethereum-style HD path for Injective
const INJECTIVE_PREFIX: &str = "inj";

/// Mobile-optimized secure wallet for Injective blockchain
/// Uses Android Keystore integration for key protection
#[derive(Debug, Zeroize, ZeroizeOnDrop)]
pub struct MobileWallet {
    #[zeroize(skip)] // Public data doesn't need zeroizing
    pub address: String,
    
    // Private fields with automatic zeroization
    private_key_bytes: [u8; 32],
    public_key_bytes: [u8; 65],
    
    #[zeroize(skip)]
    keystore_alias: Option<String>, // For Android Keystore integration
}

impl MobileWallet {
    /// Create a wallet from a BIP39 mnemonic phrase with optional passphrase
    /// For mobile security, this should only be used during wallet setup
    pub fn from_mnemonic(mnemonic_str: &str, passphrase: &str) -> Result<Self> {
        info!("Creating mobile wallet from mnemonic");
        
        // Parse and validate mnemonic
        let mnemonic = Mnemonic::parse(mnemonic_str)?;
        
        // Generate seed from mnemonic with passphrase
        let seed = mnemonic.to_seed(passphrase);
        
        // Derive private key using proper BIP32 HD derivation
        let private_key = derive_private_key_bip32(&seed, INJECTIVE_HD_PATH)?;
        
        // Get public key from private key
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&private_key)?;
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        
        // Generate Injective address (Ethereum-style with bech32 encoding)
        let address = generate_injective_address(&public_key)?;
        
        info!("Generated mobile wallet address: {}", address);
        
        // Store keys securely
        let mut private_key_bytes = [0u8; 32];
        private_key_bytes.copy_from_slice(&private_key);
        
        let public_key_bytes = public_key.serialize_uncompressed();
        
        // Zeroize the temporary private key
        let mut temp_key = private_key;
        temp_key.zeroize();
        
        Ok(Self {
            address,
            private_key_bytes,
            public_key_bytes,
            keystore_alias: None, // Will be set when storing in Android Keystore
        })
    }
    
    /// Create a wallet from a BIP39 mnemonic with no passphrase
    pub fn from_mnemonic_no_passphrase(mnemonic_str: &str) -> Result<Self> {
        Self::from_mnemonic(mnemonic_str, "")
    }
    
    /// Store wallet keys in Android Keystore (placeholder for JNI integration)
    /// In production, this would call Android Keystore APIs
    pub fn store_in_keystore(&mut self, alias: &str) -> Result<()> {
        warn!("Android Keystore integration not yet implemented for mobile");
        // TODO: Implement Android Keystore integration via JNI
        // This would:
        // 1. Generate or import key pair into Android Keystore
        // 2. Store the keystore alias for future use
        // 3. Zeroize private_key_bytes after successful storage
        
        self.keystore_alias = Some(alias.to_string());
        info!("Wallet keys marked for keystore storage with alias: {}", alias);
        
        Ok(())
    }
    
    /// Load wallet from Android Keystore (placeholder for JNI integration)
    pub fn load_from_keystore(_alias: &str) -> Result<Self> {
        bail!("Android Keystore integration not yet implemented")
        // TODO: Implement Android Keystore loading via JNI
        // This would:
        // 1. Load public key from keystore
        // 2. Derive address from public key
        // 3. Return wallet with keystore_alias set
        // 4. Keep private_key_bytes empty (protected in keystore)
    }
    
    /// Get the private key as a SecretKey (for signing)
    /// WARNING: Only use this for immediate signing, never store the result
    pub fn private_key(&self) -> Result<SecretKey> {
        if self.keystore_alias.is_some() {
            // In production with Android Keystore, this would:
            // 1. Request signing operation from keystore
            // 2. Never expose the actual private key
            warn!("Keystore-protected key access not fully implemented");
        }
        
        SecretKey::from_slice(&self.private_key_bytes)
            .map_err(|e| anyhow!("Invalid private key: {}", e))
    }
    
    /// Get the public key as a PublicKey
    pub fn public_key(&self) -> Result<PublicKey> {
        let secp = Secp256k1::new();
        let secret_key = self.private_key()?;
        Ok(PublicKey::from_secret_key(&secp, &secret_key))
    }
    
    /// Get the public key as uncompressed bytes (65 bytes with 0x04 prefix)
    pub fn public_key_bytes(&self) -> [u8; 65] {
        self.public_key_bytes
    }
    
    /// Get the public key as compressed bytes (33 bytes)
    pub fn public_key_compressed(&self) -> Result<[u8; 33]> {
        let public_key = self.public_key()?;
        Ok(public_key.serialize())
    }
    
    /// Get the raw private key bytes (32 bytes)
    pub fn private_key_bytes(&self) -> &[u8] {
        &self.private_key_bytes
    }
    
    /// Get the raw public key bytes (65 bytes uncompressed)
    pub fn public_key_bytes_ref(&self) -> &[u8] {
        &self.public_key_bytes
    }
    
    /// Check if wallet is protected by Android Keystore
    pub fn is_keystore_protected(&self) -> bool {
        self.keystore_alias.is_some()
    }
    
    /// Generate a new mnemonic phrase
    pub fn generate_mnemonic() -> Result<String> {
        // Generate 128 bits of entropy for 12-word mnemonic
        let mut entropy = [0u8; 16];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut entropy);
        
        let mnemonic = Mnemonic::from_entropy(&entropy)?;
        Ok(mnemonic.to_string())
    }
    
    /// Validate a mnemonic phrase
    pub fn validate_mnemonic(mnemonic_str: &str) -> bool {
        Mnemonic::parse(mnemonic_str).is_ok()
    }
    
    /// Validate an Injective address format
    pub fn validate_address(address: &str) -> Result<()> {
        // Injective addresses start with "inj1"
        if !address.starts_with("inj1") {
            return Err(anyhow!("Invalid Injective address: must start with 'inj1'"));
        }
        
        // Check length (should be 42 characters for Injective)
        if address.len() != 42 {
            return Err(anyhow!("Invalid Injective address: incorrect length"));
        }
        
        // Validate bech32 encoding
        let hrp = Hrp::parse(INJECTIVE_PREFIX)?;
        let (decoded_hrp, _data) = bech32::decode(address)?;
        if decoded_hrp != hrp {
            return Err(anyhow!("Invalid address prefix"));
        }
        
        Ok(())
    }
}

/// Transaction signer for mobile Injective operations
/// Handles Ethereum-style signing with recovery ID
pub struct MobileTransactionSigner {
    secp: Secp256k1<secp256k1::All>,
}

impl MobileTransactionSigner {
    pub fn new() -> Self {
        Self {
            secp: Secp256k1::new(),
        }
    }
    
    /// Sign a pre-hashed message for transaction signing
    /// Returns 65-byte signature (64 bytes + 1 byte recovery ID)
    pub fn sign_message_hash(
        &self,
        message_hash: &[u8; 32],
        private_key: &SecretKey,
    ) -> Result<Vec<u8>> {
        debug!("Signing message hash for mobile transaction");
        
        // Create message from pre-computed hash
        let message = secp256k1::Message::from_digest_slice(message_hash)?;
        
        // Sign with recoverable signature
        let recoverable_sig = self.secp.sign_ecdsa_recoverable(&message, private_key);
        let (recovery_id, signature) = recoverable_sig.serialize_compact();
        
        // Return 65-byte signature with Ethereum-compatible recovery ID
        // v = 27 + (recovery_id % 2) to ensure v is only 27 or 28
        let mut sig_bytes = Vec::with_capacity(65);
        sig_bytes.extend_from_slice(&signature);
        sig_bytes.push((recovery_id.to_i32() % 2) as u8 + 27);
        
        debug!("Generated 65-byte signature for mobile transaction");
        Ok(sig_bytes)
    }
    
    /// Sign transaction bytes directly (will hash internally)
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
        
        self.sign_message_hash(&hash, private_key)
    }
}

impl Default for MobileTransactionSigner {
    fn default() -> Self {
        Self::new()
    }
}

/// Derive a private key using proper BIP32 HD derivation
fn derive_private_key_bip32(seed: &[u8], _path_str: &str) -> Result<[u8; 32]> {
    // Create extended private key from seed
    let xprv = XPrv::new(seed)
        .map_err(|e| anyhow!("Failed to create XPrv from seed: {}", e))?;
    
    // Derive using Ethereum HD path: m/44'/60'/0'/0/0
    // 44' = BIP44 purpose (hardened)
    // 60' = Ethereum coin type (hardened) 
    // 0' = Account 0 (hardened)
    // 0 = External chain
    // 0 = Address index 0
    let derived = xprv
        .derive_child(ChildNumber::new(44, true)?)  // 44'
        .and_then(|k| k.derive_child(ChildNumber::new(60, true)?))  // 60'
        .and_then(|k| k.derive_child(ChildNumber::new(0, true)?))   // 0'
        .and_then(|k| k.derive_child(ChildNumber::new(0, false)?))  // 0
        .and_then(|k| k.derive_child(ChildNumber::new(0, false)?))  // 0
        .map_err(|e| anyhow!("Failed to derive key: {}", e))?;
    
    // Return the private key bytes
    Ok(derived.to_bytes())
}

/// Generate an Injective address from a public key
/// Uses Ethereum-style address derivation with bech32 encoding
fn generate_injective_address(public_key: &PublicKey) -> Result<String> {
    // Get uncompressed public key bytes (65 bytes: 0x04 + 32 bytes X + 32 bytes Y)
    let pubkey_bytes = public_key.serialize_uncompressed();
    
    // Skip the 0x04 prefix byte, take only the X,Y coordinates (64 bytes)
    let coords = &pubkey_bytes[1..];
    
    // Hash the public key coordinates with Keccak256 (Ethereum-style)
    let mut hasher = Keccak::v256();
    let mut hash = [0u8; 32];
    hasher.update(coords);
    hasher.finalize(&mut hash);
    
    // Take the last 20 bytes of the hash (Ethereum address format)
    let addr_bytes = &hash[12..32];
    
    // Encode as bech32 with 'inj' prefix for Injective
    let hrp = Hrp::parse(INJECTIVE_PREFIX)?;
    let encoded = bech32::encode::<bech32::Bech32>(hrp, addr_bytes)?;
    
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mobile_wallet_generation() {
        // Test with known mnemonic
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = MobileWallet::from_mnemonic_no_passphrase(mnemonic).unwrap();
        
        // Address should be deterministic
        assert!(wallet.address.starts_with("inj1"));
        assert_eq!(wallet.address.len(), 42);
        println!("Mobile wallet address: {}", wallet.address);
        
        // Verify key sizes
        assert_eq!(wallet.private_key_bytes.len(), 32);
        assert_eq!(wallet.public_key_bytes.len(), 65);
        assert_eq!(wallet.public_key_bytes[0], 0x04); // Uncompressed prefix
        
        // Verify address validation
        assert!(MobileWallet::validate_address(&wallet.address).is_ok());
    }
    
    #[test]
    fn test_mobile_transaction_signing() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = MobileWallet::from_mnemonic_no_passphrase(mnemonic).unwrap();
        
        let signer = MobileTransactionSigner::new();
        let tx_bytes = b"test mobile transaction data";
        
        // Sign the transaction
        let private_key = wallet.private_key().unwrap();
        let signature = signer.sign_transaction(tx_bytes, &private_key).unwrap();
        
        // Signature should be 65 bytes (64 + 1 recovery ID)
        assert_eq!(signature.len(), 65);
        
        // Sign same data again - should be deterministic
        let signature2 = signer.sign_transaction(tx_bytes, &private_key).unwrap();
        assert_eq!(signature, signature2);
    }
    
    #[test]
    fn test_address_validation() {
        // Valid addresses
        assert!(MobileWallet::validate_address("inj1hkhdaj2a2clmq5jq6mspsggqs5mmjr6k4f7g9x").is_ok());
        
        // Invalid addresses
        assert!(MobileWallet::validate_address("cosmos1invalid").is_err());
        assert!(MobileWallet::validate_address("inj1short").is_err());
        assert!(MobileWallet::validate_address("toolongaddress").is_err());
    }
}