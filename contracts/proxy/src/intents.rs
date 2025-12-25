//! # Intent Management Module
//!
//! Manages the lifecycle of solver intents for cross-chain swap execution.
//! An intent represents a solver's commitment to fulfill a user's swap request.
//!
//! ## Intent Lifecycle
//!
//! 1. **Created**: Solver calls `new_intent` to borrow liquidity
//! 2. **Borrowed**: Liquidity transferred to solver, intent recorded
//! 3. **Processing**: Solver executes the cross-chain swap
//! 4. **Repaid**: Solver returns liquidity with yield
//!
//! ## Yield Model
//!
//! Solvers must repay principal + 1% yield when returning borrowed funds.
//! This yield is distributed to lenders proportionally to their shares.

use crate::*;
use near_contract_standards::fungible_token::core::ext_ft_core;
use near_sdk::{
    env, ext_contract,
    json_types::{U128, U64},
    Gas, NearToken, Promise, PromiseResult,
};

/// Gas allocation for the solver borrow `ft_transfer`.
const GAS_FOR_SOLVER_BORROW: Gas = Gas::from_tgas(30);

/// Gas allocation for the `on_new_intent_callback`.
const GAS_FOR_NEW_INTENT_CALLBACK: Gas = Gas::from_tgas(8);

/// External contract interface for callback methods.
#[allow(dead_code)]
#[ext_contract(ext_self)]
trait ExtContract {
    fn on_new_intent_callback(
        &mut self,
        intent_data: String,
        solver_id: AccountId,
        user_deposit_hash: String,
        amount: U128,
    ) -> bool;
}

// ============================================================================
// Data Structures
// ============================================================================

/// Represents the current state of an intent in its lifecycle.
#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq)]
pub enum State {
    /// Liquidity has been borrowed from the vault by the solver.
    StpLiquidityBorrowed,
    /// Liquidity has been deposited on the destination chain.
    StpLiquidityDeposited,
    /// Liquidity has been withdrawn on the destination chain.
    StpLiquidityWithdrawn,
    /// User's intent account has been credited.
    StpIntentAccountCredited,
    /// The swap has been completed successfully.
    SwapCompleted,
    /// User liquidity has been borrowed (reverse flow).
    UserLiquidityBorrowed,
    /// User liquidity has been deposited (reverse flow).
    UserLiquidityDeposited,
    /// Borrowed liquidity has been returned with yield.
    StpLiquidityReturned,
}

/// Represents a solver's intent to fulfill a cross-chain swap.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Intent {
    /// Unix timestamp when the intent was created.
    pub created: U64,
    /// Current state in the intent lifecycle.
    pub state: State,
    /// Serialized intent data (quote details, destination, etc.).
    pub intent_data: String,
    /// Hash of the user's deposit transaction for verification.
    pub user_deposit_hash: String,
    /// Amount of liquidity borrowed from the vault (principal).
    pub borrow_amount: U128,
    /// Repayment amount when liquidity is returned (principal + yield).
    pub repayment_amount: Option<U128>,
}

/// Intent with its index for view methods.
#[near(serializers = [json])]
#[derive(Clone)]
pub struct IndexedIntent {
    /// The intent index in the contract.
    pub index: U128,
    /// The intent data.
    pub intent: Intent,
}

// ============================================================================
// Contract Implementation
// ============================================================================

#[near]
impl Contract {
    /// Creates a new intent and borrows liquidity from the vault.
    ///
    /// This is the entry point for solvers to start fulfilling a swap.
    /// The solver receives borrowed liquidity which they must repay with yield.
    ///
    /// # Arguments
    ///
    /// * `intent_data` - Serialized intent/quote details
    /// * `_solver_deposit_address` - Reserved for future use
    /// * `user_deposit_hash` - Hash of user's deposit for verification
    /// * `amount` - Amount of liquidity to borrow from the vault
    ///
    /// # Panics
    ///
    /// - If an intent with the same `user_deposit_hash` already exists
    /// - If there are pending redemptions in the queue
    /// - If the vault has insufficient assets
    pub fn new_intent(
        &mut self,
        intent_data: String,
        _solver_deposit_address: AccountId,
        user_deposit_hash: String,
        amount: U128,
    ) {
        self.require_not_paused();
        // Prevent duplicate intents for the same user deposit
        if self
            .index_to_intent
            .values()
            .any(|intent| intent.user_deposit_hash == user_deposit_hash)
        {
            env::panic_str("Intent with this hash already exists");
        }

        let solver_id = env::predecessor_account_id();
        let borrow_amount = amount.0;

        // Block borrowing while lenders are waiting for redemptions
        require!(
            self.pending_redemptions_head >= self.pending_redemptions.len(),
            "Cannot borrow while redemptions are pending"
        );

        // Verify sufficient liquidity
        require!(
            self.total_assets >= borrow_amount,
            "Insufficient assets for solver borrow"
        );

        // Deduct from available assets (optimistic update)
        self.total_assets = self
            .total_assets
            .checked_sub(borrow_amount)
            .expect("total_assets underflow");

        // =====================================================================
        // Cross-Contract Call: Transfer Borrowed Liquidity to Solver
        // =====================================================================
        // Transfers the borrowed amount from the vault to the solver.
        // The callback `on_new_intent_callback` records the intent on success
        // or rolls back the total_assets deduction on failure.
        // =====================================================================
        let promise: Promise = ext_ft_core::ext(self.asset.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_SOLVER_BORROW)
            .ft_transfer(
                solver_id.clone(),
                U128(borrow_amount),
                Some("Solver borrow".to_string()),
            )
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_NEW_INTENT_CALLBACK)
                    .on_new_intent_callback(
                        intent_data,
                        solver_id,
                        user_deposit_hash,
                        U128(borrow_amount),
                    ),
            );

        let _ = promise.as_return();
    }

    /// Callback after attempting to transfer borrowed liquidity.
    ///
    /// Records the intent on success or rolls back state on failure.
    #[private]
    pub fn on_new_intent_callback(
        &mut self,
        intent_data: String,
        solver_id: AccountId,
        user_deposit_hash: String,
        amount: U128,
    ) -> bool {
        match env::promise_result(0) {
            PromiseResult::Successful(_) => {
                self.insert_intent(solver_id, intent_data, user_deposit_hash, amount);
                true
            }
            _ => {
                // Rollback: restore the deducted assets
                self.total_assets = self
                    .total_assets
                    .checked_add(amount.0)
                    .expect("total_assets overflow on borrow revert");
                false
            }
        }
    }

    /// Records a new intent after successful liquidity transfer.
    fn insert_intent(
        &mut self,
        solver_id: AccountId,
        intent_data: String,
        user_deposit_hash: String,
        borrow_amount: U128,
    ) {
        let index = self.intent_nonce;
        self.intent_nonce += 1;

        // Track intent indices per solver
        let mut indices = vec![index];
        if let Some(existing_indices) = self.solver_id_to_indices.get(&solver_id) {
            indices.extend(existing_indices);
        }
        self.solver_id_to_indices.insert(solver_id.clone(), indices);

        // Track total borrowed amount
        self.total_borrowed = self
            .total_borrowed
            .checked_add(borrow_amount.0)
            .expect("total_borrowed overflow");

        self.index_to_intent.insert(
            index,
            Intent {
                created: U64(env::block_timestamp()),
                state: State::StpLiquidityBorrowed,
                intent_data,
                user_deposit_hash,
                borrow_amount,
                repayment_amount: None,
            },
        );
    }

    /// Clears all intents (owner-only, for debugging).
    pub fn clear_intents(&mut self) {
        self.require_not_paused();
        self.require_owner();
        self.solver_id_to_indices.clear();
        self.index_to_intent.clear();
        self.total_borrowed = 0;
    }

    /// Returns intents in the contract with their indices, with optional pagination.
    ///
    /// # Arguments
    ///
    /// * `from_index` - Starting index for pagination (default: 0)
    /// * `limit` - Maximum number of intents to return (default: all)
    ///
    /// # Returns
    ///
    /// A vector of indexed intents within the specified range.
    pub fn get_intents(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<IndexedIntent> {
        let from = from_index.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(self.index_to_intent.len() as u32) as usize;

        self.index_to_intent
            .iter()
            .skip(from)
            .take(limit)
            .map(|(index, intent)| IndexedIntent {
                index: U128(*index),
                intent: intent.clone(),
            })
            .collect()
    }

    /// Updates the state of an intent.
    ///
    /// Only the solver who owns the intent can update its state.
    ///
    /// # Arguments
    ///
    /// * `index` - The intent index to update
    /// * `state` - The new state to set
    ///
    /// # Panics
    ///
    /// - If the caller doesn't own the intent
    /// - If the intent doesn't exist
    pub fn update_intent_state(&mut self, index: u128, state: State) {
        self.require_not_paused();
        let solver_id = env::predecessor_account_id();
        let indices = self.get_intent_indices(solver_id);

        require!(indices.contains(&index), "Intent not owned by solver");
        let intent = self.index_to_intent.get(&index).expect("Intent not found");

        self.index_to_intent.insert(
            index,
            Intent {
                state,
                ..intent.clone()
            },
        );
    }

    /// Returns intents owned by a specific solver with optional pagination.
    ///
    /// # Arguments
    ///
    /// * `solver_id` - The solver's account ID
    /// * `from_index` - Starting index for pagination (default: 0)
    /// * `limit` - Maximum number of intents to return (default: all)
    ///
    /// # Returns
    ///
    /// A vector of intents owned by the solver within the specified range.
    pub fn get_intents_by_solver(
        &self,
        solver_id: AccountId,
        from_index: Option<u32>,
        limit: Option<u32>,
    ) -> Vec<IndexedIntent> {
        let indices = self.get_intent_indices(solver_id);
        let from = from_index.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(indices.len() as u32) as usize;

        indices
            .iter()
            .skip(from)
            .take(limit)
            .filter_map(|i| {
                self.index_to_intent.get(i).map(|intent| IndexedIntent {
                    index: U128(*i),
                    intent: intent.clone(),
                })
            })
            .collect()
    }

    /// Returns the intent indices for a solver.
    fn get_intent_indices(&self, solver_id: AccountId) -> Vec<u128> {
        self.solver_id_to_indices
            .get(&solver_id)
            .expect("No intents for solver")
            .to_vec()
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::builders::ContractBuilder;
    use crate::test_utils::helpers::init_ctx as init_account;

    #[test]
    #[should_panic(expected = "Insufficient assets for solver borrow")]
    fn new_intent_fails_when_assets_insufficient() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(1_000_000)
            .predecessor("solver.test")
            .attached(1)
            .build();
        contract.new_intent(
            "intent".to_string(),
            "solver.deposit".parse().unwrap(),
            "hash-1".to_string(),
            U128(5_000_000),
        );
    }

    #[test]
    fn new_intent_reduces_total_assets_by_requested_amount() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(10_000_000)
            .predecessor("solver.test")
            .attached(1)
            .build();
        contract.new_intent(
            "intent".to_string(),
            "solver.deposit".parse().unwrap(),
            "hash-2".to_string(),
            U128(3_000_000),
        );
        assert_eq!(contract.total_assets, 7_000_000);
    }

    #[test]
    #[should_panic(expected = "Intent with this hash already exists")]
    fn duplicate_user_deposit_hash_panics() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(10_000_000)
            .predecessor("solver.test")
            .attached(1)
            .build();
        contract.insert_intent(
            "solver.test".parse().unwrap(),
            "intent".to_string(),
            "dup-hash".to_string(),
            U128(5_000_000),
        );
        contract.new_intent(
            "intent".to_string(),
            "solver.deposit".parse().unwrap(),
            "dup-hash".to_string(),
            U128(5_000_000),
        );
    }

    #[test]
    #[should_panic(expected = "No intents for solver")]
    fn update_intent_state_restricted_to_owner_solver() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(10_000_000)
            .predecessor("solver.test")
            .attached(1)
            .build();
        contract.insert_intent(
            "solver.test".parse().unwrap(),
            "intent".to_string(),
            "hash-x".to_string(),
            U128(5_000_000),
        );
        init_account("hacker.test", 1);
        contract.update_intent_state(0, State::SwapCompleted);
    }

    #[test]
    fn update_intent_state_by_solver_succeeds() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(10_000_000)
            .predecessor("solver.test")
            .attached(1)
            .build();
        contract.insert_intent(
            "solver.test".parse().unwrap(),
            "intent".to_string(),
            "hash-y".to_string(),
            U128(5_000_000),
        );
        init_account("solver.test", 1);
        contract.update_intent_state(0, State::SwapCompleted);
        let intents = contract.get_intents(None, None);
        assert_eq!(intents.len(), 1);
        assert!(matches!(intents[0].intent.state, State::SwapCompleted));
    }
}
