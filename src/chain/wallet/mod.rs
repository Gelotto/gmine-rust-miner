mod keys;
mod signer;

pub use keys::InjectiveWallet;
pub use signer::TransactionSigner;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_wallet_generation() {
        // Test mnemonic from BIP39 spec
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = InjectiveWallet::from_mnemonic_no_passphrase(mnemonic).unwrap();
        
        // Verify address format
        assert!(wallet.address.starts_with("inj1"));
        assert_eq!(wallet.address.len(), 42); // Standard Injective address length
        
        println!("Generated address: {}", wallet.address);
    }
    
    #[test]
    fn test_deterministic_generation() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        
        // Generate wallet twice from same mnemonic with same passphrase
        let wallet1 = InjectiveWallet::from_mnemonic(mnemonic, "").unwrap();
        let wallet2 = InjectiveWallet::from_mnemonic(mnemonic, "").unwrap();
        
        // Should produce identical results
        assert_eq!(wallet1.address, wallet2.address);
        
        // Test with passphrase
        let wallet3 = InjectiveWallet::from_mnemonic(mnemonic, "mypass").unwrap();
        let wallet4 = InjectiveWallet::from_mnemonic(mnemonic, "mypass").unwrap();
        assert_eq!(wallet3.address, wallet4.address);
        
        // Different passphrases should give different addresses
        assert_ne!(wallet1.address, wallet3.address);
    }
}