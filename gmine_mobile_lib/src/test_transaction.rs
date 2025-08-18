#[cfg(test)]
mod tests {
    use crate::transaction::Eip712TransactionBuilder;
    use crate::types::Fee;
    use serde_json::json;

    #[tokio::test]
    async fn test_eip712_transaction_submission() {
        println!("\n=== Testing EIP-712 Transaction Submission ===\n");
        
        // Test private key (same as before)
        let private_key = hex::decode("d3b0d0f5a6f2a1b3e4c6d9f1a8b5c2e7f4a1b8c5d2e9f6a3b0d7e4f1a8b5c2").unwrap();
        let public_key = hex::decode("02f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9").unwrap();
        
        // Create transaction builder
        let builder = Eip712TransactionBuilder::new(&private_key, &public_key, "testnet").unwrap();
        
        // Test commitment message
        let msg = json!({
            "commit_solution": {
                "commitment": "dGVzdCBjb21taXRtZW50IGRhdGE="  // base64 encoded
            }
        });
        
        // Build transaction
        let tx = builder.build_transaction(
            "inj1hkhdaj2a2clmq5jq6mspsggqs32vynpk228q3r",  // Test address
            "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y",  // Mining contract
            msg,
            12345,  // account_number
            0,      // sequence
            Some(Fee::default()),
            "Test memo",
        ).unwrap();
        
        println!("Transaction built successfully!");
        println!("Transaction JSON:\n{}", serde_json::to_string_pretty(&tx).unwrap());
        
        // Try to submit transaction
        match builder.submit_transaction(&tx) {
            Ok(txhash) => {
                println!("✅ Transaction submitted successfully!");
                println!("Transaction hash: {}", txhash);
            }
            Err(e) => {
                println!("❌ Transaction submission failed: {}", e);
                // Check if it's a signature verification error
                if e.to_string().contains("signature verification failed") {
                    println!("Still have signature verification issues");
                } else if e.to_string().contains("account sequence mismatch") {
                    println!("Account sequence issue (not signature related)");
                } else {
                    println!("Different error: {}", e);
                }
            }
        }
    }
}