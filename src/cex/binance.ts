/**
 * binance.ts
 *
 * Binance CEX integration for deposit, swap, and withdrawal operations
 * Similar to bitfinex.ts but using Binance API via ccxt library
 */

import ccxt from 'ccxt';
import type {
    Exchange,
    Transaction,
    Order,
    Market,
} from 'ccxt';
import dotenv from 'dotenv';
const dir = process.cwd();
dotenv.config({ path: `${dir}/.env.development.local` });

// --- types -----------------------------------------------------------------

interface CheckBinanceDepositParams {
    symbol: string;
    amount: number;
    start: number; // timestamp in milliseconds
    receiver: string; // deposit address
}

interface BinanceDepositAddress {
    currency: string;
    address: string;
    network: string;
    tag?: string;
}

// Custom error types for better error handling
export class BinanceError extends Error {
    constructor(
        message: string,
        public readonly code?: string,
        public readonly originalError?: unknown,
    ) {
        super(message);
        this.name = 'BinanceError';
    }
}

export class InsufficientBalanceError extends BinanceError {
    constructor(
        public readonly symbol: string,
        public readonly required: number,
        public readonly available: number,
    ) {
        super(
            `Insufficient ${symbol} balance. Required: ${required}, Available: ${available}`,
            'INSUFFICIENT_BALANCE',
        );
        this.name = 'InsufficientBalanceError';
    }
}

export class SwapError extends BinanceError {
    constructor(
        message: string,
        public readonly fromSymbol: string,
        public readonly toSymbol: string,
        public readonly amount: number,
        originalError?: unknown,
    ) {
        super(message, 'SWAP_ERROR', originalError);
        this.name = 'SwapError';
    }
}

export class WithdrawalError extends BinanceError {
    constructor(
        message: string,
        public readonly symbol: string,
        public readonly amount: number,
        public readonly network: string,
        originalError?: unknown,
    ) {
        super(message, 'WITHDRAWAL_ERROR', originalError);
        this.name = 'WithdrawalError';
    }
}

export interface SwapResult {
    success: boolean;
    order?: Order;
    error?: BinanceError;
}

export interface WithdrawalResult {
    success: boolean;
    txId?: string;
    error?: BinanceError;
}

// --- constants -------------------------------------------------------------

const BINANCE_API_KEY = process.env.BINANCE_API_KEY_MAINNET!;
const BINANCE_API_SECRET = process.env.BINANCE_API_SECRET_MAINNET!;
const BINANCE_DRY_RUN = process.env.BINANCE_DRY_RUN === 'true';
const BINANCE_ENABLE_WITHDRAWALS = process.env.BINANCE_ENABLE_WITHDRAWALS === 'true';

// Network mapping from Intents chain IDs to Binance network codes
const NETWORK_MAP: Record<string, string> = {
    'eth-mainnet': 'ETH',
    'tron-mainnet': 'TRX',
    'bsc-mainnet': 'BSC',
    'polygon-mainnet': 'MATIC',
    'avalanche-mainnet': 'AVAX',
    'near-mainnet': 'NEAR',
};

// --- helpers ---------------------------------------------------------------

function roundToDecimals(
    value: number,
    decimals: number,
    mode: 'up' | 'down' | 'nearest' = 'nearest',
): number {
    const factor = 10 ** decimals;
    switch (mode) {
        case 'up':
            return Math.ceil(value * factor) / factor;
        case 'down':
            return Math.floor(value * factor) / factor;
        default:
            return Math.round(value * factor) / factor;
    }
}

function mapNetworkToBinance(chainId: string): string {
    return NETWORK_MAP[chainId] || chainId.toUpperCase();
}

/**
 * Retry helper for transient errors (network issues, rate limits)
 */
async function withRetry<T>(
    operation: () => Promise<T>,
    maxRetries: number = 3,
    delayMs: number = 1000,
    operationName: string = 'operation',
): Promise<T> {
    let lastError: unknown;

    for (let attempt = 1; attempt <= maxRetries; attempt++) {
        try {
            return await operation();
        } catch (error) {
            lastError = error;

            // Check if error is retryable (network errors, rate limits)
            const isRetryable =
                error instanceof ccxt.NetworkError ||
                error instanceof ccxt.RequestTimeout ||
                error instanceof ccxt.RateLimitExceeded ||
                error instanceof ccxt.ExchangeNotAvailable;

            if (!isRetryable || attempt === maxRetries) {
                throw error;
            }

            const waitTime = delayMs * Math.pow(2, attempt - 1); // Exponential backoff
            console.warn(
                `[Binance] ${operationName} failed (attempt ${attempt}/${maxRetries}), ` +
                    `retrying in ${waitTime}ms...`,
                error instanceof Error ? error.message : error,
            );

            await new Promise((resolve) => setTimeout(resolve, waitTime));
        }
    }

    throw lastError;
}

/**
 * Classify ccxt errors into our custom error types
 */
function classifyError(
    error: unknown,
    context: { operation: string; symbol?: string; amount?: number; network?: string },
): BinanceError {
    if (error instanceof BinanceError) {
        return error;
    }

    const message = error instanceof Error ? error.message : String(error);

    // Insufficient balance errors
    if (
        error instanceof ccxt.InsufficientFunds ||
        message.toLowerCase().includes('insufficient') ||
        message.toLowerCase().includes('not enough')
    ) {
        return new BinanceError(
            `Insufficient funds for ${context.operation}: ${message}`,
            'INSUFFICIENT_FUNDS',
            error,
        );
    }

    // Invalid order errors
    if (
        error instanceof ccxt.InvalidOrder ||
        message.toLowerCase().includes('invalid order') ||
        message.toLowerCase().includes('min notional')
    ) {
        return new BinanceError(
            `Invalid order for ${context.operation}: ${message}`,
            'INVALID_ORDER',
            error,
        );
    }

    // Authentication errors
    if (
        error instanceof ccxt.AuthenticationError ||
        message.toLowerCase().includes('api-key') ||
        message.toLowerCase().includes('signature')
    ) {
        return new BinanceError(
            `Authentication failed for ${context.operation}: ${message}`,
            'AUTH_ERROR',
            error,
        );
    }

    // Network/availability errors
    if (
        error instanceof ccxt.NetworkError ||
        error instanceof ccxt.ExchangeNotAvailable
    ) {
        return new BinanceError(
            `Network error during ${context.operation}: ${message}`,
            'NETWORK_ERROR',
            error,
        );
    }

    // Generic exchange error
    return new BinanceError(
        `${context.operation} failed: ${message}`,
        'UNKNOWN_ERROR',
        error,
    );
}

// Singleton exchange instance
let exchangeInstance: Exchange | null = null;

async function getExchange(): Promise<Exchange> {
    if (!exchangeInstance) {
        // Validate credentials before creating instance
        if (!BINANCE_API_KEY || !BINANCE_API_SECRET) {
            throw new BinanceError(
                'Missing Binance API credentials. Please set BINANCE_API_KEY_MAINNET and BINANCE_API_SECRET_MAINNET environment variables.',
                'MISSING_CREDENTIALS',
            );
        }

        try {
            exchangeInstance = new ccxt.binance({
                apiKey: BINANCE_API_KEY,
                secret: BINANCE_API_SECRET,
                enableRateLimit: true,
                options: {
                    defaultType: 'spot',
                },
            });

            exchangeInstance.checkRequiredCredentials(true);

            // Use retry for network operations during initialization
            await withRetry(
                () => exchangeInstance!.loadTimeDifference(),
                3,
                1000,
                'loadTimeDifference',
            );

            await withRetry(
                () => exchangeInstance!.loadMarkets(),
                3,
                1000,
                'loadMarkets',
            );

            console.log('[Binance] Exchange initialized successfully');
        } catch (error) {
            // Reset instance on failure so next call can retry
            exchangeInstance = null;
            throw classifyError(error, { operation: 'exchange initialization' });
        }
    }
    return exchangeInstance;
}

// --- public functions -------------------------------------------------------

/**
 * Get Binance deposit address for a specific asset and network
 */
export async function getBinanceDepositAddress(
    symbol: string,
    network: string,
): Promise<string | null> {
    try {
        const exchange = await getExchange();
        const binanceNetwork = mapNetworkToBinance(network);

        const payload = await (exchange as any).sapiGetCapitalDepositAddressList({
            coin: exchange.currency(symbol).id,
            network: exchange.networkCodeToId(binanceNetwork) ?? binanceNetwork,
        });

        if (Array.isArray(payload) && payload.length > 0) {
            const address = payload[0].address;
            return address || null;
        }

        return null;
    } catch (error) {
        console.error(`Error getting Binance deposit address for ${symbol} on ${network}:`, error);
        return null;
    }
}

/**
 * Check if a deposit has been received on Binance
 * Similar to checkBitfinexMoves but for Binance
 */
export async function checkBinanceDeposits({
    symbol,
    amount,
    start,
    receiver,
}: CheckBinanceDepositParams): Promise<boolean> {
    try {
        const exchange = await getExchange();
        const deposits = await exchange.fetchDeposits(symbol, start);

        const found = deposits.find(
            (deposit) =>
                deposit.address === receiver &&
                (deposit.amount ?? 0) >= amount &&
                deposit.status === 'ok',
        );

        return !!found;
    } catch (error) {
        console.error('Error checking Binance deposits:', error);
        return false;
    }
}

/**
 * Get deposit status and check if it's available for withdrawal
 */
export function getBinanceDepositStatus(deposit: Transaction): {
    finished: boolean;
    availableForWithdrawal: boolean;
    statusString: string;
    confirmations?: string;
} {
    const info = deposit.info as any;

    if (info.confirmTimes && info.unlockConfirm) {
        const [current, total] = info.confirmTimes.split('/').map(Number);
        const required = Number(info.unlockConfirm);

        const statusString = `${deposit.status}, confirmations: ${current}/${required}, internal status: ${info.status}`;

        // Deposit is finished when:
        // 1. Current confirmations >= required confirmations
        // 2. Internal status is '1' (completed)
        const finished = current >= required && info.status === '1';

        return {
            finished,
            availableForWithdrawal: finished,
            statusString,
            confirmations: `${current}/${required}`,
        };
    }

    // Default: check if status is 'ok'
    const statusStr = String(deposit.status || 'unknown');
    return {
        finished: statusStr === 'ok',
        availableForWithdrawal: statusStr === 'ok',
        statusString: statusStr,
    };
}

/**
 * Wait for a deposit to be available for withdrawal
 */
export async function waitForBinanceDeposit(
    symbol: string,
    txId: string,
    maxWaitTimeMs = 30 * 60 * 1000, // 30 minutes
    checkIntervalMs = 10 * 1000, // 10 seconds
): Promise<{ available: boolean; deposit?: Transaction; status?: string }> {
    const startTime = Date.now();
    const exchange = await getExchange();

    while (Date.now() - startTime < maxWaitTimeMs) {
        try {
            const deposits = await exchange.fetchDeposits(symbol, startTime);
            const deposit = deposits.find((d) => d.txid === txId || d.id === txId);

            if (deposit) {
                const status = getBinanceDepositStatus(deposit);

                if (status.availableForWithdrawal) {
                    return {
                        available: true,
                        deposit,
                        status: status.statusString,
                    };
                }

                console.log(
                    `Waiting for Binance deposit ${txId} to be available... ${status.confirmations || status.statusString}`,
                );
            }
        } catch (error) {
            console.error('Error checking deposit status:', error);
        }

        // Wait before next check
        await new Promise((resolve) => setTimeout(resolve, checkIntervalMs));
    }

    return {
        available: false,
        status: 'Timeout waiting for deposit to be available',
    };
}

/**
 * Execute a swap on Binance (market order)
 *
 * @param fromSymbol - The token to swap from (e.g., 'USDT')
 * @param toSymbol - The token to swap to (e.g., 'BTC')
 * @param amount - The amount to swap
 * @returns SwapResult with success status and order details or error
 */
export async function swapOnBinance(
    fromSymbol: string,
    toSymbol: string,
    amount: number,
): Promise<SwapResult> {
    const context = { operation: 'swap', symbol: fromSymbol, amount };

    if (BINANCE_DRY_RUN) {
        console.log(
            `[DRY-RUN] Would swap ${amount} ${fromSymbol} → ${toSymbol} on Binance`,
        );
        return { success: true };
    }

    try {
        const exchange = await getExchange();
        const markets = exchange.markets;

        // Try both possible pair orders
        const pair1 = `${fromSymbol}/${toSymbol}`;
        const pair2 = `${toSymbol}/${fromSymbol}`;

        let tradingPair: string | undefined;
        let side: 'buy' | 'sell' | undefined;
        let market: Market | undefined;

        if (pair1 in markets && markets[pair1].active) {
            tradingPair = pair1;
            side = 'sell'; // Selling fromSymbol to get toSymbol
            market = markets[pair1];
        } else if (pair2 in markets && markets[pair2].active) {
            tradingPair = pair2;
            side = 'buy'; // Buying toSymbol using fromSymbol
            market = markets[pair2];
        }

        if (!tradingPair || !side || !market) {
            const error = new SwapError(
                `Trading pair ${fromSymbol}/${toSymbol} not found on Binance. Tried: ${pair1} and ${pair2}`,
                fromSymbol,
                toSymbol,
                amount,
            );
            console.error('[Binance] Swap error:', error.message);
            return { success: false, error };
        }

        // Verify balance before attempting swap
        const balance = await getBinanceBalance(fromSymbol);
        if (balance < amount) {
            const error = new InsufficientBalanceError(fromSymbol, amount, balance);
            console.error('[Binance] Swap error:', error.message);
            return { success: false, error };
        }

        // Check minimum notional (value in quote currency)
        const minNotional = market.limits?.cost?.min;
        if (minNotional !== undefined) {
            const ticker = await withRetry(
                () => exchange.fetchTicker(tradingPair!),
                3,
                1000,
                'fetchTicker',
            );
            const currentPrice = ticker.last ?? ticker.ask ?? ticker.bid ?? 0;

            if (currentPrice === 0) {
                const error = new SwapError(
                    `Unable to fetch current price for ${tradingPair}`,
                    fromSymbol,
                    toSymbol,
                    amount,
                );
                console.error('[Binance] Swap error:', error.message);
                return { success: false, error };
            }

            // Calculate notional: if selling base, notional = amount * price
            // If buying base, notional = amount (we're spending quote currency)
            const notional = side === 'sell' ? amount * currentPrice : amount;

            if (notional < minNotional) {
                const requiredAmount = side === 'sell'
                    ? minNotional / currentPrice
                    : minNotional;
                const error = new SwapError(
                    `Minimum notional not met. Required: ${minNotional} ${market.quote}, ` +
                        `but your order value is ${notional.toFixed(8)} ${market.quote}. ` +
                        `Minimum amount needed: ${requiredAmount.toFixed(8)} ${fromSymbol}`,
                    fromSymbol,
                    toSymbol,
                    amount,
                );
                console.error('[Binance] Swap error:', error.message);
                return { success: false, error };
            }
        }

        // Create market order with retry for transient errors
        const order = await withRetry(
            () => exchange.createOrder(tradingPair!, 'market', side!, amount),
            3,
            1000,
            'createOrder',
        );

        console.log(
            `[Binance] Swap executed successfully: ` +
                `id=${order.id}, pair=${order.symbol}, side=${order.side}, ` +
                `amount=${order.amount}, filled=${order.filled}, status=${order.status}`,
        );

        return { success: true, order };
    } catch (error) {
        const swapError =
            error instanceof BinanceError
                ? error
                : new SwapError(
                      error instanceof Error ? error.message : String(error),
                      fromSymbol,
                      toSymbol,
                      amount,
                      error,
                  );

        console.error('[Binance] Swap failed:', {
            fromSymbol,
            toSymbol,
            amount,
            error: swapError.message,
            code: swapError.code,
        });

        return { success: false, error: swapError };
    }
}

/**
 * Withdraw funds from Binance to an external address
 *
 * @param symbol - The token symbol to withdraw (e.g., 'USDT')
 * @param amount - The amount to withdraw (excluding fees)
 * @param network - The network to withdraw to (e.g., 'near-mainnet', 'eth-mainnet')
 * @param address - The destination address
 * @param memo - Optional memo/tag for networks that require it
 * @returns WithdrawalResult with success status and transaction details or error
 */
export async function withdrawFromBinance(
    symbol: string,
    amount: number,
    network: string,
    address: string,
    memo?: string,
): Promise<WithdrawalResult> {
    const binanceNetwork = mapNetworkToBinance(network);

    if (BINANCE_DRY_RUN || !BINANCE_ENABLE_WITHDRAWALS) {
        console.log(
            `[DRY-RUN] Would withdraw ${amount} ${symbol} to ${address} on ${binanceNetwork} from Binance`,
        );
        return { success: true };
    }

    try {
        const exchange = await getExchange();

        // Verify balance before attempting withdrawal
        const balance = await getBinanceBalance(symbol);

        // Get network info to calculate fee and precision
        const currencies = await withRetry(
            () => exchange.fetchCurrencies(),
            3,
            1000,
            'fetchCurrencies',
        );
        const token = currencies[symbol];
        const networkInfo = token?.networks?.[binanceNetwork];

        if (!networkInfo) {
            const error = new WithdrawalError(
                `Network ${binanceNetwork} is not available for ${symbol}. ` +
                    `Available networks: ${Object.keys(token?.networks || {}).join(', ') || 'none'}`,
                symbol,
                amount,
                network,
            );
            console.error('[Binance] Withdrawal error:', error.message);
            return { success: false, error };
        }

        // Check if withdrawals are enabled for this network
        if (networkInfo.withdraw === false) {
            const error = new WithdrawalError(
                `Withdrawals are currently disabled for ${symbol} on ${binanceNetwork}`,
                symbol,
                amount,
                network,
            );
            console.error('[Binance] Withdrawal error:', error.message);
            return { success: false, error };
        }

        const decimals = networkInfo.precision ?? 8;
        const fee = networkInfo.fee ?? 0;
        const minWithdraw = networkInfo.limits?.withdraw?.min ?? 0;
        const totalRequired = amount + fee;

        // Check minimum withdrawal amount
        if (amount < minWithdraw) {
            const error = new WithdrawalError(
                `Amount ${amount} ${symbol} is below minimum withdrawal of ${minWithdraw} ${symbol}`,
                symbol,
                amount,
                network,
            );
            console.error('[Binance] Withdrawal error:', error.message);
            return { success: false, error };
        }

        // Verify sufficient balance (amount + fee)
        if (balance < totalRequired) {
            const error = new InsufficientBalanceError(symbol, totalRequired, balance);
            console.error('[Binance] Withdrawal error:', error.message);
            return { success: false, error };
        }

        const amountWithFee = roundToDecimals(totalRequired, decimals, 'nearest');

        console.log(
            `[Binance] Initiating withdrawal: ${amount} ${symbol} + ${fee} fee = ${amountWithFee} total ` +
                `to ${address} on ${binanceNetwork}`,
        );

        const tx = await withRetry(
            () =>
                exchange.withdraw(symbol, amountWithFee, address, memo, {
                    network: binanceNetwork,
                    fee,
                }),
            3,
            2000,
            'withdraw',
        );

        console.log(
            `[Binance] Withdrawal submitted successfully: id=${tx.id}, txid=${tx.txid || 'pending'}`,
        );

        return {
            success: true,
            txId: tx.txid || tx.id,
        };
    } catch (error) {
        const withdrawalError =
            error instanceof BinanceError
                ? error
                : new WithdrawalError(
                      error instanceof Error ? error.message : String(error),
                      symbol,
                      amount,
                      network,
                      error,
                  );

        console.error('[Binance] Withdrawal failed:', {
            symbol,
            amount,
            network: binanceNetwork,
            address,
            error: withdrawalError.message,
            code: withdrawalError.code,
        });

        return { success: false, error: withdrawalError };
    }
}

/**
 * Get Binance balance for a specific symbol
 *
 * @param symbol - The token symbol (e.g., 'USDT', 'BTC')
 * @returns The available (free) balance, or 0 if an error occurs
 */
export async function getBinanceBalance(symbol: string): Promise<number> {
    try {
        const exchange = await getExchange();
        const balances = await withRetry(
            () => exchange.fetchBalance({ type: 'spot' }),
            3,
            1000,
            'fetchBalance',
        );
        const free = (balances.free ?? {}) as unknown as Record<string, number>;
        const balance = free[symbol] ?? 0;

        console.log(`[Binance] Balance for ${symbol}: ${balance} (free)`);
        return balance;
    } catch (error) {
        const classifiedError = classifyError(error, {
            operation: 'fetchBalance',
            symbol,
        });
        console.error(`[Binance] Error fetching balance for ${symbol}:`, classifiedError.message);
        return 0;
    }
}

/**
 * Reset the exchange instance (useful for testing or re-authentication)
 */
export function resetExchangeInstance(): void {
    exchangeInstance = null;
    console.log('[Binance] Exchange instance reset');
}

// --- Intents Bridge Integration (Part 2: Binance to Intents) ---

import { IntentsBridgeService } from './intentsBridge.js';
import {
    IntentsIntentService,
    type IntentsIntentConfig,
} from './intentsIntent.js';

const INTENTS_BRIDGE_ACCOUNT_ID = process.env.INTENTS_BRIDGE_ACCOUNT_ID;
const INTENTS_BRIDGE_JWT_TOKEN = process.env.INTENTS_BRIDGE_JWT_TOKEN;

/**
 * Gets the Intents deposit address for the solver account
 * This is used to withdraw funds from Binance to the solver's Intents account
 *
 * @param symbol Token symbol (e.g., 'USDT', 'USDC')
 * @param network Network identifier (e.g., 'eth-mainnet', 'tron-mainnet')
 * @returns Deposit address and chain info, or null if configuration is missing
 */
export async function getIntentsDepositAddress(
    symbol: string,
    network: string,
): Promise<{ address: string; chain: string } | null> {
    if (!INTENTS_BRIDGE_ACCOUNT_ID) {
        console.warn(
            '[Binance] INTENTS_BRIDGE_ACCOUNT_ID not configured. Cannot get Intents deposit address.',
        );
        return null;
    }

    try {
        const bridgeService = new IntentsBridgeService({
            accountId: INTENTS_BRIDGE_ACCOUNT_ID,
            jwtToken: INTENTS_BRIDGE_JWT_TOKEN,
        });

        // Map network to Intents chain format
        const binanceNetwork = mapNetworkToBinance(network);
        let intentsChain: string;

        try {
            intentsChain = IntentsBridgeService.mapBinanceNetworkToIntentsChain(
                binanceNetwork,
            );
        } catch (error) {
            console.error(
                `[Binance] Failed to map network ${network} (${binanceNetwork}) to Intents chain:`,
                error instanceof Error ? error.message : error,
            );
            return null;
        }

        // Get supported tokens to find the correct token identifier
        const supportedTokens = await bridgeService.getSupportedTokens([intentsChain]);

        // Find the defuse asset identifier for the symbol
        const defuseAssetId = IntentsBridgeService.findDefuseAssetIdentifier(
            symbol,
            intentsChain,
            supportedTokens,
        );

        if (!defuseAssetId) {
            console.error(
                `[Binance] Token ${symbol} not found on chain ${intentsChain} in supported tokens.`,
            );
            return null;
        }

        // Extract token address from defuse asset identifier (format: chain:chainId:address)
        const parts = defuseAssetId.split(':');
        const tokenAddress = parts.length >= 3 ? parts[2] : undefined;

        // Get deposit address
        const depositAddress = await bridgeService.getDepositAddress(
            intentsChain,
            tokenAddress,
        );

        console.log(
            `[Binance] Intents deposit address for ${symbol} on ${intentsChain}: ${depositAddress.address}`,
        );

        return {
            address: depositAddress.address,
            chain: depositAddress.chain,
        };
    } catch (error) {
        const errorMessage =
            error instanceof Error ? error.message : String(error);
        console.error(
            `[Binance] Error getting Intents deposit address: ${errorMessage}`,
        );
        return null;
    }
}

/**
 * Withdraws funds from Binance to the solver's Intents account
 * This implements Part 2 of the CEX flow: Binance -> Intents
 *
 * @param symbol Token symbol (e.g., 'USDT', 'USDC')
 * @param amount Amount to withdraw (in token units, not minimal units)
 * @param network Network identifier (e.g., 'eth-mainnet', 'tron-mainnet')
 * @returns Withdrawal result with success status and transaction ID
 */
export async function withdrawToIntents(
    symbol: string,
    amount: number,
    network: string,
): Promise<WithdrawalResult> {
    if (!INTENTS_BRIDGE_ACCOUNT_ID) {
        const error = new WithdrawalError(
            'INTENTS_BRIDGE_ACCOUNT_ID not configured. Cannot withdraw to Intents.',
            symbol,
            amount,
            network,
        );
        console.error('[Binance] Withdrawal error:', error.message);
        return { success: false, error };
    }

    // Get Intents deposit address
    const depositInfo = await getIntentsDepositAddress(symbol, network);
    if (!depositInfo) {
        const error = new WithdrawalError(
            `Failed to get Intents deposit address for ${symbol} on ${network}`,
            symbol,
            amount,
            network,
        );
        console.error('[Binance] Withdrawal error:', error.message);
        return { success: false, error };
    }

    // Use the standard withdrawFromBinance function with the Intents deposit address
    console.log(
        `[Binance] Withdrawing ${amount} ${symbol} to Intents account ${INTENTS_BRIDGE_ACCOUNT_ID} on ${network}`,
    );

    return await withdrawFromBinance(
        symbol,
        amount,
        network,
        depositInfo.address,
    );
}

/**
 * Checks if a deposit has arrived in the solver's Intents account
 *
 * @param symbol Token symbol (e.g., 'USDT', 'USDC')
 * @param amount Expected amount (in token units)
 * @param network Network identifier (e.g., 'eth-mainnet', 'tron-mainnet')
 * @param startTime Start time in milliseconds to search from
 * @returns true if deposit found and completed, false otherwise
 */
export async function checkIntentsDeposit(
    symbol: string,
    amount: number,
    network: string,
    startTime: number,
): Promise<boolean> {
    if (!INTENTS_BRIDGE_ACCOUNT_ID) {
        console.warn(
            '[Binance] INTENTS_BRIDGE_ACCOUNT_ID not configured. Cannot check Intents deposits.',
        );
        return false;
    }

    try {
        const bridgeService = new IntentsBridgeService({
            accountId: INTENTS_BRIDGE_ACCOUNT_ID,
            jwtToken: INTENTS_BRIDGE_JWT_TOKEN,
        });

        // Map network to Intents chain format
        const binanceNetwork = mapNetworkToBinance(network);
        const intentsChain = IntentsBridgeService.mapBinanceNetworkToIntentsChain(
            binanceNetwork,
        );

        // Get recent deposits
        const deposits = await bridgeService.getRecentDeposits(intentsChain);

        // Filter deposits by symbol, amount, and time
        const matchingDeposits = deposits.filter((deposit) => {
            // Check if deposit is completed
            if (deposit.status !== 'COMPLETED') {
                return false;
            }

            // Check if deposit is after start time
            // Note: deposits don't have timestamp, so we check all recent deposits
            // The service returns recent deposits, so we assume they're recent enough

            // Check amount (with some tolerance for rounding)
            const depositAmount = parseFloat(deposit.amount) / Math.pow(10, deposit.decimals);
            const amountTolerance = amount * 0.01; // 1% tolerance
            const amountMatch =
                Math.abs(depositAmount - amount) <= amountTolerance;

            // Check symbol by matching asset name or defuse identifier
            const symbolMatch =
                deposit.defuse_asset_identifier.toLowerCase().includes(
                    symbol.toLowerCase(),
                ) ||
                deposit.defuse_asset_identifier
                    .split(':')
                    .pop()
                    ?.toLowerCase()
                    .includes(symbol.toLowerCase());

            return amountMatch && symbolMatch;
        });

        if (matchingDeposits.length > 0) {
            console.log(
                `[Binance] Found ${matchingDeposits.length} matching deposit(s) in Intents for ${symbol} on ${network}`,
            );
            return true;
        }

        return false;
    } catch (error) {
        const errorMessage =
            error instanceof Error ? error.message : String(error);
        console.error(
            `[Binance] Error checking Intents deposits: ${errorMessage}`,
        );
        return false;
    }
}

// --- Intents Intent Integration (Part 2: ft_withdraw from Intents to Vault) ---

const VAULT_CONTRACT_ID = process.env.NEAR_CONTRACT_ID || process.env.VAULT_CONTRACT_ID;
const SOLVER_EVM_PRIVATE_KEY = process.env.SOLVER_EVM_PRIVATE_KEY;

/**
 * Executes ft_withdraw intent from solver's Intents account to vault contract
 * This completes Part 2 of the CEX flow: Intents -> Vault (repayment)
 *
 * @param tokenId OMFT token ID (e.g., "eth-0x...omft.near")
 * @param amount Amount in minimal units (string)
 * @param receiverId Optional receiver (defaults to vault contract)
 * @returns Transaction hash of the intent execution
 */
export async function executeFtWithdrawIntent(
    tokenId: string,
    amount: string,
    receiverId?: string,
): Promise<{ success: boolean; txHash?: string; error?: BinanceError }> {
    if (!INTENTS_BRIDGE_ACCOUNT_ID) {
        const error = new BinanceError(
            'INTENTS_BRIDGE_ACCOUNT_ID not configured. Cannot execute ft_withdraw intent.',
            'MISSING_CREDENTIALS',
        );
        console.error('[Binance] Intent execution error:', error.message);
        return { success: false, error };
    }

    if (!SOLVER_EVM_PRIVATE_KEY) {
        const error = new BinanceError(
            'SOLVER_EVM_PRIVATE_KEY not configured. Cannot sign ft_withdraw intent.',
            'MISSING_CREDENTIALS',
        );
        console.error('[Binance] Intent execution error:', error.message);
        return { success: false, error };
    }

    if (!VAULT_CONTRACT_ID) {
        const error = new BinanceError(
            'VAULT_CONTRACT_ID (or NEAR_CONTRACT_ID) not configured. Cannot execute ft_withdraw intent.',
            'MISSING_CREDENTIALS',
        );
        console.error('[Binance] Intent execution error:', error.message);
        return { success: false, error };
    }

    try {
        const config: IntentsIntentConfig = {
            solverNearAccountId: INTENTS_BRIDGE_ACCOUNT_ID,
            solverEvmPrivateKey: SOLVER_EVM_PRIVATE_KEY,
            vaultContractId: VAULT_CONTRACT_ID,
            nearAccountId: process.env.NEAR_ACCOUNT_ID || INTENTS_BRIDGE_ACCOUNT_ID,
            nearPrivateKey: process.env.NEAR_PRIVATE_KEY,
            nearNetworkId: process.env.NEAR_NETWORK_ID || 'mainnet',
        };

        const intentService = new IntentsIntentService(config);

        console.log(
            `[Binance] Executing ft_withdraw intent: ${amount} from ${INTENTS_BRIDGE_ACCOUNT_ID} to ${receiverId || VAULT_CONTRACT_ID}`,
        );

        const txHash = await intentService.createAndExecuteFtWithdraw(
            tokenId,
            amount,
            receiverId,
        );

        console.log(
            `[Binance] ft_withdraw intent executed successfully. Transaction: ${txHash}`,
        );

        return { success: true, txHash };
    } catch (error: any) {
        const intentError = new BinanceError(
            `Failed to execute ft_withdraw intent: ${error instanceof Error ? error.message : String(error)}`,
            'INTENT_EXECUTION_ERROR',
            error,
        );
        console.error('[Binance] Intent execution error:', intentError.message);
        return { success: false, error: intentError };
    }
}

/**
 * Gets the OMFT token ID from Intents Bridge Service
 * Helper function to convert symbol and network to OMFT token ID
 *
 * @param symbol Token symbol (e.g., 'USDT', 'USDC')
 * @param network Network identifier (e.g., 'eth-mainnet', 'tron-mainnet')
 * @returns OMFT token ID or null if not found
 */
export async function getOmftTokenId(
    symbol: string,
    network: string,
): Promise<string | null> {
    if (!INTENTS_BRIDGE_ACCOUNT_ID) {
        console.warn(
            '[Binance] INTENTS_BRIDGE_ACCOUNT_ID not configured. Cannot get OMFT token ID.',
        );
        return null;
    }

    try {
        const bridgeService = new IntentsBridgeService({
            accountId: INTENTS_BRIDGE_ACCOUNT_ID,
            jwtToken: INTENTS_BRIDGE_JWT_TOKEN,
        });

        // Map network to Intents chain format
        const binanceNetwork = mapNetworkToBinance(network);
        const intentsChain = IntentsBridgeService.mapBinanceNetworkToIntentsChain(
            binanceNetwork,
        );

        // Get supported tokens
        const supportedTokens = await bridgeService.getSupportedTokens([intentsChain]);

        // Find the defuse asset identifier
        const defuseAssetId = IntentsBridgeService.findDefuseAssetIdentifier(
            symbol,
            intentsChain,
            supportedTokens,
        );

        if (!defuseAssetId) {
            console.error(
                `[Binance] Token ${symbol} not found on chain ${intentsChain}`,
            );
            return null;
        }

        // Find the token info to get the OMFT token ID
        const tokenInfo = supportedTokens.find(
            (t) => t.defuse_asset_identifier === defuseAssetId,
        );

        if (!tokenInfo) {
            console.error(
                `[Binance] Token info not found for ${defuseAssetId}`,
            );
            return null;
        }

        return tokenInfo.near_token_id;
    } catch (error) {
        const errorMessage =
            error instanceof Error ? error.message : String(error);
        console.error(
            `[Binance] Error getting OMFT token ID: ${errorMessage}`,
        );
        return null;
    }
}

