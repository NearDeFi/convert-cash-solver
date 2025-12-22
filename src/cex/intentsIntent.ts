/**
 * NEAR Intents Intent Service
 * Creates, signs, and executes ft_withdraw intents
 * Based on the format from get_quote_to_sign project and binance-cex-poc
 */

import { ethers } from 'ethers';
import bs58 from 'bs58';
import { JsonRpcProvider } from '@near-js/providers';
import { Account } from '@near-js/accounts';
import { KeyPairSigner } from '@near-js/signers';
import { KeyPair } from '@near-js/crypto';
import type { KeyPairString } from '@near-js/crypto';
import dotenv from 'dotenv';

const dir = process.cwd();
dotenv.config({ path: `${dir}/.env.development.local` });

export interface FtWithdrawIntent {
    intent: 'ft_withdraw';
    token: string; // OMFT token account ID (e.g., "eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near")
    receiver_id: string; // Vault contract account ID
    amount: string; // Amount in minimal units (string)
    msg?: string; // Optional message for non-NEAR chains
    memo?: string; // Optional memo
    storage_deposit?: string; // Optional storage deposit in yoctoNEAR
}

export interface IntentQuote {
    signer_id: string; // NEAR account ID of the solver (e.g., "solver-account.near")
    nonce: string; // Base64 encoded random nonce
    verifying_contract: string; // "intents.near" or "intents.testnet"
    deadline: string; // ISO 8601 deadline
    intents: FtWithdrawIntent[];
}

export interface SignedIntent {
    standard: 'erc191';
    payload: string; // JSON string of the quote
    signature: string; // secp256k1:base58 encoded signature
}

export interface IntentsIntentConfig {
    solverNearAccountId: string; // NEAR account ID of the solver (for signer_id, same as INTENTS_BRIDGE_ACCOUNT_ID)
    solverEvmPrivateKey?: string; // EVM private key for signing (required for signing intents, optional for key registration)
    vaultContractId?: string; // NEAR account ID of the vault contract (required for ft_withdraw, optional for key registration)
    nearAccountId?: string; // NEAR account ID for executing the intent (optional, defaults to solverNearAccountId)
    nearPrivateKey?: string; // NEAR private key for executing the intent (required for executing intents and registering keys)
    nearNetworkId?: string; // 'mainnet' or 'testnet' (default: 'mainnet')
}

export class IntentsIntentService {
    private readonly config: IntentsIntentConfig;
    private provider: JsonRpcProvider | null = null;
    private account: Account | null = null;

    constructor(config: IntentsIntentConfig) {
        this.config = config;
    }

    /**
     * Generates a random nonce for the quote
     */
    private generateNonce(): string {
        const randomBytes = ethers.randomBytes(32);
        return Buffer.from(randomBytes).toString('base64');
    }

    /**
     * Creates an ft_withdraw intent quote
     */
    createFtWithdrawQuote(
        token: string, // OMFT token account ID
        amount: string, // Amount in minimal units (string)
        receiverId?: string, // Optional receiver (defaults to vault contract)
        msg?: string, // Optional message for non-NEAR chains
        memo?: string, // Optional memo
    ): IntentQuote {
        const receiver_id = receiverId || this.config.vaultContractId;
        if (!receiver_id) {
            throw new Error('receiverId or vaultContractId must be provided');
        }

        const intent: FtWithdrawIntent = {
            intent: 'ft_withdraw',
            token,
            receiver_id,
            amount,
        };

        // Add optional fields
        if (msg) {
            intent.msg = msg;
        }
        if (memo) {
            intent.memo = memo;
        }

        const quote: IntentQuote = {
            signer_id: this.config.solverNearAccountId, // NEAR account ID of the solver
            nonce: this.generateNonce(),
            verifying_contract: 'intents.near',
            deadline: '2025-12-31T11:59:59.000Z', // Default deadline (can be made configurable)
            intents: [intent],
        };

        return quote;
    }

    /**
     * Signs a quote using ERC191 standard (Ethereum personal sign)
     * Returns the signature in the format expected by NEAR Intents: "secp256k1:base58(...)"
     */
    async signQuote(quote: IntentQuote): Promise<SignedIntent> {
        if (!this.config.solverEvmPrivateKey) {
            throw new Error('solverEvmPrivateKey is required for signing intents');
        }

        // Convert quote to compact JSON string (no spaces, no newlines)
        const payload = JSON.stringify(quote);

        // Create wallet from private key
        const wallet = new ethers.Wallet(this.config.solverEvmPrivateKey);

        // Sign the message using Ethereum's personal sign (ERC191)
        const hexSignature = await wallet.signMessage(payload);

        // Convert signature from hex to buffer
        const signatureBuffer = Buffer.from(hexSignature.slice(2), 'hex');

        // Adjust v value: Ethereum uses 27/28, we need 0-3
        let v = signatureBuffer[64] ?? 27;
        if (v === 27) {
            v = 0;
        } else if (v === 28) {
            v = 1;
        } else {
            v = (v - 27) % 4;
        }

        // Create RSV signature (65 bytes: 32 R + 32 S + 1 V)
        const rsvSignature = new Uint8Array(65);
        rsvSignature.set(signatureBuffer.slice(0, 32), 0); // R
        rsvSignature.set(signatureBuffer.slice(32, 64), 32); // S
        rsvSignature[64] = v; // V

        // Encode as base58 with secp256k1 prefix
        const signature = 'secp256k1:' + bs58.encode(rsvSignature);

        return {
            standard: 'erc191',
            payload,
            signature,
        };
    }

    /**
     * Initializes NEAR connection for executing intents (using modern API)
     */
    private async initNearConnection(): Promise<void> {
        if (this.provider && this.account) {
            return;
        }

        if (!this.config.nearAccountId || !this.config.nearPrivateKey) {
            throw new Error(
                'NEAR account ID and private key are required to execute intents',
            );
        }

        const networkId = this.config.nearNetworkId || 'mainnet';
        const nodeUrl =
            networkId === 'mainnet'
                ? 'https://rpc.mainnet.near.org'
                : 'https://rpc.testnet.near.org';

        // Initialize provider
        this.provider = new JsonRpcProvider({ url: nodeUrl });

        // Create signer from private key
        const keyPair = KeyPair.fromString(
            this.config.nearPrivateKey as KeyPairString,
        );
        const signer = new KeyPairSigner(keyPair);

        // Create account instance
        this.account = new Account(
            this.config.nearAccountId,
            this.provider,
            signer,
        );
    }

    /**
     * Executes the signed intent on intents.near contract
     */
    async executeIntent(signedIntent: SignedIntent): Promise<string> {
        await this.initNearConnection();

        if (!this.account) {
            throw new Error('NEAR connection not initialized');
        }

        const contractId = 'intents.near';

        // Structure the data as expected by execute_intents
        const requestData = {
            signed: [
                {
                    standard: signedIntent.standard,
                    payload: signedIntent.payload, // String JSON, not parsed
                    signature: signedIntent.signature,
                },
            ],
        };

        console.log('📤 Executing intent on intents.near...');
        console.log('   Contract:', contractId);
        console.log(
            '   Account:',
            this.config.nearAccountId || this.config.solverNearAccountId,
        );
        console.log('   Payload:', signedIntent.payload);

        try {
            const gasAmount = BigInt('300000000000000'); // 300 TGas
            const depositAmount = BigInt('0'); // No deposit needed

            const result = await this.account.functionCall({
                contractId,
                methodName: 'execute_intents',
                args: requestData,
                gas: gasAmount,
                attachedDeposit: depositAmount,
            });

            // Extract transaction hash
            const txHash = result.transaction_outcome.id;
            console.log('✅ Intent executed successfully!');
            console.log('   Transaction Hash:', txHash);

            return txHash;
        } catch (error: any) {
            console.error('❌ Error executing intent:', error);
            throw error;
        }
    }

    /**
     * Creates, signs, and executes an ft_withdraw intent
     * This is the main function to use for withdrawing from solver's Intents account to vault
     */
    async createAndExecuteFtWithdraw(
        token: string,
        amount: string,
        receiverId?: string,
        msg?: string,
        memo?: string,
    ): Promise<string> {
        // Determine the receiver (vault contract)
        const finalReceiverId = receiverId || this.config.vaultContractId;
        if (!finalReceiverId) {
            throw new Error('receiverId or vaultContractId must be provided');
        }

        // CRITICAL: Ensure the vault is registered in the OMFT token contract
        // If not registered, tokens sent to it will be lost!
        console.log(
            `\n⚠️  IMPORTANT: Verifying storage registration for ${finalReceiverId}...`,
        );
        await this.ensureStorageRegistered(token, finalReceiverId);

        console.log('\n🔍 Creating ft_withdraw intent...');
        const quote = this.createFtWithdrawQuote(
            token,
            amount,
            receiverId,
            msg,
            memo,
        );
        console.log('✅ Quote created:', JSON.stringify(quote, null, 2));

        console.log('\n🔍 Signing quote with ERC191...');
        const signedIntent = await this.signQuote(quote);
        console.log('✅ Quote signed');
        console.log(
            '   Signature:',
            signedIntent.signature.substring(0, 50) + '...',
        );

        console.log('\n🔍 Executing intent...');
        const txHash = await this.executeIntent(signedIntent);

        return txHash;
    }

    /**
     * Generates a new EVM key pair (secp256k1) for ERC191 signing
     * Returns both private key and public key in the format needed for Intents
     */
    generateEvmKeyPair(): {
        privateKey: string; // EVM private key (0x...)
        publicKey: string; // Public key in format: secp256k1:base58(...)
        evmAddress: string; // EVM address derived from the key
    } {
        // Generate a random wallet
        const randomWallet = ethers.Wallet.createRandom();

        // Get the uncompressed public key (64 bytes: x || y)
        const uncompressedPubKey = randomWallet.signingKey.publicKey;
        const uncompressedBuffer = Buffer.from(
            uncompressedPubKey.slice(2),
            'hex',
        ); // Remove '0x'
        // Remove first byte (0x04 prefix) to get raw 64 bytes
        const uncompressedWithoutPrefix = uncompressedBuffer.slice(1);

        // Encode as base58 with secp256k1 prefix (format required by Intents)
        const publicKey =
            'secp256k1:' + bs58.encode(uncompressedWithoutPrefix);

        return {
            privateKey: randomWallet.privateKey,
            publicKey,
            evmAddress: randomWallet.address,
        };
    }

    /**
     * Gets all public keys registered for the solver account in Intents
     */
    async getRegisteredPublicKeys(): Promise<string[]> {
        await this.initNearConnection();

        if (!this.provider) {
            throw new Error('NEAR connection not initialized');
        }

        const accountId =
            this.config.nearAccountId || this.config.solverNearAccountId;
        const contractId = 'intents.near';

        try {
            // Using Provider.callFunction (modern API, no deprecation warnings)
            const publicKeys = (await this.provider.callFunction(
                contractId,
                'public_keys_of',
                { account_id: accountId },
            )) as string[] | undefined;

            return publicKeys || [];
        } catch (error: any) {
            console.error('❌ Error getting public keys:', error);
            return [];
        }
    }

    /**
     * Registers an EVM public key in Intents for the solver account
     * @param publicKey Public key in format: secp256k1:base58(...)
     */
    async registerPublicKey(publicKey: string): Promise<string> {
        await this.initNearConnection();

        if (!this.account) {
            throw new Error('NEAR connection not initialized');
        }

        const accountId =
            this.config.nearAccountId || this.config.solverNearAccountId;
        const contractId = 'intents.near';

        console.log('📤 Registering public key in Intents...');
        console.log('   Account:', accountId);
        console.log('   Public Key:', publicKey);

        try {
            const gasAmount = BigInt('30000000000000'); // 30 TGas
            const depositAmount = BigInt('1'); // 1 yoctoNEAR (required by the contract)

            const result = await this.account.functionCall({
                contractId,
                methodName: 'add_public_key',
                args: { public_key: publicKey },
                gas: gasAmount,
                attachedDeposit: depositAmount,
            });

            const txHash = result.transaction_outcome.id;
            console.log('✅ Public key registered successfully!');
            console.log('   Transaction Hash:', txHash);

            return txHash;
        } catch (error: any) {
            console.error('❌ Error registering public key:', error);
            throw error;
        }
    }

    /**
     * Checks if an account is registered for storage in the OMFT token contract
     * @param tokenContractId OMFT token contract ID
     * @param accountId Account ID to check
     * @returns true if registered, false otherwise
     */
    async isStorageRegistered(
        tokenContractId: string,
        accountId: string,
    ): Promise<boolean> {
        await this.initNearConnection();

        if (!this.provider) {
            throw new Error('NEAR connection not initialized');
        }

        try {
            const storageBalance = (await this.provider.callFunction(
                tokenContractId,
                'storage_balance_of',
                { account_id: accountId },
            )) as { total: string; available: string } | null | undefined;

            // If storage_balance_of returns null/undefined, the account is not registered
            return storageBalance !== null && storageBalance !== undefined;
        } catch (error: any) {
            // If the account is not registered, storage_balance_of may throw an error
            // or return null, so we treat any error as "not registered"
            console.warn(
                `⚠️  Could not check storage registration for ${accountId}: ${error.message}`,
            );
            return false;
        }
    }

    /**
     * Gets the storage balance bounds (min/max deposit required)
     * @param tokenContractId OMFT token contract ID
     * @returns Storage balance bounds with min and max amounts
     */
    async getStorageBalanceBounds(tokenContractId: string): Promise<{
        min: string;
        max: string;
    }> {
        await this.initNearConnection();

        if (!this.provider) {
            throw new Error('NEAR connection not initialized');
        }

        try {
            const bounds = (await this.provider.callFunction(
                tokenContractId,
                'storage_balance_bounds',
                {},
            )) as {
                min: string;
                max: string;
            } | null | undefined;

            if (!bounds) {
                throw new Error('Could not get storage balance bounds');
            }

            return {
                min: bounds.min || '0',
                max: bounds.max || '0',
            };
        } catch (error: any) {
            console.error(`❌ Error getting storage balance bounds: ${error}`);
            throw error;
        }
    }

    /**
     * Registers an account for storage in the OMFT token contract
     * This must be done before the account can receive tokens
     * @param tokenContractId OMFT token contract ID
     * @param accountId Account ID to register (defaults to vault contract)
     * @returns Transaction hash
     */
    async registerStorage(
        tokenContractId: string,
        accountId?: string,
    ): Promise<string> {
        await this.initNearConnection();

        if (!this.account) {
            throw new Error('NEAR connection not initialized');
        }

        const targetAccountId = accountId || this.config.vaultContractId;
        if (!targetAccountId) {
            throw new Error('accountId or vaultContractId must be provided');
        }

        console.log(`\n🔍 Registering storage for ${targetAccountId}...`);

        try {
            // Get storage balance bounds to know how much to deposit
            const bounds = await this.getStorageBalanceBounds(tokenContractId);
            const minDeposit = BigInt(bounds.min);

            console.log(`   Required deposit: ${bounds.min} yoctoNEAR`);

            // Call storage_deposit
            // account_id is optional - if None, it registers the predecessor (our account)
            // If provided, it registers that account
            const gasAmount = BigInt('30000000000000'); // 30 TGas

            const result = await this.account.functionCall({
                contractId: tokenContractId,
                methodName: 'storage_deposit',
                args: {
                    account_id: targetAccountId,
                    registration_only: false,
                },
                gas: gasAmount,
                attachedDeposit: minDeposit,
            });

            const txHash = result.transaction_outcome.id;
            console.log(`✅ Storage registered successfully!`);
            console.log(`   Transaction Hash: ${txHash}`);

            return txHash;
        } catch (error: any) {
            console.error(`❌ Error registering storage: ${error}`);
            throw error;
        }
    }

    /**
     * Ensures an account is registered for storage, registering it if necessary
     * @param tokenContractId OMFT token contract ID
     * @param accountId Account ID to ensure is registered (defaults to vault contract)
     * @returns true if registration was needed, false if already registered
     */
    async ensureStorageRegistered(
        tokenContractId: string,
        accountId?: string,
    ): Promise<boolean> {
        const targetAccountId = accountId || this.config.vaultContractId;
        if (!targetAccountId) {
            throw new Error('accountId or vaultContractId must be provided');
        }

        console.log(
            `\n🔍 Checking if ${targetAccountId} is registered in ${tokenContractId}...`,
        );

        const isRegistered = await this.isStorageRegistered(
            tokenContractId,
            targetAccountId,
        );

        if (isRegistered) {
            console.log(`✅ ${targetAccountId} is already registered`);
            return false;
        }

        console.log(
            `⚠️  ${targetAccountId} is NOT registered. Registering now...`,
        );
        await this.registerStorage(tokenContractId, targetAccountId);
        return true;
    }

    /**
     * Gets the OMFT token balance for a specific account
     * @param tokenContractId OMFT token contract ID (e.g., "eth-0x...omft.near")
     * @param accountId NEAR account ID to check balance for
     */
    async getOmftBalance(
        tokenContractId: string,
        accountId: string,
    ): Promise<string> {
        await this.initNearConnection();

        if (!this.provider) {
            throw new Error('NEAR connection not initialized');
        }

        try {
            // Using Provider.callFunction (modern API, no deprecation warnings)
            const balance = await this.provider.callFunction(
                tokenContractId,
                'ft_balance_of',
                { account_id: accountId },
            );

            // Ensure we return a string
            if (balance === null || balance === undefined) {
                return '0';
            }
            return String(balance);
        } catch (error: any) {
            console.error(`❌ Error getting OMFT balance: ${error}`);
            // If account is not registered, return '0'
            if (error.message?.includes('not registered')) {
                return '0';
            }
            throw error;
        }
    }

    /**
     * Gets token metadata (decimals, symbol, etc.)
     * @param tokenContractId OMFT token contract ID
     */
    async getTokenMetadata(tokenContractId: string): Promise<{
        decimals: number;
        symbol: string;
        name: string;
    }> {
        await this.initNearConnection();

        if (!this.provider) {
            throw new Error('NEAR connection not initialized');
        }

        try {
            // Using Provider.callFunction (modern API, no deprecation warnings)
            const metadata = (await this.provider.callFunction(
                tokenContractId,
                'ft_metadata',
                {},
            )) as {
                decimals?: number;
                symbol?: string;
                name?: string;
            } | null | undefined;

            if (!metadata) {
                return {
                    decimals: 0,
                    symbol: 'UNKNOWN',
                    name: 'Unknown Token',
                };
            }

            return {
                decimals: metadata.decimals || 0,
                symbol: metadata.symbol || 'UNKNOWN',
                name: metadata.name || 'Unknown Token',
            };
        } catch (error: any) {
            console.error(`❌ Error getting token metadata: ${error}`);
            throw error;
        }
    }

    /**
     * Formats balance from minimal units to human-readable format
     * @param balance Balance in minimal units (string)
     * @param decimals Number of decimals
     */
    formatBalance(balance: string, decimals: number): string {
        const balanceNum = BigInt(balance);
        const divisor = BigInt(10 ** decimals);
        const wholePart = balanceNum / divisor;
        const fractionalPart = balanceNum % divisor;

        if (fractionalPart === BigInt(0)) {
            return wholePart.toString();
        }

        const fractionalStr = fractionalPart.toString().padStart(decimals, '0');
        const trimmedFractional = fractionalStr.replace(/0+$/, '');
        return `${wholePart}.${trimmedFractional || '0'}`;
    }
}

