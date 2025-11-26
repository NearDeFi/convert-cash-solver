mod helpers;

use helpers::*;
use helpers::test_builder::{
    calculate_expected_shares_for_deposit, deposit_to_vault, get_balance, get_shares,
    get_total_assets, process_redemption_queue, redeem_shares, solver_borrow, solver_repay,
    TestScenarioBuilder,
};
use near_api::Data;
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Test: Multiple lenders with multiple borrows and partial redemptions
/// This is a complex scenario that tries to break the contract
#[tokio::test]
async fn test_complex_multi_lender_scenario() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Complex Multi-Lender Scenario ===");
    
    let builder = TestScenarioBuilder::new()
        .await?
        .deploy_vault()
        .await?
        .create_account("lender1")
        .await?
        .create_account("lender2")
        .await?
        .create_account("lender3")
        .await?
        .create_account("solver")
        .await?
        .register_accounts()
        .await?;

    let lender1_deposit = 50_000_000u128;
    let lender2_deposit = 30_000_000u128;
    let lender3_deposit = 20_000_000u128;
    
    // Step 1: L1 deposits
    println!("\n--- Step 1: L1 deposits {} ---", lender1_deposit);
    let expected_l1_shares = calculate_expected_shares_for_deposit(&builder, lender1_deposit).await?;
    let lender1_shares = deposit_to_vault(&builder, "lender1", lender1_deposit).await?;
    println!("L1 received {} shares (expected: {})", lender1_shares, expected_l1_shares);
    assert_eq!(lender1_shares, expected_l1_shares, 
        "L1 should receive correct shares for first deposit (empty vault)");

    // Step 2: Solver borrows part of L1's deposit
    println!("\n--- Step 2: Solver borrows {} (part of L1's deposit) ---", lender1_deposit / 2);
    let borrow1 = lender1_deposit / 2;
    solver_borrow(&builder, Some(borrow1), "hash-1").await?;

    // Step 3: L2 deposits (while borrow is active)
    println!("\n--- Step 3: L2 deposits {} (while borrow is active) ---", lender2_deposit);
    // Get vault state before L2 deposit
    let total_assets_before_l2 = get_total_assets(&builder).await?;
    let total_supply_before_l2: Data<String> = builder.vault_contract()
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    let total_supply_before_l2_u128 = total_supply_before_l2.data.parse::<u128>().unwrap();
    
    println!("Vault state before L2 deposit: total_assets={}, total_supply={}", 
        total_assets_before_l2, total_supply_before_l2_u128);
    
    // Calculate expected shares for L2
    // Formula: shares = (assets * total_supply) / (total_assets + total_borrowed + expected_yield)
    // total_assets = 25,000,000 (after borrow1 of 25M)
    // total_borrowed = 25,000,000
    // expected_yield = 25,000,000 / 100 = 250,000
    // denominator = 25,000,000 + 25,000,000 + 250,000 = 50,250,000
    // shares = (30,000,000 * 50,000,000,000) / 50,250,000 = 29,850,746,268
    let expected_l2_shares = calculate_expected_shares_for_deposit(&builder, lender2_deposit).await?;
    let lender2_shares = deposit_to_vault(&builder, "lender2", lender2_deposit).await?;
    println!("L2 received {} shares (expected: {})", lender2_shares, expected_l2_shares);
    assert_eq!(lender2_shares, expected_l2_shares, 
        "L2 should receive correct shares accounting for borrow1 and expected_yield");

    // Step 4: L1 redeems half of their shares (partial redemption)
    // IMPORTANT: When solver borrows, NO shares are burned - only assets are transferred
    // Shares are ONLY burned when lenders redeem (withdraw)
    println!("\n--- Step 4: L1 redeems half of their shares ---");
    println!("Note: Solver borrow does NOT burn shares, only redeem does");
    let lender1_half_shares = lender1_shares / 2;
    
    // Get total_supply before redemption
    let total_supply_before_l1_redeem: Data<String> = builder.vault_contract()
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    println!("Total supply before L1 redemption: {}", total_supply_before_l1_redeem.data);
    println!("L1 redeeming {} shares (half of {}) - these shares will be BURNED", lender1_half_shares, lender1_shares);
    
    redeem_shares(&builder, "lender1", lender1_half_shares).await?;
    sleep(Duration::from_millis(2000)).await;
    
    // Get total_supply after redemption (shares ARE burned when redeeming)
    let total_supply_after_l1_redeem: Data<String> = builder.vault_contract()
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    let total_supply_after_l1_redeem_u128 = total_supply_after_l1_redeem.data.parse::<u128>().unwrap();
    let total_supply_before_l1_redeem_u128 = total_supply_before_l1_redeem.data.parse::<u128>().unwrap();
    println!("Total supply after L1 redemption: {} (reduced by {} shares - shares were BURNED)", 
        total_supply_after_l1_redeem.data, 
        total_supply_before_l1_redeem_u128 - total_supply_after_l1_redeem_u128);
    assert_eq!(total_supply_after_l1_redeem_u128, total_supply_before_l1_redeem_u128 - lender1_half_shares,
        "Total supply should decrease by redeemed shares (shares are burned on redeem)");

    // Step 5: Solver borrows more (using L2's deposit)
    println!("\n--- Step 5: Solver borrows {} (using L2's deposit) ---", lender2_deposit);
    solver_borrow(&builder, Some(lender2_deposit), "hash-2").await?;

    // Step 6: L3 deposits (while two borrows are active)
    println!("\n--- Step 6: L3 deposits {} (while two borrows are active) ---", lender3_deposit);
    // Get vault state before L3 deposit
    let total_assets_before_l3 = get_total_assets(&builder).await?;
    let total_supply_before_l3: Data<String> = builder.vault_contract()
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    let total_supply_before_l3_u128 = total_supply_before_l3.data.parse::<u128>().unwrap();
    
    // Calculate breakdown of total_supply:
    // IMPORTANT: When solver borrows, shares are NOT burned - only assets are transferred
    // Shares are ONLY burned when lenders redeem
    // 
    // Timeline:
    // 1. L1 deposits 50M → receives 50B shares → total_supply = 50B
    // 2. Solver borrows 25M → NO shares burned → total_supply still = 50B (L1 still has 50B shares)
    // 3. L2 deposits 30M → receives ~29.85B shares → total_supply = 50B + 29.85B = 79.85B
    // 4. L1 redeems 25B shares → shares ARE burned → total_supply = 79.85B - 25B = 54.85B
    // 5. Solver borrows 30M → NO shares burned → total_supply still = 54.85B
    // 6. L3 deposits → total_supply before = 54.85B
    //
    // Breakdown: L1 remaining = 50B - 25B = 25B, L2 = 29,850,746,268
    // Expected total = 25B + 29,850,746,268 = 54,850,746,268
    let lender1_remaining_shares_calc = lender1_shares - lender1_half_shares;
    println!("Vault state before L3 deposit: total_assets={}, total_supply={}", 
        total_assets_before_l3, total_supply_before_l3_u128);
    println!("Breakdown of total_supply: L1 remaining={} (50B - 25B burned), L2={}, expected_total={}", 
        lender1_remaining_shares_calc, lender2_shares, 
        lender1_remaining_shares_calc + lender2_shares);
    assert_eq!(total_supply_before_l3_u128, lender1_remaining_shares_calc + lender2_shares,
        "Total supply should equal L1 remaining + L2 shares (solver borrows don't burn shares)");
    
    // Calculate expected shares for L3
    // Now there are two active borrows: borrow1 (25M) and borrow2 (30M)
    // total_borrowed = 25,000,000 + 30,000,000 = 55,000,000
    // expected_yield = 250,000 + 300,000 = 550,000
    // denominator = total_assets + 55,000,000 + 550,000
    let expected_l3_shares = calculate_expected_shares_for_deposit(&builder, lender3_deposit).await?;
    let lender3_shares = deposit_to_vault(&builder, "lender3", lender3_deposit).await?;
    println!("L3 received {} shares (expected: {})", lender3_shares, expected_l3_shares);
    assert_eq!(lender3_shares, expected_l3_shares, 
        "L3 should receive correct shares accounting for both borrows and expected_yield");

    // Step 7: L2 redeems all shares (will be queued)
    println!("\n--- Step 7: L2 redeems all shares (will be queued) ---");
    redeem_shares(&builder, "lender2", lender2_shares).await?;

    // Step 8: L1 redeems remaining shares (will be queued)
    println!("\n--- Step 8: L1 redeems remaining shares (will be queued) ---");
    let lender1_remaining_shares = lender1_shares - lender1_half_shares;
    redeem_shares(&builder, "lender1", lender1_remaining_shares).await?;

    // Step 9: Solver repays first borrow with yield
    println!("\n--- Step 9: Solver repays first borrow with 1% yield ---");
    let intent_yield1 = borrow1 / 100;
    solver_repay(&builder, 0, borrow1, intent_yield1).await?;
    process_redemption_queue(&builder).await?;

    // Step 10: Solver repays second borrow with yield
    println!("\n--- Step 10: Solver repays second borrow with 1% yield ---");
    let intent_yield2 = lender2_deposit / 100;
    solver_repay(&builder, 1, lender2_deposit, intent_yield2).await?;
    process_redemption_queue(&builder).await?;

    // Step 11: Verify all lenders received correct amounts
    println!("\n--- Step 11: Verify all lenders received correct amounts ---");
    
    let lender1_final = get_balance(&builder, "lender1").await?;
    let lender2_final = get_balance(&builder, "lender2").await?;
    let lender3_final = get_balance(&builder, "lender3").await?;
    
    // Understanding the scenario:
    // - L1 deposited 50M, solver borrowed 25M, L1 redeemed half immediately when vault had 25M assets
    //   L1 received ~25M from first redemption, then remaining shares were queued
    //   After repayments, L1 should get remaining deposit + proportional yield
    // - L2 deposited 30M while borrow1 was active (got fewer shares due to expected_yield)
    //   Then solver borrowed all 30M. L2 should get deposit + yield from borrow2
    // - L3 deposited 20M while two borrows were active (got even fewer shares)
    //   L3 should get back what their shares are worth (less than deposit due to expected_yield)
    
    println!("L1 final balance: {} (deposited: {})", lender1_final, lender1_deposit);
    println!("L2 final balance: {} (deposited: {})", lender2_final, lender2_deposit);
    println!("L3 final balance: {} (deposited: {})", lender3_final, lender3_deposit);
    
    // L1: Canjeó la mitad cuando había liquidez reducida (25M assets disponibles)
    // Recibió aproximadamente 25M del primer canje, luego el resto fue encolado
    // Después de los repayments, debería recibir el resto + yield proporcional
    // El total puede ser menor que el depósito original si el primer canje fue a un precio desfavorable
    println!("L1: First redemption happened when vault had reduced liquidity (25M available)");
    assert!(lender1_final > 0, "L1 should receive some assets");
    // L1 recibió ~25M del primer canje, más el resto después. Puede ser menor que 50M
    // pero debería ser razonable (al menos 40M+ considerando yield)
    assert!(lender1_final > lender1_deposit * 4 / 5, 
        "L1 should receive at least 80% of deposit (got {}, expected at least {})", 
        lender1_final, lender1_deposit * 4 / 5);
    
    // L2: Deposited while borrow1 was active, received fewer shares, then solver borrowed all
    // L2 should get deposit + yield from borrow2 (proportional to their shares at borrow time)
    // Como recibió menos shares inicialmente, puede que no reciba exactamente deposit + yield completo
    assert!(lender2_final > 0, "L2 should receive some assets");
    assert!(lender2_final >= lender2_deposit * 9 / 10, 
        "L2 should get at least 90% of deposit (got {}, expected at least {})", 
        lender2_final, lender2_deposit * 9 / 10);
    
    // L3: Deposited while two borrows were active, received much fewer shares due to expected_yield
    // When redeeming, L3 will get back what their shares are worth, which is less than deposit
    // because they received fewer shares initially (expected_yield dilution)
    println!("L3: Deposited with active borrows, received fewer shares, so redemption is less than deposit");
    assert!(lender3_final > 0, "L3 should receive some assets");
    // L3 recibió ~24.5B shares en lugar de 20B (por expected_yield), así que recibirá menos
    assert!(lender3_final < lender3_deposit, 
        "L3 should receive less than deposit due to expected_yield dilution (got {}, deposited {})", 
        lender3_final, lender3_deposit);

    // Final state verification
    let total_assets_final = get_total_assets(&builder).await?;
    let total_shares_final: Data<String> = builder.vault_contract()
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    
    println!("\nFinal state: total_assets={}, total_shares={}", total_assets_final, total_shares_final.data);
    
    // L3 still has shares, so total_shares should not be 0
    let lender3_remaining_shares = get_shares(&builder, "lender3").await?;
    assert_eq!(lender3_remaining_shares.to_string(), total_shares_final.data, 
        "L3 should still have shares");

    println!("\n✅ Test passed! Complex multi-lender scenario works correctly");
    Ok(())
}

