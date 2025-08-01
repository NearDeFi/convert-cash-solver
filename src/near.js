import * as dotenv from 'dotenv';
dotenv.config({ path: './.env.development.local' });

// const and helpers
import { createHash } from 'crypto';
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

export async function getNearAddress() {
    // TODO make env vars for path and predecessor
    const derivedPublicKey = await viewFunction({
        contractId: 'v1.signer',
        methodName: 'derived_public_key',
        args: {
            path: 'pool-1',
            predecessor: 'ac-proxy.shadeagent.near',
            domain_id: 1,
        },
    });

    const accountId = Buffer.from(baseDecode(derivedPublicKey.split(':')[1]))
        .toString('hex')
        .toLowerCase();

    return accountId;
}

export async function requestLiquidityUnsigned({ to, amount }) {
    const accountId = await getNearAddress();

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
        `access_key/${accountId}/${derivedPublicKey}`,
        '',
    );
    const recentBlockHash = baseDecode(accessKey.block_hash);
    const actions = [
        actionCreators.functionCall('ft_transfer', ftArgs, ftGas, ftDeposit),
    ];
    const transaction = createTransaction(
        accountId,
        PublicKey.from(derivedPublicKey),
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
        explorerLink: `Explorer: https://nearblocks.io/txns/${result.transaction.hash}`,
    };
}
