// Common helper functions for sandbox tests

use near_api::{
    signer, Account, AccountId, Contract, NearToken, NetworkConfig, RPCEndpoint, Signer,
};
use near_api::near_primitives::views::FinalExecutionStatus;
use near_sandbox::{GenesisAccount, Sandbox};
use serde_json::json;
use std::sync::Arc;

pub mod test_builder;
pub use test_builder::*;

pub const CONTRACT_WASM_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/target/near/contract.wasm");
pub const MOCK_FT_WASM_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../mock_ft/target/near/mock_ft.wasm");
pub const EXTRA_DECIMALS: u8 = 3; // Multiplier for first deposit: 10^3 = 1000
#[allow(dead_code)]
pub const SOLVER_BORROW_AMOUNT: u128 = 5_000_000; // 5 USDC with 6 decimals

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

pub fn create_network_config(sandbox: &Sandbox) -> NetworkConfig {
    NetworkConfig {
        network_name: "sandbox".to_string(),
        rpc_endpoints: vec![RPCEndpoint::new(sandbox.rpc_addr.parse().unwrap())],
        ..NetworkConfig::testnet()
    }
}

pub async fn setup_genesis_account() -> (AccountId, Arc<Signer>) {
    let genesis_account_default = GenesisAccount::default();
    let genesis_account_id: AccountId = genesis_account_default.account_id;
    let genesis_signer: Arc<Signer> = Signer::new(Signer::from_secret_key(
        genesis_account_default.private_key.parse().unwrap(),
    ))
    .unwrap();

    (genesis_account_id, genesis_signer)
}

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

    // Read mock FT WASM
    let wasm_bytes = std::fs::read(MOCK_FT_WASM_PATH)?;
    
    // Deploy mock FT contract
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

pub async fn deploy_vault_contract(
    network_config: &NetworkConfig,
    genesis_account_id: &AccountId,
    genesis_signer: &Arc<Signer>,
) -> Result<AccountId, Box<dyn std::error::Error + Send + Sync>> {
    // First, deploy mock FT contract with initial supply
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

    // Read vault contract WASM
    let wasm_bytes = std::fs::read(CONTRACT_WASM_PATH)?;
    
    // Deploy vault contract
    let contract_signer: Arc<Signer> = Signer::new(Signer::from_secret_key(contract_secret_key)).unwrap();
    
    // Deploy with init in a single transaction (original flow)
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
        "extra_decimals": EXTRA_DECIMALS
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

