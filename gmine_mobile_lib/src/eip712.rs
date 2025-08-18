use crate::types::{Fee, SigningResult};
use k256::ecdsa::{SigningKey, Signature, signature::Signer};
use sha3::{Digest, Keccak256};
use serde_json::json;
use std::error::Error;

const CHAIN_ID: &str = "injective-888";
const MINING_CONTRACT: &str = "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y";

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
        fee: Option<Fee>,
        memo: &str,
    ) -> Result<SigningResult, Box<dyn Error>> {
        eprintln!("EIP712::sign_transaction called with msg_type: {}", msg_type);
        // Use default fee if not provided
        let fee = fee.unwrap_or_default();
        
        eprintln!("EIP712: Building typed data...");
        // Build EIP-712 typed data
        let typed_data = match build_typed_data(
            msg_type,
            msg_data,
            sender_address,
            account_number,
            sequence,
            &fee,
            memo,
        ) {
            Ok(data) => {
                eprintln!("EIP712: Typed data built successfully");
                data
            }
            Err(e) => {
                eprintln!("EIP712: Failed to build typed data: {}", e);
                return Err(e);
            }
        };
        
        // Hash the typed data
        let hash = hash_typed_data(&typed_data)?;
        eprintln!("EIP712: Hash computed, length: {}", hash.len());
        
        // Sign using secp256k1 for recoverable signatures
        // Convert k256 key to secp256k1 format
        let secret_key = secp256k1::SecretKey::from_slice(&self.signing_key.to_bytes())
            .map_err(|e| format!("Failed to convert key: {}", e))?;
        
        // Create secp256k1 context
        let secp = secp256k1::Secp256k1::new();
        
        // Create message from hash
        let message = secp256k1::Message::from_digest_slice(&hash)
            .map_err(|e| format!("Invalid message hash: {}", e))?;
        
        // Sign with recovery
        let recoverable_sig = secp.sign_ecdsa_recoverable(&message, &secret_key);
        let (recovery_id, signature) = recoverable_sig.serialize_compact();
        
        eprintln!("EIP712: After signing, signature length: {}", signature.len());
        
        // Build 65-byte signature: r (32) + s (32) + v (1)
        // For EIP-712, v should be 27 or 28 (or rarely 29/30)
        let mut full_sig = Vec::with_capacity(65);
        full_sig.extend_from_slice(&signature);
        let v_value = recovery_id.to_i32() as u8 + 27;
        full_sig.push(v_value);
        
        // Debug logging to stderr
        eprintln!("DEBUG EIP712: Signature bytes length: {}", signature.len());
        eprintln!("DEBUG EIP712: Recovery ID raw: {}", recovery_id.to_i32());
        eprintln!("DEBUG EIP712: V value: {}", v_value);
        eprintln!("DEBUG EIP712: Full signature length: {}", full_sig.len());
        
        // Debug each component
        eprintln!("DEBUG EIP712: R (first 32 bytes): {}", hex::encode(&signature[0..32]));
        eprintln!("DEBUG EIP712: S (next 32 bytes): {}", hex::encode(&signature[32..64]));
        eprintln!("DEBUG EIP712: V byte: {:02x}", v_value);
        
        // Format signature as hex with 0x prefix
        let signature_hex = format!("0x{}", hex::encode(&full_sig));
        eprintln!("DEBUG EIP712: Final hex signature: {}", signature_hex);
        eprintln!("DEBUG EIP712: Hex length with 0x: {}", signature_hex.len());
        eprintln!("DEBUG EIP712: Hex length without 0x: {}", signature_hex.len() - 2);
        
        // Also log the hash that was signed
        eprintln!("DEBUG EIP712: Message hash that was signed: {}", hex::encode(&hash));
        
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

/// Build EIP-712 typed data for Injective
fn build_typed_data(
    msg_type: &str,
    msg_data: &serde_json::Value,
    sender_address: &str,
    account_number: u64,
    sequence: u64,
    fee: &Fee,
    memo: &str,
) -> Result<serde_json::Value, Box<dyn Error>> {
    // Map message type to proper Injective format matching what transaction_manager sends
    let (injective_msg_type, formatted_msg) = match msg_type {
        "commit" | "commit_solution" => {
            log::info!("Processing commit message. msg_data: {}", msg_data);
            // Keep commitment as is - don't convert to base64
            let commitment = msg_data.get("commitment")
                .ok_or("Missing commitment field")?;
            (
                "wasmx/MsgExecuteContractCompat",
                json!({
                    "@type": "/injective.wasmx.v1.MsgExecuteContractCompat",
                    "contract": MINING_CONTRACT,
                    "msg": {
                        "commit_solution": {
                            "commitment": commitment
                        }
                    },
                    "sender": sender_address,
                    "funds": ""
                })
            )
        },
        "reveal" | "reveal_solution" => {
            // Keep arrays as is - don't convert to base64
            let nonce = msg_data.get("nonce")
                .ok_or("Missing nonce field")?;
            let digest = msg_data.get("digest")
                .ok_or("Missing digest field")?;
            let salt = msg_data.get("salt")
                .ok_or("Missing salt field")?;
            
            (
                "wasmx/MsgExecuteContractCompat",
                json!({
                    "@type": "/injective.wasmx.v1.MsgExecuteContractCompat",
                    "contract": MINING_CONTRACT,
                    "msg": {
                        "reveal_solution": {
                            "nonce": nonce,
                            "digest": digest,
                            "salt": salt
                        }
                    },
                    "sender": sender_address,
                    "funds": ""
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
                "wasmx/MsgExecuteContractCompat",
                json!({
                    "@type": "/injective.wasmx.v1.MsgExecuteContractCompat",
                    "contract": MINING_CONTRACT,
                    "msg": msg_content,
                    "sender": sender_address,
                    "funds": ""
                })
            )
        },
        "advance_epoch" => {
            (
                "wasmx/MsgExecuteContractCompat",
                json!({
                    "@type": "/injective.wasmx.v1.MsgExecuteContractCompat",
                    "contract": MINING_CONTRACT,
                    "msg": {
                        "advance_epoch": {}
                    },
                    "sender": sender_address,
                    "funds": ""
                })
            )
        },
        "finalize_epoch" => {
            let epoch_number = msg_data.get("epoch_number")
                .and_then(|v| v.as_u64())
                .ok_or("Missing epoch_number for finalize_epoch")?;
            (
                "wasmx/MsgExecuteContractCompat",
                json!({
                    "@type": "/injective.wasmx.v1.MsgExecuteContractCompat",
                    "contract": MINING_CONTRACT,
                    "msg": {
                        "finalize_epoch": {
                            "epoch_number": epoch_number
                        }
                    },
                    "sender": sender_address,
                    "funds": ""
                })
            )
        },
        _ => return Err(format!("Unknown message type: {} with data: {}", msg_type, msg_data).into()),
    };
    
    // Get msg as object for EIP-712 (will be stringified later for protobuf)
    let msg_content_obj = formatted_msg.get("msg").ok_or("Missing msg field")?.clone();
    
    // For MsgExecuteContractCompat, funds is a string field ("" for empty, matching Node.js)
    let funds_str = if let Some(funds_array) = formatted_msg.get("funds").and_then(|v| v.as_array()) {
        if funds_array.is_empty() {
            "0".to_string()  // Chain expects "0" for empty funds
        } else {
            // Format as comma-separated string like "100inj,200usdt"
            funds_array.iter()
                .filter_map(|coin| {
                    let denom = coin.get("denom")?.as_str()?;
                    let amount = coin.get("amount")?.as_str()?;
                    Some(format!("{}{}", amount, denom))
                })
                .collect::<Vec<_>>()
                .join(",")
        }
    } else {
        "0".to_string()  // Chain expects "0" for empty funds
    };
    
    // Convert msg to string for EIP-712 signing (Injective expects this)
    let msg_str = serde_json::to_string(&msg_content_obj)
        .map_err(|e| format!("Failed to stringify msg: {}", e))?;
    
    // CRITICAL: Field order must match the type definition for EIP-712!
    // The MsgValue type defines fields in order: sender, contract, msg, funds
    // We must create the object with fields in that exact order
    // Using a Vec to maintain order instead of json! macro which doesn't guarantee order
    let mut msg_value_map = serde_json::Map::new();
    // Insert in the exact order required by the type definition
    msg_value_map.insert("sender".to_string(), formatted_msg.get("sender").unwrap_or(&json!("")).clone());
    msg_value_map.insert("contract".to_string(), formatted_msg.get("contract").unwrap_or(&json!(MINING_CONTRACT)).clone());
    msg_value_map.insert("msg".to_string(), json!(msg_str));            // String for EIP-712 (Injective requirement)
    msg_value_map.insert("funds".to_string(), json!(funds_str));        // String to match protobuf
    let msg_value = serde_json::Value::Object(msg_value_map);
    
    // Debug log the msg_value to verify structure
    log::info!("EIP-712 msg_value: {}", serde_json::to_string_pretty(&msg_value).unwrap_or_default());
    
    // Build EIP-712 structure with dynamic types based on message type
    let mut types = serde_json::Map::new();
    
    // Standard types that are always the same
    types.insert("EIP712Domain".to_string(), json!([
        { "name": "name", "type": "string" },
        { "name": "version", "type": "string" },
        { "name": "chainId", "type": "uint256" },
        { "name": "verifyingContract", "type": "string" },
        { "name": "salt", "type": "string" }
    ]));
    
    types.insert("Tx".to_string(), json!([
        { "name": "account_number", "type": "string" },
        { "name": "chain_id", "type": "string" },
        { "name": "fee", "type": "Fee" },
        { "name": "memo", "type": "string" },
        { "name": "msgs", "type": "Msg[]" },
        { "name": "sequence", "type": "string" },
        { "name": "timeout_height", "type": "string" }
    ]));
    
    types.insert("Fee".to_string(), json!([
        { "name": "amount", "type": "Coin[]" },
        { "name": "gas", "type": "string" }
    ]));
    
    types.insert("Coin".to_string(), json!([
        { "name": "denom", "type": "string" },
        { "name": "amount", "type": "string" }
    ]));
    
    types.insert("Msg".to_string(), json!([
        { "name": "type", "type": "string" },
        { "name": "value", "type": "MsgValue" }
    ]));
    
    // For all message types, use string for msg field (Injective requirement)
    types.insert("MsgValue".to_string(), json!([
        { "name": "sender", "type": "string" },
        { "name": "contract", "type": "string" },
        { "name": "msg", "type": "string" },
        { "name": "funds", "type": "string" }
    ]));
    
    let typed_data = json!({
        "types": types,
        "primaryType": "Tx",
        "domain": {
            "name": "Injective Web3",
            "version": "1.0.0",
            "chainId": "0x59f", // Hex format for 1439 (Ethereum chain ID that chain expects)
            "verifyingContract": "cosmos",
            "salt": "0"
        },
        "message": {
            "account_number": account_number.to_string(),
            "chain_id": CHAIN_ID,
            "fee": {
                "amount": fee.amount.iter().map(|c| json!({
                    "denom": c.denom,
                    "amount": c.amount
                })).collect::<Vec<_>>(),
                "gas": fee.gas
            },
            "memo": memo,
            "msgs": [{
                "type": "wasmx/MsgExecuteContractCompat", // Use wasmx amino type
                "value": msg_value
            }],
            "sequence": sequence.to_string(),
            "timeout_height": "0" // Add required timeout_height field
        }
    });
    
    // Debug the actual typed data being built
    eprintln!("EIP712: Built typed data with chainId: {}", 
        typed_data.get("domain")
            .and_then(|d| d.get("chainId"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
    );
    
    // Log the complete EIP-712 payload for comparison with Node.js
    eprintln!("=== RUST EIP-712 COMPLETE PAYLOAD ===");
    eprintln!("{}", serde_json::to_string_pretty(&typed_data).unwrap_or_default());
    eprintln!("=== END RUST EIP-712 PAYLOAD ===");
    
    // Also log individual components to check canonicalization
    eprintln!("=== RUST EIP-712 DOMAIN (raw) ===");
    if let Some(domain) = typed_data.get("domain") {
        eprintln!("{}", serde_json::to_string(domain).unwrap_or_default());
    }
    eprintln!("=== RUST EIP-712 MESSAGE (raw) ===");
    if let Some(message) = typed_data.get("message") {
        eprintln!("{}", serde_json::to_string(message).unwrap_or_default());
    }
    
    // Check if msg_value fields are in correct order
    eprintln!("=== RUST MSG_VALUE OBJECT ===");
    eprintln!("{}", serde_json::to_string(&msg_value).unwrap_or_default());
    
    Ok(typed_data)
}

/// Hash EIP-712 typed data according to the standard
fn hash_typed_data(typed_data: &serde_json::Value) -> Result<[u8; 32], Box<dyn Error>> {
    eprintln!("EIP712: hash_typed_data called");
    
    // Extract the required parts
    let types = typed_data.get("types")
        .ok_or("Missing types")?;
    let primary_type = typed_data.get("primaryType")
        .and_then(|v| v.as_str())
        .ok_or("Missing primaryType")?;
    let domain = typed_data.get("domain")
        .ok_or("Missing domain")?;
    let message = typed_data.get("message")
        .ok_or("Missing message")?;
    
    eprintln!("EIP712: Hashing domain...");
    // Hash the domain
    let domain_separator = hash_struct("EIP712Domain", domain, types)?;
    eprintln!("EIP712: Domain separator hash: 0x{}", hex::encode(&domain_separator));
    
    eprintln!("EIP712: Hashing message...");
    // Hash the message
    let message_hash = hash_struct(primary_type, message, types)?;
    eprintln!("EIP712: Message hash: 0x{}", hex::encode(&message_hash));
    
    // Combine according to EIP-712 spec
    let mut hasher = Keccak256::new();
    hasher.update(b"\x19\x01");
    hasher.update(domain_separator);
    hasher.update(message_hash);
    let result = hasher.finalize();
    
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    eprintln!("EIP712: Final hash to sign: 0x{}", hex::encode(&hash));
    Ok(hash)
}

/// Hash a struct according to EIP-712
fn hash_struct(
    type_name: &str,
    data: &serde_json::Value,
    types: &serde_json::Value
) -> Result<[u8; 32], Box<dyn Error>> {
    eprintln!("DEBUG EIP712: hash_struct called for type '{}'", type_name);
    eprintln!("DEBUG EIP712: data: {}", serde_json::to_string_pretty(data).unwrap_or_default());
    
    // Get the type definition
    let type_def = types.get(type_name)
        .ok_or(format!("Type {} not found", type_name))?
        .as_array()
        .ok_or("Type definition must be array")?;
    
    // Start with the type hash
    let mut encoded = Vec::new();
    let type_hash = hash_type(type_name, types)?;
    encoded.extend_from_slice(&type_hash);
    
    // Encode each field
    for field in type_def {
        let field_name = field.get("name")
            .and_then(|v| v.as_str())
            .ok_or("Field missing name")?;
        let field_type = field.get("type")
            .and_then(|v| v.as_str())
            .ok_or("Field missing type")?;
        
        let field_value = data.get(field_name);
        if let Some(value) = field_value {
            let encoded_value = encode_value(field_type, value, types)?;
            encoded.extend_from_slice(&encoded_value);
            eprintln!("DEBUG EIP712: Field '{}' (type: {}) encoded to: 0x{}", 
                field_name, field_type, hex::encode(&encoded_value));
        } else {
            // Missing fields are encoded as zero
            encoded.extend_from_slice(&[0u8; 32]);
            eprintln!("DEBUG EIP712: Field '{}' is missing, encoded as zeros", field_name);
        }
    }
    
    // Hash the encoded struct
    let mut hasher = Keccak256::new();
    hasher.update(&encoded);
    let result = hasher.finalize();
    
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    eprintln!("DEBUG EIP712: Struct '{}' hash: 0x{}", type_name, hex::encode(&hash));
    Ok(hash)
}

/// Hash a type string according to EIP-712
fn hash_type(type_name: &str, types: &serde_json::Value) -> Result<[u8; 32], Box<dyn Error>> {
    let type_string = encode_type(type_name, types)?;
    eprintln!("DEBUG EIP712: Type '{}' encoded as: {}", type_name, type_string);
    let mut hasher = Keccak256::new();
    hasher.update(type_string.as_bytes());
    let result = hasher.finalize();
    
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    eprintln!("DEBUG EIP712: Type '{}' hash: 0x{}", type_name, hex::encode(&hash));
    Ok(hash)
}

/// Recursively collect all types referenced by the given type
fn collect_referenced_types(
    type_name: &str,
    types: &serde_json::Value,
    collected: &mut std::collections::HashSet<String>
) -> Result<(), Box<dyn Error>> {
    // Skip if already collected or if it's a primitive type
    if collected.contains(type_name) || !types.get(type_name).is_some() {
        return Ok(());
    }
    
    // Add this type to collected
    collected.insert(type_name.to_string());
    
    // Get type definition
    let type_def = types.get(type_name)
        .ok_or(format!("Type {} not found", type_name))?
        .as_array()
        .ok_or("Type definition must be array")?;
    
    // Process each field
    for field in type_def {
        let field_type = field.get("type")
            .and_then(|v| v.as_str())
            .ok_or("Field missing type")?;
        
        // Extract base type (remove array notation)
        let base_type = if field_type.ends_with("[]") {
            &field_type[..field_type.len() - 2]
        } else {
            field_type
        };
        
        // Recursively collect if it's a custom type
        if types.get(base_type).is_some() {
            collect_referenced_types(base_type, types, collected)?;
        }
    }
    
    Ok(())
}

/// Encode a type and its dependencies as a string according to EIP-712 spec
fn encode_type(type_name: &str, types: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    // First, collect all referenced types recursively
    let mut referenced_types = std::collections::HashSet::new();
    collect_referenced_types(type_name, types, &mut referenced_types)?;
    
    // Sort types: primary type first, then others alphabetically
    let mut sorted_types: Vec<&str> = referenced_types.iter().map(|s| s.as_str()).collect();
    sorted_types.retain(|&t| t != type_name); // Remove primary type from list
    sorted_types.sort(); // Sort remaining alphabetically
    sorted_types.insert(0, type_name); // Put primary type first
    
    eprintln!("DEBUG EIP712: encode_type for '{}' - sorted types: {:?}", type_name, sorted_types);
    
    // Build the encoded string
    let mut encoded = String::new();
    
    for &current_type in sorted_types.iter() {
        let type_def = types.get(current_type)
            .ok_or(format!("Type {} not found", current_type))?
            .as_array()
            .ok_or("Type definition must be array")?;
        
        // Add type name and opening parenthesis
        encoded.push_str(current_type);
        encoded.push('(');
        
        // Add fields
        for (i, field) in type_def.iter().enumerate() {
            if i > 0 {
                encoded.push(',');
            }
            let field_type = field.get("type")
                .and_then(|v| v.as_str())
                .ok_or("Field missing type")?;
            let field_name = field.get("name")
                .and_then(|v| v.as_str())
                .ok_or("Field missing name")?;
            encoded.push_str(field_type);
            encoded.push(' ');
            encoded.push_str(field_name);
        }
        
        // Close parenthesis
        encoded.push(')');
    }
    
    Ok(encoded)
}

/// Encode a value according to its type
fn encode_value(
    field_type: &str,
    value: &serde_json::Value,
    types: &serde_json::Value
) -> Result<[u8; 32], Box<dyn Error>> {
    // Handle arrays
    if field_type.ends_with("[]") {
        let base_type = &field_type[..field_type.len() - 2];
        if let Some(array) = value.as_array() {
            let mut encoded_values = Vec::new();
            for item in array {
                if base_type == "uint8" {
                    // Special handling for uint8[] (bytes)
                    if let Some(num) = item.as_u64() {
                        encoded_values.push(num as u8);
                    }
                } else {
                    let item_encoded = encode_value(base_type, item, types)?;
                    encoded_values.extend_from_slice(&item_encoded);
                }
            }
            
            // For uint8[], we hash the bytes directly
            if base_type == "uint8" {
                let mut hasher = Keccak256::new();
                hasher.update(&encoded_values);
                let result = hasher.finalize();
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&result);
                return Ok(hash);
            }
            
            // For other arrays, hash the concatenated encoded values
            let mut hasher = Keccak256::new();
            hasher.update(&encoded_values);
            let result = hasher.finalize();
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&result);
            return Ok(hash);
        }
    }
    
    // Handle basic types
    match field_type {
        "string" => {
            if let Some(s) = value.as_str() {
                let mut hasher = Keccak256::new();
                hasher.update(s.as_bytes());
                let result = hasher.finalize();
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&result);
                Ok(hash)
            } else {
                Ok([0u8; 32])
            }
        }
        "uint256" => {
            // For uint256, we need to pad to 32 bytes
            let mut result = [0u8; 32];
            if let Some(s) = value.as_str() {
                // Handle hex strings
                if s.starts_with("0x") {
                    let hex_str = &s[2..];
                    eprintln!("EIP712: Decoding hex for uint256: {} (field: {:?})", hex_str, value);
                    // Pad with leading zero if odd length
                    let padded_hex = if hex_str.len() % 2 != 0 {
                        format!("0{}", hex_str)
                    } else {
                        hex_str.to_string()
                    };
                    let bytes = hex::decode(&padded_hex)?;
                    let start = 32usize.saturating_sub(bytes.len());
                    result[start..].copy_from_slice(&bytes);
                } else if let Ok(num) = s.parse::<u64>() {
                    result[24..].copy_from_slice(&num.to_be_bytes());
                }
            } else if let Some(num) = value.as_u64() {
                result[24..].copy_from_slice(&num.to_be_bytes());
            }
            Ok(result)
        }
        _ => {
            // Check if it's a custom type
            if types.get(field_type).is_some() {
                hash_struct(field_type, value, types)
            } else {
                // Default: treat as string
                if let Some(s) = value.as_str() {
                    let mut hasher = Keccak256::new();
                    hasher.update(s.as_bytes());
                    let result = hasher.finalize();
                    let mut hash = [0u8; 32];
                    hash.copy_from_slice(&result);
                    Ok(hash)
                } else {
                    Ok([0u8; 32])
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_eip712_signing_commit() {
        // Test with known values
        let private_key = hex::decode("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80").unwrap();
        let public_key = hex::decode("02f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9").unwrap();
        
        let signer = Eip712Signer::new(&private_key, &public_key).unwrap();
        
        // Test commit_solution
        let commitment = "lsKzENeCwdyWWUXEN6zbTwMl3Cg3G7wJJhgne/sJ/N8=";
        let msg_data = json!({
            "commitment": commitment
        });
        
        let fee = Fee {
            amount: vec![crate::types::Coin {
                denom: "inj".to_string(),
                amount: "154585000000000".to_string(),
            }],
            gas: "154585".to_string(),
            granter: String::new(),
            payer: String::new(),
        };
        
        let result = signer.sign_transaction(
            "commit",
            &msg_data,
            "inj1npvwllfr9dqr8erajqqr6s0vxnk2ak55re90dz",
            36669,
            35849,
            Some(fee),
            "",
        ).unwrap();
        
        println!("Test passed!");
        println!("Signature: {:?}", result.signature);
        println!("Public key: {:?}", result.pub_key);
        
        // The signature should be a 132-character hex string (65 bytes * 2 + "0x")
        assert!(result.signature.unwrap().len() == 132);
    }
}