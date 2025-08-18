#[cfg(test)]
mod tests {
    use crate::eip712::Eip712Signer;
    use crate::types::Fee;
    use serde_json::json;

    #[test]
    fn test_eip712_phase1_fixes() {
        println!("\n=== Testing EIP-712 Phase 1 Fixes ===\n");
        
        // Test private key (same as Node.js test)
        let private_key = hex::decode("d3b0d0f5a6f2a1b3e4c6d9f1a8b5c2e7f4a1b8c5d2e9f6a3b0d7e4f1a8b5c2").unwrap();
        let public_key = hex::decode("02f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9").unwrap();
        
        // Create signer
        let signer = Eip712Signer::new(&private_key, &public_key).unwrap();
        
        // Test commitment message
        let msg_data = json!({
            "commitment": "dGVzdCBjb21taXRtZW50IGRhdGE="  // base64 encoded "test commitment data"
        });
        
        let result = signer.sign_transaction(
            "commit_solution",
            &msg_data,
            "inj1hkhdaj2a2clmq5jq6mspsggqs32vynpk228q3r",  // Test address
            12345,  // account_number
            0,      // sequence
            Some(Fee::default()),
            "Test memo",
        ).unwrap();
        
        assert!(result.success);
        assert!(result.signature.is_some());
        
        println!("✅ Signing succeeded!");
        println!("Signature: {}", result.signature.unwrap());
        println!("Public key: {}", result.pub_key.unwrap());
    }
}