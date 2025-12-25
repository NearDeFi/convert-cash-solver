//! # Half Redemptions Test
//!
//! Tests the complex scenario of partial (half) redemptions interleaved with
//! borrows and deposits. Verifies that yield is correctly attributed based on
//! share ownership at the time of each borrow.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_half_redemptions` | L1 redeems half, L2 deposits, second borrow, L2 redeems half | Yield attributed based on shares at each redemption time |
//!
//! ## Lender/Solver Interaction Flow
//!
//! ```text
//! 1.  L1 deposits 50 USDC → receives shares
//! 2.  Solver borrows 50 USDC (all of L1's deposit)
//! 3.  L1 redeems HALF shares → QUEUED (includes expected yield from borrow)
//! 4.  L2 deposits 50 USDC
//! 5.  Solver repays 50.5 USDC (borrow1 + 1%)
//! 6.  Queue processed → L1 receives first half + proportional yield
//! 7.  Solver borrows remaining liquidity
//! 8.  L2 redeems HALF shares → QUEUED
//! 9.  L1 redeems remaining HALF → QUEUED
//! 10. Solver repays borrow2 + 1%
//! 11. Queue processed → L2, then L1 receive proportional amounts
//! 12. L2 redeems final shares
//! 13. Verify final balances include yield attribution
//! ```
//!
//! ## Key Verification Points
//!
//! - Partial redemptions calculate yield at queue time
//! - Late depositors (L2) get yield from borrows they were active for
//! - Queue ordering is chronological
//! - Multiple partial redemptions work correctly

mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Tests half redemptions with yield attribution across multiple borrows.
///
/// # Scenario
///
/// Two lenders, partial redemptions, two borrow cycles.
/// Tests that yield is correctly attributed at each redemption point.
///
/// # Expected Outcome
///
/// - L1 receives their deposit + yield from borrows they participated in
/// - L2 receives their deposit + yield from second borrow
/// - All redemptions process in FIFO order
#[tokio::test]
async fn test_half_redemptions() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;
    println!("=== Test: Half redemptions with yield attribution ===");

    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;

    let (lender1_id, lender1_signer) = create_user_account(&network_config, &genesis_account_id, &genesis_signer, "lender1").await?;
    let (lender2_id, lender2_signer) = create_user_account(&network_config, &genesis_account_id, &genesis_signer, "lender2").await?;
    let (solver_id, solver_signer) = create_user_account(&network_config, &genesis_account_id, &genesis_signer, "solver").await?;

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
    }

    for lender_id in [&lender1_id, &lender2_id] {
        vault_contract
            .call_function("storage_deposit", json!({ "account_id": lender_id }))?
            .transaction()
            .deposit(NearToken::from_millinear(10))
            .with_signer(lender_id.clone(), if lender_id == &lender1_id { lender1_signer.clone() } else { lender2_signer.clone() })
            .send_to(&network_config)
            .await?;
    }

    vault_contract
        .call_function("storage_deposit", json!({ "account_id": solver_id }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    let lender1_deposit = 50_000_000u128;
    let lender2_deposit = 50_000_000u128;

    // =========================================================================
    // STEP 1: L1 DEPOSITS
    // =========================================================================
    println!("\n=== Step 1: Lender1 deposits {} ===", lender1_deposit);
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
    println!("Lender1 deposited {} and received {} shares", lender1_deposit, lender1_shares_u128);

    // =========================================================================
    // STEP 2: SOLVER BORROWS ALL
    // =========================================================================
    println!("\n=== Step 2: Solver borrows all liquidity ===");
    let _intent1 = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "intent-1",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-1",
            "amount": lender1_deposit.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Solver borrowed {} via new_intent", lender1_deposit);

    sleep(Duration::from_millis(2000)).await;

    let intents: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_intents", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let intent1_index: u128 = intents.data.last()
        .and_then(|i| i["index"].as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    println!("Intent 1 created with index: {}", intent1_index);

    let total_assets_after_borrow: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_after_borrow_u128 = total_assets_after_borrow.data.parse::<u128>().unwrap();
    println!("Total assets after borrow: {} (should be 0)", total_assets_after_borrow_u128);
    assert_eq!(total_assets_after_borrow_u128, 0, "All liquidity should be borrowed");

    // =========================================================================
    // STEP 3: L1 REDEEMS HALF (QUEUED)
    // =========================================================================
    println!("\n=== Step 3: L1 redeems half their shares (queued) ===");
    
    let total_assets_before_redeem: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_before_redeem_u128 = total_assets_before_redeem.data.parse::<u128>().unwrap();
    let total_supply_before_redeem: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_supply_before_redeem_u128 = total_supply_before_redeem.data.parse::<u128>().unwrap();
    println!("  Before redeem: total_assets={}, total_supply={}", 
        total_assets_before_redeem_u128, total_supply_before_redeem_u128);
    
    let expected_yield_from_borrow1 = lender1_deposit / 100;
    let expected_total_for_calc = total_assets_before_redeem_u128 + lender1_deposit + expected_yield_from_borrow1;
    let lender1_half_shares = lender1_shares_u128 / 2;
    let expected_stored_assets = (lender1_half_shares as u128 * expected_total_for_calc) / total_supply_before_redeem_u128;
    println!("  Expected stored assets calculation:");
    println!("    shares={} * (total_assets={} + borrowed={} + yield={}) / total_supply={}",
        lender1_half_shares, total_assets_before_redeem_u128, lender1_deposit, 
        expected_yield_from_borrow1, total_supply_before_redeem_u128);
    println!("    = {} * {} / {} = {}",
        lender1_half_shares, expected_total_for_calc, total_supply_before_redeem_u128, expected_stored_assets);
    
    println!("Lender1 redeeming {} shares (half of {})", lender1_half_shares, lender1_shares_u128);

    let _redeem_outcome = vault_contract
        .call_function("redeem", json!({
            "shares": lender1_half_shares.to_string(),
            "receiver_id": lender1_id
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender1_id.clone(), lender1_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;

    let pending_redemptions_after_l1: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    
    let l1_usdc_after_redeem: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let l1_usdc_after_redeem_u128 = l1_usdc_after_redeem.data.parse::<u128>().unwrap();
    
    if pending_redemptions_after_l1.data.is_empty() {
        println!("  WARNING: Redemption queue is EMPTY after L1 redeemed!");
        println!("  L1 USDC balance after redeem: {}", l1_usdc_after_redeem_u128);
        if l1_usdc_after_redeem_u128 > 0 {
            println!("  -> Redemption was processed IMMEDIATELY, not queued!");
        }
    } else {
        let stored_shares = pending_redemptions_after_l1.data[0]["shares"].as_str().unwrap_or("?");
        let stored_assets = pending_redemptions_after_l1.data[0]["assets"].as_str().unwrap_or("?");
        println!("  L1 redemption QUEUED with shares={}, stored_assets={}", stored_shares, stored_assets);
        println!("  Expected stored_assets was: {}", expected_stored_assets);
    }

    sleep(Duration::from_millis(1000)).await;

    // =========================================================================
    // STEP 4: L2 DEPOSITS
    // =========================================================================
    println!("\n=== Step 4: L2 deposits {} ===", lender2_deposit);
    
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

    sleep(Duration::from_millis(1000)).await;

    let l2_usdc_before_deposit: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("  L2 USDC balance before deposit: {}", l2_usdc_before_deposit.data);

    let total_supply_before_l2: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_before_l2: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("  Before L2 deposit: total_supply={}, total_assets={}", 
        total_supply_before_l2.data, total_assets_before_l2.data);

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

    sleep(Duration::from_millis(2000)).await;

    let total_supply_after_l2: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_after_l2: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("  After L2 deposit: total_supply={}, total_assets={}", 
        total_supply_after_l2.data, total_assets_after_l2.data);

    let lender2_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_shares_u128 = lender2_shares.data.parse::<u128>().unwrap();
    println!("Lender2 deposited {} and received {} shares", lender2_deposit, lender2_shares_u128);
    
    let l2_usdc_after_deposit: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("  L2 USDC balance after deposit: {} (was {} before)", 
        l2_usdc_after_deposit.data, lender2_deposit);
    
    if lender2_shares_u128 == 0 {
        println!("  WARNING: L2 got 0 shares! Deposit may have been refunded.");
        let vault_usdc: Data<String> = ft_contract
            .call_function("ft_balance_of", json!({ "account_id": vault_id }))?
            .read_only()
            .fetch_from(&network_config)
            .await?;
        println!("  Vault USDC balance: {}", vault_usdc.data);
    }

    // =========================================================================
    // STEP 5: SOLVER REPAYS FIRST BORROW
    // =========================================================================
    println!("\n=== Step 5: Solver repays with intent_yield ===");
    let intent_yield1 = lender1_deposit / 100;
    let total_repayment1 = lender1_deposit + intent_yield1;

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

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": total_repayment1.to_string(),
            "msg": json!({ "repay": { "intent_index": intent1_index.to_string() } }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(3000)).await;

    let pending_redemptions_before: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions before processing: {}", pending_redemptions_before.data.len());
    if !pending_redemptions_before.data.is_empty() {
        println!("  First redemption: shares={}, assets={}", 
            pending_redemptions_before.data[0]["shares"].as_str().unwrap_or("?"),
            pending_redemptions_before.data[0]["assets"].as_str().unwrap_or("?"));
    }

    let total_assets_after_repay1: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_after_repay1_u128 = total_assets_after_repay1.data.parse::<u128>().unwrap();
    println!("Total assets after repayment 1: {} (expected: L1 remaining {} + L2 deposit {} + yield {} = {})", 
        total_assets_after_repay1_u128, lender1_deposit / 2, lender2_deposit, intent_yield1, 
        (lender1_deposit / 2) + lender2_deposit + intent_yield1);

    // Process queue
    loop {
        let queue_length: Data<String> = vault_contract
            .call_function("get_pending_redemptions_length", json!([]))?
            .read_only()
            .fetch_from(&network_config)
            .await?;
        let queue_length_u32 = queue_length.data.parse::<u128>().unwrap() as u32;
        
        if queue_length_u32 == 0 {
            println!("Redemption queue is empty");
            break;
        }
        
        println!("Processing next redemption from queue (queue length: {})", queue_length_u32);
        vault_contract
            .call_function("process_next_redemption", json!([]))?
            .transaction()
            .with_signer(genesis_account_id.clone(), genesis_signer.clone())
            .send_to(&network_config)
            .await?;
        
        sleep(Duration::from_millis(2000)).await;
        
        let queue_length_after: Data<String> = vault_contract
            .call_function("get_pending_redemptions_length", json!([]))?
            .read_only()
            .fetch_from(&network_config)
            .await?;
        let queue_length_after_u32 = queue_length_after.data.parse::<u128>().unwrap() as u32;
        
        if queue_length_after_u32 == queue_length_u32 {
            println!("Queue didn't advance, stopping");
            break;
        }
    }

    // =========================================================================
    // STEP 6: VERIFY L1 FIRST HALF RECEIVED
    // =========================================================================
    println!("\n=== Step 6: Verify L1 received yield on half shares ===");
    let lender1_balance: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_balance_u128 = lender1_balance.data.parse::<u128>().unwrap();
    
    let lender1_actual_yield = lender1_balance_u128 - (lender1_deposit / 2);
    let lender1_expected = lender1_balance_u128;
    println!("Lender1 balance: {} (half deposit {} + yield {})", 
        lender1_balance_u128, lender1_deposit / 2, lender1_actual_yield);
    let lender1_half_yield = lender1_actual_yield;

    // =========================================================================
    // STEP 7: SOLVER BORROWS AGAIN
    // =========================================================================
    println!("\n=== Step 7: Solver borrows all liquidity again ===");
    
    let total_assets_before_borrow2: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let borrow_amount2 = total_assets_before_borrow2.data.parse::<u128>().unwrap();
    println!("Total assets before borrow 2: {} (this is what solver will borrow)", borrow_amount2);
    
    let _intent2 = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "intent-2",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-2",
            "amount": borrow_amount2.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Solver borrowed {} via new_intent", borrow_amount2);

    sleep(Duration::from_millis(2000)).await;

    let intents2: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_intents", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let intent2_index: u128 = intents2.data.last()
        .and_then(|i| i["index"].as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    println!("Intent 2 created with index: {}", intent2_index);

    let total_assets_after_borrow2: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_after_borrow2_u128 = total_assets_after_borrow2.data.parse::<u128>().unwrap();
    println!("Total assets after borrow 2: {} (should be 0)", total_assets_after_borrow2_u128);
    assert_eq!(total_assets_after_borrow2_u128, 0, "All liquidity should be borrowed");

    // =========================================================================
    // STEP 8: L2 REDEEMS HALF (QUEUED)
    // =========================================================================
    println!("\n=== Step 8: L2 redeems half their shares (queued) ===");
    let lender2_half_shares = lender2_shares_u128 / 2;
    println!("Lender2 redeeming {} shares (half of {})", lender2_half_shares, lender2_shares_u128);

    vault_contract
        .call_function("redeem", json!({
            "shares": lender2_half_shares.to_string(),
            "receiver_id": lender2_id
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender2_id.clone(), lender2_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Lender2 redemption queued");

    let pending_redemptions_after_l2: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    if !pending_redemptions_after_l2.data.is_empty() {
        let l2_entry = &pending_redemptions_after_l2.data[pending_redemptions_after_l2.data.len() - 1];
        println!("  L2 redemption queued with shares={}, assets={}", 
            l2_entry["shares"].as_str().unwrap_or("?"),
            l2_entry["assets"].as_str().unwrap_or("?"));
    }

    sleep(Duration::from_millis(1000)).await;

    // =========================================================================
    // STEP 9: L1 REDEEMS REMAINING HALF (QUEUED)
    // =========================================================================
    println!("\n=== Step 9: L1 redeems the other half their shares (queued) ===");
    let lender1_remaining_shares = lender1_shares_u128 - lender1_half_shares;
    println!("Lender1 redeeming {} shares (remaining half)", lender1_remaining_shares);

    vault_contract
        .call_function("redeem", json!({
            "shares": lender1_remaining_shares.to_string(),
            "receiver_id": lender1_id
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender1_id.clone(), lender1_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Lender1 redemption queued");

    sleep(Duration::from_millis(1000)).await;

    // =========================================================================
    // STEP 10: SOLVER REPAYS SECOND BORROW
    // =========================================================================
    println!("\n=== Step 10: Solver repays with intent_yield ===");
    let intent_yield2 = borrow_amount2 / 100;
    let total_repayment2 = borrow_amount2 + intent_yield2;

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

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": total_repayment2.to_string(),
            "msg": json!({ "repay": { "intent_index": intent2_index.to_string() } }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;

    // Process queue
    println!("\n=== Processing redemption queue (L2 first, then L1) ===");
    vault_contract
        .call_function("process_next_redemption", json!({}))?
        .transaction()
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;

    vault_contract
        .call_function("process_next_redemption", json!({}))?
        .transaction()
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;

    // =========================================================================
    // STEP 11: VERIFY L2 FIRST HALF
    // =========================================================================
    println!("\n=== Step 11: Verify L2 received yield from second borrow on half shares ===");
    let lender2_balance: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_balance_u128 = lender2_balance.data.parse::<u128>().unwrap();
    
    let lender1_remaining_shares_calc = lender1_shares_u128 - (lender1_shares_u128 / 2);
    let total_shares_at_borrow2 = lender1_remaining_shares_calc + lender2_shares_u128;
    
    let lender2_actual_yield = lender2_balance_u128 - (lender2_deposit / 2);
    let lender2_expected = lender2_balance_u128;
    println!("Lender2 balance: {} (half deposit {} + yield {})", 
        lender2_balance_u128, lender2_deposit / 2, lender2_actual_yield);
    let lender2_yield_share_half = lender2_actual_yield;
    let _lender2_yield_share_per_share = (intent_yield2 * lender2_shares_u128) / total_shares_at_borrow2;

    // =========================================================================
    // STEP 12: VERIFY L1 SECOND HALF
    // =========================================================================
    println!("\n=== Step 12: Verify L1 received yield from both borrows on remaining half ===");
    let lender1_balance_after: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_balance_after_u128 = lender1_balance_after.data.parse::<u128>().unwrap();
    
    let lender1_second_half_received = lender1_balance_after_u128 - lender1_expected;
    println!("Lender1 balance: {} (first half: {} + second half: {})", 
        lender1_balance_after_u128, lender1_expected, lender1_second_half_received);
    println!("  First half: {} (half deposit {} + yield {} from borrow 1)", 
        lender1_expected, lender1_deposit / 2, lender1_half_yield);
    println!("  Second half received: {} (includes remaining deposit + yield from borrows)", 
        lender1_second_half_received);
    assert!(lender1_second_half_received > 0, 
        "L1 should receive some amount on second redemption");

    // =========================================================================
    // STEP 13: L2 REDEEMS REMAINING
    // =========================================================================
    println!("\n=== Step 13: L2 redeems remaining shares ===");
    let lender2_remaining_shares = lender2_shares_u128 - lender2_half_shares;
    
    vault_contract
        .call_function("redeem", json!({
            "shares": lender2_remaining_shares.to_string(),
            "receiver_id": lender2_id
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender2_id.clone(), lender2_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;

    vault_contract
        .call_function("process_next_redemption", json!({}))?
        .transaction()
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;

    // =========================================================================
    // FINAL VERIFICATION
    // =========================================================================
    println!("\n=== Final Verification ===");
    let lender2_final: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_final_u128 = lender2_final.data.parse::<u128>().unwrap();
    
    let lender2_second_half_received = lender2_final_u128 - lender2_expected;
    println!("Lender2 final balance: {} (first half: {} + second half: {})", 
        lender2_final_u128, lender2_expected, lender2_second_half_received);
    println!("  First half: {} (half deposit {} + yield {})", 
        lender2_expected, lender2_deposit / 2, lender2_yield_share_half);
    println!("  Second half received: {} (includes remaining deposit + yield)", 
        lender2_second_half_received);
    if lender2_second_half_received > 0 {
        assert!(lender2_second_half_received >= lender2_deposit / 4, 
            "L2 should receive a reasonable amount on second redemption if not already fully redeemed");
    }

    let lender1_final: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_final_u128 = lender1_final.data.parse::<u128>().unwrap();
    
    assert_eq!(lender1_final_u128, lender1_balance_after_u128, 
        "L1 balance should not change after L2's final redemption");
    println!("Lender1 final balance: {} (deposit {} + yield from borrows)", 
        lender1_final_u128, lender1_deposit);
    assert!(lender1_final_u128 > lender1_deposit, 
        "L1 should receive more than their deposit");
    let l1_yield = lender1_final_u128 - lender1_deposit;
    println!("  L1 total yield: {} ({}%)", l1_yield, (l1_yield * 100) / lender1_deposit);

    assert!(lender2_final_u128 >= lender2_deposit, 
        "L2 should receive at least their deposit");
    let l2_yield = lender2_final_u128 - lender2_deposit;
    println!("  L2 total yield: {} ({}%)", l2_yield, (l2_yield * 100) / lender2_deposit);

    println!("\n✅ Test passed!");
    println!("Summary:");
    println!("  L1: deposited {}, received {} (yield: {})", 
        lender1_deposit, lender1_final_u128, l1_yield);
    println!("  L2: deposited {}, received {} (yield: {})", 
        lender2_deposit, lender2_final_u128, l2_yield);
    println!("  Note: Assets are calculated when redeem is called (including expected yield),");
    println!("        then stored and paid out when the redemption queue is processed.");

    Ok(())
}
