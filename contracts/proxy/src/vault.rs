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
    pub assets: u128,
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
    fn enqueue_redemption(
        &mut self,
        owner_id: AccountId,
        receiver_id: AccountId,
        shares: u128,
        assets: u128,
        memo: Option<String>,
    ) {
        let entry = PendingRedemption {
            owner_id: owner_id.clone(),
            receiver_id: receiver_id.clone(),
            shares,
            assets,
            memo: memo.clone(),
        };
        self.pending_redemptions.push(entry);

        env::log_str(&format!(
            "queued_redemption owner={} receiver={} shares={} assets={}",
            owner_id, receiver_id, shares, assets
        ));
    }

    #[allow(dead_code)]
    fn process_redemption_queue(&mut self) {
        env::log_str(&format!(
            "process_redemption_queue: start head={} len={} total_assets={}",
            self.pending_redemptions_head,
            self.pending_redemptions.len(),
            self.total_assets
        ));

        loop {
            if self.pending_redemptions_head >= self.pending_redemptions.len() {
                env::log_str("process_redemption_queue: head >= len, breaking");
                break;
            }

            let index = self.pending_redemptions_head;
            let Some(entry) = self.pending_redemptions.get(index).cloned() else {
                env::log_str(&format!(
                    "process_redemption_queue: no entry at index {}",
                    index
                ));
                break;
            };

            env::log_str(&format!(
                "process_redemption_queue: processing entry {} owner={} shares={}",
                index, entry.owner_id, entry.shares
            ));

            if entry.shares == 0 {
                env::log_str(&format!(
                    "process_redemption_queue: entry {} has 0 shares, skipping",
                    index
                ));
                self.pending_redemptions_head += 1;
                continue;
            }

            let owner_balance = self.token.ft_balance_of(entry.owner_id.clone()).0;
            if owner_balance < entry.shares {
                env::log_str(&format!(
                    "skipping_queued_redemption owner={} reason=insufficient_shares balance={} shares={}",
                    entry.owner_id, owner_balance, entry.shares
                ));
                self.pending_redemptions_head += 1;
                continue;
            }

            // Use the assets value that was stored when the redemption was queued
            // This ensures the lender gets the exact amount they were promised (deposit + intent_yield)
            let assets = entry.assets;

            env::log_str(&format!(
                "process_redemption_queue: entry {} stored_assets={} total_assets={}",
                index, assets, self.total_assets
            ));

            // Check if we have enough assets to process this redemption
            if assets == 0 || assets > self.total_assets {
                env::log_str(&format!(
                    "process_redemption_queue: breaking - stored_assets={} total_assets={}",
                    assets, self.total_assets
                ));
                break;
            }

            self.pending_redemptions_head += 1;

            env::log_str(&format!(
                "process_redemption_queue: processing redemption for owner={} shares={} amount={}",
                entry.owner_id, entry.shares, assets
            ));

            let promise = self.internal_execute_withdrawal(
                entry.owner_id.clone(),
                Some(entry.receiver_id.clone()),
                entry.shares,
                assets,
                entry.memo.clone(),
            );
            let _ = promise;

            env::log_str(&format!(
                "process_redemption_queue: after withdrawal total_assets={}",
                self.total_assets
            ));
        }

        env::log_str(&format!(
            "process_redemption_queue: end head={} len={} total_assets={}",
            self.pending_redemptions_head,
            self.pending_redemptions.len(),
            self.total_assets
        ));
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

        // Calculate used_amount based on shares and total_supply
        let total_supply = self.token.ft_total_supply().0;
        let used_amount = if total_supply == 0 || self.total_assets == 0 {
            // First deposit or all assets are borrowed - accept the full deposit amount
            // When total_assets is 0 (all borrowed), we can't use the normal calculation
            // because it would result in used_amount = 0
            amount.0
        } else {
            // Convert shares to assets using total_assets
            mul_div(shares, self.total_assets, total_supply, Rounding::Up)
        };

        let unused_amount = amount
            .0
            .checked_sub(used_amount)
            .expect("Overflow in unused amount calculation");

        assert!(
            used_amount > 0,
            "No assets to deposit, shares: {}, amount: {}, total_assets: {}",
            shares,
            amount.0,
            self.total_assets
        );

        let owner_id = parsed_msg.receiver_id.unwrap_or(sender_id.clone());
        self.token.internal_deposit(&owner_id, shares);
        // Track available assets
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
        env::log_str(&format!(
            "handle_repayment: sender={} amount={} intent_index={}",
            sender_id, amount.0, repay_msg.intent_index.0
        ));

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

        // Validate minimum repayment: principal + expected yield (1%)
        // This protects lenders from partial repayments that would cause losses
        let expected_yield = intent.borrow_amount / 100; // 1% yield
        let minimum_repayment = intent
            .borrow_amount
            .checked_add(expected_yield)
            .expect("minimum_repayment overflow");

        require!(
            amount.0 >= minimum_repayment,
            format!(
                "Repayment {} is less than minimum required {} (principal {} + yield {})",
                amount.0, minimum_repayment, intent.borrow_amount, expected_yield
            )
        );

        // Add repayment amount to total_assets
        // Note: Tokens from ft_transfer_call are transferred after ft_resolve_transfer completes,
        // but we update total_assets here since we know the amount being transferred
        self.total_assets = self
            .total_assets
            .checked_add(amount.0)
            .expect("total_assets overflow");

        intent.state = State::StpLiquidityReturned;
        intent.repayment_amount = Some(amount.0); // Track repayment amount for intent_yield attribution
        self.index_to_intent.insert(intent_index, intent);

        VaultDeposit {
            sender_id: &sender_id,
            owner_id: &sender_id,
            assets: amount,
            shares: U128(0),
            memo: Some("Repay"),
        }
        .emit();

        env::log_str(&format!(
            "handle_repayment: repayment processed, total_assets={}",
            self.total_assets
        ));

        PromiseOrValue::Value(U128(0))
    }
}

#[near]
impl Contract {
    /// Process the next lender in the redemption queue (FIFO)
    /// This method processes exactly one lender from the queue if liquidity is available
    pub fn process_next_redemption(&mut self) -> bool {
        env::log_str(&format!(
            "process_next_redemption: start head={} len={} total_assets={}",
            self.pending_redemptions_head,
            self.pending_redemptions.len(),
            self.total_assets
        ));

        // Check if queue is empty
        if self.pending_redemptions_head >= self.pending_redemptions.len() {
            env::log_str("process_next_redemption: queue is empty, nothing to process");
            return false;
        }

        let index = self.pending_redemptions_head;
        let Some(entry) = self.pending_redemptions.get(index).cloned() else {
            env::log_str(&format!(
                "process_next_redemption: no entry at index {}",
                index
            ));
            return false;
        };

        env::log_str(&format!(
            "process_next_redemption: processing entry {} owner={} shares={}",
            index, entry.owner_id, entry.shares
        ));

        // Skip entries with 0 shares
        if entry.shares == 0 {
            env::log_str(&format!(
                "process_next_redemption: entry {} has 0 shares, skipping",
                index
            ));
            self.pending_redemptions_head += 1;
            return true; // Processed (skipped)
        }

        // Check if owner still has sufficient shares
        let owner_balance = self.token.ft_balance_of(entry.owner_id.clone()).0;
        if owner_balance < entry.shares {
            env::log_str(&format!(
                "process_next_redemption: skipping owner={} reason=insufficient_shares balance={} shares={}",
                entry.owner_id, owner_balance, entry.shares
            ));
            self.pending_redemptions_head += 1;
            return true; // Processed (skipped)
        }

        // Use the assets value that was stored when the redemption was queued
        // This ensures the lender gets the exact amount they were promised (deposit + intent_yield)
        let assets = entry.assets;

        env::log_str(&format!(
            "process_next_redemption: entry {} stored_assets={} total_assets={}",
            index, assets, self.total_assets
        ));

        // Check if we have enough assets to process this redemption
        if assets == 0 || assets > self.total_assets {
            env::log_str(&format!(
                "process_next_redemption: insufficient liquidity - stored_assets={} total_assets={}",
                assets, self.total_assets
            ));
            return false; // Cannot process, need to wait for more liquidity
        }

        // Advance head before processing (in case of failure, we've already marked it as processed)
        self.pending_redemptions_head += 1;

        env::log_str(&format!(
            "process_next_redemption: processing redemption for owner={} shares={} amount={}",
            entry.owner_id, entry.shares, assets
        ));

        let promise = self.internal_execute_withdrawal(
            entry.owner_id.clone(),
            Some(entry.receiver_id.clone()),
            entry.shares,
            assets,
            entry.memo.clone(),
        );
        let _ = promise;

        env::log_str(&format!(
            "process_next_redemption: after withdrawal total_assets={}",
            self.total_assets
        ));

        true // Successfully processed
    }

    /// Get the length of the pending redemption queue (remaining items)
    pub fn get_pending_redemptions_length(&self) -> U128 {
        let len = self.pending_redemptions.len();
        let head = self.pending_redemptions_head;
        let remaining = if len >= head { len - head } else { 0 };
        U128(remaining as u128)
    }

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

        // Check if the lender is already in the redemption queue
        let len = self.pending_redemptions.len();
        let mut index = self.pending_redemptions_head;
        while index < len {
            if let Some(entry) = self.pending_redemptions.get(index) {
                if entry.owner_id == owner {
                    env::panic_str("Lender already has a redemption in the queue");
                }
            }
            index += 1;
        }

        let receiver = receiver_id.clone().unwrap_or_else(|| owner.clone());

        // Calculate what the shares are worth based on available assets
        // This includes expected yield from current borrows, calculated at redemption time
        // The value is stored and will be used when processing the redemption later
        let assets = self.internal_convert_to_assets(shares.0, Rounding::Down);

        env::log_str(&format!(
            "redeem: owner={} shares={} calculated_assets={} total_assets={}",
            owner, shares.0, assets, self.total_assets
        ));

        // Queue redemption if there are not enough assets
        // This happens when liquidity is borrowed and hasn't been repaid yet
        // The calculated assets value (including expected yield) is stored here
        // When the redemption is processed later, this stored value will be used
        if self.total_assets == 0 || assets == 0 || assets > self.total_assets {
            self.enqueue_redemption(owner, receiver, shares.0, assets, memo);
            return PromiseOrValue::Value(U128(0));
        }

        // Pay back assets based on the lender's share amount and total assets available
        // Yield is already included in total_assets when solvers repay
        PromiseOrValue::Promise(self.internal_execute_withdrawal(
            owner,
            Some(receiver),
            shares.0,
            assets,
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
        // Use available assets for share conversion
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
        env::log_str(&format!(
            "ft_on_transfer: sender={} amount={} msg={} predecessor={} asset={}",
            sender_id,
            amount.0,
            msg,
            env::predecessor_account_id(),
            self.asset
        ));

        assert_eq!(
            env::predecessor_account_id(),
            self.asset.clone(),
            "Only the underlying asset can call ft_on_transfer"
        );

        if let Ok(action) = serde_json::from_str::<FtTransferAction>(&msg) {
            env::log_str(&format!("ft_on_transfer: parsed action successfully"));
            match action {
                FtTransferAction::Deposit(deposit) => {
                    env::log_str("ft_on_transfer: handling deposit");
                    self.handle_deposit(sender_id, amount, deposit)
                }
                FtTransferAction::Repay(repay) => {
                    env::log_str("ft_on_transfer: handling repayment");
                    self.handle_repayment(sender_id, amount, repay)
                }
            }
        } else {
            env::log_str(&format!(
                "ft_on_transfer: failed to parse action, trying default deposit"
            ));
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

#[cfg(test)]
mod tests {

    use super::*;
    use crate::test_utils::helpers::init_contract_ex as init_contract;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    #[test]
    fn convert_to_shares_first_deposit_uses_extra_decimals() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let contract = init_contract(owner, asset, 3);
        // total_supply == 0 -> first deposit path
        let assets = U128(50_000_000); // 50 USDC @ 6 dec
        let shares = <Contract as VaultCore>::convert_to_shares(&contract, assets).0;
        assert_eq!(shares, 50_000_000 * 1_000);
    }

    #[test]
    fn convert_to_assets_empty_vault_uses_inverse_extra_decimals() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let contract = init_contract(owner, asset, 3);
        // total_supply == 0 -> shares / 10^extra
        let shares = U128(1_000); // corresponds to 1 asset unit
        let assets = <Contract as VaultCore>::convert_to_assets(&contract, shares).0;
        assert_eq!(assets, 1);
    }

    #[test]
    fn convert_to_assets_with_supply_and_assets() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        // Mint some shares to create supply
        contract
            .token
            .internal_register_account(&owner.parse().unwrap());
        contract
            .token
            .internal_deposit(&owner.parse().unwrap(), 1_000_000);
        // Set total assets
        contract.total_assets = 500_000;
        let assets = <Contract as VaultCore>::convert_to_assets(&contract, U128(1_000_000)).0;
        assert_eq!(assets, 500_000);
    }

    #[test]
    fn convert_to_shares_deposit_with_existing_supply_and_deposits() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        // existing supply and assets
        contract
            .token
            .internal_register_account(&owner.parse().unwrap());
        contract
            .token
            .internal_deposit(&owner.parse().unwrap(), 1_000_000); // supply
        contract.total_assets = 2_000_000; // Set total_assets for the new calculation logic
        let out = contract.internal_convert_to_shares_deposit(100);
        // With new logic: shares = assets * supply / (total_assets + total_borrowed + expected_yield)
        // shares = 100 * 1_000_000 / (2_000_000 + 0 + 0) = 50
        assert_eq!(out, 50);
    }

    #[test]
    fn redemption_queue_breaks_without_liquidity() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        // User holds shares
        let user: AccountId = "alice.test".parse().unwrap();
        contract.token.internal_register_account(&user);
        contract.token.internal_deposit(&user, 1_000);
        // No assets available
        contract.total_assets = 0;
        // Some deposits to compute deposit_based_assets later

        // enqueue redemption
        contract.enqueue_redemption(user.clone(), user.clone(), 100, 0, None);
        // attempt to process -> should break and not advance head
        contract.process_redemption_queue();
        assert_eq!(contract.pending_redemptions_head, 0);
    }

    #[test]
    fn redemption_queue_processes_with_liquidity() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        // State for deterministic math:
        let user: AccountId = "alice.test".parse().unwrap();
        contract.token.internal_register_account(&user);
        contract.token.internal_deposit(&user, 1_000); // total supply
        contract.total_assets = 200; // available assets
                                     // deposits for deposit_value computation

        // enqueue 100 shares redemption with expected assets value
        // Calculate assets: (100 * 200) / 1000 = 20
        contract.enqueue_redemption(user.clone(), user.clone(), 100, 20, None);
        // process should advance head to 1 (one entry processed)
        contract.process_redemption_queue();
        assert_eq!(contract.pending_redemptions_head, 1);
    }

    #[test]
    fn handle_deposit_with_donate_true_adds_to_total_assets() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        let sender: AccountId = "alice.test".parse().unwrap();
        let before = contract.total_assets;
        let msg = DepositMessage {
            min_shares: None,
            max_shares: None,
            receiver_id: None,
            memo: None,
            donate: Some(true),
        };
        let res = contract.handle_deposit(sender, U128(1_000), msg);
        // donate -> returns Value(U128(0)) and increments total_assets
        match res {
            PromiseOrValue::Value(v) => assert_eq!(v.0, 0),
            _ => panic!("expected Value"),
        }
        assert_eq!(contract.total_assets, before + 1_000);
    }

    #[test]
    fn preview_functions_match_internal_logic() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        // create some supply/deposits
        contract
            .token
            .internal_register_account(&owner.parse().unwrap());
        contract
            .token
            .internal_deposit(&owner.parse().unwrap(), 1_000_000);
        contract.total_assets = 2_000_000;

        let assets = U128(100);
        // preview_deposit uses internal_convert_to_shares_deposit
        let preview_shares = <Contract as VaultCore>::preview_deposit(&contract, assets).0;
        assert_eq!(
            preview_shares,
            contract.internal_convert_to_shares_deposit(100)
        );

        // preview_withdraw uses internal_convert_to_shares with Rounding::Up indirectly
        let preview_withdraw_shares =
            <Contract as VaultCore>::preview_withdraw(&contract, U128(100)).0;
        let expected = contract.internal_convert_to_shares(100, Rounding::Up);
        assert_eq!(preview_withdraw_shares, expected);
    }

    #[test]
    fn ft_on_transfer_routes_deposit_message() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        let user: AccountId = "alice.test".parse().unwrap();
        contract.token.internal_register_account(&user);
        // predecessor must be underlying asset
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(asset.parse().unwrap());
        testing_env!(builder.build());
        let msg = serde_json::json!({ "deposit": { "receiver_id": user } }).to_string();
        let amount = U128(1_000);
        let _ = contract.ft_on_transfer(user.clone(), amount, msg);
        // user should have received shares
        let bal = contract.token.ft_balance_of(user).0;
        assert!(bal > 0);
        assert!(contract.total_assets >= amount.0);
    }

    #[test]
    fn internal_execute_withdrawal_mutates_state_pre_callback() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        let owner_id: AccountId = owner.parse().unwrap();
        // owner has shares
        contract.token.internal_register_account(&owner_id);
        contract.token.internal_deposit(&owner_id, 1_000);
        // vault has assets
        contract.total_assets = 500;
        // call internal_execute_withdrawal (pre-callback mutations must occur)
        let _ = contract.internal_execute_withdrawal(
            owner_id.clone(),
            Some(owner_id.clone()),
            200,
            100,
            None,
        );
        // shares burned
        assert_eq!(contract.token.ft_balance_of(owner_id.clone()).0, 800);
        // assets decreased
        assert_eq!(contract.total_assets, 400);
    }

    #[test]
    fn ft_on_transfer_routes_repay_message_and_updates_intent() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        // Prepare intent owned by solver
        let solver: AccountId = "solver.test".parse().unwrap();
        // Insert mapping solver -> [0]
        contract
            .solver_id_to_indices
            .insert(solver.clone(), vec![0]);
        // Insert intent in borrow state
        contract.index_to_intent.insert(
            0,
            crate::intents::Intent {
                created: 0,
                state: crate::intents::State::StpLiquidityBorrowed,
                intent_data: "x".to_string(),
                user_deposit_hash: "h".to_string(),
                borrow_amount: 100,
                borrow_total_supply: 0,
                repayment_amount: None,
            },
        );
        // predecessor must be the asset contract
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(asset.parse().unwrap());
        testing_env!(builder.build());
        // repay 100
        let msg = serde_json::json!({ "repay": { "intent_index": "0" } }).to_string();
        let result = contract.ft_on_transfer(solver.clone(), U128(101), msg);

        // handle_repayment now returns PromiseOrValue::Value(U128(0)) synchronously
        // Verify that the repayment was processed correctly
        match result {
            PromiseOrValue::Value(v) => assert_eq!(v.0, 0),
            _ => panic!("expected PromiseOrValue::Value(U128(0))"),
        }
        
        // Now: total_assets increased and the intent updated
        assert_eq!(contract.total_assets, 100);
        let intent = contract.index_to_intent.get(&0).unwrap();
        assert!(matches!(
            intent.state,
            crate::intents::State::StpLiquidityReturned
        ));
        assert_eq!(intent.repayment_amount, Some(100));
    }
}
