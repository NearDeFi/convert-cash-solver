import { ethers } from 'ethers';
import { requestSignature } from '@neardefi/shade-agent-js';
import { viewFunction } from './near.js';
import { callWithAgent } from './app.js';
import keccak from 'keccak';
import { baseDecode } from '@near-js/utils';

export const explorerBase = 'https://etherscan.io';
export const provider = new ethers.JsonRpcProvider(
    'https://gateway.tenderly.co/public/mainnet',
);
export const CHAINSIG_PATH = `evm-1`;
export const USDT_ADDRESS = `0xdAC17F958D2ee523a2206206994597C13D831ec7`;

export async function getEvmAddress() {
    const derivedPublicKey = await viewFunction({
        contractId: 'v1.signer',
        methodName: 'derived_public_key',
        args: {
            path: CHAINSIG_PATH,
            predecessor: 'ac-proxy.shadeagent.near',
            domain_id: 0,
        },
    });

    const publicKey = baseDecode(derivedPublicKey.split(':')[1]);
    const addressBytes = keccak('keccak256')
        .update(Buffer.from(publicKey))
        .digest()
        .slice(-20);
    const address = '0x' + addressBytes.toString('hex');

    return { address, publicKey };
}

export async function verifySignature(txHash, signatureHex) {
    const txHashHex = '0x' + txHash.toString('hex');
    const signature = '0x' + signatureHex;

    const recoveredEthAddress = ethers
        .recoverAddress(txHashHex, signature)
        .toLowerCase();
    return recoveredEthAddress;
}

export async function signAndVerifyEVM() {
    const { address: evmAddress } = await getEvmAddress();
    const payload =
        '74ce137697637a6181681d3210f66fbe6516a4c4d1234471e38986a1d2ae77e5'; // dummy payload
    const sigRes = await callWithAgent({
        methodName: 'request_signature',
        args: { path: CHAINSIG_PATH, payload, key_type: 'Ecdsa' },
    });
    const sig = parseSignature({ sigRes });
    const sigHex =
        sig.r.substring(2) +
        sig.s.substring(2) +
        sig.v.toString(16).padStart(2, '0');
    const recoveredAddress = await verifySignature(payload, sigHex);

    const valid = recoveredAddress.toLowerCase() === evmAddress.toLowerCase();
    console.log('EVM signature valid:', valid);
    return valid;
}

export async function sendEVMTokens({
    tokenAddress = USDT_ADDRESS,
    receiver = '0x525521d79134822a342d330bd91da67976569af1',
    amount = ethers.parseUnits('1.0', 6),
    chainId = 1,
}) {
    const { address: sender } = await getEvmAddress();

    const { unsignedTx, txHash } = await erc20UnsignedTx({
        tokenAddress,
        sender,
        receiver,
        amount,
        chainId,
    });

    const sigRes = await callWithAgent({
        methodName: 'request_signature',
        args: { path: CHAINSIG_PATH, payload: txHash, key_type: 'Ecdsa' },
    });

    const sig = (unsignedTx.signature = parseSignature({ sigRes }));
    const sigHex =
        sig.r.substring(2) +
        sig.s.substring(2) +
        sig.v.toString(16).padStart(2, '0');
    const recoveredAddress = await verifySignature(txHash, sigHex);

    if (recoveredAddress.toLowerCase() !== sender.toLowerCase()) {
        throw new Error(
            'Address mismatch, recoveredAddress: ' + recoveredAddress,
        );
    }

    const res = await broadcastTransaction(unsignedTx.serialized);
    return res;
}

export async function sendETH({
    path,
    sender,
    receiver = '0x525521d79134822a342d330bd91da67976569af1',
    amount = ethers.parseUnits('1.0', 6),
    chainId = 1,
}) {
    const { unsignedTx, payload } = await ethUnsignedTx({
        sender,
        receiver,
        amount,
        chainId,
    });

    const sigRes = await requestSignature({ path, payload });
    unsignedTx.signature = parseSignature({ sigRes });
    const res = await broadcastTransaction(unsignedTx.serialized);
    return res;
}

export async function ethUnsignedTx({ sender, receiver, amount, chainId }) {
    // Get live network data
    const [nonce, feeData] = await Promise.all([
        provider.getTransactionCount(sender, 'latest'),
        provider.getFeeData(), // Gets current EIP-1559 gas values[1][4]
    ]);
    const gasPrice =
        (feeData.maxFeePerGas + feeData.maxPriorityFeePerGas) * BigInt('21000');
    const finalAmount = amount - gasPrice;

    const unsignedTx = ethers.Transaction.from({
        type: 2, // EIP-1559 transaction
        chainId: chainId,
        to: receiver,
        nonce, // Replace with actual sender nonce
        maxPriorityFeePerGas: feeData.maxPriorityFeePerGas,
        maxFeePerGas: feeData.maxFeePerGas,
        gasLimit: 21000, // Estimate gas
        value: finalAmount,
    });

    const txHash = ethers
        .keccak256(ethers.getBytes(unsignedTx.unsignedSerialized))
        .substring(2);

    return { unsignedTx, txHash };
}

export async function erc20UnsignedTx({
    tokenAddress,
    sender,
    receiver,
    amount,
    chainId,
}) {
    const erc20Interface = new ethers.Interface([
        'function transfer(address to, uint256 value) returns (bool)',
    ]);

    // Get live network data
    const [nonce, feeData] = await Promise.all([
        provider.getTransactionCount(sender, 'latest'),
        provider.getFeeData(), // Gets current EIP-1559 gas values[1][4]
    ]);

    const unsignedTx = ethers.Transaction.from({
        type: 2, // EIP-1559 transaction
        chainId: chainId,
        to: ethers.getAddress(tokenAddress),
        data: erc20Interface.encodeFunctionData('transfer', [receiver, amount]),
        nonce, // Replace with actual sender nonce
        maxPriorityFeePerGas: feeData.maxPriorityFeePerGas,
        maxFeePerGas: feeData.maxFeePerGas,
        gasLimit: 100000, // Estimate gas
        value: 0, // Zero for token transfers
    });

    const txHash = ethers
        .keccak256(ethers.getBytes(unsignedTx.unsignedSerialized))
        .substring(2);

    return { unsignedTx, txHash };
}

export function parseSignature({ sigRes, chainId = 1 }) {
    // parse the signature r, s, v into an ethers signature instance
    const signature = ethers.Signature.from({
        r:
            '0x' +
            Buffer.from(sigRes.big_r.affine_point.substring(2), 'hex').toString(
                'hex',
            ),
        s: '0x' + Buffer.from(sigRes.s.scalar, 'hex').toString('hex'),
        v: sigRes.recovery_id + (chainId * 2 + 35),
    });
    return signature;
}

export async function broadcastTransaction(serializedTx) {
    console.log('BROADCAST serializedTx', serializedTx);

    try {
        const hash = await provider.send('eth_sendRawTransaction', [
            serializedTx,
        ]);
        const tx = await provider.waitForTransaction(hash, 1);

        console.log('SUCCESS TX HASH:', hash);
        console.log(`EXPLORER LINK: ${explorerBase}/tx/${hash}`);

        return {
            success: true,
            tx,
            hash,
            explorerLink: `${explorerBase}/tx/${hash}`,
        };
    } catch (e) {
        console.log(e);
        return {
            success: false,
            error: e,
        };
    }
}
