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

// Singleton exchange instance
let exchangeInstance: Exchange | null = null;

async function getExchange(): Promise<Exchange> {
    if (!exchangeInstance) {
        exchangeInstance = new ccxt.binance({
            apiKey: BINANCE_API_KEY,
            secret: BINANCE_API_SECRET,
            enableRateLimit: true,
            options: {
                defaultType: 'spot',
            },
        });

        exchangeInstance.checkRequiredCredentials(true);
        await exchangeInstance.loadTimeDifference();
        await exchangeInstance.loadMarkets();
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
 */
export async function swapOnBinance(
    fromSymbol: string,
    toSymbol: string,
    amount: number,
): Promise<Order | null> {
    if (BINANCE_DRY_RUN) {
        console.log(
            `[DRY-RUN] Would swap ${amount} ${fromSymbol} → ${toSymbol} on Binance`,
        );
        return null;
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
            throw new Error(
                `Trading pair ${fromSymbol}/${toSymbol} not found on Binance. Tried: ${pair1} and ${pair2}`,
            );
        }

        // Check minimum notional (value in quote currency)
        const minNotional = market.limits?.cost?.min;
        if (minNotional !== undefined) {
            const ticker = await exchange.fetchTicker(tradingPair);
            const currentPrice = ticker.last ?? ticker.ask ?? ticker.bid ?? 0;

            // Calculate notional: if selling base, notional = amount * price
            // If buying base, notional = amount (we're spending quote currency)
            const notional = side === 'sell' ? amount * currentPrice : amount;

            if (notional < minNotional) {
                const requiredAmount = side === 'sell'
                    ? minNotional / currentPrice
                    : minNotional;
                throw new Error(
                    `Minimum notional not met. Required: ${minNotional} ${market.quote}, ` +
                        `but your order value is ${notional.toFixed(8)} ${market.quote}. ` +
                        `Minimum amount needed: ${requiredAmount.toFixed(8)} ${fromSymbol}`,
                );
            }
        }

        // Create market order
        const order = await exchange.createOrder(tradingPair, 'market', side, amount);
        console.log(`Binance swap executed: ${order.id}, ${order.symbol}, ${order.side}, ${order.amount}`);
        return order;
    } catch (error) {
        console.error('Error executing Binance swap:', error);
        throw error;
    }
}

/**
 * Withdraw funds from Binance to an external address
 */
export async function withdrawFromBinance(
    symbol: string,
    amount: number,
    network: string,
    address: string,
    memo?: string,
): Promise<boolean> {
    if (BINANCE_DRY_RUN || !BINANCE_ENABLE_WITHDRAWALS) {
        console.log(
            `[DRY-RUN] Would withdraw ${amount} ${symbol} to ${address} on ${network} from Binance`,
        );
        return false;
    }

    try {
        const exchange = await getExchange();
        const binanceNetwork = mapNetworkToBinance(network);

        // Get network info to calculate fee and precision
        const currencies = await exchange.fetchCurrencies();
        const token = currencies[symbol];
        const networkInfo = token?.networks?.[binanceNetwork];

        if (!networkInfo) {
            throw new Error(`Network ${binanceNetwork} is not available for ${symbol}`);
        }

        const decimals = networkInfo.precision ?? 8;
        const fee = networkInfo.fee ?? 0;
        const amountWithFee = roundToDecimals(amount + fee, decimals, 'nearest');

        const tx = await exchange.withdraw(symbol, amountWithFee, address, memo, {
            network: binanceNetwork,
            fee,
        });

        console.log(`Binance withdrawal submitted: ${tx.id}, ${tx.txid}`);
        return true;
    } catch (error) {
        console.error('Error withdrawing from Binance:', error);
        return false;
    }
}

/**
 * Get Binance balance for a specific symbol
 */
export async function getBinanceBalance(symbol: string): Promise<number> {
    try {
        const exchange = await getExchange();
        const balances = await exchange.fetchBalance({ type: 'spot' });
        const free = (balances.free ?? {}) as unknown as Record<string, number>;
        return free[symbol] ?? 0;
    } catch (error) {
        console.error(`Error fetching Binance balance for ${symbol}:`, error);
        return 0;
    }
}

