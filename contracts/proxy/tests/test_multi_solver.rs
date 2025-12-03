//! # Multi-Solver Test
//!
//! Tests the scenario where multiple solvers borrow sequentially from the same
//! pool, and the lender receives yield from both borrows when redeeming.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_multi_solver` | Two solvers borrow sequentially, lender queued until both repay | Lender receives deposit + yield from both solvers |
//!
//! ## Lender/Solver Interaction Flow
//!
//! ```text
//! 1. L1 deposits 100 USDC
//! 2. S1 borrows 50 USDC (half the pool)
//! 3. S2 borrows 50 USDC (remaining pool)
//! 4. L1 redeems all shares → QUEUED (no liquidity)
//! 5. S1 repays 50.5 USDC → L1 still queued (needs 101 USDC)
//! 6. S2 repays 50.5 USDC → L1 processed (now has 101 USDC)
//! 7. L1 receives 101 USDC (deposit + 0.5 + 0.5 yield)
//! ```
//!
//! ## Key Verification Points
//!
//! - Redemption waits until sufficient liquidity available
//! - Partial repayment doesn't trigger queue processing
//! - Yield accumulates from multiple solvers
//! - Vault empties after all redemptions

mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Tests yield accumulation from multiple solvers.
///
/// # Scenario
///
/// One lender, two solvers. Each solver borrows half the pool.
/// Lender's redemption is queued until both solvers repay.
///
/// # Expected Outcome
///
/// - L1 receives 100 USDC + 0.5 + 0.5 = 101 USDC
/// - L1's redemption is not processed after S1's repayment (insufficient)
/// - L1's redemption IS processed after S2's repayment (sufficient)
/// - Vault empties completely
#[tokio::test]
async fn test_multi_solver() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;
    println!("=== Test: Multi-solver borrow and repay ===");

    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;

    let (lender1_id, lender1_signer) = create_user_account(&network_config, &genesis_account_id, &genesis_signer, "lender1").await?;
    let (solver1_id, solver1_signer) = create_user_account(&network_config, &genesis_account_id, &genesis_signer, "solver1").await?;
    let (solver2_id, solver2_signer) = create_user_account(&network_config, &genesis_account_id, &genesis_signer, "solver2").await?;

    let ft_contract = Contract(ft_id.clone());
    let vault_contract = Contract(vault_id.clone());

    // Register accounts
    for account_id in [&lender1_id, &solver1_id, &solver2_id] {
        ft_contract
            .call_function("storage_deposit", json!({ "account_id": account_id }))?
            .transaction()
            .deposit(NearToken::from_millinear(10))
            .with_signer(genesis_account_id.clone(), genesis_signer.clone())
            .send_to(&network_config)
            .await?;
    }

    vault_contract
        .call_function("storage_deposit", json!({ "account_id": lender1_id }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(lender1_id.clone(), lender1_signer.clone())
        .send_to(&network_config)
        .await?;

    for (solver_id, solver_signer) in [(&solver1_id, &solver1_signer), (&solver2_id, &solver2_signer)] {
        vault_contract
            .call_function("storage_deposit", json!({ "account_id": solver_id }))?
            .transaction()
            .deposit(NearToken::from_millinear(10))
            .with_signer(solver_id.clone(), solver_signer.clone())
            .send_to(&network_config)
            .await?;
    }

    let lender1_deposit = 100_000_000u128; // 100 USDC

    // =========================================================================
    // STEP 1: L1 DEPOSITS
    // =========================================================================
    println!("\n=== Step 1: L1 deposits {} ===", lender1_deposit);
    
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": lender1_id,
            "amount": lender1_deposit.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": lender1_deposit.to_string(),
            "msg": json!({ "receiver_id": lender1_id }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender1_id.clone(), lender1_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;

    let lender1_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_shares_u128 = lender1_shares.data.parse::<u128>().unwrap();
    println!("L1 deposited {} and received {} shares", lender1_deposit, lender1_shares_u128);

    // =========================================================================
    // STEP 2: S1 BORROWS HALF
    // =========================================================================
    println!("\n=== Step 2: S1 borrows half the liquidity ===");
    let s1_borrow_amount = lender1_deposit / 2; // 50 USDC
    
    let _intent1 = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "intent-s1",
            "_solver_deposit_address": solver1_id,
            "user_deposit_hash": "hash-s1",
            "amount": s1_borrow_amount.to_string()
        }))?
        .transaction()
        .with_signer(solver1_id.clone(), solver1_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("S1 borrowed {} via new_intent", s1_borrow_amount);

    sleep(Duration::from_millis(2000)).await;

    let total_assets_after_s1: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_after_s1_u128 = total_assets_after_s1.data.parse::<u128>().unwrap();
    println!("Total assets after S1 borrow: {} (should be {})", 
        total_assets_after_s1_u128, lender1_deposit - s1_borrow_amount);
    assert_eq!(total_assets_after_s1_u128, lender1_deposit - s1_borrow_amount, 
        "Half liquidity should remain");

    // =========================================================================
    // STEP 3: S2 BORROWS OTHER HALF
    // =========================================================================
    println!("\n=== Step 3: S2 borrows the other half of the liquidity ===");
    let s2_borrow_amount = total_assets_after_s1_u128;
    
    let _intent2 = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "intent-s2",
            "_solver_deposit_address": solver2_id,
            "user_deposit_hash": "hash-s2",
            "amount": s2_borrow_amount.to_string()
        }))?
        .transaction()
        .with_signer(solver2_id.clone(), solver2_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("S2 borrowed {} via new_intent", s2_borrow_amount);

    sleep(Duration::from_millis(2000)).await;

    let total_assets_after_s2: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_after_s2_u128 = total_assets_after_s2.data.parse::<u128>().unwrap();
    println!("Total assets after S2 borrow: {} (should be 0)", total_assets_after_s2_u128);
    assert_eq!(total_assets_after_s2_u128, 0, "All liquidity should be borrowed");

    // =========================================================================
    // STEP 4: L1 REDEEMS (QUEUED)
    // =========================================================================
    println!("\n=== Step 4: L1 redeems all their shares (queued) ===");
    
    vault_contract
        .call_function("redeem", json!({
            "shares": lender1_shares_u128.to_string(),
            "receiver_id": lender1_id
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender1_id.clone(), lender1_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;

    let pending_redemptions: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions: {} (should be 1)", pending_redemptions.data.len());
    assert_eq!(pending_redemptions.data.len(), 1, "L1's redemption should be queued");
    
    let stored_shares = pending_redemptions.data[0]["shares"].as_str().unwrap_or("?");
    let stored_assets = pending_redemptions.data[0]["assets"].as_str().unwrap_or("?");
    println!("  L1 redemption queued: shares={}, stored_assets={}", stored_shares, stored_assets);

    // Calculate expected assets
    let s1_yield = s1_borrow_amount / 100;
    let s2_yield = s2_borrow_amount / 100;
    let total_yield = s1_yield + s2_yield;
    let expected_assets = lender1_deposit + total_yield;
    println!("Expected assets: {} (deposit {} + yield {} from S1 + yield {} from S2)", 
        expected_assets, lender1_deposit, s1_yield, s2_yield);

    // =========================================================================
    // STEP 5: S1 REPAYS - L1 STILL QUEUED (INSUFFICIENT)
    // =========================================================================
    println!("\n=== Step 5: S1 repays (L1 not processed - insufficient liquidity) ===");
    
    let s1_repayment = s1_borrow_amount + s1_yield;
    
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": solver1_id,
            "amount": s1_yield.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": s1_repayment.to_string(),
            "msg": json!({ "repay": { "intent_index": "0" } }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver1_id.clone(), solver1_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("S1 repaid {} (borrow {} + yield {})", s1_repayment, s1_borrow_amount, s1_yield);

    sleep(Duration::from_millis(2000)).await;

    let total_assets_after_s1_repay: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_after_s1_repay_u128 = total_assets_after_s1_repay.data.parse::<u128>().unwrap();
    println!("Total assets after S1 repay: {} (L1 needs {})", 
        total_assets_after_s1_repay_u128, expected_assets);

    // Try to process - should NOT work
    vault_contract
        .call_function("process_next_redemption", json!([]))?
        .transaction()
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;

    let pending_after_s1_repay: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after S1 repay: {} (should still be 1)", 
        pending_after_s1_repay.data.len());
    assert_eq!(pending_after_s1_repay.data.len(), 1, 
        "L1 should still be in queue - not enough liquidity");

    let l1_usdc_after_s1: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let l1_usdc_after_s1_u128 = l1_usdc_after_s1.data.parse::<u128>().unwrap();
    println!("L1 USDC balance after S1 repay: {} (should be 0)", l1_usdc_after_s1_u128);
    assert_eq!(l1_usdc_after_s1_u128, 0, "L1 should not have received USDC yet");

    // =========================================================================
    // STEP 6: S2 REPAYS - L1 PROCESSED (SUFFICIENT NOW)
    // =========================================================================
    println!("\n=== Step 6: S2 repays (L1 processed - sufficient liquidity) ===");
    
    let s2_repayment = s2_borrow_amount + s2_yield;
    
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": solver2_id,
            "amount": s2_yield.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": s2_repayment.to_string(),
            "msg": json!({ "repay": { "intent_index": "1" } }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver2_id.clone(), solver2_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("S2 repaid {} (borrow {} + yield {})", s2_repayment, s2_borrow_amount, s2_yield);

    sleep(Duration::from_millis(2000)).await;

    let total_assets_after_s2_repay: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_after_s2_repay_u128 = total_assets_after_s2_repay.data.parse::<u128>().unwrap();
    println!("Total assets after S2 repay: {} (L1 needs {})", 
        total_assets_after_s2_repay_u128, expected_assets);

    // Process - should work now
    vault_contract
        .call_function("process_next_redemption", json!([]))?
        .transaction()
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(2000)).await;

    let pending_after_s2_repay: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after S2 repay and processing: {} (should be 0)", 
        pending_after_s2_repay.data.len());
    assert_eq!(pending_after_s2_repay.data.len(), 0, 
        "L1 should be processed now");

    // =========================================================================
    // FINAL VERIFICATION
    // =========================================================================
    println!("\n=== Final Verification ===");
    let l1_final_balance: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let l1_final_balance_u128 = l1_final_balance.data.parse::<u128>().unwrap();
    
    println!("L1 final USDC balance: {}", l1_final_balance_u128);
    println!("Expected: {} (deposit {} + total yield {})", expected_assets, lender1_deposit, total_yield);
    println!("  Yield from S1's borrow: {} (1% of {})", s1_yield, s1_borrow_amount);
    println!("  Yield from S2's borrow: {} (1% of {})", s2_yield, s2_borrow_amount);
    
    assert_eq!(l1_final_balance_u128, expected_assets, 
        "L1 should receive deposit + 1% yield from both solvers");

    // Verify vault is empty
    let total_assets_final: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Vault total_assets: {} (should be 0)", total_assets_final.data);
    assert_eq!(total_assets_final.data, "0", "Vault should be empty");

    let total_shares_final: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Vault total_shares: {} (should be 0)", total_shares_final.data);
    assert_eq!(total_shares_final.data, "0", "All shares should be burned");

    println!("\n✅ Test passed!");
    println!("Summary:");
    println!("  L1 deposited: {}", lender1_deposit);
    println!("  S1 borrowed: {} (repaid with {} yield)", s1_borrow_amount, s1_yield);
    println!("  S2 borrowed: {} (repaid with {} yield)", s2_borrow_amount, s2_yield);
    println!("  L1 received: {} (deposit + {}% yield)", l1_final_balance_u128, 
        (total_yield * 100) / lender1_deposit);
    println!("  Note: L1's redemption was queued until both solvers repaid.");

    Ok(())
}
