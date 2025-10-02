/**
 * NEAR Intents WebSocket Demo
 * TypeScript equivalent of the Python main.py
 * This script demonstrates how to connect to the NEAR Intents solver relay
 * and handle quote requests and status updates.
 */

import { NearIntentsClient } from './nearIntentsClient.js';

// Handle Ctrl+C gracefully
let client: NearIntentsClient | null = null;

async function demoSolver(): Promise<void> {
    console.log('🚀 NEAR Intents WebSocket Demo - USDT ETH ↔ TRON Solver');
    console.log('============================================================');
    console.log('This demo will:');
    console.log('1. Connect to the NEAR Intents solver relay');
    console.log('2. Subscribe to quote requests');
    console.log('3. Listen for USDT ETH → TRON swap requests (min $5)');
    console.log('4. Ignore all other swap requests');
    console.log('============================================================');
    console.log('Token pairs handled:');
    console.log(
        '  From: USDT on Ethereum (nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near)',
    );
    console.log(
        '  To:   USDT on Tron (nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near)',
    );
    console.log('============================================================');

    // Create client instance
    client = new NearIntentsClient();

    try {
        // Connect to WebSocket
        console.log('Connecting to NEAR Intents solver relay...');
        const connected = await client.connect();
        if (!connected) {
            console.error('Failed to connect to WebSocket');
            return;
        }

        // Subscribe to quote requests
        console.log('Subscribing to quote requests...');
        const subscribed = await client.subscribeToQuotes();
        if (!subscribed) {
            console.error('Failed to subscribe to quotes');
            return;
        }

        console.log('✅ Successfully connected and subscribed!');
        console.log('Now listening for quote requests...');
        console.log('Press Ctrl+C to stop');

        // Start listening for events
        await client.listenForEvents();
    } catch (error) {
        if (error instanceof Error && error.message.includes('SIGINT')) {
            console.log('Received interrupt signal, shutting down...');
        } else {
            console.error(`Error in demo: ${error}`);
        }
    } finally {
        // Clean up
        await client.disconnect();
        console.log('Demo completed');
    }
}

process.on('SIGINT', async () => {
    console.log('\nReceived interrupt signal, shutting down...');
    if (client) {
        await client.disconnect();
    }
    process.exit(0);
});

// Run the demo
demoSolver().catch((error) => {
    console.error(`Fatal error: ${error}`);
    process.exit(1);
});
