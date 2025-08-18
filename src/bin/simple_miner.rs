/// GMINE Mining Client - Main Entry Point
/// 
/// This is the production mining client for the GMINE protocol on Injective.
/// It coordinates mining operations, manages epochs, and handles rewards.

use anyhow::{Result, anyhow, Context};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use gmine_miner::{
    chain::{InjectiveClient, ClientConfig, ContractAddresses, InjectiveWallet},
    orchestrator::{MiningOrchestrator, OrchestratorConfig},
};
use dialoguer::{Input, Password, Confirm};
use serde::{Deserialize, Serialize};
use bip39::Mnemonic;

/// Main CLI structure with subcommands and backward compatibility
#[derive(Parser, Debug)]
#[command(name = "gmine")]
#[command(version)]
#[command(about = "GMINE Mining Client for Injective", long_about = None)]
struct Cli {
    /// Subcommand to run
    #[command(subcommand)]
    command: Option<Commands>,
    
    /// Arguments for the default 'mine' command (backward compatibility)
    #[command(flatten)]
    mine_args: MineArgs,
}

/// Available subcommands
#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize miner configuration interactively
    Init,
    
    /// Run the miner (default behavior)
    Mine(MineArgs),
    
    /// Manage miner as a system service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    
    /// View miner logs
    Logs {
        /// Number of lines to show (like tail -n)
        #[arg(short = 'n', long, default_value = "50")]
        lines: usize,
        
        /// Follow log output (like tail -f)
        #[arg(short = 'f', long)]
        follow: bool,
    },
    
    /// Show mining status and statistics
    Status,
}

/// Service management subcommands
#[derive(Subcommand, Debug)]
enum ServiceAction {
    /// Install as system service
    Install,
    
    /// Start the mining service
    Start,
    
    /// Stop the mining service
    Stop,
    
    /// Check service status
    Status,
    
    /// Uninstall system service
    Uninstall,
}

/// Arguments for mining (used by both explicit 'mine' command and default behavior)
#[derive(Args, Debug, Clone)]
struct MineArgs {
    /// Mnemonic phrase for the wallet (or set MNEMONIC env var)
    #[arg(long, env = "MNEMONIC")]
    mnemonic: Option<String>,
    
    /// Path to mnemonic file
    #[arg(long, conflicts_with = "mnemonic")]
    mnemonic_file: Option<PathBuf>,
    
    /// Number of mining workers (CPU threads)
    #[arg(long)]
    workers: Option<usize>,
    
    /// Network to use (mainnet or testnet)
    #[arg(long)]
    network: Option<String>,
    
    /// gRPC endpoint override
    #[arg(long)]
    grpc_endpoint: Option<String>,
    
    /// State file path for crash recovery
    #[arg(long)]
    state_file: Option<PathBuf>,
    
    /// Enable debug logging
    #[arg(long)]
    debug: bool,
    
    /// Use Rust-native EIP-712 signer instead of Node.js bridge (experimental)
    #[arg(long)]
    use_rust_signer: bool,
}

/// Configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MinerConfig {
    #[serde(default)]
    mining: MiningConfig,
    
    #[serde(default)]
    telemetry: TelemetryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MiningConfig {
    mnemonic: Option<String>,
    workers: Option<usize>,
    network: String,
    grpc_endpoint: Option<String>,
    state_file: Option<String>,
    use_rust_signer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TelemetryConfig {
    enabled: bool,
}

impl Default for MiningConfig {
    fn default() -> Self {
        Self {
            mnemonic: None,
            workers: None,
            network: "testnet".to_string(),
            grpc_endpoint: None,
            state_file: None,
            use_rust_signer: false,
        }
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Handle commands
    match cli.command {
        Some(Commands::Init) => cmd_init().await,
        Some(Commands::Mine(args)) => cmd_mine(args).await,
        Some(Commands::Service { action }) => cmd_service(action).await,
        Some(Commands::Logs { lines, follow }) => cmd_logs(lines, follow).await,
        Some(Commands::Status) => cmd_status().await,
        None => {
            // No subcommand provided - run mining with backward compatibility
            cmd_mine(cli.mine_args).await
        }
    }
}

/// Initialize configuration interactively
async fn cmd_init() -> Result<()> {
    println!("🚀 GMINE Miner Setup Wizard\n");
    
    let config_dir = get_config_dir()?;
    let config_path = config_dir.join("config.toml");
    
    // Check if config already exists
    if config_path.exists() {
        let overwrite = Confirm::new()
            .with_prompt("Configuration already exists. Overwrite?")
            .default(false)
            .interact()?;
        
        if !overwrite {
            println!("Setup cancelled.");
            return Ok(());
        }
    }
    
    // Get mnemonic
    println!("\n📝 Wallet Setup");
    println!("Enter your wallet mnemonic phrase (12 or 24 words)");
    println!("⚠️  WARNING: Use a NEW wallet for mining, not your main wallet!");
    
    let mnemonic_str: String = Password::new()
        .with_prompt("Mnemonic")
        .interact()?;
    
    // Validate mnemonic using BIP39
    match Mnemonic::parse(&mnemonic_str) {
        Ok(_) => {
            // Valid mnemonic
        }
        Err(_) => {
            return Err(anyhow!(
                "Invalid mnemonic phrase. Please check for typos and ensure it is a valid BIP39 12 or 24-word phrase."
            ));
        }
    }
    
    // Derive and show wallet address
    let wallet = InjectiveWallet::from_mnemonic_no_passphrase(&mnemonic_str)?;
    println!("\n✅ Wallet address: {}", wallet.address);
    
    // Get CPU info and suggest workers
    let cpu_count = num_cpus::get();
    let suggested_workers = if cpu_count > 1 { cpu_count - 1 } else { 1 };
    
    println!("\n💻 System Configuration");
    println!("Detected {} CPU cores", cpu_count);
    
    let workers: usize = Input::new()
        .with_prompt("Number of mining workers")
        .default(suggested_workers)
        .interact()?;
    
    // Network selection
    println!("\n🌐 Network Selection");
    let network: String = Input::new()
        .with_prompt("Network (testnet/mainnet)")
        .default("testnet".to_string())
        .validate_with(|input: &String| {
            if input == "testnet" || input == "mainnet" {
                Ok(())
            } else {
                Err("Must be 'testnet' or 'mainnet'")
            }
        })
        .interact()?;
    
    if network == "mainnet" {
        println!("⚠️  Mainnet mining not yet available. Configuration will be saved for future use.");
    }
    
    // Rust signer option
    let use_rust_signer = Confirm::new()
        .with_prompt("Use experimental Rust EIP-712 signer? (Recommended for 24/7 mining)")
        .default(true)
        .interact()?;
    
    // Create config
    let config = MinerConfig {
        mining: MiningConfig {
            mnemonic: Some(mnemonic_str),
            workers: Some(workers),
            network: network.clone(),
            grpc_endpoint: None,
            state_file: Some("gmine_miner.state".to_string()),
            use_rust_signer,
        },
        telemetry: TelemetryConfig {
            enabled: true,
        },
    };
    
    // Save config
    fs::create_dir_all(&config_dir)?;
    let toml = toml::to_string_pretty(&config)?;
    fs::write(&config_path, toml)?;
    
    // Set permissions to 0600 (owner read/write only)
    #[cfg(unix)]
    {
        let metadata = fs::metadata(&config_path)?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&config_path, permissions)?;
    }
    
    println!("\n✅ Configuration saved to: {}", config_path.display());
    
    // Show next steps
    println!("\n📋 Next Steps:");
    println!("1. Get testnet INJ tokens: https://testnet.faucet.injective.network/");
    println!("2. Start mining: gmine mine");
    println!("3. Install as service: gmine service install");
    
    Ok(())
}

/// Run the miner
async fn cmd_mine(args: MineArgs) -> Result<()> {
    // Load config file first (lowest priority)
    let config_path = get_config_dir()?.join("config.toml");
    let mut config = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .context("Failed to read config file")?;
        toml::from_str::<MinerConfig>(&content)
            .context("Failed to parse config file")?
    } else {
        MinerConfig {
            mining: MiningConfig::default(),
            telemetry: TelemetryConfig::default(),
        }
    };
    
    // Override with command line args (highest priority)
    if args.mnemonic.is_some() {
        config.mining.mnemonic = args.mnemonic;
    }
    if args.workers.is_some() {
        config.mining.workers = args.workers;
    }
    if args.network.is_some() {
        config.mining.network = args.network.unwrap();
    }
    if args.grpc_endpoint.is_some() {
        config.mining.grpc_endpoint = args.grpc_endpoint;
    }
    if args.state_file.is_some() {
        config.mining.state_file = Some(args.state_file.unwrap().to_string_lossy().to_string());
    }
    if args.use_rust_signer {
        config.mining.use_rust_signer = true;
    }
    
    // Initialize logging
    if args.debug {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }
    
    log::info!("=== GMINE Mining Client v0.1.0 ===");
    log::info!("Network: {}", config.mining.network);
    
    // Load wallet
    let mnemonic = if let Some(mnemonic) = config.mining.mnemonic {
        mnemonic
    } else if let Some(path) = args.mnemonic_file {
        fs::read_to_string(path)?
            .trim()
            .to_string()
    } else {
        return Err(anyhow!("No mnemonic provided. Use --mnemonic, --mnemonic-file, or run 'gmine init'"));
    };
    
    let wallet = InjectiveWallet::from_mnemonic_no_passphrase(&mnemonic)?;
    log::info!("Wallet address: {}", wallet.address);
    
    // Get workers count
    let workers = config.mining.workers.unwrap_or_else(|| {
        let cpu_count = num_cpus::get();
        if cpu_count > 1 { cpu_count - 1 } else { 1 }
    });
    log::info!("Workers: {}", workers);
    
    // Configure client
    let client_config = if config.mining.network == "mainnet" {
        ClientConfig {
            grpc_endpoint: config.mining.grpc_endpoint.unwrap_or_else(|| 
                "https://sentry.chain.grpc.injective.network:443".to_string()
            ),
            connection_timeout: 10,
            request_timeout: 30,
            max_retries: 3,
            chain_id: "injective-1".to_string(),
        }
    } else {
        ClientConfig {
            grpc_endpoint: config.mining.grpc_endpoint.unwrap_or_else(|| 
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
    let contracts = if config.mining.network == "mainnet" {
        // TODO: Add mainnet addresses when deployed
        return Err(anyhow!("Mainnet not yet supported"));
    } else {
        ContractAddresses::testnet()
    };
    
    log::info!("Mining contract: {}", contracts.mining_contract);
    log::info!("Power token: {}", contracts.power_token);
    
    // Set up EIP-712 signing
    if config.mining.use_rust_signer {
        log::info!("Using Rust-native EIP-712 signer...");
        client.enable_rust_signer(&mnemonic, &contracts.mining_contract)?;
        log::info!("Rust-native EIP-712 signer enabled successfully!");
    } else {
        log::info!("Setting up EIP-712 bridge for Injective compatibility...");
        let mut bridge_manager = gmine_miner::BridgeManager::new(
            mnemonic.clone(),
            config.mining.network.clone()
        )?;
        
        bridge_manager.start()?;
        
        let bridge_client = gmine_miner::chain::bridge_client::BridgeClient::new(
            bridge_manager.get_url(),
            Some(bridge_manager.get_api_key())
        );
        
        log::info!("Waiting for bridge service to be healthy...");
        bridge_manager.ensure_healthy().await?;
        
        client.set_bridge_client(bridge_client);
        log::info!("EIP-712 bridge configured successfully!");
    }
    
    // Configure orchestrator
    let state_file = config.mining.state_file
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("gmine_miner.state"));
    
    let orchestrator_config = OrchestratorConfig {
        state_file,
        epoch_poll_interval: 5,
        reveal_wait_interval: 10,
        max_retries: 3,
        retry_delay_ms: 1000,
        contract_address: contracts.mining_contract.clone(),
        worker_count: workers,
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

/// Manage service
async fn cmd_service(action: ServiceAction) -> Result<()> {
    match action {
        ServiceAction::Install => service_install().await,
        ServiceAction::Start => service_start().await,
        ServiceAction::Stop => service_stop().await,
        ServiceAction::Status => service_status().await,
        ServiceAction::Uninstall => service_uninstall().await,
    }
}

/// View logs
async fn cmd_logs(lines: usize, follow: bool) -> Result<()> {
    let log_path = get_config_dir()?.join("miner.log");
    
    if !log_path.exists() {
        println!("No log file found. The miner may not have been started as a service.");
        return Ok(());
    }
    
    // Use tail command for simplicity
    let mut cmd = std::process::Command::new("tail");
    cmd.arg(format!("-n{}", lines));
    if follow {
        cmd.arg("-f");
    }
    cmd.arg(log_path);
    
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow!("Failed to read logs"));
    }
    
    Ok(())
}

/// Show mining status
async fn cmd_status() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        // Check systemd service status
        let output = std::process::Command::new("systemctl")
            .args(&["--user", "is-active", "gmine-miner.service"])
            .output()?;
        
        let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
        
        match status.as_str() {
            "active" => {
                println!("✅ Miner service is running");
                
                // Get more details
                let _ = std::process::Command::new("systemctl")
                    .args(&["--user", "status", "gmine-miner.service", "--no-pager", "-n", "10"])
                    .status();
            }
            "inactive" => {
                println!("❌ Miner service is not running");
                println!("\nTo start: gmine service start");
            }
            "failed" => {
                println!("❌ Miner service has failed");
                println!("\nCheck logs: gmine logs");
                println!("To restart: gmine service stop && gmine service start");
            }
            _ => {
                println!("⚠️  Miner service is not installed");
                println!("\nTo install: gmine service install");
            }
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        // Check launchd service status
        let output = std::process::Command::new("launchctl")
            .args(&["list", "com.gelotto.gmine-miner"])
            .output();
        
        match output {
            Ok(output) if output.status.success() => {
                let status_line = String::from_utf8_lossy(&output.stdout);
                // Parse PID from output (format: "PID Status Label")
                let parts: Vec<&str> = status_line.trim().split_whitespace().collect();
                
                if parts.len() >= 3 {
                    let pid = parts[0];
                    if pid != "-" {
                        println!("✅ Miner service is running (PID: {})", pid);
                    } else {
                        println!("❌ Miner service is loaded but not running");
                        println!("\nTo start: gmine service start");
                    }
                } else {
                    println!("⚠️  Could not parse service status");
                }
                
                // Show recent logs
                println!("\nRecent activity:");
                let _ = cmd_logs(10, false).await;
            }
            _ => {
                println!("⚠️  Miner service is not installed");
                println!("\nTo install: gmine service install");
            }
        }
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        println!("Service status not supported on this platform");
    }
    
    Ok(())
}

/// Service installation
async fn service_install() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        service_install_systemd().await
    }
    
    #[cfg(target_os = "macos")]
    {
        service_install_launchd().await
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(anyhow!("Service installation not supported on this platform"))
    }
}

#[cfg(target_os = "linux")]
async fn service_install_systemd() -> Result<()> {
    println!("Installing systemd service...");
    
    // Check if running as root
    if std::env::var("USER").unwrap_or_default() == "root" {
        return Err(anyhow!("Please run as a regular user, not root"));
    }
    
    // Use canonical installation path
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not find home directory"))?;
    let exe_path = home_dir.join(".gmine/bin/gmine");
    
    // Verify binary exists at expected location
    if !exe_path.exists() {
        return Err(anyhow!(
            "GMINE binary not found at {}. Please ensure it is installed correctly using the install script.",
            exe_path.display()
        ));
    }
    
    let service_content = format!(
        r#"[Unit]
Description=GMINE Miner
After=network.target

[Service]
Type=simple
ExecStart={} mine
Restart=always
RestartSec=30
User={}
StandardOutput=append:{}/.gmine/miner.log
StandardError=append:{}/.gmine/miner.log

[Install]
WantedBy=default.target
"#,
        exe_path.display(),
        std::env::var("USER")?,
        std::env::var("HOME")?,
        std::env::var("HOME")?
    );
    
    let service_dir = dirs::config_dir()
        .ok_or_else(|| anyhow!("Could not find config directory"))?
        .join("systemd/user");
    
    fs::create_dir_all(&service_dir)?;
    
    let service_path = service_dir.join("gmine-miner.service");
    fs::write(&service_path, service_content)?;
    
    // Reload systemd
    std::process::Command::new("systemctl")
        .args(&["--user", "daemon-reload"])
        .status()?;
    
    // Enable service
    std::process::Command::new("systemctl")
        .args(&["--user", "enable", "gmine-miner.service"])
        .status()?;
    
    println!("✅ Service installed successfully!");
    println!("\nTo start mining: gmine service start");
    println!("To check status: gmine service status");
    
    Ok(())
}

#[cfg(target_os = "macos")]
async fn service_install_launchd() -> Result<()> {
    println!("Installing launchd service...");
    
    // Use canonical installation path
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not find home directory"))?;
    let exe_path = home_dir.join(".gmine/bin/gmine");
    
    // Verify binary exists at expected location
    if !exe_path.exists() {
        return Err(anyhow!(
            "GMINE binary not found at {}. Please ensure it is installed correctly using the install script.",
            exe_path.display()
        ));
    }
    
    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.gelotto.gmine-miner</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>mine</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}/.gmine/miner.log</string>
    <key>StandardErrorPath</key>
    <string>{}/.gmine/miner.log</string>
</dict>
</plist>"#,
        exe_path.display(),
        std::env::var("HOME")?,
        std::env::var("HOME")?
    );
    
    let plist_dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not find home directory"))?
        .join("Library/LaunchAgents");
    
    fs::create_dir_all(&plist_dir)?;
    
    let plist_path = plist_dir.join("com.gelotto.gmine-miner.plist");
    fs::write(&plist_path, plist_content)?;
    
    // Load the service
    std::process::Command::new("launchctl")
        .args(&["load", &plist_path.to_string_lossy()])
        .status()?;
    
    println!("✅ Service installed successfully!");
    println!("\nThe miner will start automatically.");
    println!("To check status: gmine service status");
    
    Ok(())
}

/// Start service
async fn service_start() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let status = std::process::Command::new("systemctl")
            .args(&["--user", "start", "gmine-miner.service"])
            .status()?;
        
        if status.success() {
            println!("✅ Mining service started");
        } else {
            return Err(anyhow!("Failed to start service"));
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("launchctl")
            .args(&["start", "com.gelotto.gmine-miner"])
            .status()?;
        
        if status.success() {
            println!("✅ Mining service started");
        } else {
            return Err(anyhow!("Failed to start service"));
        }
    }
    
    Ok(())
}

/// Stop service
async fn service_stop() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let status = std::process::Command::new("systemctl")
            .args(&["--user", "stop", "gmine-miner.service"])
            .status()?;
        
        if status.success() {
            println!("✅ Mining service stopped");
        } else {
            return Err(anyhow!("Failed to stop service"));
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("launchctl")
            .args(&["stop", "com.gelotto.gmine-miner"])
            .status()?;
        
        if status.success() {
            println!("✅ Mining service stopped");
        } else {
            return Err(anyhow!("Failed to stop service"));
        }
    }
    
    Ok(())
}

/// Service status
async fn service_status() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("systemctl")
            .args(&["--user", "status", "gmine-miner.service"])
            .status()?;
    }
    
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("launchctl")
            .args(&["list", "com.gelotto.gmine-miner"])
            .status()?;
    }
    
    Ok(())
}

/// Uninstall service
async fn service_uninstall() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        // Stop service first
        let _ = std::process::Command::new("systemctl")
            .args(&["--user", "stop", "gmine-miner.service"])
            .status();
        
        // Disable service
        std::process::Command::new("systemctl")
            .args(&["--user", "disable", "gmine-miner.service"])
            .status()?;
        
        // Remove service file
        let service_path = dirs::config_dir()
            .ok_or_else(|| anyhow!("Could not find config directory"))?
            .join("systemd/user/gmine-miner.service");
        
        if service_path.exists() {
            fs::remove_file(service_path)?;
        }
        
        // Reload systemd
        std::process::Command::new("systemctl")
            .args(&["--user", "daemon-reload"])
            .status()?;
        
        println!("✅ Service uninstalled");
    }
    
    #[cfg(target_os = "macos")]
    {
        let plist_path = dirs::home_dir()
            .ok_or_else(|| anyhow!("Could not find home directory"))?
            .join("Library/LaunchAgents/com.gelotto.gmine-miner.plist");
        
        // Unload service
        if plist_path.exists() {
            std::process::Command::new("launchctl")
                .args(&["unload", &plist_path.to_string_lossy()])
                .status()?;
            
            // Remove plist file
            fs::remove_file(plist_path)?;
        }
        
        println!("✅ Service uninstalled");
    }
    
    Ok(())
}

/// Get configuration directory
fn get_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not find home directory"))?;
    Ok(home.join(".gmine"))
}