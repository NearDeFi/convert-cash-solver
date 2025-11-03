use near_api::{signer, Account, AccountId, Contract, Data, NearToken, NetworkConfig, RPCEndpoint, Signer};
use near_sandbox::{GenesisAccount, Sandbox};
use serde_json::json;
use std::sync::Arc;

const CONTRACT_WASM_PATH: &str = "./target/near/contract.wasm";

#[tokio::test]
async fn test_contract_deployment() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox
    let sandbox = Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);

    // Setup genesis account
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // Deploy contract
    let contract_id = deploy_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    
    println!("Contract deployed to: {}", contract_id);
    
    Ok(())
}

#[tokio::test]
async fn test_register_agent() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox
    let sandbox = Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);

    // Setup genesis account
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // Deploy and initialize contract
    let contract_id = deploy_contract(&network_config, &genesis_account_id, &genesis_signer).await?;

    // Create a worker account
    let worker_account_id: AccountId = format!("worker.{}", genesis_account_id).parse()?;
    let worker_secret_key = signer::generate_secret_key()?;
    let worker_signer: Arc<Signer> = Signer::new(Signer::from_secret_key(worker_secret_key.clone())).unwrap();

    Account::create_account(worker_account_id.clone())
        .fund_myself(genesis_account_id.clone(), NearToken::from_near(5))
        .public_key(worker_secret_key.public_key())
        .unwrap()
        .with_signer(genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    println!("Worker account created: {}", worker_account_id);

    // Register agent
    let codehash = "test_codehash_123".to_string();
    let contract = Contract(contract_id.clone());
    let result = contract
        .call_function("register_agent", json!({
            "codehash": codehash
        }))?
        .transaction()
        .with_signer(worker_account_id.clone(), worker_signer.clone())
        .send_to(&network_config)
        .await?;

    println!("Agent registered: {:?}", result.status);

    // Verify agent was registered
    let agent_info: Data<Vec<u8>> = contract
        .call_function("get_agent", json!({
            "account_id": worker_account_id
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Agent info: {:?}", agent_info);

    let agent_data: serde_json::Value = serde_json::from_slice(&agent_info.data)?;
    assert_eq!(agent_data["codehash"], codehash);

    Ok(())
}

#[tokio::test]
async fn test_approve_codehash() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox
    let sandbox = Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);

    // Setup genesis account (owner)
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // Deploy and initialize contract
    let contract_id = deploy_contract(&network_config, &genesis_account_id, &genesis_signer).await?;

    // Approve a codehash
    let codehash = "approved_codehash_456".to_string();
    let contract = Contract(contract_id.clone());
    let result = contract
        .call_function("approve_codehash", json!({
            "codehash": codehash
        }))?
        .transaction()
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    println!("Codehash approved: {:?}", result.status);

    Ok(())
}

#[tokio::test]
async fn test_full_workflow() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox
    let sandbox = Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);

    // Setup genesis account
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // Deploy and initialize contract
    let contract_id = deploy_contract(&network_config, &genesis_account_id, &genesis_signer).await?;

    // Create worker account
    let worker_account_id: AccountId = format!("worker.{}", genesis_account_id).parse()?;
    let worker_secret_key = signer::generate_secret_key()?;
    let worker_signer: Arc<Signer> = Signer::new(Signer::from_secret_key(worker_secret_key.clone())).unwrap();

    Account::create_account(worker_account_id.clone())
        .fund_myself(genesis_account_id.clone(), NearToken::from_near(5))
        .public_key(worker_secret_key.public_key())
        .unwrap()
        .with_signer(genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    // Approve codehash
    let codehash = "my_codehash".to_string();
    let contract = Contract(contract_id.clone());
    contract
        .call_function("approve_codehash", json!({
            "codehash": codehash
        }))?
        .transaction()
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    // Register agent with approved codehash
    contract
        .call_function("register_agent", json!({
            "codehash": codehash
        }))?
        .transaction()
        .with_signer(worker_account_id.clone(), worker_signer.clone())
        .send_to(&network_config)
        .await?;

    // Verify agent
    let agent_info: Data<Vec<u8>> = contract
        .call_function("get_agent", json!({
            "account_id": worker_account_id
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    let agent_data: serde_json::Value = serde_json::from_slice(&agent_info.data)?;
    assert_eq!(agent_data["codehash"], codehash);

    println!("Full workflow test passed!");

    Ok(())
}

// Helper functions

fn create_network_config(sandbox: &Sandbox) -> NetworkConfig {
    NetworkConfig {
        network_name: "sandbox".to_string(),
        rpc_endpoints: vec![RPCEndpoint::new(sandbox.rpc_addr.parse().unwrap())],
        ..NetworkConfig::testnet()
    }
}

async fn setup_genesis_account() -> (AccountId, Arc<Signer>) {
    let genesis_account_default = GenesisAccount::default();
    let genesis_account_id: AccountId = genesis_account_default.account_id;
    let genesis_signer: Arc<Signer> = Signer::new(Signer::from_secret_key(
        genesis_account_default.private_key.parse().unwrap(),
    ))
    .unwrap();

    (genesis_account_id, genesis_signer)
}

async fn deploy_contract(
    network_config: &NetworkConfig,
    genesis_account_id: &AccountId,
    genesis_signer: &Arc<Signer>,
) -> Result<AccountId, Box<dyn std::error::Error + Send + Sync>> {
    // Create contract account
    let contract_id: AccountId = format!("contract.{}", genesis_account_id).parse()?;
    let contract_secret_key = signer::generate_secret_key()?;

    Account::create_account(contract_id.clone())
        .fund_myself(genesis_account_id.clone(), NearToken::from_near(10))
        .public_key(contract_secret_key.public_key())
        .unwrap()
        .with_signer(genesis_signer.clone())
        .send_to(network_config)
        .await?;

    println!("Contract account created: {}", contract_id);

    // Read contract WASM
    let wasm_bytes = std::fs::read(CONTRACT_WASM_PATH)?;
    
    // Deploy contract
    let contract_signer: Arc<Signer> = Signer::new(Signer::from_secret_key(contract_secret_key)).unwrap();
    
    Contract::deploy(contract_id.clone())
        .use_code(wasm_bytes)
        .with_init_call("init", json!({
            "owner_id": genesis_account_id
        }))?
        .with_signer(contract_signer)
        .send_to(network_config)
        .await?;

    println!("Contract deployed and initialized");

    Ok(contract_id)
}

