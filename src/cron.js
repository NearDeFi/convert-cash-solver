import {
    getNearDepositAddress,
    withdrawToTron,
    checkBitfinexMoves,
} from './bitfinex.js';
import { requestLiquidityUnsigned, requestLiquidityBroadcast } from './near.js';

const PORT = 3000;

import {
    contractCall,
    contractView,
    getAgentAccount,
} from '@neardefi/shade-agent-js';

import {
    getTronAddress,
    tronUSDTUnsigned,
    tronBroadcastTx,
    constructTronSignature,
} from './tron.js';

import { sendTokens } from './evm.js';

import { callWithAgent } from './app.js';

async function getIntent(solver_id) {
    try {
        const intent = await contractView({
            methodName: 'get_intent_by_solver',
            args: {
                solver_id,
            },
        });
        return intent;
    } catch (e) {
        return null;
    }
}

async function claimIntent() {
    try {
        await callWithAgent({
            methodName: 'claim_intent',
            args: {
                index: 0,
            },
        });
        return true;
    } catch (e) {
        return false;
    }
}

async function updateIntentState(solver_id, state) {
    try {
        await contractCall({
            methodName: 'update_intent_state',
            args: {
                solver_id,
                state,
            },
        });
        return true;
    } catch (e) {
        return false;
    }
}

// main state transition functions after intent is claimed

const stateFuncs = {
    Claimed: async (intent, solver_id) => {
        try {
            const { payload, transaction } = await requestLiquidityUnsigned({
                to,
                amount: intent.amount,
            });

            const liqRes = await callWithAgent({
                methodName: 'get_signature',
                args: { path: 'pool-1', payload, key_type: 'Eddsa' },
            });

            const broadcastRes = await requestLiquidityBroadcast({
                transaction,
                signature: liqRes.signature,
            });

            console.log('requestLiquidityBroadcast', broadcastRes);

            intent.nextState = 'LiquidityProvided';
            return true;
        } catch (e) {
            return false;
        }
    },
    LiquidityProvided: (intent, solver_id) => {},
    SwapComplete: (intent, solver_id) => {},
    UserLiquidityProvided: (intent, solver_id) => {},
    LiquidityReturned: (intent, solver_id) => {},
};

const cronTimeout = () => setTimeout(cron, 10000); // 10s

export async function cron(prevIntent) {
    const solver_id = (await getAgentAccount()).workerAccountId;

    // check
    if (prevIntent) {
        // ready for next state
        const updateStateResult = await updateState(
            solver_id,
            prevIntent.nextState,
        );
        if (!updateStateResult) {
            console.log(
                `prevIntent: Failed to update state to ${intent.nextState}`,
            );
            return cronTimeout(prevIntent);
        }
    }

    // get the current intent
    const intent = await getIntent(solver_id);

    // if no current intent, claim one
    if (!intent) {
        const claimed = await claimIntent();
        if (!claimed) {
            return cronTimeout();
        }
    }

    // we are on the current intent and state, so use state functions to do next move
    const stateFuncResult = await stateFuncs[intent.state](intent, solver_id);

    if (!stateFuncResult || !intent.nextState) {
        console.log(`State function for ${intent.state} failed`);
        return cronTimeout();
    }

    // ready for next state
    const updateStateResult = await updateState(solver_id, intent.nextState);
    if (!updateStateResult) {
        console.log(`Failed to update state to ${intent.nextState}`);
        return cronTimeout(intent);
    }
}
