//! # Chain Signature Module
//!
//! Provides integration with NEAR's MPC (Multi-Party Computation) network
//! for generating cryptographic signatures. This enables the contract to
//! sign messages for external chains without exposing private keys.
//!
//! ## Supported Key Types
//!
//! - **ECDSA (secp256k1)**: For EVM chains, Bitcoin, etc.
//! - **EdDSA (ed25519)**: For Solana, NEAR, and other ed25519-based chains
//!
//! ## Usage
//!
//! The `internal_request_signature` function is called by the main contract
//! to sign payloads using a derived key path. The MPC network returns the
//! signature asynchronously via a cross-contract callback.

use crate::*;

use near_sdk::ext_contract;
use serde::Serialize;

// ============================================================================
// Data Structures
// ============================================================================

/// Payload wrapper for the MPC sign request.
///
/// The payload type determines which signing algorithm is used.
#[derive(Debug, Serialize)]
pub enum Payload {
    /// ECDSA signature over secp256k1 (32-byte hash).
    Ecdsa(String),
    /// EdDSA signature over ed25519 (32-byte hash).
    Eddsa(String),
}

/// Request structure for the MPC signing contract.
#[derive(Debug, Serialize)]
pub struct SignRequest {
    /// The payload to sign (wrapped with algorithm type).
    pub payload_v2: Payload,
    /// BIP-32 derivation path for key generation.
    pub path: String,
    /// Domain identifier (0 for ECDSA, 1 for EdDSA).
    pub domain_id: u64,
}

// ============================================================================
// External Contract Interface
// ============================================================================

/// Interface for the NEAR MPC signer contract.
#[allow(dead_code)]
#[ext_contract(mpc_contract)]
trait MPCContract {
    /// Requests a signature from the MPC network.
    fn sign(&self, request: SignRequest);
}

// ============================================================================
// Constants
// ============================================================================

/// Gas allocation for MPC sign request.
const GAS: Gas = Gas::from_tgas(10);

/// Deposit required for MPC sign request (1 yoctoNEAR).
const ATTACHED_DEPOSIT: NearToken = NearToken::from_yoctonear(1);

// ============================================================================
// Internal Functions
// ============================================================================

/// Requests a cryptographic signature from the MPC network.
///
/// This initiates a cross-contract call to the MPC signer contract.
/// The signature will be returned asynchronously.
///
/// # Arguments
///
/// * `path` - BIP-32 derivation path (e.g., "m/44'/60'/0'/0/0" for Ethereum)
/// * `payload` - The hash to sign (hex-encoded, 32 bytes)
/// * `key_type` - Either "Ecdsa" or "Eddsa"
///
/// # Returns
///
/// A promise that resolves to the signature result.
///
/// # MPC Contract Selection
///
/// The function automatically selects the appropriate MPC contract:
/// - Testnet: `v1.signer-prod.testnet`
/// - Mainnet: `v1.signer`
pub fn internal_request_signature(path: String, payload: String, key_type: String) -> Promise {
    let (payload_v2, domain_id) = match key_type.as_str() {
        "Eddsa" => (Payload::Eddsa(payload), 1),
        _ => (Payload::Ecdsa(payload), 0),
    };

    let request = SignRequest {
        payload_v2,
        path,
        domain_id,
    };

    // Determine MPC contract based on network
    let mpc_contract_id = if env::current_account_id().as_str().contains("testnet") {
        "v1.signer-prod.testnet"
    } else {
        "v1.signer"
    };

    // =========================================================================
    // Cross-Contract Call: MPC Signature Request
    // =========================================================================
    // Calls the NEAR MPC signer contract to generate a signature.
    // The MPC network consists of multiple nodes that collaboratively sign
    // without any single node having access to the full private key.
    // =========================================================================
    mpc_contract::ext(mpc_contract_id.parse().unwrap())
        .with_static_gas(GAS)
        .with_attached_deposit(ATTACHED_DEPOSIT)
        .sign(request)
}
