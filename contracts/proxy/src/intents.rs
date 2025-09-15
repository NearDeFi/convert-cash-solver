use crate::*;

#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq, Debug)]
pub enum State {
    Signed, // Signed waiting for deposit on solver account origin chain?
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
    pub payload: String,
    pub signature: String,
    pub quote_hash: String, // Quote generated on token_diff intent
    pub amount: Option<String>,
    pub deposit_hash: Option<String>, // on the account origin chain
    pub swap_hash: Option<String>, // intent hash is calculated once the intent was sent to the solver and executed successfully 
}

#[near]
impl Contract {
    pub fn new_intent(
        &mut self,
        payload: String,
        signature: String,
        quote_hash: String,
    ) {
        // TODO require intent agent

        //let found = self.intents.iter().find(|d| d.deposit_hash == deposit_hash); // IDK the deposit info, I just add a signed intent
        //require!(found.is_none(), "Intent with this hash already exists");

        self.intents.push(Intent {
            created: env::block_timestamp(),
            state: State::Signed,
            payload,
            signature,
            quote_hash,
            amount: None,
            deposit_hash: None,
            swap_hash: None,
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

    pub fn update_swap_hash(&mut self, solver_id: AccountId, swap_hash: String) {
        let (intent, index) = self.get_intent_and_index(solver_id);

        self.intents.replace(
            index,
            Intent {
                swap_hash: Some(swap_hash),
                ..intent.clone()
            },
        );
    }
    pub fn update_deposit_info(&mut self, solver_id: AccountId, amount: String, deposit_hash: String) {
        let (intent, index) = self.get_intent_and_index(solver_id);

        self.intents.replace(
            index,
            Intent { 
                amount: Some(amount), 
                deposit_hash: Some(deposit_hash), 
                ..intent.clone() 
            },
        );
    }

    pub fn update_deposit_info_by_quote_hash(&mut self, quote_hash: String, amount: String, deposit_hash: String) {
        // Find the intent by quote_hash
        let mut found_index = None;
        for (index, intent) in self.intents.iter().enumerate() {
            if intent.quote_hash == quote_hash {
                found_index = Some(index as u32);
                break;
            }
        }
        
        let index = found_index.expect("Intent with this quote_hash not found");
        let intent = self.intents.get(index).unwrap().clone();

        self.intents.replace(
            index,
            Intent { 
                amount: Some(amount), 
                deposit_hash: Some(deposit_hash), 
                ..intent.clone() 
            },
        );
    }

    pub fn get_intent_by_solver(&self, solver_id: AccountId) -> Intent {
        let (intent, _index) = self.get_intent_and_index(solver_id);
        intent
    }

    pub fn get_intent_by_quote_hash(&self, quote_hash: String) -> Intent {
        for intent in self.intents.iter() {
            if intent.quote_hash == quote_hash {
                return intent.clone();
            }
        }
        panic!("Intent with this quote_hash not found");
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
