/// Mobile transaction builder for Injective blockchain
/// Implements complete transaction signing compatible with Cosmos SDK
use anyhow::{Result, anyhow};
use prost::Message;
use tiny_keccak::{Hasher, Keccak};
use tonic::transport::Channel;
use cosmos_sdk_proto::{
    cosmos::{
        base::v1beta1::Coin,
        tx::v1beta1::{
            TxBody, AuthInfo, SignerInfo, ModeInfo, Fee, SignDoc, TxRaw, mode_info,
        },
    },
    cosmwasm::wasm::v1::MsgExecuteContract,
    Any,
};
use tracing::{info, debug, warn, error};

use crate::mobile_wallet::{MobileWallet, MobileTransactionSigner};

/// Mobile-optimized transaction builder for Cosmos SDK transactions
pub struct MobileTxBuilder {
    chain_id: String,
    account_number: u64,
    sequence: u64,
    gas_limit: u64,
    gas_price: String,
    signer: MobileTransactionSigner,
}

impl MobileTxBuilder {
    /// Create a new mobile transaction builder
    pub fn new(chain_id: String, account_number: u64, sequence: u64) -> Self {
        Self {
            chain_id,
            account_number,
            sequence,
            gas_limit: 200000,  // Default gas limit for mobile
            gas_price: "500000000inj".to_string(), // Default gas price
            signer: MobileTransactionSigner::new(),
        }
    }
    
    /// Set custom gas parameters for mobile optimization
    pub fn with_gas(mut self, gas_limit: u64, gas_price: String) -> Self {
        self.gas_limit = gas_limit;
        self.gas_price = gas_price;
        self
    }
    
    /// Build and sign a complete transaction for contract execution
    /// This implements the full 13-step transaction building process
    pub fn build_execute_contract_tx(
        &self,
        wallet: &MobileWallet,
        contract_address: &str,
        msg: Vec<u8>,
        funds: Vec<Coin>,
    ) -> Result<Vec<u8>> {
        info!("Building mobile transaction for contract: {}", contract_address);
        
        // Step 1: Create the MsgExecuteContract
        let execute_msg = MsgExecuteContract {
            sender: wallet.address.clone(),
            contract: contract_address.to_string(),
            msg,
            funds,
        };
        
        // Step 2: Encode the message and wrap in Any
        let mut msg_bytes = Vec::new();
        execute_msg.encode(&mut msg_bytes)?;
        
        let any_msg = Any {
            type_url: "/cosmwasm.wasm.v1.MsgExecuteContract".to_string(),
            value: msg_bytes,
        };
        
        debug!("Encoded MsgExecuteContract: {} bytes", any_msg.value.len());
        
        // Step 3: Create TxBody with the message
        let tx_body = TxBody {
            messages: vec![any_msg],
            memo: "".to_string(),
            timeout_height: 0,
            extension_options: vec![],
            non_critical_extension_options: vec![],
        };
        
        // Step 4: Create the Fee
        let fee = Fee {
            amount: self.parse_gas_price()?,
            gas_limit: self.gas_limit,
            payer: "".to_string(),
            granter: "".to_string(),
        };
        
        debug!("Transaction fee: {} gas at {} per unit", self.gas_limit, self.gas_price);
        
        // Step 5: Create public key protobuf for Injective ethsecp256k1
        let compressed_key = wallet.public_key_compressed()?;
        
        // Create the ethsecp256k1 PubKey message for Injective
        // Injective uses a different type URL than standard secp256k1
        let pub_key_bytes = compressed_key.to_vec();
        let pub_key_any = Any {
            type_url: "/injective.crypto.v1beta1.ethsecp256k1.PubKey".to_string(),
            value: pub_key_bytes,
        };
        
        // Step 6: Create SignerInfo
        let signer_info = SignerInfo {
            public_key: Some(pub_key_any),
            mode_info: Some(ModeInfo {
                sum: Some(mode_info::Sum::Single(mode_info::Single {
                    mode: 1, // SIGN_MODE_DIRECT = 1
                })),
            }),
            sequence: self.sequence,
        };
        
        // Step 7: Create AuthInfo
        let auth_info = AuthInfo {
            signer_infos: vec![signer_info],
            fee: Some(fee),
            tip: None,
        };
        
        // Step 8: Encode TxBody and AuthInfo
        let mut body_bytes = Vec::new();
        tx_body.encode(&mut body_bytes)?;
        
        let mut auth_info_bytes = Vec::new();
        auth_info.encode(&mut auth_info_bytes)?;
        
        debug!("Encoded TxBody: {} bytes, AuthInfo: {} bytes", 
               body_bytes.len(), auth_info_bytes.len());
        
        // Step 9: Create SignDoc for signing
        let sign_doc = SignDoc {
            body_bytes: body_bytes.clone(),
            auth_info_bytes: auth_info_bytes.clone(),
            chain_id: self.chain_id.clone(),
            account_number: self.account_number,
        };
        
        // Step 10: Encode and hash SignDoc
        let mut sign_doc_bytes = Vec::new();
        sign_doc.encode(&mut sign_doc_bytes)?;
        
        // Use Keccak256 for Injective ethsecp256k1 compatibility
        let mut hasher = Keccak::v256();
        let mut sign_hash = [0u8; 32];
        hasher.update(&sign_doc_bytes);
        hasher.finalize(&mut sign_hash);
        
        debug!("SignDoc hash: {}", hex::encode(sign_hash));
        
        // Step 11: Sign the hash using mobile wallet
        let private_key = wallet.private_key()?;
        let signature = self.signer.sign_message_hash(&sign_hash, &private_key)?;
        
        debug!("Generated signature: {} bytes", signature.len());
        
        // Step 12: Create TxRaw with signature
        let tx_raw = TxRaw {
            body_bytes,
            auth_info_bytes,
            signatures: vec![signature],
        };
        
        // Step 13: Encode TxRaw for broadcast
        let mut tx_bytes = Vec::new();
        tx_raw.encode(&mut tx_bytes)?;
        
        info!("Built complete mobile transaction: {} bytes", tx_bytes.len());
        Ok(tx_bytes)
    }
    
    /// Create a commit transaction for mining
    pub fn build_commit_tx(
        &self,
        wallet: &MobileWallet,
        contract_address: &str,
        epoch_number: u64,
        commitment_hash: [u8; 32],
    ) -> Result<Vec<u8>> {
        let msg = serde_json::json!({
            "commit": {
                "epoch": epoch_number,
                "commitment": hex::encode(commitment_hash),
            }
        });
        
        let msg_bytes = msg.to_string().into_bytes();
        
        info!("Building commit transaction for epoch {}", epoch_number);
        self.build_execute_contract_tx(wallet, contract_address, msg_bytes, vec![])
    }
    
    /// Create a reveal transaction for mining
    pub fn build_reveal_tx(
        &self,
        wallet: &MobileWallet,
        contract_address: &str,
        epoch_number: u64,
        nonce: u64,
        digest: [u8; 16],
    ) -> Result<Vec<u8>> {
        let msg = serde_json::json!({
            "reveal": {
                "epoch": epoch_number,
                "nonce": nonce.to_string(),
                "digest": hex::encode(digest),
            }
        });
        
        let msg_bytes = msg.to_string().into_bytes();
        
        info!("Building reveal transaction for epoch {} with nonce {}", epoch_number, nonce);
        self.build_execute_contract_tx(wallet, contract_address, msg_bytes, vec![])
    }
    
    /// Create a claim rewards transaction
    pub fn build_claim_rewards_tx(
        &self,
        wallet: &MobileWallet,
        contract_address: &str,
        epoch_number: u64,
    ) -> Result<Vec<u8>> {
        let msg = serde_json::json!({
            "claim_rewards": {
                "epoch": epoch_number,
            }
        });
        
        let msg_bytes = msg.to_string().into_bytes();
        
        info!("Building claim rewards transaction for epoch {}", epoch_number);
        self.build_execute_contract_tx(wallet, contract_address, msg_bytes, vec![])
    }
    
    /// Parse gas price string into Coin array
    fn parse_gas_price(&self) -> Result<Vec<Coin>> {
        // Parse format like "500000000inj"
        let price_str = &self.gas_price;
        
        // Find where the number ends and denom begins
        let split_pos = price_str
            .chars()
            .position(|c| c.is_alphabetic())
            .ok_or_else(|| anyhow!("Invalid gas price format: {}", price_str))?;
        
        let (amount_str, denom) = price_str.split_at(split_pos);
        let amount: u128 = amount_str.parse()
            .map_err(|_| anyhow!("Invalid gas amount: {}", amount_str))?;
        
        // Calculate total fee (gas_limit * gas_price)
        let total_fee = amount * (self.gas_limit as u128);
        
        Ok(vec![Coin {
            denom: denom.to_string(),
            amount: total_fee.to_string(),
        }])
    }
}

/// Mobile gRPC client for broadcasting transactions to Injective
pub struct MobileGrpcClient {
    endpoint: String,
    client: Option<cosmos_sdk_proto::cosmos::tx::v1beta1::service_client::ServiceClient<Channel>>,
}

impl MobileGrpcClient {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            client: None,
        }
    }
    
    /// Connect to the gRPC endpoint
    pub async fn connect(&mut self) -> Result<()> {
        info!("Connecting to Injective gRPC: {}", self.endpoint);
        
        let channel = Channel::from_shared(self.endpoint.clone())?
            .connect()
            .await?;
            
        self.client = Some(
            cosmos_sdk_proto::cosmos::tx::v1beta1::service_client::ServiceClient::new(channel)
        );
        
        info!("Connected to Injective gRPC successfully");
        Ok(())
    }
    
    /// Broadcast a signed transaction
    pub async fn broadcast_transaction(&mut self, tx_bytes: Vec<u8>) -> Result<String> {
        if self.client.is_none() {
            self.connect().await?;
        }
        
        let client = self.client.as_mut()
            .ok_or_else(|| anyhow!("gRPC client not connected"))?;
        
        let request = cosmos_sdk_proto::cosmos::tx::v1beta1::BroadcastTxRequest {
            tx_bytes,
            mode: cosmos_sdk_proto::cosmos::tx::v1beta1::BroadcastMode::Sync as i32,
        };
        
        info!("Broadcasting transaction to Injective network");
        let response = client.broadcast_tx(request).await?;
        let tx_response = response.into_inner().tx_response
            .ok_or_else(|| anyhow!("No tx_response in broadcast response"))?;
        
        if tx_response.code != 0 {
            return Err(anyhow!("Transaction failed: {}", tx_response.raw_log));
        }
        
        let tx_hash = tx_response.txhash;
        info!("Transaction broadcast successfully: {}", tx_hash);
        
        Ok(tx_hash)
    }
    
    /// Get account information for sequence number
    pub async fn get_account_info(&mut self, address: &str) -> Result<(u64, u64)> {
        info!("Querying account info for: {}", address);
        // TODO: Implement account query via gRPC
        // This would query cosmos.auth.v1beta1.Query/Account
        // For now, return defaults
        warn!("Account info query not fully implemented");
        Ok((0, 0)) // (account_number, sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobile_wallet::MobileWallet;
    
    #[test]
    fn test_mobile_transaction_building() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = MobileWallet::from_mnemonic_no_passphrase(mnemonic).unwrap();
        
        let builder = MobileTxBuilder::new(
            "injective-888".to_string(),
            1,
            0,
        );
        
        let commitment_hash = [1u8; 32];
        let tx_bytes = builder.build_commit_tx(
            &wallet,
            "inj1contract",
            1,
            commitment_hash,
        );
        
        assert!(tx_bytes.is_ok());
        let tx = tx_bytes.unwrap();
        assert!(!tx.is_empty());
        
        println!("Mobile transaction size: {} bytes", tx.len());
        
        // Verify we can decode it back
        let decoded = TxRaw::decode(&tx[..]);
        assert!(decoded.is_ok());
    }
    
    #[test]
    fn test_gas_price_parsing() {
        let builder = MobileTxBuilder::new("test".to_string(), 0, 0)
            .with_gas(200000, "500000000inj".to_string());
        
        let coins = builder.parse_gas_price().unwrap();
        assert_eq!(coins.len(), 1);
        assert_eq!(coins[0].denom, "inj");
        
        // Total fee should be gas_limit * gas_price
        let expected_fee = 200000u128 * 500000000u128;
        assert_eq!(coins[0].amount, expected_fee.to_string());
    }
}