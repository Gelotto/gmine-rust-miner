/// GMINE Mining Client - Main Entry Point
/// 
/// This is the production mining client for the GMINE protocol on Injective.
/// It coordinates mining operations, manages epochs, and handles rewards.

use anyhow::{Result, anyhow};
use clap::Parser;
use std::path::PathBuf;
use gmine_miner::{
    chain::{InjectiveClient, ClientConfig, ContractAddresses, InjectiveWallet},
    orchestrator::{MiningOrchestrator, OrchestratorConfig},
};

#[derive(Parser, Debug)]
#[command(name = "gmine-miner-simple")]
#[command(about = "Simple GMINE Mining Client for Injective", long_about = None)]
struct Args {
    /// Mnemonic phrase for the wallet (or set MNEMONIC env var)
    #[arg(long, env = "MNEMONIC")]
    mnemonic: Option<String>,
    
    /// Path to mnemonic file
    #[arg(long, conflicts_with = "mnemonic")]
    mnemonic_file: Option<PathBuf>,
    
    /// Number of mining workers (CPU threads)
    #[arg(long, default_value = "4")]
    workers: usize,
    
    /// Network to use (mainnet or testnet)
    #[arg(long, default_value = "testnet")]
    network: String,
    
    /// gRPC endpoint override
    #[arg(long)]
    grpc_endpoint: Option<String>,
    
    /// State file path for crash recovery
    #[arg(long, default_value = "gmine_miner.state")]
    state_file: PathBuf,
    
    /// Enable debug logging
    #[arg(long)]
    debug: bool,
    
    /// Use Rust-native EIP-712 signer instead of Node.js bridge (experimental)
    #[arg(long)]
    use_rust_signer: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize logging
    if args.debug {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }
    
    log::info!("=== GMINE Mining Client v0.1.0 ===");
    log::info!("Network: {}", args.network);
    log::info!("Workers: {}", args.workers);
    
    // Load wallet
    let mnemonic = if let Some(mnemonic) = args.mnemonic {
        mnemonic
    } else if let Some(path) = args.mnemonic_file {
        std::fs::read_to_string(path)?
            .trim()
            .to_string()
    } else {
        return Err(anyhow!("No mnemonic provided. Use --mnemonic or --mnemonic-file"));
    };
    
    let wallet = InjectiveWallet::from_mnemonic_no_passphrase(&mnemonic)?;
    log::info!("Wallet address: {}", wallet.address);
    
    // Note: Telemetry is handled internally by the orchestrator
    log::info!("Telemetry will be initialized by orchestrator...");
    
    // Configure client
    let client_config = if args.network == "mainnet" {
        ClientConfig {
            grpc_endpoint: args.grpc_endpoint.unwrap_or_else(|| 
                "https://sentry.chain.grpc.injective.network:443".to_string()
            ),
            connection_timeout: 10,
            request_timeout: 30,
            max_retries: 3,
            chain_id: "injective-1".to_string(),
        }
    } else {
        ClientConfig {
            grpc_endpoint: args.grpc_endpoint.unwrap_or_else(|| 
                "https://testnet.sentry.chain.grpc.injective.network:443".to_string()
            ),
            connection_timeout: 10,
            request_timeout: 30,
            max_retries: 3,
            chain_id: "injective-888".to_string(),
        }
    };
    
    // Create client (wallet will be moved)
    let wallet_for_client = InjectiveWallet::from_mnemonic_no_passphrase(&mnemonic)?;
    let mut client = InjectiveClient::new(client_config, wallet_for_client);
    
    // Connect to chain
    log::info!("Connecting to Injective...");
    client.connect().await?;
    log::info!("Connected successfully!");
    
    // Get contract addresses
    let contracts = if args.network == "mainnet" {
        // TODO: Add mainnet addresses when deployed
        return Err(anyhow!("Mainnet not yet supported"));
    } else {
        ContractAddresses::testnet()
    };
    
    log::info!("Mining contract: {}", contracts.mining_contract);
    log::info!("Power token: {}", contracts.power_token);
    
    // Set up EIP-712 signing
    if args.use_rust_signer {
        log::info!("Using Rust-native EIP-712 signer (experimental)...");
        
        // Enable Rust signer
        client.enable_rust_signer(&mnemonic, &contracts.mining_contract)?;
        log::info!("Rust-native EIP-712 signer enabled successfully!");
    } else {
        log::info!("Setting up EIP-712 bridge for Injective compatibility...");
        let mut bridge_manager = gmine_miner::BridgeManager::new(
            mnemonic.clone(),
            args.network.clone()
        )?;
        
        // Start bridge service
        bridge_manager.start()?;
        
        // Create bridge client
        let bridge_client = gmine_miner::chain::bridge_client::BridgeClient::new(
            bridge_manager.get_url(),
            Some(bridge_manager.get_api_key())
        );
        
        // Wait for bridge to be healthy
        log::info!("Waiting for bridge service to be healthy...");
        bridge_manager.ensure_healthy().await?;
        
        // Set bridge client in the InjectiveClient
        client.set_bridge_client(bridge_client);
        log::info!("EIP-712 bridge configured successfully!");
    }
    
    // Configure orchestrator
    let orchestrator_config = OrchestratorConfig {
        state_file: args.state_file,
        epoch_poll_interval: 5,
        reveal_wait_interval: 10,
        max_retries: 3,
        retry_delay_ms: 1000,
        contract_address: contracts.mining_contract.clone(),
        worker_count: args.workers,
    };
    
    // Create and run orchestrator
    let mut orchestrator = MiningOrchestrator::new(
        orchestrator_config,
        client,
        wallet,
    ).await?;
    
    log::info!("Starting mining orchestrator...");
    log::info!("Press Ctrl+C to stop mining");
    
    // Set up graceful shutdown
    let shutdown = tokio::signal::ctrl_c();
    
    // Run mining loop
    tokio::select! {
        result = orchestrator.run() => {
            match result {
                Ok(_) => log::info!("Mining completed successfully"),
                Err(e) => log::error!("Mining error: {}", e),
            }
        }
        _ = shutdown => {
            log::info!("Shutdown signal received, stopping mining...");
        }
    }
    
    log::info!("Mining stopped");
    Ok(())
}