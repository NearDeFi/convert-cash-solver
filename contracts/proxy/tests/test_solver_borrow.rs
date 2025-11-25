// Test solver receives mock FT tokens when creating an intent

mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_solver_borrow_liquidity() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);

    // Setup genesis account
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // Deploy vault (which also deploys mock FT)
    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;

    // Create user and deposit assets into vault so rewards can be paid
    let (user_id, user_signer) =
        create_user_account(&network_config, &genesis_account_id, &genesis_signer, "user").await?;

    let ft_contract = Contract(ft_id.clone());
    let vault_contract = Contract(vault_id.clone());

    // Register user with FT contract and vault
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

    // Transfer USDC to user and deposit into vault (provide liquidity)
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

    // Register solver with FT contract so they can receive tokens
    ft_contract
        .call_function("storage_deposit", json!({
            "account_id": solver_id
        }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    // Record solver balance before creating intent
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

    // Solver creates new intent (should receive mock FT tokens)
    let _new_intent_result = vault_contract
        .call_function("new_intent", json!({
            "intent_data": "test-intent",
            "_solver_deposit_address": solver_id,
            "user_deposit_hash": "hash-123"
        }))?
        .transaction()
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    // Wait 2 blocks because the transfer is async
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

    // Fetch intents stored for the solver and ensure the new intent was recorded
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

    let latest_intent = intents
        .data
        .first()
        .expect("intent list should contain the new intent");
    assert_eq!(latest_intent["user_deposit_hash"], "hash-123");
    assert_eq!(latest_intent["intent_data"], "test-intent");

    // Only intent so far has index 0
    let intent_index_u128: u128 = 0;

    // Give solver an extra 1% to repay with a premium
    let premium_amount = SOLVER_BORROW_AMOUNT / 100; // 1% premium
    let total_repayment = SOLVER_BORROW_AMOUNT + premium_amount;

    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": solver_id,
            "amount": premium_amount.to_string()
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

    // Solver repays borrowed liquidity plus premium
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

    // Verify solver balance is zero
    let solver_balance_final: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({
            "account_id": solver_id
        }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(solver_balance_final.data, "0");
    println!("✅ Solver balance is zero");

    let total_shares_after: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    assert_eq!(total_shares_after.data, total_shares_before.data);
    println!("✅ total shares is the same");

    // Verify contract assets increased by 10%
    let total_assets_after_repay: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;

    let before = total_assets_before_repay.data.parse::<u128>().unwrap();
    let after = total_assets_after_repay.data.parse::<u128>().unwrap();

    assert_eq!(after, before + total_repayment);

    println!("✅ Solver repaid liquidity with 1% premium, contract balance increased and solver balance is zero");
    Ok(())
}

