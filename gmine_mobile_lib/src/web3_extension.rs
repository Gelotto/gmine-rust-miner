use prost::Message;
use std::error::Error;

/// ExtensionOptionsWeb3Tx is the Injective-specific extension for EIP-712 transactions
/// The field names must match the protobuf definition exactly (camelCase)
#[derive(Clone, PartialEq, Message)]
pub struct ExtensionOptionsWeb3Tx {
    /// typedDataChainID is the Ethereum chain ID for EIP-712 (1439 for testnet)
    #[prost(uint64, tag = "1")]
    pub typedDataChainID: u64,
    
    /// feePayer is the address of the fee payer (empty for non-delegated)
    #[prost(string, tag = "2")]
    pub feePayer: String,
    
    /// feePayerSig is the signature of the fee payer (empty for non-delegated)
    #[prost(bytes = "vec", tag = "3")]
    pub feePayerSig: Vec<u8>,
}

impl ExtensionOptionsWeb3Tx {
    /// Create a new Web3Extension for Injective testnet (non-delegated)
    pub fn new_for_testnet() -> Self {
        ExtensionOptionsWeb3Tx {
            typedDataChainID: 1439, // Injective testnet Ethereum chain ID
            feePayer: String::new(), // Empty for non-delegated transactions
            feePayerSig: vec![], // Empty for non-delegated transactions
        }
    }
    
    /// Create a new Web3Extension for Injective mainnet (non-delegated)
    pub fn new_for_mainnet() -> Self {
        ExtensionOptionsWeb3Tx {
            typedDataChainID: 1, // Injective mainnet Ethereum chain ID  
            feePayer: String::new(), // Empty for non-delegated transactions
            feePayerSig: vec![], // Empty for non-delegated transactions
        }
    }
    
    /// Create a new Web3Extension with fee delegation for testnet
    pub fn new_for_testnet_with_fee_delegation(fee_payer: &str, fee_payer_sig: Vec<u8>) -> Self {
        ExtensionOptionsWeb3Tx {
            typedDataChainID: 1439,
            feePayer: fee_payer.to_string(),
            feePayerSig: fee_payer_sig,
        }
    }
    
    /// Create a new Web3Extension with fee delegation for mainnet
    pub fn new_for_mainnet_with_fee_delegation(fee_payer: &str, fee_payer_sig: Vec<u8>) -> Self {
        ExtensionOptionsWeb3Tx {
            typedDataChainID: 1,
            feePayer: fee_payer.to_string(),
            feePayerSig: fee_payer_sig,
        }
    }
    
    /// Encode the extension to protobuf bytes
    pub fn to_protobuf_bytes(&self) -> Vec<u8> {
        self.encode_to_vec()
    }
    
    /// Create the Any type wrapper for Cosmos SDK
    pub fn to_any(&self) -> Result<serde_json::Value, Box<dyn Error>> {
        use base64::{Engine as _, engine::general_purpose};
        
        let proto_bytes = self.to_protobuf_bytes();
        let base64_value = general_purpose::STANDARD.encode(&proto_bytes);
        
        Ok(serde_json::json!({
            "@type": "/injective.types.v1beta1.ExtensionOptionsWeb3Tx",
            "value": base64_value
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_web3_extension_creation() {
        let ext = ExtensionOptionsWeb3Tx::new_for_testnet();
        assert_eq!(ext.typedDataChainID, 1439);
        assert_eq!(ext.feePayer, "");
        assert!(ext.feePayerSig.is_empty());
    }
    
    #[test]
    fn test_protobuf_encoding() {
        let ext = ExtensionOptionsWeb3Tx::new_for_testnet();
        let bytes = ext.to_protobuf_bytes();
        assert!(!bytes.is_empty());
        
        // Verify it can be decoded back
        let decoded = ExtensionOptionsWeb3Tx::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.typedDataChainID, ext.typedDataChainID);
        assert_eq!(decoded.feePayer, ext.feePayer);
    }
    
    #[test]
    fn test_any_wrapper() {
        let ext = ExtensionOptionsWeb3Tx::new_for_testnet();
        let any = ext.to_any().unwrap();
        
        assert_eq!(any["@type"], "/injective.types.v1beta1.ExtensionOptionsWeb3Tx");
        assert!(any["value"].is_string());
    }
    
    #[test]
    fn test_fee_delegation() {
        let sig = vec![1, 2, 3, 4];
        let ext = ExtensionOptionsWeb3Tx::new_for_testnet_with_fee_delegation("inj1delegator...", sig.clone());
        assert_eq!(ext.typedDataChainID, 1439);
        assert_eq!(ext.feePayer, "inj1delegator...");
        assert_eq!(ext.feePayerSig, sig);
    }
}