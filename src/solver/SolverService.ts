/**
 * SolverService (SS)
 *
 * Orchestrates the complete solver flow from listening to quote requests
 * through to repaying liquidity. This service coordinates between:
 *
 * - Message Bus Service (MSB): Handles quote requests and user interactions
 * - Contract Service (CS): Handles smart contract interactions
 * - CEX Service: Handles centralized exchange operations
 *
 * ## Architecture
 *
 * Uses a state machine pattern with a background processing loop that:
 * - Handles hundreds of concurrent swaps without blocking
 * - Processes swaps in batches with configurable concurrency
 * - Retries failed operations automatically
 * - Cleans up completed/failed swaps after configurable TTL
 *
 * ## Flow Overview
 *
 * 1. Listen to message bus for quote requests
 * 2. MSB responds to quotes and handles signing
 * 3. When user accepts quote → Create intent via CS, borrow liquidity
 * 4. Deposit borrowed liquidity to CEX
 * 5. Execute swap on CEX
 * 6. Withdraw swapped tokens from CEX
 * 7. Swap tokens with user via intents
 * 8. Deposit user's liquidity to CEX
 * 9. Swap back to repayable form
 * 10. Repay liquidity via CS
 *
 * ## TODO Items (Dependencies to Complete)
 *
 * - [ ] CEX Service: deposit, swap, withdraw methods
 * - [ ] Bus Service: Event handlers for quote acceptance
 * - [ ] Bus Service: Execute swap with user
 * - [ ] Contract Service: External FT contract calls
 */

import { NearIntentsClient } from '../bus/nearIntentsClient.js';
import {
    ContractService,
    contractService,
} from '../contract/ContractService.js';
import { QuoteRequest, SignedData } from '../bus/types.js';
import {
    SwapOperation,
    SwapState,
    SolverConfig,
    TokenPair,
    SwapResult,
} from './types.js';

// Re-export types
export * from './types.js';

// Default configuration
const DEFAULT_CONFIG: SolverConfig = {
    feePercentage: 0.1, // 10% bridge fee
    protocolFeeRate: 0.00000112, // Protocol fee
    supportedPairs: [
        {
            tokenIn:
                'nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near',
            tokenOut:
                'nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near',
            minAmount: '5000000', // 5 USDT minimum
        },
    ],
    cexEnabled: true,
    cexPollInterval: 5000, // 5 seconds
    intentPollInterval: 10000, // 10 seconds
};

// Extended configuration for concurrent processing
interface ExtendedConfig extends SolverConfig {
    // Maximum concurrent operations per state
    maxConcurrentPerState: number;
    // Processing loop interval
    processingIntervalMs: number;
    // Max retries before marking as failed
    maxRetries: number;
    // TTL for completed/failed swaps before cleanup (ms)
    completedSwapTtlMs: number;
    // Batch size for processing
    batchSize: number;
}

const EXTENDED_DEFAULT_CONFIG: ExtendedConfig = {
    ...DEFAULT_CONFIG,
    maxConcurrentPerState: 50, // Process up to 50 swaps per state concurrently
    processingIntervalMs: 1000, // Process every 1 second
    maxRetries: 3,
    completedSwapTtlMs: 3600000, // 1 hour
    batchSize: 100, // Process 100 swaps per batch
};

// States that require processing (vs terminal states)
const PROCESSABLE_STATES: SwapState[] = [
    'QUOTE_ACCEPTED',
    'INTENT_CREATED',
    'LIQUIDITY_DEPOSITED_TO_CEX',
    'CEX_SWAP_IN_PROGRESS',
    'CEX_SWAP_COMPLETED',
    'CEX_WITHDRAWAL_PENDING',
    'CEX_WITHDRAWAL_COMPLETED',
    'USER_SWAP_PENDING',
    'USER_SWAP_COMPLETED',
    'USER_LIQUIDITY_DEPOSITED',
    'USER_LIQUIDITY_SWAPPED',
    'LIQUIDITY_REPAID',
];

// Terminal states (no further processing needed)
const TERMINAL_STATES: SwapState[] = ['COMPLETED', 'FAILED'];

// Helper function for sleep
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export class SolverService {
    private static instance: SolverService;
    private config: ExtendedConfig;
    private busClient: NearIntentsClient | null = null;
    private contractService: ContractService;

    // Active swap operations indexed by ID
    private activeSwaps: Map<string, SwapOperation> = new Map();

    // Index swaps by state for efficient processing
    private swapsByState: Map<SwapState, Set<string>> = new Map();

    // Track swaps currently being processed to avoid double-processing
    private processingSwaps: Set<string> = new Set();

    // Retry counts per swap
    private retryCount: Map<string, number> = new Map();

    // State
    private isRunning: boolean = false;
    private processingLoopHandle: ReturnType<typeof setInterval> | null = null;

    private constructor(config: Partial<ExtendedConfig> = {}) {
        this.config = { ...EXTENDED_DEFAULT_CONFIG, ...config };
        this.contractService = contractService;

        // Initialize state indexes
        for (const state of [...PROCESSABLE_STATES, ...TERMINAL_STATES]) {
            this.swapsByState.set(state, new Set());
        }
        this.swapsByState.set('PENDING_QUOTE_ACCEPTANCE', new Set());
    }

    /**
     * Get singleton instance of SolverService
     */
    public static getInstance(config?: Partial<ExtendedConfig>): SolverService {
        if (!SolverService.instance) {
            SolverService.instance = new SolverService(config);
        }
        return SolverService.instance;
    }

    /**
     * Initialize and start the solver service
     */
    public async start(): Promise<void> {
        if (this.isRunning) {
            console.log('[SolverService] Already running');
            return;
        }

        console.log('[SolverService] Starting solver service...');

        // Initialize contract service
        await this.contractService.initialize();

        // Initialize bus client
        this.busClient = new NearIntentsClient();

        // Connect to WebSocket
        const connected = await this.busClient.connect();
        if (!connected) {
            throw new Error('[SolverService] Failed to connect to message bus');
        }

        // Subscribe to quotes
        const subscribed = await this.busClient.subscribeToQuotes();
        if (!subscribed) {
            throw new Error('[SolverService] Failed to subscribe to quotes');
        }

        this.isRunning = true;

        // Start the background processing loop
        this.startProcessingLoop();

        // Start cleanup loop
        this.startCleanupLoop();

        console.log('[SolverService] Solver service started');
        console.log(
            `[SolverService] Config: maxConcurrent=${this.config.maxConcurrentPerState}, batchSize=${this.config.batchSize}`,
        );

        // Start listening for bus events (non-blocking)
        this.startBusEventLoop();
    }

    /**
     * Stop the solver service
     */
    public async stop(): Promise<void> {
        if (!this.isRunning) {
            return;
        }

        console.log('[SolverService] Stopping solver service...');
        this.isRunning = false;

        if (this.processingLoopHandle) {
            clearInterval(this.processingLoopHandle);
            this.processingLoopHandle = null;
        }

        if (this.busClient) {
            await this.busClient.disconnect();
            this.busClient = null;
        }

        console.log('[SolverService] Solver service stopped');
        console.log(
            `[SolverService] Final stats: ${this.activeSwaps.size} active swaps`,
        );
    }

    // =========================================================================
    // Background Processing Loop
    // =========================================================================

    /**
     * Start the main processing loop that advances swaps through their states
     */
    private startProcessingLoop(): void {
        this.processingLoopHandle = setInterval(async () => {
            if (!this.isRunning) return;

            try {
                await this.processAllStates();
            } catch (error) {
                console.error('[SolverService] Processing loop error:', error);
            }
        }, this.config.processingIntervalMs);

        console.log(
            `[SolverService] Processing loop started (interval: ${this.config.processingIntervalMs}ms)`,
        );
    }

    /**
     * Process swaps in all processable states
     */
    private async processAllStates(): Promise<void> {
        // Process each state concurrently
        const stateProcessors = PROCESSABLE_STATES.map((state) =>
            this.processSwapsInState(state),
        );

        await Promise.allSettled(stateProcessors);
    }

    /**
     * Process all swaps in a given state with concurrency control
     */
    private async processSwapsInState(state: SwapState): Promise<void> {
        const swapIds = this.swapsByState.get(state);
        if (!swapIds || swapIds.size === 0) return;

        // Get swaps that aren't already being processed
        const toProcess: SwapOperation[] = [];
        for (const swapId of swapIds) {
            if (this.processingSwaps.has(swapId)) continue;
            const swap = this.activeSwaps.get(swapId);
            if (swap && swap.state === state) {
                toProcess.push(swap);
            }
            if (toProcess.length >= this.config.batchSize) break;
        }

        if (toProcess.length === 0) return;

        // Process in batches with concurrency limit
        const batches = this.chunkArray(
            toProcess,
            this.config.maxConcurrentPerState,
        );

        for (const batch of batches) {
            // Mark as processing
            batch.forEach((swap) => this.processingSwaps.add(swap.id));

            // Process batch concurrently
            const results = await Promise.allSettled(
                batch.map((swap) => this.processSwap(swap)),
            );

            // Unmark as processing
            batch.forEach((swap) => this.processingSwaps.delete(swap.id));

            // Log any errors
            results.forEach((result, i) => {
                if (result.status === 'rejected') {
                    console.error(
                        `[SolverService] Error processing swap ${batch[i].id}:`,
                        result.reason,
                    );
                }
            });
        }
    }

    /**
     * Process a single swap based on its current state
     */
    private async processSwap(swap: SwapOperation): Promise<void> {
        const handler = this.getStateHandler(swap.state);
        if (!handler) {
            console.warn(`[SolverService] No handler for state ${swap.state}`);
            return;
        }

        try {
            await handler(swap);
        } catch (error) {
            await this.handleSwapError(swap, error);
        }
    }

    /**
     * Get the handler function for a given state
     */
    private getStateHandler(
        state: SwapState,
    ): ((swap: SwapOperation) => Promise<void>) | null {
        const handlers: Partial<
            Record<SwapState, (swap: SwapOperation) => Promise<void>>
        > = {
            QUOTE_ACCEPTED: (swap) => this.handleQuoteAccepted(swap),
            INTENT_CREATED: (swap) => this.handleIntentCreated(swap),
            LIQUIDITY_DEPOSITED_TO_CEX: (swap) =>
                this.handleLiquidityDeposited(swap),
            CEX_SWAP_IN_PROGRESS: (swap) => this.handleCexSwapInProgress(swap),
            CEX_SWAP_COMPLETED: (swap) => this.handleCexSwapCompleted(swap),
            CEX_WITHDRAWAL_PENDING: (swap) =>
                this.handleCexWithdrawalPending(swap),
            CEX_WITHDRAWAL_COMPLETED: (swap) =>
                this.handleCexWithdrawalCompleted(swap),
            USER_SWAP_PENDING: (swap) => this.handleUserSwapPending(swap),
            USER_SWAP_COMPLETED: (swap) => this.handleUserSwapCompleted(swap),
            USER_LIQUIDITY_DEPOSITED: (swap) =>
                this.handleUserLiquidityDeposited(swap),
            USER_LIQUIDITY_SWAPPED: (swap) =>
                this.handleUserLiquiditySwapped(swap),
            LIQUIDITY_REPAID: (swap) => this.handleLiquidityRepaid(swap),
        };

        return handlers[state] || null;
    }

    // =========================================================================
    // State Handlers
    // =========================================================================

    /**
     * Handle QUOTE_ACCEPTED: Create intent and borrow liquidity
     */
    private async handleQuoteAccepted(swap: SwapOperation): Promise<void> {
        console.log(`[SolverService] Creating intent for swap ${swap.id}`);

        const success = await this.contractService.borrowLiquidity(
            swap.userDepositHash!,
            swap.amountOut,
            `swap-${swap.tokenIn}-${swap.tokenOut}`,
        );

        if (success) {
            // Get the intent index
            const intents = await this.contractService.getIntentsBySolver();
            // Find intent by userDepositHash for accuracy with concurrent intents
            const intentIndex = intents.findIndex(
                (i) => i.user_deposit_hash === swap.userDepositHash,
            );
            if (intentIndex >= 0) {
                swap.intentIndex = intentIndex.toString();
            }
            this.transitionState(swap, 'INTENT_CREATED');
        } else {
            throw new Error('Failed to create intent and borrow liquidity');
        }
    }

    /**
     * Handle INTENT_CREATED: Deposit to CEX
     */
    private async handleIntentCreated(swap: SwapOperation): Promise<void> {
        console.log(`[SolverService] Depositing to CEX for swap ${swap.id}`);

        // TODO: Implement actual CEX deposit
        // const depositResult = await cexService.deposit({
        //     chain: 'near',
        //     token: 'USDT',
        //     amount: swap.amountOut,
        // });
        // swap.cexDepositTxHash = depositResult.txHash;

        // STUB: Simulate deposit
        this.transitionState(swap, 'LIQUIDITY_DEPOSITED_TO_CEX');
    }

    /**
     * Handle LIQUIDITY_DEPOSITED_TO_CEX: Execute CEX swap
     */
    private async handleLiquidityDeposited(swap: SwapOperation): Promise<void> {
        console.log(`[SolverService] Executing CEX swap for ${swap.id}`);

        // TODO: Implement actual CEX swap
        // const swapResult = await cexService.swap({...});
        // swap.cexSwapId = swapResult.orderId;

        // STUB: Simulate swap start
        this.transitionState(swap, 'CEX_SWAP_IN_PROGRESS');
    }

    /**
     * Handle CEX_SWAP_IN_PROGRESS: Poll for swap completion
     */
    private async handleCexSwapInProgress(swap: SwapOperation): Promise<void> {
        // TODO: Implement actual swap status check
        // const status = await cexService.getSwapStatus(swap.cexSwapId);
        // if (!status.completed) return; // Stay in current state

        // STUB: Simulate immediate completion
        this.transitionState(swap, 'CEX_SWAP_COMPLETED');
    }

    /**
     * Handle CEX_SWAP_COMPLETED: Withdraw from CEX
     */
    private async handleCexSwapCompleted(swap: SwapOperation): Promise<void> {
        console.log(`[SolverService] Withdrawing from CEX for swap ${swap.id}`);

        // TODO: Implement actual CEX withdrawal
        // const withdrawResult = await cexService.withdraw({...});
        // swap.cexWithdrawTxHash = withdrawResult.txHash;

        // STUB: Simulate withdrawal start
        this.transitionState(swap, 'CEX_WITHDRAWAL_PENDING');
    }

    /**
     * Handle CEX_WITHDRAWAL_PENDING: Poll for withdrawal completion
     */
    private async handleCexWithdrawalPending(
        swap: SwapOperation,
    ): Promise<void> {
        // TODO: Implement actual withdrawal status check
        // const completed = await checkBitfinexMoves({...});
        // if (!completed) return; // Stay in current state

        // STUB: Simulate immediate completion
        this.transitionState(swap, 'CEX_WITHDRAWAL_COMPLETED');
    }

    /**
     * Handle CEX_WITHDRAWAL_COMPLETED: Execute user swap
     */
    private async handleCexWithdrawalCompleted(
        swap: SwapOperation,
    ): Promise<void> {
        console.log(`[SolverService] Executing user swap for ${swap.id}`);

        this.transitionState(swap, 'USER_SWAP_PENDING');

        // Execute the swap with user via intents
        if (this.busClient && swap.userSignedIntent) {
            try {
                await this.busClient.processUserIntent(swap.userSignedIntent);
                this.transitionState(swap, 'USER_SWAP_COMPLETED');
            } catch (error) {
                throw new Error(`User swap failed: ${error}`);
            }
        } else {
            // STUB: Simulate swap completion
            this.transitionState(swap, 'USER_SWAP_COMPLETED');
        }
    }

    /**
     * Handle USER_SWAP_PENDING: Wait for user swap to complete
     */
    private async handleUserSwapPending(swap: SwapOperation): Promise<void> {
        // This state should transition quickly via handleCexWithdrawalCompleted
        // If we're here, something may be stuck - check status
        // STUB: This shouldn't normally be reached
    }

    /**
     * Handle USER_SWAP_COMPLETED: Deposit user liquidity to CEX
     */
    private async handleUserSwapCompleted(swap: SwapOperation): Promise<void> {
        console.log(
            `[SolverService] Depositing user liquidity for swap ${swap.id}`,
        );

        // TODO: Implement user liquidity deposit to CEX

        // STUB: Simulate deposit
        this.transitionState(swap, 'USER_LIQUIDITY_DEPOSITED');
    }

    /**
     * Handle USER_LIQUIDITY_DEPOSITED: Swap back to repayable form
     */
    private async handleUserLiquidityDeposited(
        swap: SwapOperation,
    ): Promise<void> {
        console.log(
            `[SolverService] Swapping user liquidity back for ${swap.id}`,
        );

        // TODO: Implement swap back to vault token

        // STUB: Simulate swap
        this.transitionState(swap, 'USER_LIQUIDITY_SWAPPED');
    }

    /**
     * Handle USER_LIQUIDITY_SWAPPED: Repay to vault
     */
    private async handleUserLiquiditySwapped(
        swap: SwapOperation,
    ): Promise<void> {
        console.log(`[SolverService] Repaying liquidity for swap ${swap.id}`);

        if (!swap.intentIndex) {
            throw new Error('No intent index found');
        }

        // Calculate repayment amount (principal + yield)
        const borrowAmount = BigInt(swap.amountOut);
        const yieldAmount = borrowAmount / 100n; // 1% minimum yield
        const repayAmount = borrowAmount + yieldAmount;

        const success = await this.contractService.repayLiquidity({
            intentIndex: swap.intentIndex,
            amount: repayAmount.toString(),
        });

        if (success) {
            this.transitionState(swap, 'LIQUIDITY_REPAID');
        } else {
            throw new Error('Failed to repay liquidity');
        }
    }

    /**
     * Handle LIQUIDITY_REPAID: Mark as complete
     */
    private async handleLiquidityRepaid(swap: SwapOperation): Promise<void> {
        console.log(`[SolverService] Swap ${swap.id} completed successfully`);

        // Update intent state on contract
        const solverId = await this.contractService.getSolverId();
        await this.contractService.updateIntentState({
            solverId,
            state: 'StpLiquidityReturned',
        });

        this.transitionState(swap, 'COMPLETED');
    }

    // =========================================================================
    // State Management
    // =========================================================================

    /**
     * Transition a swap to a new state
     */
    private transitionState(swap: SwapOperation, newState: SwapState): void {
        const oldState = swap.state;

        // Update state indexes
        this.swapsByState.get(oldState)?.delete(swap.id);
        this.swapsByState.get(newState)?.add(swap.id);

        // Update swap
        swap.state = newState;
        swap.updatedAt = Date.now();

        // Reset retry count on successful transition
        this.retryCount.delete(swap.id);

        console.log(
            `[SolverService] Swap ${swap.id}: ${oldState} -> ${newState}`,
        );
    }

    /**
     * Handle errors during swap processing
     */
    private async handleSwapError(
        swap: SwapOperation,
        error: unknown,
    ): Promise<void> {
        const currentRetries = this.retryCount.get(swap.id) || 0;
        const errorMsg = error instanceof Error ? error.message : String(error);

        if (currentRetries < this.config.maxRetries) {
            // Increment retry count and keep in current state for retry
            this.retryCount.set(swap.id, currentRetries + 1);
            console.warn(
                `[SolverService] Swap ${swap.id} error (retry ${
                    currentRetries + 1
                }/${this.config.maxRetries}): ${errorMsg}`,
            );
        } else {
            // Max retries exceeded, mark as failed
            console.error(
                `[SolverService] Swap ${swap.id} failed after ${this.config.maxRetries} retries: ${errorMsg}`,
            );
            swap.error = errorMsg;
            this.transitionState(swap, 'FAILED');
        }
    }

    // =========================================================================
    // Public API - Quote Handling
    // =========================================================================

    /**
     * Called when a quote request is received from the message bus.
     * Non-blocking - just creates the swap operation for tracking.
     *
     * @param quoteRequest - The quote request from the user
     */
    public onQuoteRequest(quoteRequest: QuoteRequest): void {
        // Check if this is a supported pair
        if (!this.isSupportedPair(quoteRequest)) {
            return;
        }

        console.log(
            `[SolverService] Processing quote request: ${quoteRequest.quote_id}`,
        );

        // Create swap operation to track this quote
        const swapOp: SwapOperation = {
            id: this.generateSwapId(),
            quoteId: quoteRequest.quote_id,
            quoteRequest,
            amountIn: quoteRequest.exact_amount_in || '0',
            amountOut: this.calculateAmountOut(
                quoteRequest.exact_amount_in || '0',
            ),
            tokenIn: quoteRequest.defuse_asset_identifier_in,
            tokenOut: quoteRequest.defuse_asset_identifier_out,
            state: 'PENDING_QUOTE_ACCEPTANCE',
            createdAt: Date.now(),
            updatedAt: Date.now(),
        };

        this.addSwap(swapOp);
        console.log(
            `[SolverService] Created swap operation: ${swapOp.id} (total active: ${this.activeSwaps.size})`,
        );
    }

    /**
     * Called when a user accepts a quote and signs their intent.
     * Non-blocking - transitions state and lets processing loop handle it.
     *
     * @param quoteId - The quote ID that was accepted
     * @param userSignedIntent - The user's signed intent
     */
    public onQuoteAccepted(
        quoteId: string,
        userSignedIntent: SignedData,
    ): void {
        // Find the swap operation
        const swapOp = this.findSwapByQuoteId(quoteId);
        if (!swapOp) {
            // Create a new swap if we don't have one (e.g., quote was from another instance)
            console.log(
                `[SolverService] Creating new swap for accepted quote ${quoteId}`,
            );
            const newSwap: SwapOperation = {
                id: this.generateSwapId(),
                quoteId,
                quoteRequest: {} as QuoteRequest, // Will be populated later if needed
                userSignedIntent,
                amountIn: '0', // Will be extracted from intent
                amountOut: '0',
                tokenIn: '',
                tokenOut: '',
                state: 'QUOTE_ACCEPTED',
                createdAt: Date.now(),
                updatedAt: Date.now(),
                userDepositHash: this.generateUserDepositHash(
                    quoteId,
                    userSignedIntent,
                ),
            };
            this.addSwap(newSwap);
            return;
        }

        console.log(
            `[SolverService] Quote ${quoteId} accepted, transitioning to QUOTE_ACCEPTED`,
        );

        swapOp.userSignedIntent = userSignedIntent;
        swapOp.userDepositHash = this.generateUserDepositHash(
            quoteId,
            userSignedIntent,
        );

        // Transition state - processing loop will handle the rest
        this.transitionState(swapOp, 'QUOTE_ACCEPTED');
    }

    // =========================================================================
    // Swap Management
    // =========================================================================

    /**
     * Add a swap to the tracking maps
     */
    private addSwap(swap: SwapOperation): void {
        this.activeSwaps.set(swap.id, swap);
        this.swapsByState.get(swap.state)?.add(swap.id);
    }

    /**
     * Remove a swap from tracking
     */
    private removeSwap(swapId: string): void {
        const swap = this.activeSwaps.get(swapId);
        if (swap) {
            this.swapsByState.get(swap.state)?.delete(swapId);
            this.activeSwaps.delete(swapId);
            this.retryCount.delete(swapId);
            this.processingSwaps.delete(swapId);
        }
    }

    /**
     * Start cleanup loop to remove old completed/failed swaps
     */
    private startCleanupLoop(): void {
        setInterval(() => {
            if (!this.isRunning) return;

            const now = Date.now();
            const toRemove: string[] = [];

            for (const [swapId, swap] of this.activeSwaps) {
                if (
                    TERMINAL_STATES.includes(swap.state) &&
                    now - swap.updatedAt > this.config.completedSwapTtlMs
                ) {
                    toRemove.push(swapId);
                }
            }

            if (toRemove.length > 0) {
                console.log(
                    `[SolverService] Cleaning up ${toRemove.length} old swaps`,
                );
                toRemove.forEach((id) => this.removeSwap(id));
            }
        }, 60000); // Check every minute
    }

    /**
     * Start bus event loop (non-blocking)
     */
    private startBusEventLoop(): void {
        if (!this.busClient) return;

        // Run in background without blocking
        this.busClient.listenForEvents().catch((error) => {
            console.error('[SolverService] Bus event loop error:', error);
        });
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /**
     * Check if a quote request is for a supported token pair
     */
    private isSupportedPair(quoteRequest: QuoteRequest): boolean {
        return this.config.supportedPairs.some(
            (pair) =>
                pair.tokenIn === quoteRequest.defuse_asset_identifier_in &&
                pair.tokenOut === quoteRequest.defuse_asset_identifier_out &&
                BigInt(quoteRequest.exact_amount_in || '0') >=
                    BigInt(pair.minAmount),
        );
    }

    /**
     * Calculate output amount after fees
     */
    private calculateAmountOut(amountIn: string): string {
        const input = BigInt(amountIn);
        const fee =
            (input * BigInt(Math.floor(this.config.feePercentage * 1000000))) /
            1000000n;
        return (input - fee).toString();
    }

    /**
     * Generate a unique swap ID
     */
    private generateSwapId(): string {
        return `swap-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    }

    /**
     * Generate a user deposit hash from quote and intent
     */
    private generateUserDepositHash(
        quoteId: string,
        intent: SignedData,
    ): string {
        // TODO: Generate proper hash from intent data
        return `hash-${quoteId}-${Date.now()}`;
    }

    /**
     * Find a swap operation by quote ID
     */
    private findSwapByQuoteId(quoteId: string): SwapOperation | undefined {
        for (const [, swapOp] of this.activeSwaps) {
            if (swapOp.quoteId === quoteId) {
                return swapOp;
            }
        }
        return undefined;
    }

    /**
     * Chunk an array into smaller arrays
     */
    private chunkArray<T>(array: T[], size: number): T[][] {
        const chunks: T[][] = [];
        for (let i = 0; i < array.length; i += size) {
            chunks.push(array.slice(i, i + size));
        }
        return chunks;
    }

    // =========================================================================
    // Public Query Methods
    // =========================================================================

    /**
     * Get all active swap operations
     */
    public getActiveSwaps(): SwapOperation[] {
        return Array.from(this.activeSwaps.values());
    }

    /**
     * Get count of active swaps
     */
    public getActiveSwapCount(): number {
        return this.activeSwaps.size;
    }

    /**
     * Get a specific swap operation by ID
     */
    public getSwap(swapId: string): SwapOperation | undefined {
        return this.activeSwaps.get(swapId);
    }

    /**
     * Get swaps in a specific state
     */
    public getSwapsByState(state: SwapState): SwapOperation[] {
        const swapIds = this.swapsByState.get(state);
        if (!swapIds) return [];
        return Array.from(swapIds)
            .map((id) => this.activeSwaps.get(id))
            .filter((swap): swap is SwapOperation => swap !== undefined);
    }

    /**
     * Get counts per state for monitoring
     */
    public getStateStats(): Record<string, number> {
        const stats: Record<string, number> = {};
        for (const [state, ids] of this.swapsByState) {
            stats[state] = ids.size;
        }
        return stats;
    }

    /**
     * Get overall statistics
     */
    public getStats(): {
        totalActive: number;
        processing: number;
        byState: Record<string, number>;
    } {
        return {
            totalActive: this.activeSwaps.size,
            processing: this.processingSwaps.size,
            byState: this.getStateStats(),
        };
    }
}

// Export singleton instance
export const solverService = SolverService.getInstance();
