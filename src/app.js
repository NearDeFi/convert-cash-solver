/** 

TRON Bitfinex Deposit Address: TC2Xv6gHTKczLnvXC2PWv8X44rWUYFwjEw
ETH Bitfinex Deposit Address: 0xdeD214b52CFc4BeDEDEAD938a82A9D2012e13d8f
NEAR Bitfinex Deposit Address: f4fed98a87edbef955c28e1a4d9a1b547343f05df9882db134447d89d318177e

EVM Derived Address, ac-proxy.shadeagent.near, evm-1: 0x5f6e67c54bef46bdac466b8c357005105e8f4ed9
Tron Derived Address ac-proxy.shadeagent.near, tron-1: TQ4Jo6cNH4hsdqKpkAYH4HZvj7ng1HcQm3
NEAR Derived Address, ac-proxy.shadeagent.near, pool-1: cfbdf6e7462659d18926d14b942c6e320a75f59b811a7f7e52c154750e059f84
Tron Test Wallet: TXrv6zHfFuCvRetZcEq2k6f7SQ8LnsgD8X

**/

import { serve } from '@hono/node-server';
import { cors } from 'hono/cors';
import { Hono } from 'hono';
import {
    getBitfinexMoves,
    getNearDepositAddress,
    getEvmDepositAddress,
} from './bitfinex.js';
import { getEvmAddress, signAndRecover } from './evm.js';
import { getTronAddress } from './tron.js';
import { getNearAddress } from './near.js';

const PORT = 3000;

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

app.get('/api/addresses', async (c) => {
    const evmBitfinex = await getEvmDepositAddress();
    const nearBitfinex = await getNearDepositAddress();
    const { address: evmDerived } = await getEvmAddress(); // emv-1
    await signAndRecover();
    const { address: nearDerived } = await getNearAddress(); // pool-1
    const { address: tronDerived } = await getTronAddress(); // tron-1
    return c.json({
        evmBitfinex,
        nearBitfinex,
        evmDerived,
        nearDerived,
        tronDerived,
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
