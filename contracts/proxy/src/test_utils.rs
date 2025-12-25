//! # Test Utilities
//!
//! Provides helper functions and builders for unit testing the contract.
//! These utilities simplify test setup by handling NEAR SDK context
//! initialization and contract configuration.
//!
//! ## Modules
//!
//! - [`helpers`]: Low-level context and contract initialization
//! - [`builders`]: Builder pattern for flexible contract configuration

/// Helper functions for test context and contract initialization.
#[cfg(test)]
pub mod helpers {
    use crate::Contract;
    use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::{testing_env, NearToken};

    /// Initializes the NEAR VM context for testing.
    ///
    /// Sets up the predecessor account and attached deposit for the
    /// subsequent contract calls.
    ///
    /// # Arguments
    ///
    /// * `predecessor` - The account ID that will be the caller
    /// * `deposit_yocto` - Amount of yoctoNEAR attached to calls
    ///
    /// # Example
    ///
    /// ```ignore
    /// init_ctx("alice.test", 1); // Alice calls with 1 yoctoNEAR
    /// contract.some_method();
    /// ```
    pub fn init_ctx(predecessor: &str, deposit_yocto: u128) {
        let mut builder = VMContextBuilder::new();
        builder
            .predecessor_account_id(predecessor.parse().unwrap())
            .attached_deposit(NearToken::from_yoctonear(deposit_yocto));
        testing_env!(builder.build());
    }

    /// Initializes a contract with default settings (3 extra decimals).
    ///
    /// # Arguments
    ///
    /// * `owner` - The contract owner account ID
    /// * `asset` - The underlying asset token account ID
    ///
    /// # Returns
    ///
    /// A new `Contract` instance ready for testing.
    pub fn init_contract(owner: &str, asset: &str) -> Contract {
        init_contract_ex(owner, asset, 3)
    }

    /// Initializes a contract with custom extra decimals.
    ///
    /// # Arguments
    ///
    /// * `owner` - The contract owner account ID
    /// * `asset` - The underlying asset token account ID
    /// * `extra_decimals` - Additional decimal precision for shares
    ///
    /// # Returns
    ///
    /// A new `Contract` instance with the specified configuration.
    pub fn init_contract_ex(owner: &str, asset: &str, extra_decimals: u8) -> Contract {
        init_ctx(owner, 0);
        let metadata = FungibleTokenMetadata {
            spec: "ft-1.0.0".to_string(),
            name: "USDC Vault Shares".to_string(),
            symbol: "vUSDC".to_string(),
            icon: None,
            reference: None,
            reference_hash: None,
            decimals: 24,
        };
        Contract::init(
            owner.parse().unwrap(),
            asset.parse().unwrap(),
            metadata,
            extra_decimals,
            1, // 1% solver fee
        )
    }
}

/// Builder pattern for flexible contract configuration in tests.
#[cfg(test)]
pub mod builders {
    use crate::test_utils::helpers::init_ctx;
    use crate::Contract;
    use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;

    /// Builder for creating test `Contract` instances with custom configuration.
    ///
    /// Provides a fluent interface for configuring contract state before tests.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let contract = ContractBuilder::new("owner.test", "usdc.test")
    ///     .total_assets(1_000_000)
    ///     .predecessor("solver.test")
    ///     .attached(1)
    ///     .build();
    /// ```
    pub struct ContractBuilder {
        owner: String,
        asset: String,
        extra: u8,
        total_assets: u128,
        supply: u128,
        predecessor: Option<String>,
        attached: u128,
    }

    impl ContractBuilder {
        /// Creates a new builder with required owner and asset accounts.
        ///
        /// # Arguments
        ///
        /// * `owner` - The contract owner account ID
        /// * `asset` - The underlying asset token account ID
        pub fn new(owner: &str, asset: &str) -> Self {
            Self {
                owner: owner.to_string(),
                asset: asset.to_string(),
                extra: 3,
                total_assets: 0,
                supply: 0,
                predecessor: Some(owner.to_string()),
                attached: 0,
            }
        }

        /// Sets the extra decimals for share precision.
        pub fn extra_decimals(mut self, n: u8) -> Self {
            self.extra = n;
            self
        }

        /// Sets the initial total assets in the vault.
        pub fn total_assets(mut self, n: u128) -> Self {
            self.total_assets = n;
            self
        }

        /// Sets the initial share supply.
        pub fn supply(mut self, n: u128) -> Self {
            self.supply = n;
            self
        }

        /// Sets the predecessor (caller) account for subsequent calls.
        pub fn predecessor(mut self, id: &str) -> Self {
            self.predecessor = Some(id.to_string());
            self
        }

        /// Sets the attached deposit in yoctoNEAR.
        pub fn attached(mut self, yocto: u128) -> Self {
            self.attached = yocto;
            self
        }

        /// Builds and returns the configured `Contract` instance.
        pub fn build(self) -> Contract {
            if let Some(p) = &self.predecessor {
                init_ctx(p, self.attached);
            }
            let meta = FungibleTokenMetadata {
                spec: "ft-1.0.0".into(),
                name: "USDC Vault Shares".into(),
                symbol: "vUSDC".into(),
                icon: None,
                reference: None,
                reference_hash: None,
                decimals: 24,
            };
            let mut c = Contract::init(
                self.owner.parse().unwrap(),
                self.asset.parse().unwrap(),
                meta,
                self.extra,
                1, // 1% solver fee
            );
            if self.supply > 0 {
                c.token
                    .internal_deposit(&self.owner.parse().unwrap(), self.supply);
            }
            c.total_assets = self.total_assets;
            c
        }
    }
}
