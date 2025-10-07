/** 

TRON Bitfinex Deposit Address: TC2Xv6gHTKczLnvXC2PWv8X44rWUYFwjEw
ETH Bitfinex Deposit Address: 0xdeD214b52CFc4BeDEDEAD938a82A9D2012e13d8f
NEAR Bitfinex Deposit Address: f4fed98a87edbef955c28e1a4d9a1b547343f05df9882db134447d89d318177e

EVM Derived Address, ac-proxy.shadeagent.near, evm-1: 0x5f6e67c54bef46bdac466b8c357005105e8f4ed9
Tron Derived Address ac-proxy.shadeagent.near, tron-1: TQ4Jo6cNH4hsdqKpkAYH4HZvj7ng1HcQm3
NEAR Derived Address, ac-proxy.shadeagent.near, pool-1: cfbdf6e7462659d18926d14b942c6e320a75f59b811a7f7e52c154750e059f84
Tron Test Wallet: TXrv6zHfFuCvRetZcEq2k6f7SQ8LnsgD8X

Next steps tron to eth

**/

import { serve } from '@hono/node-server';
import { cors } from 'hono/cors';
import { Hono } from 'hono';
import {
    getBitfinexMoves,
    getNearDepositAddress,
    getEvmDepositAddress,
} from './bitfinex.js';
import { agentAccountId, agentCall, agentView } from '@neardefi/shade-agent-js';
import { getEvmAddress, signAndVerifyEVM } from './evm.js';
import { getTronAddress, signAndVerifyTRON } from './tron.js';
import {
    getNearAddress,
    signAndVerifyNEAR,
    requestLiquidityUnsigned,
    requestLiquidityBroadcast,
    contractCall,
} from './near.js';
import { cron, updateState } from './cron.js';

import {
    erc191Verify,
    getDepositAddress,
    getRecentDeposits,
    getIntentDiffDetails,
    createLocallySignedErc191Intent,
} from './intents.js';

// add the websocket client
import * as temp from './bus/main.js';

const PORT = 3000;

// helper
export const callWithAgent = async ({
    contractId,
    methodName,
    args,
    gas,
    deposit,
}) => {
    console.log(methodName);

    let res;
    try {
        res = await agentCall({
            contractId,
            methodName,
            args,
            gas,
            deposit,
        });
    } catch (e) {
        console.log('Error from fetch call to agent', e);
    }

    if (res.error) {
        console.log('Error from fetch call to agent:', res.error);
        return { success: false };
    }

    return res;
};

const app = new Hono();

app.use('/*', cors());

app.use('/api/test-intents-key', async (c) => {
    const public_key = process.env.EVM_PUBLIC_KEY;

    const intentAddKey = await createLocallySignedErc191Intent(
        process.env.EVM_ADDRESS,
        process.env.EVM_PRIVATE_KEY,
        [
            {
                intent: 'remove_public_key',
                public_key: process.env.EVM_PUBLIC_KEY,
            },
        ],
    );

    // TODO test this
    const resAddKey = await contractCall({
        accountId: process.env.NEAR_CONTRACT_ID,
        contractId: 'intents.near',
        methodName: 'execute_intents',
        args: { signed: [intentAddKey] },
    });

    console.log('resAddKey:', resAddKey);

    const intentRemoveKey = await createLocallySignedErc191Intent(
        process.env.EVM_ADDRESS,
        process.env.EVM_PRIVATE_KEY,
        [
            {
                intent: 'remove_public_key',
                public_key: process.env.EVM_PUBLIC_KEY,
            },
        ],
    );

    // TODO test this
    const resRemoveKey = await contractCall({
        accountId: process.env.NEAR_CONTRACT_ID,
        contractId: 'intents.near',
        methodName: 'execute_intents',
        args: { signed: [intentRemoveKey] },
    });

    console.log('resRemoveKey:', resRemoveKey);

    return c.json({ res });
});

app.post('/api/verifyIntent', async (c) => {
    const args = await c.req.json();

    const tokenDiff = {
        payload: args.token_diff_payload,
        signature: args.token_diff_signature,
    };

    const withdraw = {
        payload: args.withdraw_payload,
        signature: args.withdraw_signature,
    };

    // check message and signature is valid
    const { recoveredAddress, validSignature } = await erc191Verify(tokenDiff);
    if (!validSignature) {
        return c.json({ validSignature });
    }

    const srcChain = 'eth:1';
    const srcTokenAddress = '0xdac17f958d2ee523a2206206994597c13d831ec7';

    const { depositAddress } = await getDepositAddress(
        recoveredAddress,
        srcChain,
    );

    const { deposits } = await getRecentDeposits(recoveredAddress, srcChain);
    // console.log('deposits:', deposits);

    // check recent deposit matches payload details
    const deposit = deposits
        .reverse()
        .find(
            (d) =>
                d.status === 'COMPLETED' &&
                d.defuse_asset_identifier.indexOf(srcTokenAddress) > -1,
        );
    const tokenDiffIntent = JSON.parse(tokenDiff.payload).intents[0];
    const withdrawIntent = JSON.parse(withdraw.payload).intents[0];

    const { srcAmount } = getIntentDiffDetails(tokenDiffIntent);

    const intentValidity = {
        isTokenDiff: tokenDiffIntent.intent === 'token_diff',
        accountIdMatch:
            JSON.parse(tokenDiff.payload).signer_id === deposit.account_id,
        chainMatch: srcChain === deposit.chain,
        depositAddressMatch: deposit.address === depositAddress,
        defuseAssetMatch:
            deposit.defuse_asset_identifier.indexOf(srcTokenAddress) > -1,
        amountMatch: srcAmount === '-' + deposit.amount.toString(),
        isDepositCompleted: deposit.status === 'COMPLETED',
    };

    const isIntentValid = Object.values(intentValidity).reduce(
        (acc, val) => acc && val,
        true,
    );
    console.log('isIntentValid', isIntentValid);
    // if (!isIntentValid) {
    //     console.log(intentValidity);
    //     return c.json({ intentValidity, error: 'intent is invalid' });
    // }

    // check contract to see if intent already exists
    const getIntentsRes = await agentView({
        methodName: 'get_intents',
        args: {},
    });

    if (getIntentsRes.find((i) => i.deposit_hash === deposit.tx_hash)) {
        return c.json({
            intentValidity,
            error: 'intent already exists in contract',
        });
    }

    // submit intent to contract
    let submitted = false,
        error = null;
    try {
        await agentCall({
            methodName: 'new_intent',
            args: {
                deposit_hash: deposit.tx_hash,
                data: JSON.stringify(args), // stringified args of what the user passed in and we verified
            },
        });
        submitted = true;
    } catch (e) {
        console.error('Error submitting intent:', e.message);
        error = /already exists/.test(e.message)
            ? 'intent already exists'
            : e.message;
    }

    // check contract to see if intent already exists
    const getIntentsRes2 = await agentView({
        methodName: 'get_intents',
        args: {},
    });

    if (getIntentsRes2.find((i) => i.deposit_hash === deposit.tx_hash)) {
        return c.json({
            intentValidity,
            error: 'intent already exists in contract',
        });
    }

    return c.json({ intentValidity, submitted, error });
});

app.get('/api/test-intent', async (c) => {
    const testIntent = {
        token_diff_payload:
            '{"signer_id":"0xa48c13854fa61720c652e2674cfa82a5f8514036","nonce":"ENirzu1lEehsRCP4GIEb3JKEHVV0b0nO+nwTeCa60Oc=","verifying_contract":"intents.near","deadline":"2025-12-21T23:29:34.696Z","intents":[{"intent":"token_diff","diff":{"nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near":"-5000000","nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near":"4500000"}}]}',
        token_diff_signature:
            'secp256k1:KDPeZB7MyJhyDLfgCP1WWv7c9c2MJXf7wEJpc7jypDZVFkULZMTe2571U7B6D3GxiHa3w9XYop5ZzpkMZ6JwEWRpb',
        withdraw_payload:
            '{"signer_id":"0xa48c13854fa61720c652e2674cfa82a5f8514036","nonce":"2FnYc/a8NC/4/eGx0yo+gn2l9O0zEY7Bta4PwEkb5Ps=","verifying_contract":"intents.near","deadline":"2025-12-21T23:29:34.697Z","intents":[{"intent":"ft_withdraw","token":"tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near","receiver_id":"tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near","amount":"4500000","memo":"WITHDRAW_TO:TAqER36ULBm63eWVsRJHBbJHeNrP4Lp1jq"}]}',
        withdraw_signature:
            'secp256k1:6CETEL5WQuFBtmN3wAD7hyzMTgcVVtvvuSmmGR6NPUE7yruMMYmzPfxPhqfdUaUXzHyZvJMy3LniKr8AmQbQ9qQwh',
    };

    const res = await fetch('http://localhost:3000/api/verifyIntent', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(testIntent),
    }).then((r) => r.json());

    return c.json({ res });
});

app.get('/api/cron', async (c) => {
    cron(); // can't await cron it runs forever
    return c.json({ done: true });
});

app.get('/api/state', async (c) => {
    const solver_id = (await agentAccountId()).accountId;
    const res = await updateState(solver_id, 'LiquidityCredited');
    return c.json({ res });
});

console.log('Server listening on port: ', PORT);

serve({
    fetch: app.fetch,
    port: PORT,
    hostname: '0.0.0.0',
});

/**
 *
 * Deprecated
 *
 */

app.get('/api/return-near', async (c) => {
    try {
        const { payload, transaction } = await requestLiquidityUnsigned({
            to: '70d6b5c7307f794c799370bc495ce5f7c9dfc1f27f59f411a9c135076d4e74be',
            amount: '5000000000',
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
            console.log('Error broadcasting liquidity request:', broadcastRes);
            return c.json({ success: false });
        }

        return c.json({ success: true });
    } catch (e) {
        console.log('Error requesting liquidity:', e);
    }
    return c.json({ success: false });
});

app.get('/api/test-deposit', async (c) => {
    try {
        const intentRes = await callWithAgent({
            methodName: 'new_intent',
            args: {
                amount: '5000000',
                deposit_hash:
                    '0x6b572d796b7da99f3f647bbe78a9a39cbf7560a4c182ac1f7765001a98c338fa',
                src_chain_id: 1,
                dest_chain_id: 728126428,
                src_token_address: '0xdAC17F958D2ee523a2206206994597C13D831ec7',
                dest_token_address: 'TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t',
                dest_receiver_address: 'TC2Xv6gHTKczLnvXC2PWv8X44rWUYFwjEw',
            },
        });

        return c.json({ intentRes: intentRes.status === 200 });
    } catch (e) {
        return c.json({ intentRes: false });
    }
});

app.get('/api/addresses', async (c) => {
    // bitfinex deposit addresses
    const evmBitfinex = await getEvmDepositAddress();
    const nearBitfinex = await getNearDepositAddress();

    const { address: evmDerived } = await getEvmAddress(); // emv-1
    const evmValid = await signAndVerifyEVM();

    const { address: tronDerived } = await getTronAddress(); // tron-1
    const tronValid = await signAndVerifyTRON();

    const { address: nearDerived } = await getNearAddress(); // pool-1
    const nearValid = await signAndVerifyNEAR();

    return c.json({
        evmBitfinex,
        nearBitfinex,
        evmDerived,
        evmValid,
        tronDerived,
        tronValid,
        nearDerived,
        nearValid,
    });
});

app.get('/api/bitfinex-moves', async (c) => {
    const res = await getBitfinexMoves({});
    console.log(res);
    return c.json(res);
});
