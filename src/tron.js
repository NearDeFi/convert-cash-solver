import { TronWeb } from 'tronweb';
import keccak from 'keccak';
import { viewFunction } from './near.js';
import { baseDecode } from '@near-js/utils';

const tronWeb = new TronWeb({
    fullHost: 'https://api.trongrid.io',
});

export async function verifySignature(txHash, signatureHex) {
    const txHashHex = '0x' + txHash.toString('hex');
    const signature = '0x' + signatureHex;

    const recoveredEthAddress = ethers.recoverAddress(txHashHex, signature);
    const tronAddressFromRecovered = tronWeb.address.fromHex(
        '41' + recoveredEthAddress.slice(2),
    );
    console.log('tronAddressFromRecovered', tronAddressFromRecovered);
}

export function constructTronSignature({ big_r, s, recovery_id }) {
    const rHex = big_r.affine_point.slice(2); // Drop the prefix byte (02 or 03)
    const sHex = s.scalar.padStart(64, '0'); // Ensure it's 32 bytes (64 hex chars)
    const vHex = recovery_id.toString(16).padStart(2, '0'); // 1 byte

    const signature = rHex + sHex + vHex;
    return signature.toLowerCase(); // Tron expects lowercase hex
}

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
        .update(Buffer.from(publicKey))
        .digest()
        .slice(-20);
    const tronAddressBytes = Buffer.concat([Buffer.from([0x41]), addressBytes]);
    const address = tronWeb.address.fromHex(tronAddressBytes.toString('hex'));

    return { address, publicKey };
}

export async function tronSendUnsigned({
    to = 'TQ4Jo6cNH4hsdqKpkAYH4HZvj7ng1HcQm3',
    from = 'TQ4Jo6cNH4hsdqKpkAYH4HZvj7ng1HcQm3',
    amount = 1, // in TRON
}) {
    const unsignedTx = await tronWeb.transactionBuilder.sendTrx(
        to,
        parseInt(amount) * 1000000,
        from,
    );

    return { txHash: unsignedTx.txID, rawTransaction: unsignedTx };
}

export async function tronUSDTUnsigned({
    to = 'TQ4Jo6cNH4hsdqKpkAYH4HZvj7ng1HcQm3',
    from = 'TQ4Jo6cNH4hsdqKpkAYH4HZvj7ng1HcQm3',
    amount = 1, // in USDT
    feeLimit = 30, // in TRON
}) {
    const usdtContract = 'TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t';

    const parameters = [
        { type: 'address', value: to },
        { type: 'uint256', value: tronWeb.toHex(parseInt(amount) * 1000000) },
    ];

    // Generate unsigned transaction
    const unsignedTx = await tronWeb.transactionBuilder.triggerSmartContract(
        usdtContract,
        'transfer(address,uint256)',
        { feeLimit: parseInt(feeLimit) * 1000000 },
        parameters,
        tronWeb.address.toHex(from),
    );

    return {
        txHash: unsignedTx.transaction.txID,
        rawTransaction: unsignedTx.transaction,
    };
}

export async function tronBroadcastTx(unsignedTx, manualSignature) {
    unsignedTx.signature = [manualSignature];
    return tronWeb.trx.sendRawTransaction(unsignedTx);
}
