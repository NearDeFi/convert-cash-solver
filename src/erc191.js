import { ethers } from 'ethers';

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
export async function signAndRecover() {
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

signAndRecover();
