/**
 * Solver Service Types
 * Types for the solver orchestration service
 */

import { QuoteRequest, SignedData } from '../bus/types.js';

/**
 * Represents a swap operation being processed by the solver
 */
export interface SwapOperation {
    id: string; // Unique identifier for this swap operation
    quoteId: string;
    quoteRequest: QuoteRequest;
    userSignedIntent?: SignedData;

    // Amounts
    amountIn: string;
    amountOut: string;

    // Token info
    tokenIn: string; // defuse asset identifier
    tokenOut: string; // defuse asset identifier

    // State tracking
    state: SwapState;
    createdAt: number;
    updatedAt: number;

    // CEX tracking
    cexDepositTxHash?: string;
    cexSwapId?: string;
    cexWithdrawTxHash?: string;

    // Intent tracking
    intentIndex?: string;
    userDepositHash?: string;

    // Error tracking
    error?: string;
}

export type SwapState =
    | 'PENDING_QUOTE_ACCEPTANCE' // Quote sent, waiting for user to accept
    | 'QUOTE_ACCEPTED' // User accepted the quote
    | 'INTENT_CREATED' // Intent created on contract, liquidity borrowed
    | 'LIQUIDITY_DEPOSITED_TO_CEX' // Borrowed liquidity deposited to CEX
    | 'CEX_SWAP_IN_PROGRESS' // Swap executing on CEX
    | 'CEX_SWAP_COMPLETED' // CEX swap completed
    | 'CEX_WITHDRAWAL_PENDING' // Withdrawal from CEX initiated
    | 'CEX_WITHDRAWAL_COMPLETED' // Funds withdrawn from CEX
    | 'USER_SWAP_PENDING' // Swap with user pending
    | 'USER_SWAP_COMPLETED' // Swapped tokens with user
    | 'USER_LIQUIDITY_DEPOSITED' // User's liquidity deposited to CEX
    | 'USER_LIQUIDITY_SWAPPED' // User's liquidity swapped back
    | 'LIQUIDITY_REPAID' // Liquidity repaid to vault
    | 'COMPLETED' // Full cycle complete
    | 'FAILED'; // Operation failed

export interface SolverConfig {
    // Fee configuration
    feePercentage: number; // Bridge fee (paid by user)
    protocolFeeRate: number; // Protocol fee (paid by solver)

    // Token pair configuration
    supportedPairs: TokenPair[];

    // CEX configuration
    cexEnabled: boolean;

    // Polling intervals (ms)
    cexPollInterval: number;
    intentPollInterval: number;
}

export interface TokenPair {
    tokenIn: string; // defuse asset identifier
    tokenOut: string; // defuse asset identifier
    minAmount: string; // Minimum amount to process
    maxAmount?: string; // Maximum amount to process (optional)
}

export interface SwapResult {
    success: boolean;
    swapId?: string;
    txHash?: string;
    error?: string;
}
