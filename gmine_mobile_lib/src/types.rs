use serde::{Deserialize, Serialize};

/// Mining epoch information from blockchain
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Epoch {
    pub epoch_number: u64,
    pub start_block: u64,
    pub difficulty: u8,
    pub target_hash: Vec<u8>,
}

/// Mining challenge for current epoch
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MiningChallenge {
    pub challenge: [u8; 32],
    pub difficulty: u8,
    pub epoch: u64,
    pub nonce_start: u64,
    pub nonce_end: u64,
}

/// Solution found by miner
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Solution {
    pub nonce: u64,
    pub hash: Vec<u8>,
    pub difficulty: u8,
    pub epoch: u64,
}

/// Commit message for blockchain
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommitMsg {
    pub commitment: String,
}

/// Reveal message for blockchain
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RevealMsg {
    pub nonce: String,
}

/// Claim rewards message
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaimRewardsMsg {}

/// Transaction fee structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Fee {
    pub amount: Vec<Coin>,
    pub gas: String,
    pub payer: String,
    pub granter: String,
}

/// Coin denomination
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Coin {
    pub denom: String,
    pub amount: String,
}

/// Default fee for Injective testnet
impl Default for Fee {
    fn default() -> Self {
        Fee {
            amount: vec![Coin {
                denom: "inj".to_string(),
                amount: "500000000000000".to_string(), // 0.0005 INJ
            }],
            gas: "200000".to_string(),
            payer: String::new(),
            granter: String::new(),
        }
    }
}

/// EIP-712 signing result
#[derive(Debug, Serialize, Deserialize)]
pub struct SigningResult {
    pub success: bool,
    pub signature: Option<String>,
    pub pub_key: Option<String>,
    pub error: Option<String>,
}

/// Mining stats for UI
#[derive(Debug, Serialize, Deserialize)]
pub struct MiningStats {
    #[serde(rename = "isMining")]
    pub is_mining: bool,
    pub hashrate: u64,
    #[serde(rename = "solutionsFound")]
    pub solutions_found: u64,
    #[serde(rename = "uptimeSeconds")]
    pub uptime_seconds: u64,
    pub epoch: u64,
    #[serde(rename = "lastSolutionTime")]
    pub last_solution_time: Option<String>,
    #[serde(rename = "realMining")]
    pub real_mining: bool,
    #[serde(rename = "walletAddress")]
    pub wallet_address: Option<String>,
    #[serde(rename = "currentChallenge")]
    pub current_challenge: Option<String>,
    pub difficulty: Option<u8>,
}