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
import { agentAccountId, agentCall } from '@neardefi/shade-agent-js';
import { getEvmAddress, signAndVerifyEVM } from './evm.js';
import { getTronAddress, signAndVerifyTRON } from './tron.js';
import {
    getNearAddress,
    signAndVerifyNEAR,
    requestLiquidityUnsigned,
    requestLiquidityBroadcast,
} from './near.js';
import { cron, updateState } from './cron.js';

import {
    erc191Verify,
    getDepositAddress,
    getRecentDeposits,
} from './intents.js';

const PORT = 3000;

// helper
export const callWithAgent = async ({ methodName, args }) => {
    console.log(methodName);

    let res;
    try {
        res = await agentCall({
            methodName,
            args,
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

app.post('/api/verifyIntent', async (c) => {
    // erc191 message

    /*
    {
        "standard": "erc191",
        "payload": "{\"signer_id\":\"0xa48c13854fa61720c652e2674cfa82a5f8514036\",\"nonce\":\"etfB9ran4vpg4ilG0I4q9A5K+8RtSBMpd/Ubesi1c6g=\",\"verifying_contract\":\"intents.near\",\"deadline\":\"2025-12-16T23:40:06.907Z\",\"intents\":[{\"intent\":\"token_diff\",\"diff\":{\"nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near\":\"-1000000\",\"nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near\":\"999391\"}},{\"intent\":\"ft_withdraw\",\"token\":\"tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near\",\"receiver_id\":\"tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near\",\"amount\":\"999391\",\"memo\":\"WITHDRAW_TO:TAqER36ULBm63eWVsRJHBbJHeNrP4Lp1jq\"}]}",
        "signature": "secp256k1:5ddLZT3nSge1yGMBjFJXp4rKPJguabgFXBRzmfWEcrtsjurbBtJ4iQiFHGXNdY6oVWyLCfUG1rx9N48pNNqqMHa1V"
    }
     */
    const args = await c.req.json();

    const srcChain = 'eth:1';

    // check message and signature is valid
    const { recoveredAddress, validSignature, payload } = await erc191Verify(
        args,
    );
    if (!validSignature) {
        return c.json({ validSignature });
    }

    const { depositAddress } = await getDepositAddress(
        recoveredAddress,
        srcChain,
    );

    const { deposits } = await getRecentDeposits(recoveredAddress, srcChain);

    // check recent deposit matches payload details
    const deposit = deposits[0];
    const tokenDiffIntent = payload.intents[0];
    const withdrawIntent = payload.intents[1];
    const [srcToken, srcAmount] = Object.entries(tokenDiffIntent.diff).find(
        ([k]) => k === deposit.defuse_asset_identifier,
    );
    const [destToken, destAmount] = Object.entries(tokenDiffIntent.diff).find(
        ([k]) => k !== deposit.defuse_asset_identifier,
    );

    const intentValidity = {
        isTokenDiff: tokenDiffIntent.intent === 'token_diff',
        accountIdMatch: tokenDiffIntent.signer_id === deposit.account_id,
        chainMatch: srcChain === deposit.chain,
        depositAddressMatch: deposit.address === depositsAddress,
        defuseAssetMatch: srcToken === deposit.defuse_asset_identifier,
        amountMatch: srcAmount === deposit.amount,
        isDepositCompleted: deposit.status === 'COMPLETED',
    };

    const isIntentValid = Object.values(intentValidity).reduce(
        (acc, val) => acc && val,
        true,
    );
    console.log('isIntentValid', isIntentValid);
    if (!isIntentValid) {
        return c.json({ intentValidity, error: 'intent is invalid' });
    }

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

    /*{
        "tx_hash": "",
        "chain": "CHAIN_TYPE:CHAIN_ID",
        "defuse_asset_identifier": "eth:8543:0x123",
        "decimals": 18,
        "amount": 10000000000,
        "account_id": "user.near",
        "address": "0x123",
        "status": "COMPLETED" // PENDING, FAILED
      },
      */

    // submit intent to contract
    let submitted = false,
        error = null;
    try {
        await agentCall({
            methodName: 'new_intent',
            args: {
                amount: deposit.amount,
                deposit_hash: deposit.tx_hash,
                src_token_address: srcToken,
                src_chain: deposit.chain,
                dest_token_address: destToken,
                dest_chain_id: destToken, // TODO what should this be? Do we need to keep it?
                dest_receiver_address: withdrawIntent.receiver_id,
            },
        });
        submitted = true;
    } catch (e) {
        console.error('Error submitting intent:', e.message);
        error = /already exists/.test(e.message)
            ? 'intent already exists'
            : e.message;
    }

    return c.json({ intentValidity, submitted, error });
});

app.get('/api/cron', async (c) => {
    cron(); // can't await cron it runs forever
    return c.json({ done: true });
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

app.get('/api/state', async (c) => {
    const solver_id = (await agentAccountId()).accountId;
    const res = await updateState(solver_id, 'LiquidityCredited');
    return c.json({ res });
});

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
