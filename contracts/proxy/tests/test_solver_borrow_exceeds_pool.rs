//! # Solver Borrow Exceeds Pool Test
//!
//! Tests the edge case where a solver attempts to borrow more than the
//! available liquidity (total_assets + 1). This should fail.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_solver_borrow_exceeds_pool_size` | Solver borrows total_assets + 1 | Transaction fails, state unchanged |
//!
//! ## Scenario
//!
//! ```text
//! 1. Lender deposits 50 USDC (total_assets = 50,000,000)
//! 2. Solver tries to borrow 50,000,001 (just 1 unit over)
//! 3. Contract panics with "Insufficient assets for solver borrow"
//! 4. No state changes occur
//! ```
//!
//! ## Key Verification Points
//!
//! - Transaction fails even with 1 unit over limit
//! - Vault total_assets remains unchanged
//! - Solver receives no tokens
//! - Lender's deposit is protected

mod helpers;

use helpers::test_builder::{
    deposit_to_vault, get_balance, get_total_assets,
    TestScenarioBuilder,
};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Tests that borrowing more than total_assets fails.
///
/// # Scenario
///
/// 1. Lender deposits 50 USDC
/// 2. Solver tries to borrow 50,000,001 (1 unit more than available)
///
/// # Expected Outcome
///
/// - Transaction fails with "Insufficient assets" error
/// - Vault total_assets = 50,000,000 (unchanged)
/// - Solver balance = 0
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

    // =========================================================================
    // LENDER DEPOSITS
    // =========================================================================
    println!("\n=== Step 1: Lender deposits {} ===", lender_deposit);
    let _lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;

    let total_assets_before = get_total_assets(&builder).await?;
    println!("Total assets in pool: {}", total_assets_before);
    assert_eq!(total_assets_before, lender_deposit, "Total assets should equal deposit");

    // =========================================================================
    // SOLVER ATTEMPTS TO BORROW MORE THAN AVAILABLE
    // =========================================================================
    let excessive_borrow = lender_deposit + 1; // 50,000,001 - one more than available
    println!("\n=== Step 2: Solver tries to borrow {} (more than pool size {}) ===", 
        excessive_borrow, lender_deposit);
    
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
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

    // Verify failure
    match &intent_result {
        Ok(outcome) => {
            let status_str = format!("{:?}", outcome.status);
            println!("Transaction status: {}", status_str);
            assert!(
                status_str.contains("Failure") || status_str.contains("Error"),
                "Transaction should have failed but got: {}", status_str
            );
            println!("✅ Transaction correctly failed with status: {}", status_str);
        }
        Err(e) => {
            let error_str = format!("{:?}", e);
            println!("✅ Transaction correctly returned error: {}", error_str);
            assert!(
                error_str.contains("Insufficient assets") || error_str.contains("panic"),
                "Expected 'Insufficient assets' error but got: {}", error_str
            );
        }
    }

    sleep(Duration::from_millis(1000)).await;

    // =========================================================================
    // VERIFY NO STATE CHANGES
    // =========================================================================
    let total_assets_after = get_total_assets(&builder).await?;
    
    println!("Total assets after excessive borrow attempt: {} (should still be {})", 
        total_assets_after, lender_deposit);
    assert_eq!(total_assets_after, lender_deposit, 
        "Borrow should have failed - total_assets should be unchanged");

    let solver_balance = get_balance(&builder, "solver").await?;
    println!("Solver balance: {} (should be 0)", solver_balance);
    assert_eq!(solver_balance, 0, "Solver should not have received any tokens");

    println!("\n✅ Test passed! Solver borrow exceeding pool size was correctly rejected");
    Ok(())
}
