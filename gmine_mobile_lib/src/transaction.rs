use crate::eip712::Eip712Signer;
use crate::web3_extension::ExtensionOptionsWeb3Tx;
use crate::types::{Fee, Coin};
use serde_json::{json, Value};
use std::error::Error;
use base64::{Engine as _, engine::general_purpose};
// use prost::Message; // Not needed in this file

/// Complete EIP-712 transaction builder for Injective
pub struct Eip712TransactionBuilder {
    signer: Eip712Signer,
    rest_url: String,
    chain_id: String,
}

impl Eip712TransactionBuilder {
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
        
        Ok(Eip712TransactionBuilder {
            signer,
            rest_url,
            chain_id,
        })
    }
    
    /// Sign and build a complete transaction with Web3Extension
    pub fn build_transaction(
        &self,
        sender_address: &str,
        contract_address: &str,
        msg: Value,
        account_number: u64,
        sequence: u64,
        fee: Option<Fee>,
        memo: &str,
    ) -> Result<Value, Box<dyn Error>> {
        // Create the contract execution message using Injective's MsgExecuteContractCompat
        let contract_msg = json!({
            "@type": "/injective.wasmx.v1.MsgExecuteContractCompat",
            "sender": sender_address,
            "contract": contract_address,
            "msg": general_purpose::STANDARD.encode(msg.to_string()),
            "funds": ""  // Empty string for Injective
        });
        
        // Sign the message using EIP-712 - detect the actual message type
        let msg_type = if msg.get("commit").is_some() || msg.get("commit_solution").is_some() {
            "commit_solution"
        } else if msg.get("reveal").is_some() || msg.get("reveal_solution").is_some() {
            "reveal_solution"
        } else if msg.get("claim_rewards").is_some() || msg.get("claim_reward").is_some() {
            "claim_reward"
        } else if msg.get("advance_epoch").is_some() {
            "advance_epoch"
        } else if msg.get("finalize_epoch").is_some() {
            "finalize_epoch"
        } else {
            return Err(format!("Unknown message type in: {}", msg).into());
        };
        
        // Extract the inner message data for signing
        let signing_data = match msg_type {
            "commit_solution" => {
                if let Some(commit_msg) = msg.get("commit_solution") {
                    commit_msg.clone()
                } else if let Some(commit_msg) = msg.get("commit") {
                    commit_msg.clone()
                } else {
                    return Err("Missing commit_solution data".into());
                }
            }
            "reveal_solution" => {
                if let Some(reveal_msg) = msg.get("reveal_solution") {
                    reveal_msg.clone()
                } else if let Some(reveal_msg) = msg.get("reveal") {
                    reveal_msg.clone()
                } else {
                    return Err("Missing reveal_solution data".into());
                }
            }
            "claim_reward" => {
                if let Some(claim_msg) = msg.get("claim_reward") {
                    claim_msg.clone()
                } else if let Some(claim_msg) = msg.get("claim_rewards") {
                    claim_msg.clone()
                } else {
                    json!({})
                }
            }
            "advance_epoch" => json!({}),
            "finalize_epoch" => {
                if let Some(finalize_msg) = msg.get("finalize_epoch") {
                    finalize_msg.clone()
                } else {
                    return Err("Missing finalize_epoch data".into());
                }
            }
            _ => return Err(format!("Unknown message type: {}", msg_type).into()),
        };
        
        let signing_result = self.signer.sign_transaction(
            msg_type,
            &signing_data,
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
        
        // Convert hex signature to base64
        let sig_hex = signature.trim_start_matches("0x");
        let sig_bytes = hex::decode(sig_hex)
            .map_err(|e| format!("Failed to decode signature hex: {}", e))?;
        let sig_base64 = general_purpose::STANDARD.encode(&sig_bytes);
        
        // Create Web3Extension
        let web3_extension = ExtensionOptionsWeb3Tx::new_for_testnet();
        
        // Build the complete transaction
        let tx = json!({
            "tx": {
                "body": {
                    "messages": [contract_msg],
                    "memo": memo,
                    "timeout_height": "0",
                    "extension_options": [web3_extension.to_any()?],
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
                    "fee": fee.unwrap_or_default()
                },
                "signatures": [sig_base64]
            },
            "mode": "BROADCAST_MODE_SYNC"
        });
        
        Ok(tx)
    }
    
    /// Submit a transaction to the blockchain
    pub fn submit_transaction(&self, tx: &Value) -> Result<String, Box<dyn Error>> {
        let agent = ureq::Agent::new();
        let url = format!("{}/cosmos/tx/v1beta1/txs", self.rest_url);
        
        log::info!("Submitting transaction to: {}", url);
        log::info!("Transaction: {}", serde_json::to_string_pretty(tx)?);
        
        // Debug: Check signature format
        if let Some(sig) = &tx["tx"]["signatures"][0].as_str() {
            log::info!("Signature base64: {}", sig);
            log::info!("Signature base64 length: {} chars", sig.len());
            
            // Decode to check actual byte length
            if let Ok(decoded) = general_purpose::STANDARD.decode(sig) {
                log::info!("Decoded signature: {} bytes", decoded.len());
            }
        }
        
        match agent.post(&url).send_json(tx) {
            Ok(response) => {
                let result: Value = response.into_json()?;
                
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
            Err(ureq::Error::Status(code, response)) => {
                // Try to get error details from response body
                let error_body = response.into_string().unwrap_or_else(|_| "Unknown error".to_string());
                log::error!("HTTP {} error: {}", code, error_body);
                Err(format!("HTTP request failed: {} - {}", url, error_body).into())
            }
            Err(e) => {
                Err(format!("HTTP request failed: {}", e).into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_transaction_builder() {
        let private_key = [1u8; 32];
        let public_key = [2u8; 33];
        
        let builder = Eip712TransactionBuilder::new(&private_key, &public_key, "testnet")
            .unwrap();
        
        let msg = json!({
            "commit": {
                "commitment": "0".repeat(64)
            }
        });
        
        let tx = builder.build_transaction(
            "inj1test...",
            "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y",
            msg,
            12345,
            0,
            None,
            "",
        ).unwrap();
        
        // Verify transaction structure
        assert!(tx["tx"]["body"]["extension_options"].is_array());
        assert_eq!(
            tx["tx"]["body"]["extension_options"][0]["@type"],
            "/injective.types.v1beta1.ExtensionOptionsWeb3Tx"
        );
    }
}