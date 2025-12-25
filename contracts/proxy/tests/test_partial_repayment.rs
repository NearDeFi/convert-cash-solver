//! # Partial Repayment Validation Tests
//!
//! Tests that the contract correctly validates solver repayment amounts.
//! Solvers must repay at least principal + 1% yield to protect lenders.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_partial_repayment_less_than_principal` | Solver repays 50% of principal | Rejected, state unchanged |
//! | `test_repayment_exact_principal_no_yield` | Solver repays 100% (no yield) | Rejected, state unchanged |
//! | `test_repayment_with_yield` | Solver repays 101% (1% yield) | Accepted, state updated |
//! | `test_repayment_with_extra_yield` | Solver repays 105% (5% yield) | Accepted, extra goes to lenders |
//!
//! ## Repayment Validation Rules
//!
//! ```text
//! Minimum Required = borrow_amount + (borrow_amount / 100)
//!                  = borrow_amount * 1.01
//!
//! - 50% of principal → FAIL (50,000,000 < 101,000,000)
//! - 100% of principal → FAIL (100,000,000 < 101,000,000)
//! - 101% of principal → PASS (101,000,000 >= 101,000,000)
//! - 105% of principal → PASS (105,000,000 >= 101,000,000)
//! ```
//!
//! ## Key Verification Points
//!
//! - Failed repayments revert via ft_resolve_transfer (tokens returned to solver)
//! - State remains unchanged after failed repayment
//! - Intent stays in "StpLiquidityBorrowed" state until valid repayment
//! - Extra yield beyond 1% benefits lenders

mod helpers;

use helpers::test_builder::{
    deposit_to_vault, get_balance, get_total_assets,
    TestScenarioBuilder,
};
use near_api::Data;
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Tests that repayment of less than principal is rejected.
///
/// # Scenario
///
/// Solver borrows 100 USDC, attempts to repay only 50 USDC.
///
/// # Expected Outcome
///
/// - Transaction fails or tokens are returned
/// - Total assets remains 0 (borrow not repaid)
/// - Solver retains their 100 USDC (returned via ft_resolve_transfer)
/// - Intent state remains "StpLiquidityBorrowed"
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

    // Lender deposits
    println!("=== Step 1: Lender deposits {} ===", lender_deposit);
    let _lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;

    let total_assets_after_deposit = get_total_assets(&builder).await?;
    println!("Total assets after deposit: {}", total_assets_after_deposit);
    assert_eq!(total_assets_after_deposit, lender_deposit);

    // Solver borrows entire pool
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

    // =========================================================================
    // SOLVER ATTEMPTS PARTIAL REPAYMENT (50%)
    // =========================================================================
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

    match &repay_result {
        Ok(outcome) => {
            let status_str = format!("{:?}", outcome.status);
            println!("\nTransaction status: {}", status_str);
        }
        Err(e) => {
            println!("Transaction error (expected): {:?}", e);
        }
    }

    sleep(Duration::from_millis(1500)).await;

    // =========================================================================
    // VERIFY STATE UNCHANGED
    // =========================================================================
    println!("\n=== Step 4: Verify state unchanged (repayment was rejected) ===");
    
    let total_assets_after_failed_repay = get_total_assets(&builder).await?;
    println!("Total assets after failed repayment: {} (should still be 0)", total_assets_after_failed_repay);
    assert_eq!(total_assets_after_failed_repay, 0, "Total assets should remain 0 after failed repayment");

    let solver_balance_after_failed_repay = get_balance(&builder, "solver").await?;
    println!("Solver balance after failed repayment: {} (should still be {})", 
        solver_balance_after_failed_repay, borrow_amount);
    assert_eq!(solver_balance_after_failed_repay, borrow_amount, 
        "Solver should still have their tokens after failed repayment");

    let intents: Data<Vec<serde_json::Value>> = builder.vault_contract()
        .call_function("get_intents_by_solver", json!({ "solver_id": solver_id }))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;

    if !intents.data.is_empty() {
        let indexed_intent = &intents.data[0];
        let state = indexed_intent["intent"]["state"].as_str().unwrap_or("");
        println!("Intent state: {} (should be 'StpLiquidityBorrowed')", state);
        assert_eq!(state, "StpLiquidityBorrowed", "Intent should still be in borrowed state");
        println!("✅ Contract correctly REJECTED partial repayment - state unchanged");
    }

    println!("\n✅ Test passed! Partial repayment correctly rejected, lenders protected");
    Ok(())
}

/// Tests that repayment of exact principal (no yield) is rejected.
///
/// # Scenario
///
/// Solver borrows 100 USDC, attempts to repay exactly 100 USDC (no yield).
///
/// # Expected Outcome
///
/// - Transaction fails (shortfall of 1 USDC yield)
/// - State unchanged
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

    let lender_deposit = 100_000_000u128;

    println!("=== Lender deposits {} ===", lender_deposit);
    let _lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;

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
    let repayment = borrow_amount;
    let expected_yield = borrow_amount / 100;
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

    match &repay_result {
        Ok(outcome) => {
            let status_str = format!("{:?}", outcome.status);
            println!("\nTransaction status: {}", status_str);
        }
        Err(e) => {
            println!("Transaction error (expected): {:?}", e);
        }
    }

    sleep(Duration::from_millis(1500)).await;

    // Verify state unchanged
    let total_assets_after = get_total_assets(&builder).await?;
    println!("\nTotal assets after failed repayment: {} (should be 0)", total_assets_after);
    assert_eq!(total_assets_after, 0, "Total assets should remain 0");

    let solver_balance_after = get_balance(&builder, "solver").await?;
    println!("Solver balance after failed repayment: {} (should be {})", solver_balance_after, borrow_amount);
    assert_eq!(solver_balance_after, borrow_amount, "Solver should still have tokens");

    println!("\n✅ Test passed! Repayment without yield correctly rejected");
    Ok(())
}

/// Tests that repayment with 1% yield is accepted.
///
/// # Scenario
///
/// Solver borrows 100 USDC, repays 101 USDC (principal + 1% yield).
///
/// # Expected Outcome
///
/// - Transaction succeeds
/// - Total assets = 101 USDC
/// - Intent state = "StpLiquidityReturned"
/// - Solver balance = 0
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

    let lender_deposit = 100_000_000u128;

    println!("=== Lender deposits {} ===", lender_deposit);
    let _lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;

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

    // Transfer yield tokens to solver
    let expected_yield = borrow_amount / 100; // 1% = 1,000,000
    let repayment = borrow_amount + expected_yield; // 101,000,000
    
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

    // Verify state updated
    let total_assets_after = get_total_assets(&builder).await?;
    println!("\nTotal assets after repayment: {} (should be {})", total_assets_after, repayment);
    assert_eq!(total_assets_after, repayment, "Total assets should include principal + yield");

    // After successful repayment, intents are deleted
    let intents: Data<Vec<serde_json::Value>> = builder.vault_contract()
        .call_function("get_intents", json!({}))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    
    println!("Intents after repayment: {} (should be 0 - intent deleted)", intents.data.len());
    assert!(intents.data.is_empty(), "Intent should be deleted after repayment");

    let solver_balance_final = get_balance(&builder, "solver").await?;
    println!("Solver final balance: {} (should be 0)", solver_balance_final);
    assert_eq!(solver_balance_final, 0, "Solver should have used all tokens for repayment");

    println!("\n✅ Test passed! Repayment with yield correctly accepted, lenders protected");
    Ok(())
}

/// Tests that repayment with extra yield (5%) is accepted.
///
/// # Scenario
///
/// Solver borrows 100 USDC, repays 105 USDC (principal + 5% yield).
///
/// # Expected Outcome
///
/// - Transaction succeeds
/// - Total assets = 105 USDC (extra yield benefits lenders)
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

    let lender_deposit = 100_000_000u128;

    println!("=== Lender deposits {} ===", lender_deposit);
    let _lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;

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
    let extra_yield = borrow_amount / 20; // 5% = 5,000,000
    let repayment = borrow_amount + extra_yield; // 105,000,000
    
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
