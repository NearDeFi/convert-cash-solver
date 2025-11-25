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
    /// Price per share = (total_deposits - expected_premiums) / total_shares
    /// So shares = assets / price_per_share = (assets * total_shares) / (total_deposits - expected_premiums)
    pub fn internal_convert_to_shares_deposit(&self, assets: u128) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        // Handle empty vault case - return 1:1 ratio with extra decimals for first deposit
        if total_supply == 0 {
            return assets * 10u128.pow(self.extra_decimals as u32);
        }

        // Calculate expected premiums from active borrows (1% of borrowed amounts)
        let mut expected_premiums = 0u128;
        for (_index, intent) in self.index_to_intent.iter() {
            let expected_premium = intent.borrow_amount / 100; // 1% premium
            expected_premiums = expected_premiums
                .checked_add(expected_premium)
                .expect("expected_premiums overflow");
        }

        // Subtract unscaled expected_premiums from denominator
        let denominator = self
            .total_deposits
            .checked_sub(expected_premiums)
            .expect("denominator underflow")
            .max(1);

        // Debug: Calculate intermediate values to understand why result might be 0
        let numerator = assets
            .checked_mul(total_supply)
            .expect("numerator overflow");
        let mul_div_result = mul_div(assets, total_supply, denominator, Rounding::Down);

        env::log_str(&format!(
            "convert_to_shares_deposit DEBUG: assets={} total_supply={} total_deposits={} expected_premiums={} denominator={} numerator={} mul_div_result={}",
            assets, total_supply, self.total_deposits, expected_premiums, denominator, numerator, mul_div_result
        ));

        let result = mul_div_result;

        // TODO run multi_lender_redemption_queue test to see why Lender2 is getting 0 shares

        result
    }

    pub fn internal_convert_to_assets(&self, shares: u128, rounding: Rounding) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        // For empty vault, assume 1:1 ratio with extra decimals for consistency
        if total_supply == 0 {
            return shares / 10u128.pow(self.extra_decimals as u32);
        }

        // When the vault holds no assets but still has outstanding shares,
        // treat the available assets as zero to avoid overestimating redemptions.
        if self.total_assets == 0 {
            return 0;
        }

        // Redemption should be based on total_assets (which includes premium after solver repays)
        let supply_adj = total_supply;
        let assets_adj = self.total_assets;
        let result = mul_div(shares, assets_adj, supply_adj, rounding);

        env::log_str(&format!(
            "convert_to_assets: shares={} total_supply={} total_assets={} result={}",
            shares, supply_adj, assets_adj, result
        ));

        result
    }
}
