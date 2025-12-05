//! # NEAR Intents Integration Module
//!
//! Provides integration with the NEAR Intents protocol for managing
//! authorized public keys that can sign transactions on behalf of users.
//!
//! ## Overview
//!
//! The Intents protocol on NEAR allows contracts to authorize signing keys
//! that can execute transactions within defined parameters. This module
//! provides functions to add and remove these authorized keys.
//!
//! ## Security
//!
//! Adding a public key grants it the ability to sign transactions for the
//! contract's intent operations. Keys should be carefully managed and
//! removed when no longer needed.

use crate::*;

use near_sdk::ext_contract;

// ============================================================================
// Constants
// ============================================================================

/// The NEAR Intents contract account ID on mainnet.
const INTENTS_CONTRACT_ID: &str = "intents.near";

/// Gas allocation for Intents contract calls.
const GAS: Gas = Gas::from_tgas(10);

/// Deposit required for Intents contract calls (1 yoctoNEAR).
const ATTACHED_DEPOSIT: NearToken = NearToken::from_yoctonear(1);

// ============================================================================
// External Contract Interface
// ============================================================================

/// Interface for the NEAR Intents contract.
#[allow(dead_code)]
#[ext_contract(intents_contract)]
trait IntentsContract {
    /// Adds a public key authorized to sign intent transactions.
    fn add_public_key(&self, public_key: String) -> Promise;

    /// Removes a previously authorized public key.
    fn remove_public_key(&self, public_key: String) -> Promise;
}

// ============================================================================
// Internal Functions
// ============================================================================

/// Adds a public key to the NEAR Intents contract.
///
/// The added key will be able to sign intent-based transactions
/// on behalf of this contract.
///
/// # Arguments
///
/// * `public_key` - The public key to authorize (ed25519 or secp256k1 format)
///
/// # Returns
///
/// A promise for the cross-contract call result.
pub fn internal_add_public_key(public_key: String) -> Promise {
    // =========================================================================
    // Cross-Contract Call: Add Public Key to Intents
    // =========================================================================
    // Registers a public key with the NEAR Intents protocol.
    // This allows the key holder to sign transactions for this contract's
    // intent-based operations (e.g., authorizing solver actions).
    // =========================================================================
    intents_contract::ext(INTENTS_CONTRACT_ID.parse().unwrap())
        .with_static_gas(GAS)
        .with_attached_deposit(ATTACHED_DEPOSIT)
        .add_public_key(public_key)
}

/// Removes a public key from the NEAR Intents contract.
///
/// The key will no longer be able to sign intent transactions
/// for this contract.
///
/// # Arguments
///
/// * `public_key` - The public key to remove
///
/// # Returns
///
/// A promise for the cross-contract call result.
pub fn internal_remove_public_key(public_key: String) -> Promise {
    // =========================================================================
    // Cross-Contract Call: Remove Public Key from Intents
    // =========================================================================
    // Deauthorizes a previously registered public key from the Intents protocol.
    // This should be called when a key is compromised or no longer needed.
    // =========================================================================
    intents_contract::ext(INTENTS_CONTRACT_ID.parse().unwrap())
        .with_static_gas(GAS)
        .with_attached_deposit(ATTACHED_DEPOSIT)
        .remove_public_key(public_key)
}
