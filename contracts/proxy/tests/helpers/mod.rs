//! # Test Helpers Module
//!
//! Provides common infrastructure for NEAR sandbox integration tests.
//! These helpers abstract away boilerplate for deploying contracts, creating
//! accounts, and performing common operations.
//!
//! ## Modules
//!
//! - [`test_builder`]: Builder pattern for constructing complex test scenarios
//!
//! ## Key Functions
//!
//! - [`deploy_mock_ft`]: Deploys a mock NEP-141 fungible token
//! - [`deploy_vault_contract`]: Deploys the vault contract with mock FT
//! - [`create_user_account`]: Creates funded test accounts
//! - [`create_network_config`]: Configures connection to sandbox

use near_api::{
    signer, Account, AccountId, Contract, NearToken, NetworkConfig, RPCEndpoint, Signer,
};
use near_api::near_primitives::views::FinalExecutionStatus;
use near_sandbox::{GenesisAccount, Sandbox};
use serde_json::json;
use std::sync::Arc;

pub mod test_builder;

// ============================================================================
// Constants
// ============================================================================

/// Path to the compiled vault contract WASM.
pub const CONTRACT_WASM_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/target/near/contract.wasm");

/// Path to the compiled mock FT contract WASM.
pub const MOCK_FT_WASM_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../mock_ft/target/near/mock_ft.wasm");

/// Extra decimals for vault share precision (10^3 = 1000 multiplier).
pub const EXTRA_DECIMALS: u8 = 3;

/// Default solver borrow amount (5 USDC with 6 decimals).
#[allow(dead_code)]
pub const SOLVER_BORROW_AMOUNT: u128 = 5_000_000;

// ============================================================================
// Helper Functions
// ============================================================================

/// Validates that a transaction execution status indicates success.
///
/// # Arguments
///
/// * `status` - The execution status to check
/// * `context` - Description for error messages
///
/// # Returns
///
/// `Ok(())` if successful, `Err` with context message otherwise.
fn ensure_success_status(
    status: &FinalExecutionStatus,
    context: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match status {
        FinalExecutionStatus::SuccessValue(_) => Ok(()),
        FinalExecutionStatus::Failure(err) => {
            Err(format!("{context} failed with execution error: {:?}", err).into())
        }
        other => Err(format!("{context} returned unexpected status: {:?}", other).into()),
    }
}

/// Creates a network configuration for connecting to the sandbox.
///
/// # Arguments
///
/// * `sandbox` - The running sandbox instance
///
/// # Returns
///
/// A `NetworkConfig` configured for the sandbox RPC endpoint.
pub fn create_network_config(sandbox: &Sandbox) -> NetworkConfig {
    NetworkConfig {
        network_name: "sandbox".to_string(),
        rpc_endpoints: vec![RPCEndpoint::new(sandbox.rpc_addr.parse().unwrap())],
        ..NetworkConfig::testnet()
    }
}

/// Retrieves the genesis account credentials from the sandbox.
///
/// The genesis account has the initial NEAR balance and is used to
/// fund other accounts and deploy contracts.
///
/// # Returns
///
/// A tuple of (account_id, signer) for the genesis account.
pub async fn setup_genesis_account() -> (AccountId, Arc<Signer>) {
    let genesis_account_default = GenesisAccount::default();
    let genesis_account_id: AccountId = genesis_account_default.account_id;
    let genesis_signer: Arc<Signer> = Signer::new(Signer::from_secret_key(
        genesis_account_default.private_key.parse().unwrap(),
    ))
    .unwrap();

    (genesis_account_id, genesis_signer)
}

/// Deploys a mock NEP-141 fungible token contract.
///
/// Creates a new account for the token and deploys the mock FT WASM.
/// The token is initialized with the specified total supply.
///
/// # Arguments
///
/// * `network_config` - Network connection configuration
/// * `genesis_account_id` - Account to fund the FT account
/// * `genesis_signer` - Signer for the genesis account
/// * `total_supply` - Initial token supply as a string
///
/// # Returns
///
/// The account ID of the deployed FT contract.
#[allow(dead_code)]
pub async fn deploy_mock_ft(
    network_config: &NetworkConfig,
    genesis_account_id: &AccountId,
    genesis_signer: &Arc<Signer>,
    total_supply: &str,
) -> Result<AccountId, Box<dyn std::error::Error + Send + Sync>> {
    // Create mock FT account
    let ft_id: AccountId = format!("usdc.{}", genesis_account_id).parse()?;
    let ft_secret_key = signer::generate_secret_key()?;

    Account::create_account(ft_id.clone())
        .fund_myself(genesis_account_id.clone(), NearToken::from_near(10))
        .public_key(ft_secret_key.public_key())
        .unwrap()
        .with_signer(genesis_signer.clone())
        .send_to(network_config)
        .await?;

    println!("Mock FT account created: {}", ft_id);

    // Read and deploy mock FT WASM
    let wasm_bytes = std::fs::read(MOCK_FT_WASM_PATH)?;
    let ft_signer: Arc<Signer> = Signer::new(Signer::from_secret_key(ft_secret_key)).unwrap();
    
    let deploy_res = Contract::deploy(ft_id.clone())
        .use_code(wasm_bytes)
        .with_init_call("new", json!({
            "owner_id": genesis_account_id,
            "total_supply": total_supply,
            "metadata": {
                "spec": "ft-1.0.0",
                "name": "Mock USDC",
                "symbol": "USDC",
                "icon": null,
                "reference": null,
                "reference_hash": null,
                "decimals": 6
            }
        }))?
        .with_signer(ft_signer)
        .send_to(network_config)
        .await.unwrap();

    ensure_success_status(&deploy_res.status, "Mock FT deploy")?;
    println!("Mock FT deployed with total_supply: {}", total_supply);

    Ok(ft_id)
}

/// Deploys the vault contract with a mock FT as the underlying asset.
///
/// This function:
/// 1. Deploys a mock USDC token with 1M supply
/// 2. Creates and deploys the vault contract
/// 3. Registers the vault with the FT for storage
///
/// # Arguments
///
/// * `network_config` - Network connection configuration
/// * `genesis_account_id` - Account to own the contracts
/// * `genesis_signer` - Signer for the genesis account
///
/// # Returns
///
/// The account ID of the deployed vault contract.
pub async fn deploy_vault_contract(
    network_config: &NetworkConfig,
    genesis_account_id: &AccountId,
    genesis_signer: &Arc<Signer>,
) -> Result<AccountId, Box<dyn std::error::Error + Send + Sync>> {
    // Deploy mock FT with initial supply
    let total_supply = "1000000000000"; // 1 million USDC (6 decimals)
    let asset_id = deploy_mock_ft(network_config, genesis_account_id, genesis_signer, total_supply).await?;
    
    // Create vault contract account
    let contract_id: AccountId = format!("vault.{}", genesis_account_id).parse()?;
    let contract_secret_key = signer::generate_secret_key()?;

    Account::create_account(contract_id.clone())
        .fund_myself(genesis_account_id.clone(), NearToken::from_near(10))
        .public_key(contract_secret_key.public_key())
        .unwrap()
        .with_signer(genesis_signer.clone())
        .send_to(network_config)
        .await?;

    println!("Vault contract account created: {}", contract_id);

    // Read and deploy vault WASM
    let wasm_bytes = std::fs::read(CONTRACT_WASM_PATH)?;
    let contract_signer: Arc<Signer> = Signer::new(Signer::from_secret_key(contract_secret_key)).unwrap();
    
    let init_args = json!({
        "owner_id": genesis_account_id,
        "asset": asset_id,
        "metadata": {
            "spec": "ft-1.0.0",
            "name": "USDC Vault Shares",
            "symbol": "vUSDC",
            "icon": null,
            "reference": null,
            "reference_hash": null,
            "decimals": 24
        },
        "extra_decimals": EXTRA_DECIMALS,
        "solver_fee": 1
    });
    println!("Deploying vault with init args: {}", init_args);

    let deploy_res = Contract::deploy(contract_id.clone())
        .use_code(wasm_bytes)
        .with_init_call("init", init_args)?
        .with_signer(contract_signer.clone())
        .send_to(network_config)
        .await?;

    ensure_success_status(&deploy_res.status, "Vault deploy/init")?;
    println!("Vault deploy/init status: {:?}", deploy_res.status);
    println!("Vault contract deployed and initialized with asset: {}", asset_id);

    // Register vault with the FT contract for storage
    let ft_contract = Contract(asset_id.clone());
    ft_contract
        .call_function("storage_deposit", json!({
            "account_id": contract_id
        }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(network_config)
        .await?;

    println!("Vault registered with FT contract for storage");

    Ok(contract_id)
}

/// Creates a new user account funded with NEAR.
///
/// # Arguments
///
/// * `network_config` - Network connection configuration
/// * `genesis_account_id` - Account to fund the new account
/// * `genesis_signer` - Signer for the genesis account
/// * `user_name` - Name prefix for the account (e.g., "alice" -> "alice.{genesis}")
///
/// # Returns
///
/// A tuple of (account_id, signer) for the new account.
#[allow(dead_code)]
pub async fn create_user_account(
    network_config: &NetworkConfig,
    genesis_account_id: &AccountId,
    genesis_signer: &Arc<Signer>,
    user_name: &str,
) -> Result<(AccountId, Arc<Signer>), Box<dyn std::error::Error + Send + Sync>> {
    let user_id: AccountId = format!("{}.{}", user_name, genesis_account_id).parse()?;
    let user_secret_key = signer::generate_secret_key()?;
    let user_signer: Arc<Signer> = Signer::new(Signer::from_secret_key(user_secret_key.clone())).unwrap();

    Account::create_account(user_id.clone())
        .fund_myself(genesis_account_id.clone(), NearToken::from_near(5))
        .public_key(user_secret_key.public_key())
        .unwrap()
        .with_signer(genesis_signer.clone())
        .send_to(network_config)
        .await?;

    println!("User account created: {}", user_id);

    Ok((user_id, user_signer))
}
