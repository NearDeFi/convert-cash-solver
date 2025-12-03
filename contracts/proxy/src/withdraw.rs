//! # Cross-Chain Withdrawal Module
//!
//! Enables withdrawing OMFT (Omnichain Multichain Fungible Tokens) from NEAR
//! to external chains via the OMFT bridge protocol.
//!
//! ## Supported Chains
//!
//! - **EVM Chains**: Ethereum, Polygon, Arbitrum, etc. (0x addresses)
//! - **Solana**: Base58-encoded addresses
//!
//! ## Bridge Mechanism
//!
//! The OMFT bridge recognizes a special memo format `WITHDRAW_TO:<address>` when
//! the receiver of an `ft_transfer` is the token contract itself. This triggers
//! the bridge to burn the tokens on NEAR and mint them on the destination chain.

use crate::*;
use near_contract_standards::fungible_token::core::ext_ft_core;
use near_sdk::{json_types::U128, Gas};

/// Gas allocation for OMFT withdrawal cross-contract call.
const GAS_FOR_OMFT_WITHDRAW: Gas = Gas::from_tgas(30);

#[near]
impl Contract {
    /// Burns OMFT tokens on NEAR and withdraws them to an EVM address.
    ///
    /// This initiates a cross-chain transfer by calling `ft_transfer` on the
    /// OMFT token contract with a special memo that triggers the bridge.
    ///
    /// # Arguments
    ///
    /// * `token_contract` - The OMFT token contract (must match vault asset)
    /// * `amount` - Amount to withdraw
    /// * `evm_address` - Destination EVM address (0x-prefixed, 40 hex chars)
    ///
    /// # Requirements
    ///
    /// - Caller must be the contract owner
    /// - Requires 1 yoctoNEAR attached for security
    /// - Token contract must match the vault's underlying asset
    /// - Amount must not exceed available vault assets
    /// - EVM address must be valid format (0x + 40 hex characters)
    ///
    /// # Returns
    ///
    /// A promise for the `ft_transfer` cross-contract call.
    ///
    /// # Example
    ///
    /// ```ignore
    /// contract.withdraw_omft_to_evm(
    ///     "usdc.omft.near".parse().unwrap(),
    ///     U128(1_000_000),
    ///     "0x742d35Cc6634C0532925a3b844Bc9e7595f7eA3b".to_string()
    /// );
    /// ```
    #[payable]
    pub fn withdraw_omft_to_evm(
        &mut self,
        token_contract: AccountId,
        amount: U128,
        evm_address: String,
    ) -> Promise {
        // Access control
        self.require_owner();
        near_sdk::assert_one_yocto();

        // Validate inputs
        require!(amount.0 > 0, "amount must be > 0");
        require!(
            token_contract == self.asset,
            "token_contract must match vault asset"
        );
        require!(
            amount.0 <= self.total_assets,
            "amount exceeds available assets"
        );

        // Validate EVM address format (0x + 40 hex characters)
        let evm = evm_address.trim().to_string();
        require!(
            evm.starts_with("0x")
                && evm.len() == 42
                && evm.chars().skip(2).all(|c| c.is_ascii_hexdigit()),
            "invalid EVM address format"
        );

        // Construct the bridge memo
        let memo = format!("WITHDRAW_TO:{}", evm);

        // =====================================================================
        // Cross-Contract Call: OMFT Bridge Withdrawal
        // =====================================================================
        // Calls ft_transfer on the OMFT token contract with:
        // - receiver_id = token contract itself (triggers bridge logic)
        // - memo = "WITHDRAW_TO:<evm_address>" (bridge instruction)
        // The bridge will burn tokens on NEAR and mint on the destination EVM chain.
        // =====================================================================
        ext_ft_core::ext(token_contract.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_OMFT_WITHDRAW)
            .ft_transfer(token_contract, amount, Some(memo))
    }

    /// Burns OMFT tokens on NEAR and withdraws them to a Solana address.
    ///
    /// Similar to EVM withdrawal, but uses Solana's Base58 address format.
    ///
    /// # Arguments
    ///
    /// * `token_contract` - The OMFT token contract (must match vault asset)
    /// * `amount` - Amount to withdraw
    /// * `sol_address` - Destination Solana address (Base58 encoded)
    ///
    /// # Requirements
    ///
    /// - Caller must be the contract owner
    /// - Requires 1 yoctoNEAR attached for security
    /// - Token contract must match the vault's underlying asset
    /// - Amount must not exceed available vault assets
    /// - Solana address must be valid Base58 (32-44 characters, no 0/O/I/l)
    ///
    /// # Returns
    ///
    /// A promise for the `ft_transfer` cross-contract call.
    #[payable]
    pub fn withdraw_omft_to_solana(
        &mut self,
        token_contract: AccountId,
        amount: U128,
        sol_address: String,
    ) -> Promise {
        // Access control
        self.require_owner();
        near_sdk::assert_one_yocto();

        // Validate inputs
        require!(amount.0 > 0, "amount must be > 0");
        require!(
            token_contract == self.asset,
            "token_contract must match vault asset"
        );
        require!(
            amount.0 <= self.total_assets,
            "amount exceeds available assets"
        );

        // Validate Solana address format (Base58, 32-44 chars)
        let sol = sol_address.trim().to_string();
        require!(
            sol.len() >= 32 && sol.len() <= 64,
            "invalid Solana address length"
        );

        // Validate Base58 character set (excludes 0, O, I, l)
        let is_base58 = sol.chars().all(|c| {
            matches!(c,
                '1'..='9'
                | 'A'..='H' | 'J'..='N' | 'P'..='Z'
                | 'a'..='k' | 'm'..='z'
            )
        });
        require!(is_base58, "invalid Solana address characters");

        // Construct the bridge memo
        let memo = format!("WITHDRAW_TO:{}", sol);

        // =====================================================================
        // Cross-Contract Call: OMFT Bridge Withdrawal to Solana
        // =====================================================================
        // Calls ft_transfer on the OMFT token contract with:
        // - receiver_id = token contract itself (triggers bridge logic)
        // - memo = "WITHDRAW_TO:<solana_address>" (bridge instruction)
        // The bridge will burn tokens on NEAR and mint on Solana.
        // =====================================================================
        ext_ft_core::ext(token_contract.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_OMFT_WITHDRAW)
            .ft_transfer(token_contract, amount, Some(memo))
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::builders::ContractBuilder;

    #[test]
    #[should_panic]
    fn evm_requires_one_yocto() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .predecessor("owner.test")
            .attached(0)
            .build();
        let _ = contract.withdraw_omft_to_evm(
            "usdc.test".parse().unwrap(),
            U128(1),
            "0x1111111111111111111111111111111111111111".to_string(),
        );
    }

    #[test]
    #[should_panic]
    fn sol_requires_one_yocto() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .predecessor("owner.test")
            .attached(0)
            .build();
        let _ = contract.withdraw_omft_to_solana(
            "usdc.test".parse().unwrap(),
            U128(1),
            "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        );
    }

    #[test]
    #[should_panic]
    fn evm_amount_must_be_positive() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .predecessor("owner.test")
            .attached(1)
            .build();
        let _ = contract.withdraw_omft_to_evm(
            "usdc.test".parse().unwrap(),
            U128(0),
            "0x1111111111111111111111111111111111111111".to_string(),
        );
    }

    #[test]
    #[should_panic]
    fn sol_amount_must_be_positive() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .predecessor("owner.test")
            .attached(1)
            .build();
        let _ = contract.withdraw_omft_to_solana(
            "usdc.test".parse().unwrap(),
            U128(0),
            "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        );
    }

    #[test]
    #[should_panic]
    fn token_contract_must_match_asset() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .predecessor("owner.test")
            .attached(1)
            .build();
        let _ = contract.withdraw_omft_to_evm(
            "other.test".parse().unwrap(),
            U128(1),
            "0x1111111111111111111111111111111111111111".to_string(),
        );
    }

    #[test]
    #[should_panic]
    fn amount_cannot_exceed_total_assets() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .predecessor("owner.test")
            .attached(1)
            .build();
        let _ = contract.withdraw_omft_to_evm(
            "usdc.test".parse().unwrap(),
            U128(1),
            "0x1111111111111111111111111111111111111111".to_string(),
        );
    }

    #[test]
    #[should_panic]
    fn only_owner_can_withdraw() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .predecessor("alice.test")
            .attached(1)
            .build();
        let _ = contract.withdraw_omft_to_evm(
            "usdc.test".parse().unwrap(),
            U128(1),
            "0x1111111111111111111111111111111111111111".to_string(),
        );
    }

    #[test]
    fn evm_happy_path_does_not_panic() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(2_000_000)
            .predecessor("owner.test")
            .attached(1)
            .build();
        let _ = contract.withdraw_omft_to_evm(
            "usdc.test".parse().unwrap(),
            U128(1_000_000),
            "0x1111111111111111111111111111111111111111".to_string(),
        );
    }

    #[test]
    fn sol_happy_path_does_not_panic() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(2_000_000)
            .predecessor("owner.test")
            .attached(1)
            .build();
        let _ = contract.withdraw_omft_to_solana(
            "usdc.test".parse().unwrap(),
            U128(1_000_000),
            "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        );
    }

    #[test]
    #[should_panic]
    fn evm_address_wrong_length() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(1_000_000)
            .predecessor("owner.test")
            .attached(1)
            .build();
        let _ = contract.withdraw_omft_to_evm(
            "usdc.test".parse().unwrap(),
            U128(1),
            "0x123".to_string(),
        );
    }

    #[test]
    #[should_panic]
    fn sol_address_invalid_chars() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(1_000_000)
            .predecessor("owner.test")
            .attached(1)
            .build();
        let _ = contract.withdraw_omft_to_solana(
            "usdc.test".parse().unwrap(),
            U128(1),
            "1111111111111111111111111111111111111111111111111111111111111100".to_string(),
        );
    }

    #[test]
    fn withdraw_does_not_change_total_assets_before_cc_call() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(2_000_000)
            .predecessor("owner.test")
            .attached(1)
            .build();
        let before = contract.total_assets;
        let _ = contract.withdraw_omft_to_evm(
            "usdc.test".parse().unwrap(),
            U128(1_000_000),
            "0x1111111111111111111111111111111111111111".to_string(),
        );
        assert_eq!(contract.total_assets, before);
    }
}
