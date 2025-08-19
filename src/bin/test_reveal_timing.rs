/// Test reveal timing with safety buffer
/// This demonstrates how the safety buffer prevents late reveal submissions

use anyhow::Result;
use gmine_miner::chain::{InjectiveClient, query_epoch_info};
use gmine_miner::chain::wallet::InjectiveWallet;
use gmine_miner::chain::queries::PhaseInfo;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Create wallet from test mnemonic
    let mnemonic = std::env::var("MNEMONIC")
        .unwrap_or_else(|_| "test test test test test test test test test test test junk".to_string());
    let wallet = InjectiveWallet::from_mnemonic_no_passphrase(&mnemonic)?;
    
    log::info!("Testing reveal timing with safety buffer");

    // Create and connect client
    let mut client = InjectiveClient::new_testnet(wallet);
    client.connect().await?;
    log::info!("Connected to Injective testnet");
    
    // Test contract address
    let contract = "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y";
    
    // Different safety buffer values to test
    let safety_buffers = vec![1, 5, 8, 10];
    
    // Monitor for reveal phase
    loop {
        match query_epoch_info(&client, contract).await {
            Ok(info) => {
                match &info.phase {
                    PhaseInfo::Reveal { ends_at } => {
                        let current_block = client.get_block_height().await?;
                        let blocks_remaining = if *ends_at > current_block {
                            *ends_at - current_block
                        } else {
                            0
                        };
                        
                        log::info!("=== REVEAL PHASE for epoch {} ===", info.epoch_number);
                        log::info!("Current block: {}, Ends at: {}, Blocks remaining: {}", 
                                  current_block, ends_at, blocks_remaining);
                        
                        // Test different safety buffers
                        for buffer in &safety_buffers {
                            let would_submit = blocks_remaining >= *buffer;
                            log::info!("  Safety buffer {} blocks: {} submit reveal", 
                                      buffer, if would_submit { "WOULD" } else { "WOULD NOT" });
                        }
                        
                        // Show estimated transaction processing time
                        log::info!("  Estimated tx processing: ~4 seconds (~4 blocks)");
                        log::info!("  Recommendation: Use safety buffer of 8+ blocks");
                        
                        // Wait a bit before next check
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }
                    _ => {
                        // Not in reveal phase, wait
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to query epoch info: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}