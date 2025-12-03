//! # Solver Borrow Exact Pool Size Test
//!
//! Tests the edge case where a solver borrows exactly all available liquidity
//! (total_assets). This is a valid operation and should succeed.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_solver_borrow_exact_pool_size` | Solver borrows exactly total_assets | Transaction succeeds, vault emptied |
//!
//! ## Lender/Solver Interaction
//!
//! ```text
//! 1. Lender deposits 50 USDC
//! 2. Solver borrows exactly 50 USDC (100% of pool)
//! 3. Vault total_assets = 0
//! 4. Solver balance = 50 USDC
//! ```
//!
//! ## Key Verification Points
//!
//! - Transaction succeeds with exact pool size borrow
//! - Vault total_assets becomes exactly 0
//! - Solver receives the full borrowed amount
//! - Any redemption requests will be queued (no liquidity)

mod helpers;

use helpers::test_builder::{
    deposit_to_vault, get_balance, get_total_assets,
    TestScenarioBuilder,
};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Tests that borrowing exactly total_assets succeeds.
///
/// # Scenario
///
/// 1. Lender deposits 50 USDC
/// 2. Solver borrows exactly 50 USDC (entire pool)
///
/// # Expected Outcome
///
/// - Transaction succeeds
/// - Vault total_assets = 0
/// - Solver receives full 50 USDC
#[tokio::test]
async fn test_solver_borrow_exact_pool_size() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Solver borrow exact pool size - Should SUCCEED ===");
    
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
    // SOLVER BORROWS EXACTLY TOTAL_ASSETS
    // =========================================================================
    let exact_borrow = lender_deposit; // Exactly 50,000,000
    println!("\n=== Step 2: Solver borrows exactly {} (entire pool) ===", exact_borrow);
    
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
    let intent_result = builder.vault_contract()
        .call_function("new_intent", json!({
            "intent_data": "intent-exact",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-exact",
            "amount": exact_borrow.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(builder.network_config())
        .await;

    // Verify success
    match &intent_result {
        Ok(outcome) => {
            let status_str = format!("{:?}", outcome.status);
            println!("Transaction status: {}", status_str);
            assert!(
                status_str.contains("Success"),
                "Transaction should have succeeded but got: {}", status_str
            );
            println!("✅ Transaction succeeded with status: {}", status_str);
        }
        Err(e) => {
            panic!("Transaction should have succeeded but got error: {:?}", e);
        }
    }

    sleep(Duration::from_millis(1500)).await;

    // =========================================================================
    // VERIFY VAULT EMPTIED
    // =========================================================================
    let total_assets_after = get_total_assets(&builder).await?;
    
    println!("Total assets after exact borrow: {} (should be 0)", total_assets_after);
    assert_eq!(total_assets_after, 0, "All assets should be borrowed");

    let solver_balance = get_balance(&builder, "solver").await?;
    println!("Solver balance: {} (should be {})", solver_balance, exact_borrow);
    assert_eq!(solver_balance, exact_borrow, "Solver should have received exact borrow amount");

    println!("\n✅ Test passed! Solver successfully borrowed entire pool");
    Ok(())
}
