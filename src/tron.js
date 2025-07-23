import { TronWeb } from 'tronweb';
import keccak from 'keccak';
import { createHash } from 'crypto';
import { viewFunction } from './near.js';
import { baseEncode, baseDecode } from '@near-js/utils';
import base58 from 'bs58';

const tronWeb = new TronWeb({
    fullHost: 'https://api.trongrid.io',
});

// // Initialize TronWeb (no private key needed for unsigned tx)
// const tronWeb = new TronWeb({
//     fullHost: 'https://api.trongrid.io',
// });

export async function getTronAddress() {
    const derivedPublicKey = await viewFunction({
        contractId: 'v1.signer',
        methodName: 'derived_public_key',
        args: {
            path: 'tron-1',
            predecessor: 'ac-proxy.shadeagent.near',
            domain_id: 0,
        },
    });

    const publicKey = baseDecode(derivedPublicKey.split(':')[1]);

    const addressBytes = keccak('keccak256')
        .update(Buffer.concat([Buffer.from([0x04]), publicKey]))
        .digest()
        .slice(-20);

    const tronAddressBytes = Buffer.concat([Buffer.from([0x41]), addressBytes]);

    const address = tronWeb.address.fromHex(tronAddressBytes.toString('hex'));

    return { address, publicKey };
}

export async function tronUSDTUnsigned(address) {
    const usdtContract = 'TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t';

    const parameters = [
        { type: 'address', value: address },
        { type: 'uint256', value: tronWeb.toHex(1000000) },
    ];

    // Generate unsigned transaction
    const unsignedTx = await tronWeb.transactionBuilder.triggerSmartContract(
        usdtContract,
        'transfer(address,uint256)',
        { feeLimit: 50_000_000 }, // 50 TRX fee limit
        parameters,
        tronWeb.address.toHex(address),
    );

    const txHash = unsignedTx.transaction.txID;
    const rawTransaction = unsignedTx.transaction;

    return { txHash, rawTransaction };
}

// async function broadcastSignedTx(unsignedTx, manualSignature) {
//     try {
//         // 1. Add manual signature to transaction
//         const signedTx = {
//             ...unsignedTx,
//             signature: [manualSignature], // Add hex signature string
//         };

//         // 2. Broadcast to TRON network
//         const result = await tronWeb.trx.sendRawTransaction(signedTx);

//         console.log('Broadcast Result:', result);
//         return result;
//     } catch (error) {
//         console.error('Broadcast failed:', error);
//     }
// }

// // Example usage:
// const unsignedTx = /* ... from previous step */;
// const manualSignature = 'c1dddbc3812ad0b93d...'; // 64-byte hex string
// broadcastSignedTx(unsignedTx, manualSignature);
