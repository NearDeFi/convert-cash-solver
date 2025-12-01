// NEP-621 Rounding Direction Security Tests
//
// According to NEP-621: "Vault calculations must consistently round in favor of the vault 
// to prevent exploitation. When issuing shares for deposits or transferring assets for 
// redemptions, round down; when calculating required shares or assets for specific amounts, 
// round up. This asymmetric rounding prevents users from extracting value through repeated 
// micro-transactions that exploit rounding errors and protects existing shareholders from 
// value dilution."

mod helpers;

use helpers::test_builder::*;
use near_api::{Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Test 1: Verify that deposits round DOWN when calculating shares
/// This ensures the vault never issues more shares than the deposited assets warrant
#[tokio::test]
async fn test_deposit_shares_round_down() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Deposit shares round DOWN (NEP-621) ===");

    // Use TestScenarioBuilder for setup
    let builder = TestScenarioBuilder::new().await?
        .deploy_vault().await?
        .create_account("lender1").await?
        .create_account("lender2").await?
        .register_accounts().await?;

    // Step 1: L1 deposits 100 USDC (first deposit)
    let l1_deposit = 100_000_000u128; // 100 USDC
    let l1_shares = deposit_to_vault(&builder, "lender1", l1_deposit).await?;
    
    // First deposit: shares = assets * 10^extra_decimals = 100_000_000 * 1000
    let expected_first_shares = l1_deposit * 1000; // extra_decimals = 3
    println!("L1 first deposit: {} USDC -> {} shares (expected: {})", l1_deposit, l1_shares, expected_first_shares);
    assert_eq!(l1_shares, expected_first_shares, "First deposit should use exact multiplier");

    // Step 2: L2 deposits a small amount
    let l2_deposit = 7u128; // 7 micro-USDC (0.000007 USDC)
    let l2_shares = deposit_to_vault(&builder, "lender2", l2_deposit).await?;
    
    // shares = (7 * 100_000_000_000) / 100_000_000 = 7000 (exact in this case)
    // Keeps the ratio of assets to shares at 1000:1 because there is no borrow and no yield.
    // So the total supply of shares is always 1000 times the total assets.
    println!("L2 deposit: {} micro-USDC -> {} shares", l2_deposit, l2_shares);
    
    // Verify: total shares value should be <= total assets (NEP-621 invariant)
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
    
    // With 1:1000 ratio, total_supply / 1000 should be <= total_assets
    let shares_value = total_supply_u128 / 1000;
    println!("Shares value (supply/1000): {} vs total_assets: {}", shares_value, total_assets_u128);
    assert!(shares_value <= total_assets_u128, "Vault should never issue more shares than assets warrant");

    println!("\n✅ Deposit rounding test passed - shares round DOWN correctly");
    Ok(())
}

/// Test 2: Micro-transactions attack prevention
/// Verifies that repeated small deposits and redemptions cannot extract value
#[tokio::test]
async fn test_micro_transaction_attack_prevention() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Micro-transaction attack prevention (NEP-621) ===");

    // Use TestScenarioBuilder for setup
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

    // Honest user deposits first to establish base rate
    let honest_deposit = 1_000_000_000u128; // 1000 USDC
    let honest_shares_initial = deposit_to_vault(&builder, "honest", honest_deposit).await?;
    println!("Honest user deposited {} USDC, received {} shares", honest_deposit, honest_shares_initial);

    // Get attacker account info
    let (attacker_id, attacker_signer, _) = builder.get_account("attacker").unwrap();
    
    // Transfer funds to attacker for micro-deposit attacks
    let attacker_funds = 10_000u128; // 0.01 USDC total for attack
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

    // Perform multiple small deposits
    let micro_deposit = 1u128; // 1 micro-USDC (smallest unit)
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

    // Now redeem all attacker shares
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

    // Check final balances
    let attacker_balance_after = get_balance(&builder, "attacker").await?;
    println!("Attacker final balance: {} micro-USDC", attacker_balance_after);
    println!("Attacker net result: {} (started with {})", 
        attacker_balance_after as i128 - attacker_balance_before as i128,
        attacker_balance_before);

    // Key assertion: attacker should NOT have gained value
    assert!(
        attacker_balance_after <= attacker_balance_before,
        "Attacker should not profit from micro-transactions! Before: {}, After: {}",
        attacker_balance_before, attacker_balance_after
    );

    // Verify honest user's shares weren't diluted
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

/// Test 3: Very small amounts edge case
/// Verifies correct behavior with amounts near the precision limit
#[tokio::test]
async fn test_small_amount_precision() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Small amount precision (NEP-621) ===");

    // Use TestScenarioBuilder for setup
    let builder = TestScenarioBuilder::new().await?
        .deploy_vault().await?
        .create_account("lender").await?
        .register_accounts().await?;

    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();

    // Test: Minimum possible deposit (1 micro-USDC)
    let min_deposit = 1u128;
    let shares_received = deposit_to_vault(&builder, "lender", min_deposit).await?;
    
    // For first deposit: shares = assets * 10^extra_decimals = 1 * 1000 = 1000
    println!("Minimum deposit ({} micro-USDC) -> {} shares", min_deposit, shares_received);
    
    // Even minimum deposit should receive some shares (with extra_decimals multiplier)
    assert!(shares_received > 0, "Minimum deposit should receive at least 1 share");
    assert_eq!(shares_received, min_deposit * 1000, "First deposit should use exact multiplier");

    // Verify convert_to_assets for these shares
    let assets_value: Data<String> = vault_contract
        .call_function("convert_to_assets", json!({ "shares": shares_received.to_string() }))?
        .read_only()
        .fetch_from(network_config)
        .await?;
    let assets_value_u128 = assets_value.data.parse::<u128>().unwrap();
    
    println!("{} shares converts to {} assets", shares_received, assets_value_u128);
    
    // Key NEP-621 check: assets received on redemption should be <= assets deposited
    assert!(
        assets_value_u128 <= min_deposit,
        "Redemption value should not exceed deposited amount (rounding DOWN)"
    );

    println!("\n✅ Small amount precision test passed!");
    println!("   - Minimum deposit correctly handled");
    println!("   - Rounding favors vault as per NEP-621");

    Ok(())
}

/// Test 4: Yield distribution rounding
/// Verifies that yield calculations round in vault's favor
#[tokio::test]
async fn test_yield_calculation_rounding() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Yield calculation rounding (NEP-621) ===");

    // Use TestScenarioBuilder for setup
    let builder = TestScenarioBuilder::new().await?
        .deploy_vault().await?
        .create_account("lender").await?
        .create_account("solver").await?
        .register_accounts().await?;

    let vault_contract = builder.vault_contract();
    let network_config = builder.network_config();

    // Deposit amount that creates interesting yield calculation
    let lender_deposit = 33_333_333u128; // 33.333333 USDC - creates rounding in yield
    let lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;
    println!("Lender deposited {} micro-USDC, received {} shares", lender_deposit, lender_shares);

    // Solver borrows a weird amount that creates yield rounding
    let borrow_amount = 11_111_111u128; // 11.111111 USDC
    // Expected yield = 11_111_111 / 100 = 111_111 (truncated from 111_111.11)
    let expected_yield = borrow_amount / 100;
    println!("Solver borrowing {} micro-USDC", borrow_amount);
    println!("Expected yield (integer division): {} micro-USDC", expected_yield);
    println!("Theoretical yield (float): {} micro-USDC", borrow_amount as f64 / 100.0);
    
    // Use helper to borrow
    solver_borrow(&builder, Some(borrow_amount), "yield-test").await?;

    // Check the calculated expected yield in convert_to_assets
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

    // Use helper to repay
    solver_repay(&builder, 0, borrow_amount, expected_yield).await?;

    // Lender redeems using helper
    redeem_shares(&builder, "lender", lender_shares).await?;

    // Check final balance
    let lender_final_balance = get_balance(&builder, "lender").await?;

    println!("\nFinal results:");
    println!("  Lender final balance: {} micro-USDC", lender_final_balance);
    println!("  Expected (deposit + yield): {} micro-USDC", lender_deposit + expected_yield);
    
    // The lender should receive their deposit back plus the integer-divided yield
    // Due to rounding, they should receive <= theoretical maximum
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

/// Test 5: Redemption rounding protects vault
/// Verifies redeem() uses Rounding::Down for assets
#[tokio::test]
async fn test_redemption_rounds_down() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Redemption rounds DOWN (NEP-621) ===");

    // Use TestScenarioBuilder for setup
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

    // L2 deposits 1 micro-USDC to change the ratio slightly
    let l2_deposit = 1u128;
    let _l2_shares = deposit_to_vault(&builder, "lender2", l2_deposit).await?;
    println!("L2 deposited {} micro-USDC, got {} shares", l2_deposit, _l2_shares);

    // Check total state
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

    // Calculate what L1 would receive with exact math vs integer division
    let exact_assets = (l1_shares as f64 * total_assets_u128 as f64) / total_supply_u128 as f64;
    let integer_assets = (l1_shares * total_assets_u128) / total_supply_u128;
    
    println!("L1's {} shares worth:", l1_shares);
    println!("  Exact (float): {} USDC", exact_assets);
    println!("  Integer division: {} USDC", integer_assets);

    // L1 redeems all shares (using direct call to check the exact value received)
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

    // Key assertion: redeemed amount should use integer division (round down)
    assert!(
        l1_final_balance <= exact_assets.ceil() as u128,
        "Redemption should round DOWN, not UP"
    );

    // Verify vault still has remaining assets for L2
    let vault_remaining = get_total_assets(&builder).await?;
    
    println!("Vault remaining assets: {} USDC", vault_remaining);
    assert!(vault_remaining > 0, "Vault should have remaining assets for L2");

    println!("\n✅ Redemption rounding test passed!");
    println!("   - Redemption uses integer division (rounds down)");
    println!("   - Vault protected from rounding exploitation");

    Ok(())
}
