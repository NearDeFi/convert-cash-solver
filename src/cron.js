import {
    getNearDepositAddress,
    getEvmDepositAddress,
    withdrawToTron,
    withdrawToNear,
    checkBitfinexMoves,
} from './bitfinex.js';
import {
    requestLiquidityUnsigned,
    requestLiquidityBroadcast,
    getNearAddress,
} from './near.js';

import { agentCall, agentView, agentAccountId } from '@neardefi/shade-agent-js';

import {
    tronUSDTUnsigned,
    tronBroadcastTx,
    constructTronSignature,
    checkTronTx,
    getTronAddress,
} from './tron.js';

import { getEvmAddress, sendEVMTokens } from './evm.js';

import { callWithAgent } from './app.js';

const nanoToMs = (nanos) => Math.floor(nanos / 1e6) - 1000;

async function getIntent(solver_id) {
    try {
        const intent = await agentView({
            methodName: 'get_intent_by_solver',
            args: {
                solver_id,
            },
        });
        return intent;
    } catch (e) {
        console.log(e);
        return null;
    }
}

async function claimIntent() {
    /*

TODO upgrade to latest shade-agent-js to fix this error

	Claiming new intent for solver_id 1af660a79008c0ce0c5a5605c6107fd7f355a41cb407d4728300a4e15b35cdbb
SyntaxError: Unexpected end of JSON input
    at JSON.parse (<anonymous>)
    at parseJSONFromBytes (node:internal/deps/undici/undici:5738:19)
    at successSteps (node:internal/deps/undici/undici:5719:27)
    at fullyReadBody (node:internal/deps/undici/undici:4609:9)
    at process.processTicksAndRejections (node:internal/process/task_queues:105:5)
    at async consumeBody (node:internal/deps/undici/undici:5728:7)
    at async claimIntent (file:///home/matt/Projects/mattlockyer/convert-cash-solver/src/cron.js:47:9)
    at async cron (file:///home/matt/Projects/mattlockyer/convert-cash-solver/src/cron.js:309:25)
    at async file:///home/matt/Projects/mattlockyer/convert-cash-solver/src/app.js:43:5
    at async dispatch (file:///home/matt/Projects/mattlockyer/convert-cash-solver/node_modules/hono/dist/compose.js:22:17)
Error claiming new intent 1af660a79008c0ce0c5a5605c6107fd7f355a41cb407d4728300a4e15b35cdbb
*/

    try {
        const res = await callWithAgent({
            methodName: 'claim_intent',
            args: {
                index: 0,
            },
        });
        return res.success;
    } catch (e) {
        console.log(e);
        return false;
    }
}

export async function updateState(solver_id, state) {
    try {
        await agentCall({
            methodName: 'update_intent_state',
            args: {
                solver_id,
                state,
            },
        });
        console.log(`State updated to ${state} for solver_id ${solver_id}`);
        return true;
    } catch (e) {
        console.log(e);
        return false;
    }
}

// main state transition functions after intent is claimed

const stateFuncs = {
    Claimed: async (intent, solver_id) => {
        try {
            const to = await getNearDepositAddress();
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

            if (!broadcastRes?.txHash) {
                console.log(
                    'Error broadcasting liquidity request:',
                    broadcastRes,
                );
                return false;
            }

            // every state transition needs to add "nextState" to the intent and return true
            intent.nextState = 'LiquidityProvided';
            return true;
        } catch (e) {
            console.log('Error requesting liquidity:', e);
        }
        return false;
    },
    LiquidityProvided: async (intent, solver_id) => {
        try {
            const res = await checkBitfinexMoves({
                amount: intent.amount,
                start: nanoToMs(intent.created), // convert to ms from nanos
                receiver: await getNearDepositAddress(),
                method: 'near',
            });
            console.log('Bitfinex moves check result:', res);

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
        const res = await withdrawToTron(intent.amount);

        if (!res) {
            return false;
        }
        intent.nextState = 'WithdrawRequested';
        return true;
    },
    WithdrawRequested: async (intent, solver_id) => {
        const { address: receiver } = await getTronAddress();
        // TODO finish this check to see if withdrawal requested is completed, e.g. put in arguments of withdrawal
        const res = await checkBitfinexMoves({
            amount: parseInt(intent.amount) * -1, // negative for withdrawals
            start: nanoToMs(intent.created), // convert to ms from nanos
            receiver,
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

            /*

            Tron broadcast {
  result: true,
  txid: '07838fe4ec07b26b16dd331736dd653435182874c6c6be259c3e79a51991b958',
  transaction: {
    visible: false,
    txID: '07838fe4ec07b26b16dd331736dd653435182874c6c6be259c3e79a51991b958',
    raw_data: {
      contract: [Array],
      ref_block_bytes: '0eee',
      ref_block_hash: '8f09ba181f98c49a',
      expiration: 1754606994000,
      fee_limit: 30000000,
      timestamp: 1754606935169
    },
    raw_data_hex: '0a020eee22088f09ba181f98c49a40d084e8b588335aae01081f12a9010a31747970652e676f6f676c65617069732e636f6d2f70726f746f636f6c2e54726967676572536d617274436f6e747261637412740a15419a88ad5b6319871d2aacd74db5c8f191193dd295121541a614f803b6fd780986a42c78ec9c7f77e6ded13c2244a9059cbb000000000000000000000000169149d8c1f5b11d51b98007ffd0d6af8b30830a00000000000000000000000000000000000000000000000000000000004c4b407081b9e4b5883390018087a70e',
    signature: [
      'aa274b0e4db64599bbbaf81f576dad1ebcc9d47a02b1903471951c1f3abc64c94c43fb9dc6b69ef6986657d77011689d2d40e303119cc2068fb829a55ab6922600'
    ]
  }
}

*/

            await agentCall({
                methodName: 'update_swap_hash',
                args: {
                    solver_id,
                    swap_hash: txHash,
                },
            });
            intent.nextState = 'CheckSwapComplete';
            return true;
        } catch (e) {
            console.log('Error completing swap:', e);
        }
        return false;
    },
    CheckSwapComplete: async (intent, solver_id) => {
        // make sure tron tx is complete, how many confirmations?

        // let's research confirmations for large amounts

        const txHash = intent.swap_hash;

        try {
            const complete = await checkTronTx(txHash);
            if (!complete) {
                console.log('Tron transaction not complete:', txHash);
                return false;
            }
            intent.nextState = 'SwapComplete';
            return true;
        } catch (e) {
            console.log('Error checking Tron tx:', e);
            return false;
        }
    },
    SwapComplete: async (intent, solver_id) => {
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

    LiquidityReturned: async (intent, solver_id) => {
        // check this solvers bitfinex moves
        const res = await checkBitfinexMoves({
            amount: parseInt(intent.amount) * -1, // negative for withdrawals
            start: nanoToMs(intent.created), // convert to ms from nanos
            receiver: await getNearAddress(),
            method: 'near',
        });

        if (!res) {
            return false;
        }
        intent.nextState = 'IntentComplete';
        return true;

        // check contract to see if liquidity was returned
    },
};

// the cron runner

const cronTimeout = (prevIntent) => {
    if (process.env.MANUAL_CRON) return;
    setTimeout(() => cron(prevIntent), 10000); // 10s
};

export async function cron(prevIntent) {
    const solver_id = (await agentAccountId()).workerAccountId;

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
        console.log('Claiming new intent for solver_id', solver_id);
        const claimed = await claimIntent();
        if (!claimed) {
            console.log('Error claiming new intent', solver_id);
            return cronTimeout();
        }
    }

    console.log('Cron running for solver_id, intent:', solver_id, intent);

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
