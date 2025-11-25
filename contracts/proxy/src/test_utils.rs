#[cfg(test)]
pub mod helpers {
    use crate::Contract;
    use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::{testing_env, NearToken};

    pub fn init_ctx(predecessor: &str, deposit_yocto: u128) {
        let mut builder = VMContextBuilder::new();
        builder
            .predecessor_account_id(predecessor.parse().unwrap())
            .attached_deposit(NearToken::from_yoctonear(deposit_yocto));
        testing_env!(builder.build());
    }

    pub fn init_contract(owner: &str, asset: &str) -> Contract {
        init_contract_ex(owner, asset, 3)
    }

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
        )
    }
}

#[cfg(test)]
pub mod builders {
    use crate::test_utils::helpers::init_ctx;
    use crate::Contract;
    use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;

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

        pub fn extra_decimals(mut self, n: u8) -> Self {
            self.extra = n;
            self
        }
        pub fn total_assets(mut self, n: u128) -> Self {
            self.total_assets = n;
            self
        }
        pub fn supply(mut self, n: u128) -> Self {
            self.supply = n;
            self
        }
        pub fn predecessor(mut self, id: &str) -> Self {
            self.predecessor = Some(id.to_string());
            self
        }
        pub fn attached(mut self, yocto: u128) -> Self {
            self.attached = yocto;
            self
        }

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
