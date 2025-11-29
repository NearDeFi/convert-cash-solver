// Test builder pattern for integration tests
// This builder helps simplify the creation of complex test scenarios
//
// Note: #![allow(dead_code)] is used because not all helper functions are used
// by every test file, but they are available for use across all integration tests.

#![allow(dead_code)]

use near_api::{Contract, Data, NearToken, NetworkConfig};
use near_api::AccountId;
use near_sandbox::Sandbox;
use serde_json::json;
use tokio::time::{sleep, Duration};
use std::sync::Arc;

use super::*;

pub struct TestScenarioBuilder {
    _sandbox: Sandbox, // Keep sandbox alive for the entire test duration
    network_config: NetworkConfig,
    genesis_account_id: AccountId,
    genesis_signer: Arc<Signer>,
    vault_id: Option<AccountId>,
    ft_id: Option<AccountId>,
    ft_contract: Option<Contract>,
    vault_contract: Option<Contract>,
    accounts: Vec<(AccountId, Arc<Signer>, String)>, // (account_id, signer, name)
}

impl TestScenarioBuilder {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
        let network_config = create_network_config(&sandbox);
        let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

        Ok(Self {
            _sandbox: sandbox, // Keep sandbox alive
            network_config,
            genesis_account_id,
            genesis_signer,
            vault_id: None,
            ft_id: None,
            ft_contract: None,
            vault_contract: None,
            accounts: Vec::new(),
        })
    }

    pub async fn deploy_vault(mut self) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let vault_id = deploy_vault_contract(&self.network_config, &self.genesis_account_id, &self.genesis_signer).await?;
        let ft_id: AccountId = format!("usdc.{}", self.genesis_account_id).parse()?;
        
        let ft_contract = Contract(ft_id.clone());
        let vault_contract = Contract(vault_id.clone());

        self.vault_id = Some(vault_id);
        self.ft_id = Some(ft_id);
        self.ft_contract = Some(ft_contract);
        self.vault_contract = Some(vault_contract);

        Ok(self)
    }

    pub async fn create_account(mut self, name: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (account_id, signer) = create_user_account(
            &self.network_config,
            &self.genesis_account_id,
            &self.genesis_signer,
            name,
        ).await?;
        
        self.accounts.push((account_id, signer, name.to_string()));
        Ok(self)
    }

    pub async fn register_accounts(self) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let ft_contract = self.ft_contract.as_ref().unwrap();
        let vault_contract = self.vault_contract.as_ref().unwrap();

        // Register all accounts with FT contract
        for (account_id, _signer, _) in &self.accounts {
            ft_contract
                .call_function("storage_deposit", json!({ "account_id": account_id }))?
                .transaction()
                .deposit(NearToken::from_millinear(10))
                .with_signer(self.genesis_account_id.clone(), self.genesis_signer.clone())
                .send_to(&self.network_config)
                .await?;
        }

        // Register lenders with vault (all accounts except solver)
        for (account_id, signer, name) in &self.accounts {
            if name != "solver" {
                vault_contract
                    .call_function("storage_deposit", json!({ "account_id": account_id }))?
                    .transaction()
                    .deposit(NearToken::from_millinear(10))
                    .with_signer(account_id.clone(), signer.clone())
                    .send_to(&self.network_config)
                    .await?;
            }
        }

        // Register solver with vault
        if let Some((solver_id, solver_signer, _)) = self.accounts.iter().find(|(_, _, name)| name == "solver") {
            vault_contract
                .call_function("storage_deposit", json!({ "account_id": solver_id }))?
                .transaction()
                .deposit(NearToken::from_millinear(10))
                .with_signer(solver_id.clone(), solver_signer.clone())
                .send_to(&self.network_config)
                .await?;
        }

        Ok(self)
    }

    pub fn get_account(&self, name: &str) -> Option<&(AccountId, Arc<Signer>, String)> {
        self.accounts.iter().find(|(_, _, n)| n == name)
    }

    pub fn ft_contract(&self) -> &Contract {
        self.ft_contract.as_ref().unwrap()
    }

    pub fn vault_contract(&self) -> &Contract {
        self.vault_contract.as_ref().unwrap()
    }

    pub fn vault_id(&self) -> &AccountId {
        self.vault_id.as_ref().unwrap()
    }

    pub fn network_config(&self) -> &NetworkConfig {
        &self.network_config
    }

    pub fn genesis_account_id(&self) -> &AccountId {
        &self.genesis_account_id
    }

    pub fn genesis_signer(&self) -> &Arc<Signer> {
        &self.genesis_signer
    }
}

// Helper functions for common operations
pub async fn deposit_to_vault(
    builder: &TestScenarioBuilder,
    lender_name: &str,
    amount: u128,
) -> Result<u128, Box<dyn std::error::Error + Send + Sync>> {
    let (lender_id, lender_signer, _) = builder.get_account(lender_name)
        .ok_or_else(|| format!("Account {} not found", lender_name))?;
    
    let ft_contract = builder.ft_contract();
    let vault_id = builder.vault_id();
    let genesis_account_id = builder.genesis_account_id();
    let genesis_signer = builder.genesis_signer();
    let network_config = builder.network_config();

    // Transfer tokens to lender
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": lender_id,
            "amount": amount.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(network_config)
        .await?;

    // Deposit to vault (using simple format like test_half_redemptions.rs)
    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": amount.to_string(),
            "msg": json!({ "receiver_id": lender_id }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender_id.clone(), lender_signer.clone())
        .send_to(network_config)
        .await?;

    sleep(Duration::from_millis(1200)).await;

    // Get shares received
    let vault_contract = builder.vault_contract();
    let shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender_id }))?
        .read_only()
        .fetch_from(network_config)
        .await?;
    
    Ok(shares.data.parse::<u128>().unwrap())
}

pub async fn solver_borrow(
    builder: &TestScenarioBuilder,
    amount: Option<u128>,
    intent_hash: &str,
) -> Result<u128, Box<dyn std::error::Error + Send + Sync>> {
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();

    let borrow_amount = amount.unwrap_or(SOLVER_BORROW_AMOUNT);
    
    let mut intent_params = json!({
        "intent_data": format!("intent-{}", intent_hash),
        "_solver_deposit_address": solver_id,
        "user_deposit_hash": intent_hash
    });

    if let Some(amt) = amount {
        intent_params["amount"] = json!(amt.to_string());
    }

    vault_contract
        .call_function("new_intent", intent_params)?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(network_config)
        .await?;

    sleep(Duration::from_millis(1200)).await;

    Ok(borrow_amount)
}

pub async fn solver_repay(
    builder: &TestScenarioBuilder,
    intent_index: u128,
    principal: u128,
    intent_yield: u128,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
    let ft_contract = builder.ft_contract();
    let vault_id = builder.vault_id();
    let genesis_account_id = builder.genesis_account_id();
    let genesis_signer = builder.genesis_signer();
    let network_config = builder.network_config();

    // Transfer intent_yield to solver
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": solver_id,
            "amount": intent_yield.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(network_config)
        .await?;

    sleep(Duration::from_millis(500)).await;

    // Solver repays
    let total_repayment = principal + intent_yield;
    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": total_repayment.to_string(),
            "msg": json!({ "repay": { "intent_index": intent_index.to_string() } }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(network_config)
        .await?;

    sleep(Duration::from_millis(2000)).await;

    Ok(())
}

pub async fn redeem_shares(
    builder: &TestScenarioBuilder,
    lender_name: &str,
    shares: u128,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (lender_id, lender_signer, _) = builder.get_account(lender_name)
        .ok_or_else(|| format!("Account {} not found", lender_name))?;
    
    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();

    vault_contract
        .call_function("redeem", json!({
            "shares": shares.to_string(),
            "receiver_id": lender_id,
            "memo": null
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender_id.clone(), lender_signer.clone())
        .send_to(network_config)
        .await?;

    sleep(Duration::from_millis(1200)).await;

    Ok(())
}

pub async fn process_redemption_queue(
    builder: &TestScenarioBuilder,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();

    loop {
        let queue_length: Data<String> = vault_contract
            .call_function("get_pending_redemptions_length", json!([]))?
            .read_only()
            .fetch_from(network_config)
            .await?;
        let queue_length_u32 = queue_length.data.parse::<u128>().unwrap() as u32;
        
        if queue_length_u32 == 0 {
            println!("Redemption queue is empty, stopping processing");
            break;
        }
        
        println!("Processing next redemption from queue (queue length: {})", queue_length_u32);
        vault_contract
            .call_function("process_next_redemption", json!([]))?
            .transaction()
            .with_signer(solver_id.clone(), solver_signer.clone())
            .send_to(network_config)
            .await?;
        
        sleep(Duration::from_millis(2000)).await;
        
        // Check if processing succeeded (like test_half_redemptions.rs)
        let queue_length_after: Data<String> = vault_contract
            .call_function("get_pending_redemptions_length", json!([]))?
            .read_only()
            .fetch_from(network_config)
            .await?;
        let queue_length_after_u32 = queue_length_after.data.parse::<u128>().unwrap() as u32;
        
        if queue_length_after_u32 == queue_length_u32 {
            println!("Queue didn't advance, stopping");
            break;
        }
    }

    Ok(())
}

pub async fn get_balance(
    builder: &TestScenarioBuilder,
    account_name: &str,
) -> Result<u128, Box<dyn std::error::Error + Send + Sync>> {
    let (account_id, _, _) = builder.get_account(account_name)
        .ok_or_else(|| format!("Account {} not found", account_name))?;
    
    let ft_contract = builder.ft_contract();
    let network_config = builder.network_config();

    let balance: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": account_id }))?
        .read_only()
        .fetch_from(network_config)
        .await?;
    
    Ok(balance.data.parse::<u128>().unwrap())
}

pub async fn get_shares(
    builder: &TestScenarioBuilder,
    account_name: &str,
) -> Result<u128, Box<dyn std::error::Error + Send + Sync>> {
    let (account_id, _, _) = builder.get_account(account_name)
        .ok_or_else(|| format!("Account {} not found", account_name))?;
    
    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();

    let shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": account_id }))?
        .read_only()
        .fetch_from(network_config)
        .await?;
    
    Ok(shares.data.parse::<u128>().unwrap())
}

pub async fn get_total_assets(
    builder: &TestScenarioBuilder,
) -> Result<u128, Box<dyn std::error::Error + Send + Sync>> {
    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();

    let total_assets: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(network_config)
        .await?;
    
    Ok(total_assets.data.parse::<u128>().unwrap())
}

/// Calculate expected shares for a deposit based on current vault state
/// Formula: shares = (assets * total_supply) / (total_assets + total_borrowed + expected_yield)
/// where expected_yield = sum of (borrow_amount / 100) for all active borrows
pub async fn calculate_expected_shares_for_deposit(
    builder: &TestScenarioBuilder,
    deposit_amount: u128,
) -> Result<u128, Box<dyn std::error::Error + Send + Sync>> {
    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();

    // Get current vault state
    let total_assets = get_total_assets(builder).await?;
    let total_supply: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(network_config)
        .await?;
    let total_supply_u128 = total_supply.data.parse::<u128>().unwrap();

    // If vault is empty, first deposit uses extra_decimals multiplier
    if total_supply_u128 == 0 {
        return Ok(deposit_amount * 1000u128); // extra_decimals = 3, so 10^3 = 1000
    }

    // Get all intents to calculate total_borrowed and expected_yield
    let intents: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_intents", json!([]))?
        .read_only()
        .fetch_from(network_config)
        .await?;

    let mut total_borrowed = 0u128;
    let mut expected_yield = 0u128;

    for intent in intents.data {
        // Only count active borrows (StpLiquidityBorrowed state)
        // State is serialized as string in JSON
        let state_str = intent["state"].as_str().unwrap_or("");
        
        if state_str == "StpLiquidityBorrowed" {
            // borrow_amount is serialized as string for large numbers in NEAR
            let borrow_amount = if let Some(amt_str) = intent["borrow_amount"].as_str() {
                amt_str.parse::<u128>().unwrap_or(0)
            } else if let Some(amt_num) = intent["borrow_amount"].as_u64() {
                amt_num as u128
            } else {
                0
            };
            
            total_borrowed += borrow_amount;
            expected_yield += borrow_amount / 100; // 1% intent_yield
        }
    }

    // Calculate denominator: total_assets + total_borrowed + expected_yield
    let denominator = total_assets
        .checked_add(total_borrowed)
        .unwrap_or(u128::MAX)
        .checked_add(expected_yield)
        .unwrap_or(u128::MAX)
        .max(1);

    // Calculate shares: (deposit_amount * total_supply) / denominator
    // Using integer division with rounding down (same as mul_div with Rounding::Down)
    let shares = (deposit_amount as u128)
        .checked_mul(total_supply_u128)
        .unwrap_or(0)
        .checked_div(denominator)
        .unwrap_or(0);

    Ok(shares)
}

