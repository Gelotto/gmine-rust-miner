use bip32::{XPrv, DerivationPath};
use bip39::Mnemonic;
use secp256k1::{Secp256k1, PublicKey};
use sha2::{Sha256, Digest};
use ripemd::Ripemd160;
use bech32::{self, Hrp};
use std::str::FromStr;

pub struct Wallet {
    pub address: String,
    pub private_key: Vec<u8>,
    pub public_key: Vec<u8>,
}

impl Wallet {
    /// Generate a new random mnemonic phrase
    pub fn generate_mnemonic() -> Result<String, Box<dyn std::error::Error>> {
        // Generate 128 bits of entropy for 12-word mnemonic
        let mut entropy = [0u8; 16];
        getrandom::getrandom(&mut entropy)?;
        let mnemonic = Mnemonic::from_entropy(&entropy)?;
        Ok(mnemonic.to_string())
    }
    
    /// Validate a mnemonic phrase
    pub fn validate_mnemonic(phrase: &str) -> bool {
        Mnemonic::from_str(phrase).is_ok()
    }
    
    /// Derive Injective wallet from mnemonic
    pub fn from_mnemonic(mnemonic_phrase: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Parse mnemonic
        let mnemonic = Mnemonic::from_str(mnemonic_phrase)?;
        let seed = mnemonic.to_seed("");
        
        // Use Cosmos HD derivation path: m/44'/60'/0'/0/0
        // Note: Injective uses Ethereum's coin type (60) for compatibility
        let path = DerivationPath::from_str("m/44'/60'/0'/0/0")?;
        
        // Derive private key
        let xprv = XPrv::derive_from_path(&seed[..], &path)?;
        let private_key = xprv.to_bytes();
        
        // Get public key
        let secp = Secp256k1::new();
        let secret_key = secp256k1::SecretKey::from_slice(&private_key)?;
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let public_key_bytes = public_key.serialize();
        
        // Derive Injective address (bech32 with "inj" prefix)
        let address = Self::derive_injective_address(&public_key_bytes)?;
        
        Ok(Wallet {
            address,
            private_key: private_key.to_vec(),
            public_key: public_key_bytes.to_vec(),
        })
    }
    
    /// Derive Injective bech32 address from public key
    fn derive_injective_address(public_key: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
        // Hash public key: SHA256 then RIPEMD160
        let sha256_hash = Sha256::digest(public_key);
        let ripemd160_hash = Ripemd160::digest(&sha256_hash);
        
        // Convert to bech32 with "inj" prefix
        let hrp = Hrp::parse("inj")?;
        let address = bech32::encode::<bech32::Bech32>(hrp, &ripemd160_hash)?;
        
        Ok(address)
    }
    
    /// Get ethereum-style address (for EIP-712 compatibility)
    pub fn get_eth_address(&self) -> String {
        // Keccak256 hash of public key (excluding first byte)
        use sha3::{Digest as Sha3Digest, Keccak256};
        let mut hasher = Keccak256::new();
        hasher.update(&self.public_key[1..]); // Skip first byte (0x04)
        let hash = hasher.finalize();
        
        // Take last 20 bytes
        let eth_address = &hash[12..];
        format!("0x{}", hex::encode(eth_address))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_wallet_derivation() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = Wallet::from_mnemonic(mnemonic).unwrap();
        
        // Verify address starts with "inj"
        assert!(wallet.address.starts_with("inj"));
        
        // Verify keys are correct length
        assert_eq!(wallet.private_key.len(), 32);
        assert_eq!(wallet.public_key.len(), 33); // Compressed public key
    }
    
    #[test]
    fn test_mnemonic_validation() {
        assert!(Wallet::validate_mnemonic("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"));
        assert!(!Wallet::validate_mnemonic("invalid mnemonic phrase"));
    }
}