use gmine_mobile::mobile_wallet::MobileWallet;
use gmine_mobile::wallet::Wallet;

fn main() {
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    
    println!("Testing address derivation from mnemonic:");
    println!("Mnemonic: {}", mnemonic);
    println!("");
    
    // Test the old wallet (incorrect)
    match Wallet::from_mnemonic(mnemonic) {
        Ok(wallet) => {
            println!("Old wallet (SHA256->RIPEMD160 - INCORRECT):");
            println!("  Address: {}", wallet.address);
        }
        Err(e) => {
            eprintln!("  Error: {}", e);
        }
    }
    
    println!("");
    
    // Test the mobile wallet (correct)
    match MobileWallet::from_mnemonic_no_passphrase(mnemonic) {
        Ok(wallet) => {
            println!("Mobile wallet (Keccak256 - CORRECT):");
            println!("  Address: {}", wallet.address);
            println!("  Private key length: {} bytes", wallet.private_key_bytes().len());
            println!("  Public key length: {} bytes", wallet.public_key_bytes_ref().len());
        }
        Err(e) => {
            eprintln!("  Error: {}", e);
        }
    }
    
    println!("");
    println!("Expected address (from Node.js bridge): inj1npvwllfr9dqr8erajqqr6s0vxnk2ak55re90dz");
}