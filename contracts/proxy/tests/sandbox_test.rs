use near_api::{signer, Account, AccountId, Contract, Data, NearToken, NetworkConfig, RPCEndpoint, Signer};
use near_sandbox::{GenesisAccount, Sandbox};
use serde_json::json;
use std::sync::Arc;

const CONTRACT_WASM_PATH: &str = "./target/near/contract.wasm";
const EXTRA_DECIMALS: u8 = 3; // Multiplier for first deposit: 10^3 = 1000

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
async fn test_vault_initialization() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox
    let sandbox = Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);

    // Setup genesis account
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // Deploy contract using helper (with extra_decimals = 3)
    let contract_id = deploy_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    
    let vault_contract = Contract(contract_id.clone());

    // Test: Get vault metadata (FT metadata for vault shares)
    // Note: ft_metadata returns a struct, so we use Data<serde_json::Value> for flexibility
    let metadata: Data<serde_json::Value> = vault_contract
        .call_function("ft_metadata", json!([]))? 
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Vault share metadata: {:?}", metadata.data);
    
    assert_eq!(metadata.data["name"], "USDC Vault Shares");
    assert_eq!(metadata.data["symbol"], "vUSDC");
    assert_eq!(metadata.data["decimals"], 24);

    // Test: Get underlying asset
    let asset: Data<String> = vault_contract
        .call_function("asset", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Underlying asset: {}", asset.data);
    assert!(asset.data.starts_with("usdc."), "Asset should be usdc token");

    // Test: Get total assets (should be 0 initially)
    let total_assets: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Total assets: {}", total_assets.data);
    assert_eq!(total_assets.data, "0");

    // Test: Get total supply of shares (should be 0 initially)
    let total_supply: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Total supply of vault shares: {}", total_supply.data);
    assert_eq!(total_supply.data, "0");

    println!("✅ Vault initialization test passed!");
    println!("   - Vault deployed at: {}", contract_id);
    println!("   - Underlying asset: {}", asset.data);
    println!("   - Share token: vUSDC");
    println!("   - Extra decimals: {} (multiplier: 10^{} = {})", EXTRA_DECIMALS, EXTRA_DECIMALS, 10u128.pow(EXTRA_DECIMALS as u32));
    println!("   - Initial total assets: 0");
    println!("   - Initial share supply: 0");

    Ok(())
}

#[tokio::test]
async fn test_vault_conversion_functions() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox
    let sandbox = Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);

    // Setup genesis account
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // Deploy contract with vault parameters
    let contract_id = deploy_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    
    let contract = Contract(contract_id.clone());

    // Test: convert_to_shares for empty vault with extra_decimals = 3
    let assets_to_convert = "1000000000000000000000000"; // 1 token with 24 decimals
    let shares: Data<String> = contract
        .call_function("preview_deposit", json!({
            "assets": assets_to_convert
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Assets {} converts to shares: {}", assets_to_convert, shares.data);
    
    // For empty vault with extra_decimals, multiply by 10^EXTRA_DECIMALS
    let multiplier = 10u128.pow(EXTRA_DECIMALS as u32);
    let expected_shares = (assets_to_convert.parse::<u128>().unwrap() * multiplier).to_string();
    println!("Expected shares: {} (assets × {})", expected_shares, multiplier);
    assert_eq!(shares.data, expected_shares, "Should multiply by 10^{} = {}", EXTRA_DECIMALS, multiplier);

    // Test: convert_to_assets (reverse conversion)
    // Note: For empty vault, this might panic or return 0 depending on implementation
    // Let's test preview_withdraw instead
    let preview_shares: Data<String> = contract
        .call_function("preview_withdraw", json!({
            "assets": "1000000000000000000000000"
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Preview withdraw shares needed: {}", preview_shares.data);

    println!("✅ Vault conversion test passed!");
    println!("   - Using extra_decimals = {} (from EXTRA_DECIMALS constant)", EXTRA_DECIMALS);
    println!("   - Multiplier for first deposit: {}", 10u128.pow(EXTRA_DECIMALS as u32));

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
    
    // Create a test asset account ID (in production, this would be a real FT contract)
    let asset_id: AccountId = format!("usdc.{}", genesis_account_id).parse()?;
    
    Contract::deploy(contract_id.clone())
        .use_code(wasm_bytes)
        .with_init_call("init", json!({
            "owner_id": genesis_account_id,
            "asset": asset_id,
            "metadata": {
                "spec": "ft-1.0.0",
                "name": "USDC Vault Shares",
                "symbol": "vUSDC",
                "decimals": 24
            },
            "extra_decimals": EXTRA_DECIMALS
        }))?
        .with_signer(contract_signer)
        .send_to(network_config)
        .await?;

    println!("Contract deployed and initialized");

    Ok(contract_id)
}

