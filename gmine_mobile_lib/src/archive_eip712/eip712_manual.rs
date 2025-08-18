use crate::types::SigningResult;
use k256::ecdsa::SigningKey;
use alloy_primitives::{keccak256, B256, U256};
use serde_json::{json, Value};
use std::error::Error;
use std::collections::HashMap;

const CHAIN_ID: &str = "injective-888";
const MINING_CONTRACT: &str = "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y";
const ETHEREUM_CHAIN_ID: u64 = 1439; // Testnet

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
        
        // Extract the contract message
        let contract_msg = match msg_type {
            "commit" | "commit_solution" => {
                let commitment = msg_data.get("commitment")
                    .or_else(|| msg_data.get("commit").and_then(|c| c.get("commitment")))
                    .and_then(|v| v.as_str())
                    .ok_or("Missing commitment")?;
                json!({
                    "commit_solution": {
                        "commitment": commitment
                    }
                })
            },
            "reveal" | "reveal_solution" => {
                let nonce = msg_data.get("nonce")
                    .or_else(|| msg_data.get("reveal").and_then(|r| r.get("nonce")))
                    .ok_or("Missing nonce")?;
                
                let nonce_array = if nonce.is_array() {
                    nonce.clone()
                } else if let Some(nonce_str) = nonce.as_str() {
                    serde_json::from_str(nonce_str)?
                } else {
                    return Err("Invalid nonce format".into());
                };
                
                json!({
                    "reveal_solution": {
                        "nonce": nonce_array
                    }
                })
            },
            "claim_rewards" | "claim_reward" => {
                let epoch_number = msg_data.get("epoch_number")
                    .and_then(|v| v.as_u64());
                
                if let Some(epoch) = epoch_number {
                    json!({
                        "claim_reward": {
                            "epoch_number": epoch
                        }
                    })
                } else {
                    json!({
                        "claim_reward": {}
                    })
                }
            },
            "advance_epoch" => {
                json!({
                    "advance_epoch": {}
                })
            },
            "finalize_epoch" => {
                let epoch_number = msg_data.get("epoch_number")
                    .or_else(|| msg_data.get("finalize_epoch").and_then(|f| f.get("epoch_number")))
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing epoch_number for finalize_epoch")?;
                json!({
                    "finalize_epoch": {
                        "epoch_number": epoch_number
                    }
                })
            },
            _ => return Err(format!("Unknown message type: {}", msg_type).into()),
        };
        
        // Convert the contract message to bytes
        let msg_json_str = serde_json::to_string(&contract_msg)?;
        let msg_bytes: Vec<u8> = msg_json_str.as_bytes().to_vec();
        
        // Create the EIP-712 typed data
        let typed_data = json!({
            "types": {
                "EIP712Domain": [
                    {"name": "name", "type": "string"},
                    {"name": "version", "type": "string"},
                    {"name": "chainId", "type": "uint256"},
                    {"name": "verifyingContract", "type": "string"},
                    {"name": "salt", "type": "string"}
                ],
                "Tx": [
                    {"name": "account_number", "type": "string"},
                    {"name": "chain_id", "type": "string"},
                    {"name": "fee", "type": "Fee"},
                    {"name": "memo", "type": "string"},
                    {"name": "msgs", "type": "Msg[]"},
                    {"name": "sequence", "type": "string"},
                    {"name": "timeout_height", "type": "string"}
                ],
                "Fee": [
                    {"name": "amount", "type": "Coin[]"},
                    {"name": "gas", "type": "string"}
                ],
                "Coin": [
                    {"name": "denom", "type": "string"},
                    {"name": "amount", "type": "string"}
                ],
                "Msg": [
                    {"name": "type", "type": "string"},
                    {"name": "value", "type": "MsgValue"}
                ],
                "MsgValue": [
                    {"name": "sender", "type": "string"},
                    {"name": "contract", "type": "string"},
                    {"name": "msg", "type": "uint8[]"},
                    {"name": "funds", "type": "Coin[]"}
                ]
            },
            "primaryType": "Tx",
            "domain": {
                "name": "Injective Web3",
                "version": "1.0.0",
                "chainId": format!("0x{:x}", ETHEREUM_CHAIN_ID),
                "verifyingContract": "cosmos",
                "salt": "0"
            },
            "message": {
                "account_number": account_number.to_string(),
                "chain_id": CHAIN_ID,
                "fee": {
                    "amount": fee.amount.iter().map(|coin| json!({
                        "denom": coin.denom,
                        "amount": coin.amount
                    })).collect::<Vec<_>>(),
                    "gas": fee.gas
                },
                "memo": memo,
                "msgs": vec![json!({
                    "type": "wasm/MsgExecuteContract",
                    "value": {
                        "sender": sender_address,
                        "contract": MINING_CONTRACT,
                        "msg": msg_bytes,
                        "funds": []
                    }
                })],
                "sequence": sequence.to_string(),
                "timeout_height": "0"
            }
        });
        
        // Get the digest to sign
        let digest = self.get_eip712_digest(&typed_data)?;
        
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
    
    /// Calculate the EIP-712 digest
    fn get_eip712_digest(&self, typed_data: &Value) -> Result<B256, Box<dyn Error>> {
        // Get domain separator
        let domain_separator = self.hash_domain(typed_data)?;
        
        // Get message hash
        let message_hash = self.hash_message(typed_data)?;
        
        // Create the final digest: keccak256("\x19\x01" || domainSeparator || messageHash)
        let mut final_message = Vec::with_capacity(2 + 32 + 32);
        final_message.extend_from_slice(b"\x19\x01");
        final_message.extend_from_slice(&domain_separator);
        final_message.extend_from_slice(&message_hash);
        
        Ok(keccak256(&final_message))
    }
    
    /// Hash the domain according to EIP-712
    fn hash_domain(&self, typed_data: &Value) -> Result<[u8; 32], Box<dyn Error>> {
        let domain = &typed_data["domain"];
        
        // Hash the domain type
        let domain_type_hash = keccak256(
            b"EIP712Domain(string name,string version,uint256 chainId,string verifyingContract,string salt)"
        );
        
        // Encode domain values
        let name_hash = keccak256(domain["name"].as_str().unwrap_or("").as_bytes());
        let version_hash = keccak256(domain["version"].as_str().unwrap_or("").as_bytes());
        
        // Parse chainId from hex string
        let chain_id_str = domain["chainId"].as_str().unwrap_or("0x1");
        let chain_id_hex = chain_id_str.trim_start_matches("0x");
        let chain_id = u64::from_str_radix(chain_id_hex, 16).unwrap_or(1);
        let chain_id_u256 = U256::from(chain_id);
        
        let verifying_contract_hash = keccak256(domain["verifyingContract"].as_str().unwrap_or("").as_bytes());
        let salt_hash = keccak256(domain["salt"].as_str().unwrap_or("0").as_bytes());
        
        // ABI encode
        let mut encoded = Vec::new();
        encoded.extend_from_slice(domain_type_hash.as_ref());
        encoded.extend_from_slice(name_hash.as_ref());
        encoded.extend_from_slice(version_hash.as_ref());
        encoded.extend_from_slice(&chain_id_u256.to_be_bytes::<32>());
        encoded.extend_from_slice(verifying_contract_hash.as_ref());
        encoded.extend_from_slice(salt_hash.as_ref());
        
        Ok(keccak256(&encoded).into())
    }
    
    /// Hash the message according to EIP-712
    fn hash_message(&self, typed_data: &Value) -> Result<[u8; 32], Box<dyn Error>> {
        let message = &typed_data["message"];
        let types = &typed_data["types"];
        let primary_type = typed_data["primaryType"].as_str().unwrap_or("Tx");
        
        self.hash_struct(primary_type, message, types)
    }
    
    /// Hash a struct according to EIP-712
    fn hash_struct(&self, type_name: &str, data: &Value, types: &Value) -> Result<[u8; 32], Box<dyn Error>> {
        let type_hash = self.get_type_hash(type_name, types)?;
        let encoded_data = self.encode_data(type_name, data, types)?;
        
        let mut result = Vec::new();
        result.extend_from_slice(&type_hash);
        result.extend_from_slice(&encoded_data);
        
        Ok(keccak256(&result).into())
    }
    
    /// Get the type hash for a struct
    fn get_type_hash(&self, type_name: &str, types: &Value) -> Result<[u8; 32], Box<dyn Error>> {
        let type_string = self.encode_type(type_name, types)?;
        Ok(keccak256(type_string.as_bytes()).into())
    }
    
    /// Encode the type definition
    fn encode_type(&self, type_name: &str, types: &Value) -> Result<String, Box<dyn Error>> {
        let mut deps = HashMap::new();
        self.find_dependencies(type_name, types, &mut deps)?;
        
        let mut sorted_deps: Vec<_> = deps.into_iter().collect();
        sorted_deps.sort_by(|a, b| a.0.cmp(&b.0));
        
        let mut result = format!("{}{}", type_name, self.format_type_members(type_name, types)?);
        
        for (dep_name, _) in sorted_deps {
            if dep_name != type_name {
                result.push_str(&format!("{}{}", dep_name, self.format_type_members(&dep_name, types)?));
            }
        }
        
        Ok(result)
    }
    
    /// Format type members
    fn format_type_members(&self, type_name: &str, types: &Value) -> Result<String, Box<dyn Error>> {
        let type_def = types[type_name].as_array()
            .ok_or_else(|| format!("Type {} not found", type_name))?;
        
        let members: Vec<String> = type_def.iter()
            .filter_map(|field| {
                let name = field["name"].as_str()?;
                let type_ = field["type"].as_str()?;
                Some(format!("{} {}", type_, name))
            })
            .collect();
        
        Ok(format!("({})", members.join(",")))
    }
    
    /// Find dependencies for a type
    fn find_dependencies(&self, type_name: &str, types: &Value, deps: &mut HashMap<String, bool>) -> Result<(), Box<dyn Error>> {
        if deps.contains_key(type_name) {
            return Ok(());
        }
        
        deps.insert(type_name.to_string(), true);
        
        if let Some(type_def) = types[type_name].as_array() {
            for field in type_def {
                if let Some(field_type) = field["type"].as_str() {
                    let base_type = field_type.trim_end_matches("[]");
                    if types[base_type].is_array() {
                        self.find_dependencies(base_type, types, deps)?;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Encode data according to type
    fn encode_data(&self, type_name: &str, data: &Value, types: &Value) -> Result<Vec<u8>, Box<dyn Error>> {
        let type_def = types[type_name].as_array()
            .ok_or_else(|| format!("Type {} not found", type_name))?;
        
        let mut encoded = Vec::new();
        
        for field in type_def {
            let field_name = field["name"].as_str().unwrap_or("");
            let field_type = field["type"].as_str().unwrap_or("");
            let field_value = &data[field_name];
            
            encoded.extend_from_slice(&self.encode_field(field_type, field_value, types)?);
        }
        
        Ok(encoded)
    }
    
    /// Encode a single field
    fn encode_field(&self, field_type: &str, value: &Value, types: &Value) -> Result<[u8; 32], Box<dyn Error>> {
        // Handle arrays
        if field_type.ends_with("[]") {
            let base_type = field_type.trim_end_matches("[]");
            if let Some(array) = value.as_array() {
                let mut encoded_items = Vec::new();
                for item in array {
                    if types[base_type].is_array() {
                        // Struct array
                        let item_hash = self.hash_struct(base_type, item, types)?;
                        encoded_items.extend_from_slice(&item_hash);
                    } else {
                        // Primitive array
                        encoded_items.extend_from_slice(&self.encode_field(base_type, item, types)?);
                    }
                }
                return Ok(keccak256(&encoded_items).into());
            } else {
                // Empty array
                return Ok(keccak256(&[]).into());
            }
        }
        
        // Handle structs
        if types[field_type].is_array() {
            return self.hash_struct(field_type, value, types);
        }
        
        // Handle primitives
        match field_type {
            "string" => {
                let str_val = value.as_str().unwrap_or("");
                Ok(keccak256(str_val.as_bytes()).into())
            },
            "uint256" => {
                let num = if let Some(n) = value.as_u64() {
                    U256::from(n)
                } else if let Some(s) = value.as_str() {
                    U256::from_str_radix(s, 10).unwrap_or_default()
                } else {
                    U256::ZERO
                };
                Ok(num.to_be_bytes())
            },
            "uint8[]" => {
                // Special handling for bytes
                if let Some(bytes) = value.as_array() {
                    let byte_vec: Vec<u8> = bytes.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as u8))
                        .collect();
                    Ok(keccak256(&byte_vec).into())
                } else {
                    Ok(keccak256(&[]).into())
                }
            },
            "uint8" => {
                // Handle individual uint8 values
                let mut result = [0u8; 32];
                if let Some(num) = value.as_u64() {
                    result[31] = num as u8;
                }
                Ok(result)
            },
            _ => {
                Err(format!("Unknown type: {}", field_type).into())
            }
        }
    }
}