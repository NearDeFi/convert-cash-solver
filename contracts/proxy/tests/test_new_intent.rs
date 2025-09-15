
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
fn test_new_intent() {
    // Set up test context
    let owner_id = accounts(0);
    let context = get_context(owner_id.clone());
    testing_env!(context);

    // Create the contract
    let mut contract = Contract::init(owner_id.clone());

    // Provided payload data
    let payload = r#"{"signer_id":"hasselalcala.near","nonce":"jrhUEbOQGtxdOnId2jN4JB416K7eOFQ6VPj7p2F9uKQ=","verifying_contract":"intents.near","deadline":"2025-12-14T15:59:24.801Z","intents":[{"intent":"token_diff","diff":{"nep141:wrap.near":"-1000000000000000000000000","nep141:eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near":"2588686"}},{"intent":"ft_withdraw","token":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","receiver_id":"eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near","amount":"2588686","memo":"WITHDRAW_TO:0xa48c13854fa61720c652e2674Cfa82a5F8514036"}]}"#.to_string(); 
    let signature = "secp256k1:2Xtm6Pw6tL5vUmtej7daZe2WwZTKeNCGKpzCnKxRQzm1h68FY28au49G7hL3t22bwsRno5ysYnGtC6EXQE6FAaPHH".to_string();
    let quote_hash = "HpfTtkJFtRdQjeSShxHZxjqo1dQk9KqVLEmpt1mrPRKn".to_string();

    // Call the new_intent method
    contract.new_intent(payload.clone(), signature.clone(), quote_hash.clone());

    // Verify that the intent was created correctly
    let intents = contract.get_intents();
    assert_eq!(intents.len(), 1, "Should have exactly one intent");

    let intent = &intents[0];
    
    // Verify that the state is Signed
    assert_eq!(intent.state, contract::intents::State::Signed, "State should be Signed");
    
    // Verify that the fields were stored correctly
    assert_eq!(intent.payload, payload, "Payload should match");
    assert_eq!(intent.signature, signature, "Signature should match");
    assert_eq!(intent.quote_hash, quote_hash, "Quote hash should match");
    
    // Verify that optional fields are None initially
    assert_eq!(intent.amount, None, "Amount should be None initially");
    assert_eq!(intent.deposit_hash, None, "Deposit hash should be None initially");
    assert_eq!(intent.swap_hash, None, "Swap hash should be None initially");
    
    // Verify that the creation timestamp is greater than 0
    assert!(intent.created > 0, "Creation timestamp should be greater than 0");
}

#[test]
fn test_multiple_intents() {
    // Set up test context
    let owner_id = accounts(0);
    let context = get_context(owner_id.clone());
    testing_env!(context);

    // Create the contract
    let mut contract = Contract::init(owner_id.clone());

    // Create multiple intents
    let payload1 = r#"{"signer_id":"user1.near","nonce":"nonce1","verifying_contract":"intents.near","deadline":"2025-12-14T15:59:24.801Z","intents":[]}"#.to_string();
    let signature1 = "secp256k1:signature1".to_string();
    let quote_hash1 = "hash1".to_string();

    let payload2 = r#"{"signer_id":"user2.near","nonce":"nonce2","verifying_contract":"intents.near","deadline":"2025-12-14T15:59:24.801Z","intents":[]}"#.to_string();
    let signature2 = "secp256k1:signature2".to_string();
    let quote_hash2 = "hash2".to_string();

    // Add both intents
    contract.new_intent(payload1.clone(), signature1.clone(), quote_hash1.clone());
    contract.new_intent(payload2.clone(), signature2.clone(), quote_hash2.clone());

    // Verify that both intents were created
    let intents = contract.get_intents();
    assert_eq!(intents.len(), 2, "Should have exactly two intents");

    // Verify the first intent
    assert_eq!(intents[0].payload, payload1);
    assert_eq!(intents[0].signature, signature1);
    assert_eq!(intents[0].quote_hash, quote_hash1);

    // Verify the second intent
    assert_eq!(intents[1].payload, payload2);
    assert_eq!(intents[1].signature, signature2);
    assert_eq!(intents[1].quote_hash, quote_hash2);
}