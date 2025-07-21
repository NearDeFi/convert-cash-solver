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

const BITFINEX_METHODS = {
    near: 'TETHERUSDTNEAR',
    tron: 'TETHERUSX',
};

const BITFINEX_KEY = process.env.BITFINEX_KEY;
const BITFINEX_SECRET = process.env.BITFINEX_SECRET;

// --- helpers ---------------------------------------------------------------

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

export async function checkBitfinexMoves({
    amount,
    start,
    receiver,
    method = 'near',
    timeoutMin = 60,
    pollFreq = 30000,
}) {
    const started = Date.now();

    while (Date.now() - started < timeoutMin * 60_000) {
        console.log(
            `Checking ${amount} was sent to ${receiver} after ${new Date(
                start,
            )}`,
        );
        // Bitfinex’s code for Tether is UST
        const moves = await bitfinexRequest('v2/auth/r/movements/UST/hist', {
            limit: 20,
            start,
        });

        const credited = moves.find(
            (m) =>
                m[1] === 'UST' && // currency
                m[2] === BITFINEX_METHODS[method] && // method
                // Number(m[5]) >= start && // received after start
                m[9] === 'COMPLETED' && // status
                Number(m[12]) >= amount && // amount
                m[16] == receiver, // amount
        );

        if (credited) return true; // deposit/withdrawal confirmed
        await new Promise((r) => setTimeout(r, pollFreq));
    }

    return false;
}

export async function withdrawToTron(amount) {
    const body = {
        wallet: 'exchange',
        method: METHOD_TRON,
        address: 'TXrv6zHfFuCvRetZcEq2k6f7SQ8LnsgD8X',
        amount: amount.toFixed(2),
        travel_rule_tos: false,
    };

    const res = await bitfinexRequest('v2/auth/w/withdraw', body);
    const status = res[6];
    console.log(res);
    console.log(`Withdrawal request status: ${status}`);
    if (status !== 'SUCCESS') throw new Error('Withdrawal failed');
}
