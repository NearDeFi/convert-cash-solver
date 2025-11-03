/**
 * NEAR Client for calling smart contracts
 * Uses the official @near-js packages for interacting with NEAR blockchain
 */
import * as dotenv from 'dotenv';
import { Account } from "@near-js/accounts";
import { JsonRpcProvider } from "@near-js/providers";
import { KeyPairSigner } from "@near-js/signers";
import { NEAR } from "@near-js/tokens";
dotenv.config({ path: './env.development.local' });
export class NearClient {
    constructor() {
        // Get environment variables
        const accountId = process.env.NEAR_ACCOUNT_ID;
        const privateKey = process.env.NEAR_PRIVATE_KEY;
        if (!accountId) {
            throw new Error('NEAR_ACCOUNT_ID must be set in .env file');
        }
        if (!privateKey) {
            throw new Error('NEAR_PRIVATE_KEY must be set in .env file');
        }
        this.accountId = accountId;
        // Initialize provider for mainnet
        this.provider = new JsonRpcProvider({
            url: "https://rpc.mainnet.near.org"
        });
        // Create signer directly from private key string
        // The private key format should be "ed25519:xxxxx..." or similar
        this.signer = KeyPairSigner.fromSecretKey(privateKey);
        // Create account instance
        this.account = new Account(accountId, this.provider, this.signer);
        console.log(`✅ NEAR Client initialized for account: ${accountId}`);
    }
    /**
     * Call a function on a NEAR smart contract
     * @param contractId The contract ID to call
     * @param methodName The method name to call
     * @param args The arguments to pass to the method
     * @param deposit Optional deposit amount in NEAR (string or bigint in yoctoNEAR)
     * @param gas Optional gas amount in TGas (defaults to 30 TGas)
     */
    async callFunction(contractId, methodName, args, deposit = "0", gas = "30000000000000" // 30 TGas
    ) {
        try {
            console.log(`\n📞 Calling NEAR contract:`);
            console.log(`   Contract: ${contractId}`);
            console.log(`   Method: ${methodName}`);
            console.log(`   Args: ${JSON.stringify(args, null, 2)}`);
            if (deposit !== "0" && deposit !== BigInt(0)) {
                const depositAmount = typeof deposit === 'string'
                    ? deposit
                    : deposit.toString();
                console.log(`   Deposit: ${depositAmount} NEAR`);
            }
            console.log(`   Gas: ${gas} (${parseInt(gas) / 1e12} TGas)`);
            const result = await this.account.callFunction({
                contractId,
                methodName,
                args,
                deposit: deposit === "0" ? BigInt(0) : (typeof deposit === 'string' ? BigInt(deposit) : deposit),
                gas,
            });
            console.log(`✅ Function call successful!`);
            console.log(`   Result: ${JSON.stringify(result, null, 2)}`);
            return result;
        }
        catch (error) {
            console.error(`❌ Failed to call function: ${error}`);
            throw error;
        }
    }
    /**
     * View a function on a NEAR smart contract (read-only)
     * @param contractId The contract ID to query
     * @param methodName The method name to query
     * @param args The arguments to pass to the method
     */
    async viewFunction(contractId, methodName, args = {}) {
        try {
            console.log(`\n👁️  Viewing NEAR contract:`);
            console.log(`   Contract: ${contractId}`);
            console.log(`   Method: ${methodName}`);
            console.log(`   Args: ${JSON.stringify(args, null, 2)}`);
            const result = await this.account.viewFunction({
                contractId,
                methodName,
                args,
            });
            console.log(`✅ View function call successful!`);
            console.log(`   Result: ${JSON.stringify(result, null, 2)}`);
            return result;
        }
        catch (error) {
            console.error(`❌ Failed to view function: ${error}`);
            throw error;
        }
    }
    /**
     * Get account balance
     */
    async getBalance() {
        try {
            const balance = await this.account.getBalance();
            const balanceStr = typeof balance === 'string' ? balance : balance.toString();
            return balanceStr;
        }
        catch (error) {
            console.error(`❌ Failed to get balance: ${error}`);
            throw error;
        }
    }
    /**
     * Helper method to convert NEAR to yoctoNEAR
     */
    static toYoctoNear(amount) {
        return NEAR.toUnits(amount);
    }
    /**
     * Helper method to format yoctoNEAR to NEAR
     */
    static fromYoctoNear(amount) {
        const amountStr = typeof amount === 'bigint' ? amount.toString() : amount;
        return NEAR.toDecimal(amountStr);
    }
}
