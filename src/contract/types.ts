/**
 * Contract Service Types
 * Types for interacting with the solver vault smart contract
 */

// Intent state transitions
export type IntentState =
    | 'StpLiquidityBorrowed'
    | 'StpLiquidityDeposited'
    | 'StpLiquidityWithdrawn'
    | 'StpIntentAccountCredited'
    | 'SwapCompleted'
    | 'UserLiquidityBorrowed'
    | 'UserLiquidityDeposited'
    | 'StpLiquidityReturned';

export interface Intent {
    solver_id: string;
    created: number;
    state: IntentState;
    intent_data: string;
    user_deposit_hash: string;
    borrow_amount?: string;
    repayment_amount?: string;
}

export interface NewIntentParams {
    intentData: string;
    solverDepositAddress: string;
    userDepositHash: string;
    amount?: string; // Optional: specific borrow amount
}

export interface RepayParams {
    intentIndex: string;
    amount: string;
}

export interface UpdateIntentStateParams {
    solverId: string;
    state: IntentState;
}
