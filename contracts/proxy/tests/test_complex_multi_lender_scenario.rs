mod helpers;

use helpers::test_builder::{
    calculate_expected_shares_for_deposit, deposit_to_vault, get_balance, get_shares,
    get_total_assets, redeem_shares, solver_borrow, solver_repay,
    TestScenarioBuilder,
};
use near_api::Data;
use serde_json::json;
use tokio::time::{sleep, Duration};

async fn process_redemption_queue(builder: &TestScenarioBuilder) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    loop {
        let queue_length: Data<String> = builder.vault_contract()
            .call_function("get_pending_redemptions_length", json!([]))?
            .read_only()
            .fetch_from(builder.network_config())
            .await?;
        let queue_length_u32 = queue_length.data.parse::<u128>().unwrap() as u32;
        
        if queue_length_u32 == 0 {
            println!("Redemption queue is empty");
            break;
        }
        
        println!("Processing next redemption from queue (queue length: {})", queue_length_u32);
        builder.vault_contract()
            .call_function("process_next_redemption", json!([]))?
            .transaction()
            .with_signer(builder.genesis_account_id().clone(), builder.genesis_signer().clone())
            .send_to(builder.network_config())
            .await?;
        
        sleep(Duration::from_millis(2000)).await;
        
        let queue_length_after: Data<String> = builder.vault_contract()
            .call_function("get_pending_redemptions_length", json!([]))?
            .read_only()
            .fetch_from(builder.network_config())
            .await?;
        let queue_length_after_u32 = queue_length_after.data.parse::<u128>().unwrap() as u32;
        
        if queue_length_after_u32 == queue_length_u32 {
            println!("Queue didn't advance, stopping");
            break;
        }
    }
    Ok(())
}

/// Test: Multiple lenders with multiple borrows and partial redemptions
#[tokio::test]
async fn test_complex_multi_lender_scenario() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Test: Complex Multi-Lender Scenario ===");
    
    let builder = TestScenarioBuilder::new()
        .await?
        .deploy_vault()
        .await?
        .create_account("lender1")
        .await?
        .create_account("lender2")
        .await?
        .create_account("lender3")
        .await?
        .create_account("solver")
        .await?
        .register_accounts()
        .await?;

    let lender1_deposit = 50_000_000u128;
    let lender2_deposit = 30_000_000u128;
    let lender3_deposit = 20_000_000u128;
    
    // Step 1: L1 deposits
    println!("\n--- Step 1: L1 deposits {} ---", lender1_deposit);
    let expected_l1_shares = calculate_expected_shares_for_deposit(&builder, lender1_deposit).await?;
    let lender1_shares = deposit_to_vault(&builder, "lender1", lender1_deposit).await?;
    println!("L1 received {} shares (expected: {})", lender1_shares, expected_l1_shares);
    assert_eq!(lender1_shares, expected_l1_shares, 
        "L1 should receive correct shares for first deposit (empty vault)");

    // Step 2: Solver borrows part of L1's deposit
    println!("\n--- Step 2: Solver borrows {} (part of L1's deposit) ---", lender1_deposit / 2);
    let borrow1 = lender1_deposit / 2;
    solver_borrow(&builder, Some(borrow1), "hash-1").await?;

    // Step 3: L2 deposits (while borrow is active)
    println!("\n--- Step 3: L2 deposits {} (while borrow is active) ---", lender2_deposit);
    let total_assets_before_l2 = get_total_assets(&builder).await?;
    let total_supply_before_l2: Data<String> = builder.vault_contract()
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    let total_supply_before_l2_u128 = total_supply_before_l2.data.parse::<u128>().unwrap();
    println!("Vault state before L2 deposit: total_assets={}, total_supply={}", 
        total_assets_before_l2, total_supply_before_l2_u128);
    
    let expected_l2_shares = calculate_expected_shares_for_deposit(&builder, lender2_deposit).await?;
    let lender2_shares = deposit_to_vault(&builder, "lender2", lender2_deposit).await?;
    println!("L2 received {} shares (expected: {})", lender2_shares, expected_l2_shares);
    assert_eq!(lender2_shares, expected_l2_shares, 
        "L2 should receive correct shares accounting for borrow1 and expected_yield");
    
    let total_assets_after_l2 = get_total_assets(&builder).await?;
    let used_amount_l2 = total_assets_after_l2 - total_assets_before_l2;
    println!("L2 deposit: {} deposited, {} added to total_assets (unused_amount returned)", 
        lender2_deposit, used_amount_l2);
    println!("L2 deposit: unused_amount = {} (returned to L2)", lender2_deposit - used_amount_l2);

    // Step 4: L1 redeems half of their shares (partial redemption)
    println!("\n--- Step 4: L1 redeems half of their shares ---");
    let lender1_half_shares = lender1_shares / 2;
    
    let total_assets_before_l1_redeem = get_total_assets(&builder).await?;
    let total_supply_before_l1_redeem: Data<String> = builder.vault_contract()
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    let total_supply_before_l1_redeem_u128 = total_supply_before_l1_redeem.data.parse::<u128>().unwrap();
    println!("Before L1 redemption: total_assets={}, total_supply={}", 
        total_assets_before_l1_redeem, total_supply_before_l1_redeem.data);
    
    let lender1_balance_before_redeem = get_balance(&builder, "lender1").await?;
    redeem_shares(&builder, "lender1", lender1_half_shares).await?;
    sleep(Duration::from_millis(2000)).await;
    
    let lender1_balance_after_redeem = get_balance(&builder, "lender1").await?;
    let pending_redemptions_after_l1: Data<Vec<serde_json::Value>> = builder.vault_contract()
        .call_function("get_pending_redemptions", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    
    let lender1_first_redemption_received = if pending_redemptions_after_l1.data.is_empty() {
        let received = lender1_balance_after_redeem - lender1_balance_before_redeem;
        println!("L1 redemption processed immediately, received: {}", received);
        Some(received)
    } else {
        let stored_shares = pending_redemptions_after_l1.data[0]["shares"].as_str().unwrap_or("?");
        let stored_assets = pending_redemptions_after_l1.data[0]["assets"].as_str().unwrap_or("?");
        println!("L1 redemption queued with shares={}, stored_assets={}", stored_shares, stored_assets);
        None
    };
    
    let total_supply_after_l1_redeem: Data<String> = builder.vault_contract()
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    let total_supply_after_l1_redeem_u128 = total_supply_after_l1_redeem.data.parse::<u128>().unwrap();
    println!("Total supply after L1 redemption: {} (reduced by {} shares)", 
        total_supply_after_l1_redeem.data, 
        total_supply_before_l1_redeem_u128 - total_supply_after_l1_redeem_u128);
    assert_eq!(total_supply_after_l1_redeem_u128, total_supply_before_l1_redeem_u128 - lender1_half_shares,
        "Total supply should decrease by redeemed shares");

    // Step 5: Solver borrows more (using L2's deposit)
    println!("\n--- Step 5: Solver borrows {} (using L2's deposit) ---", lender2_deposit);
    solver_borrow(&builder, Some(lender2_deposit), "hash-2").await?;

    // Step 6: L3 deposits (while two borrows are active)
    println!("\n--- Step 6: L3 deposits {} (while two borrows are active) ---", lender3_deposit);
    let total_assets_before_l3 = get_total_assets(&builder).await?;
    let total_supply_before_l3: Data<String> = builder.vault_contract()
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    let total_supply_before_l3_u128 = total_supply_before_l3.data.parse::<u128>().unwrap();
    println!("Vault state before L3 deposit: total_assets={}, total_supply={}", 
        total_assets_before_l3, total_supply_before_l3_u128);
    
    let lender1_remaining_shares_calc = lender1_shares - lender1_half_shares;
    println!("Breakdown of total_supply: L1 remaining={}, L2={}, expected_total={}", 
        lender1_remaining_shares_calc, lender2_shares, 
        lender1_remaining_shares_calc + lender2_shares);
    assert_eq!(total_supply_before_l3_u128, lender1_remaining_shares_calc + lender2_shares,
        "Total supply should equal L1 remaining + L2 shares");
    
    let expected_l3_shares = calculate_expected_shares_for_deposit(&builder, lender3_deposit).await?;
    let lender3_shares = deposit_to_vault(&builder, "lender3", lender3_deposit).await?;
    println!("L3 received {} shares (expected: {})", lender3_shares, expected_l3_shares);
    assert_eq!(lender3_shares, expected_l3_shares, 
        "L3 should receive correct shares accounting for both borrows and expected_yield");

    // Step 7: L2 redeems all shares (will be queued)
    println!("\n--- Step 7: L2 redeems all shares (will be queued) ---");
    redeem_shares(&builder, "lender2", lender2_shares).await?;

    // Step 8: L1 redeems remaining shares (will be queued)
    println!("\n--- Step 8: L1 redeems remaining shares (will be queued) ---");
    let lender1_remaining_shares = lender1_shares - lender1_half_shares;
    redeem_shares(&builder, "lender1", lender1_remaining_shares).await?;

    // Step 9: Solver repays first borrow with yield
    println!("\n--- Step 9: Solver repays first borrow with 1% yield ---");
    let intent_yield1 = borrow1 / 100;
    solver_repay(&builder, 0, borrow1, intent_yield1).await?;
    println!("Processing redemption queue after first repayment...");
    process_redemption_queue(&builder).await?;

    // Step 10: Solver repays second borrow with yield
    println!("\n--- Step 10: Solver repays second borrow with 1% yield ---");
    let intent_yield2 = lender2_deposit / 100;
    solver_repay(&builder, 1, lender2_deposit, intent_yield2).await?;
    println!("Processing redemption queue after second repayment...");
    process_redemption_queue(&builder).await?;

    // Step 11: Verify all lenders received correct amounts
    println!("\n--- Step 11: Verify all lenders received correct amounts ---");
    
    let lender1_final = get_balance(&builder, "lender1").await?;
    let lender2_final = get_balance(&builder, "lender2").await?;
    let lender3_final = get_balance(&builder, "lender3").await?;
    
    println!("L1 final balance: {} (deposited: {})", lender1_final, lender1_deposit);
    println!("L2 final balance: {} (deposited: {})", lender2_final, lender2_deposit);
    println!("L3 final balance: {} (deposited: {})", lender3_final, lender3_deposit);
    
    let lender1_total_received = lender1_final;
    assert!(lender1_total_received > 0, "L1 should receive some assets");
    assert!(lender1_total_received >= lender1_deposit * 7 / 10, 
        "L1 should receive at least 70% of deposit (got {}, expected at least {})", 
        lender1_total_received, lender1_deposit * 7 / 10);
    
    assert!(lender2_final > 0, "L2 should receive some assets");
    assert!(lender2_final >= lender2_deposit * 9 / 10, 
        "L2 should get at least 90% of deposit (got {}, expected at least {})", 
        lender2_final, lender2_deposit * 9 / 10);
    
    assert!(lender3_final > 0, "L3 should receive some assets");
    assert!(lender3_final < lender3_deposit, 
        "L3 should receive less than deposit due to expected_yield dilution (got {}, deposited {})", 
        lender3_final, lender3_deposit);

    let total_assets_final = get_total_assets(&builder).await?;
    let total_shares_final: Data<String> = builder.vault_contract()
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(builder.network_config())
        .await?;
    
    println!("\nFinal state: total_assets={}, total_shares={}", total_assets_final, total_shares_final.data);
    
    let lender3_remaining_shares = get_shares(&builder, "lender3").await?;
    assert_eq!(lender3_remaining_shares.to_string(), total_shares_final.data, 
        "L3 should still have shares");

    println!("\n✅ Test passed! Complex multi-lender scenario works correctly");
    Ok(())
}

