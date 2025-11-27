// Test: Solver tries to borrow from empty pool
// This should fail - no liquidity available

mod helpers;

use helpers::test_builder::{
    get_balance, get_total_assets,
    TestScenarioBuilder,
};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Test edge case: Solver tries to borrow from empty pool (should fail)
#[tokio::test]
async fn test_solver_borrow_empty_pool() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Solver borrow from empty pool - Should FAIL ===");
    
    let builder = TestScenarioBuilder::new()
        .await?
        .deploy_vault()
        .await?
        .create_account("solver")
        .await?
        .register_accounts()
        .await?;

    // Verify pool is empty
    let total_assets_before = get_total_assets(&builder).await?;
    println!("Total assets in pool: {} (should be 0)", total_assets_before);
    assert_eq!(total_assets_before, 0, "Pool should be empty");

    // Solver tries to borrow from empty pool (should FAIL)
    let borrow_amount = 1_000_000u128; // Try to borrow 1 USDC
    println!("\n=== Solver tries to borrow {} from empty pool ===", borrow_amount);
    
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
    let intent_result = builder.vault_contract()
        .call_function("new_intent", json!({
            "intent_data": "intent-empty",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-empty",
            "amount": borrow_amount.to_string()
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

    // Verify solver didn't receive any tokens
    let solver_balance = get_balance(&builder, "solver").await?;
    println!("Solver balance: {} (should be 0)", solver_balance);
    assert_eq!(solver_balance, 0, "Solver should not have received any tokens from empty pool");

    // Verify total_assets is still 0
    let total_assets_after = get_total_assets(&builder).await?;
    println!("Total assets after borrow attempt: {} (should be 0)", total_assets_after);
    assert_eq!(total_assets_after, 0, "Total assets should still be 0");

    println!("\n✅ Test passed! Solver borrow from empty pool was correctly rejected");
    Ok(())
}
