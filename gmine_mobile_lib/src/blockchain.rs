use crate::types::{Epoch, MiningChallenge};
use crate::web3_extension::ExtensionOptionsWeb3Tx;
use serde_json::json;
use std::error::Error;
use blake2::{Blake2b512, Digest};
use log;
use base64::{Engine as _, engine::general_purpose};

// Use official Injective testnet LCD endpoint
const TESTNET_REST_URL: &str = "https://testnet.sentry.lcd.injective.network:443";
const MINING_CONTRACT: &str = "inj1vd520adql0apl3wsuyhhpptl79yqwxx73e4j66"; // V3.5 with migration capability

pub struct BlockchainClient {
    agent: ureq::Agent,
}

impl BlockchainClient {
    pub fn new() -> Self {
        // Use ureq with default TLS configuration
        // It should work better on Android than reqwest
        let agent = ureq::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build();
            
        BlockchainClient { agent }
    }
    
    /// Get current epoch information from contract
    pub fn get_current_epoch(&self) -> Result<Epoch, Box<dyn Error>> {
        let query_msg = json!({
            "current_epoch": {}
        });
        
        use base64::{Engine as _, engine::general_purpose};
        let query_data = general_purpose::STANDARD.encode(query_msg.to_string());
        let url = format!(
            "{}/cosmwasm/wasm/v1/contract/{}/smart/{}",
            TESTNET_REST_URL,
            MINING_CONTRACT,
            query_data
        );
        
        log::info!("Fetching epoch from: {}", url);
        
        match self.agent.get(&url).call() {
            Ok(response) => {
                let result: serde_json::Value = response.into_json()?;
                log::info!("Got epoch response: {}", result);
                
                // Parse the response
                if let Some(data) = result.get("data") {
                    let epoch: Epoch = serde_json::from_value(data.clone())?;
                    return Ok(epoch);
                } else {
                    return Err("Failed to get epoch data".into());
                }
            }
            Err(e) => {
                log::error!("HTTP request failed: {}", e);
                return Err(format!("Failed to get current epoch: {}", e).into());
            }
        }
    }
    
    /// Get current mining challenge for wallet
    pub fn get_mining_challenge(&self, wallet_address: &str) -> Result<MiningChallenge, Box<dyn Error>> {
        // First get current epoch
        let epoch = self.get_current_epoch()?;
        
        // Calculate nonce range for this wallet
        let (nonce_start, nonce_end) = calculate_nonce_range(wallet_address, epoch.epoch_number);
        
        // Convert target_hash to challenge array
        let mut challenge = [0u8; 32];
        if epoch.target_hash.len() >= 32 {
            challenge.copy_from_slice(&epoch.target_hash[..32]);
        } else {
            // Pad with zeros if needed
            challenge[..epoch.target_hash.len()].copy_from_slice(&epoch.target_hash);
        }
        
        Ok(MiningChallenge {
            challenge,
            difficulty: epoch.difficulty,
            epoch: epoch.epoch_number,
            nonce_start,
            nonce_end,
        })
    }
    
    /// Query account information
    pub fn get_account_info(&self, address: &str) -> Result<(u64, u64), Box<dyn Error>> {
        let url = format!(
            "{}/cosmos/auth/v1beta1/accounts/{}",
            TESTNET_REST_URL,
            address
        );
        
        let response = self.agent.get(&url).call()
            .map_err(|e| format!("HTTP request failed: {}", e))?;
        let result: serde_json::Value = response.into_json()?;
        
        // Extract account number and sequence
        if let Some(account) = result.get("account") {
            let account_number = account.get("account_number")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
                
            let sequence = account.get("sequence")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
                
            Ok((account_number, sequence))
        } else {
            // New account - use defaults
            Ok((0, 0))
        }
    }
    
    /// Get the latest block height from the blockchain
    pub fn get_latest_block_height(&self) -> Result<u64, Box<dyn Error>> {
        let url = format!(
            "{}/cosmos/base/tendermint/v1beta1/blocks/latest",
            TESTNET_REST_URL
        );
        
        log::info!("Fetching latest block from: {}", url);
        
        match self.agent.get(&url).call() {
            Ok(response) => {
                let result: serde_json::Value = response.into_json()?;
                log::debug!("Got latest block response: {}", result);
                
                if let Some(height_str) = result.get("block")
                    .and_then(|b| b.get("header"))
                    .and_then(|h| h.get("height"))
                    .and_then(|v| v.as_str()) 
                {
                    let height = height_str.parse::<u64>()?;
                    Ok(height)
                } else {
                    Err("Failed to parse block height from response".into())
                }
            }
            Err(e) => {
                log::error!("HTTP request for latest block failed: {}", e);
                Err(e.into())
            }
        }
    }
    
    /// Submit a mining commitment using Injective's JSON format
    pub fn submit_commitment(&self, commitment: &str, from_address: &str, signature: &str, pub_key: &str, account_number: u64, sequence: u64) -> Result<String, Box<dyn Error>> {
        log::info!("submit_commitment called with:");
        log::info!("  from_address: {}", from_address);
        log::info!("  signature: {}", signature);
        log::info!("  pub_key: {}", pub_key);
        log::info!("  account_number: {}, sequence: {}", account_number, sequence);
        
        // Construct the full transaction in Injective's expected format
        let tx = json!({
            "tx": {
                "body": {
                    "messages": [{
                        "@type": "/injective.wasmx.v1.MsgExecuteContractCompat",
                        "sender": from_address,
                        "contract": MINING_CONTRACT,
                        "msg": json!({
                            "commit_solution": {
                                "commitment": commitment
                            }
                        }).to_string(),
                        "funds": "0"
                    }],
                    "memo": "",
                    "timeout_height": "0",
                    "extension_options": [
                        ExtensionOptionsWeb3Tx::new_for_testnet().to_any()?
                    ],
                    "non_critical_extension_options": []
                },
                "auth_info": {
                    "signer_infos": [{
                        "public_key": {
                            "@type": "/injective.crypto.v1beta1.ethsecp256k1.PubKey",
                            "key": pub_key
                        },
                        "mode_info": {
                            "single": {
                                "mode": "SIGN_MODE_LEGACY_AMINO_JSON"
                            }
                        },
                        "sequence": sequence.to_string()
                    }],
                    "fee": {
                        "amount": [{
                            "denom": "inj",
                            "amount": "500000000000000"
                        }],
                        "gas_limit": "350000",
                        "payer": "",
                        "granter": ""
                    }
                },
                "signatures": [signature.trim_start_matches("0x")]
            },
            "mode": "BROADCAST_MODE_SYNC"
        });
        
        let url = format!("{}/cosmos/tx/v1beta1/txs", TESTNET_REST_URL);
        
        log::info!("Submitting transaction to: {}", url);
        log::debug!("Transaction payload: {}", serde_json::to_string_pretty(&tx).unwrap_or_default());
        
        let response = match self.agent.post(&url).send_json(&tx) {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, response)) => {
                // For HTTP error status codes, try to get the response body
                let error_body = response.into_string().unwrap_or_else(|_| "Unable to read error body".to_string());
                return Err(format!("HTTP request failed: {} - {}: {}", url, code, error_body).into());
            }
            Err(e) => return Err(format!("HTTP request failed: {}", e).into()),
        };
            
        let result: serde_json::Value = response.into_json()?;
        
        log::info!("Transaction response: {}", serde_json::to_string_pretty(&result).unwrap_or_default());
        
        // Check for tx hash
        if let Some(tx_response) = result.get("tx_response") {
            if let Some(txhash) = tx_response.get("txhash").and_then(|v| v.as_str()) {
                // Check for errors
                if let Some(code) = tx_response.get("code").and_then(|v| v.as_u64()) {
                    if code != 0 {
                        let raw_log = tx_response.get("raw_log")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error");
                        return Err(format!("Transaction failed: {}", raw_log).into());
                    }
                }
                return Ok(txhash.to_string());
            }
        }
        
        Err("Failed to broadcast transaction".into())
    }
    
    /// Broadcast signed transaction
    pub fn broadcast_tx(&self, tx_bytes: &str) -> Result<String, Box<dyn Error>> {
        let url = format!("{}/cosmos/tx/v1beta1/txs", TESTNET_REST_URL);
        
        let body = json!({
            "tx_bytes": tx_bytes,
            "mode": "BROADCAST_MODE_SYNC"
        });
        
        let response = self.agent.post(&url)
            .send_json(&body)
            .map_err(|e| format!("HTTP request failed: {}", e))?;
            
        let result: serde_json::Value = response.into_json()?;
        
        // Check for tx hash
        if let Some(tx_response) = result.get("tx_response") {
            if let Some(txhash) = tx_response.get("txhash").and_then(|v| v.as_str()) {
                // Check for errors
                if let Some(code) = tx_response.get("code").and_then(|v| v.as_u64()) {
                    if code != 0 {
                        let raw_log = tx_response.get("raw_log")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error");
                        return Err(format!("Transaction failed: {}", raw_log).into());
                    }
                }
                return Ok(txhash.to_string());
            }
        }
        
        Err("Failed to broadcast transaction".into())
    }
    
    /// Submit a reveal solution using Injective's JSON format
    pub fn submit_reveal(&self, nonce: &str, digest: &str, salt: &str, from_address: &str, signature: &str, pub_key: &str, account_number: u64, sequence: u64) -> Result<String, Box<dyn Error>> {
        log::info!("submit_reveal called");
        
        // Construct the full transaction in Injective's expected format
        let tx = json!({
            "tx": {
                "body": {
                    "messages": [{
                        "@type": "/injective.wasmx.v1.MsgExecuteContractCompat",
                        "sender": from_address,
                        "contract": MINING_CONTRACT,
                        "msg": json!({
                            "reveal_solution": {
                                "nonce": nonce,
                                "digest": digest,
                                "salt": salt
                            }
                        }).to_string(),
                        "funds": "0"
                    }],
                    "memo": "",
                    "timeout_height": "0",
                    "extension_options": [
                        ExtensionOptionsWeb3Tx::new_for_testnet().to_any()?
                    ],
                    "non_critical_extension_options": []
                },
                "auth_info": {
                    "signer_infos": [{
                        "public_key": {
                            "@type": "/injective.crypto.v1beta1.ethsecp256k1.PubKey",
                            "key": pub_key
                        },
                        "mode_info": {
                            "single": {
                                "mode": "SIGN_MODE_LEGACY_AMINO_JSON"
                            }
                        },
                        "sequence": sequence.to_string()
                    }],
                    "fee": {
                        "amount": [{
                            "denom": "inj",
                            "amount": "500000000000000"
                        }],
                        "gas_limit": "350000",
                        "payer": "",
                        "granter": ""
                    }
                },
                "signatures": [signature.trim_start_matches("0x")]
            },
            "mode": "BROADCAST_MODE_SYNC"
        });
        
        let url = format!("{}/cosmos/tx/v1beta1/txs", TESTNET_REST_URL);
        
        log::info!("Submitting reveal transaction to: {}", url);
        
        let response = match self.agent.post(&url).send_json(&tx) {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, response)) => {
                let error_body = response.into_string().unwrap_or_else(|_| "Unable to read error body".to_string());
                return Err(format!("HTTP request failed: {} - {}: {}", url, code, error_body).into());
            }
            Err(e) => return Err(format!("HTTP request failed: {}", e).into()),
        };
            
        let result: serde_json::Value = response.into_json()?;
        
        log::info!("Reveal transaction response: {}", serde_json::to_string_pretty(&result).unwrap_or_default());
        
        // Check for tx hash
        if let Some(tx_response) = result.get("tx_response") {
            if let Some(txhash) = tx_response.get("txhash").and_then(|v| v.as_str()) {
                // Check for errors
                if let Some(code) = tx_response.get("code").and_then(|v| v.as_u64()) {
                    if code != 0 {
                        let raw_log = tx_response.get("raw_log")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error");
                        return Err(format!("Transaction failed: {}", raw_log).into());
                    }
                }
                return Ok(txhash.to_string());
            }
        }
        
        Err("Failed to broadcast reveal transaction".into())
    }
    
    /// Submit advance epoch transaction
    pub fn submit_advance_epoch(&self, from_address: &str, signature: &str, pub_key: &str, account_number: u64, sequence: u64) -> Result<String, Box<dyn Error>> {
        log::info!("submit_advance_epoch called");
        
        // Construct the full transaction in Injective's expected format
        let tx = json!({
            "tx": {
                "body": {
                    "messages": [{
                        "@type": "/injective.wasmx.v1.MsgExecuteContractCompat",
                        "sender": from_address,
                        "contract": MINING_CONTRACT,
                        "msg": json!({
                            "advance_epoch": {}
                        }).to_string(),
                        "funds": "0"
                    }],
                    "memo": "",
                    "timeout_height": "0",
                    "extension_options": [
                        ExtensionOptionsWeb3Tx::new_for_testnet().to_any()?
                    ],
                    "non_critical_extension_options": []
                },
                "auth_info": {
                    "signer_infos": [{
                        "public_key": {
                            "@type": "/injective.crypto.v1beta1.ethsecp256k1.PubKey",
                            "key": pub_key
                        },
                        "mode_info": {
                            "single": {
                                "mode": "SIGN_MODE_LEGACY_AMINO_JSON"
                            }
                        },
                        "sequence": sequence.to_string()
                    }],
                    "fee": {
                        "amount": [{
                            "denom": "inj",
                            "amount": "500000000000000"
                        }],
                        "gas_limit": "350000",
                        "payer": "",
                        "granter": ""
                    }
                },
                "signatures": [signature.trim_start_matches("0x")]
            },
            "mode": "BROADCAST_MODE_SYNC"
        });
        
        let url = format!("{}/cosmos/tx/v1beta1/txs", TESTNET_REST_URL);
        
        log::info!("Submitting advance epoch transaction to: {}", url);
        
        let response = match self.agent.post(&url).send_json(&tx) {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, response)) => {
                let error_body = response.into_string().unwrap_or_else(|_| "Unable to read error body".to_string());
                return Err(format!("HTTP request failed: {} - {}: {}", url, code, error_body).into());
            }
            Err(e) => return Err(format!("HTTP request failed: {}", e).into()),
        };
            
        let result: serde_json::Value = response.into_json()?;
        
        log::info!("Advance epoch transaction response: {}", serde_json::to_string_pretty(&result).unwrap_or_default());
        
        // Check for tx hash
        if let Some(tx_response) = result.get("tx_response") {
            if let Some(txhash) = tx_response.get("txhash").and_then(|v| v.as_str()) {
                // Check for errors
                if let Some(code) = tx_response.get("code").and_then(|v| v.as_u64()) {
                    if code != 0 {
                        let raw_log = tx_response.get("raw_log")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error");
                        return Err(format!("Transaction failed: {}", raw_log).into());
                    }
                }
                return Ok(txhash.to_string());
            }
        }
        
        Err("Failed to broadcast advance epoch transaction".into())
    }
    
    /// Submit finalize epoch transaction
    pub fn submit_finalize_epoch(&self, epoch_number: u64, from_address: &str, signature: &str, pub_key: &str, account_number: u64, sequence: u64) -> Result<String, Box<dyn Error>> {
        log::info!("submit_finalize_epoch called for epoch {}", epoch_number);
        
        // Construct the full transaction in Injective's expected format
        let tx = json!({
            "tx": {
                "body": {
                    "messages": [{
                        "@type": "/injective.wasmx.v1.MsgExecuteContractCompat",
                        "sender": from_address,
                        "contract": MINING_CONTRACT,
                        "msg": json!({
                            "finalize_epoch": {
                                "epoch_number": epoch_number
                            }
                        }).to_string(),
                        "funds": "0"
                    }],
                    "memo": "",
                    "timeout_height": "0",
                    "extension_options": [
                        ExtensionOptionsWeb3Tx::new_for_testnet().to_any()?
                    ],
                    "non_critical_extension_options": []
                },
                "auth_info": {
                    "signer_infos": [{
                        "public_key": {
                            "@type": "/injective.crypto.v1beta1.ethsecp256k1.PubKey",
                            "key": pub_key
                        },
                        "mode_info": {
                            "single": {
                                "mode": "SIGN_MODE_LEGACY_AMINO_JSON"
                            }
                        },
                        "sequence": sequence.to_string()
                    }],
                    "fee": {
                        "amount": [{
                            "denom": "inj",
                            "amount": "500000000000000"
                        }],
                        "gas_limit": "350000",
                        "payer": "",
                        "granter": ""
                    }
                },
                "signatures": [signature.trim_start_matches("0x")]
            },
            "mode": "BROADCAST_MODE_SYNC"
        });
        
        let url = format!("{}/cosmos/tx/v1beta1/txs", TESTNET_REST_URL);
        
        log::info!("Submitting finalize epoch transaction to: {}", url);
        
        let response = match self.agent.post(&url).send_json(&tx) {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, response)) => {
                let error_body = response.into_string().unwrap_or_else(|_| "Unable to read error body".to_string());
                return Err(format!("HTTP request failed: {} - {}: {}", url, code, error_body).into());
            }
            Err(e) => return Err(format!("HTTP request failed: {}", e).into()),
        };
            
        let result: serde_json::Value = response.into_json()?;
        
        log::info!("Finalize epoch transaction response: {}", serde_json::to_string_pretty(&result).unwrap_or_default());
        
        // Check for tx hash
        if let Some(tx_response) = result.get("tx_response") {
            if let Some(txhash) = tx_response.get("txhash").and_then(|v| v.as_str()) {
                // Check for errors
                if let Some(code) = tx_response.get("code").and_then(|v| v.as_u64()) {
                    if code != 0 {
                        let raw_log = tx_response.get("raw_log")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error");
                        return Err(format!("Transaction failed: {}", raw_log).into());
                    }
                }
                return Ok(txhash.to_string());
            }
        }
        
        Err("Failed to broadcast finalize epoch transaction".into())
    }
    
    /// Submit claim reward transaction
    pub fn submit_claim_reward(&self, epoch_number: u64, from_address: &str, signature: &str, pub_key: &str, account_number: u64, sequence: u64) -> Result<String, Box<dyn Error>> {
        log::info!("submit_claim_reward called for epoch {}", epoch_number);
        
        // Construct the full transaction in Injective's expected format
        let tx = json!({
            "tx": {
                "body": {
                    "messages": [{
                        "@type": "/injective.wasmx.v1.MsgExecuteContractCompat",
                        "sender": from_address,
                        "contract": MINING_CONTRACT,
                        "msg": json!({
                            "claim_reward": {
                                "epoch_number": epoch_number
                            }
                        }).to_string(),
                        "funds": "0"
                    }],
                    "memo": "",
                    "timeout_height": "0",
                    "extension_options": [
                        ExtensionOptionsWeb3Tx::new_for_testnet().to_any()?
                    ],
                    "non_critical_extension_options": []
                },
                "auth_info": {
                    "signer_infos": [{
                        "public_key": {
                            "@type": "/injective.crypto.v1beta1.ethsecp256k1.PubKey",
                            "key": pub_key
                        },
                        "mode_info": {
                            "single": {
                                "mode": "SIGN_MODE_LEGACY_AMINO_JSON"
                            }
                        },
                        "sequence": sequence.to_string()
                    }],
                    "fee": {
                        "amount": [{
                            "denom": "inj",
                            "amount": "500000000000000"
                        }],
                        "gas_limit": "350000",
                        "payer": "",
                        "granter": ""
                    }
                },
                "signatures": [signature.trim_start_matches("0x")]
            },
            "mode": "BROADCAST_MODE_SYNC"
        });
        
        let url = format!("{}/cosmos/tx/v1beta1/txs", TESTNET_REST_URL);
        
        log::info!("Submitting claim reward transaction to: {}", url);
        
        let response = match self.agent.post(&url).send_json(&tx) {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, response)) => {
                let error_body = response.into_string().unwrap_or_else(|_| "Unable to read error body".to_string());
                return Err(format!("HTTP request failed: {} - {}: {}", url, code, error_body).into());
            }
            Err(e) => return Err(format!("HTTP request failed: {}", e).into()),
        };
            
        let result: serde_json::Value = response.into_json()?;
        
        log::info!("Claim reward transaction response: {}", serde_json::to_string_pretty(&result).unwrap_or_default());
        
        // Check for tx hash
        if let Some(tx_response) = result.get("tx_response") {
            if let Some(txhash) = tx_response.get("txhash").and_then(|v| v.as_str()) {
                // Check for errors
                if let Some(code) = tx_response.get("code").and_then(|v| v.as_u64()) {
                    if code != 0 {
                        let raw_log = tx_response.get("raw_log")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error");
                        return Err(format!("Transaction failed: {}", raw_log).into());
                    }
                }
                return Ok(txhash.to_string());
            }
        }
        
        Err("Failed to broadcast claim reward transaction".into())
    }
}

/// Calculate nonce range for a wallet address
/// This must match the contract's drillx_utils::get_nonce_range_for_address
fn calculate_nonce_range(address: &str, epoch: u64) -> (u64, u64) {
    // Create commitment using Blake2b512
    let mut hasher = Blake2b512::new();
    hasher.update(address.as_bytes());
    hasher.update(epoch.to_le_bytes());
    let commitment = hasher.finalize();
    
    // Extract first 8 bytes as u64
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&commitment[..8]);
    let hash_value = u64::from_le_bytes(bytes);
    
    // Calculate partition (1/1000th of nonce space)
    let partition_size = u64::MAX / 1000;
    let partition_index = hash_value % 1000;
    
    let nonce_start = partition_index * partition_size;
    let nonce_end = if partition_index == 999 {
        u64::MAX
    } else {
        (partition_index + 1) * partition_size - 1
    };
    
    (nonce_start, nonce_end)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_nonce_range_calculation() {
        let address = "inj1testaddress123456789";
        let epoch = 1287;
        
        let (start, end) = calculate_nonce_range(address, epoch);
        
        // Verify partition size
        assert!(end - start >= u64::MAX / 1000 - 1);
        assert!(end - start <= u64::MAX / 1000 + 1);
    }
}