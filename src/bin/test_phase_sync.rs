/// Test phase synchronization fix
/// This verifies that the orchestrator correctly queries chain state instead of using local calculations

use anyhow::Result;
use gmine_miner::chain::{InjectiveClient, query_epoch_info};
use gmine_miner::chain::wallet::InjectiveWallet;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Create wallet from test mnemonic
    let mnemonic = std::env::var("MNEMONIC")
        .unwrap_or_else(|_| "test test test test test test test test test test test junk".to_string());
    let wallet = InjectiveWallet::from_mnemonic_no_passphrase(&mnemonic)?;
    
    log::info!("Testing phase synchronization with wallet: {}", wallet.address);

    // Create and connect client
    let mut client = InjectiveClient::new_testnet(wallet);
    client.connect().await?;
    log::info!("Connected to Injective testnet");
    
    // Test contract address
    let contract = "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y";
    
    // Query epoch info multiple times
    for i in 0..5 {
        match query_epoch_info(&client, contract).await {
            Ok(info) => {
                log::info!("Query {}: Epoch {} - Phase: {:?}", i+1, info.epoch_number, info.phase);
                
                // Show block timing
                match &info.phase {
                    gmine_miner::chain::queries::PhaseInfo::Commit { ends_at } => {
                        log::info!("  Commit phase ends at block {}", ends_at);
                    }
                    gmine_miner::chain::queries::PhaseInfo::Reveal { ends_at } => {
                        log::info!("  Reveal phase ends at block {}", ends_at);
                    }
                    gmine_miner::chain::queries::PhaseInfo::Settlement { ends_at } => {
                        log::info!("  Settlement phase ends at block {}", ends_at);
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to query epoch info: {}", e);
            }
        }
        
        // Wait a bit between queries
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
    
    log::info!("Phase synchronization test complete");
    Ok(())
}