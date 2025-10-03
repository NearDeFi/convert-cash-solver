/**
 * Quote Generator - Test client to generate quote requests
 * that our bus can detect and respond to
 */

import WebSocket from 'ws';
import * as dotenv from 'dotenv';

dotenv.config({ path: './env.development.local' });

interface QuoteRequest {
    jsonrpc: string;
    id: number;
    method: string;
    params: [{
        defuse_asset_identifier_in: string;
        defuse_asset_identifier_out: string;
        exact_amount_in: string;
        min_deadline_ms?: number;
    }];
}

export class QuoteGenerator {
    private websocket: WebSocket | null = null;
    private messageId: number = 1;

    constructor(
        private websocketUrl: string = 'wss://solver-relay-v2.chaindefuser.com/ws'
    ) {}

    async connect(): Promise<boolean> {
        try {
            this.websocket = new WebSocket(this.websocketUrl);

            return new Promise((resolve, reject) => {
                if (!this.websocket) {
                    reject(new Error('Failed to create WebSocket'));
                    return;
                }

                this.websocket.on('open', () => {
                    console.log(`✅ Connected to relay: ${this.websocketUrl}`);
                    resolve(true);
                });

                this.websocket.on('error', (error: Error) => {
                    console.error(`❌ Connection error: ${error}`);
                    reject(error);
                });
            });
        } catch (error) {
            console.error(`❌ Connection failed: ${error}`);
            return false;
        }
    }

    async disconnect(): Promise<void> {
        if (this.websocket) {
            this.websocket.close();
            this.websocket = null;
            console.log('🔌 Disconnected from WebSocket');
        }
    }

    private getNextId(): number {
        this.messageId += 1;
        return this.messageId;
    }

    async sendQuoteRequest(): Promise<void> {
        if (!this.websocket || this.websocket.readyState !== WebSocket.OPEN) {
            console.error('❌ Not connected to WebSocket');
            return;
        }

        // Define tokens for USDT ETH → TRON
        const tokenIdUsdtOnEth = 'nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near';
        const tokenIdUsdtOnTron = 'nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near';

        const amountIn = '4685840'; // $4.69 USDT - original amount 

        const quoteRequest: QuoteRequest = {
            jsonrpc: '2.0',
            id: this.getNextId(),
            method: 'quote_request',
            params: [{
                defuse_asset_identifier_in: tokenIdUsdtOnEth,
                defuse_asset_identifier_out: tokenIdUsdtOnTron,
                exact_amount_in: amountIn,
                min_deadline_ms: 60000 // 1 minute
            }]
        };

        console.log('📤 Sending quote request...');
        console.log(`   From: ${tokenIdUsdtOnEth}`);
        console.log(`   To: ${tokenIdUsdtOnTron}`);
        console.log(`   Amount: ${amountIn} ($${(parseInt(amountIn) / 1000000).toFixed(2)})`);

        this.websocket.send(JSON.stringify(quoteRequest));
        console.log('✅ Request sent, waiting for response...');

        // Listen for response
        this.websocket.on('message', (data: WebSocket.Data) => {
            try {
                const response = JSON.parse(data.toString());
                console.log('📥 Response received:', JSON.stringify(response, null, 2));
            } catch (error) {
                console.error('❌ Error processing response:', error);
            }
        });
    }
}

// Function to run the generator
async function runQuoteGenerator(): Promise<void> {
    console.log('🚀 Quote Generator Test');
    console.log('=======================');
    console.log('This script will send a USDT ETH → TRON quote request');
    console.log('that your bus should detect and respond to.');
    console.log('=======================');

    const generator = new QuoteGenerator();

    try {
        const connected = await generator.connect();
        if (!connected) {
            console.error('❌ Could not connect to relay');
            return;
        }

        // Send request after 2 seconds
        setTimeout(async () => {
            await generator.sendQuoteRequest();
        }, 2000);

        // Keep connection open for 30 seconds
        setTimeout(async () => {
            await generator.disconnect();
            console.log('✅ Test completed');
            process.exit(0);
        }, 30000);

    } catch (error) {
        console.error(`❌ Generator error: ${error}`);
        await generator.disconnect();
        process.exit(1);
    }
}

// Execute if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
    runQuoteGenerator();
}
