//! # Vault Module
//!
//! Implements the core vault functionality following the NEP-621 Fungible Token Vault standard.
//! This module handles:
//!
//! - Deposit processing via `ft_on_transfer`
//! - Redemption queuing and processing
//! - Share-to-asset conversion calculations
//! - Loan repayment from solvers
//!
//! ## Deposit Flow
//!
//! 1. User calls `ft_transfer_call` on the asset token with this contract as receiver
//! 2. Contract receives `ft_on_transfer` callback with deposit message
//! 3. Shares are minted based on current vault ratio
//!
//! ## Redemption Flow
//!
//! 1. User calls `redeem` with shares to burn
//! 2. If liquidity available, assets are transferred immediately
//! 3. If liquidity is borrowed, redemption is queued (FIFO)
//! 4. When solvers repay, `process_next_redemption` fulfills queued requests

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
use near_sdk::{
    assert_one_yocto, env, json_types::U128, near, require, AccountId, NearToken, PromiseOrValue,
};

// ============================================================================
// Constants
// ============================================================================

/// Minimum deposit/redeem amount to prevent spam (1 USDC with 6 decimals).
pub const MIN_DEPOSIT_AMOUNT: u128 = 1_000_000;

// ============================================================================
// Data Structures
// ============================================================================

/// Represents a pending redemption request in the FIFO queue.
///
/// When a lender requests redemption but liquidity is borrowed by solvers,
/// their request is queued until repayment occurs.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct PendingRedemption {
    /// The account that owns the shares being redeemed.
    pub owner_id: AccountId,
    /// The account that will receive the assets.
    pub receiver_id: AccountId,
    /// Number of shares to burn.
    pub shares: u128,
    /// Asset amount calculated at queue time (includes expected yield).
    pub assets: u128,
    /// Optional memo for the transaction.
    pub memo: Option<String>,
}

/// JSON-serializable view of a pending redemption for API responses.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct PendingRedemptionView {
    /// The share owner's account ID.
    pub owner_id: String,
    /// The asset receiver's account ID.
    pub receiver_id: String,
    /// Number of shares pending redemption.
    pub shares: U128,
}

impl From<PendingRedemption> for PendingRedemptionView {
    fn from(value: PendingRedemption) -> Self {
        PendingRedemptionView {
            owner_id: value.owner_id.to_string(),
            receiver_id: value.receiver_id.to_string(),
            shares: U128(value.shares),
        }
    }
}

/// Actions that can be performed when receiving tokens via `ft_transfer_call`.
#[near(serializers = [json, borsh])]
#[serde(rename_all = "snake_case")]
pub enum FtTransferAction {
    /// Deposit assets into the vault to receive shares.
    Deposit(DepositMessage),
    /// Repay borrowed liquidity for a specific intent.
    Repay(LiquidityRepaymentMessage),
}

/// Message payload for deposit operations.
#[near(serializers = [json, borsh])]
pub struct DepositMessage {
    /// Minimum shares to receive; transaction reverts if not met.
    pub min_shares: Option<U128>,
    /// Maximum shares to receive; excess assets are returned.
    pub max_shares: Option<U128>,
    /// Account to receive the minted shares (defaults to sender).
    pub receiver_id: Option<AccountId>,
    /// Optional memo for the deposit event.
    pub memo: Option<String>,
    /// If true, assets are donated to the vault without minting shares.
    pub donate: Option<bool>,
}

/// Message payload for loan repayment operations.
#[near(serializers = [json, borsh])]
pub struct LiquidityRepaymentMessage {
    /// The intent index being repaid.
    pub intent_index: U128,
}

// ============================================================================
// Internal Implementation
// ============================================================================

impl Contract {
    /// Adds a redemption request to the FIFO queue.
    ///
    /// Called when liquidity is insufficient for immediate redemption.
    /// The request will be processed when `process_next_redemption` is called
    /// after solvers repay their borrowed funds.
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

    /// Processes a redemption request, either executing immediately or queuing.
    ///
    /// This internal method handles the common logic for both `redeem` (shares-based)
    /// and `withdraw` (assets-based) operations:
    /// 1. Checks for duplicate queue entries for the same owner
    /// 2. Queues the request if insufficient liquidity
    /// 3. Executes immediately if liquidity is available
    ///
    /// # Arguments
    ///
    /// * `owner` - The account that owns the shares being redeemed
    /// * `receiver_id` - Optional account to receive assets (defaults to owner)
    /// * `shares` - Number of shares to burn
    /// * `assets` - Asset amount to transfer
    /// * `memo` - Optional memo for the transaction
    ///
    /// # Returns
    ///
    /// The amount of assets transferred, or 0 if queued.
    fn process_redemption_request(
        &mut self,
        owner: AccountId,
        receiver_id: Option<AccountId>,
        shares: u128,
        assets: u128,
        memo: Option<String>,
    ) -> PromiseOrValue<U128> {
        // Prevent duplicate queue entries for same owner
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

        env::log_str(&format!(
            "process_redemption_request: owner={} shares={} assets={} total_assets={}",
            owner, shares, assets, self.total_assets
        ));

        // Queue if insufficient liquidity
        if self.total_assets == 0 || assets == 0 || assets > self.total_assets {
            self.enqueue_redemption(owner, receiver, shares, assets, memo);
            return PromiseOrValue::Value(U128(0));
        }

        // Execute immediate withdrawal
        PromiseOrValue::Promise(self.internal_execute_withdrawal(
            owner,
            Some(receiver),
            shares,
            assets,
            memo,
        ))
    }

    /// Processes an incoming deposit via `ft_on_transfer`.
    ///
    /// Calculates shares based on the current vault ratio and mints them
    /// to the receiver. Supports slippage protection via `min_shares`/`max_shares`.
    ///
    /// # Arguments
    ///
    /// * `sender_id` - The account that sent the tokens
    /// * `amount` - The amount of tokens deposited
    /// * `parsed_msg` - The parsed deposit message with options
    ///
    /// # Returns
    ///
    /// The amount of unused tokens to refund (0 if all used).
    fn handle_deposit(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        parsed_msg: DepositMessage,
    ) -> PromiseOrValue<U128> {
        // Require minimum deposit amount to prevent spam
        require!(
            amount.0 >= MIN_DEPOSIT_AMOUNT,
            format!(
                "Deposit amount {} is below minimum {}",
                amount.0, MIN_DEPOSIT_AMOUNT
            )
        );

        // Handle donation mode - assets go to vault without minting shares
        if parsed_msg.donate.unwrap_or(false) {
            self.total_assets = self
                .total_assets
                .checked_add(amount.0)
                .expect("total_assets overflow");
            return PromiseOrValue::Value(U128(0));
        }

        // Calculate shares based on current vault ratio
        let calculated_shares = self.internal_convert_to_shares_deposit(amount.0);

        // Check minimum shares slippage protection
        if let Some(min_shares) = parsed_msg.min_shares {
            if calculated_shares < min_shares.0 {
                return PromiseOrValue::Value(amount);
            }
        }

        // Apply maximum shares cap if specified
        let shares = if let Some(max_shares) = parsed_msg.max_shares {
            if calculated_shares > max_shares.0 {
                max_shares.0
            } else {
                calculated_shares
            }
        } else {
            calculated_shares
        };

        // Calculate actual asset amount used based on final share count
        // Use same effective_total as share calculation (includes borrowed + yield)
        let total_supply = self.token.ft_total_supply().0;
        let (total_borrowed, expected_yield) = self.calculate_expected_yield();
        let effective_total = self.total_assets + total_borrowed + expected_yield;
        
        let used_amount = if total_supply == 0 || effective_total == 0 {
            // First deposit or all assets borrowed - accept full amount
            amount.0
        } else {
            // Convert shares back to assets for precise accounting
            mul_div(shares, effective_total, total_supply, Rounding::Up)
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

        // Mint shares to the receiver
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

    /// Processes a loan repayment from a solver.
    ///
    /// Validates that the repayment meets the minimum required amount
    /// (principal + 1% yield) and updates the intent state.
    ///
    /// # Arguments
    ///
    /// * `sender_id` - The solver account repaying the loan
    /// * `amount` - The repayment amount
    /// * `repay_msg` - The repayment message with intent index
    ///
    /// # Returns
    ///
    /// Always returns 0 (no refund) on success.
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

        // Verify solver owns this intent
        let intent_index: u128 = repay_msg.intent_index.0;
        let solver_indices = self
            .solver_id_to_indices
            .get(&sender_id)
            .unwrap_or_else(|| env::panic_str("Solver has no intents"));
        require!(
            solver_indices.contains(&intent_index),
            "Intent not owned by solver"
        );

        let intent = self
            .index_to_intent
            .get(&intent_index)
            .unwrap_or_else(|| env::panic_str("Intent not found"))
            .clone();

        require!(
            intent.state == State::StpLiquidityBorrowed,
            "Intent is not in borrow state"
        );

        // Validate minimum repayment: principal + solver_fee% yield
        // This protects lenders from partial repayments
        let expected_yield = intent.borrow_amount.0 * self.solver_fee as u128 / 100;
        let minimum_repayment = intent
            .borrow_amount
            .0
            .checked_add(expected_yield)
            .expect("minimum_repayment overflow");

        require!(
            amount.0 >= minimum_repayment,
            format!(
                "Repayment {} is less than minimum required {} (principal {} + yield {})",
                amount.0, minimum_repayment, intent.borrow_amount.0, expected_yield
            )
        );

        // Add repayment to vault assets
        self.total_assets = self
            .total_assets
            .checked_add(amount.0)
            .expect("total_assets overflow");

        // Decrement total borrowed amount
        self.total_borrowed = self
            .total_borrowed
            .checked_sub(intent.borrow_amount.0)
            .expect("total_borrowed underflow");

        // Remove intent from storage (it's complete)
        self.index_to_intent.remove(&intent_index);

        // Remove intent index from solver's list
        if let Some(mut indices) = self.solver_id_to_indices.get(&sender_id).cloned() {
            indices.retain(|&idx| idx != intent_index);
            if indices.is_empty() {
                self.solver_id_to_indices.remove(&sender_id);
            } else {
                self.solver_id_to_indices.insert(sender_id.clone(), indices);
            }
        }

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

// ============================================================================
// Redemption Queue Processing
// ============================================================================

#[near]
impl Contract {
    /// Processes the next pending redemption in the FIFO queue.
    ///
    /// This method should be called after solver repayments to fulfill
    /// queued redemption requests. It processes exactly one redemption
    /// per call if sufficient liquidity is available.
    ///
    /// Processed entries are removed from the queue to prevent unbounded growth.
    ///
    /// # Returns
    ///
    /// * `true` - A redemption was processed (or skipped due to invalid state)
    /// * `false` - Queue is empty or insufficient liquidity
    pub fn process_next_redemption(&mut self) -> bool {
        self.require_not_paused();
        env::log_str(&format!(
            "process_next_redemption: start head={} len={} total_assets={}",
            self.pending_redemptions_head,
            self.pending_redemptions.len(),
            self.total_assets
        ));

        // Check if queue is empty
        if self.pending_redemptions_head >= self.pending_redemptions.len() {
            // Compact the queue when empty to release storage
            self.compact_pending_redemptions();
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

        // Skip zero-share entries
        if entry.shares == 0 {
            env::log_str(&format!(
                "process_next_redemption: entry {} has 0 shares, skipping",
                index
            ));
            self.pending_redemptions_head += 1;
            self.try_compact_pending_redemptions();
            return true;
        }

        // Verify owner still has sufficient shares
        let owner_balance = self.token.ft_balance_of(entry.owner_id.clone()).0;
        if owner_balance < entry.shares {
            env::log_str(&format!(
                "process_next_redemption: skipping owner={} reason=insufficient_shares balance={} shares={}",
                entry.owner_id, owner_balance, entry.shares
            ));
            self.pending_redemptions_head += 1;
            self.try_compact_pending_redemptions();
            return true;
        }

        // Use the pre-calculated asset value from queue time
        let assets = entry.assets;

        env::log_str(&format!(
            "process_next_redemption: entry {} stored_assets={} total_assets={}",
            index, assets, self.total_assets
        ));

        // Check liquidity availability
        if assets == 0 || assets > self.total_assets {
            env::log_str(&format!(
                "process_next_redemption: insufficient liquidity - stored_assets={} total_assets={}",
                assets, self.total_assets
            ));
            return false;
        }

        // Advance queue head before processing
        self.pending_redemptions_head += 1;

        // Compact the queue after processing to release storage
        self.try_compact_pending_redemptions();

        env::log_str(&format!(
            "process_next_redemption: processing redemption for owner={} shares={} amount={}",
            entry.owner_id, entry.shares, assets
        ));

        // Execute the withdrawal
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

        true
    }

    /// Compacts the pending redemptions queue by removing all processed entries.
    ///
    /// This should be called when the queue is empty (all entries processed)
    /// to release storage and reset the head pointer.
    fn compact_pending_redemptions(&mut self) {
        if self.pending_redemptions_head > 0 {
            self.pending_redemptions.clear();
            self.pending_redemptions_head = 0;
            env::log_str("compact_pending_redemptions: queue cleared");
        }
    }

    /// Attempts to compact the queue if all entries have been processed.
    fn try_compact_pending_redemptions(&mut self) {
        if self.pending_redemptions_head >= self.pending_redemptions.len() {
            self.compact_pending_redemptions();
        }
    }

    /// Returns the number of pending redemptions in the queue.
    pub fn get_pending_redemptions_length(&self) -> U128 {
        let len = self.pending_redemptions.len();
        let head = self.pending_redemptions_head;
        let remaining = if len >= head { len - head } else { 0 };
        U128(remaining as u128)
    }

    /// Callback to finalize or rollback a withdrawal after asset transfer.
    ///
    /// Called automatically after the cross-contract `ft_transfer` completes.
    /// On success, emits the `VaultWithdraw` event. On failure, restores
    /// the burned shares and asset balance.
    #[private]
    pub fn resolve_withdraw(
        &mut self,
        owner: AccountId,
        receiver: AccountId,
        shares: U128,
        assets: U128,
        memo: Option<String>,
    ) -> U128 {
        match env::promise_result(0) {
            near_sdk::PromiseResult::Successful(_) => {
                // Transfer succeeded - emit withdrawal event
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
                // Transfer failed - rollback state changes
                self.token.internal_deposit(&owner, shares.0);
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

// ============================================================================
// View Methods
// ============================================================================

#[near]
impl Contract {
    /// Returns pending redemptions in the queue with optional pagination.
    ///
    /// Useful for UI display and monitoring queue status.
    ///
    /// # Arguments
    ///
    /// * `from_index` - Starting index for pagination (default: 0)
    /// * `limit` - Maximum number of redemptions to return (default: all)
    ///
    /// # Returns
    ///
    /// A vector of pending redemptions within the specified range.
    pub fn get_pending_redemptions(
        &self,
        from_index: Option<u32>,
        limit: Option<u32>,
    ) -> Vec<PendingRedemptionView> {
        let len = self.pending_redemptions.len();
        let head = self.pending_redemptions_head;
        let queue_size = if len >= head { len - head } else { 0 };

        let from = from_index.unwrap_or(0);
        let limit = limit.unwrap_or(queue_size);

        let mut result = Vec::new();
        let start_index = head + from;
        let end_index = (start_index + limit).min(len);

        let mut index = start_index;
        while index < end_index {
            if let Some(entry) = self.pending_redemptions.get(index).cloned() {
                result.push(PendingRedemptionView::from(entry));
            }
            index += 1;
        }

        result
    }
}

// ============================================================================
// NEP-621 Vault Core Implementation
// ============================================================================

#[near]
impl VaultCore for Contract {
    /// Returns the underlying asset token account ID.
    fn asset(&self) -> AccountId {
        self.asset.clone()
    }

    /// Returns the total available assets in the vault.
    fn total_assets(&self) -> U128 {
        U128(self.total_assets)
    }

    /// Redeems shares for underlying assets.
    ///
    /// Burns the specified shares and transfers the corresponding assets
    /// to the receiver. If liquidity is insufficient (borrowed by solvers),
    /// the redemption is queued for later processing.
    ///
    /// # Arguments
    ///
    /// * `shares` - Number of shares to redeem
    /// * `receiver_id` - Account to receive assets (defaults to caller)
    /// * `memo` - Optional memo for the transaction
    ///
    /// # Returns
    ///
    /// The amount of assets transferred, or 0 if queued.
    #[payable]
    fn redeem(
        &mut self,
        shares: U128,
        receiver_id: Option<AccountId>,
        memo: Option<String>,
    ) -> PromiseOrValue<U128> {
        self.require_not_paused();
        assert_one_yocto();

        require!(shares.0 > 0, "Shares must be greater than 0");

        let owner = env::predecessor_account_id();

        assert!(
            shares.0 <= self.max_redeem(owner.clone()).0,
            "Exceeds max redeem"
        );

        // Calculate asset value including expected yield from active borrows
        let assets = self.internal_convert_to_assets(shares.0, Rounding::Down);

        // Require minimum redemption amount to prevent spam
        require!(
            assets >= MIN_DEPOSIT_AMOUNT,
            format!(
                "Redemption amount {} is below minimum {}",
                assets, MIN_DEPOSIT_AMOUNT
            )
        );

        self.process_redemption_request(owner, receiver_id, shares.0, assets, memo)
    }

    /// Withdraws a specific amount of assets.
    ///
    /// Calculates and burns the required shares to withdraw the
    /// specified asset amount. If insufficient liquidity, the request
    /// is queued and processed when funds become available.
    ///
    /// # Arguments
    ///
    /// * `assets` - Amount of assets to withdraw
    /// * `receiver_id` - Account to receive assets (defaults to caller)
    /// * `memo` - Optional memo for the transaction
    ///
    /// # Returns
    ///
    /// The amount of assets transferred, or 0 if queued.
    #[payable]
    fn withdraw(
        &mut self,
        assets: U128,
        receiver_id: Option<AccountId>,
        memo: Option<String>,
    ) -> PromiseOrValue<U128> {
        self.require_not_paused();
        assert_one_yocto();

        // Require minimum withdrawal amount to prevent spam
        require!(
            assets.0 >= MIN_DEPOSIT_AMOUNT,
            format!(
                "Withdrawal amount {} is below minimum {}",
                assets.0, MIN_DEPOSIT_AMOUNT
            )
        );

        let owner = env::predecessor_account_id();
        assert!(
            assets.0 <= self.max_withdraw(owner.clone()).0,
            "Exceeds max withdraw"
        );

        // Calculate shares needed (round up to ensure sufficient shares are burned)
        let shares = self.internal_convert_to_shares(assets.0, Rounding::Up);

        self.process_redemption_request(owner, receiver_id, shares, assets.0, memo)
    }

    /// Converts an asset amount to shares for deposit preview.
    fn convert_to_shares(&self, assets: U128) -> U128 {
        U128(self.internal_convert_to_shares_deposit(assets.0))
    }

    /// Converts a share amount to assets.
    fn convert_to_assets(&self, shares: U128) -> U128 {
        U128(self.internal_convert_to_assets(shares.0, Rounding::Down))
    }

    /// Previews the shares that would be minted for a given deposit.
    fn preview_deposit(&self, assets: U128) -> U128 {
        U128(self.internal_convert_to_shares_deposit(assets.0))
    }

    /// Previews the shares required for a given withdrawal amount.
    fn preview_withdraw(&self, assets: U128) -> U128 {
        U128(self.internal_convert_to_shares(assets.0, Rounding::Up))
    }
}

// ============================================================================
// NEP-141 Fungible Token Receiver
// ============================================================================

#[near]
impl FungibleTokenReceiver for Contract {
    /// Handles incoming token transfers via `ft_transfer_call`.
    ///
    /// Routes the transfer to either deposit or repayment handling
    /// based on the message content.
    ///
    /// # Arguments
    ///
    /// * `sender_id` - The account that initiated the transfer
    /// * `amount` - The amount of tokens transferred
    /// * `msg` - JSON message specifying the action (deposit or repay)
    ///
    /// # Returns
    ///
    /// The amount of tokens to refund (unused portion).
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.require_not_paused();
        env::log_str(&format!(
            "ft_on_transfer: sender={} amount={} msg={} predecessor={} asset={}",
            sender_id,
            amount.0,
            msg,
            env::predecessor_account_id(),
            self.asset
        ));

        // Only accept transfers from the underlying asset contract
        assert_eq!(
            env::predecessor_account_id(),
            self.asset.clone(),
            "Only the underlying asset can call ft_on_transfer"
        );

        // Parse and route the action
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
            // Fallback: try parsing as a deposit message directly
            let deposit: DepositMessage = serde_json::from_str(&msg).unwrap_or_else(|_| {
                env::panic_str("Invalid ft_on_transfer message");
            });
            self.handle_deposit(sender_id, amount, deposit)
        }
    }
}

// ============================================================================
// NEP-141 Fungible Token Core (Vault Shares)
// ============================================================================

#[near]
impl FungibleTokenCore for Contract {
    /// Transfers vault shares to another account.
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.require_not_paused();
        self.token.ft_transfer(receiver_id, amount, memo)
    }

    /// Transfers vault shares with a callback to the receiver.
    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.require_not_paused();
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
    }

    /// Returns the total supply of vault shares.
    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }

    /// Returns the share balance of an account.
    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenResolver for Contract {
    /// Resolves the result of `ft_transfer_call` on shares.
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

// ============================================================================
// Storage Management
// ============================================================================

#[near]
impl StorageManagement for Contract {
    /// Registers an account for holding vault shares.
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> near_contract_standards::storage_management::StorageBalance {
        self.require_not_paused();
        self.token.storage_deposit(account_id, registration_only)
    }

    /// Withdraws unused storage deposit.
    #[payable]
    fn storage_withdraw(
        &mut self,
        amount: Option<NearToken>,
    ) -> near_contract_standards::storage_management::StorageBalance {
        self.require_not_paused();
        self.token.storage_withdraw(amount)
    }

    /// Returns the storage balance bounds for this contract.
    fn storage_balance_bounds(
        &self,
    ) -> near_contract_standards::storage_management::StorageBalanceBounds {
        self.token.storage_balance_bounds()
    }

    /// Returns the storage balance for an account.
    fn storage_balance_of(
        &self,
        account_id: AccountId,
    ) -> Option<near_contract_standards::storage_management::StorageBalance> {
        self.token.storage_balance_of(account_id)
    }

    /// Unregisters the caller and refunds storage deposit.
    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        self.require_not_paused();
        self.token.storage_unregister(force)
    }
}

// ============================================================================
// Metadata Provider
// ============================================================================

#[near]
impl FungibleTokenMetadataProvider for Contract {
    /// Returns the vault share token metadata.
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.clone()
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

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
        let assets = U128(50_000_000);
        let shares = <Contract as VaultCore>::convert_to_shares(&contract, assets).0;
        assert_eq!(shares, 50_000_000 * 1_000);
    }

    #[test]
    fn convert_to_assets_empty_vault_uses_inverse_extra_decimals() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let contract = init_contract(owner, asset, 3);
        let shares = U128(1_000);
        let assets = <Contract as VaultCore>::convert_to_assets(&contract, shares).0;
        assert_eq!(assets, 1);
    }

    #[test]
    fn convert_to_assets_with_supply_and_assets() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        contract
            .token
            .internal_register_account(&owner.parse().unwrap());
        contract
            .token
            .internal_deposit(&owner.parse().unwrap(), 1_000_000);
        contract.total_assets = 500_000;
        let assets = <Contract as VaultCore>::convert_to_assets(&contract, U128(1_000_000)).0;
        assert_eq!(assets, 500_000);
    }

    #[test]
    fn convert_to_shares_deposit_with_existing_supply_and_deposits() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        contract
            .token
            .internal_register_account(&owner.parse().unwrap());
        contract
            .token
            .internal_deposit(&owner.parse().unwrap(), 1_000_000);
        contract.total_assets = 2_000_000;
        let out = contract.internal_convert_to_shares_deposit(100);
        assert_eq!(out, 50);
    }

    #[test]
    fn redemption_queue_breaks_without_liquidity() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        let user: AccountId = "alice.test".parse().unwrap();
        contract.token.internal_register_account(&user);
        // Use realistic values above MIN_DEPOSIT_AMOUNT
        contract.token.internal_deposit(&user, 100_000_000); // 100 shares
        contract.total_assets = 0;

        // Enqueue redemption with realistic amounts
        contract.enqueue_redemption(user.clone(), user.clone(), 50_000_000, 0, None);
        let processed = contract.process_next_redemption();
        assert!(!processed, "Should not process when no liquidity");
        assert_eq!(contract.pending_redemptions_head, 0);
    }

    #[test]
    fn redemption_queue_processes_with_liquidity() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        let user: AccountId = "alice.test".parse().unwrap();
        contract.token.internal_register_account(&user);
        // Use realistic values above MIN_DEPOSIT_AMOUNT
        contract.token.internal_deposit(&user, 100_000_000); // 100 shares
        contract.total_assets = 50_000; // Enough liquidity for redemption

        // Enqueue redemption with realistic amounts
        contract.enqueue_redemption(user.clone(), user.clone(), 50_000_000, 20_000, None);
        let processed = contract.process_next_redemption();
        assert!(processed, "Should process when liquidity is available");
        // Queue is compacted after processing when empty
        assert_eq!(contract.pending_redemptions_head, 0);
        assert_eq!(contract.pending_redemptions.len(), 0);
    }

    #[test]
    fn handle_deposit_with_donate_true_adds_to_total_assets() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        let sender: AccountId = "alice.test".parse().unwrap();
        let before = contract.total_assets;
        let deposit_amount = 1_000_000u128; // 1 USDC - at MIN_DEPOSIT_AMOUNT
        let msg = DepositMessage {
            min_shares: None,
            max_shares: None,
            receiver_id: None,
            memo: None,
            donate: Some(true),
        };
        let res = contract.handle_deposit(sender, U128(deposit_amount), msg);
        match res {
            PromiseOrValue::Value(v) => assert_eq!(v.0, 0),
            _ => panic!("expected Value"),
        }
        assert_eq!(contract.total_assets, before + deposit_amount);
    }

    #[test]
    fn preview_functions_match_internal_logic() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        contract
            .token
            .internal_register_account(&owner.parse().unwrap());
        contract
            .token
            .internal_deposit(&owner.parse().unwrap(), 1_000_000);
        contract.total_assets = 2_000_000;

        let assets = U128(100);
        let preview_shares = <Contract as VaultCore>::preview_deposit(&contract, assets).0;
        assert_eq!(
            preview_shares,
            contract.internal_convert_to_shares_deposit(100)
        );

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
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(asset.parse().unwrap());
        testing_env!(builder.build());
        let msg = serde_json::json!({ "deposit": { "receiver_id": user } }).to_string();
        let amount = U128(1_000_000); // 1 USDC - at MIN_DEPOSIT_AMOUNT
        let _ = contract.ft_on_transfer(user.clone(), amount, msg);
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
        contract.token.internal_register_account(&owner_id);
        contract.token.internal_deposit(&owner_id, 1_000);
        contract.total_assets = 500;
        let _ = contract.internal_execute_withdrawal(
            owner_id.clone(),
            Some(owner_id.clone()),
            200,
            100,
            None,
        );
        assert_eq!(contract.token.ft_balance_of(owner_id.clone()).0, 800);
        assert_eq!(contract.total_assets, 400);
    }

    #[test]
    fn ft_on_transfer_routes_repay_message_and_updates_intent() {
        let owner = "owner.test";
        let asset = "usdc.test";
        let mut contract = init_contract(owner, asset, 3);
        let solver: AccountId = "solver.test".parse().unwrap();
        contract
            .solver_id_to_indices
            .insert(solver.clone(), vec![0]);
        contract.index_to_intent.insert(
            0,
            crate::intents::Intent {
                created: near_sdk::json_types::U64(0),
                state: crate::intents::State::StpLiquidityBorrowed,
                intent_data: "x".to_string(),
                user_deposit_hash: "h".to_string(),
                borrow_amount: U128(100),
                repayment_amount: None,
            },
        );
        // Set total_borrowed to match the manually inserted intent
        contract.total_borrowed = 100;
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(asset.parse().unwrap());
        testing_env!(builder.build());
        let msg = serde_json::json!({ "repay": { "intent_index": "0" } }).to_string();
        let result = contract.ft_on_transfer(solver.clone(), U128(101), msg);

        match result {
            PromiseOrValue::Value(v) => assert_eq!(v.0, 0),
            _ => panic!("expected PromiseOrValue::Value(U128(0))"),
        }

        assert_eq!(contract.total_assets, 101);
        assert_eq!(contract.total_borrowed, 0);
        // Intent should be deleted after repayment
        assert!(contract.index_to_intent.get(&0).is_none());
        // Solver's indices should be empty/removed
        assert!(contract.solver_id_to_indices.get(&solver).is_none());
    }
}
