// Test: Repayment validation
// 
// These tests verify that the contract correctly validates repayment amounts.
// The contract requires: repayment >= borrow_amount + expected_yield (1%)
//
// Test cases:
// 1. Partial repayment (50% of principal) → Should FAIL
// 2. Exact principal (100%, no yield) → Should FAIL  
// 3. Principal + yield (101%) → Should SUCCEED

mod helpers;

use helpers::test_builder::{
    deposit_to_vault, get_balance, get_total_assets,
    TestScenarioBuilder,
};
use near_api::Data;
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Test: Solver tries to repay less than borrowed amount (50%)
/// This should FAIL - partial repayments are not allowed
#[tokio::test]
async fn test_partial_repayment_less_than_principal() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Partial Repayment (50% of principal) - Should FAIL ===\n");
    
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

    let lender_deposit = 100_000_000u128; // 100 USDC

    // Step 1: Lender deposits 100 USDC
    println!("=== Step 1: Lender deposits {} ===", lender_deposit);
    let _lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;

    let total_assets_after_deposit = get_total_assets(&builder).await?;
    println!("Total assets after deposit: {}", total_assets_after_deposit);
    assert_eq!(total_assets_after_deposit, lender_deposit);

    // Step 2: Solver borrows entire pool (100 USDC)
    let borrow_amount = lender_deposit;
    println!("\n=== Step 2: Solver borrows {} (entire pool) ===", borrow_amount);
    
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
    let _intent_result = builder.vault_contract()
        .call_function("new_intent", json!({
            "intent_data": "intent-partial-test",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-partial-test",
            "amount": borrow_amount.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(builder.network_config())
        .await?;

    sleep(Duration::from_millis(1500)).await;

    let solver_balance_after_borrow = get_balance(&builder, "solver").await?;
    println!("Solver balance after borrow: {}", solver_balance_after_borrow);
    assert_eq!(solver_balance_after_borrow, borrow_amount);

    let total_assets_after_borrow = get_total_assets(&builder).await?;
    println!("Total assets after borrow: {} (should be 0)", total_assets_after_borrow);
    assert_eq!(total_assets_after_borrow, 0);

    // Step 3: Solver attempts to repay only HALF (50 USDC instead of 101 USDC minimum)
    let partial_repayment = borrow_amount / 2; // Only 50 USDC
    let expected_yield = borrow_amount / 100; // 1% = 1 USDC
    let minimum_required = borrow_amount + expected_yield; // 101 USDC
    
    println!("\n=== Step 3: Solver attempts partial repayment ===");
    println!("Borrowed: {}", borrow_amount);
    println!("Expected yield (1%): {}", expected_yield);
    println!("Minimum required: {} (principal + yield)", minimum_required);
    println!("Attempting to repay: {} ({}% of minimum)", partial_repayment, (partial_repayment * 100) / minimum_required);
    
    let repay_result = builder.ft_contract()
        .call_function("ft_transfer_call", json!({
            "receiver_id": builder.vault_id(),
            "amount": partial_repayment.to_string(),
            "msg": json!({ "repay": { "intent_index": "0" } }).to_string()
        }))?
        .transaction()
        .deposit(near_api::NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(builder.network_config())
        .await;

    // Verify the transaction failed
    match &repay_result {
        Ok(outcome) => {
            let status_str = format!("{:?}", outcome.status);
            println!("\nTransaction status: {}", status_str);
            assert!(
                status_str.contains("Failure") || status_str.contains("Repayment") && status_str.contains("less than minimum"),
                "Partial repayment should fail but got: {}", status_str
            );
            println!("✅ Contract correctly REJECTED partial repayment");
        }
        Err(e) => {
            let error_str = format!("{:?}", e);
            println!("✅ Contract correctly REJECTED partial repayment with error: {}", error_str);
        }
    }

    sleep(Duration::from_millis(1000)).await;

    // Step 4: Verify state unchanged after failed repayment
    println!("\n=== Step 4: Verify state unchanged ===");
    
    let total_assets_after_failed_repay = get_total_assets(&builder).await?;
    println!("Total assets after failed repayment: {} (should still be 0)", total_assets_after_failed_repay);
    assert_eq!(total_assets_after_failed_repay, 0, "Total assets should remain 0 after failed repayment");

    // Check intent state - should still be borrowed
    let intents: Data<Vec<serde_json::Value>> = builder.vault_contract()
        .call_function("get_intents_by_solver", json!({ "solver_id": solver_id }))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;

    if !intents.data.is_empty() {
        let intent = &intents.data[0];
        let state = intent["state"].as_str().unwrap_or("");
        println!("Intent state: {} (should be 'StpLiquidityBorrowed')", state);
        assert_eq!(state, "StpLiquidityBorrowed", "Intent should still be in borrowed state");
    }

    println!("\n✅ Test passed! Partial repayment correctly rejected, lenders protected");
    Ok(())
}

/// Test: Solver tries to repay exactly the principal (no yield)
/// This should FAIL - must include 1% yield
#[tokio::test]
async fn test_repayment_exact_principal_no_yield() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Repayment exactly equal to principal (no yield) - Should FAIL ===\n");
    
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

    let lender_deposit = 100_000_000u128; // 100 USDC

    // Lender deposits
    println!("=== Lender deposits {} ===", lender_deposit);
    let _lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;

    // Solver borrows
    let borrow_amount = lender_deposit;
    println!("\n=== Solver borrows {} ===", borrow_amount);
    
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
    let _intent_result = builder.vault_contract()
        .call_function("new_intent", json!({
            "intent_data": "intent-exact-repay",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-exact-repay",
            "amount": borrow_amount.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(builder.network_config())
        .await?;

    sleep(Duration::from_millis(1500)).await;

    // Solver tries to repay exactly what was borrowed (no yield)
    let repayment = borrow_amount; // Exact principal, no yield
    let expected_yield = borrow_amount / 100; // 1% = 1 USDC
    let minimum_required = borrow_amount + expected_yield;
    
    println!("\n=== Solver attempts to repay exactly {} (no yield) ===", repayment);
    println!("Minimum required: {} (principal {} + yield {})", minimum_required, borrow_amount, expected_yield);
    println!("Shortfall: {}", minimum_required - repayment);
    
    let repay_result = builder.ft_contract()
        .call_function("ft_transfer_call", json!({
            "receiver_id": builder.vault_id(),
            "amount": repayment.to_string(),
            "msg": json!({ "repay": { "intent_index": "0" } }).to_string()
        }))?
        .transaction()
        .deposit(near_api::NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(builder.network_config())
        .await;

    // Verify the transaction failed
    match &repay_result {
        Ok(outcome) => {
            let status_str = format!("{:?}", outcome.status);
            println!("\nTransaction status: {}", status_str);
            assert!(
                status_str.contains("Failure") || status_str.contains("less than minimum"),
                "Repayment without yield should fail but got: {}", status_str
            );
            println!("✅ Contract correctly REJECTED repayment without yield");
        }
        Err(e) => {
            let error_str = format!("{:?}", e);
            println!("✅ Contract correctly REJECTED repayment without yield: {}", error_str);
        }
    }

    sleep(Duration::from_millis(1000)).await;

    // Verify state unchanged
    let total_assets_after = get_total_assets(&builder).await?;
    println!("\nTotal assets after failed repayment: {} (should be 0)", total_assets_after);
    assert_eq!(total_assets_after, 0, "Total assets should remain 0");

    println!("\n✅ Test passed! Repayment without yield correctly rejected");
    Ok(())
}

/// Test: Solver repays principal + yield (correct amount)
/// This should SUCCEED
#[tokio::test]
async fn test_repayment_with_yield() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Repayment with yield (principal + 1%) - Should SUCCEED ===\n");
    
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

    let lender_deposit = 100_000_000u128; // 100 USDC

    // Lender deposits
    println!("=== Lender deposits {} ===", lender_deposit);
    let _lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;

    // Solver borrows
    let borrow_amount = lender_deposit;
    println!("\n=== Solver borrows {} ===", borrow_amount);
    
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
    let _intent_result = builder.vault_contract()
        .call_function("new_intent", json!({
            "intent_data": "intent-with-yield",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-with-yield",
            "amount": borrow_amount.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(builder.network_config())
        .await?;

    sleep(Duration::from_millis(1500)).await;

    let solver_balance_after_borrow = get_balance(&builder, "solver").await?;
    println!("Solver balance after borrow: {}", solver_balance_after_borrow);

    // Solver needs extra tokens for the yield - transfer from genesis
    let expected_yield = borrow_amount / 100; // 1% = 1,000,000 (1 USDC)
    let repayment = borrow_amount + expected_yield; // 101,000,000 (101 USDC)
    
    println!("\n=== Transfer yield tokens to solver ===");
    println!("Yield amount: {}", expected_yield);
    
    builder.ft_contract()
        .call_function("ft_transfer", json!({
            "receiver_id": solver_id,
            "amount": expected_yield.to_string()
        }))?
        .transaction()
        .deposit(near_api::NearToken::from_yoctonear(1))
        .with_signer(builder.genesis_account_id().clone(), builder.genesis_signer().clone())
        .send_to(builder.network_config())
        .await?;

    sleep(Duration::from_millis(500)).await;

    let solver_balance_with_yield = get_balance(&builder, "solver").await?;
    println!("Solver balance with yield: {} (should be {})", solver_balance_with_yield, repayment);
    assert_eq!(solver_balance_with_yield, repayment);

    // Solver repays principal + yield
    println!("\n=== Solver repays {} (principal {} + yield {}) ===", repayment, borrow_amount, expected_yield);
    
    let repay_result = builder.ft_contract()
        .call_function("ft_transfer_call", json!({
            "receiver_id": builder.vault_id(),
            "amount": repayment.to_string(),
            "msg": json!({ "repay": { "intent_index": "0" } }).to_string()
        }))?
        .transaction()
        .deposit(near_api::NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(builder.network_config())
        .await;

    // Verify the transaction succeeded
    match &repay_result {
        Ok(outcome) => {
            let status_str = format!("{:?}", outcome.status);
            println!("\nTransaction status: {}", status_str);
            assert!(
                status_str.contains("Success"),
                "Repayment with yield should succeed but got: {}", status_str
            );
            println!("✅ Repayment with yield accepted");
        }
        Err(e) => {
            panic!("Repayment with yield should succeed but got error: {:?}", e);
        }
    }

    sleep(Duration::from_millis(1500)).await;

    // Verify total_assets is restored with yield
    let total_assets_after = get_total_assets(&builder).await?;
    println!("\nTotal assets after repayment: {} (should be {})", total_assets_after, repayment);
    assert_eq!(total_assets_after, repayment, "Total assets should include principal + yield");

    // Verify intent state is returned
    let intents: Data<Vec<serde_json::Value>> = builder.vault_contract()
        .call_function("get_intents_by_solver", json!({ "solver_id": solver_id }))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;

    if !intents.data.is_empty() {
        let intent = &intents.data[0];
        let state = intent["state"].as_str().unwrap_or("");
        println!("Intent state: {} (should be 'StpLiquidityReturned')", state);
        assert_eq!(state, "StpLiquidityReturned", "Intent should be in returned state");
        
        let repayment_amount = intent["repayment_amount"].as_str().unwrap_or("0");
        println!("Repayment amount recorded: {}", repayment_amount);
    }

    // Verify solver balance is 0 after repayment
    let solver_balance_final = get_balance(&builder, "solver").await?;
    println!("Solver final balance: {} (should be 0)", solver_balance_final);
    assert_eq!(solver_balance_final, 0, "Solver should have used all tokens for repayment");

    println!("\n✅ Test passed! Repayment with yield correctly accepted, lenders protected");
    Ok(())
}

/// Test: Solver repays MORE than minimum (extra yield)
/// This should SUCCEED - extra yield benefits lenders
#[tokio::test]
async fn test_repayment_with_extra_yield() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Repayment with extra yield (principal + 5%) - Should SUCCEED ===\n");
    
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

    let lender_deposit = 100_000_000u128; // 100 USDC

    // Lender deposits
    println!("=== Lender deposits {} ===", lender_deposit);
    let _lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;

    // Solver borrows
    let borrow_amount = lender_deposit;
    println!("\n=== Solver borrows {} ===", borrow_amount);
    
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
    let _intent_result = builder.vault_contract()
        .call_function("new_intent", json!({
            "intent_data": "intent-extra-yield",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-extra-yield",
            "amount": borrow_amount.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(builder.network_config())
        .await?;

    sleep(Duration::from_millis(1500)).await;

    // Solver pays extra yield (5% instead of minimum 1%)
    let extra_yield = borrow_amount / 20; // 5% = 5,000,000 (5 USDC)
    let repayment = borrow_amount + extra_yield; // 105,000,000 (105 USDC)
    
    println!("\n=== Transfer extra yield tokens to solver ===");
    println!("Extra yield amount: {} (5%)", extra_yield);
    
    builder.ft_contract()
        .call_function("ft_transfer", json!({
            "receiver_id": solver_id,
            "amount": extra_yield.to_string()
        }))?
        .transaction()
        .deposit(near_api::NearToken::from_yoctonear(1))
        .with_signer(builder.genesis_account_id().clone(), builder.genesis_signer().clone())
        .send_to(builder.network_config())
        .await?;

    sleep(Duration::from_millis(500)).await;

    // Solver repays principal + extra yield
    println!("\n=== Solver repays {} (principal {} + extra yield {}) ===", repayment, borrow_amount, extra_yield);
    
    let repay_result = builder.ft_contract()
        .call_function("ft_transfer_call", json!({
            "receiver_id": builder.vault_id(),
            "amount": repayment.to_string(),
            "msg": json!({ "repay": { "intent_index": "0" } }).to_string()
        }))?
        .transaction()
        .deposit(near_api::NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(builder.network_config())
        .await;

    // Verify the transaction succeeded
    match &repay_result {
        Ok(outcome) => {
            let status_str = format!("{:?}", outcome.status);
            println!("\nTransaction status: {}", status_str);
            assert!(
                status_str.contains("Success"),
                "Repayment with extra yield should succeed but got: {}", status_str
            );
            println!("✅ Repayment with extra yield accepted");
        }
        Err(e) => {
            panic!("Repayment with extra yield should succeed but got error: {:?}", e);
        }
    }

    sleep(Duration::from_millis(1500)).await;

    // Verify total_assets includes extra yield
    let total_assets_after = get_total_assets(&builder).await?;
    println!("\nTotal assets after repayment: {} (should be {})", total_assets_after, repayment);
    assert_eq!(total_assets_after, repayment, "Total assets should include principal + extra yield");

    println!("\n✅ Test passed! Extra yield benefits lenders");
    Ok(())
}
