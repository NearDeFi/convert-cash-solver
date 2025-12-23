# Binance CEX Integration

This document describes the Binance integration for the Convert Cash Solver, which handles deposit detection, token swaps, and withdrawals through the Binance exchange.

## Overview

The Binance integration (`binance.ts`) provides an alternative to the Bitfinex integration for CEX operations. It uses the [ccxt](https://github.com/ccxt/ccxt) library for standardized exchange communication.

## Features

### Part 1: OMFT Burn to Binance (Already Implemented)
- **Deposit Address Generation**: Get deposit addresses for various networks
- **Deposit Detection**: Monitor incoming deposits with confirmation tracking
- **Token Swaps**: Execute market orders with validation and error handling
- **Withdrawals**: Send funds to external addresses with fee calculation
- **Balance Queries**: Check available balances for any token

### Part 2: Binance to Intents (New)
- **Intents Deposit Address**: Get deposit address for solver's Intents account
- **Withdraw to Intents**: Withdraw funds from Binance to solver's Intents account
- **Intents Deposit Verification**: Check if deposits arrived in Intents account
- **ft_withdraw Intent**: Execute ft_withdraw intent from solver's Intents account to vault contract (repayment)

## Configuration

### Environment Variables

Add these variables to your `.env.development.local` file:

```bash
# CEX Selection: Set to 'true' to use Binance, 'false' for Bitfinex (default)
USE_BINANCE=true

# Binance API Configuration (mainnet only)
BINANCE_API_KEY_MAINNET=your_api_key_here
BINANCE_API_SECRET_MAINNET=your_api_secret_here

# Safety flags
BINANCE_DRY_RUN=false          # Set to 'true' to simulate operations without executing
BINANCE_ENABLE_WITHDRAWALS=true # Set to 'false' to disable withdrawals

# Intents Bridge Configuration (Part 2: Binance to Intents)
INTENTS_BRIDGE_ACCOUNT_ID=your_solver_near_account.near  # NEAR account ID registered in Intents
INTENTS_BRIDGE_JWT_TOKEN=your_jwt_token_here             # Optional JWT token for bridge service authentication

# Intents Intent Configuration (Part 2: ft_withdraw to Vault)
# SOLVER_EVM_PRIVATE_KEY is no longer needed - solvers use NEP-413 (NEAR native signing) instead of ERC-191
NEAR_PRIVATE_KEY=ed25519:...                            # NEAR private key for signing and executing intents (required)
VAULT_CONTRACT_ID=your_vault_contract.near              # Vault contract account ID (or use NEAR_CONTRACT_ID)
NEAR_NETWORK_ID=mainnet                                 # Optional: 'mainnet' or 'testnet' (default: 'mainnet')
```

### API Key Requirements

Your Binance API key needs the following permissions:
- **Enable Reading**: For balance checks and deposit monitoring
- **Enable Spot & Margin Trading**: For executing swaps
- **Enable Withdrawals**: For sending funds (if `BINANCE_ENABLE_WITHDRAWALS=true`)

⚠️ **Security Recommendations**:
- Use IP whitelisting in Binance API settings
- Never commit API keys to version control
- Use separate API keys for development and production

## Supported Networks

| Chain ID | Binance Network Code |
|----------|---------------------|
| `eth-mainnet` | ETH |
| `tron-mainnet` | TRX |
| `bsc-mainnet` | BSC |
| `polygon-mainnet` | MATIC |
| `avalanche-mainnet` | AVAX |
| `near-mainnet` | NEAR |

## API Reference

### Functions

#### `getBinanceDepositAddress(symbol, network)`

Get a deposit address for a specific token on a network.

```typescript
const address = await getBinanceDepositAddress('USDT', 'near-mainnet');
// Returns: "cfbdf6e7462659d18926d14b942c6e320a75f59b811a7f7e52c154750e059f84" or null
```

#### `checkBinanceDeposits(params)`

Check if a deposit has been received and confirmed.

```typescript
const received = await checkBinanceDeposits({
    symbol: 'USDT',
    amount: 100,
    start: Date.now() - 3600000, // 1 hour ago
    receiver: '0x...',
});
// Returns: true | false
```

#### `waitForBinanceDeposit(symbol, txId, maxWaitTimeMs?, checkIntervalMs?)`

Wait for a deposit to be fully confirmed and available for withdrawal.

```typescript
const result = await waitForBinanceDeposit('USDT', 'tx_hash_here');
// Returns: { available: boolean, deposit?: Transaction, status?: string }
```

#### `swapOnBinance(fromSymbol, toSymbol, amount)`

Execute a market swap between two tokens.

```typescript
const result = await swapOnBinance('USDT', 'BTC', 1000);
// Returns: SwapResult { success: boolean, order?: Order, error?: BinanceError }
```

**Validations performed:**
- Trading pair existence and availability
- Sufficient balance
- Minimum notional value
- Current market price

#### `withdrawFromBinance(symbol, amount, network, address, memo?)`

Withdraw funds to an external address.

```typescript
const result = await withdrawFromBinance(
    'USDT',
    100,
    'near-mainnet',
    'receiver.near'
);
// Returns: WithdrawalResult { success: boolean, txId?: string, error?: BinanceError }
```

**Validations performed:**
- Network availability for the token
- Withdrawal enabled on network
- Minimum withdrawal amount
- Sufficient balance (including fees)

#### `getBinanceBalance(symbol)`

Get the available (free) balance for a token.

```typescript
const balance = await getBinanceBalance('USDT');
// Returns: number (e.g., 1234.56)
```

### Part 2: Binance to Intents Functions

#### `getIntentsDepositAddress(symbol, network)`

Get the deposit address for the solver's Intents account. This address is used to withdraw funds from Binance to Intents.

```typescript
const depositInfo = await getIntentsDepositAddress('USDT', 'eth-mainnet');
// Returns: { address: string, chain: string } | null
```

**Requirements:**
- `INTENTS_BRIDGE_ACCOUNT_ID` must be set in environment variables
- The account must be registered in NEAR Intents

#### `withdrawToIntents(symbol, amount, network)`

Withdraw funds from Binance to the solver's Intents account. This implements Part 2 of the CEX flow.

**Note:** This is a convenience wrapper around `withdrawFromBinance`. It automatically:
1. Gets the Intents deposit address for the solver account
2. Calls `withdrawFromBinance` with that address
3. Returns the same result as `withdrawFromBinance`

```typescript
const result = await withdrawToIntents('USDT', 100, 'eth-mainnet');
// Returns: WithdrawalResult { success: boolean, txId?: string, error?: BinanceError }

// Equivalent to:
const depositInfo = await getIntentsDepositAddress('USDT', 'eth-mainnet');
const result = await withdrawFromBinance('USDT', 100, 'eth-mainnet', depositInfo.address);
```

**Flow:**
1. Gets the Intents deposit address for the solver account via `getIntentsDepositAddress`
2. Calls `withdrawFromBinance` internally with the Intents deposit address
3. Funds are sent from Binance to the solver's Intents account

**Requirements:**
- `INTENTS_BRIDGE_ACCOUNT_ID` must be set in environment variables
- The account must be registered in NEAR Intents
- All validations from `withdrawFromBinance` apply (network availability, minimum amounts, balance checks, etc.)

#### `checkIntentsDeposit(symbol, amount, network, startTime)`

Check if a deposit has arrived in the solver's Intents account.

```typescript
const received = await checkIntentsDeposit('USDT', 100, 'eth-mainnet', Date.now() - 3600000);
// Returns: boolean
```

**Note:** This checks recent deposits from the Intents Bridge Service. The service returns recent deposits, so timing is approximate.

#### `executeFtWithdrawIntent(tokenId, amount, receiverId?)`

Executes an `ft_withdraw` intent from the solver's Intents account to the vault contract. This completes the repayment flow.

```typescript
const result = await executeFtWithdrawIntent(
    'eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near',
    '10000000', // Amount in minimal units
);
// Returns: { success: boolean, txHash?: string, error?: BinanceError }
```

**Requirements:**
- `INTENTS_BRIDGE_ACCOUNT_ID` must be set
- `NEAR_PRIVATE_KEY` must be set (for NEP-413 signing and executing the intent)
- `VAULT_CONTRACT_ID` (or `NEAR_CONTRACT_ID`) must be set
- `NEAR_ACCOUNT_ID` (optional, defaults to `INTENTS_BRIDGE_ACCOUNT_ID`)

**What it does:**
1. Verifies vault contract is registered in OMFT (registers if needed)
2. Creates ft_withdraw intent quote
3. Signs quote with NEP-413 (NEAR native signing) using NEAR private key
4. Executes intent via `execute_intents` method on `intents.near` contract
5. Tokens are transferred from solver's Intents account to vault contract

**Note:** Solvers with NEAR accounts use NEP-413 signing, not ERC-191. No EVM private key needed!

#### `getOmftTokenId(symbol, network)`

Gets the OMFT token ID for a given symbol and network. This is a helper function to convert token symbols to OMFT token IDs.

```typescript
const tokenId = await getOmftTokenId('USDT', 'eth-mainnet');
// Returns: "eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near" | null
```

## Error Handling

The integration includes comprehensive error handling with custom error types:

### Error Types

| Error Class | Code | Description |
|-------------|------|-------------|
| `BinanceError` | Various | Base error class |
| `InsufficientBalanceError` | `INSUFFICIENT_BALANCE` | Not enough funds |
| `SwapError` | `SWAP_ERROR` | Failed to execute swap |
| `WithdrawalError` | `WITHDRAWAL_ERROR` | Failed to withdraw |

### Error Codes

- `INSUFFICIENT_FUNDS` - Not enough balance for operation
- `INVALID_ORDER` - Order parameters invalid (min notional, etc.)
- `AUTH_ERROR` - API key/signature issues
- `NETWORK_ERROR` - Network/connectivity issues
- `MISSING_CREDENTIALS` - API keys not configured
- `UNKNOWN_ERROR` - Unexpected error

### Example Error Handling

```typescript
import { 
    swapOnBinance, 
    InsufficientBalanceError, 
    SwapError 
} from './cex/binance.js';

const result = await swapOnBinance('USDT', 'BTC', 1000);

if (!result.success) {
    if (result.error instanceof InsufficientBalanceError) {
        console.log(`Need ${result.error.required}, have ${result.error.available}`);
    } else if (result.error instanceof SwapError) {
        console.log(`Swap failed: ${result.error.message}`);
    }
}
```

## Retry Logic

Transient errors (network issues, rate limits) are automatically retried with exponential backoff:

- **Max retries**: 3
- **Initial delay**: 1000ms
- **Backoff**: Exponential (1s, 2s, 4s)

Retryable errors:
- Network errors
- Request timeouts
- Rate limit exceeded
- Exchange temporarily unavailable

## Dry Run Mode

When `BINANCE_DRY_RUN=true`, operations are logged but not executed:

```
[DRY-RUN] Would swap 100 USDT → BTC on Binance
[DRY-RUN] Would withdraw 100 USDT to receiver.near on NEAR from Binance
```

This is useful for:
- Development and testing
- Validating integration without real transactions
- Debugging flow issues

## Integration with Cron

The `cron.ts` module automatically switches between Binance and Bitfinex based on the `USE_BINANCE` environment variable:

```typescript
// In cron.ts
const USE_BINANCE = process.env.USE_BINANCE === 'true';

// Functions automatically route to correct CEX:
await checkCexDeposit('USDT', amount, start, receiver, network);
await withdrawFromCex('USDT', amount, network, address);
```

### Part 2 Flow in Cron

When `USE_BINANCE=true`, the flow includes Part 2 (Binance to Intents):

1. **SwapCompleted State**: After swap is completed, withdraws from Binance to Intents using `withdrawToIntents()`
2. **StpIntentAccountCredited State**: Checks if deposit arrived in Intents account using `checkIntentsDeposit()`

**State Flow (Binance):**
```
StpLiquidityBorrowed → StpLiquidityDeposited → (swapOnBinance) → (withdrawToIntents) → 
StpLiquidityWithdrawn → (checkIntentsDeposit) → 
StpIntentAccountCredited → (executeFtWithdrawIntent) → 
SwapCompleted → UserLiquidityDeposited
```

**Detailed Flow:**
1. **StpLiquidityBorrowed**: Check if OMFT was burned and deposited to Binance (Part 1 - already existed)
2. **StpLiquidityDeposited**: 
   - **Part 1b**: Execute swap on Binance (from source token to destination token) (`swapOnBinance`)
   - **Part 2a**: Withdraw from Binance to solver's Intents account (`withdrawToIntents`)
3. **StpLiquidityWithdrawn**: Verify deposit arrived in Intents account (`checkIntentsDeposit`)
4. **StpIntentAccountCredited**: Execute `ft_withdraw` intent to repay vault (`executeFtWithdrawIntent`)
   - Verifies/registers vault storage in OMFT contract
   - Creates intent quote
   - Signs with NEP-413 (NEAR native signing)
   - Executes via `execute_intents` method on `intents.near`
5. **SwapCompleted**: Swap and repayment completed
6. **UserLiquidityDeposited**: Continue with user flow

**Complete Flow Explanation:**
- **Part 1**: OMFT burn from NEAR contract → Binance deposit address (already working)
- **Part 1b**: Swap on Binance (e.g., USDT on ETH → USDT on TRON) - **NEW**
- **Part 2a**: Withdraw from Binance → Solver's Intents account (new asset after swap)
- **Part 2b**: Verify deposit in Intents account
- **Part 2c**: Execute `ft_withdraw` intent from solver's Intents account → Vault contract (repayment)

## Troubleshooting

### Common Issues

**"Missing Binance API credentials"**
- Ensure `BINANCE_API_KEY_MAINNET` and `BINANCE_API_SECRET_MAINNET` are set

**"Trading pair not found"**
- Check if the pair exists on Binance (e.g., some pairs like USDT/EUR might not exist)
- Try the reverse pair order

**"Minimum notional not met"**
- Increase the trade amount
- Binance requires minimum order values (usually ~10 USDT equivalent)

**"Network not available for withdrawal"**
- Check if withdrawals are enabled for that network on Binance
- Some networks have maintenance periods

**"Insufficient balance"**
- Ensure you have enough balance including withdrawal fees
- Check if funds are locked in open orders

### Debug Logging

All operations log detailed information:

```
[Binance] Exchange initialized successfully
[Binance] Balance for USDT: 1234.56 (free)
[Binance] Swap executed successfully: id=123, pair=USDT/BTC, side=sell, amount=100
[Binance] Initiating withdrawal: 100 USDT + 1 fee = 101 total to receiver.near on NEAR
[Binance] Withdrawal submitted successfully: id=456, txid=abc123
```

## Testing

To test the integration:

1. Set `BINANCE_DRY_RUN=true` for safe testing
2. Use small amounts on testnet/mainnet
3. Monitor the console output for detailed logs

```bash
# Run with dry run enabled
BINANCE_DRY_RUN=true yarn bus:dev
```

## Security Considerations

1. **API Key Security**: Store keys in environment variables, never in code
2. **IP Whitelisting**: Configure allowed IPs in Binance API settings
3. **Withdrawal Addresses**: Consider using address whitelisting on Binance
4. **Rate Limits**: The integration respects rate limits automatically via ccxt
5. **Dry Run**: Use `BINANCE_DRY_RUN=true` during development