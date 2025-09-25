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

import { checkTronTx } from './tron.js';

import {
    getRecentDeposits,
    getDepositAddress,
    getIntentDiffDetails,
    createSignedErc191Intent,
} from './intents.js';

import { getEvmAddress, parseSignature, sendEVMTokens } from './evm.js';

import { callWithAgent } from './app.js';

const INTENTS_CHAIN_ID_TRON = 'tron:mainnet';

const nanoToMs = (nanos) => Math.floor(nanos / 1e6) - 1000;

async function getIntent(solver_id) {
    try {
        const intent = await agentView({
            methodName: 'get_intent_by_solver',
            args: {
                solver_id,
            },
        });
        if (intent.error) {
            return null;
        }
        return intent;
    } catch (e) {
        console.log(e);
        return null;
    }
}

async function claimIntent() {
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
        const res = await agentCall({
            methodName: 'update_intent_state',
            args: {
                solver_id,
                state,
            },
        });
        if (res.error) {
            throw new Error(res.error);
        }
        console.log(`State updated to ${state} for solver_id ${solver_id}`);
        return true;
    } catch (e) {
        console.log(e);
        return false;
    }
}

function parseAmount(amount) {
    return Math.max(5000000, Math.abs(parseInt(amount, 10)));
}

// main state transition functions after intent is claimed

const stateFuncs = {
    Claimed: async (intent, solver_id) => {
        try {
            const to = await getNearDepositAddress();
            const { payload, transaction } = await requestLiquidityUnsigned({
                to,
                amount: parseAmount(intent.destAmount).toString(),
            });

            const liqRes = await callWithAgent({
                methodName: 'request_signature',
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
                amount: parseAmount(intent.destAmount),
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
        // intent.nextState = 'WithdrawRequested';
        // return true;
    },
    WithdrawRequested: async (intent, solver_id) => {
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
        intent.nextState = 'CompleteSwap';

        return true;
    },
    CompleteSwap: async (intent, solver_id) => {
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
                    [srcToken]: srcAmount.substring(1),
                    [destToken]: (parseInt(destAmount, 10) * -1).toString(),
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
                amount: srcAmount,
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

        const res = await callWithAgent({
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

        const res2 = await callWithAgent({
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
    const solver_id = (await agentAccountId()).accountId;

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
    let intent = await getIntent(solver_id);

    // if no current intent, claim one
    if (!intent) {
        console.log('Claiming new intent for solver_id', solver_id);
        const claimed = await claimIntent();
        if (!claimed) {
            console.log('Error claiming new intent', solver_id);
            return cronTimeout();
        }
    }

    const intentDataJson = JSON.parse(intent.data);
    const userTokenDiffPayload = JSON.parse(intentDataJson.token_diff_payload);
    const userTokenDiffSignature = intentDataJson.token_diff_signature;
    const userWithdrawPayload = JSON.parse(intentDataJson.withdraw_payload);
    const userWithdrawSignature = intentDataJson.withdraw_signature;
    const diffDetails = getIntentDiffDetails(userTokenDiffPayload.intents[0]);
    intent = {
        ...intent,
        ...diffDetails,
        userTokenDiffPayload,
        userTokenDiffSignature,
        userWithdrawPayload,
        userWithdrawSignature,
    };

    console.log('Cron running for solver_id, intent:', solver_id, intent);

    stateFuncs.CompleteSwap(intent, solver_id);

    return;

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
