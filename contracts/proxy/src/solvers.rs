use crate::*;

// #[allow(dead_code)]
// #[ext_contract(ext_self)]
// trait Callbacks {
//     fn complete_signature_callback(
//         &mut self,
//         solver_id: AccountId,
//         new_state: State,
//         path: String,
//         payload: String,
//         key_type: String,
//         #[callback_result] call_result: Result<(), PromiseError>,
//     ) -> bool;
// }

// const CALLBACK_GAS: Gas = Gas::from_tgas(5);

#[near]
impl Contract {
    pub fn request_liquidity(&mut self, payload: String) -> Promise {
        let solver_id = env::predecessor_account_id();

        // they can only request liquidity if they have claimed the intent
        let intent = self.get_intent_by_solver(solver_id.clone());
        require!(intent.state == State::Claimed, "Intent is not claimed");

        let path = "pool-1".to_owned();
        let key_type = "Eddsa".to_owned();

        chainsig::internal_request_signature(path.clone(), payload.clone(), key_type.clone())
    }

    pub fn complete_swap(&mut self, payload: String) -> Promise {
        // let solver_id = env::predecessor_account_id();

        // // they can only complete the swap and request a chain sig if they have had the liquidity provided
        // let intent = self.get_intent_by_solver(solver_id.clone());
        // require!(
        //     intent.state == State::LiquidityProvided,
        //     "Intent has not had LiquidityProvided"
        // );

        let path = "tron-1".to_owned();
        let key_type = "Ecdsa".to_owned();

        chainsig::internal_request_signature(path.clone(), payload.clone(), key_type.clone())
    }
}
