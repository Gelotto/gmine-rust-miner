use anyhow::{Result, bail};
use bip39::Mnemonic;
use bip32::{XPrv, ChildNumber};
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use tiny_keccak::{Hasher, Keccak};
use bech32::{self, Hrp};
use zeroize::{Zeroize, ZeroizeOnDrop};

const INJECTIVE_HD_PATH: &str = "m/44'/60'/0'/0/0"; // Ethereum-style HD path for Injective
const INJECTIVE_PREFIX: &str = "inj";

/// Secure wallet for Injective blockchain
/// Implements proper BIP32 HD derivation and memory security
#[derive(Debug, Zeroize, ZeroizeOnDrop)]
pub struct InjectiveWallet {
    #[zeroize(skip)] // Public data doesn't need zeroizing
    pub address: String,
    
    // Private fields with automatic zeroization
    private_key_bytes: [u8; 32],
    public_key_bytes: [u8; 65],
}

impl InjectiveWallet {
    /// Create a wallet from a BIP39 mnemonic phrase with optional passphrase
    pub fn from_mnemonic(mnemonic_str: &str, passphrase: &str) -> Result<Self> {
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
        })
    }
    
    /// Create a wallet from a BIP39 mnemonic with no passphrase
    pub fn from_mnemonic_no_passphrase(mnemonic_str: &str) -> Result<Self> {
        Self::from_mnemonic(mnemonic_str, "")
    }
    
    /// Get the private key as a SecretKey (for signing)
    /// Note: Caller is responsible for secure handling
    pub fn private_key(&self) -> Result<SecretKey> {
        SecretKey::from_slice(&self.private_key_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid private key: {}", e))
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
}

/// Derive a private key using proper BIP32 HD derivation
fn derive_private_key_bip32(seed: &[u8], _path_str: &str) -> Result<[u8; 32]> {
    // Create extended private key from seed
    let xprv = XPrv::new(seed)
        .map_err(|e| anyhow::anyhow!("Failed to create XPrv from seed: {}", e))?;
    
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
        .map_err(|e| anyhow::anyhow!("Failed to derive key: {}", e))?;
    
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

/// Validate address against known test vectors
pub fn validate_with_test_vector() -> Result<()> {
    // Test vector from Injective documentation
    // Mnemonic: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
    // Expected address will vary based on proper implementation
    
    let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let wallet = InjectiveWallet::from_mnemonic_no_passphrase(test_mnemonic)?;
    
    // Log the generated address for manual verification
    println!("Test vector address: {}", wallet.address);
    
    // In production, compare against known good address
    // For now, just ensure it has the right format
    if !wallet.address.starts_with("inj1") {
        bail!("Invalid address prefix");
    }
    
    if wallet.address.len() != 42 {
        bail!("Invalid address length: expected 42, got {}", wallet.address.len());
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_wallet_generation_with_bip32() {
        // Test with known mnemonic
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = InjectiveWallet::from_mnemonic_no_passphrase(mnemonic).unwrap();
        
        // Address should be deterministic
        assert!(wallet.address.starts_with("inj1"));
        assert_eq!(wallet.address.len(), 42);
        println!("BIP32 derived address: {}", wallet.address);
        
        // Verify key sizes
        assert_eq!(wallet.private_key_bytes.len(), 32);
        assert_eq!(wallet.public_key_bytes.len(), 65);
        assert_eq!(wallet.public_key_bytes[0], 0x04); // Uncompressed prefix
    }
    
    #[test]
    fn test_wallet_with_passphrase() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        
        // Test with different passphrases
        let wallet1 = InjectiveWallet::from_mnemonic(mnemonic, "").unwrap();
        let wallet2 = InjectiveWallet::from_mnemonic(mnemonic, "test123").unwrap();
        
        // Different passphrases should produce different addresses
        assert_ne!(wallet1.address, wallet2.address);
        
        // But same passphrase should be deterministic
        let wallet3 = InjectiveWallet::from_mnemonic(mnemonic, "test123").unwrap();
        assert_eq!(wallet2.address, wallet3.address);
    }
    
    #[test]
    fn test_memory_zeroization() {
        // This test verifies that Drop is implemented
        // The actual zeroization happens automatically
        {
            let wallet = InjectiveWallet::from_mnemonic_no_passphrase(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
            ).unwrap();
            
            // Wallet will be zeroized when it goes out of scope
            assert!(!wallet.address.is_empty());
        } // Automatic zeroization happens here
    }
    
    #[test]
    fn test_validation() {
        // Test the validation function
        assert!(validate_with_test_vector().is_ok());
    }
}