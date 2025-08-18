/// Comprehensive test to validate commitment hash matches contract implementation

#[cfg(test)]
mod commitment_validation {
    use blake2::{Blake2b512, Digest};
    use super::super::commit::create_commitment;
    
    #[test]
    fn test_commitment_matches_contract_implementation() {
        // Test vectors from the contract
        let nonce: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        let digest: [u8; 16] = [
            0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22,
            0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00
        ];
        let salt: [u8; 32] = [42u8; 32];
        
        // Create commitment using our implementation
        let our_commitment = create_commitment(nonce, digest, salt);
        
        // Manually create the same commitment to verify
        let mut hasher = Blake2b512::new();
        hasher.update(&nonce);
        hasher.update(&digest);
        hasher.update(&salt);
        let result = hasher.finalize();
        let mut expected_commitment = [0u8; 32];
        expected_commitment.copy_from_slice(&result[0..32]);
        
        // Verify they match
        assert_eq!(our_commitment, expected_commitment);
        
        // Verify it's not all zeros (sanity check)
        assert_ne!(our_commitment, [0u8; 32]);
        
        // Verify different inputs produce different commitments
        let different_nonce = [8, 7, 6, 5, 4, 3, 2, 1];
        let different_commitment = create_commitment(different_nonce, digest, salt);
        assert_ne!(our_commitment, different_commitment);
    }
    
    #[test]
    fn test_commitment_deterministic() {
        let nonce: [u8; 8] = [0xFF; 8];
        let digest: [u8; 16] = [0xAB; 16];
        let salt: [u8; 32] = [0xCD; 32];
        
        // Create commitment multiple times
        let commitment1 = create_commitment(nonce, digest, salt);
        let commitment2 = create_commitment(nonce, digest, salt);
        let commitment3 = create_commitment(nonce, digest, salt);
        
        // All should be identical
        assert_eq!(commitment1, commitment2);
        assert_eq!(commitment2, commitment3);
    }
    
    #[test]
    fn test_commitment_blake2b512_truncation() {
        // Blake2b512 produces 64 bytes, we need exactly 32
        let nonce: [u8; 8] = [1; 8];
        let digest: [u8; 16] = [2; 16];
        let salt: [u8; 32] = [3; 32];
        
        let commitment = create_commitment(nonce, digest, salt);
        
        // Verify it's exactly 32 bytes
        assert_eq!(commitment.len(), 32);
        
        // Verify it matches manual calculation
        let mut hasher = Blake2b512::new();
        hasher.update(&nonce);
        hasher.update(&digest);
        hasher.update(&salt);
        let full_hash = hasher.finalize();
        
        // Our commitment should be first 32 bytes of the 64-byte hash
        assert_eq!(&commitment[..], &full_hash[0..32]);
    }
}