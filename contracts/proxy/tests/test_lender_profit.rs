mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_multi_lender_profit_distribution() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    for account_id in [&lender1_id, &lender2_id, &solver_id] {
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

    vault_contract
        .call_function("storage_deposit", json!({ "account_id": lender2_id }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(lender2_id.clone(), lender2_signer.clone())
        .send_to(&network_config)
        .await?;

    vault_contract
        .call_function("storage_deposit", json!({ "account_id": solver_id }))?
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    let deposit_amount = 50_000_000u128;
    let premium_amount = (SOLVER_BORROW_AMOUNT * 10) / 100;

    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": lender1_id,
            "amount": deposit_amount.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": deposit_amount.to_string(),
            "msg": json!({ "receiver_id": lender1_id }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender1_id.clone(), lender1_signer.clone())
        .send_to(&network_config)
        .await?;

    let lender1_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_shares_u128 = lender1_shares.data.parse::<u128>().unwrap();

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

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": (SOLVER_BORROW_AMOUNT + premium_amount).to_string(),
            "msg": json!({ "repay": { "intent_index": "0" } }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(solver_id.clone(), solver_signer.clone())
        .send_to(&network_config)
        .await?;

    ft_contract
        .call_function("ft_transfer", json!({
            "receiver_id": lender2_id,
            "amount": deposit_amount.to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    ft_contract
        .call_function("ft_transfer_call", json!({
            "receiver_id": vault_id,
            "amount": deposit_amount.to_string(),
            "msg": json!({ "receiver_id": lender2_id }).to_string()
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(lender2_id.clone(), lender2_signer.clone())
        .send_to(&network_config)
        .await?;

    let lender2_shares: Data<String> = vault_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_shares_u128 = lender2_shares.data.parse::<u128>().unwrap();
    println!(
        "Assertion: lender2 shares < lender1 shares | lender1_shares={} lender2_shares={}",
        lender1_shares_u128, lender2_shares_u128
    );
    assert!(lender2_shares_u128 < lender1_shares_u128);

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

    let lender1_final_balance: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender1_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender1_final_u128 = lender1_final_balance.data.parse::<u128>().unwrap();

    println!(
        "Assertion: lender1 final balance equals deposit + premium | final_balance={} deposit_amount={} premium_amount={}",
        lender1_final_u128, deposit_amount, premium_amount
    );
    assert_eq!(lender1_final_u128, deposit_amount + premium_amount);

    let remaining_assets_after_l1: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let remaining_after_l1 = remaining_assets_after_l1.data.parse::<u128>().unwrap();

    println!(
        "Assertion: remaining assets after lender1 redemption equals lender2 deposit | remaining_assets={} expected={}",
        remaining_after_l1, deposit_amount
    );
    assert_eq!(remaining_after_l1, deposit_amount);

    let total_shares_before_l2: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!(
        "Pre-lender2 redeem totals | total_shares={} total_assets={}",
        total_shares_before_l2.data, remaining_after_l1
    );

    let preview_l2_assets: Data<String> = vault_contract
        .call_function(
            "convert_to_assets",
            json!({ "shares": lender2_shares_u128.to_string() }),
        )?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!(
        "convert_to_assets view for lender2 shares | shares={} preview_assets={}",
        lender2_shares_u128, preview_l2_assets.data
    );

    let lender2_redeem_amount = lender2_shares_u128.to_string();
    let lender2_redeem_outcome = vault_contract
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

    println!(
        "lender2 redeem outcome status: {:?}",
        lender2_redeem_outcome.status
    );
    if !lender2_redeem_outcome.receipts_outcome.is_empty() {
        for (idx, receipt) in lender2_redeem_outcome.receipts_outcome.iter().enumerate() {
            println!(
                "lender2 redeem receipt[{idx}] status={:?} logs={:?}",
                receipt.outcome.status, receipt.outcome.logs
            );
        }
    }

    sleep(Duration::from_millis(1200)).await;

    let lender2_final_balance: Data<String> = ft_contract
        .call_function("ft_balance_of", json!({ "account_id": lender2_id }))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    let lender2_final_u128 = lender2_final_balance.data.parse::<u128>().unwrap();

    println!(
        "Assertion: lender2 final balance equals original deposit | final_balance={} expected={}",
        lender2_final_u128, deposit_amount
    );
    assert_eq!(lender2_final_u128, deposit_amount);

    let total_assets_final: Data<String> = vault_contract
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!(
        "Assertion: total assets final equals zero | total_assets={}",
        total_assets_final.data
    );
    assert_eq!(total_assets_final.data, "0");

    let total_shares_final: Data<String> = vault_contract
        .call_function("ft_total_supply", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    println!(
        "Assertion: total shares final equals zero | total_shares={}",
        total_shares_final.data
    );
    assert_eq!(total_shares_final.data, "0");

    Ok(())
}
