# Convert Cash Solver Proxy Contract

A NEAR smart contract that combines a NEP-621 compliant liquidity vault with intent-based cross-chain swap infrastructure. This contract enables solvers to borrow liquidity from lenders to fulfill user swap requests across multiple chains.

## Overview

The proxy contract serves three main functions:

1. **Liquidity Vault**: Accepts deposits from lenders, issues shares (NEP-141), and manages a redemption queue
2. **Intent Solver**: Allows approved solvers to borrow liquidity for cross-chain swap execution
3. **Cross-Chain Bridge**: Integrates with OMFT for withdrawals to EVM and Solana chains

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Proxy Contract                              │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   Vault     │  │   Intents   │  │   Cross-Chain           │  │
│  │  (NEP-621)  │  │  Management │  │   Integrations          │  │
│  ├─────────────┤  ├─────────────┤  ├─────────────────────────┤  │
│  │ • Deposits  │  │ • Borrow    │  │ • MPC Signatures        │  │
│  │ • Shares    │  │ • Repay     │  │ • OMFT Bridge           │  │
│  │ • Redemption│  │ • States    │  │ • NEAR Intents          │  │
│  │ • Queue     │  │ • Yield     │  │                         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Module Structure

```
src/
├── lib.rs              # Contract entry point and initialization
├── vault.rs            # NEP-621 vault implementation
├── intents.rs          # Solver intent management
├── withdraw.rs         # Cross-chain OMFT withdrawals
├── chainsig.rs         # MPC signature requests
├── near_intents.rs     # NEAR Intents protocol integration
├── test_utils.rs       # Unit test helpers
└── vault_standards/
    ├── mod.rs          # Module exports
    ├── core.rs         # VaultCore trait definition
    ├── events.rs       # NEP-000 event logging
    ├── internal.rs     # Internal conversion helpers
    └── mul_div.rs      # Safe arithmetic operations
```

## Key Features

### Vault Operations

-   **Deposit**: Send assets via `ft_transfer_call` to receive vault shares
-   **Redemption**: Burn shares to receive proportional assets
-   **Queue System**: FIFO queue for redemptions when liquidity is borrowed
-   **Yield Distribution**: 1% yield from solver repayments distributed to lenders

### Intent System

-   **Borrow**: Solvers borrow liquidity from the vault to fulfill swaps
-   **Repay**: Solvers return principal + 1% yield after completing swaps
-   **State Tracking**: Full lifecycle tracking of intent states
-   **Duplicate Prevention**: Prevents duplicate intents for the same user deposit

### Cross-Chain

-   **MPC Signatures**: Request ECDSA/EdDSA signatures from NEAR's MPC network
-   **OMFT Bridge**: Withdraw to EVM chains or Solana via the OMFT protocol
-   **NEAR Intents**: Manage authorized signing keys for intent operations

## Building

### Prerequisites

-   Rust 1.86+ with `wasm32-unknown-unknown` target
-   [cargo-near](https://github.com/near/cargo-near) for WASM compilation

### Install Dependencies

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WASM target
rustup target add wasm32-unknown-unknown

# Install cargo-near
cargo install cargo-near
```

### Build the Contract

```bash
# Build optimized WASM (non-reproducible, faster)
cargo near build non-reproducible-wasm

# Build reproducible WASM (for verification)
cargo near build
```

The compiled WASM will be at `target/wasm32-unknown-unknown/release/contract.wasm`.

## Testing

### Unit Tests

Run the Rust unit tests:

```bash
# Run all unit tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_vault_deposit
```

### Sandbox Integration Tests

The contract includes comprehensive sandbox tests that run against a local NEAR node:

```bash
# Build and run all tests
./test.sh

# Run with verbose output
./test.sh -v

# Run specific test file
./test.sh -t test_vault_deposit

# Run all tests in sequence with output
./test.sh -a

# Skip build (tests only)
./test.sh --no-build
```

### Available Test Suites

| Test File                               | Description                 |
| --------------------------------------- | --------------------------- |
| `test_vault_deposit.rs`                 | Deposit and share minting   |
| `test_withdrawals.rs`                   | Withdrawal and redemption   |
| `test_solver_borrow.rs`                 | Solver borrowing mechanics  |
| `test_lender_profit.rs`                 | Yield distribution          |
| `test_fifo_redemption_queue.rs`         | Queue processing            |
| `test_multi_solver.rs`                  | Multiple concurrent solvers |
| `test_rounding_nep621.rs`               | NEP-621 rounding compliance |
| `test_complex_multi_lender_scenario.rs` | Complex scenarios           |

## Deployment

### Testnet

```bash
# Deploy to testnet
near deploy --accountId your-contract.testnet \
  --wasmFile target/wasm32-unknown-unknown/release/contract.wasm

# Initialize
near call your-contract.testnet init '{
  "owner_id": "your-account.testnet",
  "asset": "usdc.fakes.testnet",
  "metadata": {
    "spec": "ft-1.0.0",
    "name": "USDC Vault Shares",
    "symbol": "vUSDC",
    "decimals": 24
  },
  "extra_decimals": 3
}' --accountId your-account.testnet
```

### Mainnet

```bash
# Deploy to mainnet (use with caution)
near deploy --accountId your-contract.near \
  --wasmFile target/wasm32-unknown-unknown/release/contract.wasm \
  --networkId mainnet
```

## Contract Methods

### Vault Methods

| Method                    | Access        | Description                             |
| ------------------------- | ------------- | --------------------------------------- |
| `ft_on_transfer`          | Asset token   | Handles deposits via `ft_transfer_call` |
| `redeem`                  | Any (1 yocto) | Burns shares for assets                 |
| `withdraw`                | Any (1 yocto) | Withdraws specific asset amount         |
| `process_next_redemption` | Any           | Processes queued redemptions            |
| `ft_transfer`             | Any (1 yocto) | Transfers vault shares                  |
| `ft_balance_of`           | View          | Returns share balance                   |
| `ft_total_supply`         | View          | Returns total shares                    |
| `total_assets`            | View          | Returns vault asset balance             |
| `get_pending_redemptions` | View          | Returns redemption queue                |

### Intent Methods

| Method                  | Access | Description                     |
| ----------------------- | ------ | ------------------------------- |
| `new_intent`            | Solver | Borrows liquidity for an intent |
| `update_intent_state`   | Solver | Updates intent state            |
| `get_intents`           | View   | Returns all intents             |
| `get_intents_by_solver` | View   | Returns solver's intents        |

### Admin Methods

| Method                    | Access          | Description                |
| ------------------------- | --------------- | -------------------------- |
| `approve_codehash`        | Owner           | Approves TEE codehash      |
| `register_agent`          | Any             | Registers worker agent     |
| `withdraw_omft_to_evm`    | Owner (1 yocto) | Withdraws to EVM chain     |
| `withdraw_omft_to_solana` | Owner (1 yocto) | Withdraws to Solana        |
| `clear_intents`           | Owner           | Clears all intents (debug) |

### Signature Methods

| Method              | Access | Description              |
| ------------------- | ------ | ------------------------ |
| `request_signature` | Any    | Requests MPC signature   |
| `add_public_key`    | Any    | Adds key to Intents      |
| `remove_public_key` | Any    | Removes key from Intents |

## Example Flows

### Lender Deposit Flow

```bash
# 1. Register for storage (if first time)
near call vault.near storage_deposit '' \
  --accountId lender.near --deposit 0.00125N

# 2. Deposit USDC and receive shares
near call usdc.near ft_transfer_call '{
  "receiver_id": "vault.near",
  "amount": "1000000",
  "msg": "{\"deposit\":{}}"
}' --accountId lender.near --depositYocto 1 --gas 100Tgas
```

### Solver Borrow Flow

```bash
# 1. Create intent and borrow liquidity
near call vault.near new_intent '{
  "intent_data": "{\"swap\":\"ETH->USDC\"}",
  "_solver_deposit_address": "solver.near",
  "user_deposit_hash": "0x123...",
  "amount": "5000000"
}' --accountId solver.near --gas 100Tgas

# 2. (Solver executes cross-chain swap off-chain)

# 3. Repay with yield
near call usdc.near ft_transfer_call '{
  "receiver_id": "vault.near",
  "amount": "5050000",
  "msg": "{\"repay\":{\"intent_index\":\"0\"}}"
}' --accountId solver.near --depositYocto 1 --gas 100Tgas
```

### Lender Redemption Flow

```bash
# Redeem shares for assets
near call vault.near redeem '{
  "shares": "1000000000",
  "receiver_id": null,
  "memo": null
}' --accountId lender.near --depositYocto 1 --gas 100Tgas

# If queued, wait for repayment then process
near call vault.near process_next_redemption '' \
  --accountId anyone.near --gas 50Tgas
```

## Security Considerations

-   **Access Control**: Owner-only methods require predecessor check
-   **Yocto Requirement**: Payable methods require 1 yoctoNEAR to prevent CSRF
-   **CEI Pattern**: Withdrawals follow Checks-Effects-Interactions pattern
-   **Redemption Priority**: Pending redemptions block new borrows
-   **Minimum Repayment**: Solvers must repay principal + 1% yield

## License

This project is licensed under the MIT License.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Run tests: `./test.sh`
4. Submit a pull request

## Support

For questions or issues, please open a GitHub issue.
