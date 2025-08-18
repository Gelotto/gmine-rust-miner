use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub chain: ChainConfig,
    pub miner: MinerConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub rpc_endpoint: String,
    pub grpc_endpoint: String,
    pub chain_id: String,
    pub mining_contract: String,
    pub power_token: String,
    pub gas_price: f64,
    pub gas_adjustment: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinerConfig {
    // Note: Mnemonic should be provided via environment variable MINER_MNEMONIC
    // Never store sensitive keys in config files!
    pub address: String,
    pub threads: usize,
    pub batch_size: usize,
    pub target_hashrate: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            chain: ChainConfig {
                rpc_endpoint: "https://testnet.sentry.tm.injective.network:443".to_string(),
                grpc_endpoint: "https://testnet.sentry.chain.grpc.injective.network:443".to_string(),
                chain_id: "injective-888".to_string(),
                mining_contract: "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y".to_string(),
                power_token: "inj1326k32dr7vjx5tnkuxlt58vkejj60r5ens29s8".to_string(),
                gas_price: 500000000.0,
                gas_adjustment: 1.3,
            },
            miner: MinerConfig {
                address: String::new(),
                threads: num_cpus::get(),
                batch_size: 1000,
                target_hashrate: None,
            },
            database: DatabaseConfig {
                path: "gmine_miner.db".to_string(),
            },
        }
    }
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

// Note: extern crate not needed in Rust 2021 edition