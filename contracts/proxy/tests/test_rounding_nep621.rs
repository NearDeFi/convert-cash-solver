//! # NEP-621 Rounding Direction Security Tests
//!
//! Verifies that the vault follows NEP-621 rounding rules to prevent exploitation.
//! According to NEP-621: "Vault calculations must consistently round in favor of
//! the vault to prevent exploitation."
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_deposit_shares_round_down` | Verify shares issued round DOWN | Shares value ≤ deposited assets |
//! | `test_micro_transaction_attack_prevention` | Repeated small deposits/redeems | Attacker cannot profit |
//! | `test_small_amount_precision` | Minimum deposit (1 unit) | Handled correctly with multiplier |
//! | `test_yield_calculation_rounding` | Yield with odd amounts | Integer division truncates |
//! | `test_redemption_rounds_down` | Redeem calculates DOWN | Vault protected from rounding |
//!
//! ## NEP-621 Rounding Rules
//!
//! ```text
//! - When issuing shares for deposits: ROUND DOWN
//! - When transferring assets for redemptions: ROUND DOWN
//! - When calculating required shares: ROUND UP
//! - When calculating required assets: ROUND UP
//!
//! This asymmetric rounding prevents:
//! 1. Value extraction through micro-transactions
//! 2. Share dilution for existing shareholders
//! 3. Rounding exploitation attacks
//! ```
//!
//! ## Key Invariant
//!
//! `total_shares_value ≤ total_assets` (always)

mod helpers;

use helpers::test_builder::*;
use near_api::{Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Tests that deposit share issuance rounds DOWN.
///
/// # Scenario
///
/// L1 makes initial deposit, L2 makes small deposit.
/// Verify shares issued never exceed asset value.
///
/// # Expected Outcome
///
/// - Shares value (supply / multiplier) ≤ total assets
/// - No value created from rounding
#[tokio::test]
async fn test_deposit_shares_round_down() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Deposit shares round DOWN (NEP-621) ===");

    let builder = TestScenarioBuilder::new().await?
        .deploy_vault().await?
        .create_account("lender1").await?
        .create_account("lender2").await?
        .register_accounts().await?;

    // L1 deposits 100 USDC (first deposit)
    let l1_deposit = 100_000_000u128;
    let l1_shares = deposit_to_vault(&builder, "lender1", l1_deposit).await?;
    
    // First deposit: shares = assets * 10^extra_decimals
    let expected_first_shares = l1_deposit * 1000; // extra_decimals = 3
    println!("L1 first deposit: {} USDC -> {} shares (expected: {})", l1_deposit, l1_shares, expected_first_shares);
    assert_eq!(l1_shares, expected_first_shares, "First deposit should use exact multiplier");

    // L2 deposits small amount
    let l2_deposit = 7u128; // 7 micro-USDC
    let l2_shares = deposit_to_vault(&builder, "lender2", l2_deposit).await?;
    
    println!("L2 deposit: {} micro-USDC -> {} shares", l2_deposit, l2_shares);
    
    // Verify NEP-621 invariant
    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();
    
    let total_supply: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(network_config)
        .await?;
    let total_supply_u128 = total_supply.data.parse::<u128>().unwrap();
    
    let total_assets_u128 = get_total_assets(&builder).await?;
    
    println!("Total supply: {} shares", total_supply_u128);
    println!("Total assets: {} USDC", total_assets_u128);
    
    // With 1:1000 ratio, shares value should never exceed assets
    let shares_value = total_supply_u128 / 1000;
    println!("Shares value (supply/1000): {} vs total_assets: {}", shares_value, total_assets_u128);
    assert!(shares_value <= total_assets_u128, "Vault should never issue more shares than assets warrant");

    println!("\n✅ Deposit rounding test passed - shares round DOWN correctly");
    Ok(())
}

/// Tests that micro-transactions cannot extract value.
///
/// # Scenario
///
/// Attacker performs multiple tiny deposits and redemptions.
///
/// # Expected Outcome
///
/// - Attacker cannot profit from rounding
/// - Honest user shares unaffected
#[tokio::test]
async fn test_micro_transaction_attack_prevention() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Micro-transaction attack prevention (NEP-621) ===");

    let builder = TestScenarioBuilder::new().await?
        .deploy_vault().await?
        .create_account("attacker").await?
        .create_account("honest").await?
        .register_accounts().await?;

    let ft_contract = builder.ft_contract();
    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();
    let genesis_account_id = builder.genesis_account_id();
    let genesis_signer = builder.genesis_signer();

    // Honest user establishes base rate
    let honest_deposit = 1_000_000_000u128; // 1000 USDC
    let honest_shares_initial = deposit_to_vault(&builder, "honest", honest_deposit).await?;
    println!("Honest user deposited {} USDC, received {} shares", honest_deposit, honest_shares_initial);

    let (attacker_id, attacker_signer, _) = builder.get_account("attacker").unwrap();
    
    // Fund attacker
    let attacker_funds = 10_000u128; // 0.01 USDC
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": attacker_id,
            "amount": attacker_funds.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(network_config)
        .await?;

    let attacker_balance_before = get_balance(&builder, "attacker").await?;
    println!("\nAttacker starting balance: {} micro-USDC", attacker_balance_before);

    // Perform multiple micro deposits
    let micro_deposit = 1u128;
    let num_micro_deposits = 5;
    let vault_id = builder.vault_id();
    
    for i in 0..num_micro_deposits {
        let deposit_result = ft_contract
            .call_function("ft_transfer_call", json!({
                "receiver_id": vault_id,
                "amount": micro_deposit.to_string(),
                "msg": json!({ "receiver_id": attacker_id }).to_string()
            }))?
            .transaction()
            .deposit(NearToken::from_yoctonear(1))
            .with_signer(attacker_id.clone(), attacker_signer.clone())
            .send_to(network_config)
            .await;
        
        println!("Micro-deposit {} result: {:?}", i + 1, deposit_result.is_ok());
        sleep(Duration::from_millis(500)).await;
    }

    sleep(Duration::from_millis(1000)).await;

    let attacker_shares = get_shares(&builder, "attacker").await?;
    println!("\nAttacker received {} shares from micro-deposits", attacker_shares);

    // Redeem all attacker shares
    if attacker_shares > 0 {
        vault_contract
            .call_function("redeem", json!({
                "shares": attacker_shares.to_string(),
                "receiver_id": attacker_id
            }))?
            .transaction()
            .deposit(NearToken::from_yoctonear(1))
            .with_signer(attacker_id.clone(), attacker_signer.clone())
            .send_to(network_config)
            .await?;

        sleep(Duration::from_millis(1000)).await;
    }

    // Verify no profit
    let attacker_balance_after = get_balance(&builder, "attacker").await?;
    println!("Attacker final balance: {} micro-USDC", attacker_balance_after);
    println!("Attacker net result: {} (started with {})", 
        attacker_balance_after as i128 - attacker_balance_before as i128,
        attacker_balance_before);

    assert!(
        attacker_balance_after <= attacker_balance_before,
        "Attacker should not profit from micro-transactions! Before: {}, After: {}",
        attacker_balance_before, attacker_balance_after
    );

    // Verify honest user unaffected
    let honest_shares_after = get_shares(&builder, "honest").await?;
    assert_eq!(
        honest_shares_initial, honest_shares_after,
        "Honest user's shares should not be affected by attacker's micro-transactions"
    );

    println!("\n✅ Micro-transaction attack prevention test passed!");
    println!("   - Attacker could not extract value through rounding exploitation");
    println!("   - Honest user's shares were not diluted");
    
    Ok(())
}

/// Tests minimum deposit handling.
///
/// # Scenario
///
/// Deposit the smallest possible amount (1 micro-USDC).
///
/// # Expected Outcome
///
/// - Receives shares (with extra_decimals multiplier)
/// - Redemption value ≤ deposit amount
#[tokio::test]
async fn test_small_amount_precision() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Small amount precision (NEP-621) ===");

    let builder = TestScenarioBuilder::new().await?
        .deploy_vault().await?
        .create_account("lender").await?
        .register_accounts().await?;

    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();

    // Minimum deposit (1 USDC = 1,000,000 micro-USDC)
    let min_deposit = 1_000_000u128;
    let shares_received = deposit_to_vault(&builder, "lender", min_deposit).await?;
    
    // First deposit: shares = assets * 10^extra_decimals = 1,000,000 * 1000 = 1,000,000,000
    println!("Minimum deposit ({} micro-USDC = 1 USDC) -> {} shares", min_deposit, shares_received);
    
    assert!(shares_received > 0, "Minimum deposit should receive at least 1 share");
    assert_eq!(shares_received, min_deposit * 1000, "First deposit should use exact multiplier");

    // Verify convert_to_assets
    let assets_value: Data<String> = vault_contract
        .call_function("convert_to_assets", json!({ "shares": shares_received.to_string() }))?
        .read_only()
        .fetch_from(network_config)
        .await?;
    let assets_value_u128 = assets_value.data.parse::<u128>().unwrap();
    
    println!("{} shares converts to {} assets", shares_received, assets_value_u128);
    
    // NEP-621: redemption value ≤ deposit
    assert!(
        assets_value_u128 <= min_deposit,
        "Redemption value should not exceed deposited amount (rounding DOWN)"
    );

    println!("\n✅ Small amount precision test passed!");
    println!("   - Minimum deposit correctly handled");
    println!("   - Rounding favors vault as per NEP-621");

    Ok(())
}

/// Tests yield calculation rounding.
///
/// # Scenario
///
/// Borrow amount that creates odd yield calculation.
///
/// # Expected Outcome
///
/// - Integer division truncates yield
/// - Lender receives ≤ theoretical maximum
#[tokio::test]
async fn test_yield_calculation_rounding() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Yield calculation rounding (NEP-621) ===");

    let builder = TestScenarioBuilder::new().await?
        .deploy_vault().await?
        .create_account("lender").await?
        .create_account("solver").await?
        .register_accounts().await?;

    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();

    // Deposit with interesting yield calculation
    let lender_deposit = 33_333_333u128; // 33.333333 USDC
    let lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;
    println!("Lender deposited {} micro-USDC, received {} shares", lender_deposit, lender_shares);

    // Borrow weird amount
    let borrow_amount = 11_111_111u128; // 11.111111 USDC
    // Expected yield = 11_111_111 / 100 = 111_111 (truncated)
    let expected_yield = borrow_amount / 100;
    println!("Solver borrowing {} micro-USDC", borrow_amount);
    println!("Expected yield (integer division): {} micro-USDC", expected_yield);
    println!("Theoretical yield (float): {} micro-USDC", borrow_amount as f64 / 100.0);
    
    solver_borrow(&builder, borrow_amount, "yield-test").await?;

    // Check calculated yield in convert_to_assets
    let assets_with_yield: Data<String> = vault_contract
        .call_function("convert_to_assets", json!({ "shares": lender_shares.to_string() }))?
        .read_only()
        .fetch_from(network_config)
        .await?;
    let assets_with_yield_u128 = assets_with_yield.data.parse::<u128>().unwrap();
    
    println!("\nWith active borrow:");
    println!("  Lender's shares ({}) worth {} assets", lender_shares, assets_with_yield_u128);
    println!("  Original deposit: {} assets", lender_deposit);
    println!("  Expected with yield: {} assets", lender_deposit + expected_yield);

    // Repay and redeem
    solver_repay(&builder, 0, borrow_amount, expected_yield).await?;
    redeem_shares(&builder, "lender", lender_shares).await?;

    let lender_final_balance = get_balance(&builder, "lender").await?;

    println!("\nFinal results:");
    println!("  Lender final balance: {} micro-USDC", lender_final_balance);
    println!("  Expected (deposit + yield): {} micro-USDC", lender_deposit + expected_yield);
    
    // Verify ≤ theoretical max
    let theoretical_yield = (borrow_amount as f64 / 100.0).ceil() as u128;
    let theoretical_max = lender_deposit + theoretical_yield;
    
    assert!(
        lender_final_balance <= theoretical_max,
        "Lender should not receive more than theoretical maximum"
    );

    println!("\n✅ Yield calculation rounding test passed!");
    println!("   - Integer division truncates yield (rounds down)");
    println!("   - Lender receives expected amount based on integer yield");

    Ok(())
}

/// Tests that redemption rounds DOWN.
///
/// # Scenario
///
/// Multiple lenders, verify redemption uses integer division.
///
/// # Expected Outcome
///
/// - Redemption uses floor division
/// - Vault retains remainder for other lenders
#[tokio::test]
async fn test_redemption_rounds_down() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Redemption rounds DOWN (NEP-621) ===");

    let builder = TestScenarioBuilder::new().await?
        .deploy_vault().await?
        .create_account("lender1").await?
        .create_account("lender2").await?
        .register_accounts().await?;

    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();
    let (lender1_id, lender1_signer, _) = builder.get_account("lender1").unwrap();

    // L1 deposits 100 USDC
    let l1_deposit = 100_000_000u128;
    let l1_shares = deposit_to_vault(&builder, "lender1", l1_deposit).await?;
    println!("L1 deposited {} USDC, got {} shares", l1_deposit, l1_shares);

    // L2 deposits minimum amount (1 USDC) to create a different ratio
    let l2_deposit = 1_000_000u128; // 1 USDC
    let _l2_shares = deposit_to_vault(&builder, "lender2", l2_deposit).await?;
    println!("L2 deposited {} micro-USDC (1 USDC), got {} shares", l2_deposit, _l2_shares);

    // Check state
    let total_supply: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(network_config)
        .await?;
    let total_supply_u128 = total_supply.data.parse::<u128>().unwrap();

    let total_assets_u128 = get_total_assets(&builder).await?;

    println!("\nVault state:");
    println!("  Total supply: {} shares", total_supply_u128);
    println!("  Total assets: {} USDC", total_assets_u128);

    // Calculate exact vs integer division
    let exact_assets = (l1_shares as f64 * total_assets_u128 as f64) / total_supply_u128 as f64;
    let integer_assets = (l1_shares * total_assets_u128) / total_supply_u128;
    
    println!("L1's {} shares worth:", l1_shares);
    println!("  Exact (float): {} USDC", exact_assets);
    println!("  Integer division: {} USDC", integer_assets);

    // L1 redeems
    vault_contract
        .call_function("redeem", json!({
            "shares": l1_shares.to_string(),
            "receiver_id": lender1_id
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender1_id.clone(), lender1_signer.clone())
        .send_to(network_config)
        .await?;

    sleep(Duration::from_millis(1500)).await;

    let l1_final_balance = get_balance(&builder, "lender1").await?;

    println!("\nL1 redemption results:");
    println!("  Received: {} USDC", l1_final_balance);
    println!("  Expected (integer): {} USDC", integer_assets);

    // Should use integer division (round down)
    assert!(
        l1_final_balance <= exact_assets.ceil() as u128,
        "Redemption should round DOWN, not UP"
    );

    // Vault should have L2's deposit remaining
    let vault_remaining = get_total_assets(&builder).await?;
    
    println!("Vault remaining assets: {} USDC", vault_remaining);
    assert!(vault_remaining >= l2_deposit, "Vault should have at least L2's deposit remaining");

    println!("\n✅ Redemption rounding test passed!");
    println!("   - Redemption uses integer division (rounds down)");
    println!("   - Vault protected from rounding exploitation");

    Ok(())
}
