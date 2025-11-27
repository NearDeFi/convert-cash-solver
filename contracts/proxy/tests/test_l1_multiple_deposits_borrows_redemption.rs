mod helpers;

use helpers::*;
use helpers::test_builder::{
    deposit_to_vault, get_balance, get_total_assets, process_redemption_queue,
    redeem_shares, solver_borrow, solver_repay, TestScenarioBuilder,
};
use near_api::Data;
use serde_json::json;

/// Test: L1 deposit -> solver borrows -> L1 deposit more -> solver borrows -> L1 redeems all -> solver repays all
/// This tests that L1 gets the correct amount back (all deposits + yield from both borrows)
#[tokio::test]
async fn test_l1_multiple_deposits_borrows_redemption() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: L1 Multiple Deposits, Borrows, and Full Redemption ===");
    
    let builder = TestScenarioBuilder::new()
        .await?
        .deploy_vault()
        .await?
        .create_account("lender1")
        .await?
        .create_account("solver")
        .await?
        .register_accounts()
        .await?;

    let lender1_deposit1 = 50_000_000u128;
    let lender1_deposit2 = 30_000_000u128;
    
    // Step 1: L1 deposits first amount
    println!("\n--- Step 1: L1 deposits {} ---", lender1_deposit1);
    let lender1_shares1 = deposit_to_vault(&builder, "lender1", lender1_deposit1).await?;
    println!("L1 received {} shares", lender1_shares1);

    // Step 2: Solver borrows all liquidity
    println!("\n--- Step 2: Solver borrows {} (all liquidity) ---", lender1_deposit1);
    let borrow1 = solver_borrow(&builder, Some(lender1_deposit1), "hash-1").await?;
    let total_assets_after_borrow1 = get_total_assets(&builder).await?;
    assert_eq!(total_assets_after_borrow1, 0, "All assets should be borrowed");

    // Step 3: L1 deposits more (while vault has 0 assets)
    println!("\n--- Step 3: L1 deposits {} more (vault has 0 assets) ---", lender1_deposit2);
    let lender1_shares2 = deposit_to_vault(&builder, "lender1", lender1_deposit2).await?;
    println!("L1 received {} additional shares (total: {})", lender1_shares2, lender1_shares1 + lender1_shares2);
    
    // Step 4: Solver borrows again (all of L1's second deposit)
    println!("\n--- Step 4: Solver borrows {} again ---", lender1_deposit2);
    let borrow2 = solver_borrow(&builder, Some(lender1_deposit2), "hash-2").await?;
    let total_assets_after_borrow2 = get_total_assets(&builder).await?;
    assert_eq!(total_assets_after_borrow2, 0, "All assets should be borrowed again");

    // Step 5: L1 redeems all shares (will be queued)
    println!("\n--- Step 5: L1 redeems all shares (will be queued) ---");
    let lender1_total_shares = lender1_shares1 + lender1_shares2;
    redeem_shares(&builder, "lender1", lender1_total_shares).await?;
    
    let queue_length: Data<String> = builder.vault_contract()
        .call_function("get_pending_redemptions_length", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    assert!(queue_length.data.parse::<u128>().unwrap() > 0, "L1's redemption should be queued");

    // Step 6: Solver repays first borrow with 1% yield
    println!("\n--- Step 6: Solver repays first borrow with 1% yield ---");
    let intent_yield1 = borrow1 / 100;
    solver_repay(&builder, 0, borrow1, intent_yield1).await?;
    process_redemption_queue(&builder).await?;

    // Step 7: Solver repays second borrow with 1% yield
    println!("\n--- Step 7: Solver repays second borrow with 1% yield ---");
    let intent_yield2 = borrow2 / 100;
    solver_repay(&builder, 1, borrow2, intent_yield2).await?;
    process_redemption_queue(&builder).await?;

    // Step 8: Verify L1 received all deposits + yield from both borrows
    println!("\n--- Step 8: Verify L1 received all deposits + yield ---");
    let lender1_final_balance = get_balance(&builder, "lender1").await?;
    let total_deposits = lender1_deposit1 + lender1_deposit2;
    let total_yield = intent_yield1 + intent_yield2;
    let expected_balance = total_deposits + total_yield;
    
    println!("L1 final balance: {} (expected: {} deposits + {} yield = {})", 
        lender1_final_balance, total_deposits, total_yield, expected_balance);
    assert_eq!(lender1_final_balance, expected_balance, 
        "L1 should receive all deposits + yield from both borrows");

    // Final state verification
    let total_assets_final = get_total_assets(&builder).await?;
    assert_eq!(total_assets_final, 0, "Total assets should be 0 after all redemptions");

    println!("\n✅ Test passed! L1 received all deposits + yield from both borrows");
    Ok(())
}

