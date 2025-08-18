#[cfg(test)]
mod tests {
    use crate::tx_proto::ProtoTransactionBuilder;
    use crate::mobile_wallet::MobileWallet;
    use crate::types::Fee;
    use serde_json::json;

    #[tokio::test]
    async fn test_tx_proto_submission() {
        println!("\n=== Testing TX Proto Implementation ===\n");
        
        // Use test wallet
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = MobileWallet::from_mnemonic(mnemonic, "").unwrap();
        let address = wallet.address.clone();
        println!("Wallet address: {}", address);
        
        // Create proto transaction builder
        let builder = ProtoTransactionBuilder::new(
            &wallet.private_key().unwrap().secret_bytes(),
            &wallet.public_key().unwrap().serialize(),
            "testnet"
        ).unwrap();
        
        // Test commitment message
        let msg = json!({
            "commit_solution": {
                "commitment": "dGVzdCBjb21taXRtZW50IGRhdGE="
            }
        });
        
        // Build transaction
        match builder.build_transaction(
            &address,
            "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y",
            msg,
            0,  // account_number
            0,  // sequence
            Some(Fee::default()),
            "Test TX Proto",
        ) {
            Ok(tx_bytes) => {
                println!("✅ Transaction built successfully!");
                println!("Transaction size: {} bytes", tx_bytes.len());
                println!("Transaction hex: {}", hex::encode(&tx_bytes));
                
                // Try to submit the transaction
                let agent = ureq::Agent::new();
                let url = "https://testnet.sentry.lcd.injective.network:443/cosmos/tx/v1beta1/txs";
                
                // Encode transaction for broadcast
                let tx_request = json!({
                    "tx_bytes": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &tx_bytes),
                    "mode": "BROADCAST_MODE_SYNC"
                });
                
                match agent.post(url).send_json(&tx_request) {
                    Ok(response) => {
                        let result: serde_json::Value = response.into_json().unwrap();
                        println!("\nSubmission response: {}", serde_json::to_string_pretty(&result).unwrap());
                        
                        if let Some(tx_response) = result.get("tx_response") {
                            if let Some(code) = tx_response.get("code").and_then(|c| c.as_u64()) {
                                if code == 0 {
                                    println!("✅ Transaction succeeded!");
                                    if let Some(txhash) = tx_response.get("txhash").and_then(|h| h.as_str()) {
                                        println!("Transaction hash: {}", txhash);
                                    }
                                } else {
                                    println!("❌ Transaction failed with code: {}", code);
                                    if let Some(raw_log) = tx_response.get("raw_log").and_then(|l| l.as_str()) {
                                        println!("Error: {}", raw_log);
                                    }
                                }
                            }
                        }
                    }
                    Err(ureq::Error::Status(code, response)) => {
                        let error_body = response.into_string().unwrap_or_else(|_| "Unknown error".to_string());
                        println!("❌ HTTP {} error: {}", code, error_body);
                    }
                    Err(e) => {
                        println!("❌ Request failed: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("❌ Transaction submission failed: {}", e);
                let error_str = e.to_string();
                if error_str.contains("signature verification failed") {
                    println!("Signature verification issue (code 4)");
                } else if error_str.contains("invalid empty tx") {
                    println!("Invalid empty tx (code 3)");
                } else if error_str.contains("account sequence mismatch") {
                    println!("Sequence mismatch");
                } else {
                    println!("Different error type");
                }
            }
        }
    }
}