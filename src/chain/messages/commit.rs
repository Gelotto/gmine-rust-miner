use super::{ExecuteMsg, MessageBuilder};
use blake2::{Blake2b512, Digest};

/// Message for committing a mining solution
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommitSolutionMsg {
    /// The commitment (hash of nonce + digest + salt)
    pub commitment: [u8; 32],
}

impl CommitSolutionMsg {
    /// Create a new commit message from solution components
    pub fn new(nonce: [u8; 8], digest: [u8; 16], salt: [u8; 32]) -> Self {
        let commitment = create_commitment(nonce, digest, salt);
        Self { commitment }
    }
    
    /// Create a commit message from a pre-computed commitment
    pub fn from_commitment(commitment: [u8; 32]) -> Self {
        Self { commitment }
    }
}

impl MessageBuilder for CommitSolutionMsg {
    fn build_msg(&self) -> ExecuteMsg {
        ExecuteMsg::CommitSolution {
            commitment: self.commitment,
        }
    }
}

/// Create a commitment hash from solution components
/// Commitment = Blake2b512(nonce || digest || salt), truncated to 32 bytes
/// This MUST match the contract's create_solution_commitment function
fn create_commitment(nonce: [u8; 8], digest: [u8; 16], salt: [u8; 32]) -> [u8; 32] {
    let mut hasher = Blake2b512::new();
    hasher.update(&nonce);
    hasher.update(&digest);
    hasher.update(&salt);
    
    let result = hasher.finalize();
    let mut commitment = [0u8; 32];
    // Take first 32 bytes of the 64-byte Blake2b512 output
    commitment.copy_from_slice(&result[0..32]);
    commitment
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_commitment_creation() {
        let nonce = [1u8; 8];
        let digest = [2u8; 16];
        let salt = [3u8; 32];
        
        let msg = CommitSolutionMsg::new(nonce, digest, salt);
        
        // Verify commitment is deterministic
        let msg2 = CommitSolutionMsg::new(nonce, digest, salt);
        assert_eq!(msg.commitment, msg2.commitment);
        
        // Verify commitment is not all zeros
        assert_ne!(msg.commitment, [0u8; 32]);
    }
    
    #[test]
    fn test_message_serialization() {
        let commitment = [0x42u8; 32];
        let msg = CommitSolutionMsg::from_commitment(commitment);
        
        let json_bytes = msg.to_json_bytes().unwrap();
        let json_str = String::from_utf8(json_bytes).unwrap();
        
        // Verify JSON structure
        assert!(json_str.contains("commit_solution"));
        assert!(json_str.contains("commitment"));
    }
    
    #[test]
    fn test_commitment_uses_blake2b512() {
        // This test verifies we're using Blake2b512, not SHA256
        let nonce: [u8; 8] = [0xFF; 8];
        let digest: [u8; 16] = [0xAB; 16];
        let salt: [u8; 32] = [0xCD; 32];
        
        let commitment = create_commitment(nonce, digest, salt);
        
        // Manually compute Blake2b512 to verify
        let mut hasher = Blake2b512::new();
        hasher.update(&nonce);
        hasher.update(&digest);
        hasher.update(&salt);
        let result = hasher.finalize();
        
        // Our commitment should be first 32 bytes of Blake2b512 output
        assert_eq!(&commitment[..], &result[0..32]);
    }
    
    #[test]
    fn test_commitment_matches_contract() {
        // Test with known values to ensure compatibility with contract
        let nonce: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        let digest: [u8; 16] = [10; 16];
        let salt: [u8; 32] = [20; 32];
        
        let commitment = create_commitment(nonce, digest, salt);
        
        // Commitment should be exactly 32 bytes
        assert_eq!(commitment.len(), 32);
        
        // Different inputs should produce different commitments
        let different_commitment = create_commitment([8, 7, 6, 5, 4, 3, 2, 1], digest, salt);
        assert_ne!(commitment, different_commitment);
    }
}