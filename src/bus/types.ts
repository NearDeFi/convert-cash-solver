/**
 * TypeScript types for NEAR Intents WebSocket Client
 * Equivalent to the Python dataclasses and types
 */

export interface TrustedMetadata {
    source?: string;
    upstream_metadata?: {
        traceparent?: string;
        partner_id?: string;
    };
    upstream_trusted_metadata?: {
        source?: string;
        quote_type?: string;
        partner_id?: string;
        quote_request_data?: {
            dry?: boolean;
            slippageTolerance?: number;
        };
    };
}

export interface QuoteRequest {
    quote_id: string;
    defuse_asset_identifier_in: string;
    defuse_asset_identifier_out: string;
    exact_amount_in?: string;
    exact_amount_out?: string;
    min_deadline_ms?: number;
    min_wait_ms?: number;
    max_wait_ms?: number;
    protocol_fee_included?: boolean;
    trusted_metadata?: TrustedMetadata;
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

