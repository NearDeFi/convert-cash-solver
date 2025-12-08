import {
    getNearDepositAddress,
    getEvmDepositAddress,
    withdrawToTron,
    withdrawToNear,
    checkBitfinexMoves,
} from '../cex/bitfinex.js';
import {
    getBinanceDepositAddress,
    checkBinanceDeposits,
    swapOnBinance,
    withdrawFromBinance,
    waitForBinanceDeposit,
} from '../cex/binance.js';
import {
    requestLiquidityUnsigned,
    requestLiquidityBroadcast,
    getNearAddress,
} from '../deprecated/near.js';

import { agentCall, agentView, agentAccountId } from '@neardefi/shade-agent-js';

import {
    getRecentDeposits,
    getDepositAddress,
    getIntentDiffDetails,
    createSignedErc191Intent,
} from '../deprecated/intents.js';

import {
    getEvmAddress,
    parseSignature,
    sendEVMTokens,
} from '../deprecated/evm.js';

// --- types -----------------------------------------------------------------

type IntentState =
    | 'StpLiquidityBorrowed'
    | 'StpLiquidityDeposited'
    | 'StpLiquidityWithdrawn'
    | 'StpIntentAccountCredited'
    | 'SwapCompleted'
    | 'UserLiquidityBorrowed'
    | 'UserLiquidityDeposited'
    | 'StpLiquidityReturned';

interface Intent {
    solver_id: string;
    created: number;
    state: IntentState;
    data: string;
    user_deposit_hash: string;
    nextState?: IntentState;
    destAmount?: string;
    srcToken?: string;
    srcAmount?: string;
    destToken?: string;
    amount?: string;
    swap_hash?: string;
    userTokenDiffPayload?: any;
    userTokenDiffSignature?: string;
    userWithdrawPayload?: any;
    userWithdrawSignature?: string;
    error?: string;
}

type StateFunction = (
    intent: Intent,
    solver_id: string,
) => Promise<boolean | void>;

// --- constants -------------------------------------------------------------

const INTENTS_CHAIN_ID_TRON = 'tron:mainnet';

// CEX selection: use Binance if USE_BINANCE=true, otherwise use Bitfinex (default)
const USE_BINANCE = process.env.USE_BINANCE === 'true';

// --- helper functions ------------------------------------------------------

const nanoToMs = (nanos: number): number => Math.floor(nanos / 1e6) - 1000;

function parseAmount(amount: string | undefined): number {
    return Math.max(5000000, Math.abs(parseInt(amount || '0', 10)));
}

/**
 * Helper function to check if deposit arrived at CEX (Binance or Bitfinex)
 */
async function checkCexDeposit(
    symbol: string,
    amount: number,
    start: number,
    receiver: string,
    network?: string,
): Promise<boolean> {
    if (USE_BINANCE) {
        console.log('Using Binance to check deposit...');
        return await checkBinanceDeposits({
            symbol,
            amount,
            start,
            receiver,
        });
    } else {
        console.log('Using Bitfinex to check deposit...');
        // For Bitfinex, we need to map network to method
        const method = network === 'tron:mainnet' ? 'tron' : network?.includes('evm') ? 'evm' : 'near';
        return await checkBitfinexMoves({
            amount,
            start,
            receiver,
            method,
        });
    }
}

/**
 * Helper function to get CEX deposit address (Binance or Bitfinex)
 */
async function getCexDepositAddress(symbol: string, network: string): Promise<string | null> {
    if (USE_BINANCE) {
        console.log('Getting Binance deposit address...');
        return await getBinanceDepositAddress(symbol, network);
    } else {
        console.log('Getting Bitfinex deposit address...');
        if (network.includes('near') || network === 'near-mainnet') {
            return await getNearDepositAddress();
        } else if (network.includes('evm') || network.includes('eth')) {
            return await getEvmDepositAddress();
        }
        return null;
    }
}

/**
 * Helper function to withdraw from CEX (Binance or Bitfinex)
 */
async function withdrawFromCex(
    symbol: string,
    amount: number,
    network: string,
    address: string,
): Promise<boolean> {
    if (USE_BINANCE) {
        console.log('Withdrawing from Binance...');
        const result = await withdrawFromBinance(symbol, amount, network, address);
        if (!result.success && result.error) {
            console.error('Binance withdrawal error:', result.error.message);
        }
        return result.success;
    } else {
        console.log('Withdrawing from Bitfinex...');
        if (network === 'tron:mainnet' || network.includes('tron')) {
            return await withdrawToTron(amount, address);
        } else if (network.includes('near')) {
            return await withdrawToNear(amount);
        }
        return false;
    }
}

export async function createIntent(
    data: string,
    user_deposit_hash: string,
): Promise<boolean> {
    // Get deposit address from selected CEX
    const solver_deposit_address = USE_BINANCE
        ? await getBinanceDepositAddress('USDT', 'near-mainnet')
        : await getNearDepositAddress();

    try {
        // update args

        const res = await agentCall({
            methodName: 'new_intent',
            args: {
                data,
                solver_deposit_address,
                user_deposit_hash,
            },
        });
        if ((res as any).error) {
            throw new Error((res as any).error);
        }
        console.log(`Intent created with hash ${user_deposit_hash}`);
        return true;
    } catch (e) {
        console.log('Error creating intent:', e);
        return false;
    }
}

// main state transition functions after intent is created

const stateFuncs: Record<IntentState, StateFunction> = {
    StpLiquidityBorrowed: async (intent: Intent, solver_id: string) => {
        try {
            // Get deposit address for the source network (usually NEAR)
            const depositAddress = await getCexDepositAddress('USDT', 'near-mainnet');
            if (!depositAddress) {
                console.log('Failed to get CEX deposit address');
                return false;
            }

            const res = await checkCexDeposit(
                'USDT',
                parseAmount(intent.destAmount),
                nanoToMs(intent.created),
                depositAddress,
                'near-mainnet',
            );
            console.log(`${USE_BINANCE ? 'Binance' : 'Bitfinex'} deposit check result:`, res);

            if (!res) {
                return false;
            }

            intent.nextState = 'StpLiquidityDeposited';
            return true;
        } catch (e) {
            console.log(`Error checking ${USE_BINANCE ? 'Binance' : 'Bitfinex'} deposit:`, e);
        }
        return false;
    },
    StpLiquidityDeposited: async (intent: Intent, solver_id: string) => {
        // Get the destination deposit address (based off the contract id)
        const { depositAddress } = await getDepositAddress(
            process.env.NEAR_CONTRACT_ID,
            INTENTS_CHAIN_ID_TRON,
        );

        console.log('TRON depositAddress', depositAddress);

        // Withdraw from CEX to destination address
        const res = await withdrawFromCex(
            'USDT',
            parseAmount(intent.destAmount),
            INTENTS_CHAIN_ID_TRON,
            depositAddress,
        );

        if (!res) {
            return false;
        }
        intent.nextState = 'StpLiquidityWithdrawn';
        return true;
    },
    StpLiquidityWithdrawn: async (intent: Intent, solver_id: string) => {
        // the deposit address is based off the contract id
        const { depositAddress } = await getDepositAddress(
            process.env.NEAR_CONTRACT_ID,
            INTENTS_CHAIN_ID_TRON,
        );

        console.log('TRON depositAddress', depositAddress);

        // Check if withdrawal completed (for Binance, we check deposits with negative amount)
        // For Bitfinex, negative amount indicates withdrawal
        const amount = USE_BINANCE
            ? parseAmount(intent.destAmount) // Binance: check positive deposit at destination
            : parseAmount(intent.destAmount) * -1; // Bitfinex: negative for withdrawals

        const res = await checkCexDeposit(
            'USDT',
            Math.abs(amount),
            nanoToMs(intent.created),
            depositAddress,
            INTENTS_CHAIN_ID_TRON,
        );

        if (!res) {
            return false;
        }
        intent.nextState = 'StpIntentAccountCredited';
        return true;
    },
    StpIntentAccountCredited: async (intent: Intent, solver_id: string) => {
        // TODO something here to execute intents

        intent.nextState = 'SwapCompleted';
        return true;
    },

    // Potentially check status of intent here?

    SwapCompleted: async (intent: Intent, solver_id: string) => {
        // TODO ftWithdraw intent for solver to bitfinex deposit address

        intent.nextState = 'UserLiquidityDeposited';
        return true;
    },
    UserLiquidityBorrowed: async (intent: Intent, solver_id: string) => {
        // Get deposit address for EVM network
        const depositAddress = await getCexDepositAddress('USDT', 'eth-mainnet');
        if (!depositAddress) {
            console.log('Failed to get CEX deposit address for EVM');
            return false;
        }

        const res = await checkCexDeposit(
            'USDT',
            parseInt(intent.amount!),
            nanoToMs(intent.created),
            depositAddress,
            'eth-mainnet',
        );
        if (!res) {
            return false;
        }
        intent.nextState = 'UserLiquidityDeposited';
        return true;
    },
    UserLiquidityDeposited: async (intent: Intent, solver_id: string) => {
        // Get NEAR deposit address for withdrawal
        const { depositAddress } = await getDepositAddress(
            process.env.NEAR_CONTRACT_ID,
            'near-mainnet',
        );

        const res = await withdrawFromCex(
            'USDT',
            parseInt(intent.amount!),
            'near-mainnet',
            depositAddress,
        );

        if (!res) {
            return false;
        }
        intent.nextState = 'StpLiquidityReturned';
        return true;
    },
    StpLiquidityReturned: async (intent: Intent, solver_id: string) => {
        // check this solvers bitfinex moves
        const res = await checkBitfinexMoves({
            amount: parseInt(intent.amount!) * -1, // negative for withdrawals
            start: nanoToMs(intent.created), // convert to ms from nanos
            receiver: (await getNearAddress()).address!,
            method: 'near',
        });

        if (!res) {
            return false;
        }
        // Final state - intent is complete
        return true;

        // check contract to see if liquidity was returned
    },

    //TODO one more state toremove intent from contract ???
};

// the cron helper functions

export async function updateState(
    solver_id: string,
    state: IntentState,
): Promise<boolean> {
    try {
        const res = await agentCall({
            methodName: 'update_intent_state',
            args: {
                solver_id,
                state,
            },
        });
        if ((res as any).error) {
            throw new Error((res as any).error);
        }
        console.log(`State updated to ${state} for solver_id ${solver_id}`);
        return true;
    } catch (e) {
        console.log(e);
        return false;
    }
}

async function getIntents(solver_id: string): Promise<Intent[]> {
    try {
        const intents = await agentView({
            methodName: 'get_intents_by_solver',
            args: {
                solver_id,
            },
        });
        if ((intents as any).error) {
            return [];
        }
        return intents as Intent[];
    } catch (e) {
        console.log(e);
        return [];
    }
}

const cronTimeout = (): void => {
    if (process.env.MANUAL_CRON) return;
    setTimeout(() => cron(), 10000); // 10s
};

let stateUpdateFailed: Intent[] = [];

// the main cron function

export async function cron(): Promise<void> {
    const solver_id = (await agentAccountId()).accountId;

    // get intents for solver
    let intents = await getIntents(solver_id);

    // Remove intents that are already in stateUpdateFailed (to avoid processing twice)
    const failedUpdateHashes = new Set(
        stateUpdateFailed.map((intent) => intent.user_deposit_hash),
    );
    intents = intents.filter(
        (intent) => !failedUpdateHashes.has(intent.user_deposit_hash),
    );

    // retry state updates
    const stillFailedUpdates: Intent[] = [];

    for (const intent of stateUpdateFailed) {
        const updateStateResult = await updateState(
            solver_id,
            intent.nextState!,
        );
        if (!updateStateResult) {
            console.log(
                `prevIntent: Failed to update state for intent ${intent.user_deposit_hash} to ${intent.nextState}`,
            );
            stillFailedUpdates.push(intent);
        } else {
            console.log(
                `Successfully updated failed intent ${intent.user_deposit_hash} to ${intent.nextState}`,
            );
        }
    }

    // Update stateUpdateFailed to only include those that still failed
    stateUpdateFailed = stillFailedUpdates;

    // if no current intent, claim one
    if (!intents.length) {
        console.log('No intents for solver_id', solver_id);
        return cronTimeout();
    }

    for (const intent of intents) {
        const stateFuncResult = await stateFuncs[intent.state](
            intent,
            solver_id,
        );

        if (!stateFuncResult || !intent.nextState) {
            console.log(
                `State function for ${intent.user_deposit_hash} failed. Will retry state function logic next cron cycle.`,
            );
        } else {
            const updateStateResult = await updateState(
                solver_id,
                intent.nextState,
            );
            if (!updateStateResult) {
                console.log(`Failed to update state to ${intent.nextState}`);
                stateUpdateFailed.push(intent);
            }
        }
    }

    return cronTimeout();
}
