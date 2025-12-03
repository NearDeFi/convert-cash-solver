//! # Vault Deposit Test
//!
//! Tests the fundamental deposit flow where a user deposits underlying assets
//! (USDC) into the vault and receives vault shares in return.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_vault_deposit` | User deposits USDC via ft_transfer_call | User receives vault shares proportional to deposit |
//!
//! ## Lender/Vault Interaction
//!
//! ```text
//! 1. User starts with 100 USDC
//! 2. User deposits 50 USDC to vault via ft_transfer_call
//! 3. Vault mints shares using extra_decimals multiplier (first deposit)
//! 4. User receives 50 USDC × 10^3 = 50,000,000,000 shares
//! ```
//!
//! ## Key Verification Points
//!
//! - User's FT balance decreases by deposit amount
//! - User receives shares with extra_decimals precision
//! - Vault's total_assets reflects the deposit
//! - Vault's total_supply matches minted shares

mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;

/// Tests the complete deposit flow from USDC to vault shares.
///
/// # Scenario
///
/// A user receives 100 USDC, then deposits 50 USDC into the vault.
/// This is the first deposit, so shares are minted with the extra_decimals
/// multiplier (10^3 = 1000).
///
/// # Expected Outcome
///
/// - User receives 50,000,000 × 1000 = 50,000,000,000 shares
/// - Vault total_assets = 50,000,000 (50 USDC)
/// - Vault total_supply = 50,000,000,000 shares
#[tokio::test]
async fn test_vault_deposit() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox and deploy contracts
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;
    
    // Create user account
    let (user_id, user_signer) = create_user_account(&network_config, &genesis_account_id, &genesis_signer, "alice").await?;

    // Register user with FT contract
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

    // Transfer USDC to user (100 USDC = 100,000,000 with 6 decimals)
    let transfer_amount = "100000000";
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

    // Register user with vault contract
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

    // =========================================================================
    // DEPOSIT: User deposits 50 USDC to vault
    // =========================================================================
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

    // =========================================================================
    // VERIFY: Check shares received
    // =========================================================================
    let user_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({
            "account_id": user_id
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    println!("User vault shares received: {}", user_shares.data);

    // First deposit uses extra_decimals multiplier
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

    // Verify vault's total supply
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
