//! # Convert Cash Solver Proxy Contract
//!
//! A NEAR smart contract that serves as a liquidity vault and intent solver proxy.
//! This contract enables:
//!
//! - **Vault Operations**: NEP-141 compliant vault for depositing and withdrawing fungible tokens
//! - **Intent Solving**: Manages intents for solvers to borrow liquidity and fulfill cross-chain swaps
//! - **Agent Management**: TEE-based worker agent registration and codehash verification
//! - **MPC Signatures**: Request chain signatures via NEAR's MPC network
//! - **Cross-Chain Withdrawals**: OMFT bridge integration for EVM and Solana withdrawals
//!
//! ## Architecture
//!
//! The contract is organized into several modules:
//! - [`vault`]: Core vault logic for deposits, redemptions, and share calculations
//! - [`intents`]: Intent lifecycle management for solver borrowing
//! - [`withdraw`]: Cross-chain withdrawal functionality (EVM/Solana)
//! - [`chainsig`]: MPC signature request handling
//! - [`near_intents`]: NEAR Intents protocol integration
//! - [`vault_standards`]: NEP-621 vault standard implementation

use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env, near, require,
    store::{IterableMap, IterableSet, Vector},
    AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault, Promise,
};

use near_contract_standards::fungible_token::{
    core_impl::FungibleToken, metadata::FungibleTokenMetadata,
};

mod chainsig;
mod intents;
mod near_intents;
mod vault;
mod vault_standards;
mod withdraw;

#[cfg(test)]
pub mod test_utils;

use intents::Intent;
use vault::PendingRedemption;

/// Represents a registered TEE worker agent with its attestation codehash.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Worker {
    /// The codehash from the TEE attestation, used to verify the agent's integrity.
    codehash: String,
}

/// Storage keys for NEAR SDK collections.
#[derive(BorshSerialize, BorshDeserialize, BorshStorageKey)]
pub enum StorageKey {
    /// Storage prefix for the NEP-141 fungible token (vault shares).
    FungibleToken,
    /// Storage prefix for the pending redemption queue.
    PendingRedemptions,
}

/// Main contract state containing vault, intent, and agent management data.
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    /// The account authorized to manage contract settings.
    pub owner_id: AccountId,
    /// Set of approved TEE codehashes for worker agent verification.
    pub approved_codehashes: IterableSet<String>,
    /// Set of approved solver account IDs.
    pub approved_solvers: IterableSet<AccountId>,
    /// Mapping from account ID to registered worker agent.
    pub worker_by_account_id: IterableMap<AccountId, Worker>,
    /// Mapping from solver ID to their intent indices.
    pub solver_id_to_indices: IterableMap<AccountId, Vec<u128>>,
    /// Mapping from intent index to intent data.
    pub index_to_intent: IterableMap<u128, Intent>,
    /// Global nonce for generating unique intent indices.
    pub intent_nonce: u128,

    // Vault State
    /// NEP-141 fungible token representing vault shares.
    pub token: FungibleToken,
    /// Metadata for the vault share token.
    pub metadata: FungibleTokenMetadata,
    /// Account ID of the underlying asset token (NEP-141).
    pub asset: AccountId,
    /// Total available assets in the vault (deposits minus active borrows).
    pub total_assets: u128,
    /// Vault owner account.
    pub owner: AccountId,
    /// Extra decimals for share precision (e.g., 3 means 1000 shares per asset unit).
    pub extra_decimals: u8,
    /// FIFO queue for pending redemptions awaiting liquidity.
    pub pending_redemptions: Vector<PendingRedemption>,
    /// Head index of the pending redemptions queue.
    pub pending_redemptions_head: u32,
}

#[near]
impl Contract {
    /// Initializes the contract with vault configuration.
    ///
    /// # Arguments
    ///
    /// * `owner_id` - Account authorized to manage contract settings
    /// * `asset` - Account ID of the underlying NEP-141 asset token
    /// * `metadata` - Fungible token metadata for vault shares
    /// * `extra_decimals` - Additional decimal precision for shares
    ///
    /// # Returns
    ///
    /// A new `Contract` instance with initialized state.
    #[init]
    #[private]
    pub fn init(
        owner_id: AccountId,
        asset: AccountId,
        metadata: FungibleTokenMetadata,
        extra_decimals: u8,
    ) -> Self {
        Self {
            owner_id,
            approved_codehashes: IterableSet::new(b"a"),
            approved_solvers: IterableSet::new(b"b"),
            worker_by_account_id: IterableMap::new(b"c"),
            solver_id_to_indices: IterableMap::new(b"d"),
            index_to_intent: IterableMap::new(b"e"),
            intent_nonce: 0,
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata,
            asset,
            total_assets: 0,
            owner: env::predecessor_account_id(),
            extra_decimals,
            pending_redemptions: Vector::new(StorageKey::PendingRedemptions),
            pending_redemptions_head: 0,
        }
    }

    /// Asserts that the caller is the contract owner.
    ///
    /// # Panics
    ///
    /// Panics if the predecessor account is not the owner.
    pub fn require_owner(&mut self) {
        require!(env::predecessor_account_id() == self.owner_id);
    }

    /// Approves a TEE codehash for worker agent registration.
    ///
    /// Only approved codehashes can register as worker agents. This provides
    /// security by ensuring only verified TEE environments can operate.
    ///
    /// # Arguments
    ///
    /// * `codehash` - The TEE attestation codehash to approve
    ///
    /// # Panics
    ///
    /// Panics if caller is not the contract owner.
    pub fn approve_codehash(&mut self, codehash: String) {
        self.require_owner();
        self.approved_codehashes.insert(codehash);
    }

    /// Asserts that the caller has an approved codehash.
    ///
    /// # Panics
    ///
    /// Panics if the caller is not registered or their codehash is not approved.
    pub fn require_approved_codehash(&mut self) {
        let worker = self.get_agent(env::predecessor_account_id());
        require!(self.approved_codehashes.contains(&worker.codehash));
    }

    /// Registers a worker agent with a TEE codehash.
    ///
    /// In production, this should verify the TEE attestation before registration.
    /// Currently simplified for local development.
    ///
    /// # Arguments
    ///
    /// * `codehash` - The TEE attestation codehash for this agent
    ///
    /// # Returns
    ///
    /// `true` if registration succeeded.
    pub fn register_agent(&mut self, codehash: String) -> bool {
        let predecessor = env::predecessor_account_id();
        self.worker_by_account_id
            .insert(predecessor, Worker { codehash });

        true
    }

    /// Requests a cryptographic signature from the MPC network.
    ///
    /// This initiates a cross-contract call to the MPC signer contract
    /// to sign a payload using the specified derivation path.
    ///
    /// # Arguments
    ///
    /// * `path` - BIP-32 derivation path for key generation
    /// * `payload` - The data to sign (hex-encoded hash)
    /// * `key_type` - Either "Ecdsa" for secp256k1 or "Eddsa" for ed25519
    ///
    /// # Returns
    ///
    /// A promise that resolves to the signature.
    pub fn request_signature(
        &mut self,
        path: String,
        payload: String,
        key_type: String,
    ) -> Promise {
        chainsig::internal_request_signature(path, payload, key_type)
    }

    /// Adds a public key to the NEAR Intents contract.
    ///
    /// This allows the contract to authorize transactions on behalf of
    /// users via the Intents protocol.
    ///
    /// # Arguments
    ///
    /// * `public_key` - The public key to register
    ///
    /// # Returns
    ///
    /// A promise for the cross-contract call result.
    pub fn add_public_key(&mut self, public_key: String) -> Promise {
        near_intents::internal_add_public_key(public_key)
    }

    /// Removes a public key from the NEAR Intents contract.
    ///
    /// # Arguments
    ///
    /// * `public_key` - The public key to remove
    ///
    /// # Returns
    ///
    /// A promise for the cross-contract call result.
    pub fn remove_public_key(&mut self, public_key: String) -> Promise {
        near_intents::internal_remove_public_key(public_key)
    }

    // ==================== View Methods ====================

    /// Retrieves a registered worker agent by account ID.
    ///
    /// # Arguments
    ///
    /// * `account_id` - The account ID to look up
    ///
    /// # Returns
    ///
    /// The `Worker` struct for the given account.
    ///
    /// # Panics
    ///
    /// Panics if no worker is registered for the given account.
    pub fn get_agent(&self, account_id: AccountId) -> Worker {
        self.worker_by_account_id
            .get(&account_id)
            .expect("no worker found")
            .to_owned()
    }
}
