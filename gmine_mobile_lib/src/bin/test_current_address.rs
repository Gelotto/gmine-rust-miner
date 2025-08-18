fn main() {
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    
    println!("Testing wallet address derivation...");
    println!("Mnemonic: {}", mnemonic);
    
    // Import the actual wallet being used
    use gmine_mobile::mobile_wallet::MobileWallet as Wallet;
    
    match Wallet::from_mnemonic_no_passphrase(mnemonic) {
        Ok(wallet) => {
            println!("Address from current Rust lib: {}", wallet.address);
            println!("Expected address: inj1npvwllfr9dqr8erajqqr6s0vxnk2ak55re90dz");
            
            if wallet.address == "inj1npvwllfr9dqr8erajqqr6s0vxnk2ak55re90dz" {
                println!("✅ CORRECT: Using Ethereum-style derivation");
            } else {
                println!("❌ INCORRECT: Wrong address derivation!");
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}