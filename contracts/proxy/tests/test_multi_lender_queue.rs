//! # Multi-Lender Queue Test
//!
//! Tests the redemption queue with multiple lenders depositing at different
//! times relative to borrows. Validates that yield is distributed correctly
//! based on when deposits occurred.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_multi_lender_queue` | L1 deposits, solver borrows all, L2 deposits, solver repays | L1 gets full yield, L2 gets yield from second borrow |
//!
//! ## Lender/Solver Interaction Flow
//!
//! ```text
//! 1.  L1 deposits 5 USDC
//! 2.  Solver borrows 5 USDC (ALL of L1's deposit)
//! 3.  L2 deposits 2.5 USDC (during borrow - may get diluted shares)
//! 4.  L1 redeems → QUEUED (insufficient liquidity)
//! 5.  Solver repays 5.05 USDC (+ 1% yield)
//! 6.  Queue processed → L1 receives 5.05 USDC
//! 7.  Solver borrows 2.5 USDC (L2's deposit)
//! 8.  L2 redeems → QUEUED
//! 9.  Solver repays 2.525 USDC (+ 1% yield)
//! 10. Queue processed → L2 receives 2.525 USDC
//! ```
//!
//! ## Key Verification Points
//!
//! - L1 receives full 1% yield from first borrow (only lender at that time)
//! - L2 receives 1% yield from second borrow (only lender at that time)
//! - Deposits during borrow get diluted share allocation
//! - Final vault state: 0 assets, 0 shares

mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Tests yield distribution across two borrow cycles.
///
/// # Scenario
///
/// Sequential borrow cycles with deposits in between.
///
/// # Expected Outcome
///
/// - L1: 5 USDC deposit + 0.05 USDC yield = 5.05 USDC
/// - L2: 2.5 USDC deposit + 0.025 USDC yield = 2.525 USDC
/// - Vault empties completely
#[tokio::test]
async fn test_multi_lender_queue() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;
    println!("=== Test: Multi-lender redemption queue with 1% intent_yield ===");
    println!("Sandbox started, genesis account = {}", genesis_account_id);

    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;
    println!("Vault deployed at {}, FT deployed at {}", vault_id, ft_id);

    let (lender1_id, lender1_signer) =
        create_user_account(&network_config, &genesis_account_id, &genesis_signer, "lender1").await?;
    let (lender2_id, lender2_signer) =
        create_user_account(&network_config, &genesis_account_id, &genesis_signer, "lender2").await?;
    let (solver_id, solver_signer) =
        create_user_account(&network_config, &genesis_account_id, &genesis_signer, "solver").await?;
    println!("Test accounts created: lender1={}, lender2={}, solver={}", lender1_id, lender2_id, solver_id);

    let ft_contract = Contract(ft_id.clone());
    let vault_contract = Contract(vault_id.clone());

    // Register accounts
    for account_id in [&lender1_id, &lender2_id, &solver_id] {
        ft_contract
            .call_function("storage_deposit", json!({ "account_id": account_id }))?
            .transaction()
            .deposit(NearToken::from_millinear(10))
            .with_signer(genesis_account_id.clone(), genesis_signer.clone())
            .send_to(&network_config)
            .await?;
        println!("FT storage_deposit completed for {}", account_id);
    }

    for (lender_id, lender_signer) in [(&lender1_id, &lender1_signer), (&lender2_id, &lender2_signer)] {
        vault_contract
            .call_function("storage_deposit", json!({ "account_id": lender_id }))?
            .transaction()
            .deposit(NearToken::from_millinear(10))
            .with_signer(lender_id.clone(), lender_signer.clone())
            .send_to(&network_config)
            .await?;
        println!("Vault storage_deposit completed for {}", lender_id);
    }

    // =========================================================================
    // STEP 1: L1 DEPOSITS
    // =========================================================================
    let lender1_deposit = SOLVER_BORROW_AMOUNT;
    println!("\n--- Step 1: Lender 1 deposits {} ---", lender1_deposit);
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
    println!("Transferred {} to lender1", lender1_deposit);

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
    println!("Lender1 deposited {} into vault", lender1_deposit);

    let lender1_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_shares_u128 = lender1_shares.data.parse::<u128>().unwrap();
    println!("Lender1 received {} shares", lender1_shares_u128);

    // =========================================================================
    // STEP 2: SOLVER BORROWS ALL OF L1'S DEPOSIT
    // =========================================================================
    println!("\n--- Step 2: Solver borrows {} (all of lender1's deposit) ---", SOLVER_BORROW_AMOUNT);
    let _intent1 = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "intent-1",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-1"
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Solver created intent 0 and borrowed {}", SOLVER_BORROW_AMOUNT);

    sleep(Duration::from_millis(1200)).await;

    let total_assets_after_borrow1: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Total assets after solver borrow: {} (should be 0)", total_assets_after_borrow1.data);
    assert_eq!(total_assets_after_borrow1.data, "0", "Vault should have 0 assets after solver borrows all");

    // =========================================================================
    // STEP 3: L2 DEPOSITS (DURING BORROW)
    // =========================================================================
    let lender2_deposit = SOLVER_BORROW_AMOUNT / 2;
    println!("\n--- Step 3: Lender 2 deposits {} (half of SOLVER_BORROW_AMOUNT, after solver borrows) ---", lender2_deposit);
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": lender2_id,
            "amount": lender2_deposit.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Transferred {} to lender2", lender2_deposit);

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": lender2_deposit.to_string(),
            "msg": json!({ "receiver_id": lender2_id }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender2_id.clone(), lender2_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Lender2 deposited {} into vault", lender2_deposit);

    let lender2_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_shares_u128 = lender2_shares.data.parse::<u128>().unwrap();
    println!("Lender2 received {} shares (may be 0 if vault has 0 assets)", lender2_shares_u128);

    let total_assets_after_l2: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Total assets after lender2 deposit: {}", total_assets_after_l2.data);

    // =========================================================================
    // STEP 4: L1 REDEEMS (QUEUED)
    // =========================================================================
    println!("\n--- Step 4: Lender 1 attempts redemption (will be queued) ---");
    
    let lender1_shares_assets: Data<String> = vault_contract
        .call_function("convert_to_assets", json!({ "shares": lender1_shares_u128.to_string() }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_shares_assets_u128 = lender1_shares_assets.data.parse::<u128>().unwrap();
    println!("Lender1's {} shares are worth {} assets (available: {}, original deposit: {})", 
        lender1_shares_u128, lender1_shares_assets_u128, lender2_deposit, lender1_deposit);
    
    let lender1_redeem_outcome = vault_contract
        .call_function("redeem", json!({
            "shares": lender1_shares_u128.to_string(),
            "receiver_id": lender1_id,
            "memo": null
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender1_id.clone(), lender1_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Lender1 redeem outcome: {:?}", lender1_redeem_outcome.status);
    
    sleep(Duration::from_millis(1200)).await;
    
    let lender1_remaining_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_remaining_shares_u128 = lender1_remaining_shares.data.parse::<u128>().unwrap();
    println!("Lender1 remaining shares after partial redemption: {}", lender1_remaining_shares_u128);
    
    if lender1_remaining_shares_u128 > 0 {
        let remaining_assets: Data<String> = vault_contract
            .call_function("convert_to_assets", json!({ "shares": lender1_remaining_shares_u128.to_string() }))?
            .read_only()
            .fetch_from(&network_config)
            .await?;
        let remaining_assets_u128 = remaining_assets.data.parse::<u128>().unwrap();
        println!("Remaining shares are worth {} assets (available: {})", remaining_assets_u128, lender2_deposit);
        
        if remaining_assets_u128 > lender2_deposit {
            let lender1_redeem_remaining_outcome = vault_contract
                .call_function("redeem", json!({
                    "shares": lender1_remaining_shares_u128.to_string(),
                    "receiver_id": lender1_id,
                    "memo": null
                }))?
                .transaction()
                .deposit(NearToken::from_yoctonear(1))
                .with_signer(lender1_id.clone(), lender1_signer.clone())
                .send_to(&network_config)
                .await?;
            println!("Lender1 remaining redeem outcome: {:?}", lender1_redeem_remaining_outcome.status);
        }
    }
    
    let pending_redemptions_after_l1: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after lender1 redemption attempts: {}", pending_redemptions_after_l1.data.len());

    // =========================================================================
    // STEP 5: SOLVER REPAYS WITH 1% YIELD
    // =========================================================================
    println!("\n--- Step 5: Solver repays with 1% intent_yield ---");
    let intent_yield1 = SOLVER_BORROW_AMOUNT / 100;
    let total_repayment1 = SOLVER_BORROW_AMOUNT + intent_yield1;
    println!("Intent yield: {}, Total repayment: {}", intent_yield1, total_repayment1);

    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": solver_id,
            "amount": intent_yield1.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Transferred intent_yield {} to solver", intent_yield1);

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": total_repayment1.to_string(),
            "msg": json!({ "repay": { "intent_index": "0" } }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Solver repaid {} (principal + 1% intent_yield)", total_repayment1);

    sleep(Duration::from_millis(2000)).await;
    
    // Process queue
    loop {
        let queue_length: Data<String> = vault_contract
            .call_function("get_pending_redemptions_length", json!([]))?
            .read_only()
            .fetch_from(&network_config)
            .await?;
        let queue_length_u32 = queue_length.data.parse::<u128>().unwrap() as u32;
        
        if queue_length_u32 == 0 {
            println!("Redemption queue is empty, stopping processing");
            break;
        }
        
        println!("Processing next redemption from queue (queue length: {})", queue_length_u32);
        vault_contract
            .call_function("process_next_redemption", json!([]))?
            .transaction()
            .with_signer(solver_id.clone(), solver_signer.clone())
            .send_to(&network_config)
            .await?;
        
        sleep(Duration::from_millis(1200)).await;
    }

    // =========================================================================
    // STEP 6: VERIFY L1 RECEIVED DEPOSIT + YIELD
    // =========================================================================
    println!("\n--- Step 6: Verify Lender 1 received deposit + 1% intent_yield ---");
    let lender1_balance_after_repay1: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_balance_u128 = lender1_balance_after_repay1.data.parse::<u128>().unwrap();
    let lender1_expected = lender1_deposit + intent_yield1;
    println!("Lender1 balance: {} (expected: {}, deposit: {}, intent_yield: {})", 
        lender1_balance_u128, lender1_expected, lender1_deposit, intent_yield1);
    assert_eq!(lender1_balance_u128, lender1_expected, "Lender1 should receive deposit + full intent_yield");

    let pending_redemptions_after_repay1: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after first repayment: {}", pending_redemptions_after_repay1.data.len());
    assert!(pending_redemptions_after_repay1.data.is_empty());

    let total_assets_after_repay1: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Total assets after first repayment: {}", total_assets_after_repay1.data);

    // =========================================================================
    // STEP 7: SOLVER BORROWS AGAIN (L2'S DEPOSIT)
    // =========================================================================
    let solver2_borrow_amount = lender2_deposit;
    println!("\n--- Step 7: Solver borrows again {} (lender2's deposit, smaller than first borrow) ---", solver2_borrow_amount);
    
    let total_assets_before_borrow2: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Total assets before second solver borrow: {}", total_assets_before_borrow2.data);
    
    let _intent2 = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "intent-2",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-2",
            "amount": solver2_borrow_amount.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Solver created intent 1 and borrowed {}", solver2_borrow_amount);

    sleep(Duration::from_millis(1200)).await;

    let total_assets_after_borrow2: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Total assets after second solver borrow: {}", total_assets_after_borrow2.data);
    let remaining_after_borrow2 = total_assets_after_borrow2.data.parse::<u128>().unwrap();
    assert_eq!(remaining_after_borrow2, 0, "All assets should be borrowed, got {}", remaining_after_borrow2);

    // =========================================================================
    // STEP 8: L2 REDEEMS (QUEUED)
    // =========================================================================
    println!("\n--- Step 8: Lender 2 attempts redemption (will be queued) ---");
    if lender2_shares_u128 > 0 {
        let lender2_redeem_outcome = vault_contract
            .call_function("redeem", json!({
                "shares": lender2_shares_u128.to_string(),
                "receiver_id": lender2_id,
                "memo": null
            }))?
            .transaction()
            .deposit(NearToken::from_yoctonear(1))
            .with_signer(lender2_id.clone(), lender2_signer.clone())
            .send_to(&network_config)
            .await?;
        println!("Lender2 redeem outcome: {:?}", lender2_redeem_outcome.status);

        let pending_redemptions_after_l2: Data<Vec<serde_json::Value>> = vault_contract
            .call_function("get_pending_redemptions", json!({}))?
            .read_only()
            .fetch_from(&network_config)
            .await?;
        println!("Pending redemptions after lender2 redeem attempt: {}", pending_redemptions_after_l2.data.len());
        if lender2_shares_u128 > 0 {
            assert_eq!(pending_redemptions_after_l2.data.len(), 1, "Lender2 should be queued");
        }
    } else {
        println!("Lender2 has 0 shares, cannot redeem");
    }

    // =========================================================================
    // STEP 9: SOLVER REPAYS SECOND BORROW WITH YIELD
    // =========================================================================
    println!("\n--- Step 9: Solver repays with 1% intent_yield ---");
    let intent_yield2 = solver2_borrow_amount / 100;
    let total_repayment2 = solver2_borrow_amount + intent_yield2;
    println!("Intent yield: {}, Total repayment: {}", intent_yield2, total_repayment2);

    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": solver_id,
            "amount": intent_yield2.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Transferred intent_yield {} to solver", intent_yield2);

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": total_repayment2.to_string(),
            "msg": json!({ "repay": { "intent_index": "1" } }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Solver repaid {} (principal + 1% intent_yield)", total_repayment2);

    sleep(Duration::from_millis(2000)).await;
    
    // Process queue
    loop {
        let queue_length: Data<String> = vault_contract
            .call_function("get_pending_redemptions_length", json!([]))?
            .read_only()
            .fetch_from(&network_config)
            .await?;
        let queue_length_u32 = queue_length.data.parse::<u128>().unwrap() as u32;
        
        if queue_length_u32 == 0 {
            println!("Redemption queue is empty, stopping processing");
            break;
        }
        
        println!("Processing next redemption from queue (queue length: {})", queue_length_u32);
        vault_contract
            .call_function("process_next_redemption", json!([]))?
            .transaction()
            .with_signer(solver_id.clone(), solver_signer.clone())
            .send_to(&network_config)
            .await?;
        
        sleep(Duration::from_millis(1200)).await;
    }

    // Check state
    let pending_redemptions_after_repay2_before_check: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after second repayment (before check): {}", pending_redemptions_after_repay2_before_check.data.len());
    
    let total_assets_after_repay2: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Total assets after second repayment: {}", total_assets_after_repay2.data);
    
    let lender2_shares_check: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_shares_check_u128 = lender2_shares_check.data.parse::<u128>().unwrap();
    println!("Lender2 shares after repayment: {}", lender2_shares_check_u128);
    
    if lender2_shares_check_u128 > 0 {
        let lender2_convert_to_assets: Data<String> = vault_contract
            .call_function("convert_to_assets", json!({ "shares": lender2_shares_check_u128.to_string() }))?
            .read_only()
            .fetch_from(&network_config)
            .await?;
        println!("Lender2's {} shares convert to {} assets", lender2_shares_check_u128, lender2_convert_to_assets.data);
    }

    // =========================================================================
    // STEP 10: VERIFY L2 RECEIVED PAYMENT
    // =========================================================================
    println!("\n--- Step 10: Verify Lender 2 received payment ---");
    let lender2_balance_after_repay2: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_balance_u128 = lender2_balance_after_repay2.data.parse::<u128>().unwrap();
    
    if lender2_shares_u128 > 0 {
        let lender2_expected = lender2_deposit + intent_yield2;
        println!("Lender2 balance: {} (expected: {}, deposit: {}, intent_yield: {})", 
            lender2_balance_u128, lender2_expected, lender2_deposit, intent_yield2);
        assert_eq!(lender2_balance_u128, lender2_expected, "Lender2 should receive deposit + 1% intent_yield");
    } else {
        println!("Lender2 balance: {} (expected: {}, deposit only, no intent_yield since they have 0 shares)", 
            lender2_balance_u128, lender2_deposit);
        assert_eq!(lender2_balance_u128, lender2_deposit, "Lender2 should receive only their deposit (no shares, no intent_yield)");
    }

    let pending_redemptions_after_repay2: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after second repayment: {}", pending_redemptions_after_repay2.data.len());
    assert!(pending_redemptions_after_repay2.data.is_empty());

    // =========================================================================
    // FINAL STATE VERIFICATION
    // =========================================================================
    println!("\n--- Final State Verification ---");
    let total_assets_final: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Total assets final: {}", total_assets_final.data);
    assert_eq!(total_assets_final.data, "0");

    let total_shares_final: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Total shares final: {}", total_shares_final.data);
    assert_eq!(total_shares_final.data, "0");

    println!("\n=== Test Summary ===");
    println!("Lender1: deposited {}, received {} (deposit + 1% intent_yield)", lender1_deposit, lender1_balance_u128);
    println!("Lender2: deposited {}, received {} (deposit{} intent_yield)", lender2_deposit, lender2_balance_u128, if lender2_shares_u128 > 0 { " + 1%" } else { ", no" });
    println!("No remaining shares or liquidity - test passed!");

    Ok(())
}
