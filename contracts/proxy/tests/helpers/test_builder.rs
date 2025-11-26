// Test builder pattern for integration tests
// This builder helps simplify the creation of complex test scenarios

use near_api::{Contract, Data, NearToken, NetworkConfig};
use near_api::AccountId;
use serde_json::json;
use tokio::time::{sleep, Duration};
use std::sync::Arc;

use super::*;

pub struct TestScenarioBuilder {
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

    // Deposit to vault
    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": amount.to_string(),
            "msg": json!({ "deposit": { "receiver_id": lender_id } }).to_string()
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
    let vault_contract = builder.vault_contract();
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
            break;
        }
        
        vault_contract
            .call_function("process_next_redemption", json!([]))?
            .transaction()
            .with_signer(solver_id.clone(), solver_signer.clone())
            .send_to(network_config)
            .await?;
        
        sleep(Duration::from_millis(2000)).await;
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

