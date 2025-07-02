/**
 * near-to-solana-bitfinex.js
 *
 * Deposit USDT (NEAR) → Bitfinex → Withdraw USDT (Solana)
 */

import fetch from 'node-fetch'; // v2 – CommonJS compatible
import crypto from 'crypto';
import nearAPI from 'near-api-js';
import { KeyPair } from 'near-api-js';
import dotenv from 'dotenv';
dotenv.config(); // loads .env

// --- constants -------------------------------------------------------------

const NEAR_ENDPOINT = 'https://rpc.mainnet.near.org';
const USDT_CONTRACT = 'usdt.tether-token.near'; // official NEAR USD₮ contract
const USDT_DECIMALS = 6; // NEAR FT has 6 dp
const GAS = '300000000000000'; // 300 Tgas
const YOCTO_NEAR = '1'; // min deposit for FT calls

const BITFINEX_REST = 'https://api.bitfinex.com';
const METHOD_NEAR = 'tetherusdtnear'; // NEAR transport string
const METHOD_SOL = 'tetherusdtsol'; // Solana transport string

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
        .createHmac('sha384', process.env.BITFINEX_SECRET)
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
            'bfx-apikey': process.env.BITFINEX_KEY,
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

// --- step 1 : fetch deposit address ---------------------------------------

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

// --- step 2 : send USDT on NEAR -------------------------------------------

export async function sendUsdtNear(receiver, humanAmount) {
    const keyStore = new nearAPI.keyStores.InMemoryKeyStore();
    const keyPair = KeyPair.fromString(process.env.FUNDING_NEAR_SECRET_KEY);
    await keyStore.setKey('mainnet', process.env.FUNDING_NEAR_ADDRESS, keyPair);

    const near = await nearAPI.connect({
        networkId: 'mainnet',
        nodeUrl: NEAR_ENDPOINT,
        deps: { keyStore },
    });

    const account = await near.account(process.env.FUNDING_NEAR_ADDRESS);

    await account.functionCall({
        contractId: USDT_CONTRACT,
        methodName: 'ft_transfer',
        args: {
            receiver_id: receiver,
            amount: toSubunits(humanAmount),
            memo: 'Bitfinex deposit',
        },
        gas: GAS,
        attachedDeposit: YOCTO_NEAR,
    });
}

// --- step 3 : wait for credit ----------------------------------------------

export async function waitForBitfinexCredit(
    targetAmount,
    sendTimeMs,
    timeoutMin = 60,
) {
    const started = Date.now();

    while (Date.now() - started < timeoutMin * 60_000) {
        console.log('checking for credit…');
        // Bitfinex’s code for Tether is UST
        const moves = await bitfinexRequest('v2/auth/r/movements/UST/hist', {
            limit: 20,
        });

        const credited = moves.find(
            (m) =>
                /* currency  */ m[1] === 'UST' &&
                /* method    */ m[2] === 'TETHERUSDTNEAR' &&
                Number(m[5]) >= sendTimeMs &&
                /* status    */ m[9] === 'COMPLETED' &&
                /* amount    */ Number(m[12]) >= targetAmount,
        );

        if (credited) return true; // deposit confirmed → continue script
        await new Promise((r) => setTimeout(r, 20_000)); // otherwise wait 20 s and poll again
    }

    return false;
}

// // --- step 4 : withdraw to Solana -----------------------------------------

// async function withdrawToSolana(amount) {
//     const body = {
//         wallet: 'exchange',
//         method: METHOD_SOL,
//         address: process.env.SOLANA_DEST,
//         amount: amount.toFixed(2),
//         travel_rule_tos: false, // set real fields if required (VASPs, etc.)
//     };

//     const res = await bitfinexRequest('v2/auth/w/withdraw', body);
//     const status = res[6];
//     console.log(res);
//     console.log(`Withdrawal request status: ${status}`);
//     if (status !== 'SUCCESS') throw new Error('Withdrawal failed');
// }

// // -------------------------------------------------------------------------

// async function main() {
//     const human = Number(process.argv[2] || process.env.AMOUNT);
//     if (!human || human <= 0) throw new Error('Provide a positive USDT amount');

//     console.log(`Moving ${human} USDT from NEAR → Bitfinex → Solana…`);

//     const depositAddr = await getNearDepositAddress();
//     console.log('Bitfinex NEAR deposit address:', depositAddr);

//     //   await sendUsdtNear(depositAddr, human);
//     //   console.log('✅ NEAR transfer broadcast; waiting for credit…');

//     await waitForBitfinexCredit(human);
//     console.log('✅ Deposit credited on Bitfinex.');

//     await withdrawToSolana(human);
//     console.log('🎉 Withdrawal submitted. Monitor on Solana explorer.');
// }
