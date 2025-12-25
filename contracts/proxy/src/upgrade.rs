//! # Contract Upgrade Module
//!
//! Provides functionality for upgrading the contract code.
//! Only the contract owner can perform upgrades.
//!
//! ## Usage
//!
//! To upgrade the contract, call `update_contract` with the new WASM code
//! as the function input (not as an argument).
//!
//! Example using NEAR CLI:
//! ```bash
//! near call <contract_id> update_contract --base64-file <path_to_wasm> --accountId <owner_id>
//! ```

use crate::*;

#[near]
impl Contract {
    /// Upgrades the contract code to a new version.
    ///
    /// The new WASM code should be passed as the transaction input (not as an argument).
    /// This allows for larger contract sizes that wouldn't fit in normal arguments.
    ///
    /// # Access Control
    ///
    /// Only the contract owner can call this method.
    ///
    /// # Returns
    ///
    /// A promise that deploys the new contract code.
    ///
    /// # Panics
    ///
    /// - If the caller is not the contract owner
    /// - If no input data is provided
    pub fn update_contract(&self) -> Promise {
        self.require_owner();

        let code = env::input().expect("No contract code provided").to_vec();

        Promise::new(env::current_account_id())
            .deploy_contract(code)
            .as_return()
    }
}
