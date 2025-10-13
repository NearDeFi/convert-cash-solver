use crate::*;

#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq)]
pub enum State {
    StpLiquidityBorrowed,
    StpLiquidityDeposited,
    StpLiquidityWithdrawn,
    StpIntentAccountCredited,
    IntentsExecuted,
    SwapCompleted,
    UserLiquidityDeposited,
    UserLiquidityWithdrawn,
    StpLiquidityReturned,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Intent {
    pub created: u64,
    pub state: State,
    pub data: String,
    pub user_deposit_hash: String,
}

#[near]
impl Contract {
    pub fn new_intent(&mut self, data: String, user_deposit_hash: String) {
        // TODO require intent agent

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
            self.solver_id_to_indices.insert(solver_id, indices);
        } else {
            self.solver_id_to_indices.insert(solver_id, vec![index]);
        }

        self.index_to_intent.insert(
            index,
            Intent {
                created: env::block_timestamp(),
                state: State::StpLiquidityBorrowed,
                data,
                user_deposit_hash,
            },
        );

        self.intent_nonce += 1;
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
