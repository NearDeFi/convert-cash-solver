use crate::*;
use near_contract_standards::fungible_token::core::ext_ft_core;
use near_sdk::Gas;

const GAS_FOR_OMFT_WITHDRAW: Gas = Gas::from_tgas(30);

#[near]
impl Contract {

    /// Burns OMFT on NEAR and withdraws to an EVM address controlled by the pool.
    /// The OMFT bridge recognizes the memo "WITHDRAW_TO:<0x...>" when receiver_id is the token contract itself.
    #[payable]
    pub fn withdraw_omft_to_evm(
        &mut self,
        token_contract: AccountId,
        amount: U128,
        evm_address: String,
    ) -> Promise {
        // Access control and anti-CSRF
        self.require_owner();
        near_sdk::assert_one_yocto();

        // Basic input validations
        require!(amount.0 > 0, "amount must be > 0");
        require!(
            token_contract == self.asset,
            "token_contract must match vault asset"
        );
        // Ensure we are not attempting to move more than managed assets
        require!(
            amount.0 <= self.total_assets,
            "amount exceeds available assets"
        );

        // Normalize and validate EVM address
        let evm = evm_address.trim().to_string();
        // Basic format validation for 0x... address (length check); bridge will enforce strictly.
        require!(
            evm.starts_with("0x")
                && evm.len() == 42
                && evm
                    .chars()
                    .skip(2)
                    .all(|c| c.is_ascii_hexdigit()),
            "invalid EVM address format"
        );

        let memo = format!("WITHDRAW_TO:{}", evm);

        // Send ft_transfer to the OMFT contract itself with 1 yocto deposit
        ext_ft_core::ext(token_contract.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_OMFT_WITHDRAW)
            .ft_transfer(token_contract, amount, Some(memo))
    }

    /// Burns OMFT on NEAR and withdraws to a Solana address (base58).
    /// The OMFT bridge recognizes the memo "WITHDRAW_TO:<solana_address>" when receiver_id is the token contract itself.
    #[payable]
    pub fn withdraw_omft_to_solana(
        &mut self,
        token_contract: AccountId,
        amount: U128,
        sol_address: String,
    ) -> Promise {
        // Access control and anti-CSRF
        self.require_owner();
        near_sdk::assert_one_yocto();

        // Basic input validations
        require!(amount.0 > 0, "amount must be > 0");
        require!(
            token_contract == self.asset,
            "token_contract must match vault asset"
        );
        // Ensure we are not attempting to move more than managed assets
        require!(
            amount.0 <= self.total_assets,
            "amount exceeds available assets"
        );
        // Minimal sanity checks; actual validation is enforced by the bridge/relayer.
        let sol = sol_address.trim().to_string();
        // Length bounds
        require!(
            sol.len() >= 32 && sol.len() <= 64,
            "invalid Solana address length"
        );
        // Base58 charset (no 0, O, I, l)
        let is_base58 = sol.chars().all(|c| {
            matches!(c,
                '1'..='9'
                | 'A'..='H' | 'J'..='N' | 'P'..='Z'
                | 'a'..='k' | 'm'..='z'
            )
        });
        require!(is_base58, "invalid Solana address characters");

        let memo = format!("WITHDRAW_TO:{}", sol);

        ext_ft_core::ext(token_contract.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_OMFT_WITHDRAW)
            .ft_transfer(token_contract, amount, Some(memo))
    }
}