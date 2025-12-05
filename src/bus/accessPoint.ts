import { serve } from '@hono/node-server';
import { cors } from 'hono/cors';
import { Hono } from 'hono';
import type { SignedData } from './types.js';
import { NearIntentsClient } from './nearIntentsClient.js';

const PORT = parseInt(process.env.SOLVER_API_PORT || '3001', 10);
const app = new Hono();
app.use('/*', cors());

const intentsClient = new NearIntentsClient();

app.post('/submit-intent', async (c) => {
    try {
        const body = await c.req.json();

        if (!body || !body.signed_data) {
            return c.json({ error: 'Missing signed_data' }, 400);
        }

        const userSignedIntent: SignedData = body.signed_data;

        if (userSignedIntent.standard !== 'erc191') {
            return c.json({ error: 'Only erc191 is supported' }, 400);
        }
        if (!userSignedIntent.payload || !userSignedIntent.signature) {
            return c.json({ error: 'Invalid signed_data format' }, 400);
        }

        const result = await intentsClient.processUserIntent(userSignedIntent);
        return c.json({ success: true, result });
    } catch (e: any) {
        return c.json({ error: 'Failed to process intent', message: e?.message || String(e) }, 500);
    }
});

app.get('/health', (c) => c.json({ status: 'ok', service: 'solver-access-point' }));

console.log(`Solver access point listening on port: ${PORT}`);
serve({ fetch: app.fetch, port: PORT, hostname: '0.0.0.0' });