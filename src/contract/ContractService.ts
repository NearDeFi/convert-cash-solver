/**
 * ContractService (CS)
 *
 * Handles all agent-related smart contract interactions for the solver.
 * Uses the shade-agent-js library to make authenticated calls to the vault contract.
 *
 * Contract Methods:
 * - new_intent: Creates a new intent and borrows liquidity from the vault
 * - update_intent_state: Updates the state of an existing intent
 * - get_intents_by_solver: Retrieves all intents for a specific solver
 * - repay: Repays borrowed liquidity (via ft_transfer_call to vault)
 *
 * @see /contracts/proxy/tests for integration test examples
 */

import { agentCall, agentView, agentAccountId } from '@neardefi/shade-agent-js';
import {
    Intent,
    IntentState,
    NewIntentParams,
    RepayParams,
    UpdateIntentStateParams,
} from './types.js';

// Re-export types for convenience
export * from './types.js';

export class ContractService {
    private static instance: ContractService;
    private solverId: string | null = null;

    private constructor() {}

    /**
     * Get singleton instance of ContractService
     */
    public static getInstance(): ContractService {
        if (!ContractService.instance) {
            ContractService.instance = new ContractService();
        }
        return ContractService.instance;
    }

    /**
     * Initialize the service and cache the solver ID
     */
    public async initialize(): Promise<void> {
        const { accountId } = await agentAccountId();
        this.solverId = accountId;
        console.log(
            `[ContractService] Initialized with solver ID: ${this.solverId}`,
        );
    }

    /**
     * Get the current solver's account ID
     */
    public async getSolverId(): Promise<string> {
        if (!this.solverId) {
            await this.initialize();
        }
        return this.solverId!;
    }

    /**
     * Creates a new intent and borrows liquidity from the vault.
     *
     * The vault will transfer tokens to the solver's deposit address.
     *
     * @param params - Intent creation parameters
     * @returns true if intent was created successfully
     *
     * @example
     * ```typescript
     * const success = await contractService.newIntent({
     *     intentData: 'swap-eth-tron-usdt',
     *     solverDepositAddress: 'solver.near',
     *     userDepositHash: 'hash-123',
     *     amount: '5000000' // Optional: 5 USDC
     * });
     * ```
     */
    public async newIntent(params: NewIntentParams): Promise<boolean> {
        const { intentData, solverDepositAddress, userDepositHash, amount } =
            params;

        try {
            const args: Record<string, string> = {
                intent_data: intentData,
                _solver_deposit_address: solverDepositAddress,
                user_deposit_hash: userDepositHash,
            };

            // Add optional amount if specified
            if (amount) {
                args.amount = amount;
            }

            const res = await agentCall({
                methodName: 'new_intent',
                args,
            });

            if ((res as any).error) {
                throw new Error((res as any).error);
            }

            console.log(
                `[ContractService] Intent created with hash ${userDepositHash}`,
            );
            return true;
        } catch (e) {
            console.error('[ContractService] Error creating intent:', e);
            return false;
        }
    }

    /**
     * Updates the state of an existing intent.
     *
     * State transitions follow the solver lifecycle:
     * StpLiquidityBorrowed → StpLiquidityDeposited → StpLiquidityWithdrawn →
     * StpIntentAccountCredited → SwapCompleted → UserLiquidityDeposited →
     * StpLiquidityReturned
     *
     * @param params - State update parameters
     * @returns true if state was updated successfully
     */
    public async updateIntentState(
        params: UpdateIntentStateParams,
    ): Promise<boolean> {
        const { solverId, state } = params;

        try {
            const res = await agentCall({
                methodName: 'update_intent_state',
                args: {
                    solver_id: solverId,
                    state,
                },
            });

            if ((res as any).error) {
                throw new Error((res as any).error);
            }

            console.log(
                `[ContractService] State updated to ${state} for solver ${solverId}`,
            );
            return true;
        } catch (e) {
            console.error('[ContractService] Error updating intent state:', e);
            return false;
        }
    }

    /**
     * Retrieves all intents for a specific solver.
     *
     * @param solverId - The solver's account ID
     * @returns Array of intents for the solver
     */
    public async getIntentsBySolver(solverId?: string): Promise<Intent[]> {
        const id = solverId || (await this.getSolverId());

        try {
            const intents = await agentView({
                methodName: 'get_intents_by_solver',
                args: {
                    solver_id: id,
                },
            });

            if ((intents as any).error) {
                return [];
            }

            return intents as Intent[];
        } catch (e) {
            console.error('[ContractService] Error fetching intents:', e);
            return [];
        }
    }

    /**
     * Repays borrowed liquidity to the vault.
     *
     * This is done via ft_transfer_call to the vault contract with a repay message.
     * Minimum repayment is principal + 1% yield.
     *
     * @param params - Repayment parameters
     * @returns true if repayment was successful
     *
     * @example
     * ```typescript
     * // Repay with 1% yield
     * const borrowAmount = 100_000_000n; // 100 USDC
     * const yield = borrowAmount / 100n; // 1% yield
     * const repayAmount = borrowAmount + yield;
     *
     * const success = await contractService.repayLiquidity({
     *     intentIndex: '0',
     *     amount: repayAmount.toString()
     * });
     * ```
     */
    public async repayLiquidity(params: RepayParams): Promise<boolean> {
        const { intentIndex, amount } = params;

        try {
            // The repayment is done via ft_transfer_call to the vault
            // The msg contains: { "repay": { "intent_index": "0" } }
            const msg = JSON.stringify({
                repay: {
                    intent_index: intentIndex,
                },
            });

            const res = await agentCall({
                methodName: 'ft_transfer_call',
                args: {
                    receiver_id: process.env.NEAR_CONTRACT_ID,
                    amount,
                    msg,
                },
                // Note: ft_transfer_call requires 1 yoctoNEAR deposit
                // This should be handled by the shade-agent-js library
            });

            if ((res as any).error) {
                throw new Error((res as any).error);
            }

            console.log(
                `[ContractService] Repaid ${amount} for intent index ${intentIndex}`,
            );
            return true;
        } catch (e) {
            console.error('[ContractService] Error repaying liquidity:', e);
            return false;
        }
    }

    /**
     * Borrows liquidity from the vault by creating a new intent.
     * This is a convenience method that combines newIntent with the borrow action.
     *
     * @param userDepositHash - Unique hash identifying the user's deposit
     * @param amount - Amount to borrow (in smallest units, e.g., 1000000 = 1 USDC)
     * @param intentData - Additional data for the intent
     * @returns true if borrow was successful
     */
    public async borrowLiquidity(
        userDepositHash: string,
        amount: string,
        intentData: string = 'solver-borrow',
    ): Promise<boolean> {
        const solverId = await this.getSolverId();

        return this.newIntent({
            intentData,
            solverDepositAddress: solverId,
            userDepositHash,
            amount,
        });
    }

    /**
     * Get the total assets currently in the vault.
     * Useful for checking available liquidity before borrowing.
     *
     * @returns Total assets as a string
     */
    public async getTotalAssets(): Promise<string> {
        try {
            const result = await agentView({
                methodName: 'total_assets',
                args: {},
            });

            if ((result as any).error) {
                throw new Error((result as any).error);
            }

            return result as string;
        } catch (e) {
            console.error('[ContractService] Error fetching total assets:', e);
            return '0';
        }
    }

    /**
     * Get the solver's current token balance.
     *
     * @param tokenContract - The FT contract address
     * @returns Balance as a string
     */
    public async getSolverBalance(tokenContract: string): Promise<string> {
        const solverId = await this.getSolverId();

        try {
            // TODO: This should call the FT contract's ft_balance_of method
            // For now, stub this out - implementation depends on how shade-agent-js
            // handles calls to external contracts
            console.log(
                `[ContractService] Getting balance for ${solverId} on ${tokenContract}`,
            );

            // Stub: actual implementation would be:
            // const balance = await agentView({
            //     contractId: tokenContract,
            //     methodName: 'ft_balance_of',
            //     args: { account_id: solverId },
            // });

            return '0';
        } catch (e) {
            console.error(
                '[ContractService] Error fetching solver balance:',
                e,
            );
            return '0';
        }
    }
}

// Export singleton instance
export const contractService = ContractService.getInstance();
