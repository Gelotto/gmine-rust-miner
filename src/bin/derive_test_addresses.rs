/// Derive addresses for all 6 test wallets
use anyhow::Result;
use gmine_miner::chain::wallet::InjectiveWallet;

fn main() -> Result<()> {
    println!("=== Deriving GMINE Test Wallet Addresses ===\n");
    
    let test_wallets = vec![
        ("Miner-1", "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"),
        ("Miner-2", "test test test test test test test test test test test junk"),
        ("Miner-3", "word word word word word word word word word word word word"),
        ("Miner-4", "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong"),
        ("Miner-5", "all all all all all all all all all all all all"),
        ("Miner-6", "void come effort suffer camp survey warrior heavy shoot primary clutch crush open amazing screen patrol group space point ten exist slush involve unfold"),
    ];
    
    println!("# Copy these addresses to your wallet_config.env:\n");
    
    for (name, mnemonic) in test_wallets {
        match InjectiveWallet::from_mnemonic_no_passphrase(mnemonic) {
            Ok(wallet) => {
                println!("{}: {}", name, wallet.address);
            }
            Err(e) => {
                println!("{}: ERROR - {}", name, e);
            }
        }
    }
    
    println!("\n# Funding Instructions:");
    println!("# 1. Join Injective Discord: https://discord.gg/injective");
    println!("# 2. Go to #testnet-faucet channel");
    println!("# 3. Request 10 INJ for each address");
    println!("# 4. Wait 1 minute between requests to avoid rate limiting");
    println!("\n# Total needed: 60 INJ (10 per wallet)");
    
    Ok(())
}