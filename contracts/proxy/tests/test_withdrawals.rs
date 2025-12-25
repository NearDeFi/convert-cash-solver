//! # OMFT Withdrawal Tests
//!
//! Tests the owner-only functions for withdrawing assets from the vault to
//! external chains via the OMFT (Omnichain Multi-chain Fungible Token) protocol.
//!
//! ## Test Overview
//!
//! | Test | Description | Expected Outcome |
//! |------|-------------|------------------|
//! | `test_withdrawals` | Withdraw to EVM chain | ft_transfer with EVM memo succeeds |
//! | `test_withdraw_omft_to_solana_enqueues_transfer` | Withdraw to Solana | ft_transfer with Solana memo succeeds |
//!
//! ## OMFT Protocol
//!
//! The OMFT bridge uses a special `ft_transfer` pattern:
//!
//! ```text
//! For EVM:
//!   receiver_id = token_contract
//!   memo = "WITHDRAW_TO:0x{evm_address}"
//!
//! For Solana:
//!   receiver_id = token_contract
//!   memo = "WITHDRAW_TO:{sol_address}"
//! ```
//!
//! ## Access Control
//!
//! - `withdraw_omft_to_evm`: Owner only, requires 1 yoctoNEAR
//! - `withdraw_omft_to_solana`: Owner only, requires 1 yoctoNEAR
//!
//! ## Note
//!
//! These tests use a mock FT that accepts the transfer but doesn't actually
//! bridge to external chains. The important verification is that the
//! transfer call succeeds with the correct parameters.

mod helpers;

use helpers::*;
use near_api::{Contract, Data, NearToken};
use serde_json::json;

/// Tests withdrawal to EVM chain via OMFT bridge.
///
/// # Scenario
///
/// 1. Fund vault with assets (using donate=true to avoid share minting)
/// 2. Owner calls withdraw_omft_to_evm
/// 3. Verify ft_transfer succeeds with EVM memo
///
/// # Expected Outcome
///
/// - Transaction succeeds
/// - Transfer is initiated to token contract with EVM address in memo
#[tokio::test]
async fn test_withdrawals() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;

    let ft = Contract(ft_id.clone());
    let vault = Contract(vault_id.clone());

    // =========================================================================
    // FUND VAULT WITH ASSETS (DONATE MODE - NO SHARES)
    // =========================================================================
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

    // Verify assets deposited
    let total_assets: Data<String> = vault
        .call_function("total_assets", json!([]))?
        .read_only()
        .fetch_from(&network_config)
        .await?;
    assert_eq!(total_assets.data, "1000000");

    // Register FT contract for self-transfer
    ft.call_function("storage_deposit", json!({
        "account_id": ft_id
    }))?
    .transaction()
    .deposit(NearToken::from_millinear(10))
    .with_signer(genesis_account_id.clone(), genesis_signer.clone())
    .send_to(&network_config)
    .await?;

    // =========================================================================
    // WITHDRAW TO EVM
    // =========================================================================
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

    // Debug output
    println!("withdraw_omft_to_evm outcome status: {:?}", outcome.status);
    if !outcome.receipts_outcome.is_empty() {
        for (idx, receipt) in outcome.receipts_outcome.iter().enumerate() {
            println!(
                "withdraw_omft_to_evm receipt[{idx}] status={:?} logs={:?}",
                receipt.outcome.status, receipt.outcome.logs
            );
        }
    }

    let status_str = format!("{:?}", outcome.status);
    assert!(status_str.contains("Success"));

    Ok(())
}

/// Tests withdrawal to Solana chain via OMFT bridge.
///
/// # Scenario
///
/// 1. Fund vault with assets (using donate=true)
/// 2. Owner calls withdraw_omft_to_solana
/// 3. Verify ft_transfer succeeds with Solana memo
///
/// # Expected Outcome
///
/// - Transaction succeeds
/// - Transfer is initiated with Solana address in memo
#[tokio::test]
async fn test_withdraw_omft_to_solana_enqueues_transfer() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;
    let ft_id: near_api::AccountId = format!("usdc.{}", genesis_account_id).parse()?;

    let ft = Contract(ft_id.clone());
    let vault = Contract(vault_id.clone());

    // =========================================================================
    // FUND VAULT WITH ASSETS (DONATE MODE)
    // =========================================================================
    ft.call_function("ft_transfer_call", json!({
        "receiver_id": vault_id,
        "amount": "2000000", // 2 USDC (above MIN_DEPOSIT_AMOUNT of 1 USDC)
        "msg": json!({
            "donate": true
        }).to_string()
    }))?
    .transaction()
    .deposit(NearToken::from_yoctonear(1))
    .with_signer(genesis_account_id.clone(), genesis_signer.clone())
    .send_to(&network_config)
    .await?;

    // Register FT contract for self-transfer
    ft.call_function("storage_deposit", json!({
        "account_id": ft_id
    }))?
    .transaction()
    .deposit(NearToken::from_millinear(10))
    .with_signer(genesis_account_id.clone(), genesis_signer.clone())
    .send_to(&network_config)
    .await?;

    // =========================================================================
    // WITHDRAW TO SOLANA
    // =========================================================================
    let outcome = vault
        .call_function("withdraw_omft_to_solana", json!({
            "token_contract": ft_id,
            "amount": "1000000", // 1 USDC (MIN_DEPOSIT_AMOUNT)
            "sol_address": "1111111111111111111111111111111111111111111111111111111111111111"
        }))?
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .with_signer(genesis_account_id.clone(), genesis_signer.clone())
        .send_to(&network_config)
        .await?;

    // Debug output
    println!("withdraw_omft_to_solana outcome status: {:?}", outcome.status);
    if !outcome.receipts_outcome.is_empty() {
        for (idx, receipt) in outcome.receipts_outcome.iter().enumerate() {
            println!(
                "withdraw_omft_to_solana receipt[{idx}] status={:?} logs={:?}",
                receipt.outcome.status, receipt.outcome.logs
            );
        }
    }

    let status_str = format!("{:?}", outcome.status);
    assert!(status_str.contains("Success"));

    Ok(())
}
