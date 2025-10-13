use crate::*;

use near_sdk::ext_contract;

const INTENTS_CONTRACT_ID: &str = "intents.near";
const GAS: Gas = Gas::from_tgas(10);
const ATTACHED_DEPOSIT: NearToken = NearToken::from_yoctonear(1);

#[allow(dead_code)]
#[ext_contract(intents_contract)]
trait IntentsContract {
    fn add_public_key(&self, public_key: String) -> Promise;
    fn remove_public_key(&self, public_key: String) -> Promise;
}

pub fn internal_add_public_key(public_key: String) -> Promise {
    intents_contract::ext(INTENTS_CONTRACT_ID.parse().unwrap())
        .with_static_gas(GAS)
        .with_attached_deposit(ATTACHED_DEPOSIT)
        .add_public_key(public_key)
}

pub fn internal_remove_public_key(public_key: String) -> Promise {
    intents_contract::ext(INTENTS_CONTRACT_ID.parse().unwrap())
        .with_static_gas(GAS)
        .with_attached_deposit(ATTACHED_DEPOSIT)
        .remove_public_key(public_key)
}
