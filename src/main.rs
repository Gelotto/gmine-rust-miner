mod config;
mod miner;
mod chain;
mod orchestrator;
mod telemetry;
mod bridge_manager;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::time::Duration;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Constants for validation
const MIN_DIFFICULTY: u8 = 6;
const MAX_DIFFICULTY: u8 = 32;
const MAX_THREADS: usize = 256;
const MAX_DURATION: u64 = 86400; // 24 hours

#[derive(Parser)]
#[command(name = "gmine-miner")]
#[command(about = "GMINE Mining Client for Injective", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Test mining locally
    Test {
        /// Number of threads to use
        #[arg(short, long, default_value = "1")]
        threads: usize,
        
        /// Difficulty target
        #[arg(short, long, default_value = "8")]
        difficulty: u8,
        
        /// Mining duration in seconds
        #[arg(long, default_value = "60")]
        duration: u64,
    },
    
    /// Run the full miner
    Mine {
        /// Configuration file path
        #[arg(short, long, default_value = "config.toml")]
        config: String,
    },
    
    /// Generate a default configuration file
    Init {
        /// Output path for config file
        #[arg(short, long, default_value = "config.toml")]
        output: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gmine_miner=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Test { threads, difficulty, duration } => {
            run_test(threads, difficulty, duration).await?;
        }
        Commands::Mine { config: _ } => {
            info!("Full mining mode not yet implemented. Use 'test' command for now.");
        }
        Commands::Init { output } => {
            let config = config::Config::default();
            config.save(&output)?;
            info!("Configuration file created at: {}", output);
        }
    }

    Ok(())
}

async fn run_test(threads: usize, difficulty: u8, duration: u64) -> Result<()> {
    // Validate inputs
    if difficulty < MIN_DIFFICULTY || difficulty > MAX_DIFFICULTY {
        bail!("Difficulty must be between {} and {}", MIN_DIFFICULTY, MAX_DIFFICULTY);
    }
    if threads == 0 || threads > MAX_THREADS {
        bail!("Thread count must be between 1 and {}", MAX_THREADS);
    }
    if duration == 0 || duration > MAX_DURATION {
        bail!("Duration must be between 1 and {} seconds", MAX_DURATION);
    }
    
    info!("Starting mining test");
    info!("Threads: {}", threads);
    info!("Difficulty: {}", difficulty);
    info!("Duration: {} seconds", duration);
    
    let mut engine = miner::MiningEngine::new(threads);
    
    // Start mining with a large nonce range
    let nonce_start = 0u64;
    let nonce_end = u64::MAX / 1000; // Use 1/1000th of total range
    let test_epoch = 1; // Test epoch number
    
    engine.start_mining(test_epoch, difficulty, (nonce_start, nonce_end)).await?;
    
    // Poll for solution
    let start_time = std::time::Instant::now();
    let timeout = Duration::from_secs(duration);
    
    loop {
        if start_time.elapsed() > timeout {
            info!("No solution found within {} seconds", duration);
            info!("Final hashrate: {:.2} H/s", engine.get_hashrate().await);
            break;
        }
        
        if let Some(commitment_data) = engine.check_solution().await {
            info!("=== SOLUTION FOUND ===");
            info!("Epoch: {}", commitment_data.epoch);
            info!("Nonce: {}", hex::encode(&commitment_data.nonce));
            info!("Digest: {}", hex::encode(&commitment_data.digest));
            info!("Commitment: {}", hex::encode(&commitment_data.commitment));
            info!("Time taken: {} ms", start_time.elapsed().as_millis());
            break;
        }
        
        // Small delay to prevent busy waiting
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    engine.stop_mining().await?;
    info!("Mining test complete");
    
    Ok(())
}
