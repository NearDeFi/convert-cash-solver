//! # Internal Vault Operations
//!
//! Provides internal helper functions for vault share/asset conversions
//! and withdrawal execution. These functions implement the core vault
//! accounting logic used by the public API.
//!
//! ## Key Functions
//!
//! - `internal_convert_to_shares`: Converts assets to shares for redemption
//! - `internal_convert_to_shares_deposit`: Converts assets to shares for deposits
//! - `internal_convert_to_assets`: Converts shares to assets
//! - `internal_execute_withdrawal`: Executes a withdrawal with CEI pattern
//! - `calculate_expected_yield`: Computes expected yield from active borrows

use near_contract_standards::fungible_token::{
    core::ext_ft_core, events::FtBurn, FungibleTokenCore,
};
use near_sdk::{env, ext_contract, json_types::U128, AccountId, Gas, NearToken, Promise};

use super::mul_div::{mul_div, Rounding};

/// Gas allocation for asset transfer during withdrawal.
pub const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(30);

use crate::Contract;

// ============================================================================
// External Contract Interface
// ============================================================================

/// Callback interface for withdrawal resolution.
#[ext_contract(ext_self)]
pub trait _ExtSelf {
    /// Called after asset transfer to finalize or rollback withdrawal.
    fn resolve_withdraw(
        &mut self,
        owner: AccountId,
        receiver: AccountId,
        shares: U128,
        assets: U128,
        memo: Option<String>,
    );

    /// Called after repayment transfer to verify receipt.
    fn resolve_repayment(
        &mut self,
        sender_id: AccountId,
        expected_amount: U128,
        intent_index: U128,
        previous_balance: U128,
    );
}

// ============================================================================
// Contract Implementation
// ============================================================================

impl Contract {
    /// Initiates an asset transfer with a resolution callback.
    ///
    /// This is used internally by `internal_execute_withdrawal` to transfer
    /// assets and handle success/failure via `resolve_withdraw`.
    pub fn internal_transfer_assets_with_callback(
        &self,
        receiver_id: AccountId,
        amount: u128,
        owner: AccountId,
        shares: u128,
        memo: Option<String>,
    ) -> Promise {
        // =====================================================================
        // Cross-Contract Call: Transfer Assets to Receiver
        // =====================================================================
        // Transfers the underlying assets from the vault to the receiver.
        // The `resolve_withdraw` callback handles success (emit event) or
        // failure (rollback share burn and asset deduction).
        // =====================================================================
        ext_ft_core::ext(self.asset.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .ft_transfer(receiver_id.clone(), U128(amount), memo.clone())
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(10))
                    .resolve_withdraw(owner, receiver_id, U128(shares), U128(amount), memo),
            )
    }

    /// Executes a withdrawal following the CEI (Checks-Effects-Interactions) pattern.
    ///
    /// 1. **Checks**: Validates balances and amounts
    /// 2. **Effects**: Burns shares and updates total_assets
    /// 3. **Interactions**: Transfers assets via cross-contract call
    ///
    /// The callback `resolve_withdraw` handles rollback on transfer failure.
    ///
    /// # Arguments
    ///
    /// * `owner` - The share owner initiating the withdrawal
    /// * `receiver_id` - The account to receive assets (defaults to owner)
    /// * `shares_to_burn` - Number of shares to burn
    /// * `assets_to_transfer` - Amount of assets to transfer
    /// * `memo` - Optional transaction memo
    ///
    /// # Returns
    ///
    /// A promise that resolves after the asset transfer completes.
    pub fn internal_execute_withdrawal(
        &mut self,
        owner: AccountId,
        receiver_id: Option<AccountId>,
        shares_to_burn: u128,
        assets_to_transfer: u128,
        memo: Option<String>,
    ) -> Promise {
        let receiver_id = receiver_id.unwrap_or(owner.clone());

        // Checks
        assert!(
            self.token.ft_balance_of(owner.clone()).0 >= shares_to_burn,
            "Insufficient shares"
        );
        assert!(assets_to_transfer > 0, "No assets to withdraw");
        assert!(
            assets_to_transfer <= self.total_assets,
            "Insufficient vault assets"
        );

        // Effects - CEI Pattern: Update state before external call
        self.token.internal_withdraw(&owner, shares_to_burn);
        self.total_assets = self
            .total_assets
            .checked_sub(assets_to_transfer)
            .expect("total_assets underflow");

        FtBurn {
            owner_id: &owner,
            amount: U128(shares_to_burn),
            memo: Some("Withdrawal"),
        }
        .emit();

        // Interactions - External call with callback
        self.internal_transfer_assets_with_callback(
            receiver_id,
            assets_to_transfer,
            owner,
            shares_to_burn,
            memo,
        )
    }

    /// Converts assets to shares for redemption/withdrawal.
    ///
    /// Uses the current vault ratio to calculate shares. Returns 0 if the
    /// vault has no supply or no assets.
    ///
    /// # Arguments
    ///
    /// * `assets` - The asset amount to convert
    /// * `rounding` - Whether to round up or down
    ///
    /// # Returns
    ///
    /// The equivalent share amount.
    pub fn internal_convert_to_shares(&self, assets: u128, rounding: Rounding) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        if total_supply == 0 {
            return 0;
        }

        if self.total_assets == 0 {
            return 0;
        }

        let supply_adj = total_supply;
        let assets_adj = self.total_assets;

        mul_div(assets, supply_adj, assets_adj, rounding)
    }

    /// Converts assets to shares for deposit operations.
    ///
    /// This differs from `internal_convert_to_shares` by including expected
    /// yield from active borrows in the denominator. This prevents new
    /// depositors from diluting yield reserved for existing lenders.
    ///
    /// Formula: shares = (assets * total_supply) / (total_assets + borrowed + yield)
    ///
    /// # Arguments
    ///
    /// * `assets` - The asset amount being deposited
    ///
    /// # Returns
    ///
    /// The number of shares to mint.
    pub fn internal_convert_to_shares_deposit(&self, assets: u128) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        // First deposit: use 1:1 ratio with extra decimals
        if total_supply == 0 {
            return assets * 10u128.pow(self.extra_decimals as u32);
        }

        // Include expected yield in denominator to protect existing lenders
        let (total_borrowed, expected_yield) = self.calculate_expected_yield();

        let denominator = self
            .total_assets
            .checked_add(total_borrowed)
            .expect("denominator overflow")
            .checked_add(expected_yield)
            .expect("denominator overflow")
            .max(1);

        let result = mul_div(assets, total_supply, denominator, Rounding::Down);

        result
    }

    /// Converts shares to equivalent assets.
    ///
    /// Includes expected yield from active borrows in the calculation,
    /// ensuring lenders see their full expected value.
    ///
    /// Formula: assets = (shares * (total_assets + borrowed + yield)) / total_supply
    ///
    /// # Arguments
    ///
    /// * `shares` - The share amount to convert
    /// * `rounding` - Whether to round up or down
    ///
    /// # Returns
    ///
    /// The equivalent asset amount.
    pub fn internal_convert_to_assets(&self, shares: u128, rounding: Rounding) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        if total_supply == 0 {
            return shares / 10u128.pow(self.extra_decimals as u32);
        }

        let (total_borrowed, expected_yield) = self.calculate_expected_yield();
        let total_assets = self.total_assets + total_borrowed + expected_yield;

        env::log_str(&format!(
            "internal_convert_to_assets: shares={} total_supply={} total_assets={} total_borrowed={} expected_yield={} calculated_total={}",
            shares, total_supply, self.total_assets, total_borrowed, expected_yield, total_assets
        ));

        let result = mul_div(shares, total_assets, total_supply, rounding);

        env::log_str(&format!(
            "internal_convert_to_assets: result={} (shares={} * total_assets={} / total_supply={})",
            result, shares, total_assets, total_supply
        ));

        result
    }

    /// Calculates expected yield from all active (unpaid) borrows.
    ///
    /// Uses the tracked `total_borrowed` field for O(1) lookup instead of
    /// iterating through all intents.
    ///
    /// # Returns
    ///
    /// A tuple of (total_borrowed, expected_yield).
    pub fn calculate_expected_yield(&self) -> (u128, u128) {
        let expected_yield = self.total_borrowed * self.solver_fee as u128 / 100;
        (self.total_borrowed, expected_yield)
    }
}
