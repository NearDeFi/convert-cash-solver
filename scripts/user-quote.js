import { ethers } from 'ethers';
import dotenv from 'dotenv';

dotenv.config({ path: '.env.development.local' });

const ASSET_IN =
    'nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near';
const ASSET_OUT =
    'nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near';
const AMOUNT_IN = '4685840';
const DEADLINE_MS = 600000; // 10 minutes

const nearIntentsFetch = async (method, params, bridgeUrl = false) => {
    try {
        const res = await fetch(
            bridgeUrl
                ? 'https://bridge.chaindefuser.com/rpc'
                : 'https://solver-relay-v2.chaindefuser.com/rpc',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    jsonrpc: '2.0',
                    id: 1,
                    method,
                    params: [params],
                }),
            },
        ).then((r) => r.json());

        return res;
    } catch (e) {
        console.error(
            `Error calling near intents method ${method} with params ${params}. Error:`,
            e,
        );
        return null;
    }
};

async function main() {
    try {
        // Check if private key is provided
        if (!process.env.EVM_USER_PRIVATE_KEY) {
            throw new Error('PRIVATE_KEY not found in environment variables');
        }

        // Check if RPC URL is provided
        if (!process.env.EVM_USER_RPC_URL) {
            throw new Error('RPC_URL not found in environment variables');
        }

        // Create wallet from private key
        const wallet = new ethers.Wallet(process.env.EVM_USER_PRIVATE_KEY);
        console.log('Wallet Address:', wallet.address);
        console.log('---');

        // Connect wallet to provider
        const provider = new ethers.JsonRpcProvider(
            process.env.EVM_USER_RPC_URL,
        );
        const connectedWallet = wallet.connect(provider);

        // Get and display wallet balance
        console.log('Fetching balance...');
        const balance = await connectedWallet.provider.getBalance(
            wallet.address,
        );
        const balanceInEth = ethers.formatEther(balance);
        console.log('Balance:', balanceInEth, 'ETH');
        console.log('Balance (Wei):', balance.toString());
        console.log('---');

        // Sign a message using ERC-191 standard
        const message = 'Hello from Ethereum wallet!';
        console.log('Message to sign:', message);

        // signMessage automatically uses ERC-191 standard (prefixes with "\x19Ethereum Signed Message:\n" + message.length)
        const signature = await connectedWallet.signMessage(message);
        console.log('Signature:', signature);
        console.log('---');

        // Verify the signature
        const recoveredAddress = ethers.verifyMessage(message, signature);
        console.log('Recovered Address:', recoveredAddress);
        console.log(
            'Signature Valid:',
            recoveredAddress.toLowerCase() === wallet.address.toLowerCase(),
        );
        console.log('---');

        // Additional example: Sign typed data (EIP-712)
        console.log('ERC-191 Message Hash:');
        const messageHash = ethers.hashMessage(message);
        console.log('Message Hash:', messageHash);

        console.log('---');

        const quoteRes = await nearIntentsFetch('quote', {
            defuse_asset_identifier_in: ASSET_IN,
            defuse_asset_identifier_out: ASSET_OUT,
            exact_amount_in: AMOUNT_IN,
            min_deadline_ms: DEADLINE_MS, // OPTIONAL. default 60_000ms / 1min
        });
        console.log('Quote:', quoteRes);

        const depositRes = await nearIntentsFetch(
            'deposit_address',
            {
                account_id: wallet.address,
                chain: 'eth:1',
            },
            true,
        );

        console.log('From address:', wallet.address);
        console.log('Deposit:', depositRes);
    } catch (error) {
        console.error('Error:', error.message);
        process.exit(1);
    }
}

main();
