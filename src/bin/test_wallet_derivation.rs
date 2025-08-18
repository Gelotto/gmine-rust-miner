use gmine_mobile::mobile_wallet::MobileWallet;
use gmine_mobile::wallet::Wallet;

fn main() {
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    
    println!("=== Testing Wallet Address Derivation ===\n");
    
    // Test regular wallet (Cosmos-style)
    match Wallet::from_mnemonic(mnemonic) {
        Ok(wallet) => {
            println!("Cosmos-style wallet derivation:");
            println!("Address: {}", wallet.address);
            println!("Public key (hex): {}", hex::encode(&wallet.public_key));
            println!("Public key length: {} bytes", wallet.public_key.len());
        }
        Err(e) => {
            eprintln!("Error with Cosmos wallet: {}", e);
        }
    }
    
    println!("\n---\n");
    
    // Test mobile wallet (Ethereum-style for Injective)
    match MobileWallet::from_mnemonic_no_passphrase(mnemonic) {
        Ok(wallet) => {
            println!("Ethereum-style wallet derivation (for Injective):");
            println!("Address: {}", wallet.address);
            let pub_key_bytes = wallet.public_key_bytes();
            println!("Public key (hex): {}", hex::encode(&pub_key_bytes));
            println!("Public key length: {} bytes", pub_key_bytes.len());
            println!("\nExpected address: inj1npvwllfr9dqr8erajqqr6s0vxnk2ak55re90dz");
        }
        Err(e) => {
            eprintln!("Error with Mobile wallet: {}", e);
        }
    }
}