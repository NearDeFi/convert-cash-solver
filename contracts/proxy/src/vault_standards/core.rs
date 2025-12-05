//! # Vault Core Trait
//!
//! Defines the `VaultCore` trait following the NEP-621 Fungible Token Vault standard.
//! This trait extends NEP-141 fungible tokens with vault deposit/withdrawal mechanics.
//!
//! ## Share Mechanics
//!
//! Vaults issue shares representing a proportional claim on the underlying assets.
//! The share price = total_assets / total_supply, and increases as yield accrues.
//!
//! ## Conversion Functions
//!
//! - `convert_to_shares(assets)`: How many shares for a given asset amount
//! - `convert_to_assets(shares)`: How many assets for a given share amount
//!
//! ## Preview Functions
//!
//! Preview functions allow users to see the expected outcome of operations
//! before executing them, useful for slippage protection in UIs.

use near_contract_standards::fungible_token::{receiver::FungibleTokenReceiver, FungibleTokenCore};
use near_sdk::{json_types::U128, AccountId, PromiseOrValue};
use uint::construct_uint;

construct_uint! {
    /// 256-bit unsigned integer for overflow-safe arithmetic.
    pub struct U256(4);
}

/// Core vault trait following NEP-621 Fungible Token Vault standard.
///
/// Implementors must provide deposit and withdrawal logic while inheriting
/// default implementations for conversion and preview functions.
#[allow(unused)]
pub trait VaultCore: FungibleTokenCore + FungibleTokenReceiver {
    /// Returns the account ID of the underlying asset token.
    fn asset(&self) -> AccountId;

    /// Returns the total assets currently held by the vault.
    fn total_assets(&self) -> U128;

    /// Redeems shares for underlying assets.
    ///
    /// # Arguments
    ///
    /// * `shares` - Number of shares to redeem
    /// * `receiver_id` - Account to receive the assets (defaults to caller)
    /// * `memo` - Optional memo for the transaction
    fn redeem(
        &mut self,
        shares: U128,
        receiver_id: Option<AccountId>,
        memo: Option<String>,
    ) -> PromiseOrValue<U128>;

    /// Withdraws a specific amount of underlying assets.
    ///
    /// # Arguments
    ///
    /// * `assets` - Amount of assets to withdraw
    /// * `receiver_id` - Account to receive the assets (defaults to caller)
    /// * `memo` - Optional memo for the transaction
    fn withdraw(
        &mut self,
        assets: U128,
        receiver_id: Option<AccountId>,
        memo: Option<String>,
    ) -> PromiseOrValue<U128>;

    /// Converts an asset amount to equivalent shares.
    ///
    /// Uses the current exchange rate: shares = assets * total_supply / total_assets
    fn convert_to_shares(&self, assets: U128) -> U128 {
        if self.total_assets().0 == 0u128 {
            return assets;
        }

        U256::from(self.ft_total_supply().0)
            .checked_mul(U256::from(assets.0))
            .expect("Too much assets")
            .checked_div(U256::from(self.total_assets().0))
            .unwrap()
            .as_u128()
            .into()
    }

    /// Converts a share amount to equivalent assets.
    ///
    /// Uses the current exchange rate: assets = shares * total_assets / total_supply
    ///
    /// # Panics
    ///
    /// Panics if no shares have been issued yet.
    fn convert_to_assets(&self, shares: U128) -> U128 {
        assert!(self.ft_total_supply().0 > 0, "No shares issued yet");

        U256::from(shares.0)
            .checked_mul(U256::from(self.total_assets().0))
            .expect("Too many shares")
            .checked_div(U256::from(self.ft_total_supply().0))
            .unwrap()
            .as_u128()
            .into()
    }

    /// Returns the maximum amount of assets that can be deposited by `receiver_id`.
    ///
    /// Considers overflow constraints on both asset and share totals.
    fn max_deposit(&self, receiver_id: AccountId) -> U128 {
        let max_assets = u128::MAX - self.total_assets().0;
        let max_assets_from_shares = self
            .convert_to_assets(U128(u128::MAX - self.ft_total_supply().0))
            .0;

        if max_assets < max_assets_from_shares {
            max_assets.into()
        } else {
            max_assets_from_shares.into()
        }
    }

    /// Previews the shares that would be minted for a given deposit.
    ///
    /// # Panics
    ///
    /// Panics if the deposit would exceed `max_deposit`.
    fn preview_deposit(&self, assets: U128) -> U128 {
        assert!(assets <= self.max_deposit(near_sdk::env::predecessor_account_id()));
        self.convert_to_shares(assets)
    }

    /// Returns the maximum number of shares that can be minted to `receiver_id`.
    fn max_mint(&self, receiver_id: AccountId) -> U128 {
        let max_shares = u128::MAX - self.ft_total_supply().0;
        let max_shares_from_assets = self
            .convert_to_shares(U128(u128::MAX - self.total_assets().0))
            .0;

        if max_shares < max_shares_from_assets {
            max_shares.into()
        } else {
            max_shares_from_assets.into()
        }
    }

    /// Previews the assets required to mint a given number of shares.
    ///
    /// # Panics
    ///
    /// Panics if the mint would exceed `max_mint`.
    fn preview_mint(&self, shares: U128) -> U128 {
        assert!(shares <= self.max_mint(near_sdk::env::predecessor_account_id()));
        self.convert_to_assets(shares)
    }

    /// Returns the maximum shares that `owner_id` can redeem.
    ///
    /// This is simply their share balance.
    fn max_redeem(&self, owner_id: AccountId) -> U128 {
        self.ft_balance_of(owner_id)
    }

    /// Previews the assets that would be received for redeeming shares.
    ///
    /// # Panics
    ///
    /// Panics if the redeem would exceed `max_redeem`.
    fn preview_redeem(&self, shares: U128) -> U128 {
        assert!(shares <= self.max_redeem(near_sdk::env::predecessor_account_id()));
        self.convert_to_assets(shares)
    }

    /// Returns the maximum assets that `owner_id` can withdraw.
    ///
    /// Converts their share balance to equivalent assets.
    fn max_withdraw(&self, owner_id: AccountId) -> U128 {
        self.convert_to_assets(self.ft_balance_of(owner_id))
    }

    /// Previews the shares required to withdraw a given amount of assets.
    ///
    /// # Panics
    ///
    /// Panics if the withdrawal would exceed `max_withdraw`.
    fn preview_withdraw(&self, assets: U128) -> U128 {
        assert!(assets <= self.max_withdraw(near_sdk::env::predecessor_account_id()));
        self.convert_to_shares(assets)
    }
}
