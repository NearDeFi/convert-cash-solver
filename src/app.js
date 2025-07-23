import { serve } from '@hono/node-server';
import { cors } from 'hono/cors';
import { Hono } from 'hono';
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

import { getTronAddress, tronUSDTUnsigned } from './tron.js';

const callWithAgent = async ({ methodName, args }) =>
    fetch(`http://localhost:3140/api/call`, {
        method: 'POST',
        body: JSON.stringify({
            methodName,
            args,
        }),
    }).then((r) => r.json());

const app = new Hono();

app.use('/*', cors());

app.get('/api/test-tron', async (c) => {
    const { address } = await getTronAddress();
    const { txHash, rawTransaction } = await tronUSDTUnsigned(address);

    return c.json({ address, txHash });
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
        'http://localhost:3000/api/request-liquidity',
    ).then((r) => r.json());
    console.log(res5);
});

app.get('/api/test-deposit', async (c) => {
    try {
        const intentRes = await callWithAgent({
            methodName: 'new_intent',
            args: {
                amount: '1000000',
                hash: '0xe2a2b0f97cbbf233a23d33548e33ded6911848623992487325beab95eb6f7d27',
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
        methodName: 'request_liquidity',
        args: { payload },
    });

    const broadcastRes = await requestLiquidityBroadcast({
        transaction,
        signature: liqRes.signature,
    });

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
