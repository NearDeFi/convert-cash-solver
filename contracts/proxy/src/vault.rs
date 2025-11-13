use crate::intents::State;
use crate::vault_standards::events::{VaultDeposit, VaultWithdraw};
use crate::vault_standards::mul_div::Rounding;
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

            let assets = self.internal_convert_to_assets(entry.shares, Rounding::Down);

            if assets == 0 || assets > self.total_assets {
                break;
            }

            self.pending_redemptions_head += 1;
            let promise = self.internal_execute_withdrawal(
                entry.owner_id.clone(),
                Some(entry.receiver_id.clone()),
                entry.shares,
                assets,
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

            return PromiseOrValue::Value(U128(0));
        }

        let calculated_shares = self.convert_to_shares(amount).0;

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

        let used_amount = self.internal_convert_to_assets(shares, Rounding::Up);
        let unused_amount = amount
            .0
            .checked_sub(used_amount)
            .expect("Overflow in unused amount calculation");

        assert!(
            used_amount > 0,
            "No assets to deposit, shares: {}, amount: {}",
            shares,
            amount.0
        );

        let owner_id = parsed_msg.receiver_id.unwrap_or(sender_id.clone());
        self.token.internal_deposit(&owner_id, shares);
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

        let assets = self.internal_convert_to_assets(shares.0, Rounding::Down);
        let receiver = receiver_id.clone().unwrap_or_else(|| owner.clone());

        if assets == 0 || assets > self.total_assets {
            self.enqueue_redemption(owner, receiver, shares.0, memo);
            return PromiseOrValue::Value(U128(0));
        }

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
        U128(self.internal_convert_to_shares(assets.0, Rounding::Down))
    }

    fn convert_to_assets(&self, shares: U128) -> U128 {
        U128(self.internal_convert_to_assets(shares.0, Rounding::Down))
    }

    fn preview_deposit(&self, assets: U128) -> U128 {
        U128(self.internal_convert_to_shares(assets.0, Rounding::Down))
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
