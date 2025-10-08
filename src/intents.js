import { ethers } from 'ethers';
import { randomBytes } from 'crypto';
import { baseDecode, baseEncode } from '@near-js/utils';
import { callWithAgent } from './app.js';
import { KeyPair } from '@near-js/crypto';
import crypto from 'crypto';
import { parseSeedPhrase } from 'near-seed-phrase';
const { NEAR_CONTRACT_ID, NEAR_SEED_PHRASE } = process.env;

const nearIntentsFetch = async (method, params) => {
    try {
        const res = await fetch('https://bridge.chaindefuser.com/rpc', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                jsonrpc: '2.0',
                id: 1,
                method,
                params: [params],
            }),
        }).then((r) => r.json());

        return res.result;
    } catch (e) {
        console.error(
            `Error calling near intents method ${method} with params ${params}. Error:`,
            e,
        );
        return null;
    }
};

export async function erc191Verify(message) {
    // 2. Recover the address from the signed sample
    const recoveredAddress = ethers.verifyMessage(
        message.payload,
        '0x' +
            Buffer.from(
                baseDecode(message.signature.replace('secp256k1:', '')),
            ).toString('hex'),
    );

    const payload = JSON.parse(message.payload);
    console.log('erc191Verify signerId:', payload.signer_id.toLowerCase());
    console.log(
        'erc191Verify recovered address:',
        recoveredAddress.toLowerCase(),
    );

    return {
        recoveredAddress,
        validSignature:
            payload.signer_id.toLowerCase() === recoveredAddress.toLowerCase(),
    };
}

export async function getDepositAddress(account_id, chain) {
    console.log(account_id, chain);

    const { address } = await nearIntentsFetch('deposit_address', {
        account_id,
        chain,
    });
    return { depositAddress: address, chain };
}

export async function getRecentDeposits(account_id, chain) {
    const { deposits } = await nearIntentsFetch('recent_deposits', {
        account_id,
        chain,
    });
    return { deposits, chain };
}

const srcTokenAddress = '0xdac17f958d2ee523a2206206994597c13d831ec7';
export function getIntentDiffDetails(tokenDiffIntent) {
    const [srcToken, srcAmount] = Object.entries(tokenDiffIntent.diff).find(
        ([k]) => k.indexOf(srcTokenAddress) > -1,
    );
    const [destToken, destAmount] = Object.entries(tokenDiffIntent.diff).find(
        ([k]) => k.indexOf(srcTokenAddress) === -1,
    );

    return {
        srcToken,
        srcAmount,
        destToken,
        destAmount,
    };
}

export async function createSignedErc191Intent(address, intents) {
    const standard = 'erc191';
    const nonce = Buffer.from(randomBytes(32)).toString('base64');
    const deadline = new Date(Date.now() + 10 * 60 * 1000).toISOString(); // 10 minutes from now
    const verifying_contract = 'intents.near';

    const payload = JSON.stringify({
        signer_id: address,
        nonce,
        verifying_contract,
        deadline,
        intents,
    });

    // sign payload with evm key using chain signatures
    const payloadBuffer = Buffer.from(payload);
    const prefixBuffer = Buffer.from('\x19Ethereum Signed Message:\n');
    const lengthBuffer = Buffer.from(payloadBuffer.length.toString());

    const payloadHex = ethers.keccak256(
        Buffer.concat([prefixBuffer, lengthBuffer, payloadBuffer]),
    );

    const sigRes = await callWithAgent({
        methodName: 'request_signature',
        args: {
            path: 'evm-1',
            payload: payloadHex.substring(2),
            key_type: 'Ecdsa',
        },
    });

    // parse signature response
    const r = Buffer.from(sigRes.big_r.affine_point.substring(2), 'hex');
    const s = Buffer.from(sigRes.s.scalar, 'hex');
    const v = sigRes.recovery_id;
    const rsvSignature = new Uint8Array(65);
    rsvSignature.set(r, 0);
    rsvSignature.set(s, 32);
    rsvSignature[64] = v;
    const signature = 'secp256k1:' + baseEncode(rsvSignature);

    // final intent
    const nearIntent = {
        standard,
        payload,
        signature,
    };

    return nearIntent;
}

export async function createLocallySignedErc191Intent(privateKey, intents) {
    const wallet = new ethers.Wallet(privateKey);

    const standard = 'erc191';
    const nonce = Buffer.from(randomBytes(32)).toString('base64');
    const deadline = new Date(Date.now() + 3600 * 24 * 60 * 1000).toISOString(); // 24 hours from now
    const verifying_contract = 'intents.near';

    const payload = JSON.stringify({
        signer_id: process.env.NEAR_CONTRACT_ID,
        nonce,
        verifying_contract,
        deadline,
        intents,
    });
    const hexSignature = await wallet.signMessage(payload);
    const hash = ethers.hashMessage(payload);
    const recoveredAddress = ethers.recoverAddress(hash, hexSignature);
    //0xdB8e334b75A3368ED02d9f8380D31667B524619c
    console.log('Addresses', recoveredAddress, wallet.address);

    const signatureBuffer = Buffer.from(hexSignature.slice(2), 'hex');
    console.log('v', signatureBuffer[64]);
    signatureBuffer.set([signatureBuffer[64] - 27], 64);
    const signature = 'secp256k1:' + baseEncode(signatureBuffer);

    // final intent
    const nearIntent = {
        standard,
        payload,
        signature,
    };

    console.log('createLocallySignedErc191Intent:', nearIntent);

    return nearIntent;
}

import { BorshSchema, borshSerialize } from 'borsher';

const nep413PayloadSchema = BorshSchema.Struct({
    message: BorshSchema.String,
    nonce: BorshSchema.Array(BorshSchema.u8, 32),
    recipient: BorshSchema.String,
    callback_url: BorshSchema.Option(BorshSchema.String),
});
export async function createLocallySignedNep413Intent(intents) {
    const { secretKey } = parseSeedPhrase(NEAR_SEED_PHRASE.replaceAll('"', ''));
    const keyPair = KeyPair.fromString(secretKey);
    const public_key = keyPair.getPublicKey().toString();

    const standard = 'nep413';
    const nonce = Buffer.from(randomBytes(32));
    const deadline = new Date(Date.now() + 10 * 60 * 1000).toISOString(); // 10 minutes from now
    const verifying_contract = 'intents.near';

    const message = JSON.stringify({
        signer_id: NEAR_CONTRACT_ID,
        deadline,
        intents,
    });

    const payload = {
        message,
        nonce,
        recipient: 'intents.near',
    };

    const schema = {
        struct: {
            message: 'string',
            nonce: 'string',
            recipient: 'string',
        },
    };

    const payloadToSign = new Uint8Array(
        crypto
            .createHash('sha256')
            .update(
                Buffer.concat([
                    borshSerialize(BorshSchema.u32, 2147484061),
                    borshSerialize(nep413PayloadSchema, payload),
                ]),
            )
            .digest(),
    );

    const { publicKey, signature } = await keyPair.sign(payloadToSign);

    payload.nonce = payload.nonce.toString('base64');

    // final intent
    const nearIntent = {
        standard,
        payload,
        public_key,
        signature: 'ed25519:' + baseEncode(signature),
    };

    console.log('createLocallySignedNep413Intent:', nearIntent);

    return nearIntent;
}

/**
 *
 * Testing
 *
 */

// Example private key for demonstration only. DO NOT use this in production.
const privateKey =
    '0x4c0883a69102937d623414e1c5d53d5b4ae7f07bba82738eefa1c862ada6df25';
const wallet = new ethers.Wallet(privateKey);

const sample = {
    standard: 'erc191',
    payload: `{"signer_id": "${wallet.address}", "verifying_contract": "intents.near", "deadline": "2025-05-26T13:24:16.983Z", "nonce": "U3UMmW79FqTMtBx3DYLI2DUxxwAFY+Eo4kY11PEI3PU=", "intents": [{ "intent": "token_diff", "diff": { "nep141:usdc.near": "-1000", "nep141:usdt.near": "1000" } }, { "intent": "ft_withdraw", "token": "usdt.near", "receiver_id": "bob.near", "amount": "1000" }]}`,
    signature:
        'secp256k1:4jpo5EuztCFUe3gVqWpmwowoorFUmt4ynu3Z8WPo9zw2BSoHB279PZtDz934L1uCi6VfgXYJdTEfRaxyM3a1zaUw1',
};
// Sign a message using EIP-191 ('personal_sign'/default for signMessage)
export async function testSignAndRecover() {
    // 1. Sign the sample
    const signature = await wallet.signMessage(sample.payload);
    sample.signature = 'secp256k1:' + signature;

    // 2. Recover the address from the signed sample
    const recoveredAddress = ethers.verifyMessage(
        sample.payload,
        sample.signature.replace('secp256k1:', ''),
    );

    const json = JSON.parse(sample.payload);

    console.log('Recovered address:', recoveredAddress);
    console.log(json.signer_id === recoveredAddress ? 'Match' : 'No match');
}
