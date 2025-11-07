use crate::*;
use near_contract_standards::fungible_token::core::ext_ft_core;
use near_sdk::{env, ext_contract, json_types::U128, Gas, NearToken, Promise, PromiseResult};

const GAS_FOR_SOLVER_BORROW: Gas = Gas::from_tgas(30);
const GAS_FOR_NEW_INTENT_CALLBACK: Gas = Gas::from_tgas(8);
pub const SOLVER_BORROW_AMOUNT: u128 = 5_000_000; // 5 USDC with 6 decimals (mock FT)

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
}

#[near]
impl Contract {
    pub fn new_intent(
        &mut self,
        intent_data: String,
        _solver_deposit_address: AccountId,
        user_deposit_hash: String,
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

        require!(
            self.total_assets >= SOLVER_BORROW_AMOUNT,
            "Insufficient assets for solver borrow"
        );

        self.total_assets = self
            .total_assets
            .checked_sub(SOLVER_BORROW_AMOUNT)
            .expect("total_assets underflow");

        // Intent checks out, let solver borrow liquidity

        let promise: Promise = ext_ft_core::ext(self.asset.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_SOLVER_BORROW)
            .ft_transfer(
                solver_id.clone(),
                U128(SOLVER_BORROW_AMOUNT),
                Some("Solver borrow".to_string()),
            )
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_NEW_INTENT_CALLBACK)
                    .on_new_intent_callback(
                        intent_data,
                        solver_id,
                        user_deposit_hash,
                        U128(SOLVER_BORROW_AMOUNT),
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
                self.insert_intent(solver_id, intent_data, user_deposit_hash);
                true
            }
            _ => false,
        }
    }

    fn insert_intent(
        &mut self,
        solver_id: AccountId,
        intent_data: String,
        user_deposit_hash: String,
    ) {
        let index = self.intent_nonce;
        self.intent_nonce += 1;

        let mut indices = vec![index];
        if let Some(existing_indices) = self.solver_id_to_indices.get(&solver_id) {
            indices.extend(existing_indices);
        }
        self.solver_id_to_indices.insert(solver_id.clone(), indices);

        self.index_to_intent.insert(
            index,
            Intent {
                created: env::block_timestamp(),
                state: State::StpLiquidityBorrowed,
                intent_data,
                user_deposit_hash,
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
