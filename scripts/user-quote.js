import { ethers } from 'ethers';
import dotenv from 'dotenv';
import crypto from 'crypto';
import bs58 from 'bs58';

dotenv.config({ path: './env.development.local' });

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
        const recoveredAddress = ethers.verifyMessage(message, signature).toLowerCase();
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

        // Select best option - chooses the one with HIGHEST amount_out (best for user)
        const selectBestOption = (options) => {
            if (!options || options.length === 0) {
                return null;
            }
            
            let bestOption = null;
            for (const option of options) {
                if (!bestOption || parseInt(option.amount_out) > parseInt(bestOption.amount_out)) {
                    bestOption = option;
                }
            }
            return bestOption;
        };

        // Create token_diff_quote equivalent to Python create_token_diff_quote
        const createTokenDiffQuote = (evm_account, token_in, amount_in, token_out, amount_out) => {
            // Generate random nonce (equivalent to Python's base64.b64encode(random.getrandbits(256).to_bytes(32, byteorder='big')))
            const nonce = Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString('base64');
            
            // Create the quote object (equivalent to Python's Quote TypedDict)
            // For ERC191/NEP413, ALL fields including nonce and verifying_contract are signed
            const quote = {
                signer_id: evm_account.toLowerCase(), // Convert to lowercase for NEAR compatibility
                nonce: nonce,
                verifying_contract: "intents.near",
                deadline: "2025-12-31T11:59:59.000Z",
                intents: [
                    {
                        intent: "token_diff",
                        diff: {
                            [token_in]: "-" + amount_in,  // Negative for input token
                            [token_out]: amount_out       // Positive for output token
                        }
                    }
                ]
            };
            
            return quote;
        };

        // Publish intent to the solver bus
        const publishIntent = async (signed_intent) => {
            const rpc_request = {
                id: "dontcare",
                jsonrpc: "2.0",
                method: "publish_intent",
                params: [signed_intent]
            };
            
            try {
                const response = await fetch('https://solver-relay-v2.chaindefuser.com/rpc', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(rpc_request)
                });
                
                const result = await response.json();
                return result;
            } catch (error) {
                console.error('Error publishing intent:', error);
                return null;
            }
        };

        // Sign quote using EVM account with secp256k1 algorithm (ERC191 standard)
        const signQuoteSecp256k1FromEvm = async (evm_account, quote) => {
            console.log(`->>>>>>> Signing quote: ${quote}`);
            
            // Convert quote to JSON string if it's an object, otherwise use as string
            let quote_json;
            if (typeof quote === 'object') {
                quote_json = JSON.stringify(quote, null, 0); // Compact JSON without spaces
            } else {
                quote_json = quote;
            }
            
            // Create the message hash using Ethereum's personal message format (ERC-191)
            const message_hash = ethers.hashMessage(quote_json);
            
            // Sign the message using the connected wallet
            const signature = await connectedWallet.signMessage(quote_json);
            
            // Extract signature components (remove 0x prefix)
            const sig = signature.slice(2);
            const r = sig.slice(0, 64);
            const s = sig.slice(64, 128);
            const v = sig.slice(128, 130);
            
            // Convert v from hex to number
            let v_num = parseInt(v, 16);
            
            // Convert v from Ethereum format (27/28) to modern format (0-3)
            if (v_num === 27) {
                v_num = 0;
            } else if (v_num === 28) {
                v_num = 1;
            } else {
                v_num = v_num - 27; // Convert other values
            }
            
            // Ensure v is in valid range (0-3)
            v_num = v_num % 4;
            
            // Combine R, S, V
            const r_buffer = Buffer.from(r, 'hex');
            const s_buffer = Buffer.from(s, 'hex');
            const v_buffer = Buffer.from([v_num]);
            const rsv_signature = Buffer.concat([r_buffer, s_buffer, v_buffer]);
            
            // Encode as base58
            const signature_b58 = bs58.encode(rsv_signature);
            const signature_formatted = `secp256k1:${signature_b58}`;
            
            // Return Commitment object
            return {
                standard: "erc191",
                payload: quote_json,
                signature: signature_formatted
            };
        };

        // Select and display the best option
        if (quoteRes && quoteRes.result && Array.isArray(quoteRes.result)) {
            const bestOption = selectBestOption(quoteRes.result);
            if (bestOption) {
                console.log('---');
                console.log('Best Quote Option:');
                console.log('Quote Hash:', bestOption.quote_hash);
                console.log('Amount In:', bestOption.amount_in);
                console.log('Amount Out:', bestOption.amount_out);
                console.log('Expiration:', bestOption.expiration_time);
                console.log('asset in:', ASSET_IN);
                console.log('asset out:', ASSET_OUT);
                console.log('---');
                
                // Create token_diff_quote using the best option
                // Ensure amounts are strings (contract expects string values for amounts)
                const tokenDiffQuote = createTokenDiffQuote(
                    wallet.address,
                    ASSET_IN,
                    String(bestOption.amount_in),
                    ASSET_OUT,
                    String(bestOption.amount_out)
                );
                
                console.log('Generated Token Diff Quote:');
                console.log(JSON.stringify(tokenDiffQuote, null, 2));
                console.log('---');

                // Sign the quote (all fields including nonce and verifying_contract)
                // This returns { standard: "erc191", payload: "<JSON string>", signature: "..." }
                // where payload is the signed message itself
                const signedCommitment = await signQuoteSecp256k1FromEvm(wallet.address, tokenDiffQuote);
                console.log('Signed Commitment:');
                console.log(JSON.stringify(signedCommitment, null, 2));
                console.log('---');
                
                // // Create PublishIntent object
                // const publishIntentData = {
                //     signed_data: signedCommitment,
                //     quote_hashes: [bestOption.quote_hash]
                // };
                
                // console.log('Publishing Intent:');
                // console.log(JSON.stringify(publishIntentData, null, 2));
                // console.log('---');
                
                // // Publish the intent to the solver bus
                // const publishResult = await publishIntent(publishIntentData);
                // console.log('Publish Intent Result:');
                // console.log(JSON.stringify(publishResult, null, 2));
                // console.log('---');

                // For execute_intents with ERC191, the payload structure might be different
                // Let's try with just the message directly as payload (no nested structure)
                const executeIntentsArgs = {
                    signed: [{
                        standard: "erc191",
                        payload: signedCommitment.payload,  // Just the JSON string message
                        signature: signedCommitment.signature
                    }]
                };
                
                console.log('->>>>>>> Arguments for execute_intents:');
                console.log(JSON.stringify(executeIntentsArgs, null, 2));

                //Once user have the signed quote, they can publish it to the contract using the execute_intents method.
                // To make this call, use the near-js API
                console.log('\n📤 Publishing intent to NEAR contract...');
                
                try {
                    // Initialize NEAR client
                    const nearClient = new NearClient();
                    
                    // Call the execute_intents method on intents.near contract
                    // Pass the args object directly - NEAR API will serialize it correctly
                    const result = await nearClient.callFunction(
                        'intents.near',  // Contract ID
                        'execute_intents',  // Method name
                        executeIntentsArgs,  // Arguments: { signed: [...] }
                        '0',  // No deposit
                        '100000000000000'  // 100 TGas for safety
                    );
                    
                    console.log('✅ Intent successfully published to NEAR contract!');
                    console.log('Transaction result:', result);
                } catch (error) {
                    console.error('❌ Failed to publish intent to NEAR contract:', error);
                }
            }
        }

        // Verify the quotes 

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
