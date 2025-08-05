/** 

TRON Bitfinex Deposit Address: TC2Xv6gHTKczLnvXC2PWv8X44rWUYFwjEw
ETH Bitfinex Deposit Address: 0xdeD214b52CFc4BeDEDEAD938a82A9D2012e13d8f
NEAR Bitfinex Deposit Address: f4fed98a87edbef955c28e1a4d9a1b547343f05df9882db134447d89d318177e

EVM Derived Address, ac-proxy.shadeagent.near, evm-1: 0x09Ff5BE31041ec96406EaFa3Abc9d098085381e9
Tron Derived Address ac-proxy.shadeagent.near, tron-1: TQ4Jo6cNH4hsdqKpkAYH4HZvj7ng1HcQm3
NEAR Derived Address, ac-proxy.shadeagent.near, pool-1: cfbdf6e7462659d18926d14b942c6e320a75f59b811a7f7e52c154750e059f84
Tron Test Wallet: TXrv6zHfFuCvRetZcEq2k6f7SQ8LnsgD8X

**/

import { serve } from '@hono/node-server';
import { cors } from 'hono/cors';
import { Hono } from 'hono';
import {
    getNearDepositAddress,
    withdrawToTron,
    checkBitfinexMoves,
    getBitfinexMoves,
    getEvmDepositAddress,
} from './bitfinex.js';

import {
    requestLiquidityUnsigned,
    requestLiquidityBroadcast,
    getNearAddress,
} from './near.js';

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

export const callWithAgent = async ({ methodName, args }) =>
    fetch(`http://localhost:3140/api/call`, {
        method: 'POST',
        body: JSON.stringify({
            methodName,
            args,
        }),
    }).then((r) => r.json());

const app = new Hono();

app.use('/*', cors());

app.get('/api/evm-address', async (c) => {
    const address = await getEvmDepositAddress();
    return c.json({ address });
});

app.get('/api/near-address', async (c) => {
    const address = await getNearAddress();
    return c.json({ address });
});
app.get('/api/tron-address', async (c) => {
    const { address } = await getTronAddress();
    return c.json({ address });
});

app.get('/api/bitfinex-moves', async (c) => {
    const res = await getBitfinexMoves({});
    console.log(res);
    return c.json(res);
});

app.get('/api/test-evm', async (c) => {
    const res = await sendTokens({});
    return c.json(res);
});

app.get('/api/test-tron', async (c) => {
    const { address } = await getTronAddress();

    const { txHash, rawTransaction } = await tronUSDTUnsigned({
        to: 'TC2Xv6gHTKczLnvXC2PWv8X44rWUYFwjEw',
        from: address,
        amount: 5,
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

    return c.json({ res });
});

app.get('/api/test', async (c) => {
    const res1 = await fetch('http://localhost:3000/api/test-deposit').then(
        (r) => r.json(),
    );
    console.log(res1);

    const res2 = await fetch('http://localhost:3000/api/intents').then((r) =>
        r.json(),
    );
    console.log(res2);

    const res3 = await fetch('http://localhost:3000/api/claim-intent').then(
        (r) => r.json(),
    );
    console.log(res3);

    const res4 = await fetch('http://localhost:3000/api/solver-intent').then(
        (r) => r.json(),
    );
    console.log(res4);

    const res5 = await fetch(
        'http://localhost:3000/api/update-intent-state',
    ).then((r) => r.json());
    console.log(res5);

    const res6 = await fetch('http://localhost:3000/api/solver-intent').then(
        (r) => r.json(),
    );
    console.log(res6);
});

app.get('/api/test-deposit', async (c) => {
    try {
        const intentRes = await callWithAgent({
            methodName: 'new_intent',
            args: {
                amount: '1000000',
                deposit_hash:
                    '0xe2a2b0f97cbbf233a23d33548e33ded6911848623992487325beab95eb6f7d27',
                src_token_address: '0xdAC17F958D2ee523a2206206994597C13D831ec7',
                src_chain_id: 1,
                dest_token_address: 'TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t',
                dest_chain_id: 728126428,
                dest_receiver_address:
                    '0x525521d79134822a342d330bd91DA67976569aF1',
            },
        });

        return c.json({ intentRes: intentRes.status === 200 });
    } catch (e) {
        return c.json({ intentRes: false });
    }
});

app.get('/api/intents', async (c) => {
    const getDepositsRes = await contractView({
        methodName: 'get_intents',
        args: {},
    });

    if (getDepositsRes.length === 0) {
        return c.json({ error: 'No intents found' }, 404);
    }

    return c.json(getDepositsRes);
});

app.get('/api/claim-intent', async (c) => {
    try {
        const claimIntent = await callWithAgent({
            methodName: 'claim_intent',
            args: {
                index: 0,
            },
        });

        return c.json({ intentRes: claimIntent.status === 200 });
    } catch (e) {
        return c.json({ claimIntent: false });
    }
});

app.get('/api/solver-intent', async (c) => {
    const solver_id = (await getAgentAccount()).workerAccountId;

    const solverIntent = await contractView({
        methodName: 'get_intent_by_solver',
        args: {
            solver_id,
        },
    });

    return c.json(solverIntent);
});

app.get('/api/update-intent-state', async (c) => {
    const solver_id = (await getAgentAccount()).workerAccountId;

    const solverIntent = await contractCall({
        methodName: 'update_intent_state',
        args: {
            solver_id,
            state: 'LiquidityProvided',
        },
    });

    return c.json(solverIntent);
});

app.get('/api/request-liquidity', async (c) => {
    const solver_id = (await getAgentAccount()).workerAccountId;
    const to = await getNearDepositAddress();
    const solverIntent = await contractView({
        methodName: 'get_intent_by_solver',
        args: {
            solver_id,
        },
    });

    const { payload, transaction } = await requestLiquidityUnsigned({
        to,
        amount: solverIntent.amount,
    });

    const liqRes = await callWithAgent({
        methodName: 'get_signature',
        args: { path: 'pool-1', payload, key_type: 'Eddsa' },
    });

    const broadcastRes = await requestLiquidityBroadcast({
        transaction,
        signature: liqRes.signature,
    });

    // TODO if successful, update intent state to LiquidityProvided

    return c.json({ broadcastRes });
});

app.get('/api/check-near', async (c) => {
    const res = await checkBitfinexMoves({
        amount: 1,
        start: 1751062473000,
        receiver: await getNearDepositAddress(),
        method: 'near',
    });

    if (!res) {
        return c.json({ error: 'Failed to get required credit' }, 500);
    }

    return c.json(res);
});

app.get('/api/check-tron', async (c) => {
    const res = await checkBitfinexMoves({
        amount: -5,
        start: 1751062473000,
        receiver: 'TXrv6zHfFuCvRetZcEq2k6f7SQ8LnsgD8X',
        method: 'tron',
    });

    if (!res) {
        return c.json({ error: 'Failed to get required credit' }, 500);
    }

    return c.json(res);
});

console.log('Server listening on port: ', PORT);

serve({
    fetch: app.fetch,
    port: PORT,
    hostname: '0.0.0.0',
});
