# ✅ Sandbox Testing Setup Complete!

Your NEAR smart contract testing environment using `near-sandbox-rs` is now fully configured and working!

## What Was Fixed

The initial test file had several API mismatches with the `near-api` crate. Here's what was corrected:

### 1. **Contract Instantiation**

-   ❌ **Wrong**: `Contract::new(contract_id, account_id)`
-   ✅ **Correct**: `Contract(contract_id)` - Contract is a tuple struct wrapping AccountId

### 2. **Calling Contract Functions**

-   ❌ **Wrong**: `Account::call_function()` static method
-   ✅ **Correct**: Instance method on Contract:

```rust
let contract = Contract(contract_id);
contract.call_function("method_name", args)?
    .transaction()
    .with_signer(account_id, signer)
    .send_to(&network_config)
    .await?;
```

### 3. **View Functions (Read-Only)**

-   ❌ **Wrong**: `Account::view_function()` and using `.result` field
-   ✅ **Correct**: Use `.read_only()` and `.data` field with type annotation:

```rust
let result: Data<Vec<u8>> = contract
    .call_function("get_data", args)?
    .read_only()
    .fetch_from(&network_config)
    .await?;

let data: serde_json::Value = serde_json::from_slice(&result.data)?;
```

### 4. **Contract Deployment**

-   ❌ **Wrong**: `Account::deploy_contract()`
-   ✅ **Correct**: `Contract::deploy()` with proper init call:

```rust
Contract::deploy(contract_id)
    .use_code(wasm_bytes)
    .with_init_call("init", json!({ "owner_id": owner_id }))?
    .with_signer(signer)
    .send_to(&network_config)
    .await?;
```

## Files Created/Modified

1. **`tests/sandbox_test.rs`** - Complete test suite with 4 tests:

    - `test_contract_deployment` - Basic deployment test
    - `test_register_agent` - Agent registration workflow
    - `test_approve_codehash` - Owner approval functionality
    - `test_full_workflow` - End-to-end integration test

2. **`TESTING.md`** - Comprehensive testing guide

3. **`test.sh`** - Convenient test script

## How to Use

### Build the Contract

```bash
cd /home/matt/Projects/mattlockyer/convert-cash-solver/contracts/proxy
cargo near build
```

### Run All Tests

```bash
cargo test
```

### Run Specific Test

```bash
cargo test test_register_agent
```

### Run with Verbose Output

```bash
cargo test -- --nocapture
```

### Use the Test Script

```bash
./test.sh                  # Run all tests
./test.sh --verbose        # Verbose output
./test.sh --debug          # With debug logging
./test.sh --test test_name # Run specific test
```

## Test Structure

Each test follows this pattern:

1. **Start Sandbox** - Automatically starts a local NEAR node
2. **Setup Accounts** - Creates genesis account and any needed test accounts
3. **Deploy Contract** - Deploys and initializes your contract
4. **Execute Test Logic** - Calls contract methods and verifies results
5. **Cleanup** - Sandbox automatically cleans up when test completes

## Key Dependencies

```toml
[dev-dependencies]
near-sandbox = "0.2.1"  # Sandbox environment
tokio = { version = "1", features = ["full"] }  # Async runtime
near-api = "0.6.1"  # NEAR API for Rust
```

## Next Steps

1. ✅ Build your contract: `cargo near build`
2. ✅ Run the tests: `cargo test`
3. ✅ Add more tests as needed for your contract methods
4. ✅ Customize test scenarios based on your business logic

## Troubleshooting

### Contract WASM not found

```bash
cargo near build
```

### Tests timing out

```bash
NEAR_RPC_TIMEOUT_SECS=30 cargo test
```

### Debug sandbox issues

```bash
NEAR_ENABLE_SANDBOX_LOG=1 cargo test -- --nocapture
```

## Example Test Output

When tests run successfully, you'll see:

```
running 4 tests
test test_contract_deployment ... ok
test test_register_agent ... ok
test test_approve_codehash ... ok
test test_full_workflow ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Additional Resources

-   [near-sandbox-rs Documentation](https://docs.rs/near-sandbox)
-   [near-sandbox-rs GitHub](https://github.com/near/near-sandbox-rs)
-   [near-api Documentation](https://docs.rs/near-api)
-   [NEAR Smart Contract Docs](https://docs.near.org/develop/testing/introduction)

---

**Status**: ✅ All tests compile successfully!

**Last Updated**: 2025-10-29
