use crate::*;

#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq)]
pub enum State {
    Deposited,
    Claimed,
    LiquidityProvided,
    LiquidityCredited,
    WithdrawRequested,
    CompleteSwap,
    CheckSwapComplete,
    SwapComplete,
    UserLiquidityProvided,
    ReturnLiquidity,
    LiquidityReturned,
    IntentComplete,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Intent {
    pub created: u64,
    pub state: State,
    pub data: String,
    pub deposit_hash: String,
}

#[near]
impl Contract {
    pub fn new_intent(&mut self, data: String, deposit_hash: String) {
        // TODO require intent agent

        let found = self.intents.iter().find(|d| d.deposit_hash == deposit_hash);
        require!(found.is_none(), "Intent with this hash already exists");

        self.intents.push(Intent {
            created: env::block_timestamp(),
            state: State::Deposited,
            data,
            deposit_hash,
        });
    }

    // debugging remove later
    pub fn clear_intents(&mut self) {
        self.require_owner();

        self.intents.clear();
    }

    pub fn get_intents(&self) -> Vec<&Intent> {
        self.intents.iter().collect()
    }

    // TODO make only intents in deposited state
    pub fn claim_intent(&mut self, index: u32) {
        let solver_id = env::predecessor_account_id();
        // require!(
        //     self.approved_solvers.contains(&solver_id),
        //     "Solver not approved"
        // );

        let intent = self.intents.get(index).expect("Intent not found");

        self.solver_id_to_intent_index.insert(solver_id, index);

        self.intents.replace(
            index,
            Intent {
                state: State::Claimed,
                ..intent.clone()
            },
        );
    }

    pub fn update_intent_state(&mut self, solver_id: AccountId, state: State) {
        let (intent, index) = self.get_intent_and_index(solver_id);

        self.intents.replace(
            index,
            Intent {
                state,
                ..intent.clone()
            },
        );
    }

    pub fn get_intent_by_solver(&self, solver_id: AccountId) -> Intent {
        let (intent, _index) = self.get_intent_and_index(solver_id);
        intent
    }

    // helper

    fn get_intent_and_index(&self, solver_id: AccountId) -> (Intent, u32) {
        let index = self
            .solver_id_to_intent_index
            .get(&solver_id)
            .expect("No intent index found for solver");
        let intent = self.intents.get(*index).unwrap().clone();
        (intent, *index)
    }
}
