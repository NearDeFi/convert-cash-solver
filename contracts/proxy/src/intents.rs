use crate::*;
use near_contract_standards::fungible_token::core::ext_ft_core;
use near_sdk::{json_types::U128, Gas, NearToken};

const GAS_FOR_SOLVER_BORROW: Gas = Gas::from_tgas(30);
pub const SOLVER_BORROW_AMOUNT: u128 = 5_000_000; // 5 USDC with 6 decimals (mock FT)

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
        solver_deposit_address: AccountId,
        user_deposit_hash: String,
    ) {
        // update user_deposit_hash to the request_id for intent

        // TODO check intent / quote for solver and make sure it's valid
        // TODO require intent agent
        // TODO move liquidity and create new intent with callback after liquidity is transferred to deposit address successfully
        // ft_transfer with a callback to create new intent with callback after liquidity is transferred to deposit address successfully

        // Check if intent with this hash already exists
        for (_, intent) in self.index_to_intent.iter() {
            require!(
                intent.user_deposit_hash != user_deposit_hash,
                "Intent with this hash already exists"
            );
        }

        let index = self.intent_nonce;
        let solver_id = env::predecessor_account_id();
        let mut indices = vec![index];

        if let Some(existing_indices) = self.solver_id_to_indices.get(&solver_id) {
            indices.extend(existing_indices);
            self.solver_id_to_indices.insert(solver_id.clone(), indices);
        } else {
            self.solver_id_to_indices
                .insert(solver_id.clone(), vec![index]);
        }

        self.index_to_intent.insert(
            index,
            Intent {
                created: env::block_timestamp(),
                state: State::StpLiquidityBorrowed,
                intent_data,
                user_deposit_hash,
            },
        );

        self.intent_nonce += 1;

        self.borrow_liquidity(&solver_id.clone());
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

    fn borrow_liquidity(&mut self, solver_id: &AccountId) {
        if SOLVER_BORROW_AMOUNT == 0 {
            return;
        }

        require!(
            self.total_assets >= SOLVER_BORROW_AMOUNT,
            "Insufficient assets for solver reward"
        );

        self.total_assets = self
            .total_assets
            .checked_sub(SOLVER_BORROW_AMOUNT)
            .expect("total_assets underflow");

        let _ = ext_ft_core::ext(self.asset.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_SOLVER_BORROW)
            .ft_transfer(
                solver_id.clone(),
                U128(SOLVER_BORROW_AMOUNT),
                Some("Solver reward".to_string()),
            );
    }
}
