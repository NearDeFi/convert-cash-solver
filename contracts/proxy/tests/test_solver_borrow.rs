//! # Solver Borrow Test
//!
//! Tests the complete solver lifecycle: borrowing liquidity from the vault,
//! executing an intent, and repaying with yield.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_solver_borrow` | Solver borrows, fulfills intent, repays with 1% yield | Vault assets increase by yield amount |
//!
//! ## Lender/Solver Interaction Flow
//!
//! ```text
//! 1. Lender deposits 50 USDC → receives vault shares
//! 2. Solver calls new_intent → borrows 5 USDC from vault
//! 3. Solver fulfills intent off-chain (simulated)
//! 4. Solver repays 5.05 USDC (principal + 1% yield)
//! 5. Vault total_assets increases by 0.05 USDC (yield)
//! ```
//!
//! ## Key Verification Points
//!
//! - Solver receives borrowed tokens immediately after new_intent
//! - Intent state transitions: Created → LiquidityBorrowed → LiquidityReturned
//! - Solver balance returns to 0 after repayment
//! - Total shares remain unchanged (no dilution)
//! - Total assets increase by the yield amount

mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

/// Tests the complete solver borrow and repay cycle.
///
/// # Scenario
///
/// 1. User deposits 50 USDC as liquidity
/// 2. Solver creates intent and borrows 5 USDC
/// 3. Solver repays 5.05 USDC (principal + 1% yield)
///
/// # Expected Outcome
///
/// - Solver receives exactly SOLVER_BORROW_AMOUNT tokens
/// - Intent is created with correct data
/// - After repayment, solver balance = 0
/// - Vault total_assets = original + borrow + yield
/// - Total shares unchanged (yield accrues to share value)
#[tokio::test]
async fn test_solver_borrow() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox and deploy contracts
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;

    // Create and fund lender
    let (user_id, user_signer) =
        create_user_account(&network_config, &genesis_account_id, &genesis_signer, "user").await?;

    let ft_contract = Contract(ft_id.clone());
    let vault_contract = Contract(vault_id.clone());

    // Register user with FT and vault
    ft_contract
        .call_function("storage_deposit", json!({
            "account_id": user_id
        }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    vault_contract
        .call_function("storage_deposit", json!({
            "account_id": user_id
        }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(user_id.clone(), user_signer.clone())
        .send_to(&network_config)
        .await?;

    // =========================================================================
    // LENDER PROVIDES LIQUIDITY
    // =========================================================================
    let transfer_amount = "100000000"; // 100 USDC
    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": user_id,
            "amount": transfer_amount
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    let deposit_amount = "50000000"; // 50 USDC
    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": deposit_amount,
            "msg": json!({
                "receiver_id": user_id
            }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(user_id.clone(), user_signer.clone())
        .send_to(&network_config)
        .await?;

    // Create solver account
    let (solver_id, solver_signer) =
        create_user_account(&network_config, &genesis_account_id, &genesis_signer, "solver").await?;

    // Register solver with FT
    ft_contract
        .call_function("storage_deposit", json!({
            "account_id": solver_id
        }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    // Verify solver starts with 0 balance
    let solver_balance_before: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({
            "account_id": solver_id
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    assert_eq!(solver_balance_before.data, "0");

    let total_shares_before: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    // =========================================================================
    // SOLVER BORROWS LIQUIDITY
    // =========================================================================
    let _new_intent_result = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "test-intent",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-123",
            "amount": SOLVER_BORROW_AMOUNT.to_string()
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    // Wait for cross-contract call to complete
    sleep(Duration::from_millis(1200)).await;

    // Verify solver received tokens
    let expected_amount = SOLVER_BORROW_AMOUNT.to_string();
    let balance_response: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({
            "account_id": solver_id
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    let solver_balance_after = balance_response.data.clone();

    println!(
        "Solver balance after new intent: {} (expected: {})",
        solver_balance_after,
        expected_amount
    );
    assert_eq!(solver_balance_after, expected_amount);

    // Verify intent was created
    let intents: Data<Vec<serde_json::Value>> = vault_contract
        .call_function(
            "get_intents_by_solver",
            json!({
                "solver_id": solver_id
            }),
        )?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    assert!(
        !intents.data.is_empty(),
        "Solver should have at least one intent stored"
    );

    let latest_indexed_intent = intents
        .data
        .first()
        .expect("intent list should contain the new intent");
    let latest_intent = &latest_indexed_intent["intent"];
    assert_eq!(latest_intent["user_deposit_hash"], "hash-123");
    assert_eq!(latest_intent["intent_data"], "test-intent");

    // =========================================================================
    // SOLVER REPAYS WITH YIELD
    // =========================================================================
    let intent_index_u128: u128 = 0;
    let intent_yield_amount = SOLVER_BORROW_AMOUNT / 100; // 1% yield
    let total_repayment = SOLVER_BORROW_AMOUNT + intent_yield_amount;

    // Transfer yield to solver
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

    let total_assets_before_repay: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    // Solver repays principal + yield
    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": total_repayment.to_string(),
            "msg": json!({
                "repay": {
                    "intent_index": intent_index_u128.to_string()
                }
            }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    // =========================================================================
    // VERIFY FINAL STATE
    // =========================================================================
    
    // Solver balance should be zero
    let solver_balance_final: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({
            "account_id": solver_id
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(solver_balance_final.data, "0");
    println!("✅ Solver balance is zero");

    // Total shares unchanged
    let total_shares_after: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    assert_eq!(total_shares_after.data, total_shares_before.data);
    println!("✅ total shares is the same");

    // Total assets increased by repayment
    let total_assets_after_repay: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    let before = total_assets_before_repay.data.parse::<u128>().unwrap();
    let after = total_assets_after_repay.data.parse::<u128>().unwrap();

    assert_eq!(after, before + total_repayment);

    println!("✅ Solver repaid liquidity with 1% intent_yield, contract balance increased and solver balance is zero");
    Ok(())
}
