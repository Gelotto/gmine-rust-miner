/// Test program to verify contract queries work
/// Run with: cargo run --bin test_queries

use anyhow::Result;
use gmine_miner::chain::{
    InjectiveClient, ClientConfig, ContractAddresses,
    query_epoch_info,
    InjectiveWallet,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();
    
    println!("=== GMINE Contract Query Test ===\n");
    
    // Create a test wallet (won't be used for queries, but needed for client)
    let wallet = InjectiveWallet::from_mnemonic_no_passphrase(
        "test test test test test test test test test test test junk"
    )?;
    
    let wallet_address = wallet.address.clone();
    println!("Test wallet address: {}", wallet_address);
    
    // Get testnet contract addresses
    let contracts = ContractAddresses::testnet();
    println!("Mining contract: {}", contracts.mining_contract);
    println!("Power token: {}", contracts.power_token);
    println!();
    
    // Create client with testnet config
    // Use official Injective testnet gRPC endpoint with TLS
    let config = ClientConfig {
        grpc_endpoint: "https://testnet.sentry.chain.grpc.injective.network:443".to_string(),
        connection_timeout: 10,
        request_timeout: 30,
        max_retries: 3,
        chain_id: "injective-888".to_string(),
    };
    
    let mut client = InjectiveClient::new(config, wallet);
    
    // Connect to the chain
    println!("Connecting to Injective testnet...");
    client.connect().await?;
    println!("Connected!\n");
    
    // Test 1: Query epoch info
    println!("=== Testing Epoch Info Query ===");
    match query_epoch_info(&client, &contracts.mining_contract).await {
        Ok(info) => {
            println!("✅ Epoch query successful!");
            println!("  Epoch number: {}", info.epoch_number);
            println!("  Current phase: {:?}", info.phase);
            println!("  Difficulty: {}", info.difficulty);
            println!("  Reward pool: {}", info.reward_pool);
            println!("  Leading miner: {:?}", info.leading_miner);
            println!("  Best score: {:?}", info.best_score);
            println!("  Start block: {}", info.start_block);
        }
        Err(e) => {
            println!("❌ Epoch query failed: {}", e);
            println!("   This is expected if using mock data for now");
        }
    }
    println!();
    
    // Test 2: Query POWER token balance  
    println!("=== Testing POWER Token Balance ===");
    let miner_wallet = "inj1npvwllfr9dqr8erajqqr6s0vxnk2ak55re90dz"; // Our test miner wallet
    println!("Checking POWER balance for: {}", miner_wallet);
    
    // Query POWER token balance using contract query
    match client.query_contract_smart(
        &contracts.power_token,
        serde_json::to_vec(&serde_json::json!({
            "balance": {
                "address": miner_wallet
            }
        }))?
    ).await {
        Ok(balance_response) => {
            println!("✅ POWER token balance query successful!");
            println!("  Raw response: {}", balance_response);
            if let Some(balance) = balance_response.get("balance") {
                println!("  POWER Balance: {}", balance);
            }
        }
        Err(e) => {
            println!("❌ POWER token query failed: {}", e);
        }
    }
    println!();
    
    println!("=== Query Test Complete ===");
    println!("\nNext steps:");
    println!("1. Replace mock data in query_contract_smart() with real gRPC calls");
    println!("2. Implement proper CosmWasm query proto messages");
    println!("3. Test with actual contract on testnet");
    
    Ok(())
}