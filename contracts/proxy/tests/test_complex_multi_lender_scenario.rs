mod helpers;

use helpers::*;
use helpers::test_builder::{
    deposit_to_vault, get_balance, get_shares, get_total_assets, process_redemption_queue,
    redeem_shares, solver_borrow, solver_repay, TestScenarioBuilder,
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
    let lender1_shares = deposit_to_vault(&builder, "lender1", lender1_deposit).await?;
    println!("L1 received {} shares", lender1_shares);

    // Step 2: Solver borrows part of L1's deposit
    println!("\n--- Step 2: Solver borrows {} (part of L1's deposit) ---", lender1_deposit / 2);
    let borrow1 = lender1_deposit / 2;
    solver_borrow(&builder, Some(borrow1), "hash-1").await?;

    // Step 3: L2 deposits (while borrow is active)
    println!("\n--- Step 3: L2 deposits {} (while borrow is active) ---", lender2_deposit);
    let lender2_shares = deposit_to_vault(&builder, "lender2", lender2_deposit).await?;
    println!("L2 received {} shares", lender2_shares);

    // Step 4: L1 redeems half of their shares (partial redemption)
    println!("\n--- Step 4: L1 redeems half of their shares ---");
    let lender1_half_shares = lender1_shares / 2;
    redeem_shares(&builder, "lender1", lender1_half_shares).await?;
    sleep(Duration::from_millis(2000)).await;

    // Step 5: Solver borrows more (using L2's deposit)
    println!("\n--- Step 5: Solver borrows {} (using L2's deposit) ---", lender2_deposit);
    solver_borrow(&builder, Some(lender2_deposit), "hash-2").await?;

    // Step 6: L3 deposits (while two borrows are active)
    println!("\n--- Step 6: L3 deposits {} (while two borrows are active) ---", lender3_deposit);
    let lender3_shares = deposit_to_vault(&builder, "lender3", lender3_deposit).await?;
    println!("L3 received {} shares", lender3_shares);

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
    
    // L1: deposited lender1_deposit, should get deposit + proportional yield from borrow1
    // L2: deposited lender2_deposit, should get deposit + yield from borrow2
    // L3: deposited lender3_deposit, should get only deposit (no yield, deposited after borrows)
    
    println!("L1 final balance: {} (deposited: {})", lender1_final, lender1_deposit);
    println!("L2 final balance: {} (deposited: {})", lender2_final, lender2_deposit);
    println!("L3 final balance: {} (deposited: {})", lender3_final, lender3_deposit);
    
    // L1 should get at least their deposit + some yield
    assert!(lender1_final >= lender1_deposit, "L1 should get at least their deposit");
    
    // L2 should get deposit + yield
    assert!(lender2_final >= lender2_deposit, "L2 should get at least their deposit");
    
    // L3 should get only deposit (no yield since they deposited after borrows)
    assert_eq!(lender3_final, lender3_deposit, "L3 should get only their deposit (no yield)");

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

