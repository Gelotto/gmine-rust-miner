use super::{ExecuteMsg, MessageBuilder};

/// Message for revealing a mining solution
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RevealSolutionMsg {
    /// The nonce used in mining
    pub nonce: [u8; 8],
    /// The digest (hash output from drillx)
    pub digest: [u8; 16],
    /// The salt used in commitment
    pub salt: [u8; 32],
}

impl RevealSolutionMsg {
    /// Create a new reveal message
    pub fn new(nonce: [u8; 8], digest: [u8; 16], salt: [u8; 32]) -> Self {
        Self {
            nonce,
            digest,
            salt,
        }
    }
    
    /// Create from a nonce value and digest
    pub fn from_solution(nonce: u64, digest: [u8; 16], salt: [u8; 32]) -> Self {
        Self {
            nonce: nonce.to_le_bytes(),
            digest,
            salt,
        }
    }
    
    /// Generate a random salt for this reveal
    pub fn generate_salt() -> [u8; 32] {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut salt = [0u8; 32];
        rng.fill(&mut salt);
        salt
    }
}

impl MessageBuilder for RevealSolutionMsg {
    fn build_msg(&self) -> ExecuteMsg {
        ExecuteMsg::RevealSolution {
            nonce: self.nonce,
            digest: self.digest,
            salt: self.salt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_reveal_creation() {
        let nonce = [1u8; 8];
        let digest = [2u8; 16];
        let salt = [3u8; 32];
        
        let msg = RevealSolutionMsg::new(nonce, digest, salt);
        assert_eq!(msg.nonce, nonce);
        assert_eq!(msg.digest, digest);
        assert_eq!(msg.salt, salt);
    }
    
    #[test]
    fn test_reveal_from_solution() {
        let nonce_val = 12345u64;
        let digest = [0xAAu8; 16];
        let salt = RevealSolutionMsg::generate_salt();
        
        let msg = RevealSolutionMsg::from_solution(nonce_val, digest, salt);
        assert_eq!(msg.nonce, nonce_val.to_le_bytes());
        assert_eq!(msg.digest, digest);
        
        // Salt should be random (not all zeros or all same value)
        assert_ne!(salt, [0u8; 32]);
        assert_ne!(salt, [salt[0]; 32]);
    }
    
    #[test]
    fn test_message_serialization() {
        let msg = RevealSolutionMsg::new(
            [1u8; 8],
            [2u8; 16],
            [3u8; 32],
        );
        
        let json_bytes = msg.to_json_bytes().unwrap();
        let json_str = String::from_utf8(json_bytes).unwrap();
        
        // Verify JSON structure
        assert!(json_str.contains("reveal_solution"));
        assert!(json_str.contains("nonce"));
        assert!(json_str.contains("digest"));
        assert!(json_str.contains("salt"));
    }
}