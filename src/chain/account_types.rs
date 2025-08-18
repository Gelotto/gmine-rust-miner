/// Polymorphic Account Type System for Cosmos SDK and Injective Chain
/// 
/// This module provides a comprehensive, type-safe way to handle all possible
/// account types that can be returned from auth module queries. It solves the
/// UTF-8 decoding issue by properly handling the google.protobuf.Any wrapper
/// that contains different account implementations.

use anyhow::{Result, anyhow};
use prost::Message;

// Import all the proto types we need
use crate::chain::proto::{
    cosmos::auth::v1beta1::BaseAccount,
    cosmos::vesting::v1beta1::{
        BaseVestingAccount, ContinuousVestingAccount, DelayedVestingAccount, 
        PeriodicVestingAccount, PermanentLockedAccount
    },
    injective::types::v1beta1::EthAccount,
};

/// Comprehensive enum representing all possible account types in Cosmos SDK and Injective
#[derive(Debug, Clone)]
pub enum Account {
    // Standard Cosmos SDK account types
    Base(BaseAccount),
    
    // Vesting account types
    BaseVesting(BaseVestingAccount),
    ContinuousVesting(ContinuousVestingAccount),
    DelayedVesting(DelayedVestingAccount),
    PeriodicVesting(PeriodicVestingAccount),
    PermanentLocked(PermanentLockedAccount),
    
    // Injective-specific account types
    Eth(EthAccount),
    
    // Forward compatibility for unknown account types
    Unsupported { 
        type_url: String,
        raw_value: Vec<u8>,
    },
}

/// Common account information extracted from any account type
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub address: String,
    pub sequence: u64,
    pub account_number: u64,
}

impl Default for AccountInfo {
    fn default() -> Self {
        Self {
            address: String::new(),
            sequence: 0,
            account_number: 0,
        }
    }
}

impl Account {
    /// Decode a google.protobuf.Any message into the appropriate Account variant
    /// 
    /// This is the core function that solves the UTF-8 decoding issue by properly
    /// handling the polymorphic Any wrapper based on type_url.
    pub fn decode_any(type_url: &str, value: &[u8]) -> Result<Self> {
        let account = match type_url {
            // Standard Cosmos SDK types
            "/cosmos.auth.v1beta1.BaseAccount" => {
                let base_account = BaseAccount::decode(value)
                    .map_err(|e| anyhow!("Failed to decode BaseAccount: {}", e))?;
                Account::Base(base_account)
            },
            
            // Vesting account types
            "/cosmos.vesting.v1beta1.BaseVestingAccount" => {
                let vesting_account = BaseVestingAccount::decode(value)
                    .map_err(|e| anyhow!("Failed to decode BaseVestingAccount: {}", e))?;
                Account::BaseVesting(vesting_account)
            },
            "/cosmos.vesting.v1beta1.ContinuousVestingAccount" => {
                let continuous_account = ContinuousVestingAccount::decode(value)
                    .map_err(|e| anyhow!("Failed to decode ContinuousVestingAccount: {}", e))?;
                Account::ContinuousVesting(continuous_account)
            },
            "/cosmos.vesting.v1beta1.DelayedVestingAccount" => {
                let delayed_account = DelayedVestingAccount::decode(value)
                    .map_err(|e| anyhow!("Failed to decode DelayedVestingAccount: {}", e))?;
                Account::DelayedVesting(delayed_account)
            },
            "/cosmos.vesting.v1beta1.PeriodicVestingAccount" => {
                let periodic_account = PeriodicVestingAccount::decode(value)
                    .map_err(|e| anyhow!("Failed to decode PeriodicVestingAccount: {}", e))?;
                Account::PeriodicVesting(periodic_account)
            },
            "/cosmos.vesting.v1beta1.PermanentLockedAccount" => {
                let permanent_account = PermanentLockedAccount::decode(value)
                    .map_err(|e| anyhow!("Failed to decode PermanentLockedAccount: {}", e))?;
                Account::PermanentLocked(permanent_account)
            },
            
            // Injective-specific types
            "/injective.types.v1beta1.EthAccount" => {
                let eth_account = EthAccount::decode(value)
                    .map_err(|e| anyhow!("Failed to decode EthAccount: {}", e))?;
                Account::Eth(eth_account)
            },
            
            // Unknown/unsupported type - store for future compatibility
            unsupported_type => {
                log::warn!("Encountered unsupported account type: {}", unsupported_type);
                Account::Unsupported {
                    type_url: unsupported_type.to_string(),
                    raw_value: value.to_vec(),
                }
            }
        };
        
        Ok(account)
    }
    
    /// Extract common account information in a panic-safe way
    /// 
    /// Returns None if the account type doesn't contain the required base information.
    /// This prevents panics and allows the application to decide how to handle
    /// incomplete account data.
    pub fn get_account_info(&self) -> Option<AccountInfo> {
        match self {
            // Direct BaseAccount
            Account::Base(acc) => Some(AccountInfo {
                address: acc.address.clone(),
                sequence: acc.sequence,
                account_number: acc.account_number,
            }),
            
            // EthAccount embeds BaseAccount
            Account::Eth(acc) => {
                if let Some(base) = acc.base_account.as_ref() {
                    log::info!("EthAccount extracted - address: {}, sequence: {}, account_number: {}", 
                        base.address, base.sequence, base.account_number);
                    Some(AccountInfo {
                        address: base.address.clone(),
                        sequence: base.sequence,
                        account_number: base.account_number,
                    })
                } else {
                    log::warn!("EthAccount has no base_account!");
                    None
                }
            },
            
            // BaseVesting has BaseAccount directly
            Account::BaseVesting(acc) => acc.base_account.as_ref().map(|base| AccountInfo {
                address: base.address.clone(),
                sequence: base.sequence,
                account_number: base.account_number,
            }),
            
            // Other vesting accounts have BaseVestingAccount -> BaseAccount
            Account::ContinuousVesting(acc) => acc.base_vesting_account.as_ref()
                .and_then(|bva| bva.base_account.as_ref())
                .map(|base| AccountInfo {
                    address: base.address.clone(),
                    sequence: base.sequence,
                    account_number: base.account_number,
                }),
            
            Account::DelayedVesting(acc) => acc.base_vesting_account.as_ref()
                .and_then(|bva| bva.base_account.as_ref())
                .map(|base| AccountInfo {
                    address: base.address.clone(),
                    sequence: base.sequence,
                    account_number: base.account_number,
                }),
            
            Account::PeriodicVesting(acc) => acc.base_vesting_account.as_ref()
                .and_then(|bva| bva.base_account.as_ref())
                .map(|base| AccountInfo {
                    address: base.address.clone(),
                    sequence: base.sequence,
                    account_number: base.account_number,
                }),
            
            Account::PermanentLocked(acc) => acc.base_vesting_account.as_ref()
                .and_then(|bva| bva.base_account.as_ref())
                .map(|base| AccountInfo {
                    address: base.address.clone(),
                    sequence: base.sequence,
                    account_number: base.account_number,
                }),
            
            // Unsupported accounts have no extractable info
            Account::Unsupported { .. } => None,
        }
    }
    
    /// Get the account type as a string for logging and debugging
    pub fn account_type(&self) -> &'static str {
        match self {
            Account::Base(_) => "BaseAccount",
            Account::BaseVesting(_) => "BaseVestingAccount",
            Account::ContinuousVesting(_) => "ContinuousVestingAccount",
            Account::DelayedVesting(_) => "DelayedVestingAccount",
            Account::PeriodicVesting(_) => "PeriodicVestingAccount",
            Account::PermanentLocked(_) => "PermanentLockedAccount",
            Account::Eth(_) => "EthAccount",
            Account::Unsupported { .. } => "UnsupportedAccount",
        }
    }
    
    /// Check if this account type is supported for transaction operations
    pub fn is_supported(&self) -> bool {
        !matches!(self, Account::Unsupported { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_account_info_extraction() {
        // Test BaseAccount
        let base_account = BaseAccount {
            address: "inj1test123".to_string(),
            sequence: 5,
            account_number: 12345,
            pub_key: None,
        };
        let account = Account::Base(base_account);
        let info = account.get_account_info().unwrap();
        assert_eq!(info.address, "inj1test123");
        assert_eq!(info.sequence, 5);
        assert_eq!(info.account_number, 12345);
    }
    
    #[test]
    fn test_unsupported_account() {
        let unsupported = Account::Unsupported {
            type_url: "/unknown.type".to_string(),
            raw_value: vec![1, 2, 3],
        };
        assert!(unsupported.get_account_info().is_none());
        assert!(!unsupported.is_supported());
        assert_eq!(unsupported.account_type(), "UnsupportedAccount");
    }
}