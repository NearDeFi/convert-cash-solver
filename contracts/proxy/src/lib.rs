use near_sdk::{
    env, near, require,
    store::{IterableMap, IterableSet},
    AccountId, Gas, NearToken, PanicOnDefault, Promise,
};

mod chainsig;
mod intents;
mod near_intents;
use intents::Intent;

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Worker {
    codehash: String,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    pub owner_id: AccountId,
    pub approved_codehashes: IterableSet<String>,
    pub approved_solvers: IterableSet<AccountId>,
    pub worker_by_account_id: IterableMap<AccountId, Worker>,
    pub solver_id_to_indices: IterableMap<AccountId, Vec<u128>>,
    pub index_to_intent: IterableMap<u128, Intent>,
    pub intent_nonce: u128,
}

#[near]
impl Contract {
    #[init]
    #[private]
    pub fn init(owner_id: AccountId) -> Self {
        Self {
            owner_id,
            approved_codehashes: IterableSet::new(b"a"),
            approved_solvers: IterableSet::new(b"b"),
            worker_by_account_id: IterableMap::new(b"c"),
            solver_id_to_indices: IterableMap::new(b"d"),
            index_to_intent: IterableMap::new(b"e"),
            intent_nonce: 0,
        }
    }

    pub fn require_owner(&mut self) {
        require!(env::predecessor_account_id() == self.owner_id);
    }

    pub fn approve_codehash(&mut self, codehash: String) {
        // !!! UPGRADE TO YOUR METHOD OF MANAGING APPROVED WORKER AGENT CODEHASHES !!!
        self.require_owner();
        self.approved_codehashes.insert(codehash);
    }

    /// will throw on client if worker agent is not registered with a codehash in self.approved_codehashes
    pub fn require_approved_codehash(&mut self) {
        let worker = self.get_agent(env::predecessor_account_id());
        require!(self.approved_codehashes.contains(&worker.codehash));
    }

    pub fn register_agent(&mut self, codehash: String) -> bool {
        // THIS IS A LOCAL DEV CONTRACT, SKIPPING ATTESTATION CHECKS

        let predecessor = env::predecessor_account_id();
        self.worker_by_account_id
            .insert(predecessor, Worker { codehash });

        true
    }

    pub fn request_signature(
        &mut self,
        path: String,
        payload: String,
        key_type: String,
    ) -> Promise {
        // self.require_approved_codehash();

        chainsig::internal_request_signature(path, payload, key_type)
    }

    // TODO limit keys added by solvers to one per solver?

    pub fn add_public_key(&mut self, public_key: String) -> Promise {
        // self.require_approved_codehash();

        near_intents::internal_add_public_key(public_key)
    }

    pub fn remove_public_key(&mut self, public_key: String) -> Promise {
        // self.require_approved_codehash();

        near_intents::internal_remove_public_key(public_key)
    }

    // TODO remove_public_key

    // views

    pub fn get_agent(&self, account_id: AccountId) -> Worker {
        self.worker_by_account_id
            .get(&account_id)
            .expect("no worker found")
            .to_owned()
    }
}
