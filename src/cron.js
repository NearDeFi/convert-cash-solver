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

import { getDepositAddress } from './intents.js';

import { getEvmAddress, parseSignature, sendEVMTokens } from './evm.js';

import { callWithAgent } from './app.js';

const INTENTS_CHAIN_ID_TRON = 'tron:728126428';

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
        // get an EVM address to use as implicit-eth address for the solver agent
        const { address } = await getEvmAddress();
        // get a deposit address for that EVM address on Tron
        const { depositAddress } = await getDepositAddress(
            address,
            INTENTS_CHAIN_ID_TRON,
        );
        const res = await withdrawToTron(intent.amount, depositAddress);

        if (!res) {
            return false;
        }
        intent.nextState = 'WithdrawRequested';
        return true;
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
            amount: parseInt(intent.amount) * -1, // negative for withdrawals
            start: nanoToMs(intent.created), // convert to ms from nanos
            receiver: depositAddress,
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
        // get an EVM address to use as implicit-eth address for the solver agent
        const { address } = await getEvmAddress();
        // get a deposit address for that EVM address on Tron
        const { depositAddress } = await getDepositAddress(
            address,
            INTENTS_CHAIN_ID_TRON,
        );

        const fee = 500000;
        const sendToken =
            'nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near';
        const receiveToken =
            'nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near';
        const sendAmount = '-' + (1000000 - fee).toString();
        const receiveAmount = '1000000';
        const standard = 'erc191';
        const nonce = Buffer.from(randomBytes(n)).toString('base64');
        const deadline = new Date(Date.now() + 10 * 60 * 1000).toISOString(); // 10 minutes from now

        const payload = {
            signer_id: address,
            nonce,
            verifying_contract: 'intents.near',
            deadline,
            intents: [
                {
                    intent: 'token_diff',
                    diff: {
                        [sendToken]: sendAmount,
                        [receiveToken]: receiveAmount,
                    },
                },
                {
                    intent: 'ft_withdraw',
                    token: receiveToken,
                    receiver_id: depositAddress, // TODO bitfinex withdrawal address
                    amount: receiveAmount,
                    memo: `WITHDRAW_TO:${depositAddress}`,
                },
            ],
        };

        // sign payload with evm key using chain signatures
        const payloadStr = JSON.stringify(payload);
        const payloadHex = Buffer.from(payloadStr).toString('hex');
        const sigRes = await callWithAgent({
            methodName: 'request_signature',
            args: {
                path: 'evm-1',
                payload: payloadHex,
                key_type: 'Ecdsa',
            },
        });

        // parse signature response
        const r = Buffer.from(sigRes.big_r.affine_point.substring(2), 'hex');
        const s = Buffer.from(sigRes.s.scalar, 'hex');
        const v = sigRes.recovery_id;
        const rsvSignature = new Uint8Array(65);
        rsvSignature.set(r, 0);
        rsvSignature.set(s, 32);
        rsvSignature[64] = v;
        const signature = 'secp256k1:' + bs58.encode(rsvSignature);

        // final intent
        const nearIntent = {
            standard,
            payload,
            signature,
        };

        // TODO combine with user intent and verify intent

        return nearIntent;

        // TODO
        // sign intent with evm key using chain signatures

        //     const sigRes = await callWithAgent({
        //         methodName: 'request_signature',
        //         args: { path: 'tron-1', payload: txHash, key_type: 'Ecdsa' },
        //     });
        //     console.log(sigRes);
        //     const signatureHex = constructTronSignature(sigRes);
        //     console.log('sig:', signatureHex);

        // construct evm signature and signed ERC 191 payload

        /**
         * Deprecated in favor of using intents to execute the swap
         * token_diff, token_diff, ft_withdraw, ft_withdraw
         *
         */
        // const { address } = await getTronAddress();
        // try {
        //     const { txHash, rawTransaction } = await tronUSDTUnsigned({
        //         to: intent.dest_receiver_address,
        //         from: address,
        //         amount: intent.amount,
        //     });
        //     console.log('tron tx: getting chain sig');
        //     const sigRes = await callWithAgent({
        //         methodName: 'request_signature',
        //         args: { path: 'tron-1', payload: txHash, key_type: 'Ecdsa' },
        //     });
        //     console.log(sigRes);
        //     const signatureHex = constructTronSignature(sigRes);
        //     console.log('sig:', signatureHex);
        //     // await verifySignature(txHash, signatureHex);
        //     console.log('tron tx: broadcasting');
        //     const res = await tronBroadcastTx(rawTransaction, signatureHex);
        //     // TODO check if tron tx was succcessful or not right away
        //     console.log('Tron broadcast', res);
        //     await agentCall({
        //         methodName: 'update_swap_hash',
        //         args: {
        //             solver_id,
        //             swap_hash: txHash,
        //         },
        //     });
        //     intent.nextState = 'CheckSwapComplete';
        //     return true;
        // } catch (e) {
        //     console.log('Error completing swap:', e);
        // }
        // return false;
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
