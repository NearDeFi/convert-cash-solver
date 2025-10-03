/**
 * TypeScript types for NEAR Intents WebSocket Client
 * Equivalent to the Python dataclasses and types
 */

export interface QuoteRequest {
    quote_id: string;
    defuse_asset_identifier_in: string;
    defuse_asset_identifier_out: string;
    exact_amount_in?: string;
    exact_amount_out?: string;
    min_deadline_ms?: number;
}

export interface WebSocketMessage {
    jsonrpc: string;
    id?: number;
    method: string;
    params?: any[];
    result?: any;
    error?: any;
}

export interface QuoteResponse {
    quote_id: string;
    quote_output: {
        amount_out: string;
    };
    signed_data: {
        standard: string;
        payload: string;
        signature: string;
    };
}

export interface SignedData {
    standard: string;
    payload: string;
    signature: string;
}

export interface IntentMessage {
    signer_id: string;
    deadline: string;
    verifying_contract: string;
    nonce: string;
    intents: Array<{
        intent: string;
        diff: Record<string, string>;
    }>;
}

