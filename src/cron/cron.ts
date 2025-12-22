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
    withdrawToIntents,
    checkIntentsDeposit,
    executeFtWithdrawIntent,
    getOmftTokenId,
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
        if (USE_BINANCE) {
            // Part 1b: Execute swap on Binance (from source token to destination token)
            console.log('Executing swap on Binance...');
            
            // Extract token symbols from OMFT token IDs
            // Format: "eth-0x...omft.near" or "tron-...omft.near"
            // For now, we assume USDT for both (can be enhanced to extract from token ID)
            const fromSymbol = 'USDT'; // Token that arrived in Binance (from burn)
            const toSymbol = 'USDT'; // Token needed for destination (same for now, but could be different)
            
            // Get the amount that arrived (use srcAmount if available, otherwise destAmount)
            const swapAmount = intent.srcAmount 
                ? parseAmount(intent.srcAmount) 
                : parseAmount(intent.destAmount);

            // Execute swap (if tokens are different, otherwise skip)
            if (fromSymbol !== toSymbol) {
                const swapResult = await swapOnBinance(fromSymbol, toSymbol, swapAmount);
                
                if (!swapResult.success) {
                    console.error(
                        'Failed to swap on Binance:',
                        swapResult.error?.message,
                    );
                    return false;
                }

                console.log(
                    `Swap completed successfully on Binance: ${fromSymbol} → ${toSymbol}`,
                );
                // Wait a bit for swap to settle
                await new Promise(resolve => setTimeout(resolve, 2000));
            } else {
                console.log(
                    `No swap needed: source and destination tokens are the same (${fromSymbol})`,
                );
            }

            // Part 2a: Withdraw from Binance to solver's Intents account
            console.log('Withdrawing from Binance to Intents...');
            
            // Determine the destination network from the intent
            const destNetwork = intent.destToken?.includes('tron')
                ? 'tron-mainnet'
                : intent.destToken?.includes('eth')
                  ? 'eth-mainnet'
                  : 'eth-mainnet'; // Default to ETH if unclear

            const result = await withdrawToIntents(
                toSymbol, // Use the token after swap
                parseAmount(intent.destAmount),
                destNetwork,
            );

            if (!result.success) {
                console.error(
                    'Failed to withdraw from Binance to Intents:',
                    result.error?.message,
                );
                return false;
            }

            console.log(
                `Successfully withdrew to Intents. Transaction: ${result.txId || 'pending'}`,
            );
            intent.nextState = 'StpLiquidityWithdrawn';
            return true;
        } else {
            // For Bitfinex: Get the destination deposit address (based off the contract id)
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
        }
    },
    StpLiquidityWithdrawn: async (intent: Intent, solver_id: string) => {
        if (USE_BINANCE) {
            // Part 2b: Check if deposit arrived in Intents account
            console.log('Checking if deposit arrived in Intents account...');
            
            const destNetwork = intent.destToken?.includes('tron')
                ? 'tron-mainnet'
                : intent.destToken?.includes('eth')
                  ? 'eth-mainnet'
                  : 'eth-mainnet';

            const depositReceived = await checkIntentsDeposit(
                'USDT',
                parseAmount(intent.destAmount),
                destNetwork,
                nanoToMs(intent.created),
            );

            if (!depositReceived) {
                console.log('Deposit not yet received in Intents account');
                return false;
            }

            console.log('Deposit confirmed in Intents account');
            intent.nextState = 'StpIntentAccountCredited';
            return true;
        } else {
            // For Bitfinex: the deposit address is based off the contract id
            const { depositAddress } = await getDepositAddress(
                process.env.NEAR_CONTRACT_ID,
                INTENTS_CHAIN_ID_TRON,
            );

            console.log('TRON depositAddress', depositAddress);

            // Check if withdrawal completed (for Bitfinex, negative amount indicates withdrawal)
            const amount = parseAmount(intent.destAmount) * -1;

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
        }
    },
    StpIntentAccountCredited: async (intent: Intent, solver_id: string) => {
        // Part 2c: Execute ft_withdraw intent to repay the vault (for Binance flow)
        if (USE_BINANCE) {
            console.log('Executing ft_withdraw intent to repay vault...');
            
            // Determine the destination network from the intent
            const destNetwork = intent.destToken?.includes('tron')
                ? 'tron-mainnet'
                : intent.destToken?.includes('eth')
                  ? 'eth-mainnet'
                  : 'eth-mainnet';

            // Get OMFT token ID for the destination network
            const tokenId = await getOmftTokenId('USDT', destNetwork);
            if (!tokenId) {
                console.error('Failed to get OMFT token ID');
                return false;
            }

            // Convert amount to minimal units (assuming 6 decimals for USDT)
            const amountInMinimalUnits = (
                parseAmount(intent.destAmount) * 1_000_000
            ).toString();

            const result = await executeFtWithdrawIntent(
                tokenId,
                amountInMinimalUnits,
            );

            if (!result.success) {
                console.error(
                    'Failed to execute ft_withdraw intent:',
                    result.error?.message,
                );
                return false;
            }

            console.log(
                `ft_withdraw intent executed successfully. Transaction: ${result.txHash}`,
            );
            intent.nextState = 'SwapCompleted';
            return true;
        } else {
            // For Bitfinex, keep the old flow
            intent.nextState = 'SwapCompleted';
            return true;
        }
    },

    // Potentially check status of intent here?

    SwapCompleted: async (intent: Intent, solver_id: string) => {
        // Swap is completed, ft_withdraw intent has been executed
        // Move to next state
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
