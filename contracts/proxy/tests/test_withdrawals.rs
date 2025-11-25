mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;

#[tokio::test]
async fn test_withdrawals() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox and config
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // Deploy vault + mock FT
    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;

    let ft = Contract(ft_id.clone());
    let vault = Contract(vault_id.clone());

    // Ensure vault registered in FT (helpers already do this during deployment)
    // Fund vault assets using donate=true (no share minting)
    ft.call_function("ft_transfer_call", json!({
        "receiver_id": vault_id,
        "amount": "1000000", // 1 USDC
        "msg": json!({
            "donate": true
        }).to_string()
    }))?
    .transaction()
    .deposit(NearToken::from_yoctonear(1))
    .with_signer(genesis_account_id.clone(), genesis_signer.clone())
    .send_to(&network_config)
    .await?;

    // Verify total assets increased
    let total_assets: Data<String> = vault
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(total_assets.data, "1000000");

    // Ensure the FT contract has its own storage registration (required for receiver_id = token)
    ft.call_function("storage_deposit", json!({
        "account_id": ft_id
    }))?
    .transaction()
    .deposit(NearToken::from_millinear(10))
    .with_signer(genesis_account_id.clone(), genesis_signer.clone())
    .send_to(&network_config)
    .await?;

    // Call withdraw_omft_to_evm as owner (genesis)
    let outcome = vault
        .call_function("withdraw_omft_to_evm", json!({
            "token_contract": ft_id,
            "amount": "500000",
            "evm_address": "0x1111111111111111111111111111111111111111"
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    // Debug outcome to inspect exact status and logs
    println!("withdraw_omft_to_evm outcome status: {:?}", outcome.status);
    if !outcome.receipts_outcome.is_empty() {
        for (idx, receipt) in outcome.receipts_outcome.iter().enumerate() {
            println!(
                "withdraw_omft_to_evm receipt[{idx}] status={:?} logs={:?}",
                receipt.outcome.status, receipt.outcome.logs
            );
        }
    }
    // Consider it success if status contains "Success" (version-agnostic)
    let status_str = format!("{:?}", outcome.status);
    assert!(status_str.contains("Success"));

    Ok(())
}

#[tokio::test]
async fn test_withdraw_omft_to_solana_enqueues_transfer() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start sandbox and config
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // Deploy vault + mock FT
    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;

    let ft = Contract(ft_id.clone());
    let vault = Contract(vault_id.clone());

    // Fund vault assets using donate=true (no share minting)
    ft.call_function("ft_transfer_call", json!({
        "receiver_id": vault_id,
        "amount": "800000", // 0.8 USDC
        "msg": json!({
            "donate": true
        }).to_string()
    }))?
    .transaction()
    .deposit(NearToken::from_yoctonear(1))
    .with_signer(genesis_account_id.clone(), genesis_signer.clone())
    .send_to(&network_config)
    .await?;

    // Ensure the FT contract has its own storage registration (required for receiver_id = token)
    ft.call_function("storage_deposit", json!({
        "account_id": ft_id
    }))?
    .transaction()
    .deposit(NearToken::from_millinear(10))
    .with_signer(genesis_account_id.clone(), genesis_signer.clone())
    .send_to(&network_config)
    .await?;

    // Call withdraw_omft_to_solana as owner (genesis)
    let outcome = vault
        .call_function("withdraw_omft_to_solana", json!({
            "token_contract": ft_id,
            "amount": "300000",
            "sol_address": "1111111111111111111111111111111111111111111111111111111111111111"
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    // Debug outcome to inspect exact status and logs
    println!("withdraw_omft_to_solana outcome status: {:?}", outcome.status);
    if !outcome.receipts_outcome.is_empty() {
        for (idx, receipt) in outcome.receipts_outcome.iter().enumerate() {
            println!(
                "withdraw_omft_to_solana receipt[{idx}] status={:?} logs={:?}",
                receipt.outcome.status, receipt.outcome.logs
            );
        }
    }
    // Consider it success if status contains "Success" (version-agnostic)
    let status_str = format!("{:?}", outcome.status);
    assert!(status_str.contains("Success"));

    Ok(())
}

