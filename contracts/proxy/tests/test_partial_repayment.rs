// Test: Partial repayment behavior
// 
// This test documents what happens when a solver repays LESS than the borrowed amount.
// Expected behavior: The contract should either:
// 1. Reject partial repayments (require repayment >= borrow_amount), OR
// 2. Track the remaining debt and allow completion later
//
// Current behavior (to be verified): The contract accepts any repayment amount
// and marks the intent as "returned", potentially causing losses for lenders.

mod helpers;

use helpers::test_builder::{
    deposit_to_vault, get_balance, get_total_assets, get_shares,
    TestScenarioBuilder,
};
use near_api::Data;
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Test: Solver repays less than borrowed amount
/// This documents the current behavior - should the contract reject this?
#[tokio::test]
async fn test_partial_repayment_less_than_principal() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Partial Repayment (less than principal) ===");
    println!("This test documents what happens when solver repays less than borrowed.\n");
    
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
    let lender_shares = deposit_to_vault(&builder, "lender", lender_deposit).await?;
    println!("Lender received {} shares", lender_shares);

    let total_assets_after_deposit = get_total_assets(&builder).await?;
    println!("Total assets after deposit: {}", total_assets_after_deposit);
    assert_eq!(total_assets_after_deposit, lender_deposit);

    // Step 2: Solver borrows entire pool (100 USDC)
    let borrow_amount = lender_deposit;
    println!("\n=== Step 2: Solver borrows {} (entire pool) ===", borrow_amount);
    
    let (solver_id, solver_signer, _) = builder.get_account("solver")
        .ok_or_else(|| "Solver account not found".to_string())?;
    
    let intent_result = builder.vault_contract()
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

    println!("Borrow transaction status: {:?}", intent_result.status);
    sleep(Duration::from_millis(1500)).await;

    let solver_balance_after_borrow = get_balance(&builder, "solver").await?;
    println!("Solver balance after borrow: {}", solver_balance_after_borrow);
    assert_eq!(solver_balance_after_borrow, borrow_amount, "Solver should have borrowed amount");

    let total_assets_after_borrow = get_total_assets(&builder).await?;
    println!("Total assets after borrow: {} (should be 0)", total_assets_after_borrow);
    assert_eq!(total_assets_after_borrow, 0, "All assets should be borrowed");

    // Step 3: Solver attempts to repay only HALF (50 USDC instead of 100 USDC)
    let partial_repayment = borrow_amount / 2; // Only 50 USDC
    println!("\n=== Step 3: Solver attempts partial repayment ===");
    println!("Borrowed: {}", borrow_amount);
    println!("Attempting to repay: {} (only {}%)", partial_repayment, (partial_repayment * 100) / borrow_amount);
    
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

    // Check if partial repayment was accepted or rejected
    match &repay_result {
        Ok(outcome) => {
            let status_str = format!("{:?}", outcome.status);
            println!("\n⚠️  Partial repayment transaction status: {}", status_str);
            
            if status_str.contains("Success") {
                println!("❌ WARNING: Contract ACCEPTED partial repayment!");
                println!("   This means the intent is marked as 'returned' but {} USDC is missing!", 
                    borrow_amount - partial_repayment);
            } else {
                println!("✅ Contract correctly REJECTED partial repayment");
            }
        }
        Err(e) => {
            println!("✅ Contract correctly REJECTED partial repayment with error: {:?}", e);
        }
    }

    sleep(Duration::from_millis(1500)).await;

    // Step 4: Check the state after partial repayment attempt
    println!("\n=== Step 4: Verify state after partial repayment attempt ===");
    
    let total_assets_after_repay = get_total_assets(&builder).await?;
    println!("Total assets after repayment attempt: {}", total_assets_after_repay);
    
    let solver_balance_after_repay = get_balance(&builder, "solver").await?;
    println!("Solver balance after repayment attempt: {}", solver_balance_after_repay);

    // Check intent state
    let intents: Data<Vec<serde_json::Value>> = builder.vault_contract()
        .call_function("get_intents_by_solver", json!({ "solver_id": solver_id }))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;

    if !intents.data.is_empty() {
        let intent = &intents.data[0];
        println!("\nIntent state:");
        println!("  - state: {:?}", intent["state"]);
        println!("  - borrow_amount: {:?}", intent["borrow_amount"]);
        println!("  - repayment_amount: {:?}", intent["repayment_amount"]);
        
        // Check if repayment_amount < borrow_amount
        if let (Some(borrow_str), Some(repay_str)) = (
            intent["borrow_amount"].as_str().or(intent["borrow_amount"].as_u64().map(|n| n.to_string()).as_deref()),
            intent["repayment_amount"].as_str()
        ) {
            let borrow = borrow_str.parse::<u128>().unwrap_or(0);
            let repay = repay_str.parse::<u128>().unwrap_or(0);
            
            if repay < borrow {
                println!("\n❌ POTENTIAL ISSUE: repayment_amount ({}) < borrow_amount ({})", repay, borrow);
                println!("   Missing amount: {} USDC", borrow - repay);
                println!("   Lenders may lose funds if this intent is considered 'complete'!");
            }
        }
    }

    // Step 5: Check if lender can redeem and what they would receive
    println!("\n=== Step 5: Lender attempts to redeem shares ===");
    
    let lender_shares_before_redeem = get_shares(&builder, "lender").await?;
    println!("Lender shares before redeem: {}", lender_shares_before_redeem);
    
    let (lender_id, lender_signer, _) = builder.get_account("lender")
        .ok_or_else(|| "Lender account not found".to_string())?;
    
    let _redeem_result = builder.vault_contract()
        .call_function("redeem", json!({
            "shares": lender_shares_before_redeem.to_string(),
            "receiver_id": lender_id,
            "memo": null
        }))?
        .transaction()
        .deposit(near_api::NearToken::from_yoctonear(1))
        .with_signer(lender_id.clone(), lender_signer.clone())
        .send_to(builder.network_config())
        .await;

    sleep(Duration::from_millis(1500)).await;

    let lender_final_balance = get_balance(&builder, "lender").await?;
    println!("Lender final balance: {}", lender_final_balance);
    println!("Lender original deposit: {}", lender_deposit);
    
    if lender_final_balance < lender_deposit {
        let loss = lender_deposit - lender_final_balance;
        println!("\n❌ LENDER LOSS DETECTED!");
        println!("   Deposited: {}", lender_deposit);
        println!("   Received:  {}", lender_final_balance);
        println!("   Loss:      {} ({:.2}%)", loss, (loss as f64 / lender_deposit as f64) * 100.0);
    } else if lender_final_balance == lender_deposit {
        println!("\n⚠️  Lender received exactly their deposit (no yield)");
    } else {
        println!("\n✅ Lender received deposit + yield: {} (gain: {})", 
            lender_final_balance, lender_final_balance - lender_deposit);
    }

    println!("\n=== Test Summary ===");
    println!("This test documents the current partial repayment behavior.");
    println!("If the contract accepted partial repayment, this may be a security concern.");
    
    Ok(())
}

/// Test: Solver repays exactly the borrowed amount (no yield) - baseline test
#[tokio::test]
async fn test_repayment_exact_principal_no_yield() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Repayment exactly equal to principal (no yield) ===");
    
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

    // Lender deposits
    println!("\n=== Lender deposits {} ===", lender_deposit);
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

    // Solver repays exactly what was borrowed (no yield)
    let repayment = borrow_amount; // Exact principal, no yield
    println!("\n=== Solver repays exactly {} (no yield) ===", repayment);
    
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
            println!("Repayment status: {}", status_str);
            assert!(status_str.contains("Success"), "Exact repayment should succeed");
        }
        Err(e) => {
            panic!("Exact repayment should succeed but got error: {:?}", e);
        }
    }

    sleep(Duration::from_millis(1500)).await;

    // Verify total_assets is restored
    let total_assets_after = get_total_assets(&builder).await?;
    println!("Total assets after repayment: {} (should be {})", total_assets_after, lender_deposit);
    assert_eq!(total_assets_after, lender_deposit, "Total assets should be restored to original deposit");

    println!("\n✅ Test passed! Exact repayment (no yield) works correctly");
    Ok(())
}

