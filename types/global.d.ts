// Global type definitions for the convert-cash-solver project

declare global {
    namespace NodeJS {
        interface ProcessEnv {
            NODE_ENV?: 'development' | 'production' | 'test';
            PORT?: string;
            // Add other environment variables as needed
        }
    }
}

// Common types used across the project
export interface ApiResponse<T = any> {
    success: boolean;
    data?: T;
    error?: string;
    message?: string;
}

export interface ChainConfig {
    name: string;
    rpcUrl: string;
    chainId: number;
}

export interface AssetInfo {
    symbol: string;
    decimals: number;
    address?: string;
}

export {};
