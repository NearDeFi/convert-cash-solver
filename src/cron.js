import {
    getNearDepositAddress,
    getEvmDepositAddress,
    withdrawToTron,
    withdrawToNear,
    checkBitfinexMoves,
} from './bitfinex.js';
import { requestLiquidityUnsigned, requestLiquidityBroadcast } from './near.js';

import {
    contractCall,
    contractView,
    getAgentAccount,
} from '@neardefi/shade-agent-js';

import {
    tronUSDTUnsigned,
    tronBroadcastTx,
    constructTronSignature,
    checkTronTx,
    getTronAddress,
} from './tron.js';

import { getEvmAddress, sendEVMTokens } from './evm.js';

import { callWithAgent } from './app.js';

const nanoToMs = (nanos) => nanos / 1e6 - 1000;

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

async function updateState(solver_id, state) {
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
        console.log('Requesting liquidity for intent', intent.amount);
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

            // TODO check broadcastRes for status SuccessValue etc... what should it say?

            // every state transition needs to add "nextState" to the intent and return true
            intent.nextState = 'LiquidityProvided';
            return true;
        } catch (e) {
            console.log('Error requesting liquidity:', e);
        }
        return false;
    },
    LiquidityProvided: async (intent, solver_id) => {
        console.log(
            'Checking Bitfinex moves to see if liquidity was provided for intent',
            intent.amount,
        );
        try {
            const res = await checkBitfinexMoves({
                amount: intent.amount,
                start: nanoToMs(intent.created), // convert to ms from nanos
                receiver: await getNearDepositAddress(),
                method: 'near',
            });

            if (!res) {
                return false;
            }

            intent.nextState = 'LiquidityCredited';
            return true;
        } catch (e) {
            console.log('Error checking Bitfinex moves:', e);
        }
        return false;
    },
    LiquidityCredited: async (intent, solver_id) => {
        console.log('Withdrawing liquidity to Tron for intent', intent.amount);

        const res = await withdrawToTron(intent.amount);

        if (!res) {
            return false;
        }
        intent.nextState = 'WithdrawRequested';
        return true;
    },
    WithdrawRequested: async (intent, solver_id) => {
        // TODO finish this check to see if withdrawal requested is completed, e.g. put in arguments of withdrawal
        const res = await checkBitfinexMoves({
            amount: parseInt(intent.amount) * -1, // negative for withdrawals
            start: nanoToMs(intent.created), // convert to ms from nanos
            receiver: await getTronAddress(),
            method: 'tron',
        });

        if (!res) {
            return false;
        }
        intent.nextState = 'CompleteSwap';
        return true;

        // TODO update intent state in contract with txHash of tron withdrawal
    },
    CompleteSwap: async (intent, solver_id) => {
        // TODO replace this with derived address?
        const { address } = await getTronAddress();

        try {
            const { txHash, rawTransaction } = await tronUSDTUnsigned({
                to: intent.dest_receiver_address,
                from: address,
                amount: intent.amount,
            });

            console.log('tron tx: getting chain sig');
            const sigRes = await callWithAgent({
                methodName: 'get_signature',
                args: { path: 'tron-1', payload: txHash, key_type: 'Ecdsa' },
            });
            console.log(sigRes);
            const signatureHex = constructTronSignature(sigRes);
            console.log('sig:', signatureHex);
            // await verifySignature(txHash, signatureHex);
            console.log('tron tx: broadcasting');
            const res = await tronBroadcastTx(rawTransaction, signatureHex);

            // TODO check if tron tx was succcessful or not right away

            console.log('Tron broadcast', res);

            await contractCall({
                methodName: 'update_swap_hash',
                args: {
                    solver_id,
                    swap_hash,
                    txHash,
                },
            });
            return true;
        } catch (e) {
            console.log('Error completing swap:', e);
        }
        return false;
    },
    CheckSwapComplete: async (intent, solver_id) => {
        // make sure tron tx is complete, how many confirmations?

        const txHash = intent.swap_hash;

        try {
            return await checkTronTx(txHash);
        } catch (e) {
            console.log('Error checking Tron tx:', e);
            return false;
        }
    },
    SwapComplete: async (intent, solver_id) => {
        // TODO move user liquidity to Bitfinex

        const receiver = await getEvmDepositAddress();

        // tokenAddress defaults to USDT on ETH mainnet
        const res = await sendEVMTokens({
            receiver,
            amount: intent.amount,
            chainId: 1,
        });

        if (!res.success) {
            console.log('Error sending EVMTokens:', res);
            return false;
        }

        intent.nextState = 'UserLiquidityProvided';
        return true;
    },
    UserLiquidityProvided: async (intent, solver_id) => {
        // TODO finish this check to see if withdrawal requested is completed, e.g. put in arguments of withdrawal
        const res = await checkBitfinexMoves({
            method: 'evm',
            amount: parseInt(intent.amount) * -1,
            start: nanoToMs(intent.created),
            receiver: await getEvmDepositAddress(),
        });
        if (!res) {
            return false;
        }
        intent.nextState = 'ReturnLiquidity';
        return true;
    },
    ReturnLiquidity: async (intent, solver_id) => {
        const res = await withdrawToNear(intent.amount);

        if (!res) {
            return false;
        }
        intent.nextState = 'LiquidityReturned';
        return true;
    },
    // TODO is this check needed?

    // LiquidityReturned: async (intent, solver_id) => {
    //     const res = await checkBitfinexMoves({
    //         amount: parseInt(intent.amount) * -1, // negative for withdrawals
    //         start: nanoToMs(intent.created), // convert to ms from nanos
    //         receiver: await getEvmAddress(),
    //         method: 'tron',
    //     });

    //     if (!res) {
    //         return false;
    //     }
    //     intent.nextState = 'CompleteSwap';
    //     return true;
    // },
};

// the cron runner

const cronTimeout = () => setTimeout(cron, 10000); // 10s

export async function cron(prevIntent) {
    const solver_id = (await getAgentAccount()).workerAccountId;

    // check if we have a previous intent whose state was not updated successfully
    if (prevIntent) {
        // update to the next state (this should eventually resolve)
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
        // state was updated, continue with cron and get intent at next state
        return cronTimeout();
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

    // update to next state
    const updateStateResult = await updateState(solver_id, intent.nextState);
    if (!updateStateResult) {
        console.log(`Failed to update state to ${intent.nextState}`);
        return cronTimeout(intent);
    }

    return cronTimeout();
}
