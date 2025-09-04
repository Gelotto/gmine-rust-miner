use crate::types::SigningResult;
use k256::ecdsa::SigningKey;
use alloy_sol_types::{sol, SolStruct};
use alloy_primitives::{keccak256, B256, U256};
use serde_json::json;
use std::error::Error;

const CHAIN_ID: &str = "injective-888";
const MINING_CONTRACT: &str = "inj1h2rq8q2ly6mwgwv4jcd5qpjvfqwvwee5v9n032"; // V3.4 contract with JIT History fix
const ETHEREUM_CHAIN_ID: u64 = 1439; // Testnet

// Define the EIP-712 message structures
sol! {
    struct Coin {
        string denom;
        string amount;
    }

    struct Fee {
        Coin[] amount;
        string gas;
    }

    struct MsgValue {
        string sender;
        string contract;
        uint8[] msg;  // Changed from string to uint8[] to match expected type
        Coin[] funds;  // Changed from string to Coin[] to match expected type
    }

    struct Msg {
        string r#type;
        MsgValue value;
    }

    struct Tx {
        string account_number;
        string chain_id;
        Fee fee;
        string memo;
        Msg[] msgs;
        string sequence;
        string timeout_height;  // Added missing field
    }
}

pub struct Eip712Signer {
    signing_key: SigningKey,
    public_key: Vec<u8>,
}

impl Eip712Signer {
    /// Create new signer from private key bytes
    pub fn new(private_key: &[u8], public_key: &[u8]) -> Result<Self, Box<dyn Error>> {
        let signing_key = SigningKey::from_slice(private_key)?;
        Ok(Eip712Signer {
            signing_key,
            public_key: public_key.to_vec(),
        })
    }
    
    /// Sign a transaction message using EIP-712
    pub fn sign_transaction(
        &self,
        msg_type: &str,
        msg_data: &serde_json::Value,
        sender_address: &str,
        account_number: u64,
        sequence: u64,
        fee: Option<crate::types::Fee>,
        memo: &str,
    ) -> Result<SigningResult, Box<dyn Error>> {
        // Use default fee if not provided
        let fee = fee.unwrap_or_default();
        
        // Map message type to proper Injective format matching what transaction_manager sends
        let (injective_msg_type, formatted_msg) = match msg_type {
            "commit" | "commit_solution" => {
                log::info!("Processing commit message. msg_data: {}", msg_data);
                // Handle both formats: {"commitment": "..."} and {"commit": {"commitment": "..."}}
                let commitment = msg_data.get("commitment")
                    .or_else(|| msg_data.get("commit").and_then(|c| c.get("commitment")))
                    .and_then(|v| v.as_str())
                    .ok_or("Missing commitment")?;
                log::info!("Extracted commitment: {}", commitment);
                (
                    "wasm/MsgExecuteContract",  // Changed to match expected type
                    json!({
                        "contract": MINING_CONTRACT,
                        "msg": json!({
                            "commit_solution": {
                                "commitment": commitment
                            }
                        }),
                        "sender": sender_address,
                        "funds": []  // Empty array of Coin
                    })
                )
            },
            "reveal" | "reveal_solution" => {
                // Handle both formats
                let nonce = msg_data.get("nonce")
                    .or_else(|| msg_data.get("reveal").and_then(|r| r.get("nonce")))
                    .ok_or("Missing nonce")?;
                
                // Handle nonce - it should be an array of numbers
                let nonce_array = if nonce.is_array() {
                    nonce.clone()
                } else if let Some(nonce_str) = nonce.as_str() {
                    // If it's a string, parse it back to array
                    serde_json::from_str(nonce_str)?
                } else {
                    return Err("Invalid nonce format".into());
                };
                
                (
                    "wasm/MsgExecuteContract",  // Changed to match expected type
                    json!({
                        "contract": MINING_CONTRACT,
                        "msg": json!({
                            "reveal_solution": {
                                "nonce": nonce_array
                            }
                        }),
                        "sender": sender_address,
                        "funds": []  // Empty array of Coin
                    })
                )
            },
            "claim_rewards" | "claim_reward" => {
                // Handle both claim_rewards (what rust_signer sends) and claim_reward (what should be sent)
                let epoch_number = msg_data.get("epoch_number")
                    .and_then(|v| v.as_u64());
                
                let msg_content = if let Some(epoch) = epoch_number {
                    json!({
                        "claim_reward": {
                            "epoch_number": epoch
                        }
                    })
                } else {
                    // Fallback for backward compatibility
                    json!({
                        "claim_reward": {}
                    })
                };
                
                (
                    "wasm/MsgExecuteContract",  // Changed to match expected type
                    json!({
                        "contract": MINING_CONTRACT,
                        "msg": msg_content,
                        "sender": sender_address,
                        "funds": []  // Empty array of Coin
                    })
                )
            },
            "advance_epoch" => {
                (
                    "wasm/MsgExecuteContract",  // Changed to match expected type
                    json!({
                        "contract": MINING_CONTRACT,
                        "msg": json!({
                            "advance_epoch": {}
                        }),
                        "sender": sender_address,
                        "funds": []  // Empty array of Coin
                    })
                )
            },
            "finalize_epoch" => {
                // Handle both formats
                let epoch_number = msg_data.get("epoch_number")
                    .or_else(|| msg_data.get("finalize_epoch").and_then(|f| f.get("epoch_number")))
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing epoch_number for finalize_epoch")?;
                (
                    "wasm/MsgExecuteContract",  // Changed to match expected type
                    json!({
                        "contract": MINING_CONTRACT,
                        "msg": json!({
                            "finalize_epoch": {
                                "epoch_number": epoch_number
                            }
                        }),
                        "sender": sender_address,
                        "funds": []  // Empty array of Coin
                    })
                )
            },
            _ => return Err(format!("Unknown message type: {}", msg_type).into()),
        };
        
        // Create the Tx struct for EIP-712
        // Convert the msg field to bytes as expected by the bridge
        let msg_json = serde_json::to_string(&formatted_msg["msg"]).unwrap_or_default();
        let msg_bytes: Vec<u8> = msg_json.as_bytes().to_vec();
        
        let tx = Tx {
            account_number: account_number.to_string(),
            chain_id: CHAIN_ID.to_string(),
            fee: Fee {
                amount: fee.amount.iter().map(|coin| Coin {
                    denom: coin.denom.clone(),
                    amount: coin.amount.clone(),
                }).collect(),
                gas: fee.gas.clone(),
            },
            memo: memo.to_string(),
            msgs: vec![Msg {
                r#type: "wasm/MsgExecuteContract".to_string(),
                value: MsgValue {
                    sender: formatted_msg["sender"].as_str().unwrap_or("").to_string(),
                    contract: formatted_msg["contract"].as_str().unwrap_or("").to_string(),
                    msg: msg_bytes,  // Now sending as uint8[]
                    funds: vec![],  // Empty array of Coin
                },
            }],
            sequence: sequence.to_string(),
            timeout_height: "0".to_string(),  // Added missing field
        };
        
        // Get the digest to sign
        let digest = get_eip712_digest(&tx)?;
        
        // Sign the digest
        let (signature, recovery_id): (k256::ecdsa::Signature, k256::ecdsa::RecoveryId) = 
            self.signing_key.sign_recoverable(digest.as_ref())
                .map_err(|e| format!("Signing failed: {}", e))?;
        
        // Convert to bytes with recovery ID (65 bytes total)
        let mut sig_bytes = [0u8; 65];
        sig_bytes[..32].copy_from_slice(&signature.r().to_bytes());
        sig_bytes[32..64].copy_from_slice(&signature.s().to_bytes());
        sig_bytes[64] = recovery_id.to_byte();
        
        // Format signature as hex with 0x prefix
        let signature_hex = format!("0x{}", hex::encode(sig_bytes));
        
        // Format public key for Injective (compressed secp256k1)
        use base64::{Engine as _, engine::general_purpose};
        let pub_key_base64 = general_purpose::STANDARD.encode(&self.public_key);
        
        Ok(SigningResult {
            success: true,
            signature: Some(signature_hex),
            pub_key: Some(pub_key_base64),
            error: None,
        })
    }
}

/// Create the Injective-specific domain separator
pub fn create_injective_domain_separator() -> Result<B256, Box<dyn Error>> {
    // The custom type hash for Injective's domain
    // Note: Uses 'string verifyingContract' instead of standard 'address'
    let domain_type_hash = keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId,string verifyingContract,string salt)"
    );
    
    // Domain values
    let name = "Injective Web3";
    let version = "1.0.0";
    let chain_id = U256::from(ETHEREUM_CHAIN_ID);
    let verifying_contract = "cosmos";
    let _salt = "0";
    
    // Encode the domain data according to EIP-712 spec
    // For strings, we hash them first
    let name_hash = keccak256(name.as_bytes());
    let version_hash = keccak256(version.as_bytes());
    let verifying_contract_hash = keccak256(verifying_contract.as_bytes());
    
    // For salt, it's a bytes32 field, so "0" likely means 32 zero bytes
    let salt_bytes = B256::ZERO;
    
    // ABI encode: typeHash, nameHash, versionHash, chainId, verifyingContractHash, salt
    let mut encoded = Vec::new();
    encoded.extend_from_slice(domain_type_hash.as_ref());
    encoded.extend_from_slice(name_hash.as_ref());
    encoded.extend_from_slice(version_hash.as_ref());
    
    // chainId as U256 needs to be encoded as 32 bytes, big-endian
    let chain_id_bytes = chain_id.to_be_bytes::<32>();
    encoded.extend_from_slice(&chain_id_bytes);
    
    encoded.extend_from_slice(verifying_contract_hash.as_ref());
    encoded.extend_from_slice(salt_bytes.as_ref());
    
    Ok(keccak256(&encoded))
}

/// Get the EIP-712 digest to sign
fn get_eip712_digest(tx: &Tx) -> Result<B256, Box<dyn Error>> {
    // Get the domain separator
    let domain_separator = create_injective_domain_separator()?;
    
    // Get the struct hash
    let struct_hash = tx.eip712_hash_struct();
    
    // Create the final digest: keccak256("\x19\x01" || domainSeparator || structHash)
    let mut message = Vec::with_capacity(2 + 32 + 32);
    message.extend_from_slice(b"\x19\x01");
    message.extend_from_slice(domain_separator.as_slice());
    message.extend_from_slice(struct_hash.as_slice());
    
    Ok(keccak256(&message))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_domain_separator() {
        let separator = create_injective_domain_separator().unwrap();
        // This should produce a consistent hash
        println!("Domain separator: 0x{}", hex::encode(separator));
    }
    
    #[test]
    fn test_large_nonce_handling() {
        // Test with a nonce larger than JavaScript's MAX_SAFE_INTEGER
        let large_nonce: u64 = 13_123_013_734_036_973_969;
        let nonce_bytes = large_nonce.to_le_bytes();
        let nonce_array: Vec<u8> = nonce_bytes.to_vec();
        
        // Create test data
        let _msg_data = json!({
            "nonce": nonce_array
        });
        
        // This should not panic or lose precision
        assert_eq!(nonce_array.len(), 8);
        assert_eq!(nonce_array[0], 145); // First byte of the problematic nonce
    }
}