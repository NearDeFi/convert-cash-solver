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
import { getAgentAccount } from '@neardefi/shade-agent-js';
import { getEvmAddress, signAndVerifyEVM } from './evm.js';
import { getTronAddress, signAndVerifyTRON } from './tron.js';
import { getNearAddress, signAndVerifyNEAR } from './near.js';
import { cron, updateState } from './cron.js';

const PORT = 3000;

export const callWithAgent = async ({ methodName, args }) => {
    const res = await fetch(`http://localhost:3140/api/call`, {
        method: 'POST',
        body: JSON.stringify({
            methodName,
            args,
        }),
    });

    try {
        return res.json();
    } catch (e) {
        if (res.status !== 200) {
            console.log('Error from fetch call to agent:', await res.text());
            return { success: false };
        }
        return { success: true };
    }
};

const app = new Hono();

app.use('/*', cors());

app.get('/api/cron', async (c) => {
    cron(); // can't await cron it runs forever
    return c.json({ done: true });
});

app.get('/api/state', async (c) => {
    const solver_id = (await getAgentAccount()).workerAccountId;
    const res = await updateState(solver_id, 'LiquidityProvided');
    return c.json({ res });
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

console.log('Server listening on port: ', PORT);

serve({
    fetch: app.fetch,
    port: PORT,
    hostname: '0.0.0.0',
});
