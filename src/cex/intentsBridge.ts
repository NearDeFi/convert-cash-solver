/**
 * NEAR Intents Bridge Service
 * Implements the Passive Deposit/Withdrawal Service API
 * Documentation: https://docs.near-intents.org/near-intents/market-makers/passive-deposit-withdrawal-service
 */

export interface SupportedToken {
    defuse_asset_identifier: string; // CHAIN_TYPE:CHAIN_ID:ADDRESS
    near_token_id: string;
    decimals: number;
    asset_name: string;
    min_deposit_amount: string;
    min_withdrawal_amount: string;
    withdrawal_fee: string;
}

export interface SupportedTokensResponse {
    tokens: SupportedToken[];
}

export interface DepositAddressResponse {
    address: string;
    chain: string;
}

export interface RecentDeposit {
    tx_hash: string;
    chain: string;
    defuse_asset_identifier: string;
    decimals: number;
    amount: string;
    account_id: string;
    address: string;
    status: 'COMPLETED' | 'PENDING' | 'FAILED';
}

export interface RecentDepositsResponse {
    deposits: RecentDeposit[];
}

export interface IntentsBridgeConfig {
    accountId: string; // NEAR account ID registered in Intents
    jwtToken?: string; // Optional JWT token for authentication
}

export class IntentsBridgeService {
    private readonly baseUrl = 'https://bridge.chaindefuser.com/rpc';
    private supportedTokensCache: SupportedToken[] | null = null;
    private cacheExpiry: number = 0;
    private readonly CACHE_DURATION = 5 * 60 * 1000; // 5 minutes

    constructor(private readonly config: IntentsBridgeConfig) {}

    /**
     * Makes a JSON-RPC request to the bridge service
     */
    private async makeRequest<T = any>(method: string, params: any[]): Promise<T> {
        const headers: Record<string, string> = {
            'Content-Type': 'application/json',
        };

        // Add JWT token if available
        if (this.config.jwtToken) {
            headers['Authorization'] = `Bearer ${this.config.jwtToken}`;
        }

        const request = {
            jsonrpc: '2.0',
            id: 1,
            method,
            params,
        };

        try {
            const response = await fetch(this.baseUrl, {
                method: 'POST',
                headers,
                body: JSON.stringify(request),
            });

            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }

            const result = (await response.json()) as {
                jsonrpc: string;
                id: number;
                result?: T;
                error?: any;
            };

            if (result.error) {
                throw new Error(
                    `Bridge service error: ${result.error.message || JSON.stringify(result.error)}`,
                );
            }

            if (!result.result) {
                throw new Error('Bridge service returned no result');
            }

            return result.result;
        } catch (error) {
            const errorMessage =
                error instanceof Error ? error.message : String(error);
            throw new Error(`Failed to call bridge service: ${errorMessage}`);
        }
    }

    /**
     * Gets the list of tokens supported by the service
     * @param chains Optional chain filter (e.g., ["eth:1", "tron:mainnet"])
     */
    async getSupportedTokens(chains?: string[]): Promise<SupportedToken[]> {
        // Check cache
        if (this.supportedTokensCache && Date.now() < this.cacheExpiry) {
            if (!chains) {
                return this.supportedTokensCache;
            }
            // Filter by chains if specified
            return this.supportedTokensCache.filter((token) => {
                const [chainType, chainId] = token.defuse_asset_identifier.split(':');
                const chainKey = `${chainType}:${chainId}`;
                return chains.includes(chainKey);
            });
        }

        const params = chains ? [{ chains }] : [{}];
        const result = await this.makeRequest<SupportedTokensResponse>(
            'supported_tokens',
            params,
        );

        // Update cache
        this.supportedTokensCache = result.tokens;
        this.cacheExpiry = Date.now() + this.CACHE_DURATION;

        return result.tokens;
    }

    /**
     * Gets the deposit address for a specific account, chain, and optionally token
     * @param chain Network type and chain id (e.g., "eth:1", "tron:mainnet")
     * @param token Optional token identifier for ERC-20 tokens (for native tokens like SOL, this should be omitted or "native")
     */
    async getDepositAddress(
        chain: string,
        token?: string,
    ): Promise<DepositAddressResponse> {
        const params: any = {
            account_id: this.config.accountId,
            chain,
        };

        // Only add token parameter if it's provided and not "native"
        // For native tokens like SOL, we should omit the token parameter
        if (token && token !== 'native') {
            params.token = token;
        }

        return await this.makeRequest<DepositAddressResponse>(
            'deposit_address',
            [params],
        );
    }

    /**
     * Gets recent deposits for the configured account
     * @param chain Network type and chain id (e.g., "eth:1", "tron:mainnet")
     */
    async getRecentDeposits(chain: string): Promise<RecentDeposit[]> {
        const params = {
            account_id: this.config.accountId,
            chain,
        };

        const result = await this.makeRequest<RecentDepositsResponse>(
            'recent_deposits',
            [params],
        );
        return result.deposits || [];
    }

    /**
     * Maps Binance network codes to Intents chain format
     * @param binanceNetwork Binance network code (e.g., "ETH", "TRX", "BSC")
     * @returns Intents chain format (e.g., "eth:1", "tron:mainnet", "bsc:56")
     */
    static mapBinanceNetworkToIntentsChain(binanceNetwork: string): string {
        const network = binanceNetwork.toUpperCase();
        const mapping: Record<string, string> = {
            ETH: 'eth:1',
            ERC20: 'eth:1', // Alternative name for Ethereum
            TRX: 'tron:mainnet',
            TRON: 'tron:mainnet', // Alternative name
            BSC: 'bsc:56',
            BNB: 'bsc:56', // Alternative name
            MATIC: 'polygon:137',
            POLYGON: 'polygon:137', // Alternative name
            AVAX: 'avalanche:43114',
            AVALANCHE: 'avalanche:43114', // Alternative name
            SOL: 'sol:mainnet',
            SOLANA: 'sol:mainnet', // Alternative name
            NEAR: 'near:mainnet',
        };

        const mapped = mapping[network];
        if (!mapped) {
            throw new Error(
                `Network ${binanceNetwork} is not mapped. Available networks: ${Object.keys(mapping).join(', ')}`,
            );
        }

        return mapped;
    }

    /**
     * Finds the defuse asset identifier for a token symbol and chain
     * @param symbol Token symbol (e.g., "USDT", "USDC")
     * @param chain Intents chain format (e.g., "eth:1", "tron:mainnet")
     * @param supportedTokens List of supported tokens
     * @returns Defuse asset identifier or null if not found
     */
    static findDefuseAssetIdentifier(
        symbol: string,
        chain: string,
        supportedTokens: SupportedToken[],
    ): string | null {
        const upperSymbol = symbol.toUpperCase();
        const [chainType, chainId] = chain.split(':');

        // Find token matching symbol and chain
        const token = supportedTokens.find((t) => {
            const parts = t.defuse_asset_identifier.split(':');
            if (parts.length < 3) return false;

            const tokenChainType = parts[0];
            const tokenChainId = parts[1];
            const tokenSymbol = t.asset_name.toUpperCase();

            return (
                tokenChainType === chainType &&
                tokenChainId === chainId &&
                tokenSymbol === upperSymbol
            );
        });

        return token?.defuse_asset_identifier || null;
    }
}

