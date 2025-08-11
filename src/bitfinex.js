/**
 * near-to-solana-bitfinex.js
 *
 * Deposit USDT (NEAR) → Bitfinex → Withdraw USDT (Solana)
 */

import fetch from 'node-fetch'; // v2 – CommonJS compatible
import crypto from 'crypto';
import dotenv from 'dotenv';
const dir = process.cwd();
dotenv.config({ path: `${dir}/.env.development.local` });

import { getTronAddress } from './tron.js';
import { getNearAddress } from './near.js';

// --- constants -------------------------------------------------------------

const NEAR_ENDPOINT = 'https://rpc.mainnet.near.org';
const USDT_CONTRACT = 'usdt.tether-token.near'; // official NEAR USD₮ contract
const USDT_DECIMALS = 6; // NEAR FT has 6 dp
const GAS = '300000000000000'; // 300 Tgas
const YOCTO_NEAR = '1'; // min deposit for FT calls

const BITFINEX_REST = 'https://api.bitfinex.com';
const METHOD_NEAR = 'tetherusdtnear'; // NEAR transport string
const METHOD_SOL = 'tetherusdtsol'; // Solana transport string
const METHOD_TRON = 'tetherusx'; // Solana transport string
const METHOD_EVM = 'tetheruse';

const BITFINEX_METHODS = {
    near: 'TETHERUSDTNEAR',
    tron: 'TETHERUSX',
    evm: 'TETHERUSE',
};

const BITFINEX_KEY = process.env.BITFINEX_KEY;
const BITFINEX_SECRET = process.env.BITFINEX_SECRET;

export const NEAR_DERIVED_ADDRESS =
    'cfbdf6e7462659d18926d14b942c6e320a75f59b811a7f7e52c154750e059f84';

// --- helpers ---------------------------------------------------------------

export function usdtToSingle(amount) {
    return Math.floor(Number(amount) / 1000000).toString();
}

function nonce() {
    return `${Date.now() * 1000}`;
}

function signV2Payload(path, body) {
    const n = nonce();
    const raw = `/api/${path}${n}${JSON.stringify(body)}`;
    const sig = crypto
        .createHmac('sha384', BITFINEX_SECRET)
        .update(raw)
        .digest('hex');

    return { payload: raw, signature: sig, nonce: n };
}

async function bitfinexRequest(path, body) {
    const { signature, nonce: n } = signV2Payload(path, body);

    const res = await fetch(`${BITFINEX_REST}/${path}`, {
        method: 'POST',
        headers: {
            'bfx-nonce': n,
            'bfx-apikey': BITFINEX_KEY,
            'bfx-signature': signature,
            'content-type': 'application/json',
        },
        body: JSON.stringify(body),
    });

    const data = await res.json();

    // “write” endpoints come back with ["MTS", "SUCCESS", …] or ["error", code, msg].
    // “read” endpoints (like /movements/…) are just plain arrays.
    // Only treat the explicit 'error' tuple as fatal.
    if (Array.isArray(data) && data[0] === 'error') {
        throw new Error(`Bitfinex error: ${data[2]}`);
    }

    return data;
}

// scale human amount → smallest-unit string
function toSubunits(amount) {
    return BigInt(Math.round(amount * 10 ** USDT_DECIMALS)).toString();
}

export async function getNearDepositAddress() {
    const path = 'v2/auth/w/deposit/address';
    const body = { wallet: 'exchange', method: METHOD_NEAR, op_renew: 0 };
    const res = await bitfinexRequest(path, body);

    const address = res[4][4]; // address field
    if (address?.length !== 64) {
        address = null;
    }

    return address;
}

export async function getEvmDepositAddress() {
    const path = 'v2/auth/w/deposit/address';
    const body = { wallet: 'exchange', method: METHOD_EVM, op_renew: 0 };
    const res = await bitfinexRequest(path, body);

    let address = res[4][4]; // address field

    if (address?.length !== 42) {
        address = null;
    }

    return address;
}

export async function getBitfinexMoves(start = 0) {
    const moves = await bitfinexRequest('v2/auth/r/movements/UST/hist', {
        limit: 20,
        start,
    });

    return moves;
}

export async function checkBitfinexMoves({
    amount,
    start,
    receiver,
    method = 'near',
}) {
    const moves = await getBitfinexMoves(start);

    // console.log(amount, start, receiver, method);
    // console.log('moves:', moves);

    const credited = moves.find(
        (m) =>
            m[1] === 'UST' && // currency
            m[2] === BITFINEX_METHODS[method] && // method
            // Number(m[5]) >= start && // received after start
            m[9] === 'COMPLETED' && // status
            Number(m[12]) >= usdtToSingle(amount) && // amount
            m[16] == receiver, // amount
    );

    return !!credited;
}

export async function withdrawToTron(amount) {
    const { address } = await getTronAddress();
    console.log('Withdrawing liquidity to Tron derived address', address);
    const body = {
        wallet: 'exchange',
        method: METHOD_TRON,
        address,
        amount: usdtToSingle(amount),
        travel_rule_tos: false,
    };

    const res = await bitfinexRequest('v2/auth/w/withdraw', body);
    const status = res[6];
    console.log(`Withdrawal request status: ${status}`);
    if (status !== 'SUCCESS') return false;
    return true;
}

export async function withdrawToNear(amount) {
    const { address } = await getNearAddress();
    const body = {
        wallet: 'exchange',
        method: METHOD_NEAR,
        address,
        amount: usdtToSingle(amount),
        travel_rule_tos: false,
    };

    const res = await bitfinexRequest('v2/auth/w/withdraw', body);
    const status = res[6];
    console.log(`Withdrawal request status: ${status}`);
    if (status !== 'SUCCESS') return false;
    return true;
}

// moves sample output:

/*
[
    [
        23672426,
        'UST',
        'TETHERUSX',
        null,
        null,
        1754347945000,
        1754348152000,
        null,
        null,
        'COMPLETED',
        null,
        null,
        -5,
        0,
        null,
        null,
        'TQ4Jo6cNH4hsdqKpkAYH4HZvj7ng1HcQm3',
        null,
        null,
        null,
        '933ae456e277b2c283302fab5ea73aab2bd2cb2fd67c8429a6bc06df73f15172',
        null,
    ],
    [
        23672417,
        'UST',
        'TETHERUSDTNEAR',
        null,
        null,
        1754347473000,
        1754347773000,
        null,
        null,
        'COMPLETED',
        null,
        null,
        55,
        0,
        null,
        null,
        '14e7dc7619fb07ad49044e92c99c55d5a352aebbc4be2a714f638a4fc561058f',
        null,
        null,
        null,
        'gHVFb8AYzGERsf7papcPNs1WHumMzDgJ95qe8BFQr7E',
        null,
    ],
    [
        23604418,
        'UST',
        'TETHERUSX',
        null,
        null,
        1751930687000,
        1751957933000,
        null,
        null,
        'COMPLETED',
        null,
        null,
        -5,
        0,
        null,
        null,
        'TXrv6zHfFuCvRetZcEq2k6f7SQ8LnsgD8X',
        null,
        null,
        null,
        'e9476cb74b930feba79c70c5d27e111496e5835b6d18316ddfeb5b73ebb4815a',
        null,
    ],
    [
        23586275,
        'UST',
        'TETHERUSDTNEAR',
        null,
        null,
        1751062473000,
        1751062773000,
        null,
        null,
        'COMPLETED',
        null,
        null,
        1,
        0,
        null,
        null,
        '14e7dc7619fb07ad49044e92c99c55d5a352aebbc4be2a714f638a4fc561058f',
        null,
        null,
        null,
        'AzYSaAjpbKzgKCzPBoZqR7RonKzhR12gN8qeoNknyHcM',
        null,
    ],
    [
        23572075,
        'UST',
        'TETHERUSDTSOL',
        null,
        null,
        1750451557000,
        1750451798000,
        null,
        null,
        'COMPLETED',
        null,
        null,
        -5,
        -0.5,
        null,
        null,
        '9Y7SQmMoMq7E9x3f4MMCYj1DWZXQspNHzEaqPW4MQAvp',
        null,
        null,
        null,
        '4XVfw9XdZu5h3j9GUS9fs7fcM5MZr56TTZPPT2jAJDyocg6w3NdKxuUR6Nf96qkR1qzaG2L9GHeCEy4v4TUaRs2M',
        null,
    ],
    [
        23572049,
        'UST',
        'TETHERUSDTNEAR',
        null,
        null,
        1750450984000,
        1750451313000,
        null,
        null,
        'COMPLETED',
        null,
        null,
        5,
        0,
        null,
        null,
        '14e7dc7619fb07ad49044e92c99c55d5a352aebbc4be2a714f638a4fc561058f',
        null,
        null,
        null,
        '3y2LVU6WZ9yhJU5DrdL9SGXTeKQQBJFTH9jb7ZLNFS4z',
        null,
    ],
    [
        23572021,
        'UST',
        'TETHERUSDTSOL',
        null,
        null,
        1750450181000,
        1750450495000,
        null,
        null,
        'COMPLETED',
        null,
        null,
        -5,
        -0.5,
        null,
        null,
        '9Y7SQmMoMq7E9x3f4MMCYj1DWZXQspNHzEaqPW4MQAvp',
        null,
        null,
        null,
        '5N3fuLgm8Rt3Y9khAjmdVjXu2kY2rUPy1d8QuTucrPr9YzASNpiXfqtf5b9SvGenhY1mVxGZn7Gap9LGWiyBH4Wp',
        null,
    ],
    [
        23571981,
        'UST',
        'TETHERUSDTNEAR',
        null,
        null,
        1750449124000,
        1750449784000,
        null,
        null,
        'COMPLETED',
        null,
        null,
        5,
        0,
        null,
        null,
        '14e7dc7619fb07ad49044e92c99c55d5a352aebbc4be2a714f638a4fc561058f',
        null,
        null,
        null,
        '5DbyEpfcfmAXQky7nDsWBv4Jat561HYXMzgRmJctqNHk',
        null,
    ],
    [
        23571962,
        'UST',
        'TETHERUSDTNEAR',
        null,
        null,
        1750448283000,
        1750448584000,
        null,
        null,
        'COMPLETED',
        null,
        null,
        5,
        0,
        null,
        null,
        '14e7dc7619fb07ad49044e92c99c55d5a352aebbc4be2a714f638a4fc561058f',
        null,
        null,
        null,
        'HEuFD3VXHtSEASSbYqUqD7WPLHHcR7vjLJdPqHfyqCwQ',
        null,
    ],
];

*/
