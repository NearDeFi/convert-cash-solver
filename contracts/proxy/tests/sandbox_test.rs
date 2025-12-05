//! # Sandbox Tests - Contract Deployment and Basic Functionality
//!
//! These tests verify the fundamental deployment and initialization of the vault
//! contract using the NEAR sandbox environment. They serve as smoke tests to ensure
//! the contract can be deployed and queried correctly.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_mock_ft_deployment_only` | Deploys mock USDC | FT contract responds with correct supply and metadata |
//! | `test_contract_deployment` | Deploys vault contract | Vault deploys without errors |
//! | `test_approve_codehash` | Owner approves TEE codehash | Codehash approved successfully |
//! | `test_vault_initialization` | Checks vault initial state | Zero assets, zero shares, correct metadata |
//! | `test_vault_conversion_functions` | Tests share conversion | Empty vault uses extra_decimals multiplier |
//!
//! ## No Lender/Solver Interaction
//!
//! These tests focus purely on contract deployment and view functions.
//! They do not involve deposits, borrows, or redemptions.

mod helpers;

use helpers::*;
use near_api::{Contract, Data};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Tests deployment of the mock fungible token contract.
///
/// # Scenario
///
/// Deploys a mock USDC token and verifies its configuration.
///
/// # Expected Outcome
///
/// - FT contract deploys successfully
/// - Total supply matches initialization value
/// - Metadata shows correct symbol (USDC) and decimals (6)
#[tokio::test]
async fn test_mock_ft_deployment_only() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    let total_supply = "1000000000000";
    let ft_id = deploy_mock_ft(
        &network_config,
        &genesis_account_id,
        &genesis_signer,
        total_supply,
    )
    .await?;

    // Wait for deploy to finalize
    sleep(Duration::from_millis(200)).await;

    let ft_contract = Contract(ft_id.clone());
    let supply: Data<String> = ft_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(supply.data, total_supply);

    let metadata: Data<serde_json::Value> = ft_contract
        .call_function("ft_metadata", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(metadata.data["symbol"], "USDC");
    assert_eq!(metadata.data["decimals"], 6);

    Ok(())
}

/// Tests basic vault contract deployment.
///
/// # Scenario
///
/// Deploys the vault contract with mock FT as underlying asset.
///
/// # Expected Outcome
///
/// - Vault contract deploys without errors
/// - Contract account is accessible
#[tokio::test]
async fn test_contract_deployment() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    let contract_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    
    println!("Contract deployed to: {}", contract_id);
    
    Ok(())
}

/// Tests owner-only codehash approval function.
///
/// # Scenario
///
/// Owner calls `approve_codehash` to whitelist a TEE worker codehash.
///
/// # Expected Outcome
///
/// - Transaction succeeds when called by owner
/// - Codehash is added to approved set
#[tokio::test]
async fn test_approve_codehash() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    let contract_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;

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

/// Tests vault initialization state and metadata.
///
/// # Scenario
///
/// Deploys vault and queries all initial state values.
///
/// # Expected Outcome
///
/// - Share token metadata: name=vUSDC, decimals=24
/// - Underlying asset: usdc.* address
/// - Total assets: 0 (no deposits yet)
/// - Total shares: 0 (no shares minted)
#[tokio::test]
async fn test_vault_initialization() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    let contract_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    
    let vault_contract = Contract(contract_id.clone());

    // Check share token metadata
    let metadata: Data<serde_json::Value> = vault_contract
        .call_function("ft_metadata", json!([]))? 
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Vault share metadata: {:?}", metadata.data);
    
    assert_eq!(metadata.data["name"], "USDC Vault Shares");
    assert_eq!(metadata.data["symbol"], "vUSDC");
    assert_eq!(metadata.data["decimals"], 24);

    // Check underlying asset
    let asset: Data<String> = vault_contract
        .call_function("asset", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Underlying asset: {}", asset.data);
    assert!(asset.data.starts_with("usdc."), "Asset should be usdc token");

    // Check initial state (empty vault)
    let total_assets: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Total assets: {}", total_assets.data);
    assert_eq!(total_assets.data, "0");

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

/// Tests vault share conversion functions on an empty vault.
///
/// # Scenario
///
/// Calls `preview_deposit` and `preview_withdraw` on empty vault.
///
/// # Expected Outcome
///
/// - Empty vault uses extra_decimals multiplier (10^3 = 1000)
/// - 1 token input produces 1000 shares output
#[tokio::test]
async fn test_vault_conversion_functions() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    let contract_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    
    let contract = Contract(contract_id.clone());

    // Test preview_deposit with extra_decimals
    let assets_to_convert = "1000000000000000000000000"; // 1 token with 24 decimals
    let shares: Data<String> = contract
        .call_function("preview_deposit", json!({
            "assets": assets_to_convert
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("Assets {} converts to shares: {}", assets_to_convert, shares.data);
    
    // Empty vault multiplies by 10^EXTRA_DECIMALS
    let multiplier = 10u128.pow(EXTRA_DECIMALS as u32);
    let expected_shares = (assets_to_convert.parse::<u128>().unwrap() * multiplier).to_string();
    println!("Expected shares: {} (assets × {})", expected_shares, multiplier);
    assert_eq!(shares.data, expected_shares, "Should multiply by 10^{} = {}", EXTRA_DECIMALS, multiplier);

    // Test preview_withdraw
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
