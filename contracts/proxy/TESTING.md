# Testing with near-sandbox-rs

This guide explains how to test your NEAR smart contract using `near-sandbox-rs`.

## Prerequisites

-   Rust v1.85.0 or newer
-   MacOS (ARM64) or Linux (x86)

## System Requirements (Linux Only)

The NEAR sandbox requires specific kernel parameters to be set. You'll need to configure these before running tests:

### Quick Setup (Temporary - Until Reboot)

```bash
./scripts/set_kernel_params.sh
```

### Permanent Setup (Recommended)

```bash
./scripts/set_kernel_params_permanent.sh
```

### Manual Setup

If you prefer to set these manually:

**Temporary (until reboot):**

```bash
sudo sysctl -w net.core.rmem_max=8388608
sudo sysctl -w net.core.wmem_max=8388608
sudo sysctl -w net.ipv4.tcp_rmem="4096 87380 8388608"
sudo sysctl -w net.ipv4.tcp_wmem="4096 16384 8388608"
sudo sysctl -w net.ipv4.tcp_slow_start_after_idle=0
```

**Permanent (survives reboots):**

```bash
sudo tee /etc/sysctl.d/99-near-sandbox.conf > /dev/null << EOF
net.core.rmem_max = 8388608
net.core.wmem_max = 8388608
net.ipv4.tcp_rmem = 4096 87380 8388608
net.ipv4.tcp_wmem = 4096 16384 8388608
net.ipv4.tcp_slow_start_after_idle = 0
EOF
sudo sysctl -p /etc/sysctl.d/99-near-sandbox.conf
```

## Setup

The dependencies are already configured in `Cargo.toml`:

```toml
[dev-dependencies]
near-sandbox = "0.2.1"
tokio = { version = "1", features = ["full"] }
near-api = "0.6.1"
```

## Building the Contract

Before running tests, you need to build the contract WASM file:

```bash
# Build the contract for NEAR
cargo near build

# Or use the standard build command
cargo build --target wasm32-unknown-unknown --release
```

This will create the WASM file at `target/near/contract.wasm`.

## Running Tests

Run all tests:

```bash
cargo test
```

Run a specific test:

```bash
cargo test test_register_agent
```

Run tests with output:

```bash
cargo test -- --nocapture
```

## Test Structure

The tests are located in `tests/sandbox_test.rs` and include:

### 1. **test_contract_deployment**

Tests basic contract deployment to the sandbox.

### 2. **test_register_agent**

Tests the agent registration workflow:

-   Deploys the contract
-   Creates a worker account
-   Registers an agent with a codehash
-   Verifies the registration

### 3. **test_approve_codehash**

Tests the codehash approval functionality (owner-only).

### 4. **test_full_workflow**

Tests the complete workflow:

-   Deploy contract
-   Create worker account
-   Approve codehash (as owner)
-   Register agent with approved codehash
-   Verify registration

## Writing Your Own Tests

Here's a basic template for writing new tests:

```rust
#[tokio::test]
async fn test_my_feature() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Start sandbox
    let sandbox = Sandbox::start_sandbox().await?;
    let network_config = create_network_config(&sandbox);

    // 2. Setup genesis account
    let (genesis_account_id, genesis_signer) = setup_genesis_account().await;

    // 3. Deploy contract
    let contract_id = deploy_contract(&network_config, &genesis_account_id, &genesis_signer).await?;

    // 4. Your test logic here
    // Call contract methods using Account::call_function()
    // View contract state using Account::view_function()

    Ok(())
}
```

## Key Concepts

### Network Configuration

```rust
let network_config = NetworkConfig {
    network_name: "sandbox".to_string(),
    rpc_endpoints: vec![RPCEndpoint::new(sandbox.rpc_addr.parse().unwrap())],
    ..NetworkConfig::testnet()
};
```

### Creating Accounts

```rust
let account_id: AccountId = "myaccount.test.near".parse()?;
let secret_key = signer::generate_secret_key()?;

Account::create_account(account_id.clone())
    .fund_myself(genesis_account_id, NearToken::from_near(5))
    .public_key(secret_key.public_key())
    .unwrap()
    .with_signer(genesis_signer)
    .send_to(&network_config)
    .await?;
```

### Calling Contract Methods (Change State)

```rust
let contract = Contract(contract_id.clone());
contract
    .call_function("my_method", json!({
        "param1": "value1"
    }))?
    .transaction()
    .with_signer(account_id, signer)
    .send_to(&network_config)
    .await?;
```

### Viewing Contract State (Read-Only)

```rust
let contract = Contract(contract_id.clone());
let result: Data<Vec<u8>> = contract
    .call_function("get_data", json!({
        "key": "some_key"
    }))?
    .read_only()
    .fetch_from(&network_config)
    .await?;

let data: serde_json::Value = serde_json::from_slice(&result.data)?;
```

## Environment Variables

Customize sandbox behavior:

-   `NEAR_SANDBOX_BIN_PATH`: Use a custom sandbox binary
-   `NEAR_RPC_TIMEOUT_SECS`: Set RPC timeout (default: 10)
-   `NEAR_ENABLE_SANDBOX_LOG=1`: Enable sandbox logging
-   `NEAR_SANDBOX_LOG`: Custom log levels (e.g., `debug`, `info`)

Example:

```bash
NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test -- --nocapture
```

## Troubleshooting

### Contract WASM not found

Make sure to build the contract first:

```bash
cargo near build
```

### Sandbox timeout

Increase the timeout:

```bash
NEAR_RPC_TIMEOUT_SECS=30 cargo test
```

### See sandbox logs

Enable logging to debug issues:

```bash
NEAR_ENABLE_SANDBOX_LOG=1 cargo test -- --nocapture
```

## Running the Example Binary

You can also run the example sandbox binary:

```bash
cargo run --bin sandbox
```

This demonstrates basic account creation and token transfers.

## Additional Resources

-   [near-sandbox-rs Documentation](https://docs.rs/near-sandbox)
-   [near-sandbox-rs GitHub](https://github.com/near/near-sandbox-rs)
-   [near-api Documentation](https://docs.rs/near-api)
-   [NEAR Smart Contract Testing Guide](https://docs.near.org/develop/testing/introduction)
