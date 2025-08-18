/// Manages the lifecycle of the Go bridge signing service
use anyhow::{Result, anyhow};
use std::process::{Command, Child, Stdio};
use std::env;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

pub struct BridgeManager {
    process: Option<Child>,
    bridge_path: PathBuf,
    mnemonic: String,
    network: String,
    port: u16,
}

impl BridgeManager {
    pub fn new(mnemonic: String, network: String) -> Result<Self> {
        // Find bridge executable
        let bridge_path = Self::find_bridge_executable()?;
        
        Ok(Self {
            process: None,
            bridge_path,
            mnemonic,
            network,
            port: 8080,
        })
    }

    /// Find the Node.js bridge script in various locations
    fn find_bridge_executable() -> Result<PathBuf> {
        // Look for the Node.js bridge
        let possible_paths = vec![
            // Development path - compiled JavaScript
            PathBuf::from("../gmine-bridge/bridge_nodejs/dist/index.js"),
            // Alternative development path
            PathBuf::from("../gmine-bridge/bridge_nodejs/src/index.ts"),
            // Same directory as miner
            env::current_exe()?.parent().unwrap().join("bridge_nodejs/dist/index.js"),
            // Docker path
            PathBuf::from("/app/bridge_nodejs/dist/index.js"),
        ];

        for path in possible_paths {
            if path.exists() {
                log::info!("Found bridge script at: {:?}", path);
                return Ok(path);
            }
        }

        // Try to build it if in development
        if PathBuf::from("../gmine-bridge/bridge_nodejs/package.json").exists() {
            log::info!("Building Node.js bridge from source...");
            Self::build_bridge()?;
            return Ok(PathBuf::from("../gmine-bridge/bridge_nodejs/dist/index.js"));
        }

        Err(anyhow!("Bridge script not found. Please ensure bridge_nodejs is built and available"))
    }

    /// Build the Node.js bridge from source
    fn build_bridge() -> Result<()> {
        // First, ensure dependencies are installed
        let npm_install = Command::new("npm")
            .arg("install")
            .current_dir("../gmine-bridge/bridge_nodejs")
            .output()?;

        if !npm_install.status.success() {
            return Err(anyhow!("Failed to install dependencies: {}", 
                String::from_utf8_lossy(&npm_install.stderr)));
        }

        // Build the TypeScript
        let npm_build = Command::new("npm")
            .args(&["run", "build"])
            .current_dir("../gmine-bridge/bridge_nodejs")
            .output()?;

        if !npm_build.status.success() {
            return Err(anyhow!("Failed to build bridge: {}", 
                String::from_utf8_lossy(&npm_build.stderr)));
        }

        Ok(())
    }

    /// Start the bridge service
    pub fn start(&mut self) -> Result<()> {
        if self.process.is_some() {
            return Ok(()); // Already running
        }

        log::info!("Starting Node.js bridge service on port {}", self.port);

        // Determine how to run the bridge
        let child = if self.bridge_path.extension().and_then(|s| s.to_str()) == Some("js") {
            // Run compiled JavaScript with Node
            Command::new("node")
                .arg(&self.bridge_path)
                .env("MNEMONIC", &self.mnemonic)
                .env("NETWORK", &self.network)
                .env("PORT", self.port.to_string())
                .env("BRIDGE_API_KEY", "gmine-internal-key")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?
        } else if self.bridge_path.extension().and_then(|s| s.to_str()) == Some("ts") {
            // Run TypeScript directly with ts-node (development)
            Command::new("npx")
                .args(&["ts-node", self.bridge_path.to_str().unwrap()])
                .env("MNEMONIC", &self.mnemonic)
                .env("NETWORK", &self.network)
                .env("PORT", self.port.to_string())
                .env("BRIDGE_API_KEY", "gmine-internal-key")
                .current_dir("../gmine-bridge/bridge_nodejs")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?
        } else {
            return Err(anyhow!("Unknown bridge script type"));
        };

        self.process = Some(child);
        
        log::info!("Bridge service started with PID: {:?}", 
            self.process.as_ref().unwrap().id());

        Ok(())
    }

    /// Stop the bridge service
    pub fn stop(&mut self) -> Result<()> {
        if let Some(mut process) = self.process.take() {
            log::info!("Stopping bridge service...");
            process.kill()?;
            process.wait()?;
            log::info!("Bridge service stopped");
        }
        Ok(())
    }

    /// Check if the bridge is running
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut process) = self.process {
            match process.try_wait() {
                Ok(Some(_)) => {
                    // Process has exited
                    self.process = None;
                    false
                }
                Ok(None) => true, // Still running
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Get the bridge URL
    pub fn get_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Get the API key
    pub fn get_api_key(&self) -> String {
        "gmine-internal-key".to_string()
    }

    /// Ensure bridge is running and healthy
    pub async fn ensure_healthy(&mut self) -> Result<()> {
        // Start if not running
        if !self.is_running() {
            self.start()?;
            
            // Give it time to start
            sleep(Duration::from_secs(2)).await;
        }

        // Create client to check health
        let client = crate::chain::bridge_client::BridgeClient::new(
            self.get_url(),
            Some(self.get_api_key()),
        );

        // Wait for health
        client.wait_for_health(30).await?;

        Ok(())
    }
}

impl Drop for BridgeManager {
    fn drop(&mut self) {
        // Clean shutdown
        if let Err(e) = self.stop() {
            log::error!("Error stopping bridge service: {}", e);
        }
    }
}