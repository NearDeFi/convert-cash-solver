use near_api::{signer, Account, AccountId, Contract, Data, NearToken, NetworkConfig, RPCEndpoint, Signer};
use near_sandbox::{GenesisAccount, Sandbox};
use serde_json::json;
use std::sync::Arc;

const CONTRACT_WASM_PATH: &str = "./target/near/contract.wasm";
const MOCK_FT_WASM_PATH: &str = "../mock_ft/target/near/mock_ft.wasm";
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

#[tokio::test]
async fn test_vault_deposit_and_receive_shares() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox
    let sandbox = Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);

    // Setup genesis account
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // Deploy vault (which also deploys mock FT)
    let vault_id = deploy_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: AccountId = format!("usdc.{}", genesis_account_id).parse()?;
    
    // Create a user account
    let user_id: AccountId = format!("alice.{}", genesis_account_id).parse()?;
    let user_secret_key = signer::generate_secret_key()?;
    let user_signer: Arc<Signer> = Signer::new(Signer::from_secret_key(user_secret_key.clone())).unwrap();

    Account::create_account(user_id.clone())
        .fund_myself(genesis_account_id.clone(), NearToken::from_near(5))
        .public_key(user_secret_key.public_key())
        .unwrap()
        .with_signer(genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    println!("User account created: {}", user_id);

    // Register user with FT contract (storage deposit)
    let ft_contract = Contract(ft_id.clone());
    ft_contract
        .call_function("storage_deposit", json!({
            "account_id": user_id
        }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    println!("User registered with FT contract");

    // Transfer some USDC to user (100 USDC = 100,000,000 base units with 6 decimals)
    let transfer_amount = "100000000"; // 100 USDC
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": user_id,
            "amount": transfer_amount,
            "memo": "Initial funding"
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    println!("Transferred {} USDC to user", transfer_amount);

    // Verify user's FT balance
    let user_ft_balance: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({
            "account_id": user_id
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("User FT balance: {}", user_ft_balance.data);
    assert_eq!(user_ft_balance.data, transfer_amount);

    // Register user with vault contract (storage deposit)
    let vault_contract = Contract(vault_id.clone());
    vault_contract
        .call_function("storage_deposit", json!({
            "account_id": user_id
        }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(user_id.clone(), user_signer.clone())
        .send_to(&network_config)
        .await?;

    println!("User registered with vault contract");

    // User deposits USDC into vault via ft_transfer_call
    let deposit_amount = "50000000"; // 50 USDC
    let deposit_result = ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": deposit_amount,
            "memo": "Depositing to vault",
            "msg": json!({
                "receiver_id": user_id
            }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(user_id.clone(), user_signer.clone())
        .send_to(&network_config)
        .await?;

    println!("User deposited {} USDC to vault: {:?}", deposit_amount, deposit_result.status);

    // Check user's vault share balance
    let user_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({
            "account_id": user_id
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("User vault shares received: {}", user_shares.data);

    // Calculate expected shares (first deposit with extra_decimals)
    let multiplier = 10u128.pow(EXTRA_DECIMALS as u32);
    let expected_shares = (deposit_amount.parse::<u128>().unwrap() * multiplier).to_string();
    println!("Expected shares: {} (deposit × {})", expected_shares, multiplier);
    
    assert_eq!(user_shares.data, expected_shares, "User should receive shares with extra_decimals multiplier");

    // Verify vault's total assets
    let vault_total_assets: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Vault total assets: {}", vault_total_assets.data);
    assert_eq!(vault_total_assets.data, deposit_amount, "Vault should track deposited assets");

    // Verify vault's total supply of shares
    let vault_total_supply: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Vault total share supply: {}", vault_total_supply.data);
    assert_eq!(vault_total_supply.data, expected_shares);

    println!("✅ Vault deposit and share issuance test passed!");
    println!("   - User deposited: {} USDC", deposit_amount);
    println!("   - User received: {} shares", user_shares.data);
    println!("   - Multiplier: {} (10^{})", multiplier, EXTRA_DECIMALS);
    println!("   - Vault total assets: {}", vault_total_assets.data);
    println!("   - Vault total shares: {}", vault_total_supply.data);

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

async fn deploy_mock_ft(
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
    
    Contract::deploy(ft_id.clone())
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
        .await?;

    println!("Mock FT deployed with total_supply: {}", total_supply);

    Ok(ft_id)
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

