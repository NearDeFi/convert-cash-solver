//! # Lender Profit Test
//!
//! Tests that yield is distributed only to lenders whose deposits were active
//! during the borrow period. Lenders who deposit after the borrow should not
//! receive yield from that borrow.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_lender_profit` | L1 deposits before borrow, L2 deposits after | L1 gets yield, L2 gets only deposit back |
//!
//! ## Lender/Solver Interaction Flow
//!
//! ```text
//! 1. L1 deposits 50 USDC → receives shares
//! 2. Solver borrows 5 USDC (liquidity from L1)
//! 3. L1 redeems all shares → QUEUED (liquidity borrowed)
//! 4. L2 deposits 1 USDC (AFTER borrow, AFTER L1's redemption queued)
//! 5. Solver repays 5.05 USDC (principal + 1% yield)
//! 6. Queue processed → L1 receives 50 + 0.05 USDC
//! 7. L2 redeems → receives only 1 USDC (no yield from prior borrow)
//! ```
//!
//! ## Key Verification Points
//!
//! - Yield goes to lenders active during borrow period
//! - Late depositors don't earn yield from prior borrows
//! - Queue processing respects chronological order
//! - Vault empties completely after all redemptions

mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Tests yield distribution based on borrow timing.
///
/// # Scenario
///
/// L1 deposits before borrow (earns yield).
/// L2 deposits after borrow and after L1's redemption is queued (no yield).
///
/// # Expected Outcome
///
/// - L1 receives deposit (50 USDC) + yield (0.05 USDC) = 50.05 USDC
/// - L2 receives only their deposit (1 USDC)
/// - Vault empties: total_assets = 0, total_shares = 0
#[tokio::test]
async fn test_lender_profit() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

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

    let lender1_deposit_amount = 50_000_000u128;
    let lender2_deposit_amount = 1_000_000u128; // Very small to not affect L1's yield significantly
    let intent_yield_amount = SOLVER_BORROW_AMOUNT / 100; // 1% yield

    // =========================================================================
    // STEP 1: L1 DEPOSITS BEFORE BORROW
    // =========================================================================
    println!("\n=== Step 1: Lender 1 deposits ===");
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": lender1_id,
            "amount": lender1_deposit_amount.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": lender1_deposit_amount.to_string(),
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
    println!("Lender1 deposited {} and received {} shares", lender1_deposit_amount, lender1_shares_u128);

    // =========================================================================
    // STEP 2: SOLVER BORROWS (L1's LIQUIDITY)
    // =========================================================================
    println!("\n=== Step 2: Solver borrows ===");
    let _intent = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "intent",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-profit"
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;
    println!("Solver borrowed {}", SOLVER_BORROW_AMOUNT);

    // =========================================================================
    // STEP 3: L1 REDEEMS (QUEUED - NO LIQUIDITY)
    // =========================================================================
    println!("\n=== Step 3: Lender 1 redeems (will be queued) ===");
    let lender1_redeem_amount = lender1_shares_u128.to_string();
    vault_contract
        .call_function("redeem", json!({
            "shares": lender1_redeem_amount,
            "receiver_id": lender1_id,
            "memo": null
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender1_id.clone(), lender1_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;

    let queue_length_before: Data<String> = vault_contract
        .call_function("get_pending_redemptions_length", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after lender1 redemption: {}", queue_length_before.data);
    assert!(queue_length_before.data.parse::<u128>().unwrap() > 0, "Lender1's redemption should be queued");

    // =========================================================================
    // STEP 4: L2 DEPOSITS AFTER BORROW (NO YIELD FROM THIS BORROW)
    // =========================================================================
    println!("\n=== Step 4: Lender 2 deposits (after borrow and after Lender1's redemption is queued) ===");
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": lender2_id,
            "amount": lender2_deposit_amount.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": lender2_deposit_amount.to_string(),
            "msg": json!({ "receiver_id": lender2_id }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender2_id.clone(), lender2_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(1000)).await;

    let lender2_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_shares_u128 = lender2_shares.data.parse::<u128>().unwrap();
    println!("Lender2 deposited {} and received {} shares", lender2_deposit_amount, lender2_shares_u128);

    // =========================================================================
    // STEP 5: SOLVER REPAYS WITH YIELD
    // =========================================================================
    println!("\n=== Step 5: Solver repays with intent_yield ===");
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": solver_id,
            "amount": intent_yield_amount.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": (SOLVER_BORROW_AMOUNT + intent_yield_amount).to_string(),
            "msg": json!({ "repay": { "intent_index": "0" } }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(2000)).await;
    println!("Solver repaid {} (principal + {} intent_yield)", SOLVER_BORROW_AMOUNT + intent_yield_amount, intent_yield_amount);

    // =========================================================================
    // STEP 6: PROCESS QUEUE - L1 RECEIVES YIELD
    // =========================================================================
    println!("\n=== Step 6: Process redemption queue (Lender 1) ===");
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
            .with_signer(solver_id.clone(), solver_signer.clone())
            .send_to(&network_config)
            .await?;
        
        sleep(Duration::from_millis(2000)).await;
    }

    // =========================================================================
    // STEP 7: VERIFY L1 RECEIVED DEPOSIT + YIELD
    // =========================================================================
    println!("\n=== Step 7: Verify Lender 1 received deposit + intent_yield ===");
    let lender1_final_balance: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_final_u128 = lender1_final_balance.data.parse::<u128>().unwrap();

    println!("Lender1 final balance: {} (expected: {} deposit + {} intent_yield = {})", 
        lender1_final_u128, lender1_deposit_amount, intent_yield_amount, lender1_deposit_amount + intent_yield_amount);
    assert_eq!(lender1_final_u128, lender1_deposit_amount + intent_yield_amount, "Lender1 should receive deposit + intent_yield");

    // =========================================================================
    // STEP 8: L2 REDEEMS - ONLY GETS DEPOSIT BACK
    // =========================================================================
    println!("\n=== Step 8: Lender 2 redeems ===");
    let lender2_redeem_amount = lender2_shares_u128.to_string();
    vault_contract
        .call_function("redeem", json!({
            "shares": lender2_redeem_amount,
            "receiver_id": lender2_id,
            "memo": null
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender2_id.clone(), lender2_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(2000)).await;

    // =========================================================================
    // STEP 9: VERIFY L2 RECEIVED ONLY DEPOSIT (NO YIELD)
    // =========================================================================
    println!("\n=== Step 9: Verify Lender 2 received only deposit (no intent_yield) ===");
    let lender2_final_balance: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_final_u128 = lender2_final_balance.data.parse::<u128>().unwrap();

    println!("Lender2 final balance: {} (expected: {} deposit, no intent_yield)", 
        lender2_final_u128, lender2_deposit_amount);
    assert_eq!(lender2_final_u128, lender2_deposit_amount, "Lender2 should receive only their deposit (no intent_yield)");

    // Final state verification
    let total_assets_final: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(total_assets_final.data, "0", "Total assets should be 0 after all redemptions");

    let total_shares_final: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(total_shares_final.data, "0", "Total shares should be 0 after all redemptions");

    println!("\n✅ Test passed! Lender1 received deposit + intent_yield, Lender2 received only deposit");

    Ok(())
}
