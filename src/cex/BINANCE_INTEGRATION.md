# Binance CEX Integration

This document describes the Binance integration for the Convert Cash Solver, which handles deposit detection, token swaps, and withdrawals through the Binance exchange.

## Overview

The Binance integration (`binance.ts`) provides an alternative to the Bitfinex integration for CEX operations. It uses the [ccxt](https://github.com/ccxt/ccxt) library for standardized exchange communication.

## Features

- **Deposit Address Generation**: Get deposit addresses for various networks
- **Deposit Detection**: Monitor incoming deposits with confirmation tracking
- **Token Swaps**: Execute market orders with validation and error handling
- **Withdrawals**: Send funds to external addresses with fee calculation
- **Balance Queries**: Check available balances for any token

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