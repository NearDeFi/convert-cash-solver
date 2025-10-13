import {
    getNearDepositAddress,
    getEvmDepositAddress,
    withdrawToTron,
    withdrawToNear,
    checkBitfinexMoves,
} from '../cex/bitfinex.js';
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
    | 'IntentsExecuted'
    | 'SwapCompleted'
    | 'UserLiquidityDeposited'
    | 'UserLiquidityWithdrawn'
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

// --- helper functions ------------------------------------------------------

const nanoToMs = (nanos: number): number => Math.floor(nanos / 1e6) - 1000;

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

function parseAmount(amount: string | undefined): number {
    return Math.max(5000000, Math.abs(parseInt(amount || '0', 10)));
}

// main state transition functions after intent is claimed

const stateFuncs: Record<IntentState, StateFunction> = {
    StpLiquidityBorrowed: async (intent: Intent, solver_id: string) => {
        try {
            const to = await getNearDepositAddress();
            const { payload, transaction } = await requestLiquidityUnsigned({
                to: to!,
                amount: parseAmount(intent.destAmount).toString(),
            });

            const liqRes = await (agentCall as any)({
                methodName: 'request_signature',
                args: { path: 'pool-1', payload, key_type: 'Eddsa' },
            });

            const broadcastRes = await requestLiquidityBroadcast({
                transaction,
                signature: (liqRes as any).signature,
            });

            if (!(broadcastRes as any)?.txHash) {
                console.log(
                    'Error broadcasting liquidity request:',
                    broadcastRes,
                );
                return false;
            }

            // every state transition needs to add "nextState" to the intent and return true
            intent.nextState = 'StpLiquidityDeposited';
            return true;
        } catch (e) {
            console.log('Error requesting liquidity:', e);
        }
        return false;
    },
    StpLiquidityDeposited: async (intent: Intent, solver_id: string) => {
        try {
            const res = await checkBitfinexMoves({
                amount: parseAmount(intent.destAmount),
                start: nanoToMs(intent.created), // convert to ms from nanos
                receiver: (await getNearDepositAddress())!,
                method: 'near',
            });
            console.log('Bitfinex moves check result:', res);

            if (!res) {
                return false;
            }

            intent.nextState = 'StpLiquidityWithdrawn';
            return true;
        } catch (e) {
            console.log('Error checking Bitfinex moves:', e);
        }
        return false;
    },
    StpLiquidityWithdrawn: async (intent: Intent, solver_id: string) => {
        // get an EVM address to use as implicit-eth address for the solver agent
        const { address } = await getEvmAddress();
        // get a deposit address for that EVM address on Tron
        const { depositAddress } = await getDepositAddress(
            address,
            INTENTS_CHAIN_ID_TRON,
        );

        console.log('TRON depositAddress', depositAddress);

        const res = await withdrawToTron(
            parseAmount(intent.destAmount),
            depositAddress,
        );

        // if (!res) {
        //     return false;
        // }
        // intent.nextState = 'StpIntentAccountCredited';
        // return true;
    },
    StpIntentAccountCredited: async (intent: Intent, solver_id: string) => {
        // get an EVM address to use as implicit-eth address for the solver agent
        const { address } = await getEvmAddress();
        // get a deposit address for that EVM address on Tron
        const { depositAddress } = await getDepositAddress(
            address,
            INTENTS_CHAIN_ID_TRON,
        );
        // TODO finish this check to see if withdrawal requested is completed, e.g. put in arguments of withdrawal
        const res = await checkBitfinexMoves({
            amount: parseAmount(intent.destAmount) * -1, // negative for withdrawals
            start: nanoToMs(intent.created), // convert to ms from nanos
            receiver: depositAddress,
            method: 'tron',
        });

        if (!res) {
            return false;
        }
        intent.nextState = 'IntentsExecuted';

        return true;
    },
    IntentsExecuted: async (intent: Intent, solver_id: string) => {
        // get an EVM address to use as implicit-eth address for the solver agent
        const { address } = await getEvmAddress();
        // get a deposit address for that EVM address on Tron
        const { depositAddress } = await getDepositAddress(
            address,
            INTENTS_CHAIN_ID_TRON,
        );

        const { srcToken, srcAmount, destToken, destAmount } = intent;

        const tokenDiffIntent = await createSignedErc191Intent(address, [
            {
                intent: 'token_diff',
                diff: {
                    // !!! reverse parity TODO clean this up and make more secure
                    [srcToken!]: srcAmount!.substring(1),
                    [destToken!]: (parseInt(destAmount!, 10) * -1).toString(),
                },
            },
        ]);

        console.log('tokenDiffIntent:', tokenDiffIntent);

        const ethWithdrawTokenId =
            'eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near';
        const ethWithdrawAddress = `0x525521d79134822a342d330bd91DA67976569aF1`;
        const withdrawIntent = await createSignedErc191Intent(address, [
            {
                intent: 'ft_withdraw',
                token: ethWithdrawTokenId,
                receiver_id: ethWithdrawTokenId,
                amount: srcAmount!,
                memo: `WITHDRAW_TO:${ethWithdrawAddress}`,
            },
        ]);

        console.log('withdrawIntent:', withdrawIntent);

        const standard = 'erc191';

        const args = {
            signed: [
                tokenDiffIntent,
                {
                    standard,
                    payload: JSON.stringify(intent.userTokenDiffPayload),
                    signature: intent.userTokenDiffSignature,
                },
            ],
        };

        console.log('calling with args', args);

        const res = await agentCall({
            contractId: 'intents.near',
            methodName: 'execute_intents',
            args,
            gas: '300000000000000',
            deposit: '0',
        });

        console.log(res);

        const args2 = {
            signed: [
                withdrawIntent,
                {
                    standard,
                    payload: JSON.stringify(intent.userWithdrawPayload),
                    signature: intent.userWithdrawSignature,
                },
            ],
        };

        console.log('second call with args', args2);

        const res2 = await agentCall({
            contractId: 'intents.near',
            methodName: 'execute_intents',
            args: args2,
            gas: '300000000000000',
            deposit: '0',
        });

        console.log(res2);

        // TODO combine with user intent and verify intent

        return;
    },
    SwapCompleted: async (intent: Intent, solver_id: string) => {
        const receiver = await getEvmDepositAddress();

        // tokenAddress defaults to USDT on ETH mainnet
        const res = await sendEVMTokens({
            receiver: receiver!,
            amount: BigInt(intent.amount!),
            chainId: 1,
        });

        if (!(res as any).success) {
            console.log('Error sending EVMTokens:', res);
            return false;
        }

        intent.nextState = 'UserLiquidityDeposited';
        return true;
    },
    UserLiquidityDeposited: async (intent: Intent, solver_id: string) => {
        // TODO finish this check to see if withdrawal requested is completed, e.g. put in arguments of withdrawal
        const res = await checkBitfinexMoves({
            method: 'evm',
            amount: parseInt(intent.amount!) * -1,
            start: nanoToMs(intent.created),
            receiver: (await getEvmDepositAddress())!,
        });
        if (!res) {
            return false;
        }
        intent.nextState = 'UserLiquidityWithdrawn';
        return true;
    },
    UserLiquidityWithdrawn: async (intent: Intent, solver_id: string) => {
        const res = await withdrawToNear(intent.amount!);

        if (!res) {
            return false;
        }
        intent.nextState = 'StpLiquidityReturned';
        return true;
    },
    // TODO is this check needed?

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
};

// the cron runner

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
