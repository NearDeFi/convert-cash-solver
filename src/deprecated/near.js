import * as dotenv from 'dotenv';
dotenv.config({ path: './.env.development.local' });

// const and helpers
import { verify, createHash, createPublicKey } from 'crypto';
import { parseSeedPhrase } from 'near-seed-phrase';
import {
    createTransaction,
    actionCreators,
    Signature,
    SignedTransaction,
    SCHEMA,
} from '@near-js/transactions';
import { JsonRpcProvider } from '@near-js/providers';
import { KeyPairSigner } from '@near-js/signers';
import { Account } from '@near-js/accounts';
import { NEAR } from '@near-js/tokens';
import { KeyPair, PublicKey } from '@near-js/crypto';
import { baseDecode } from '@near-js/utils';
import { serialize } from 'borsh';

const GAS = BigInt('300000000000000');
const CHAINSIG_PATH = 'pool-1'; // NEAR derived address path

const contractId = process.env.NEAR_CONTRACT_ID?.replaceAll('"', '');
const networkId = /testnet/gi.test(contractId) ? 'testnet' : 'mainnet';
let accountId, signer, keyPair;
const { NEAR_ACCOUNT_ID, NEAR_SEED_PHRASE } = process.env;
// if we're running within the API image and we have ENV vars for NEAR_ACCOUNT_ID and NEAR_SEED_PRASE
if (NEAR_ACCOUNT_ID && NEAR_SEED_PHRASE) {
    accountId = NEAR_ACCOUNT_ID.replaceAll('"', '');
    const { secretKey } = parseSeedPhrase(NEAR_SEED_PHRASE.replaceAll('"', ''));
    keyPair = KeyPair.fromString(secretKey);
    signer = new KeyPairSigner(keyPair);
}
const provider = new JsonRpcProvider({
    url:
        networkId === 'testnet'
            ? 'https://test.rpc.fastnear.com'
            : 'https://free.rpc.fastnear.com',
});
//helpers
export const parseNearAmount = (amt) => NEAR.toUnits(amt);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
export const getAccount = (id = accountId) => new Account(id, provider, signer);
export const viewFunction = ({ contractId, methodName, args }) =>
    provider.callFunction(contractId, methodName, args);

export async function signAndVerifyNEAR() {
    const { publicKey: nearPublicKey } = await getNearAddress();

    const publicKey = createPublicKey({
        format: 'der',
        type: 'spki',
        key: Buffer.concat([
            Buffer.from('302a300506032b6570032100', 'hex'), // Ed25519 DER prefix
            Buffer.from(baseDecode(nearPublicKey.split(':')[1])), // Your 32-byte public key
        ]),
    });

    const payload =
        '74ce137697637a6181681d3210f66fbe6516a4c4d1234471e38986a1d2ae77e5'; // dummy payload
    const sigRes = await agentCall({
        methodName: 'request_signature',
        args: { path: CHAINSIG_PATH, payload, key_type: 'Eddsa' },
    });

    const valid = verify(
        null,
        Buffer.from(payload, 'hex'),
        publicKey,
        Buffer.from(sigRes.signature),
    );
    console.log('NEAR signature valid:', valid);
    return valid;
}

export async function getNearAddress() {
    // TODO make env vars for path and predecessor
    const derivedPublicKey = await viewFunction({
        contractId: 'v1.signer',
        methodName: 'derived_public_key',
        args: {
            path: CHAINSIG_PATH,
            predecessor: 'ac-proxy.shadeagent.near',
            domain_id: 1,
        },
    });

    const accountId = Buffer.from(baseDecode(derivedPublicKey.split(':')[1]))
        .toString('hex')
        .toLowerCase();

    return { address: accountId, publicKey: derivedPublicKey };
}

export async function requestLiquidityUnsigned({ to, amount }) {
    const { address, publicKey } = await getNearAddress();

    // USDT contract and FT transfer details
    const usdtContract = 'usdt.tether-token.near'; // mainnet USDT contract
    const ftArgs = {
        receiver_id: to,
        amount,
        memo: 'Bitfinex deposit',
    };
    const ftGas = '30000000000000'; // 30 Tgas
    const ftDeposit = '1'; // 1 yoctoNEAR required for ft_transfer
    const accessKey = await provider.query(
        `access_key/${address}/${publicKey}`,
        '',
    );
    const recentBlockHash = baseDecode(accessKey.block_hash);
    const actions = [
        actionCreators.functionCall('ft_transfer', ftArgs, ftGas, ftDeposit),
    ];
    const transaction = createTransaction(
        address,
        PublicKey.from(publicKey),
        usdtContract,
        ++accessKey.nonce,
        actions,
        recentBlockHash,
    );

    const serializedTx = Buffer.from(
        serialize(SCHEMA.Transaction, transaction),
    );

    const payload = createHash('sha256')
        .update(serializedTx)
        .digest()
        .toString('hex');

    return {
        payload,
        transaction,
    };
}

export async function requestLiquidityBroadcast({ transaction, signature }) {
    const signatureBytes = Buffer.from(signature);

    const signedTransaction = new SignedTransaction({
        transaction,
        signature: new Signature({
            keyType: 0,
            data: signatureBytes,
        }),
    });

    const result = await provider.sendTransaction(signedTransaction);

    console.log(
        `Explorer: https://nearblocks.io/txns/${result.transaction.hash}`,
    );

    return {
        txHash: result.transaction.hash,
        explorerLink: `Explorer: https://nearblocks.io/txns/${result.transaction.hash}`,
    };
}

export const contractView = async ({
    contractId = _contractId,
    methodName,
    args = {},
}) => {
    let res;
    try {
        res = await provider.callFunction(contractId, methodName, args);
    } catch (e) {
        console.log('contractView error:', e);
    }
    return res;
};

export const contractCall = async ({
    accountId = _accountId,
    contractId = _contractId,
    methodName,
    args,
    gas = GAS,
    deposit = BigInt('0'),
}) => {
    const account = getAccount(accountId);

    let res;
    try {
        res = await account.callFunction({
            contractId,
            methodName,
            args,
            gas,
            deposit,
            waitUntil: 'EXECUTED',
        });
    } catch (e) {
        console.log(e);
        if (/deserialize/gi.test(JSON.stringify(e))) {
            return console.log(`Bad arguments to ${methodName} method`);
        }
        throw e;
    }
    return res;
};
