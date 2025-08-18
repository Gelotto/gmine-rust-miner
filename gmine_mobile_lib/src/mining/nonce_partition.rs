/// Nonce partitioning logic for GMINE mobile mining
/// Implements Blake2b512-based deterministic nonce range calculation
/// Each miner gets 1/1000th of nonce space per epoch to prevent conflicts

use blake2::{Blake2b512, Digest};
use anyhow::Result;

/// Calculate deterministic nonce range for a miner in a specific epoch
/// This MUST match the contract's calculate_nonce_range function exactly
pub fn calculate_nonce_range(miner_address: &str, epoch_number: u64) -> Result<(u64, u64)> {
    // Hash miner address + epoch to get deterministic partition
    let mut hasher = Blake2b512::new();
    hasher.update(miner_address.as_bytes());
    hasher.update(&epoch_number.to_be_bytes());
    
    let hash = hasher.finalize();
    let partition_seed = u64::from_be_bytes([
        hash[0], hash[1], hash[2], hash[3], hash[4], hash[5], hash[6], hash[7],
    ]);
    
    // Each miner gets 1/1000th of total nonce space per epoch
    let nonce_space = u64::MAX / 1000;
    let partition_offset = (partition_seed % 1000) * nonce_space;
    
    // Rotate partitions each epoch to prevent grinding attacks
    let epoch_rotation = (epoch_number * 37) % 1000; // Prime number rotation
    let rotated_offset = partition_offset.wrapping_add(epoch_rotation * nonce_space);
    let max_nonce = rotated_offset.wrapping_add(nonce_space);
    
    log::info!("Blake2b512 nonce range for epoch {} miner {}: {} to {}", 
        epoch_number, miner_address, rotated_offset, max_nonce);
    
    Ok((rotated_offset, max_nonce))
}

/// Validate that a nonce falls within the miner's allocated range
pub fn validate_nonce_in_range(
    nonce: u64,
    miner_address: &str,
    epoch_number: u64
) -> Result<bool> {
    let (start_nonce, end_nonce) = calculate_nonce_range(miner_address, epoch_number)?;
    
    let in_range = nonce >= start_nonce && nonce < end_nonce;
    
    if !in_range {
        log::warn!("Nonce {} out of range [{}, {}) for miner {} epoch {}", 
            nonce, start_nonce, end_nonce, miner_address, epoch_number);
    }
    
    Ok(in_range)
}

/// Calculate recommended nonce chunk size for efficient mining
/// Mobile devices have limited CPU, so use smaller chunks
pub fn get_mobile_chunk_size() -> u64 {
    // Mobile-optimized chunk size
    // Desktop uses ~100_000, mobile should use smaller chunks
    10_000
}

/// Calculate next nonce to try based on current progress
pub fn get_next_nonce_chunk(
    current_nonce: u64,
    chunk_size: u64,
    miner_address: &str,
    epoch_number: u64
) -> Result<(u64, u64)> {
    let (start_nonce, end_nonce) = calculate_nonce_range(miner_address, epoch_number)?;
    
    // Calculate next chunk boundaries
    let chunk_start = current_nonce;
    let chunk_end = (current_nonce + chunk_size).min(end_nonce);
    
    // Wrap around if we've exhausted our range
    let (final_start, final_end) = if chunk_start >= end_nonce {
        // Wrap to beginning of our partition
        (start_nonce, (start_nonce + chunk_size).min(end_nonce))
    } else {
        (chunk_start, chunk_end)
    };
    
    log::debug!("Next nonce chunk for epoch {}: {} to {}", 
        epoch_number, final_start, final_end);
    
    Ok((final_start, final_end))
}

/// Get total nonce space allocated to this miner
pub fn get_miner_nonce_space_size() -> u64 {
    u64::MAX / 1000
}

/// Calculate mining progress as percentage
pub fn calculate_mining_progress(
    current_nonce: u64,
    miner_address: &str,
    epoch_number: u64
) -> Result<f64> {
    let (start_nonce, end_nonce) = calculate_nonce_range(miner_address, epoch_number)?;
    
    if current_nonce < start_nonce {
        return Ok(0.0);
    }
    
    if current_nonce >= end_nonce {
        return Ok(100.0);
    }
    
    let progress = (current_nonce - start_nonce) as f64 / (end_nonce - start_nonce) as f64;
    Ok((progress * 100.0).min(100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nonce_range_calculation() {
        let miner = "inj1test123456789";
        let epoch = 123;
        
        let (start, end) = calculate_nonce_range(miner, epoch).unwrap();
        
        // Each miner should get exactly 1/1000th of space
        let expected_size = u64::MAX / 1000;
        assert_eq!(end - start, expected_size);
        
        // Range should be deterministic for same inputs
        let (start2, end2) = calculate_nonce_range(miner, epoch).unwrap();
        assert_eq!(start, start2);
        assert_eq!(end, end2);
    }
    
    #[test]
    fn test_nonce_validation() {
        let miner = "inj1test123456789";
        let epoch = 123;
        let (start, end) = calculate_nonce_range(miner, epoch).unwrap();
        
        // Test valid nonce
        let mid_nonce = start + (end - start) / 2;
        assert!(validate_nonce_in_range(mid_nonce, miner, epoch).unwrap());
        
        // Test invalid nonces
        if start > 0 {
            assert!(!validate_nonce_in_range(start - 1, miner, epoch).unwrap());
        }
        assert!(!validate_nonce_in_range(end, miner, epoch).unwrap());
    }
    
    #[test]
    fn test_different_epochs_different_ranges() {
        let miner = "inj1test123456789";
        
        let (start1, end1) = calculate_nonce_range(miner, 1).unwrap();
        let (start2, end2) = calculate_nonce_range(miner, 2).unwrap();
        
        // Different epochs should produce different ranges (due to rotation)
        assert_ne!(start1, start2);
        
        // But size should be the same
        assert_eq!(end1 - start1, end2 - start2);
    }
    
    #[test]
    fn test_mobile_chunk_size() {
        let chunk_size = get_mobile_chunk_size();
        assert_eq!(chunk_size, 10_000);
        assert!(chunk_size > 0);
        assert!(chunk_size < 100_000); // Smaller than desktop
    }
}