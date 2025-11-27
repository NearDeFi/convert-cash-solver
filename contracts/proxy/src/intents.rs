use crate::*;
use near_contract_standards::fungible_token::{core::ext_ft_core, FungibleTokenCore};
use near_sdk::{env, ext_contract, json_types::U128, Gas, NearToken, Promise, PromiseResult};

const GAS_FOR_SOLVER_BORROW: Gas = Gas::from_tgas(30);
const GAS_FOR_NEW_INTENT_CALLBACK: Gas = Gas::from_tgas(8);
pub const SOLVER_BORROW_AMOUNT: u128 = 5_000_000; // 5 USDC with 6 decimals (mock FT)

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

#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq)]
pub enum State {
    StpLiquidityBorrowed,
    StpLiquidityDeposited,
    StpLiquidityWithdrawn,
    StpIntentAccountCredited,
    SwapCompleted,
    UserLiquidityBorrowed,
    UserLiquidityDeposited,
    StpLiquidityReturned,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Intent {
    pub created: u64,
    pub state: State,
    pub intent_data: String,
    pub user_deposit_hash: String,
    pub borrow_amount: u128, // Amount borrowed (principal) for this Intent
    pub borrow_total_supply: u128, // Total share supply at time of borrow (for intent_yield attribution)
    pub repayment_amount: Option<u128>, // Repayment amount (principal + intent_yield) when repaid
}

#[near]
impl Contract {
    pub fn new_intent(
        &mut self,
        intent_data: String,
        _solver_deposit_address: AccountId,
        user_deposit_hash: String,
        amount: Option<U128>,
    ) {
        // update user_deposit_hash to the request_id for intent

        // TODO check intent / quote for solver and make sure it's valid
        // TODO require intent agent
        // TODO move liquidity and create new intent with callback after liquidity is transferred to deposit address successfully
        // ft_transfer with a callback to create new intent with callback after liquidity is transferred to deposit address successfully

        if self
            .index_to_intent
            .values()
            .any(|intent| intent.user_deposit_hash == user_deposit_hash)
        {
            env::panic_str("Intent with this hash already exists");
        }

        let solver_id = env::predecessor_account_id();

        // Use provided amount or default to SOLVER_BORROW_AMOUNT
        let borrow_amount = amount.map(|a| a.0).unwrap_or(SOLVER_BORROW_AMOUNT);

        // Prevent borrowing when there are pending redemptions in the queue
        // This ensures lenders waiting for liquidity are prioritized over new borrows
        require!(
            self.pending_redemptions_head >= self.pending_redemptions.len(),
            "Cannot borrow while redemptions are pending"
        );

        require!(
            self.total_assets >= borrow_amount,
            "Insufficient assets for solver borrow"
        );

        self.total_assets = self
            .total_assets
            .checked_sub(borrow_amount)
            .expect("total_assets underflow");

        // Intent checks out, let solver borrow liquidity

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

        promise.as_return();
    }

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
                self.total_assets = self
                    .total_assets
                    .checked_add(amount.0)
                    .expect("total_assets overflow on borrow revert");
                false
            }
        }
    }

    fn insert_intent(
        &mut self,
        solver_id: AccountId,
        intent_data: String,
        user_deposit_hash: String,
        borrow_amount: U128,
    ) {
        let index = self.intent_nonce;
        self.intent_nonce += 1;

        let mut indices = vec![index];
        if let Some(existing_indices) = self.solver_id_to_indices.get(&solver_id) {
            indices.extend(existing_indices);
        }
        self.solver_id_to_indices.insert(solver_id.clone(), indices);

        // Capture deposit state at borrow time for intent_yield attribution
        let borrow_total_supply = self.token.ft_total_supply().0;

        self.index_to_intent.insert(
            index,
            Intent {
                created: env::block_timestamp(),
                state: State::StpLiquidityBorrowed,
                intent_data,
                user_deposit_hash,
                borrow_amount: borrow_amount.0,
                borrow_total_supply,
                repayment_amount: None,
            },
        );
    }

    // debugging remove later
    pub fn clear_intents(&mut self) {
        self.require_owner();
        self.solver_id_to_indices.clear();
        self.index_to_intent.clear();
    }

    pub fn get_intents(&self) -> Vec<Intent> {
        self.index_to_intent.values().cloned().collect()
    }

    pub fn update_intent_state(&mut self, index: u128, state: State) {
        let solver_id = env::predecessor_account_id();
        let indices = self.get_intent_indices(solver_id);

        // must exist and be owned by the solver
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

    pub fn get_intents_by_solver(&self, solver_id: AccountId) -> Vec<Intent> {
        let indices = self.get_intent_indices(solver_id);
        indices
            .iter()
            .filter_map(|i| self.index_to_intent.get(i).cloned())
            .collect()
    }

    // helper

    fn get_intent_indices(&self, solver_id: AccountId) -> Vec<u128> {
        self.solver_id_to_indices
            .get(&solver_id)
            .expect("No intents for solver")
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::builders::ContractBuilder;
    use crate::test_utils::helpers::init_ctx as init_account;

    #[test]
    #[should_panic(expected = "Insufficient assets for solver borrow")]
    fn new_intent_fails_when_assets_insufficient() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(1_000_000) // less than default borrow
            .predecessor("solver.test")
            .attached(1)
            .build();
        contract.new_intent(
            "intent".to_string(),
            "solver.deposit".parse().unwrap(),
            "hash-1".to_string(),
            None,
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
            Some(U128(3_000_000)),
        );
        assert_eq!(contract.total_assets, 7_000_000);
    }

    #[test]
    fn new_intent_default_amount_uses_solver_borrow_amount() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(SOLVER_BORROW_AMOUNT + 1_000_000)
            .predecessor("solver.test")
            .attached(1)
            .build();
        contract.new_intent(
            "intent".to_string(),
            "solver.deposit".parse().unwrap(),
            "hash-default".to_string(),
            None,
        );
        assert_eq!(contract.total_assets, 1_000_000);
    }
    #[test]
    #[should_panic(expected = "Intent with this hash already exists")]
    fn duplicate_user_deposit_hash_panics() {
        let mut contract = ContractBuilder::new("owner.test", "usdc.test")
            .total_assets(10_000_000)
            .predecessor("solver.test")
            .attached(1)
            .build();
        // Insert intent for solver
        contract.insert_intent(
            "solver.test".parse().unwrap(),
            "intent".to_string(),
            "dup-hash".to_string(),
            U128(5_000_000),
        );
        // Now calling new_intent with same hash should panic
        contract.new_intent(
            "intent".to_string(),
            "solver.deposit".parse().unwrap(),
            "dup-hash".to_string(),
            None,
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
        // Insert intent for solver
        contract.insert_intent(
            "solver.test".parse().unwrap(),
            "intent".to_string(),
            "hash-x".to_string(),
            U128(5_000_000),
        );
        // Now try to update from a different predecessor
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
        // Insert intent for solver
        contract.insert_intent(
            "solver.test".parse().unwrap(),
            "intent".to_string(),
            "hash-y".to_string(),
            U128(5_000_000),
        );
        // Update as same solver
        init_account("solver.test", 1);
        contract.update_intent_state(0, State::SwapCompleted);
        let intents = contract.get_intents();
        assert_eq!(intents.len(), 1);
        assert!(matches!(intents[0].state, State::SwapCompleted));
    }
}
