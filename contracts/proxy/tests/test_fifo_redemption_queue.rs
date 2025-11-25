mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_fifo_redemption_queue() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;
    println!("=== Test: FIFO Redemption Queue with Proportional Yield ===");
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

    // Register all accounts with FT contract
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
    
    // Note: Vault doesn't need to be registered with FT contract to receive tokens via ft_transfer_call
    // The FT contract will automatically handle the transfer

    // Register lenders with vault
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

    println!("\n=== Test: FIFO Redemption Queue with Proportional Yield ===\n");

    // Step 1: Lender 1 deposits 50,000,000
    let lender1_deposit = 50_000_000u128;
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

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": lender1_deposit.to_string(),
            "msg": json!({ "deposit": {} }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender1_id.clone(), lender1_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Lender1 deposited {} into vault", lender1_deposit);

    sleep(Duration::from_millis(1200)).await;

    let lender1_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_shares_u128 = lender1_shares.data.parse::<u128>().unwrap();
    println!("Lender1 received {} shares", lender1_shares_u128);
    
    sleep(Duration::from_millis(1200)).await;

    // Step 2: Lender 2 deposits 25,000,000
    let lender2_deposit = 25_000_000u128;
    println!("\n--- Step 2: Lender 2 deposits {} ---", lender2_deposit);
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

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": lender2_deposit.to_string(),
            "msg": json!({ "deposit": {} }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender2_id.clone(), lender2_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Lender2 deposited {} into vault", lender2_deposit);

    sleep(Duration::from_millis(1200)).await;

    let lender2_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_shares_u128 = lender2_shares.data.parse::<u128>().unwrap();
    println!("Lender2 received {} shares", lender2_shares_u128);
    
    sleep(Duration::from_millis(1200)).await;

    let total_assets_before_borrow: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Total assets before solver borrow: {}", total_assets_before_borrow.data);

    // Step 3: Solver borrows 75,000,000 (all liquidity)
    let solver_borrow_amount = lender1_deposit + lender2_deposit; // 75,000,000
    println!("\n--- Step 3: Solver borrows {} (all liquidity) ---", solver_borrow_amount);
    let intent_outcome = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "intent-1",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-1",
            "amount": solver_borrow_amount.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Solver created intent and borrowed {}, outcome: {:?}", solver_borrow_amount, intent_outcome.status);

    sleep(Duration::from_millis(1200)).await;

    let total_assets_after_borrow: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Total assets after solver borrow: {} (should be 0)", total_assets_after_borrow.data);
    assert_eq!(total_assets_after_borrow.data, "0", "All assets should be borrowed");

    // Step 4: Lender 2 redeems (queued first)
    println!("\n--- Step 4: Lender 2 attempts redemption (will be queued first) ---");
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

    sleep(Duration::from_millis(1200)).await;

    let pending_redemptions_after_l2: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after lender2 redemption attempt: {}", pending_redemptions_after_l2.data.len());
    assert_eq!(pending_redemptions_after_l2.data.len(), 1, "Lender2 should be queued");

    // Step 5: Lender 1 redeems (queued after Lender 2)
    println!("\n--- Step 5: Lender 1 attempts redemption (will be queued after Lender 2) ---");
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

    let pending_redemptions_after_l1: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after lender1 redemption attempt: {}", pending_redemptions_after_l1.data.len());
    assert_eq!(pending_redemptions_after_l1.data.len(), 2, "Both lenders should be queued");

    // Verify queue order: Lender2 should be first, Lender1 second
    if pending_redemptions_after_l1.data.len() >= 2 {
        let first_owner = pending_redemptions_after_l1.data[0]["owner_id"].as_str().unwrap();
        let second_owner = pending_redemptions_after_l1.data[1]["owner_id"].as_str().unwrap();
        println!("Queue order: 1st={}, 2nd={}", first_owner, second_owner);
        assert_eq!(first_owner, lender2_id.to_string(), "Lender2 should be first in queue");
        assert_eq!(second_owner, lender1_id.to_string(), "Lender1 should be second in queue");
    }

    // Step 6: Solver repays 75,000,000 + 1% intent_yield
    let intent_yield = solver_borrow_amount / 100; // 1% intent_yield
    let total_repayment = solver_borrow_amount + intent_yield; // 82,500,000
    println!("\n--- Step 6: Solver repays {} (principal + 1% intent_yield) ---", total_repayment);
    println!("Intent yield: {}, Total repayment: {}", intent_yield, total_repayment);

    // Check solver's balance before repayment
    let solver_balance_before: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": solver_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let solver_balance_before_u128 = solver_balance_before.data.parse::<u128>().unwrap();
    println!("Solver balance before repayment: {} (should have {} from borrow)", solver_balance_before_u128, solver_borrow_amount);

    // Solver already has the principal (75,000,000) from the borrow
    // We only need to transfer the intent_yield (7,500,000) to the solver
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": solver_id,
            "amount": intent_yield.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Transferred intent_yield {} to solver", intent_yield);
    
    sleep(Duration::from_millis(500)).await;
    
    // Check solver's balance after intent_yield transfer
    let solver_balance_after_intent_yield: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": solver_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let solver_balance_after_intent_yield_u128 = solver_balance_after_intent_yield.data.parse::<u128>().unwrap();
    println!("Solver balance after intent_yield transfer: {} (should be {})", solver_balance_after_intent_yield_u128, solver_borrow_amount + intent_yield);

    // Solver repays the full amount (principal + intent_yield)
    let repay_outcome = ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": total_repayment.to_string(),
            "msg": json!({ "repay": { "intent_index": "0" } }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Solver repaid {} (principal + 1% intent_yield), outcome: {:?}", total_repayment, repay_outcome.status);

    // Wait for tokens to be transferred and ft_resolve_transfer to complete
    sleep(Duration::from_millis(3000)).await;
    
    // Verify vault actually received the repayment tokens
    let vault_balance_check: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": vault_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let vault_balance_check_u128 = vault_balance_check.data.parse::<u128>().unwrap();
    println!("Vault balance after repayment (before processing queue): {}", vault_balance_check_u128);
    
    // Process the redemption queue - call process_next_redemption until queue is empty
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
        
        // Wait for FT transfer to complete and lender balance to update
        sleep(Duration::from_millis(2000)).await;
    }
    
    // Check solver's balance after repayment
    let solver_balance_after: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": solver_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let solver_balance_after_u128 = solver_balance_after.data.parse::<u128>().unwrap();
    println!("Solver balance after repayment: {} (should be 0)", solver_balance_after_u128);
    
    // Check vault's balance after repayment
    let vault_balance_after: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": vault_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let vault_balance_after_u128 = vault_balance_after.data.parse::<u128>().unwrap();
    println!("Vault balance after repayment: {} (should be {})", vault_balance_after_u128, total_repayment);

    // Check pending redemptions and total assets after repayment
    let pending_redemptions_after_repay: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after repayment: {}", pending_redemptions_after_repay.data.len());
    
    let total_assets_after_repay: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Total assets after repayment: {}", total_assets_after_repay.data);

    // Step 7: Verify Lender 2 is paid out first (FIFO)
    println!("\n--- Step 7: Verify Lender 2 is paid out first (FIFO) ---");
    let lender2_balance_after_repay: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_balance_u128 = lender2_balance_after_repay.data.parse::<u128>().unwrap();
    
    // Lender2 should get deposit + proportional intent_yield (1/3 of intent_yield since 25/75 = 1/3)
    let lender2_intent_yield_share = intent_yield / 3; // 25,000,000 / 75,000,000 = 1/3
    let lender2_expected = lender2_deposit + lender2_intent_yield_share; // 25,000,000 + 2,500,000 = 27,500,000
    println!("Lender2 balance: {} (expected: {}, deposit: {}, intent_yield share: {})", 
        lender2_balance_u128, lender2_expected, lender2_deposit, lender2_intent_yield_share);
    assert_eq!(lender2_balance_u128, lender2_expected, "Lender2 should receive deposit + proportional intent_yield");

    // Step 8: Verify Lender 1 is paid out second
    println!("\n--- Step 8: Verify Lender 1 is paid out second ---");
    let lender1_balance_after_repay: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_balance_u128 = lender1_balance_after_repay.data.parse::<u128>().unwrap();
    
    // Lender1 should get deposit + proportional intent_yield (2/3 of intent_yield since 50/75 = 2/3)
    let lender1_intent_yield_share = (intent_yield * 2) / 3; // 50,000,000 / 75,000,000 = 2/3
    let lender1_expected = lender1_deposit + lender1_intent_yield_share; // 50,000,000 + 5,000,000 = 55,000,000
    println!("Lender1 balance: {} (expected: {}, deposit: {}, intent_yield share: {})", 
        lender1_balance_u128, lender1_expected, lender1_deposit, lender1_intent_yield_share);
    assert_eq!(lender1_balance_u128, lender1_expected, "Lender1 should receive deposit + proportional intent_yield");

    // Verify queue is empty
    let pending_redemptions_final: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!("Pending redemptions after repayment: {}", pending_redemptions_final.data.len());
    assert!(pending_redemptions_final.data.is_empty(), "All redemptions should be processed");

    // Final assertions
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
    println!("Lender1: deposited {}, received {} (deposit + 2/3 of intent_yield)", lender1_deposit, lender1_balance_u128);
    println!("Lender2: deposited {}, received {} (deposit + 1/3 of intent_yield)", lender2_deposit, lender2_balance_u128);
    println!("Lender2 was paid out before Lender1 (FIFO queue) - test passed!");

    Ok(())
}

