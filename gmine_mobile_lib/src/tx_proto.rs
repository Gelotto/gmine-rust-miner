use crate::eip712::Eip712Signer;
use crate::web3_extension::ExtensionOptionsWeb3Tx;
use crate::types::{Fee, Coin};
use serde_json::{Value, json};
use std::error::Error;
use base64::{Engine as _, engine::general_purpose};
use prost::Message;

// Import protobuf types (we'll need to add these to Cargo.toml)
use cosmos_sdk_proto::cosmos::tx::v1beta1::{
    TxRaw, TxBody, AuthInfo, SignerInfo, ModeInfo, Fee as ProtoFee,
    mode_info, BroadcastTxRequest, BroadcastMode,
};
use cosmos_sdk_proto::cosmos::base::v1beta1::Coin as ProtoCoin;
use cosmos_sdk_proto::Any;
use crate::msg_execute_contract_compat::MsgExecuteContractCompat;

/// Build a protobuf transaction for Injective
pub struct ProtoTransactionBuilder {
    signer: Eip712Signer,
    rest_url: String,
    chain_id: String,
}

impl ProtoTransactionBuilder {
    /// Create a new transaction builder
    pub fn new(
        private_key: &[u8], 
        public_key: &[u8],
        network: &str
    ) -> Result<Self, Box<dyn Error>> {
        let signer = Eip712Signer::new(private_key, public_key)?;
        
        let (rest_url, chain_id) = match network {
            "testnet" => (
                "https://testnet.sentry.lcd.injective.network:443".to_string(),
                "injective-888".to_string()
            ),
            "mainnet" => (
                "https://sentry.lcd.injective.network:443".to_string(),
                "injective-1".to_string()
            ),
            _ => return Err("Invalid network".into()),
        };
        
        Ok(ProtoTransactionBuilder {
            signer,
            rest_url,
            chain_id,
        })
    }
    
    /// Build and sign a transaction, returning protobuf bytes
    pub fn build_transaction(
        &self,
        sender_address: &str,
        contract_address: &str,
        msg: Value,
        account_number: u64,
        sequence: u64,
        fee: Option<Fee>,
        memo: &str,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        // The msg parameter contains the inner message data
        // We need to wrap it in the proper contract message format with PascalCase
        // Create clean message without _msg_type for contract
        let clean_msg = if msg.get("_msg_type").is_some() {
            let mut cleaned = msg.clone();
            if let Some(obj) = cleaned.as_object_mut() {
                obj.remove("_msg_type");
            }
            cleaned
        } else {
            msg.clone()
        };
        
        let contract_msg = if msg.get("_msg_type").and_then(|v| v.as_str()) == Some("advance_epoch") {
            json!({"advance_epoch": {}})
        } else if msg.get("commitment").is_some() {
            json!({"commit_solution": clean_msg})
        } else if msg.get("nonce").is_some() {
            json!({"reveal_solution": clean_msg})
        } else if msg.get("epoch_number").is_some() {
            // Check the hint first to distinguish between finalize_epoch and claim_reward
            if msg.get("_msg_type").and_then(|v| v.as_str()) == Some("claim_reward") {
                // For claim_reward, only include epoch_number
                let epoch_number = msg.get("epoch_number").unwrap();
                json!({"claim_reward": {"epoch_number": epoch_number}})
            } else {
                json!({"finalize_epoch": clean_msg})
            }
        } else if msg.is_object() && msg.as_object().unwrap().is_empty() {
            json!({"claim_reward": {}})
        } else {
            clean_msg
        };
        
        // Debug log what we're sending to the contract
        log::info!("Contract message being sent: {}", serde_json::to_string_pretty(&contract_msg).unwrap_or_default());
        
        // Additional debugging - show exact bytes
        let msg_bytes = contract_msg.to_string().as_bytes().to_vec();
        log::debug!("Message as bytes: {:?}", msg_bytes);
        log::debug!("Message byte length: {}", msg_bytes.len());
        log::debug!("Message as hex: {}", hex::encode(&msg_bytes));
        
        // Create the MsgExecuteContractCompat (Injective-specific)
        // IMPORTANT: The msg field expects a JSON string. The contract will parse this
        // string as ExecuteMsg. We must ensure the JSON is properly formatted.
        let execute_msg = MsgExecuteContractCompat {
            sender: sender_address.to_string(),
            contract: contract_address.to_string(),
            msg: contract_msg.to_string(), // This creates the JSON string
            funds: "0".to_string(), // Empty funds as "0" string
        };
        
        // Wrap in Any
        let any_msg = Any {
            type_url: MsgExecuteContractCompat::type_url().to_string(),
            value: execute_msg.encode_to_vec(),
        };
        
        // Create Web3Extension (non-delegated transaction)
        let web3_extension = ExtensionOptionsWeb3Tx::new_for_testnet();
        let mut web3_ext_bytes = Vec::new();
        web3_extension.encode(&mut web3_ext_bytes)
            .map_err(|e| format!("Failed to encode Web3Extension: {}", e))?;
        let any_extension = Any {
            type_url: "/injective.types.v1beta1.ExtensionOptionsWeb3Tx".to_string(),
            value: web3_ext_bytes,
        };
        
        // Create TxBody
        let tx_body = TxBody {
            messages: vec![any_msg],
            memo: memo.to_string(),
            timeout_height: 0,
            extension_options: vec![any_extension],
            non_critical_extension_options: vec![],
        };
        
        // Sign the transaction using EIP-712
        // The msg parameter here is the contract execute message content, not wrapped
        log::info!("ProtoTransactionBuilder: determining message type from: {}", msg);
        let msg_type = if msg.get("commitment").is_some() {
            "commit_solution"
        } else if msg.get("nonce").is_some() {
            "reveal_solution"
        } else if msg.get("_msg_type").is_some() {
            // Handle hint from rust_signer first
            msg.get("_msg_type").unwrap().as_str().unwrap()
        } else if msg.get("epoch_number").is_some() {
            "finalize_epoch"
        } else if msg.is_object() && msg.as_object().unwrap().is_empty() {
            // Empty object defaults to advance_epoch
            "advance_epoch"
        } else {
            return Err(format!("Unknown message type in: {}", msg).into());
        };
        
        // Remove _msg_type hint before passing to signer
        let msg_for_signing = if msg.get("_msg_type").is_some() {
            let mut cleaned = msg.clone();
            if let Some(obj) = cleaned.as_object_mut() {
                obj.remove("_msg_type");
            }
            // If it's now empty (was just _msg_type), return empty object
            if cleaned.as_object().map(|o| o.is_empty()).unwrap_or(false) {
                json!({})
            } else {
                cleaned
            }
        } else {
            msg.clone()
        };
        
        let signing_result = self.signer.sign_transaction(
            msg_type,
            &msg_for_signing,
            sender_address,
            account_number,
            sequence,
            fee.clone(),
            memo,
        )?;
        
        if !signing_result.success {
            return Err(signing_result.error.unwrap_or("Signing failed".to_string()).into());
        }
        
        let signature = signing_result.signature.ok_or("No signature")?;
        let pub_key = signing_result.pub_key.ok_or("No public key")?;
        
        // Convert hex signature to bytes
        let sig_hex = signature.trim_start_matches("0x");
        eprintln!("DEBUG: Raw signature from signer: {}", signature);
        eprintln!("DEBUG: Signature after trimming 0x: {}", sig_hex);
        eprintln!("DEBUG: Hex length (without 0x): {}", sig_hex.len());
        
        // Check if hex length is even
        if sig_hex.len() % 2 != 0 {
            eprintln!("ERROR: Hex string has odd length: {}", sig_hex.len());
            eprintln!("ERROR: First 10 chars: {}", &sig_hex[..sig_hex.len().min(10)]);
            eprintln!("ERROR: Last 10 chars: {}", &sig_hex[sig_hex.len().saturating_sub(10)..]);
            eprintln!("ERROR: Full hex: {}", sig_hex);
        }
        
        let sig_bytes = match hex::decode(sig_hex) {
            Ok(bytes) => bytes,
            Err(e) => {
                // Try to understand the error better
                if sig_hex.is_empty() {
                    return Err("Signature hex is empty".into());
                }
                // Check each character
                for (i, ch) in sig_hex.chars().enumerate() {
                    if !ch.is_ascii_hexdigit() {
                        return Err(format!("Invalid hex character '{}' at position {}", ch, i).into());
                    }
                }
                return Err(format!("Failed to decode signature hex: {} (hex length: {})", e, sig_hex.len()).into());
            }
        };
        
        // Create public key Any
        let pub_key_any = Any {
            type_url: "/injective.crypto.v1beta1.ethsecp256k1.PubKey".to_string(),
            value: {
                // The public key protobuf message just wraps the key bytes
                let mut buf = Vec::new();
                // Tag 1, wire type 2 (length-delimited)
                buf.push(0x0a);
                // Decode the public key from base64
                let key_bytes = general_purpose::STANDARD.decode(&pub_key)?;
                prost::encoding::encode_varint(key_bytes.len() as u64, &mut buf);
                buf.extend_from_slice(&key_bytes);
                buf
            },
        };
        
        // Create SignerInfo
        let signer_info = SignerInfo {
            public_key: Some(pub_key_any),
            mode_info: Some(ModeInfo {
                sum: Some(mode_info::Sum::Single(mode_info::Single {
                    mode: cosmos_sdk_proto::cosmos::tx::signing::v1beta1::SignMode::LegacyAminoJson as i32,
                })),
            }),
            sequence,
        };
        
        // Convert fee - use default if not provided
        let fee = fee.unwrap_or_default();
        let proto_fee = ProtoFee {
            amount: fee.amount.into_iter().map(|c| ProtoCoin {
                denom: c.denom,
                amount: c.amount,
            }).collect(),
            gas_limit: fee.gas.parse().unwrap_or(250000),
            payer: fee.payer,
            granter: fee.granter,
        };
        
        // Create AuthInfo
        let auth_info = AuthInfo {
            signer_infos: vec![signer_info],
            fee: Some(proto_fee),
            tip: None,
        };
        
        // Create TxRaw
        let body_bytes = tx_body.encode_to_vec();
        let auth_info_bytes = auth_info.encode_to_vec();
        
        println!("=== PROTOBUF DEBUGGING ===");
        println!("TxBody bytes (hex): {}", hex::encode(&body_bytes));
        println!("TxBody bytes length: {}", body_bytes.len());
        println!("AuthInfo bytes (hex): {}", hex::encode(&auth_info_bytes));
        println!("AuthInfo bytes length: {}", auth_info_bytes.len());
        println!("Signature bytes (hex): {}", hex::encode(&sig_bytes));
        println!("Signature bytes length: {}", sig_bytes.len());
        
        // Debug the actual message content
        println!("Message type: {}", MsgExecuteContractCompat::type_url());
        println!("Message sender: {}", sender_address);
        println!("Message contract: {}", contract_address);
        println!("Message content (execute_msg.msg): {}", execute_msg.msg);
        
        let tx_raw = TxRaw {
            body_bytes,
            auth_info_bytes,
            signatures: vec![sig_bytes],
        };
        
        let tx_raw_bytes = tx_raw.encode_to_vec();
        println!("TxRaw bytes (hex): {}", hex::encode(&tx_raw_bytes));
        println!("TxRaw bytes length: {}", tx_raw_bytes.len());
        println!("=== END PROTOBUF DEBUGGING ===");
        
        // Encode to protobuf bytes
        Ok(tx_raw_bytes)
    }
    
    /// Submit a transaction to the blockchain
    pub async fn submit_transaction(&self, tx_bytes: Vec<u8>) -> Result<String, Box<dyn Error>> {
        let client = reqwest::Client::new();
        let url = format!("{}/cosmos/tx/v1beta1/txs", self.rest_url);
        
        // Create the broadcast request
        // IMPORTANT: Injective REST API expects uppercase enum values with BROADCAST_MODE_ prefix
        // Valid values: BROADCAST_MODE_SYNC, BROADCAST_MODE_ASYNC
        let broadcast_mode = "BROADCAST_MODE_SYNC";
        log::info!("Using BROADCAST_MODE_SYNC - will poll for on-chain confirmation");
        
        let tx_json = serde_json::json!({
            "tx_bytes": general_purpose::STANDARD.encode(&tx_bytes),
            "mode": broadcast_mode
        });
        
        log::info!("Submitting transaction to: {}", url);
        log::info!("Transaction bytes length: {}", tx_bytes.len());
        
        let response = client.post(&url)
            .json(&tx_json)
            .send()
            .await?;
        
        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            log::error!("HTTP {} error: {}", status, error_body);
            return Err(format!("HTTP request failed with status {}: {}", status, error_body).into());
        }
        
        let result: Value = response.json().await?;
        
        // Debug log the full response
        log::debug!("Full broadcast response: {}", serde_json::to_string_pretty(&result).unwrap_or_default());
        
        // Check for tx hash
        if let Some(tx_response) = result.get("tx_response") {
            if let Some(txhash) = tx_response.get("txhash").and_then(|v| v.as_str()) {
                // Check for errors
                if let Some(code) = tx_response.get("code").and_then(|v| v.as_u64()) {
                    if code != 0 {
                        let raw_log = tx_response.get("raw_log")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error");
                        
                        // Enhanced error logging for debugging
                        log::error!("=== TRANSACTION EXECUTION FAILED ===");
                        log::error!("TX Hash: {}", txhash);
                        log::error!("Error Code: {}", code);
                        log::error!("Raw Log: {}", raw_log);
                        
                        // Parse common error patterns
                        if raw_log.contains("Wrong phase") {
                            log::error!("TIMING ERROR: Transaction submitted in wrong epoch phase");
                        } else if raw_log.contains("parse") || raw_log.contains("unmarshal") {
                            log::error!("MESSAGE FORMAT ERROR: Contract cannot parse the message");
                            log::error!("This suggests the JSON message format doesn't match contract expectations");
                        } else if raw_log.contains("signature verification failed") {
                            log::error!("SIGNATURE ERROR: The message signed doesn't match what the contract received");
                        }
                        log::error!("=================================");
                        
                        return Err(format!("Transaction failed with code {}: {}", code, raw_log).into());
                    }
                }
                
                // Success case - transaction accepted to mempool
                log::info!("Transaction submitted to mempool: {}", txhash);
                
                // Poll for transaction confirmation
                match self.poll_for_tx_confirmation(txhash).await {
                    Ok(confirmed) => {
                        if confirmed {
                            log::info!("Transaction confirmed on-chain: {}", txhash);
                            return Ok(txhash.to_string());
                        } else {
                            return Err("Transaction failed on-chain execution".into());
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to poll for transaction confirmation: {}", e);
                        // Return the tx hash anyway since it was submitted
                        return Ok(txhash.to_string());
                    }
                }
            }
        }
        
        Err("Failed to broadcast transaction: no tx_hash in response".into())
    }
    
    /// Poll for transaction confirmation
    async fn poll_for_tx_confirmation(&self, tx_hash: &str) -> Result<bool, Box<dyn Error>> {
        const POLL_INTERVAL_MS: u64 = 2500; // 2.5 seconds
        const TIMEOUT_MS: u64 = 60000; // 60 seconds
        const MAX_ATTEMPTS: u32 = (TIMEOUT_MS / POLL_INTERVAL_MS) as u32;
        
        let client = reqwest::Client::new();
        let url = format!("{}/cosmos/tx/v1beta1/txs/{}", self.rest_url, tx_hash);
        
        log::info!("Polling for transaction confirmation: {}", tx_hash);
        
        for attempt in 1..=MAX_ATTEMPTS {
            // Wait before polling (except first attempt)
            if attempt > 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
            }
            
            log::debug!("Poll attempt {}/{} for tx {}", attempt, MAX_ATTEMPTS, tx_hash);
            
            match client.get(&url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        if let Ok(result) = response.json::<serde_json::Value>().await {
                            if let Some(tx_response) = result.get("tx_response") {
                                // Check if transaction is in a block
                                if let Some(height) = tx_response.get("height").and_then(|v| v.as_str()) {
                                    if height != "0" {
                                        // Transaction is in a block, check execution result
                                        if let Some(code) = tx_response.get("code").and_then(|v| v.as_u64()) {
                                            if code == 0 {
                                                log::info!("Transaction confirmed successfully in block {}", height);
                                                return Ok(true);
                                            } else {
                                                let raw_log = tx_response.get("raw_log")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("Unknown error");
                                                log::error!("Transaction failed with code {}: {}", code, raw_log);
                                                
                                                // Log specific error patterns
                                                if raw_log.contains("Invalid type") {
                                                    log::error!("CONTRACT ERROR: Message format doesn't match contract expectations");
                                                } else if raw_log.contains("signature verification failed") {
                                                    log::error!("SIGNATURE ERROR: EIP-712 signature verification failed");
                                                }
                                                
                                                return Ok(false);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if response.status() == 404 {
                        // Transaction not found yet, continue polling
                        log::debug!("Transaction not found yet, continuing to poll...");
                    } else {
                        log::warn!("Unexpected status {} while polling", response.status());
                    }
                }
                Err(e) => {
                    log::warn!("Error polling for transaction: {}", e);
                }
            }
        }
        
        log::warn!("Transaction polling timeout after {} seconds", TIMEOUT_MS / 1000);
        Err("Transaction confirmation timeout".into())
    }
}