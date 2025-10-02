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
    QuoteResponse,
    SignedData,
    IntentMessage,
    NearConfig,
} from './types.js';

dotenv.config();

export class NearIntentsClient {
    private websocket: WebSocket | null = null;
    private quoteSubscriptionId: string | null = null;
    private messageId: number = 1;
    private nearConfig: NearConfig;

    // Fee configuration
    private readonly FEE_PERCENTAGE = 0.1; // 10% bridge fee (paid by user)
    private readonly PROTOCOL_FEE_RATE = 0.00000112; // 0.000111% NEAR Intents protocol fee (paid by solver)

    constructor(
        websocketUrl: string = 'wss://solver-relay-v2.chaindefuser.com/ws',
    ) {
        this.websocketUrl = websocketUrl;
        this.nearConfig = {
            contract_id: process.env.CONTRACT_ID,
            account_id: process.env.ACCOUNT_ID,
            private_key: process.env.PRIVATE_KEY,
        };
    }

    private websocketUrl: string;

    async connect(): Promise<boolean> {
        try {
            this.websocket = new WebSocket(this.websocketUrl);

            return new Promise((resolve, reject) => {
                if (!this.websocket) {
                    reject(new Error('Failed to create WebSocket'));
                    return;
                }

                this.websocket.on('open', () => {
                    console.log(`Connected to ${this.websocketUrl}`);
                    resolve(true);
                });

                this.websocket.on('error', (error: Error) => {
                    console.error(`Failed to connect: ${error}`);
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

                // Handle quote requests - these come as "event" method with subscription ID
                if (
                    message.method === 'event' &&
                    message.params?.subscription === this.quoteSubscriptionId
                ) {
                    const quoteRequest = this.parseQuoteRequest(message);
                    if (quoteRequest) {
                        await this.handleQuoteRequest(quoteRequest);
                    }
                }
                // Handle subscription responses
                else if (message.id && message.result) {
                    console.log(
                        `Subscription response: ${JSON.stringify(message)}`,
                    );
                }
                // Handle quote response results
                else if (message.result === 'OK') {
                    console.log('✅ Quote response accepted by relay!');
                } else if (message.error) {
                    const errorData = message.error;
                    const errorMsg =
                        typeof errorData === 'object'
                            ? errorData.message || 'Unknown error'
                            : String(errorData);
                    console.warn(`⚠️ Quote response error: ${errorMsg}`);
                } else {
                    console.log(
                        `Received other message: ${JSON.stringify(message)}`,
                    );
                }
            } catch (error) {
                console.error(`Error processing message: ${error}`);
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

    async handleQuoteRequest(quoteRequest: QuoteRequest): Promise<void> {
        // Define token identifiers
        const tokenIdUsdtOnEth =
            'nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near';
        const tokenIdUsdtOnTron =
            'nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near';
        const defineAmountIn = 4685840; // After deducting fees

        // Check if this is the swap we want to handle
        const isEthToTronSwap =
            quoteRequest.defuse_asset_identifier_in === tokenIdUsdtOnEth &&
            quoteRequest.defuse_asset_identifier_out === tokenIdUsdtOnTron;

        // Check if exact_amount_in is provided and meets minimum
        const hasValidAmount =
            quoteRequest.exact_amount_in !== undefined &&
            parseInt(quoteRequest.exact_amount_in) >= defineAmountIn;

        if (isEthToTronSwap && hasValidAmount) {
            console.log(
                `\n\n Processing USDT ETH->TRON swap: ${quoteRequest.quote_id}`,
            );
            console.log(`From: ${quoteRequest.defuse_asset_identifier_in}`);
            console.log(`To: ${quoteRequest.defuse_asset_identifier_out}`);
            console.log(
                `Amount in: ${quoteRequest.exact_amount_in} ($${(
                    parseInt(quoteRequest.exact_amount_in!) / 1000000
                ).toFixed(2)})`,
            );

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

            // Send the quote response
            const success = await this.sendQuoteResponse(
                quoteRequest,
                amountOut.toString(),
            );

            if (success) {
                console.log('📤 Quote response sent to relay');
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
        solverAccount: string = 'hasselalcala.near',
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
        const amountOutWithSign =
            messageData.intents[0].diff[
                quoteRequest.defuse_asset_identifier_out
            ];
        // Remove the negative sign to get the actual amount out
        const finalAmountOut = amountOutWithSign.replace('-', '');
        console.log(`->>>Amount out: ${finalAmountOut}`);

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

        console.log(`->>>Quote response: ${JSON.stringify(quoteResponse)}`);

        try {
            if (
                !this.websocket ||
                this.websocket.readyState !== WebSocket.OPEN
            ) {
                console.error('❌ WebSocket connection is not available');
                return false;
            }

            this.websocket.send(JSON.stringify(quoteResponse));
            console.log(`📤 Quote response sent for ${quoteRequest.quote_id}`);
            console.log(`   Offered: ${finalAmountOut} USDT TRON`);
            console.log(
                `   Requested: ${quoteRequest.exact_amount_in} USDT ETH`,
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
        const amountOutInt = parseInt(amountOut);
        const protocolFee = Math.floor(amountOutInt * this.PROTOCOL_FEE_RATE);
        const newAmountOut = amountOutInt + protocolFee;

        const messageData: IntentMessage = {
            signer_id: solverAccount,
            deadline: '2025-12-31T11:59:59.000Z',
            verifying_contract: 'intents.near',
            nonce: await this.generateNonce(),
            intents: [
                {
                    intent: 'token_diff',
                    diff: {
                        // We're providing USDT on TRON (positive amount - what user receives)
                        [quoteRequest.defuse_asset_identifier_in]:
                            quoteRequest.exact_amount_in!,
                        // We're receiving USDT on ETH (negative amount - user input + protocol fee)
                        [quoteRequest.defuse_asset_identifier_out]: `-${newAmountOut}`,
                    },
                },
            ],
        };

        const messageJson = JSON.stringify(messageData);
        const erc191Signature = await this.signQuoteSecp256k1FromEvm(
            messageJson,
        );
        console.log(`\n->>>ERC191 Signature using EVM: ${erc191Signature}`);

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

        console.log(`->>>>>>> Signing quote: ${quote}`);

        // Sign the message directly using EVM (like Matt's testSignAndRecover)
        const signature = await evmAccount.signMessage(quote);

        // Parse signature to extract R, S, V components
        const signatureBytes = Buffer.from(signature.slice(2), 'hex'); // Remove 0x prefix
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
}
