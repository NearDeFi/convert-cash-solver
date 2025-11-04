# Test Structure

This directory contains modular tests for the NEAR smart contract with vault functionality.

## 📁 File Structure

```
tests/
├── README.md                    # This file
├── helpers/
│   └── mod.rs                   # Shared helper functions and constants
├── sandbox_test.rs              # Basic contract tests
└── test_vault_deposit.rs        # Comprehensive vault deposit test
```

## 🔧 Helper Functions (`helpers/mod.rs`)

Shared utilities used across all tests:

### **Constants**

-   `CONTRACT_WASM_PATH` - Path to vault contract WASM
-   `MOCK_FT_WASM_PATH` - Path to mock FT contract WASM
-   `EXTRA_DECIMALS` - Vault extra decimals setting (default: 3)

### **Helper Functions**

-   `create_network_config()` - Creates sandbox network configuration
-   `setup_genesis_account()` - Sets up the default genesis account
-   `deploy_mock_ft()` - Deploys mock USDC fungible token
-   `deploy_vault_contract()` - Deploys vault with mock FT as underlying asset
-   `create_user_account()` - Creates a test user account

## 📝 Test Files

### **`sandbox_test.rs`** - Basic Tests

Simple, fast tests for core functionality:

1. **`test_contract_deployment`** - Verifies contract deploys successfully
2. **`test_approve_codehash`** - Tests codehash approval (owner function)
3. **`test_vault_initialization`** - Verifies vault initial state
4. **`test_vault_conversion_functions`** - Tests asset/share conversions

### **`test_vault_deposit.rs`** - Comprehensive Deposit Test

Full end-to-end test of vault deposit functionality:

1. Deploys vault and mock FT contracts
2. Creates user account
3. Funds user with mock USDC
4. User deposits USDC to vault via `ft_transfer_call`
5. Verifies user receives correct shares (with `extra_decimals` multiplier)
6. Verifies vault accounting (total assets, total shares)

## 🚀 Running Tests

### **Run All Tests**

```bash
cd /home/matt/Projects/mattlockyer/convert-cash-solver/contracts/proxy
./test.sh -v
```

### **Run Specific Test File**

```bash
# Basic tests
./test.sh test_vault_initialization
./test.sh test_vault_conversion_functions

# Vault deposit test
./test.sh test_vault_deposit_and_receive_shares
```

### **Run Individual Tests Directly**

```bash
cargo test test_vault_deposit_and_receive_shares -- --nocapture
cargo test test_vault_initialization -- --nocapture
```

### **Run All Tests in a File**

```bash
# All basic tests
cargo test --test sandbox_test -- --nocapture

# Deposit test
cargo test --test test_vault_deposit -- --nocapture
```

## 📊 Adding New Tests

### **For Simple Tests**

Add to `sandbox_test.rs`:

```rust
#[tokio::test]
async fn test_my_feature() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    let vault_id = deploy_vault_contract(&network_config, &genesis_account_id, &genesis_signer).await?;

    // Your test logic here

    Ok(())
}
```

### **For Complex Tests**

Create a new file `tests/test_my_feature.rs`:

```rust
mod helpers;

use helpers::*;
use near_api::Contract;
use serde_json::json;

#[tokio::test]
async fn test_my_complex_feature() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Use helper functions
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // Your comprehensive test logic

    Ok(())
}
```

## 🎯 Benefits of This Structure

1. **DRY Principle**: Helper functions defined once, used everywhere
2. **Easy to Add Tests**: Just create a new file and import helpers
3. **Fast Iteration**: Simple tests in one file, complex in separate files
4. **Individual Execution**: Each test file can be run independently
5. **Maintainability**: Changes to helpers automatically propagate

## 🔍 Test Organization Guidelines

**Put tests in `sandbox_test.rs` if they:**

-   Are quick to run (< 10 seconds)
-   Test a single function or simple workflow
-   Don't require complex setup

**Create separate test file if:**

-   Test is long/complex (> 50 lines)
-   Requires extensive setup (multiple accounts, contracts)
-   Tests complete end-to-end workflows
-   You want to run it separately from other tests

## 📚 Examples

### **Quick Test** (`sandbox_test.rs`)

```rust
#[tokio::test]
async fn test_get_metadata() -> Result<...> {
    // 5-10 lines of simple testing
}
```

### **Complex Test** (separate file)

```rust
// tests/test_vault_withdrawal.rs
#[tokio::test]
async fn test_multi_user_deposits_and_withdrawals() -> Result<...> {
    // 50+ lines testing complex scenarios
}
```

## 🛠️ Maintaining Tests

### **Update Constants**

Change `EXTRA_DECIMALS` in `helpers/mod.rs` to affect all tests

### **Add New Helpers**

Add new helper functions to `helpers/mod.rs`:

```rust
pub async fn my_new_helper(...) -> Result<...> {
    // Implementation
}
```

Then use in any test file:

```rust
use helpers::*;
// my_new_helper is now available
```

## ✅ Current Test Coverage

-   ✅ Contract deployment
-   ✅ Vault initialization
-   ✅ Asset/share conversions with `extra_decimals`
-   ✅ FT deposit to vault
-   ✅ Share issuance
-   ✅ Codehash approval

## 🔜 Future Test Ideas

-   Vault withdrawals (redeem shares for assets)
-   Multiple users depositing (share price changes)
-   Donation functionality
-   Slippage protection (`min_shares`, `max_shares`)
-   Edge cases (zero amounts, max values)
