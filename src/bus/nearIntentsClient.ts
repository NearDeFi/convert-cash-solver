/**
 * NEAR Intents WebSocket Client
 * TypeScript equivalent of the Python near_intents_client.py
 */

import WebSocket from 'ws';
import * as dotenv from 'dotenv';
import { ethers } from 'ethers';
import bs58 from 'bs58';
import {
    QuoteRequest,
    WebSocketMessage,
    SignedData,
    IntentMessage,
} from './types.js';
import { NearClient } from './nearClient.js';
import { erc191SignMessage } from '../key/erc191.js';

dotenv.config({ path: './env.development.local' });

export class NearIntentsClient {
    private websocket: WebSocket | null = null;
    private quoteSubscriptionId: string | null = null;
    private messageId: number = 1;

    // Fee configuration
    private readonly FEE_PERCENTAGE = 0.000001; // 10% bridge fee (paid by user)
    private readonly PROTOCOL_FEE_RATE = 0.00000112; // 0.000111% NEAR Intents protocol fee (paid by solver)

    constructor(
        websocketUrl: string = 'wss://solver-relay-v2.chaindefuser.com/ws',
    ) {
        this.websocketUrl = websocketUrl;
    }

    private websocketUrl: string;

    async connect(): Promise<boolean> {
        try {
            // Get JWT token from environment
            const jwtToken = process.env.JWT_INTENTS;
            
            // Create headers with JWT if available
            const headers = jwtToken ? { Authorization: `Bearer ${jwtToken}` } : undefined;
            
            console.log('🔐 JWT Authentication Status:');
            if (jwtToken) {
                console.log('   ✅ JWT Token found - Using authentication');
                console.log(`   📝 Token: ${jwtToken.substring(0, 50)}...`);
            } else {
                console.log('   ❌ JWT Token NOT found - Connection without authentication');
            }
            console.log('=====================================');

            this.websocket = new WebSocket(this.websocketUrl, { headers });

            return new Promise((resolve, reject) => {
                if (!this.websocket) {
                    reject(new Error('Failed to create WebSocket'));
                    return;
                }

                this.websocket.on('open', () => {
                    console.log(`🔗 Connected to ${this.websocketUrl}`);
                    if (jwtToken) {
                        console.log('🔐 Authenticated connection with JWT');
                    } else {
                        console.log('⚠️ Connection WITHOUT JWT authentication');
                    }
                    resolve(true);
                });

                this.websocket.on('error', (error: Error) => {
                    console.error(`❌ Failed to connect: ${error}`);
                    reject(error);
                });

                // Set up ping/pong for connection health
                this.websocket.on('pong', () => {
                    // Connection is alive
                });

                // Send ping every 20 seconds
                setInterval(() => {
                    if (
                        this.websocket &&
                        this.websocket.readyState === WebSocket.OPEN
                    ) {
                        this.websocket.ping();
                    }
                }, 20000);
            });
        } catch (error) {
            console.error(`Failed to connect: ${error}`);
            return false;
        }
    }

    async disconnect(): Promise<void> {
        if (this.websocket) {
            this.websocket.close();
            this.websocket = null;
            console.log('Disconnected from WebSocket');
        }
    }

    private getNextId(): number {
        this.messageId += 1;
        return this.messageId;
    }

    async subscribeToQuotes(): Promise<boolean> {
        if (!this.websocket || this.websocket.readyState !== WebSocket.OPEN) {
            console.error('Not connected to WebSocket');
            return false;
        }

        const subscribeMessage: WebSocketMessage = {
            jsonrpc: '2.0',
            id: this.getNextId(),
            method: 'subscribe',
            params: ['quote'],
        };

        return new Promise((resolve) => {
            if (!this.websocket) {
                resolve(false);
                return;
            }

            this.websocket.send(JSON.stringify(subscribeMessage));

            const handleMessage = (data: WebSocket.Data) => {
                try {
                    const response = JSON.parse(data.toString());

                    if (response.result) {
                        this.quoteSubscriptionId = response.result;
                        console.log(
                            `Subscribed to quotes with ID: ${this.quoteSubscriptionId}`,
                        );
                        this.websocket?.off('message', handleMessage);
                        resolve(true);
                    } else {
                        console.error(
                            `Failed to subscribe to quotes: ${JSON.stringify(
                                response,
                            )}`,
                        );
                        this.websocket?.off('message', handleMessage);
                        resolve(false);
                    }
                } catch (error) {
                    console.error(`Error subscribing to quotes: ${error}`);
                    this.websocket?.off('message', handleMessage);
                    resolve(false);
                }
            };

            this.websocket.on('message', handleMessage);
        });
    }

    parseQuoteRequest(messageData: any): QuoteRequest | null {
        try {
            const params = messageData.params || {};
            const data = params.data || {};

            return {
                quote_id: data.quote_id,
                defuse_asset_identifier_in: data.defuse_asset_identifier_in,
                defuse_asset_identifier_out: data.defuse_asset_identifier_out,
                exact_amount_in: data.exact_amount_in,
                exact_amount_out: data.exact_amount_out,
                min_deadline_ms: data.min_deadline_ms || 60000,
                min_wait_ms: data.min_wait_ms,
                max_wait_ms: data.max_wait_ms,
                protocol_fee_included: data.protocol_fee_included,
                trusted_metadata: data.trusted_metadata,
            };
        } catch (error) {
            console.error(`Error parsing quote request: ${error}`);
            return null;
        }
    }

    async listenForEvents(): Promise<void> {
        if (!this.websocket || this.websocket.readyState !== WebSocket.OPEN) {
            console.error('Not connected to WebSocket');
            return;
        }

        console.log('Listening for events...');

        this.websocket.on('message', async (data: WebSocket.Data) => {
            try {
                const message = JSON.parse(data.toString());
                const jwtStatus = process.env.JWT_INTENTS ? 'WITH JWT' : 'WITHOUT JWT';

                // Handle quote requests - these come as "event" method with subscription ID
                if (
                    message.method === 'event' &&
                    message.params?.subscription === this.quoteSubscriptionId
                ) {
                    const quoteRequest = this.parseQuoteRequest(message);
                    if (quoteRequest) {
                        await this.handleQuoteRequest(quoteRequest, message);
                    }
                }
                // Handle quote response results (show complete response)
                else if (message.result === 'OK') {
                    console.log(`✅ [${jwtStatus}] Quote response accepted by relay!`);
                    console.log(`📋 Complete response from relay:`);
                    console.log(`   📊 Full message: ${JSON.stringify(message, null, 2)}`);
                } else if (message.error) {
                    const errorData = message.error;
                    const errorMsg =
                        typeof errorData === 'object'
                            ? errorData.message || 'Unknown error'
                            : String(errorData);
                    console.warn(`⚠️ [${jwtStatus}] Quote response error: ${errorMsg}`);
                    console.log(`📋 Complete error response:`);
                    console.log(`   📊 Full message: ${JSON.stringify(message, null, 2)}`);
                }
                // Show any other important responses from relay
                else if (message.id && message.result) {
                    console.log(`📝 [${jwtStatus}] Other response from relay:`);
                    console.log(`   📊 Full message: ${JSON.stringify(message, null, 2)}`);
                }
                // Silently ignore all other messages
            } catch (error) {
                console.error(`❌ Error processing message: ${error}`);
            }
        });

        this.websocket.on('close', () => {
            console.log('WebSocket connection closed');
            this.websocket = null;
        });

        this.websocket.on('error', (error: Error) => {
            console.error(`Error in event loop: ${error}`);
            this.websocket = null;
        });

        // Keep the function running indefinitely to maintain the connection
        return new Promise((resolve, reject) => {
            // This promise will never resolve, keeping the connection alive
            // The connection will only close when the process is terminated
            this.websocket?.on('close', () => {
                reject(new Error('WebSocket connection closed'));
            });

            this.websocket?.on('error', (error: Error) => {
                reject(error);
            });
        });
    }

    async handleQuoteRequest(quoteRequest: QuoteRequest, originalMessage?: any): Promise<void> {
        // Define token identifiers
        const tokenIdUsdtOnEth =
            'nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near';
        const tokenIdUsdtOnTron =
            'nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near';

        // Check if this is the swap we want to handle (USDT ETH -> USDT TRON)
        const isEthToTronSwap =
            quoteRequest.defuse_asset_identifier_in === tokenIdUsdtOnEth &&
            quoteRequest.defuse_asset_identifier_out === tokenIdUsdtOnTron;

        // Check if exact_amount_in matches the specific amount we want
        //const defineAmountIn = 4685835; // After deducting fees
        const hasValidAmount =
            quoteRequest.exact_amount_in !== undefined &&
            parseInt(quoteRequest.exact_amount_in) === 4999995;//4685835;

        // Only process and log USDT ETH -> USDT TRON swaps with exact amount
        if (isEthToTronSwap && hasValidAmount) {
            const jwtStatus = process.env.JWT_INTENTS ? 'WITH JWT' : 'WITHOUT JWT';
            
            // Log the complete request from WebSocket only for filtered transactions
            if (originalMessage) {
                console.log(`\n📥 [${jwtStatus}] Complete Quote Request from WebSocket:`);
                console.log(JSON.stringify(originalMessage, null, 2));
                console.log('---');
            }
            
            console.log(
                `\n🔄 [${jwtStatus}] Processing USDT ETH->TRON swap: ${quoteRequest.quote_id}`,
            );
            
            // Show all quote request fields
            console.log(`📋 Complete Quote Request Data:`);
            console.log(`   🆔 Quote ID: ${quoteRequest.quote_id}`);
            console.log(`   📤 From: ${quoteRequest.defuse_asset_identifier_in}`);
            console.log(`   📥 To: ${quoteRequest.defuse_asset_identifier_out}`);
            console.log(`   💰 Amount in: ${quoteRequest.exact_amount_in} ($${(
                parseInt(quoteRequest.exact_amount_in!) / 1000000
            ).toFixed(2)})`);
            console.log(`   💰 Amount out: ${quoteRequest.exact_amount_out || 'N/A'}`);
            console.log(`   ⏰ Min deadline: ${quoteRequest.min_deadline_ms}ms`);
            console.log(`   ⏱️ Min wait: ${quoteRequest.min_wait_ms || 'N/A'}ms`);
            console.log(`   ⏱️ Max wait: ${quoteRequest.max_wait_ms || 'N/A'}ms`);
            console.log(`   💳 Protocol fee included: ${quoteRequest.protocol_fee_included || 'N/A'}`);
            
            // Show trusted metadata if available
            if (quoteRequest.trusted_metadata) {
                console.log(`🔐 Trusted Metadata:`);
                console.log(`   📍 Source: ${quoteRequest.trusted_metadata.source || 'N/A'}`);
                
                if (quoteRequest.trusted_metadata.upstream_metadata) {
                    console.log(`   🔗 Upstream Metadata:`);
                    console.log(`     📊 Traceparent: ${quoteRequest.trusted_metadata.upstream_metadata.traceparent || 'N/A'}`);
                    console.log(`     👥 Partner ID: ${quoteRequest.trusted_metadata.upstream_metadata.partner_id || 'N/A'}`);
                }
                
                if (quoteRequest.trusted_metadata.upstream_trusted_metadata) {
                    console.log(`   🔐 Upstream Trusted Metadata:`);
                    console.log(`     📍 Source: ${quoteRequest.trusted_metadata.upstream_trusted_metadata.source || 'N/A'}`);
                    console.log(`     📋 Quote type: ${quoteRequest.trusted_metadata.upstream_trusted_metadata.quote_type || 'N/A'}`);
                    console.log(`     👥 Partner ID: ${quoteRequest.trusted_metadata.upstream_trusted_metadata.partner_id || 'N/A'}`);
                    
                    if (quoteRequest.trusted_metadata.upstream_trusted_metadata.quote_request_data) {
                        console.log(`     📊 Quote request data:`);
                        console.log(`       🔍 Dry run: ${quoteRequest.trusted_metadata.upstream_trusted_metadata.quote_request_data.dry || 'N/A'}`);
                        console.log(`       📈 Slippage tolerance: ${quoteRequest.trusted_metadata.upstream_trusted_metadata.quote_request_data.slippageTolerance || 'N/A'}`);
                    }
                }
            } else {
                console.log(`🔐 Trusted Metadata: N/A`);
            }
        } else {
            // Silently ignore all other swaps (wrong tokens or wrong amount)
            return;
        }

        if (isEthToTronSwap && hasValidAmount) {

            // Calculate quote with proper fee structure
            const amountIn = parseInt(quoteRequest.exact_amount_in!);

            // Calculate bridge fee (10% of input - paid by user)
            const bridgeFee = Math.floor(amountIn * this.FEE_PERCENTAGE);

            // Calculate output amount (what user receives - input minus bridge fee)
            const amountOut = amountIn - bridgeFee;

            // Calculate protocol fee on output (0.000111% - paid by solver)
            const protocolFee = Math.floor(amountOut * this.PROTOCOL_FEE_RATE);

            console.log(`💰 Fee calculation:`);
            console.log(
                `   Input amount: ${amountIn} ($${(amountIn / 1000000).toFixed(
                    2,
                )})`,
            );
            console.log(
                `   Bridge fee (10%): ${bridgeFee} ($${(
                    bridgeFee / 1000000
                ).toFixed(2)})`,
            );
            console.log(
                `   Output amount (user receives): ${amountOut} ($${(
                    amountOut / 1000000
                ).toFixed(2)})`,
            );
            console.log(`   Protocol fee: ${protocolFee}`);
            console.log(
                `   Total solver cost: ${amountOut + protocolFee} ($${(
                    (amountOut + protocolFee) /
                    1000000
                ).toFixed(2)})`,
            );

            console.log(
                `Proposed amount out: ${amountOut} ($${(
                    amountOut / 1000000
                ).toFixed(2)})`,
            );

            // Send the REAL quote response (requires actual funds)
            const success = await this.sendQuoteResponse(
                quoteRequest,
                amountOut.toString(),
            );

            if (success) {
                const jwtStatus = process.env.JWT_INTENTS ? 'WITH JWT' : 'WITHOUT JWT';
                console.log(`📤 [${jwtStatus}] Quote response sent to relay`);
                console.log('   ⏳ Waiting for relay validation...');
            } else {
                console.error('❌ Failed to send quote response');
            }
        }
        // Silently ignore requests that don't meet our criteria
    }

    async sendQuoteResponse(
        quoteRequest: QuoteRequest,
        amountOut: string,
        solverAccount: string = process.env.LOCAL_TESTING === 'true'
            ? process.env.NEAR_CONTRACT_ID!
            : '0xa48c13854fa61720c652e2674Cfa82a5F8514036'.toLowerCase(),
    ): Promise<boolean> {
        if (!this.websocket || this.websocket.readyState !== WebSocket.OPEN) {
            console.error('Not connected to WebSocket');
            return false;
        }

        // Create signed data structure (NEP413 format)
        const signedData = await this.signedDataErc191(
            solverAccount,
            quoteRequest,
            amountOut,
        );

        // Parse the JSON payload string to access the intents data
        const messageData = JSON.parse(signedData.payload);
        // Extract amount_out from diff (it's negative, so we get the absolute value)
        const amountOutFromDiff =
            messageData.intents[0].diff[
                quoteRequest.defuse_asset_identifier_out
            ];
        // Remove the negative sign to get the actual amount out (must match quote_output.amount_out)
        const finalAmountOut = amountOutFromDiff.replace('-', '');
        console.log(`->>>Amount out (from diff): ${finalAmountOut}`);

        // Create quote response message
        const quoteResponse: WebSocketMessage = {
            jsonrpc: '2.0',
            id: this.getNextId(),
            method: 'quote_response',
            params: [
                {
                    quote_id: quoteRequest.quote_id,
                    quote_output: {
                        amount_out: finalAmountOut,
                    },
                    signed_data: signedData,
                },
            ],
        };

        // Log the complete response that will be sent
        const jwtStatus = process.env.JWT_INTENTS ? 'WITH JWT' : 'WITHOUT JWT';
        console.log(`📤 [${jwtStatus}] Complete Solver Quote Response:`);
        console.log(JSON.stringify(quoteResponse, null, 2));
        console.log('---');

        try {
            if (
                !this.websocket ||
                this.websocket.readyState !== WebSocket.OPEN
            ) {
                console.error('❌ WebSocket connection is not available');
                return false;
            }

            this.websocket.send(JSON.stringify(quoteResponse));
            const jwtStatus = process.env.JWT_INTENTS ? 'WITH JWT' : 'WITHOUT JWT';
            console.log(`📤 [${jwtStatus}] Quote response sent for ${quoteRequest.quote_id}`);
            console.log(`   💰 Offered: ${finalAmountOut} USDT TRON`);
            console.log(
                `   📥 Requested: ${quoteRequest.exact_amount_in} USDT ETH`,
            );
            console.log('   ⏳ Waiting for relay response...');
            return true;
        } catch (error) {
            console.error(`❌ Failed to send quote response: ${error}`);
            return false;
        }
    }

    async signedDataErc191(
        solverAccount: string,
        quoteRequest: QuoteRequest,
        amountOut: string,
    ): Promise<SignedData> {
        // Calculate protocol fee for the token_diff
        const amountInInt = parseInt(quoteRequest.exact_amount_in!);
        const protocolFee = Math.floor(amountInInt * this.PROTOCOL_FEE_RATE);
        const totalAmountIn = amountInInt + protocolFee;
        const amountOutInt = parseInt(amountOut);

        const messageData: IntentMessage = {
            signer_id: solverAccount,
            deadline: '2025-12-31T11:59:59.000Z',
            verifying_contract: 'intents.near',
            nonce: await this.generateNonce(),
            intents: [
                {
                    intent: 'token_diff',
                    diff: {
                        // Solver receives ETH (positive - solver gains)
                        [quoteRequest.defuse_asset_identifier_in]: totalAmountIn.toString(),
                        // Solver pays TRON (negative - solver pays, must match quote_output.amount_out)
                        [quoteRequest.defuse_asset_identifier_out]: `-${amountOutInt}`,
                    },
                },
            ],
        };

        const messageJson = JSON.stringify(messageData);
        const erc191Signature = await this.signQuoteSecp256k1FromEvm(
            messageJson,
        );
        //console.log(`\n->>>ERC191 Signature using EVM: ${erc191Signature}`);

        // For ERC191, the payload is simply the signed message (not a nested structure)
        const erc191Format: SignedData = {
            standard: 'erc191',
            payload: messageJson,
            signature: erc191Signature,
        };

        return erc191Format;
    }

    async generateNonce(): Promise<string> {
        // Generate a random nonce for quotes
        const randomBytes = new Uint8Array(32);
        crypto.getRandomValues(randomBytes);
        return Buffer.from(randomBytes).toString('base64');
    }

    createEvmAccount(): ethers.Wallet {
        // Use the specific private key for the Ethereum address
        const evmPrivateKey = process.env.EVM_PRIVATE_KEY;
        if (!evmPrivateKey) {
            throw new Error('EVM_PRIVATE_KEY must be set in .env file');
        }

        return new ethers.Wallet(evmPrivateKey);
    }

    async signQuoteSecp256k1FromEvm(quote: string): Promise<string> {
        const evmAccount = this.createEvmAccount();

        //console.log(`->>>>>>> Signing quote: ${quote}`);

        // Sign the message directly using EVM (like Matt's testSignAndRecover)
        const signature =
            process.env.LOCAL_TESTING === 'true'
                ? await erc191SignMessage(quote)
                : await evmAccount.signMessage(quote);

        // Parse signature to extract R, S, V components
        const signatureBytes = Buffer.from(signature!.slice(2), 'hex'); // Remove 0x prefix
        const r = signatureBytes.slice(0, 32);
        const s = signatureBytes.slice(32, 64);
        let v = signatureBytes[64];

        // Convert EVM V (27/28) to NEAR V (0/1/2/3)
        if (v === 27) v = 0;
        else if (v === 28) v = 1;

        // Reconstruct signature with corrected V
        const correctedSignature = Buffer.concat([r, s, Buffer.from([v])]);
        const signatureB58 = bs58.encode(correctedSignature);
        const signatureFormatted = `secp256k1:${signatureB58}`;

        return signatureFormatted;
    }

    // async createBalancedSolverIntent(userSignedIntent: SignedData): Promise<SignedData> {
    //     const userIntentData: IntentMessage = JSON.parse(userSignedIntent.payload);
    //     const userDiff = userIntentData.intents[0].diff;

    //     const solverDiff: Record<string, string> = {};
    //     for (const [token, amount] of Object.entries(userDiff)) {
    //         const amountStr = amount as string;
    //         solverDiff[token] = amountStr.startsWith('-') ? amountStr.substring(1) : `-${amountStr}`;
    //     }

    //     const solverAccount =
    //         process.env.LOCAL_TESTING === 'true'
    //             ? process.env.NEAR_CONTRACT_ID!
    //             : '0xa48c13854fa61720c652e2674Cfa82a5F8514036'.toLowerCase();

    //     const solverIntentData: IntentMessage = {
    //         signer_id: solverAccount,
    //         deadline: userIntentData.deadline,
    //         verifying_contract: userIntentData.verifying_contract,
    //         nonce: await this.generateNonce(),
    //         intents: [{ intent: 'token_diff', diff: solverDiff }],
    //     };

    //     const messageJson = JSON.stringify(solverIntentData);
    //     const erc191Signature = await this.signQuoteSecp256k1FromEvm(messageJson);

    //     return { standard: 'erc191', payload: messageJson, signature: erc191Signature };
    // }

    async createBalancedSolverIntent(userSignedIntent: SignedData): Promise<SignedData> {
        const userIntentData: IntentMessage = JSON.parse(userSignedIntent.payload);
        const userDiff = userIntentData.intents[0].diff;

        const FEE_PPM = parseInt(process.env.PROTOCOL_FEE_PPM || '1', 10);
        const feeFor = (amount: number) => Math.ceil((amount * FEE_PPM) / 1_000_000);

        const solverDiff: Record<string, string> = {};
        for (const [token, amountStr] of Object.entries(userDiff)) {
            const isUserNegative = amountStr.startsWith('-');
            const absAmount = Math.abs(parseInt(amountStr, 10));
            const fee = feeFor(absAmount);

            if (isUserNegative) {
                // User pays this token -> solver receives with fee deducted
                const recv = Math.max(absAmount - fee, 0);
                solverDiff[token] = recv.toString(); // positive
            } else {
                // User receives this token -> solver pays with fee added
                const pay = absAmount + fee;
                solverDiff[token] = `-${pay}`; // negative
            }
        }

        const solverAccount =
            process.env.LOCAL_TESTING === 'true'
                ? process.env.NEAR_CONTRACT_ID!
                : '0xa48c13854fa61720c652e2674Cfa82a5F8514036'.toLowerCase();

        const solverIntentData: IntentMessage = {
            signer_id: solverAccount,
            deadline: userIntentData.deadline,
            verifying_contract: userIntentData.verifying_contract,
            nonce: await this.generateNonce(),
            intents: [{ intent: 'token_diff', diff: solverDiff }],
        };

        const messageJson = JSON.stringify(solverIntentData);
        const erc191Signature = await this.signQuoteSecp256k1FromEvm(messageJson);

        return { standard: 'erc191', payload: messageJson, signature: erc191Signature };
    }
    
    async processUserIntent(userSignedIntent: SignedData): Promise<any> {
        if (userSignedIntent.standard !== 'erc191') {
            throw new Error('Only erc191 is supported');
        }

        let userIntentData: IntentMessage;
        try {
            userIntentData = JSON.parse(userSignedIntent.payload);
        } catch {
            throw new Error('Invalid user intent payload');
        }

        // Validate the swap pair
        const tokenIdUsdtOnEth =
            'nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near';
        const tokenIdUsdtOnTron =
            'nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near';
        const diff = userIntentData.intents[0].diff;

        if (!(tokenIdUsdtOnEth in diff) || !(tokenIdUsdtOnTron in diff)) {
            throw new Error('Unsupported swap pair for this solver');
        }

        const solverSignedIntent = await this.createBalancedSolverIntent(userSignedIntent);
        const executeArgs = { signed: [userSignedIntent, solverSignedIntent] };

        const nearClient = new NearClient();
        return await nearClient.callFunction(
            'intents.near',
            'execute_intents',
            executeArgs,
            '0',
            '100000000000000'
        );
    }
}
