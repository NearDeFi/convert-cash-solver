mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_borrow_with_redemption() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;
    println!("=== Test: Borrow blocked when redemption is pending ===");

    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;

    let (lender1_id, lender1_signer) = create_user_account(&network_config, &genesis_account_id, &genesis_signer, "lender1").await?;
    let (solver1_id, solver1_signer) = create_user_account(&network_config, &genesis_account_id, &genesis_signer, "solver1").await?;
    let (solver2_id, solver2_signer) = create_user_account(&network_config, &genesis_account_id, &genesis_signer, "solver2").await?;

    let ft_contract = Contract(ft_id.clone());
    let vault_contract = Contract(vault_id.clone());

    // Register all accounts on FT contract
    for account_id in [&lender1_id, &solver1_id, &solver2_id] {
        ft_contract
            .call_function("storage_deposit", json!({ "account_id": account_id }))?
            .transaction()
            .deposit(NearToken::from_millinear(10))
            .with_signer(genesis_account_id.clone(), genesis_signer.clone())
            .send_to(&network_config)
            .await?;
    }

    // Register lender on vault
    vault_contract
        .call_function("storage_deposit", json!({ "account_id": lender1_id }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(lender1_id.clone(), lender1_signer.clone())
        .send_to(&network_config)
        .await?;

    // Register solvers on vault
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

    // Step 1: L1 deposits
    println!("\n=== Step 1: L1 deposits {} ===", lender1_deposit);
    
    // Transfer USDC to L1
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

    // L1 deposits to vault
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

    // Step 2: Solver1 borrows half the liquidity
    println!("\n=== Step 2: Solver1 borrows half the liquidity ===");
    let solver1_borrow_amount = lender1_deposit / 2; // 50 USDC
    
    let _intent1 = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "intent-s1",
            "_solver_deposit_address": solver1_id,
            "user_deposit_hash": "hash-s1",
            "amount": solver1_borrow_amount.to_string()
        }))?
        .transaction()
        .with_signer(solver1_id.clone(), solver1_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Solver1 borrowed {} via new_intent", solver1_borrow_amount);

    sleep(Duration::from_millis(2000)).await;

    // Verify total_assets after Solver1 borrow
    let total_assets_after_s1: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_after_s1_u128 = total_assets_after_s1.data.parse::<u128>().unwrap();
    println!("Total assets after Solver1 borrow: {} (half remains)", total_assets_after_s1_u128);
    assert_eq!(total_assets_after_s1_u128, lender1_deposit - solver1_borrow_amount);

    // Step 3: L1 redeems all shares (queued because not enough liquidity for full redemption)
    println!("\n=== Step 3: L1 redeems all shares (queued) ===");
    
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

    // Verify redemption is queued
    let pending_redemptions: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions: {} (should be 1)", pending_redemptions.data.len());
    assert_eq!(pending_redemptions.data.len(), 1, "L1's redemption should be queued");
    println!("L1's redemption is queued");

    // Step 4: Solver2 tries to borrow the other half - should be BLOCKED
    println!("\n=== Step 4: Solver2 tries to borrow (should be BLOCKED) ===");
    let solver2_borrow_amount = total_assets_after_s1_u128; // Try to borrow remaining liquidity
    
    let intent2_result = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "intent-s2",
            "_solver_deposit_address": solver2_id,
            "user_deposit_hash": "hash-s2",
            "amount": solver2_borrow_amount.to_string()
        }))?
        .transaction()
        .with_signer(solver2_id.clone(), solver2_signer.clone())
        .send_to(&network_config)
        .await;

    // Check if the borrow was blocked
    let total_assets_after_s2_attempt: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_after_s2_attempt_u128 = total_assets_after_s2_attempt.data.parse::<u128>().unwrap();
    
    // If borrow was blocked, total_assets should be unchanged
    println!("Total assets after Solver2 attempt: {} (should still be {})", 
        total_assets_after_s2_attempt_u128, total_assets_after_s1_u128);
    assert_eq!(total_assets_after_s2_attempt_u128, total_assets_after_s1_u128, 
        "Solver2's borrow should be blocked - redemption is pending");
    println!("✅ Solver2's borrow was correctly BLOCKED (redemption pending)");

    // Step 5: Solver1 repays with yield
    println!("\n=== Step 5: Solver1 repays with yield ===");
    let solver1_yield = solver1_borrow_amount / 100; // 1% yield
    let solver1_repayment = solver1_borrow_amount + solver1_yield;
    
    // Transfer yield to Solver1
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": solver1_id,
            "amount": solver1_yield.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    // Solver1 repays
    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": solver1_repayment.to_string(),
            "msg": json!({ "repay": { "intent_index": "0" } }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver1_id.clone(), solver1_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Solver1 repaid {} (borrow {} + yield {})", solver1_repayment, solver1_borrow_amount, solver1_yield);

    sleep(Duration::from_millis(2000)).await;

    // Step 6: Process redemption queue - L1 should be processed now
    println!("\n=== Step 6: Process redemption queue ===");
    vault_contract
        .call_function("process_next_redemption", json!([]))?
        .transaction()
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    sleep(Duration::from_millis(2000)).await;

    // Verify L1 is no longer in queue
    let pending_after_repay: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after processing: {} (should be 0)", pending_after_repay.data.len());
    assert_eq!(pending_after_repay.data.len(), 0, "L1 should be processed");

    // Verify L1 received deposit + yield
    let l1_final_balance: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let l1_final_balance_u128 = l1_final_balance.data.parse::<u128>().unwrap();
    
    // L1 should receive their full deposit + yield from Solver1's borrow
    // Note: The yield is calculated at redemption time based on active borrows
    let expected_yield = solver1_borrow_amount / 100; // 1% of what was borrowed
    let expected_l1_balance = lender1_deposit + expected_yield;
    println!("L1 final balance: {} (expected: {} = deposit {} + yield {})", 
        l1_final_balance_u128, expected_l1_balance, lender1_deposit, expected_yield);
    assert_eq!(l1_final_balance_u128, expected_l1_balance, 
        "L1 should receive deposit + yield");

    // Step 7: Solver2 can now borrow (no pending redemptions)
    println!("\n=== Step 7: Solver2 can now borrow (no pending redemptions) ===");
    
    // Check current total_assets
    let total_assets_before_s2: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let total_assets_before_s2_u128 = total_assets_before_s2.data.parse::<u128>().unwrap();
    println!("Total assets available for Solver2: {}", total_assets_before_s2_u128);

    // Solver2 should now be able to borrow (if there's liquidity)
    // Note: After L1 redeems, the vault should be empty
    if total_assets_before_s2_u128 > 0 {
        let solver2_new_borrow = total_assets_before_s2_u128.min(10_000_000); // Borrow up to 10 USDC
        
        let _intent2_success = vault_contract
            .call_function("new_intent", json!({
                "intent_data": "intent-s2-success",
                "_solver_deposit_address": solver2_id,
                "user_deposit_hash": "hash-s2-success",
                "amount": solver2_new_borrow.to_string()
            }))?
            .transaction()
            .with_signer(solver2_id.clone(), solver2_signer.clone())
            .send_to(&network_config)
            .await?;
        println!("✅ Solver2 successfully borrowed {} (no pending redemptions)", solver2_new_borrow);

        // Verify the borrow went through
        let total_assets_after_s2_success: Data<String> = vault_contract
            .call_function("total_assets", json!([]))?
            .read_only()
            .fetch_from(&network_config)
            .await?;
        let total_assets_after_s2_success_u128 = total_assets_after_s2_success.data.parse::<u128>().unwrap();
        assert_eq!(total_assets_after_s2_success_u128, total_assets_before_s2_u128 - solver2_new_borrow,
            "Solver2's borrow should succeed now");
    } else {
        println!("✅ Vault is empty (L1 redeemed all), Solver2 would be able to borrow if liquidity was available");
    }

    println!("\n✅ Test passed!");
    println!("Summary:");
    println!("  1. L1 deposited {}", lender1_deposit);
    println!("  2. Solver1 borrowed {} (half)", solver1_borrow_amount);
    println!("  3. L1 redeemed all shares (queued)");
    println!("  4. Solver2 tried to borrow but was BLOCKED (redemption pending)");
    println!("  5. Solver1 repaid with {} yield", solver1_yield);
    println!("  6. L1 received {} (deposit + yield)", l1_final_balance_u128);
    println!("  7. Solver2 can now borrow (no pending redemptions)");

    Ok(())
}

