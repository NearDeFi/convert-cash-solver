use near_contract_standards::fungible_token::{
    core::ext_ft_core, events::FtBurn, FungibleTokenCore,
};
use near_sdk::{env, ext_contract, json_types::U128, AccountId, Gas, NearToken, Promise};

use super::mul_div::{mul_div, Rounding};

pub const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(30);

use crate::Contract;

#[ext_contract(ext_self)]
pub trait _ExtSelf {
    fn resolve_withdraw(
        &mut self,
        owner: AccountId,
        receiver: AccountId,
        shares: U128,
        assets: U128,
        memo: Option<String>,
    );

    fn resolve_repayment(
        &mut self,
        sender_id: AccountId,
        expected_amount: U128,
        intent_index: U128,
        previous_balance: U128,
    );
}

impl Contract {
    pub fn internal_transfer_assets_with_callback(
        &self,
        receiver_id: AccountId,
        amount: u128,
        owner: AccountId,
        shares: u128,
        memo: Option<String>,
    ) -> Promise {
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
        // Burn shares immediately (prevents reuse)
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

        // Interactions - External call
        self.internal_transfer_assets_with_callback(
            receiver_id,
            assets_to_transfer,
            owner,
            shares_to_burn,
            memo,
        )
    }

    pub fn internal_convert_to_shares(&self, assets: u128, rounding: Rounding) -> u128 {
        // For redemption/withdrawal, use available assets
        let total_supply = self.token.ft_total_supply().0;

        // Handle empty vault case
        if total_supply == 0 {
            return 0;
        }

        // When the vault holds no assets but still has outstanding shares,
        // return 0 to avoid overestimating shares.
        if self.total_assets == 0 {
            return 0;
        }

        let supply_adj = total_supply;
        let assets_adj = self.total_assets;

        mul_div(assets, supply_adj, assets_adj, rounding)
    }

    /// Convert assets to shares when depositing
    /// To prevent new depositors from diluting intent_yield reserved for early lenders with queued redemptions,
    /// we use a denominator that includes expected_yield, giving new depositors fewer shares.
    /// Price per share = (total_assets + total_borrowed + expected_yield) / total_shares
    /// So shares = assets / price_per_share = (assets * total_shares) / (total_assets + total_borrowed + expected_yield)
    pub fn internal_convert_to_shares_deposit(&self, assets: u128) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        // Handle empty vault case - return 1:1 ratio with extra decimals for first deposit
        if total_supply == 0 {
            return assets * 10u128.pow(self.extra_decimals as u32);
        }

        // Calculate expected intent_yield from active borrows (1% of borrowed amounts)
        // This intent_yield is reserved for early lenders, so we add it to the denominator
        // to give new depositors fewer shares
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

    pub fn internal_convert_to_assets(&self, shares: u128, rounding: Rounding) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        // For empty vault, assume 1:1 ratio with extra decimals for consistency
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

    pub fn calculate_expected_yield(&self) -> (u128, u128) {
        let mut expected_yield = 0u128;
        let mut total_borrowed = 0u128;
        for (_index, intent) in self.index_to_intent.iter() {
            // Only count active borrows (not yet repaid)
            if intent.state == crate::intents::State::StpLiquidityBorrowed {
                total_borrowed = total_borrowed
                    .checked_add(intent.borrow_amount)
                    .expect("total_borrowed overflow");
                let intent_yield = intent.borrow_amount / 100; // 1% intent_yield
                expected_yield = expected_yield
                    .checked_add(intent_yield)
                    .expect("expected_yield overflow");
            }
        }
        (total_borrowed, expected_yield)
    }
}
