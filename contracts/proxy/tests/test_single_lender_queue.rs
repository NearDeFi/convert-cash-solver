//! # Single Lender Queue Test
//!
//! Tests the basic redemption queue flow with a single lender. When a solver
//! borrows all liquidity, the lender's redemption is queued until the solver
//! repays.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_single_lender_queue` | Lender redemption queued while solver has liquidity | Lender receives deposit + yield after repayment |
//!
//! ## Lender/Solver Interaction Flow
//!
//! ```text
//! 1. Lender deposits 5 USDC → receives vault shares
//! 2. Solver borrows 5 USDC (all liquidity)
//! 3. Lender tries to redeem → gets QUEUED (no liquidity)
//! 4. Solver repays 5.05 USDC (principal + 1% yield)
//! 5. Queue is processed → lender receives 5.05 USDC
//! 6. Vault empties: total_assets = 0, total_shares = 0
//! ```
//!
//! ## Key Verification Points
//!
//! - Redemption is queued when liquidity is insufficient
//! - Queue contains correct share amount
//! - After repayment, lender receives full amount including yield
//! - Vault is fully emptied after processing

mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Tests the complete queue lifecycle with a single lender.
///
/// # Scenario
///
/// A single lender deposits, solver borrows all, lender redeems (queued),
/// solver repays, and lender receives deposit + yield.
///
/// # Expected Outcome
///
/// - Lender receives SOLVER_BORROW_AMOUNT + 1% yield
/// - Vault total_assets = 0
/// - Vault total_shares = 0
/// - Queue is empty
#[tokio::test]
async fn test_single_lender_queue() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;
    println!("Sandbox started, genesis account = {}", genesis_account_id);

    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;
    println!("Vault deployed at {}, FT deployed at {}", vault_id, ft_id);

    let (lender_id, lender_signer) =
        create_user_account(&network_config, &genesis_account_id, &genesis_signer, "lender").await?;
    let (solver_id, solver_signer) =
        create_user_account(&network_config, &genesis_account_id, &genesis_signer, "solver").await?;
    println!("Test accounts created: lender={}, solver={}", lender_id, solver_id);

    let ft_contract = Contract(ft_id.clone());
    let vault_contract = Contract(vault_id.clone());

    // Register accounts
    for account_id in [&lender_id, &solver_id] {
        ft_contract
            .call_function("storage_deposit", json!({ "account_id": account_id }))?
            .transaction()
            .deposit(NearToken::from_millinear(10))
            .with_signer(genesis_account_id.clone(), genesis_signer.clone())
            .send_to(&network_config)
            .await?;
        println!("FT storage_deposit completed for {}", account_id);
    }

    vault_contract
        .call_function("storage_deposit", json!({ "account_id": lender_id }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(lender_id.clone(), lender_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Vault storage_deposit completed for {}", lender_id);

    vault_contract
        .call_function("storage_deposit", json!({ "account_id": solver_id }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;
    println!("Vault storage_deposit completed for {}", solver_id);

    // =========================================================================
    // LENDER DEPOSITS
    // =========================================================================
    let deposit_amount = SOLVER_BORROW_AMOUNT;
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": lender_id,
            "amount": deposit_amount.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;
    println!(
        "FT transfer genesis -> {} amount={} (fund lender)",
        lender_id, deposit_amount
    );

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": deposit_amount.to_string(),
            "msg": json!({ "receiver_id": lender_id }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender_id.clone(), lender_signer.clone())
        .send_to(&network_config)
        .await?;
    println!(
        "FT transfer_call {} -> vault amount={} (initial deposit)",
        lender_id, deposit_amount
    );

    let lender_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender_shares_u128 = lender_shares.data.parse::<u128>().unwrap();
    println!(
        "{} received vault shares amount={}",
        lender_id, lender_shares_u128
    );

    let total_assets_before_borrow: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(total_assets_before_borrow.data, deposit_amount.to_string());
    println!(
        "vault total_assets before borrow = {}",
        total_assets_before_borrow.data
    );

    // =========================================================================
    // SOLVER BORROWS ALL LIQUIDITY
    // =========================================================================
    let _intent = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "intent",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-queue",
            "amount": SOLVER_BORROW_AMOUNT.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;
    println!(
        "{} borrowed liquidity via new_intent (intent hash 'hash-queue')",
        solver_id
    );

    println!("Waiting for borrow transfer to finalize...");
    sleep(Duration::from_millis(1200)).await;

    let total_assets_after_borrow: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(total_assets_after_borrow.data, "0");
    println!(
        "vault total_assets immediately after borrow = {}",
        total_assets_after_borrow.data
    );

    // =========================================================================
    // LENDER REDEEMS (QUEUED)
    // =========================================================================
    let redeem_outcome = vault_contract
        .call_function("redeem", json!({
            "shares": lender_shares_u128.to_string(),
            "receiver_id": lender_id,
            "memo": null
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender_id.clone(), lender_signer.clone())
        .send_to(&network_config)
        .await?;

    println!("queued redemption outcome: {:?}", redeem_outcome.status);
    let status_str = format!("{:?}", redeem_outcome.status);
    assert!(
        status_str.contains("SuccessValue"),
        "expected redeem outcome SuccessValue, got {status_str}"
    );

    let pending_redemptions: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(pending_redemptions.data.len(), 1);
    assert_eq!(
        pending_redemptions.data[0]["shares"]
            .as_str()
            .expect("shares should be a string"),
        lender_shares_u128.to_string()
    );
    println!(
        "pending redemptions queued entries: {:?}",
        pending_redemptions.data
    );

    // =========================================================================
    // SOLVER REPAYS WITH YIELD
    // =========================================================================
    let intent_yield_amount = SOLVER_BORROW_AMOUNT / 100; // 1% yield
    let total_repayment = SOLVER_BORROW_AMOUNT + intent_yield_amount;

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
    println!(
        "FT transfer genesis -> {} intent_yield amount={}",
        solver_id, intent_yield_amount
    );

    ft_contract
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
    println!(
        "FT transfer_call {} -> vault repayment amount={} (principal + intent_yield)",
        solver_id, total_repayment
    );

    // Wait for repayment to finalize
    sleep(Duration::from_millis(2000)).await;
    
    // =========================================================================
    // PROCESS REDEMPTION QUEUE
    // =========================================================================
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
    // VERIFY FINAL STATE
    // =========================================================================
    let lender_final_balance: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(lender_final_balance.data, total_repayment.to_string());
    println!(
        "{} final FT balance after repayment processing = {}",
        lender_id, lender_final_balance.data
    );

    let total_assets_final: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(total_assets_final.data, "0");
    println!(
        "vault total_assets after repayment processing = {}",
        total_assets_final.data
    );

    let total_shares_final: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(total_shares_final.data, "0");
    println!(
        "vault total shares after repayment processing = {}",
        total_shares_final.data
    );

    let pending_redemptions_final: Data<Vec<serde_json::Value>> = vault_contract
        .call_function("get_pending_redemptions", json!({}))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert!(pending_redemptions_final.data.is_empty());
    println!(
        "pending redemption queue after repayment = {:?}",
        pending_redemptions_final.data
    );

    Ok(())
}
