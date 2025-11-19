use crate::intents::State;
use crate::vault_standards::events::{VaultDeposit, VaultWithdraw};
use crate::vault_standards::mul_div::{mul_div, Rounding};
use crate::vault_standards::VaultCore;
use crate::{Contract, ContractExt};
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider,
};
use near_contract_standards::fungible_token::{
    core::FungibleTokenCore, events::FtMint, receiver::FungibleTokenReceiver, FungibleTokenResolver,
};
use near_contract_standards::storage_management::StorageManagement;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    assert_one_yocto, env, json_types::U128, near, require, AccountId, NearToken, PromiseOrValue,
};
use schemars::JsonSchema;

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct PendingRedemption {
    pub owner_id: AccountId,
    pub receiver_id: AccountId,
    pub shares: u128,
    pub memo: Option<String>,
}

#[derive(Serialize, JsonSchema, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct PendingRedemptionView {
    pub owner_id: String,
    pub receiver_id: String,
    pub shares: String,
}

impl From<PendingRedemption> for PendingRedemptionView {
    fn from(value: PendingRedemption) -> Self {
        PendingRedemptionView {
            owner_id: value.owner_id.to_string(),
            receiver_id: value.receiver_id.to_string(),
            shares: value.shares.to_string(),
        }
    }
}

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[serde(rename_all = "snake_case")]
pub enum FtTransferAction {
    Deposit(DepositMessage),
    Repay(LiquidityRepaymentMessage),
}

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct DepositMessage {
    pub min_shares: Option<U128>,
    pub max_shares: Option<U128>,
    pub receiver_id: Option<AccountId>,
    pub memo: Option<String>,
    pub donate: Option<bool>,
}

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct LiquidityRepaymentMessage {
    pub intent_index: U128,
}

impl Contract {
    /// Calculate what a lender should receive based on their shares and deposit state
    /// Returns (deposit_value, premium_value, total_value)
    fn calculate_lender_entitlement(&self, shares: u128) -> (u128, u128, u128) {
        let total_supply = self.token.ft_total_supply().0;
        if total_supply == 0 {
            return (0, 0, 0);
        }

        // Calculate base deposit value
        let deposit_value = if self.total_deposits > 0 {
            mul_div(shares, self.total_deposits, total_supply, Rounding::Down)
        } else {
            0
        };

        // Calculate premium based on Intents where this lender had ownership at borrow time
        let mut premium_value = 0u128;

        // Iterate through all Intents to find ones where this lender had ownership
        for (_index, intent) in self.index_to_intent.iter() {
            if intent.state == crate::intents::State::StpLiquidityReturned {
                // This Intent has been repaid
                if let Some(repayment_amount) = intent.repayment_amount {
                    let principal_borrowed = intent.borrow_amount; // Use the actual borrow amount for this Intent

                    // If lender's shares represent 100% of borrow_total_supply, they get 100% of premium from this Intent
                    if intent.borrow_total_supply > 0 && shares >= intent.borrow_total_supply {
                        // Lender had 100% ownership at borrow time - they get full premium from this Intent
                        let intent_premium = repayment_amount.saturating_sub(principal_borrowed);
                        premium_value = premium_value.saturating_add(intent_premium);
                    } else if intent.borrow_total_supply > 0 {
                        // Lender had partial ownership - they get proportional premium
                        let lender_share_at_borrow =
                            mul_div(shares, 1, intent.borrow_total_supply, Rounding::Down);
                        let intent_premium = repayment_amount.saturating_sub(principal_borrowed);
                        let lender_premium =
                            mul_div(lender_share_at_borrow, intent_premium, 1, Rounding::Down);
                        premium_value = premium_value.saturating_add(lender_premium);
                    }
                }
            }
        }

        let total_value = deposit_value.saturating_add(premium_value);

        (deposit_value, premium_value, total_value)
    }

    fn enqueue_redemption(
        &mut self,
        owner_id: AccountId,
        receiver_id: AccountId,
        shares: u128,
        memo: Option<String>,
    ) {
        let entry = PendingRedemption {
            owner_id: owner_id.clone(),
            receiver_id: receiver_id.clone(),
            shares,
            memo: memo.clone(),
        };
        self.pending_redemptions.push(entry);

        env::log_str(&format!(
            "queued_redemption owner={} receiver={} shares={}",
            owner_id, receiver_id, shares
        ));
    }

    fn process_redemption_queue(&mut self) {
        loop {
            if self.pending_redemptions_head >= self.pending_redemptions.len() {
                break;
            }

            let index = self.pending_redemptions_head;
            let Some(entry) = self.pending_redemptions.get(index).cloned() else {
                break;
            };

            if entry.shares == 0 {
                self.pending_redemptions_head += 1;
                continue;
            }

            let owner_balance = self.token.ft_balance_of(entry.owner_id.clone()).0;
            if owner_balance < entry.shares {
                env::log_str(&format!(
                    "skipping_queued_redemption owner={} reason=insufficient_shares",
                    entry.owner_id
                ));
                self.pending_redemptions_head += 1;
                continue;
            }

            // Calculate what the shares are worth based on available assets
            let assets = self.internal_convert_to_assets(entry.shares, Rounding::Down);

            // Calculate full redemption value (deposit + premiums based on Intent borrow state)
            let (_deposit_value, _premium_value, full_redemption_value) =
                self.calculate_lender_entitlement(entry.shares);

            // Check if we have enough assets for the full redemption value (deposit + premium)
            // This prevents partial redemptions - user must wait until full amount is available
            if assets == 0 || full_redemption_value > self.total_assets {
                break;
            }

            self.pending_redemptions_head += 1;

            // Decrement total_deposits by the deposit value being redeemed
            let total_supply = self.token.ft_total_supply().0;
            let deposit_value_being_redeemed = if total_supply > 0 && self.total_deposits > 0 {
                mul_div(
                    entry.shares,
                    self.total_deposits,
                    total_supply,
                    Rounding::Up,
                )
            } else {
                0
            };
            self.total_deposits = self
                .total_deposits
                .saturating_sub(deposit_value_being_redeemed);

            let promise = self.internal_execute_withdrawal(
                entry.owner_id.clone(),
                Some(entry.receiver_id.clone()),
                entry.shares,
                full_redemption_value,
                entry.memo.clone(),
            );
            let _ = promise;
        }
    }

    fn handle_deposit(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        parsed_msg: DepositMessage,
    ) -> PromiseOrValue<U128> {
        if parsed_msg.donate.unwrap_or(false) {
            self.total_assets = self
                .total_assets
                .checked_add(amount.0)
                .expect("total_assets overflow");
            // Donations don't count as deposits for share calculations
            return PromiseOrValue::Value(U128(0));
        }

        // Calculate shares based on total deposits, not available assets
        let calculated_shares = self.internal_convert_to_shares_deposit(amount.0);

        if let Some(min_shares) = parsed_msg.min_shares {
            if calculated_shares < min_shares.0 {
                return PromiseOrValue::Value(amount);
            }
        }

        let shares = if let Some(max_shares) = parsed_msg.max_shares {
            if calculated_shares > max_shares.0 {
                max_shares.0
            } else {
                calculated_shares
            }
        } else {
            calculated_shares
        };

        // For deposits, used_amount should be based on total_deposits, not total_assets
        // When total_assets == 0 (all borrowed), we still want to accept the deposit
        // So we convert shares back to assets using total_deposits instead
        let used_amount = if self.total_assets == 0 && self.total_deposits > 0 {
            // When all assets are borrowed, calculate used_amount based on total_deposits
            // This ensures deposits still work correctly
            let total_supply = self.token.ft_total_supply().0;
            if total_supply == 0 {
                amount.0
            } else {
                // Convert shares to assets using total_deposits
                mul_div(shares, self.total_deposits, total_supply, Rounding::Up)
            }
        } else {
            // Normal case: use total_assets for conversion
            self.internal_convert_to_assets(shares, Rounding::Up)
        };

        let unused_amount = amount
            .0
            .checked_sub(used_amount)
            .expect("Overflow in unused amount calculation");

        assert!(
            used_amount > 0,
            "No assets to deposit, shares: {}, amount: {}, total_assets: {}, total_deposits: {}",
            shares,
            amount.0,
            self.total_assets,
            self.total_deposits
        );

        let owner_id = parsed_msg.receiver_id.unwrap_or(sender_id.clone());
        self.token.internal_deposit(&owner_id, shares);
        // Track both deposits and available assets
        self.total_deposits = self
            .total_deposits
            .checked_add(used_amount)
            .expect("total_deposits overflow");
        self.total_assets = self
            .total_assets
            .checked_add(used_amount)
            .expect("total_assets overflow");

        FtMint {
            owner_id: &owner_id,
            amount: U128(shares),
            memo: Some("Deposit"),
        }
        .emit();

        VaultDeposit {
            sender_id: &sender_id,
            owner_id: &owner_id,
            assets: U128(used_amount),
            shares: U128(shares),
            memo: parsed_msg.memo.as_deref(),
        }
        .emit();

        PromiseOrValue::Value(U128(unused_amount))
    }

    fn handle_repayment(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        repay_msg: LiquidityRepaymentMessage,
    ) -> PromiseOrValue<U128> {
        require!(amount.0 > 0, "Repayment amount must be positive");

        let intent_index: u128 = repay_msg.intent_index.0;
        let solver_indices = self
            .solver_id_to_indices
            .get(&sender_id)
            .unwrap_or_else(|| env::panic_str("Solver has no intents"));
        require!(
            solver_indices.contains(&intent_index),
            "Intent not owned by solver"
        );

        let mut intent = self
            .index_to_intent
            .get(&intent_index)
            .unwrap_or_else(|| env::panic_str("Intent not found"))
            .clone();

        require!(
            intent.state == State::StpLiquidityBorrowed,
            "Intent is not in borrow state"
        );

        self.total_assets = self
            .total_assets
            .checked_add(amount.0)
            .expect("total_assets overflow");

        intent.state = State::StpLiquidityReturned;
        intent.repayment_amount = Some(amount.0); // Track repayment amount for premium attribution
        self.index_to_intent.insert(intent_index, intent);

        VaultDeposit {
            sender_id: &sender_id,
            owner_id: &sender_id,
            assets: amount,
            shares: U128(0),
            memo: Some("Repay"),
        }
        .emit();

        self.process_redemption_queue();

        PromiseOrValue::Value(U128(0))
    }
}

#[near]
impl Contract {
    #[private]
    pub fn resolve_withdraw(
        &mut self,
        owner: AccountId,
        receiver: AccountId,
        shares: U128,
        assets: U128,
        memo: Option<String>,
    ) -> U128 {
        // Check if the transfer succeeded
        match env::promise_result(0) {
            near_sdk::PromiseResult::Successful(_) => {
                // Transfer succeeded - finalize withdrawal

                // Emit VaultWithdraw event
                VaultWithdraw {
                    owner_id: &owner,
                    receiver_id: &receiver,
                    assets,
                    shares,
                    memo: memo.as_deref(),
                }
                .emit();

                assets
            }
            _ => {
                // Transfer failed - rollback state changes using callback parameters
                // Restore shares that were burned
                self.token.internal_deposit(&owner, shares.0);
                // Restore total_assets that was reduced
                self.total_assets = self
                    .total_assets
                    .checked_add(assets.0)
                    .expect("total_assets overflow");

                FtMint {
                    owner_id: &owner,
                    amount: U128(shares.0),
                    memo: Some("Withdrawal rollback"),
                }
                .emit();

                0.into()
            }
        }
    }
}

#[near]
impl Contract {
    pub fn get_pending_redemptions(&self) -> Vec<PendingRedemptionView> {
        let mut result = Vec::new();
        let len = self.pending_redemptions.len();
        let mut index = self.pending_redemptions_head;

        while index < len {
            if let Some(entry) = self.pending_redemptions.get(index).cloned() {
                result.push(PendingRedemptionView::from(entry));
            }
            index += 1;
        }

        result
    }
}

// ===== Implement FungibleTokenVaultCore Trait =====
#[near]
impl VaultCore for Contract {
    fn asset(&self) -> AccountId {
        self.asset.clone()
    }

    fn total_assets(&self) -> U128 {
        U128(self.total_assets)
    }

    #[payable]
    fn redeem(
        &mut self,
        shares: U128,
        receiver_id: Option<AccountId>,
        memo: Option<String>,
    ) -> PromiseOrValue<U128> {
        assert_one_yocto();

        let owner = env::predecessor_account_id();

        assert!(
            shares.0 <= self.max_redeem(owner.clone()).0,
            "Exceeds max redeem"
        );

        let receiver = receiver_id.clone().unwrap_or_else(|| owner.clone());

        // Calculate what the shares are worth based on available assets
        let assets = self.internal_convert_to_assets(shares.0, Rounding::Down);

        // Calculate full redemption value (deposit + premiums based on Intent borrow state)
        let (_deposit_value, _premium_value, full_redemption_value) =
            self.calculate_lender_entitlement(shares.0);

        // Calculate base deposit value for queuing logic
        let total_supply = self.token.ft_total_supply().0;
        let deposit_based_assets = if total_supply == 0 {
            0
        } else if self.total_deposits > 0 {
            mul_div(shares.0, self.total_deposits, total_supply, Rounding::Down)
        } else {
            assets
        };

        // Queue redemption if:
        // 1. No assets to redeem
        // 2. Available assets are insufficient for the full redemption value (deposit + premium)
        // This prevents partial redemptions - user must wait until full amount is available
        if assets == 0
            || deposit_based_assets > self.total_assets
            || full_redemption_value > self.total_assets
        {
            self.enqueue_redemption(owner, receiver, shares.0, memo);
            return PromiseOrValue::Value(U128(0));
        }

        // Decrement total_deposits by the deposit value being redeemed
        let deposit_value_being_redeemed = if total_supply > 0 && self.total_deposits > 0 {
            mul_div(shares.0, self.total_deposits, total_supply, Rounding::Up)
        } else {
            0
        };
        self.total_deposits = self
            .total_deposits
            .saturating_sub(deposit_value_being_redeemed);

        // Use full_redemption_value which includes deposit + premiums based on Intent borrow state
        PromiseOrValue::Promise(self.internal_execute_withdrawal(
            owner,
            Some(receiver),
            shares.0,
            full_redemption_value,
            memo,
        ))
    }

    #[payable]
    fn withdraw(
        &mut self,
        assets: U128,
        receiver_id: Option<AccountId>,
        memo: Option<String>,
    ) -> PromiseOrValue<U128> {
        assert_one_yocto();

        let owner = env::predecessor_account_id();
        assert!(
            assets.0 <= self.max_withdraw(owner.clone()).0,
            "Exceeds max withdraw"
        );

        let shares = self.internal_convert_to_shares(assets.0, Rounding::Up);

        PromiseOrValue::Promise(self.internal_execute_withdrawal(
            owner,
            receiver_id,
            shares,
            assets.0,
            memo,
        ))
    }

    fn convert_to_shares(&self, assets: U128) -> U128 {
        // For deposits, use total_deposits; for other cases use available assets
        // This is the default implementation for preview_deposit and max_mint
        // which should show what shares would be received based on deposits
        U128(self.internal_convert_to_shares_deposit(assets.0))
    }

    fn convert_to_assets(&self, shares: U128) -> U128 {
        U128(self.internal_convert_to_assets(shares.0, Rounding::Down))
    }

    fn preview_deposit(&self, assets: U128) -> U128 {
        // Preview should show shares based on total deposits, not available assets
        U128(self.internal_convert_to_shares_deposit(assets.0))
    }

    fn preview_withdraw(&self, assets: U128) -> U128 {
        U128(self.internal_convert_to_shares(assets.0, Rounding::Up))
    }
}

#[near]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert_eq!(
            env::predecessor_account_id(),
            self.asset.clone(),
            "Only the underlying asset can call ft_on_transfer"
        );

        if let Ok(action) = serde_json::from_str::<FtTransferAction>(&msg) {
            match action {
                FtTransferAction::Deposit(deposit) => {
                    self.handle_deposit(sender_id, amount, deposit)
                }
                FtTransferAction::Repay(repay) => self.handle_repayment(sender_id, amount, repay),
            }
        } else {
            // can send a default deposit message to match vault standards
            let deposit: DepositMessage = serde_json::from_str(&msg).unwrap_or_else(|_| {
                env::panic_str("Invalid ft_on_transfer message");
            });
            self.handle_deposit(sender_id, amount, deposit)
        }
    }
}

// ===== Implement Fungible Token Traits for Vault Shares =====
#[near]
impl FungibleTokenCore for Contract {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.token.ft_transfer(receiver_id, amount, memo)
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
    }

    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenResolver for Contract {
    #[private]
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        self.token
            .ft_resolve_transfer(sender_id, receiver_id, amount)
    }
}

#[near]
impl StorageManagement for Contract {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> near_contract_standards::storage_management::StorageBalance {
        self.token.storage_deposit(account_id, registration_only)
    }

    #[payable]
    fn storage_withdraw(
        &mut self,
        amount: Option<NearToken>,
    ) -> near_contract_standards::storage_management::StorageBalance {
        self.token.storage_withdraw(amount)
    }

    fn storage_balance_bounds(
        &self,
    ) -> near_contract_standards::storage_management::StorageBalanceBounds {
        self.token.storage_balance_bounds()
    }

    fn storage_balance_of(
        &self,
        account_id: AccountId,
    ) -> Option<near_contract_standards::storage_management::StorageBalance> {
        self.token.storage_balance_of(account_id)
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        self.token.storage_unregister(force)
    }
}

#[near]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.clone()
    }
}
