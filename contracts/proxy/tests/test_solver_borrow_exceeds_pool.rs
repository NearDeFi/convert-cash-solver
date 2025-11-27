// Test: Solver tries to borrow more than total_assets in the pool
// This should fail with "Insufficient assets for solver borrow"

mod helpers;

use helpers::test_builder::{
    deposit_to_vault, get_balance, get_total_assets,
    TestScenarioBuilder,
};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Test that solver cannot borrow more than total_assets in the pool
/// This should fail with "Insufficient assets for solver borrow"
#[tokio::test]
async fn test_solver_borrow_exceeds_pool_size() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Solver borrow exceeds pool size - Should FAIL ===");
    
    let builder = TestScenarioBuilder::new()
        .await?
        .deploy_vault()
        .await?
        .create_account("lender")
        .await?
        .create_account("solver")
        .await?
        .register_accounts()
        .await?;

    let lender_deposit = 50_000_000u128; // 50 USDC

    // Step 1: Lender deposits 50 USDC
    println!("\n=== Step 1: Lender deposits {} ===", lender_deposit);
    let _lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;

    // Verify total_assets
    let total_assets_before = get_total_assets(&builder).await?;
    println!("Total assets in pool: {}", total_assets_before);
    assert_eq!(total_assets_before, lender_deposit, "Total assets should equal deposit");

    // Step 2: Solver tries to borrow MORE than total_assets (should FAIL)
    let excessive_borrow = lender_deposit + 1; // 50,000,001 - one more than available
    println!("\n=== Step 2: Solver tries to borrow {} (more than pool size {}) ===", 
        excessive_borrow, lender_deposit);
    
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
    // Attempt to borrow more than available - this should fail
    let intent_result = builder.vault_contract()
        .call_function("new_intent", json!({
            "intent_data": "intent-excessive",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-excessive",
            "amount": excessive_borrow.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(builder.network_config())
        .await;

    // Verify the transaction failed or returned an error
    match &intent_result {
        Ok(outcome) => {
            let status_str = format!("{:?}", outcome.status);
            println!("Transaction status: {}", status_str);
            // Check if status indicates failure
            assert!(
                status_str.contains("Failure") || status_str.contains("Error"),
                "Transaction should have failed but got: {}", status_str
            );
            println!("✅ Transaction correctly failed with status: {}", status_str);
        }
        Err(e) => {
            let error_str = format!("{:?}", e);
            println!("✅ Transaction correctly returned error: {}", error_str);
            // Verify it's the expected error message
            assert!(
                error_str.contains("Insufficient assets") || error_str.contains("panic"),
                "Expected 'Insufficient assets' error but got: {}", error_str
            );
        }
    }

    sleep(Duration::from_millis(1000)).await;

    // Verify total_assets unchanged (borrow should have failed)
    let total_assets_after = get_total_assets(&builder).await?;
    
    println!("Total assets after excessive borrow attempt: {} (should still be {})", 
        total_assets_after, lender_deposit);
    assert_eq!(total_assets_after, lender_deposit, 
        "Borrow should have failed - total_assets should be unchanged");

    // Verify solver didn't receive any tokens
    let solver_balance = get_balance(&builder, "solver").await?;
    println!("Solver balance: {} (should be 0)", solver_balance);
    assert_eq!(solver_balance, 0, "Solver should not have received any tokens");

    println!("\n✅ Test passed! Solver borrow exceeding pool size was correctly rejected");
    Ok(())
}
