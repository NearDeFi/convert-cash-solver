// Test: Solver borrows exactly total_assets (entire pool)
// This should succeed - edge case where solver takes all available liquidity

mod helpers;

use helpers::test_builder::{
    deposit_to_vault, get_balance, get_total_assets,
    TestScenarioBuilder,
};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Test edge case: Solver borrows exactly total_assets (should succeed)
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

    // Step 1: Lender deposits 50 USDC
    println!("\n=== Step 1: Lender deposits {} ===", lender_deposit);
    let _lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;

    // Verify total_assets
    let total_assets_before = get_total_assets(&builder).await?;
    println!("Total assets in pool: {}", total_assets_before);
    assert_eq!(total_assets_before, lender_deposit, "Total assets should equal deposit");

    // Step 2: Solver borrows EXACTLY total_assets (should SUCCEED)
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

    // Verify the transaction succeeded
    match &intent_result {
        Ok(outcome) => {
            let status_str = format!("{:?}", outcome.status);
            println!("Transaction status: {}", status_str);
            // Check if status indicates success
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

    // Verify total_assets is now 0
    let total_assets_after = get_total_assets(&builder).await?;
    
    println!("Total assets after exact borrow: {} (should be 0)", total_assets_after);
    assert_eq!(total_assets_after, 0, "All assets should be borrowed");

    // Verify solver received the tokens
    let solver_balance = get_balance(&builder, "solver").await?;
    println!("Solver balance: {} (should be {})", solver_balance, exact_borrow);
    assert_eq!(solver_balance, exact_borrow, "Solver should have received exact borrow amount");

    println!("\n✅ Test passed! Solver successfully borrowed entire pool");
    Ok(())
}
