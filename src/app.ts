import { serve } from '@hono/node-server';
import { cors } from 'hono/cors';
import { Hono } from 'hono';
import { addErc191Key } from './key/erc191.js';

// --- constants -------------------------------------------------------------

const PORT = 3000;

// --- app setup -------------------------------------------------------------

const app = new Hono();

app.use('/*', cors());

// for local testing

if (process.env.LOCAL_TESTING) {
    addErc191Key();
}

console.log('Server listening on port: ', PORT);

serve({
    fetch: app.fetch,
    port: PORT,
    hostname: '0.0.0.0',
});
