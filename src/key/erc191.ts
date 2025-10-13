import { ethers } from 'ethers';
import bs58 from 'bs58';

import { contractView, contractCall } from '../deprecated/near.js';

// --- types -----------------------------------------------------------------

interface GeneratedKey {
    erc191PublicKey: string;
}

// --- module state ----------------------------------------------------------

let erc191PrivateKey: string | null = null;
let erc191PublicKey: string | null = null;

// --- functions -------------------------------------------------------------

export function generateErc191Key(): GeneratedKey {
    // Generate a random wallet
    const randomWallet = ethers.Wallet.createRandom();

    erc191PrivateKey = randomWallet.privateKey;

    const uncompressedPubKey = randomWallet.signingKey.publicKey;
    const uncompressedBuffer = Buffer.from(uncompressedPubKey.slice(2), 'hex'); // Remove '0x'
    const uncompressedWithoutPrefix = uncompressedBuffer.slice(1); // Remove first byte (0x04)
    erc191PublicKey = 'secp256k1:' + bs58.encode(uncompressedWithoutPrefix);

    return { erc191PublicKey };
}

export async function erc191SignMessage(message: string): Promise<string> {
    if (!erc191PrivateKey) {
        throw new Error(
            'ERC191 private key not initialized. Call generateErc191Key() first.',
        );
    }
    const wallet = new ethers.Wallet(erc191PrivateKey);
    return wallet.signMessage(message);
}

// helper for local testing

export async function addErc191Key(): Promise<void> {
    const publicKeys = (await contractView({
        contractId: 'intents.near',
        methodName: 'public_keys_of',
        args: { account_id: process.env.NEAR_CONTRACT_ID },
    })) as string[] | undefined;
    console.log('publicKeys', publicKeys);

    if (publicKeys && Array.isArray(publicKeys)) {
        for (const public_key of publicKeys) {
            console.log('removing public key:', public_key);
            try {
                const removePublicKeyRes = await contractCall({
                    accountId: process.env.NEAR_CONTRACT_ID!,
                    contractId: 'intents.near',
                    methodName: 'remove_public_key',
                    args: { public_key },
                    deposit: BigInt('1'),
                });
                console.log('removePublicKeyRes:', removePublicKeyRes === '');
            } catch (e) {
                console.log('Error removePublicKeyRes:', e);
            }
        }
    }

    const { erc191PublicKey } = generateErc191Key();
    console.log('erc191PublicKey', erc191PublicKey);

    try {
        const addPublicKeyRes = await contractCall({
            accountId: process.env.NEAR_CONTRACT_ID!,
            contractId: 'intents.near',
            methodName: 'add_public_key',
            args: { public_key: erc191PublicKey },
            deposit: BigInt('1'),
        });
        console.log('addPublicKeyRes:', addPublicKeyRes === '');
    } catch (e) {
        console.log('Error adding public key:', e);
    }
}
