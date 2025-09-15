use near_sdk::test_utils::{accounts, VMContextBuilder};
use near_sdk::{testing_env, AccountId, VMContext};
use contract::Contract;

fn get_context(predecessor_account_id: AccountId) -> VMContext {
    VMContextBuilder::new()
        .predecessor_account_id(predecessor_account_id)
        .block_timestamp(1_000_000_000_000_000_000) // Set a valid timestamp
        .build()
}

#[test]
fn test_update_deposit_info_near() {
    // Set up test context
    let owner_id = accounts(0);
    let _solver_id = accounts(1);
    let context = get_context(owner_id.clone());
    testing_env!(context);

    // Create the contract
    let mut contract = Contract::init(owner_id.clone());

    // Create an intent first
    let payload = r#"{"signer_id":"hasselalcala.near","nonce":"jrhUEbOQGtxdOnId2jN4JB416K7eOFQ6VPj7p2F9uKQ=","verifying_contract":"intents.near","deadline":"2025-12-14T15:59:24.801Z","intents":[{"intent":"token_diff","diff":{"nep141:wrap.near":"-1000000000000000000000000","nep141:eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near":"2588686"}},{"intent":"ft_withdraw","token":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","receiver_id":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","amount":"2588686","memo":"WITHDRAW_TO:0xa48c13854fa61720c652e2674Cfa82a5F8514036"}]}"#.to_string();
    let signature = "secp256k1:2Xtm6Pw6tL5vUmtej7daZe2WwZTKeNCGKpzCnKxRQzm1h68FY28au49G7hL3t22bwsRno5ysYnGtC6EXQE6FAaPHH".to_string();
    let quote_hash = "HpfTtkJFtRdQjeSShxHZxjqo1dQk9KqVLEmpt1mrPRKn".to_string();

    // Create the intent
    contract.new_intent(payload, signature, quote_hash.clone());

    // Update deposit info with NEAR transaction (after detecting deposit on chain)
    let amount = "1000000000000000000000000".to_string(); // 1 NEAR in yoctoNEAR
    let deposit_hash = "HpfTtkJFtRdQjeSShxHZxjqo1dQk9KqVLEmpt1mrPRKn".to_string(); // NEAR transaction hash format

    contract.update_deposit_info_by_quote_hash(quote_hash.clone(), amount.clone(), deposit_hash.clone());

    // Verify the intent was updated correctly
    let intent = contract.get_intent_by_quote_hash(quote_hash);
    
    assert_eq!(intent.amount, Some(amount), "Amount should be updated");
    assert_eq!(intent.deposit_hash, Some(deposit_hash), "Deposit hash should be updated");
    assert_eq!(intent.state, contract::intents::State::Signed, "State should remain Signed");
}

#[test]
fn test_update_deposit_info_ethereum() {
    // Set up test context
    let owner_id = accounts(0);
    let _solver_id = accounts(1);
    let context = get_context(owner_id.clone());
    testing_env!(context);

    // Create the contract
    let mut contract = Contract::init(owner_id.clone());

    // Create an intent first
    let payload = r#"{"signer_id":"hasselalcala.near","nonce":"jrhUEbOQGtxdOnId2jN4JB416K7eOFQ6VPj7p2F9uKQ=","verifying_contract":"intents.near","deadline":"2025-12-14T15:59:24.801Z","intents":[{"intent":"token_diff","diff":{"nep141:wrap.near":"-1000000000000000000000000","nep141:eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near":"2588686"}},{"intent":"ft_withdraw","token":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","receiver_id":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","amount":"2588686","memo":"WITHDRAW_TO:0xa48c13854fa61720c652e2674Cfa82a5F8514036"}]}"#.to_string();
    let signature = "secp256k1:2Xtm6Pw6tL5vUmtej7daZe2WwZTKeNCGKpzCnKxRQzm1h68FY28au49G7hL3t22bwsRno5ysYnGtC6EXQE6FAaPHH".to_string();
    let quote_hash = "HpfTtkJFtRdQjeSShxHZxjqo1dQk9KqVLEmpt1mrPRKn".to_string();

    // Create the intent
    contract.new_intent(payload, signature, quote_hash.clone());

    // Update deposit info with Ethereum transaction (after detecting deposit on chain)
    let amount = "100000000000000000".to_string(); // 0.1 ETH in wei
    let deposit_hash = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(); // Ethereum transaction hash format

    contract.update_deposit_info_by_quote_hash(quote_hash.clone(), amount.clone(), deposit_hash.clone());

    // Verify the intent was updated correctly
    let intent = contract.get_intent_by_quote_hash(quote_hash);
    
    assert_eq!(intent.amount, Some(amount), "Amount should be updated");
    assert_eq!(intent.deposit_hash, Some(deposit_hash), "Deposit hash should be updated");
    assert_eq!(intent.state, contract::intents::State::Signed, "State should remain Signed");
}

#[test]
fn test_update_deposit_info_bitcoin() {
    // Set up test context
    let owner_id = accounts(0);
    let _solver_id = accounts(1);
    let context = get_context(owner_id.clone());
    testing_env!(context);

    // Create the contract
    let mut contract = Contract::init(owner_id.clone());

    // Create an intent first
    let payload = r#"{"signer_id":"hasselalcala.near","nonce":"jrhUEbOQGtxdOnId2jN4JB416K7eOFQ6VPj7p2F9uKQ=","verifying_contract":"intents.near","deadline":"2025-12-14T15:59:24.801Z","intents":[{"intent":"token_diff","diff":{"nep141:wrap.near":"-1000000000000000000000000","nep141:eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near":"2588686"}},{"intent":"ft_withdraw","token":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","receiver_id":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","amount":"2588686","memo":"WITHDRAW_TO:0xa48c13854fa61720c652e2674Cfa82a5F8514036"}]}"#.to_string();
    let signature = "secp256k1:2Xtm6Pw6tL5vUmtej7daZe2WwZTKeNCGKpzCnKxRQzm1h68FY28au49G7hL3t22bwsRno5ysYnGtC6EXQE6FAaPHH".to_string();
    let quote_hash = "HpfTtkJFtRdQjeSShxHZxjqo1dQk9KqVLEmpt1mrPRKn".to_string();

    // Create the intent
    contract.new_intent(payload, signature, quote_hash.clone());

    // Update deposit info with Bitcoin transaction (after detecting deposit on chain)
    let amount = "100000000".to_string(); // 1 BTC in satoshis
    let deposit_hash = "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456".to_string(); // Bitcoin transaction hash format

    contract.update_deposit_info_by_quote_hash(quote_hash.clone(), amount.clone(), deposit_hash.clone());

    // Verify the intent was updated correctly
    let intent = contract.get_intent_by_quote_hash(quote_hash);
    
    assert_eq!(intent.amount, Some(amount), "Amount should be updated");
    assert_eq!(intent.deposit_hash, Some(deposit_hash), "Deposit hash should be updated");
    assert_eq!(intent.state, contract::intents::State::Signed, "State should remain Signed");
}

#[test]
fn test_update_deposit_info_solana() {
    // Set up test context
    let owner_id = accounts(0);
    let _solver_id = accounts(1);
    let context = get_context(owner_id.clone());
    testing_env!(context);

    // Create the contract
    let mut contract = Contract::init(owner_id.clone());

    // Create an intent first
    let payload = r#"{"signer_id":"hasselalcala.near","nonce":"jrhUEbOQGtxdOnId2jN4JB416K7eOFQ6VPj7p2F9uKQ=","verifying_contract":"intents.near","deadline":"2025-12-14T15:59:24.801Z","intents":[{"intent":"token_diff","diff":{"nep141:wrap.near":"-1000000000000000000000000","nep141:eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near":"2588686"}},{"intent":"ft_withdraw","token":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","receiver_id":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","amount":"2588686","memo":"WITHDRAW_TO:0xa48c13854fa61720c652e2674Cfa82a5F8514036"}]}"#.to_string();
    let signature = "secp256k1:2Xtm6Pw6tL5vUmtej7daZe2WwZTKeNCGKpzCnKxRQzm1h68FY28au49G7hL3t22bwsRno5ysYnGtC6EXQE6FAaPHH".to_string();
    let quote_hash = "HpfTtkJFtRdQjeSShxHZxjqo1dQk9KqVLEmpt1mrPRKn".to_string();

    // Create the intent
    contract.new_intent(payload, signature, quote_hash.clone());

    // Update deposit info with Solana transaction (after detecting deposit on chain)
    let amount = "1000000000".to_string(); // 1 SOL in lamports
    let deposit_hash = "5J7X8C9D2E1F4A6B8C0D3E5F7A9B2C4D6E8F0A1B3C5D7E9F2A4B6C8D0E2F4A6B8C".to_string(); // Solana transaction signature format

    contract.update_deposit_info_by_quote_hash(quote_hash.clone(), amount.clone(), deposit_hash.clone());

    // Verify the intent was updated correctly
    let intent = contract.get_intent_by_quote_hash(quote_hash);
    
    assert_eq!(intent.amount, Some(amount), "Amount should be updated");
    assert_eq!(intent.deposit_hash, Some(deposit_hash), "Deposit hash should be updated");
    assert_eq!(intent.state, contract::intents::State::Signed, "State should remain Signed");
}

#[test]
fn test_update_deposit_info_multiple_updates() {
    // Set up test context
    let owner_id = accounts(0);
    let _solver_id = accounts(1);
    let context = get_context(owner_id.clone());
    testing_env!(context);

    // Create the contract
    let mut contract = Contract::init(owner_id.clone());

    // Create an intent first
    let payload = r#"{"signer_id":"hasselalcala.near","nonce":"jrhUEbOQGtxdOnId2jN4JB416K7eOFQ6VPj7p2F9uKQ=","verifying_contract":"intents.near","deadline":"2025-12-14T15:59:24.801Z","intents":[{"intent":"token_diff","diff":{"nep141:wrap.near":"-1000000000000000000000000","nep141:eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near":"2588686"}},{"intent":"ft_withdraw","token":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","receiver_id":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","amount":"2588686","memo":"WITHDRAW_TO:0xa48c13854fa61720c652e2674Cfa82a5F8514036"}]}"#.to_string();
    let signature = "secp256k1:2Xtm6Pw6tL5vUmtej7daZe2WwZTKeNCGKpzCnKxRQzm1h68FY28au49G7hL3t22bwsRno5ysYnGtC6EXQE6FAaPHH".to_string();
    let quote_hash = "HpfTtkJFtRdQjeSShxHZxjqo1dQk9KqVLEmpt1mrPRKn".to_string();

    // Create the intent
    contract.new_intent(payload, signature, quote_hash.clone());

    // First update with NEAR (after detecting deposit on chain)
    let amount1 = "1000000000000000000000000".to_string(); // 1 NEAR
    let deposit_hash1 = "HpfTtkJFtRdQjeSShxHZxjqo1dQk9KqVLEmpt1mrPRKn".to_string();
    contract.update_deposit_info_by_quote_hash(quote_hash.clone(), amount1.clone(), deposit_hash1.clone());

    // Verify first update
    let intent = contract.get_intent_by_quote_hash(quote_hash.clone());
    assert_eq!(intent.amount, Some(amount1), "First amount should be updated");
    assert_eq!(intent.deposit_hash, Some(deposit_hash1), "First deposit hash should be updated");

    // Second update with Ethereum (overwriting the first update)
    let amount2 = "100000000000000000".to_string(); // 0.1 ETH
    let deposit_hash2 = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string();
    contract.update_deposit_info_by_quote_hash(quote_hash.clone(), amount2.clone(), deposit_hash2.clone());

    // Verify second update
    let intent = contract.get_intent_by_quote_hash(quote_hash);
    assert_eq!(intent.amount, Some(amount2), "Second amount should be updated");
    assert_eq!(intent.deposit_hash, Some(deposit_hash2), "Second deposit hash should be updated");
}