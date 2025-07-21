import * as dotenv from 'dotenv';
dotenv.config({ path: './.env.development.local' });

// const and helpers

import { parseSeedPhrase } from 'near-seed-phrase';
import { createTransaction } from '@near-js/transactions';
import { JsonRpcProvider } from '@near-js/providers';
import { KeyPairSigner } from '@near-js/signers';
import { Account } from '@near-js/accounts';
import { NEAR } from '@near-js/tokens';
import { KeyPair } from '@near-js/crypto';
import { baseDecode, baseEncode } from '@near-js/utils';

const contractId = process.env.NEAR_CONTRACT_ID?.replaceAll('"', '');
const networkId = /testnet/gi.test(contractId) ? 'testnet' : 'mainnet';
let accountId, signer, keyPair;
const { NEAR_ACCOUNT_ID, NEAR_SEED_PHRASE } = process.env;
// if we're running within the API image and we have ENV vars for NEAR_ACCOUNT_ID and NEAR_SEED_PRASE
if (NEAR_ACCOUNT_ID && NEAR_SEED_PHRASE) {
    accountId = NEAR_ACCOUNT_ID.replaceAll('"', '');
    const { secretKey } = parseSeedPhrase(NEAR_SEED_PHRASE.replaceAll('"', ''));
    keyPair = KeyPair.fromString(secretKey);
    signer = new KeyPairSigner(keyPair);
}
const provider = new JsonRpcProvider({
    url:
        networkId === 'testnet'
            ? 'https://test.rpc.fastnear.com'
            : 'https://free.rpc.fastnear.com',
});
//helpers
export const parseNearAmount = (amt) => NEAR.toUnits(amt);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
export const getAccount = (id = accountId) => new Account(id, provider, signer);

export async function requestLiquidity() {
    // relay from solvers funding account in env vars
    const account = getAccount();
    const balance = account.getBalance();
    console.log('Balance:', balance);

    // TODO make env vars for path and predecessor
    const derivedKey = await account.callFunction({
        contractId: 'v1.signer',
        methodName: 'derived_public_key',
        args: {
            path: 'pool-1',
            predecessor: 'ac-proxy.shadeagent.near',
            domain_id: 1,
        },
    });

    const derivedAddress = Buffer.from(
        baseDecode(derivedKey.split(':')[1]),
    ).toString('hex');

    // WIP set the rest of this up

    // USDT contract and FT transfer details
    const usdtContract = 'usdt.tether-token.near'; // mainnet USDT contract
    const ftAmount = '1000000'; // 1 USDT (6 decimals)
    const ftArgs = {
        receiver_id: request.to,
        amount: ftAmount,
        // memo: request.memo, // optionally include memo if needed
    };
    const ftGas = '30000000000000'; // 30 Tgas
    const ftDeposit = '1'; // 1 yoctoNEAR required for ft_transfer
    const accessKey = await near.connection.provider.query(
        `access_key/${accountId}/${keyPair.getPublicKey().toString()}`,
        '',
    );
    const recentBlockHash = nearUtils.serialize.baseDecode(
        accessKey.block_hash,
    );
    const actions = [
        transactions.functionCall('ft_transfer', ftArgs, ftGas, ftDeposit),
    ];
    const transaction = createTransaction(
        accountId,
        keyPair.getPublicKey(),
        usdtContract,
        ++accessKey.nonce,
        actions,
        recentBlockHash,
    );

    console.log(
        `Initial transaction: ${accountId} -> ${usdtContract} (ft_transfer to ${request.to}, amount: ${ftAmount})`,
    );
    console.log(`Nonce: ${transaction.nonce}`);
    console.log(`(This will be recreated for the derived account)`);

    // Step 2: Hash the transaction
    console.log(
        '\nStep 2: Serializing derived account transaction for signing',
    );

    const serializedTx = nearUtils.serialize.serialize(
        transactions.SCHEMA.Transaction,
        transaction,
    );

    console.log(
        `Initial serialization: ${serializedTx.length} bytes (will be replaced with MPC transaction)`,
    );

    // Step 3: MPC sign with Ed25519
    console.log('\nStep 3: MPC signing with Ed25519');

    const contract = new contracts.ChainSignatureContract({
        networkId: 'testnet',
        contractId: 'v1.signer-prod.testnet',
    });

    const derivationPath = 'near-1';

    // Get the derived public key for MPC signing
    const derivedPublicKey = await contract.getDerivedPublicKey({
        path: derivationPath,
        predecessor: accountId,
        IsEd25519: true,
    });

    console.log(`Using MPC contract: v1.signer-prod.testnet`);
    console.log(`Derivation path: ${derivationPath}`);
    console.log(`Derived public key: ${derivedPublicKey}`);

    // Create a proper derived account name
    const mpcPublicKey = nearUtils.PublicKey.fromString(derivedPublicKey);
    const derivedAccountId = `${derivationPath}.${accountId}`;

    console.log(`Derived NEAR account: ${derivedAccountId}`);

    // Check if derived account exists, create if it doesn't
    let derivedAccount;
    let derivedAccountExists = false;

    try {
        derivedAccount = await near.account(derivedAccountId);
        const derivedBalance = await derivedAccount.getAccountBalance();
        console.log(
            `Derived account balance: ${nearUtils.format.formatNearAmount(
                derivedBalance.available,
            )} NEAR`,
        );
        derivedAccountExists = true;
    } catch (error) {
        console.log(`Derived account ${derivedAccountId} does not exist`);
        console.log(`Creating derived account controlled by MPC...`);

        // Create the derived account with MPC public key
        try {
            const createResult = await controllerAccount.createAccount(
                derivedAccountId,
                mpcPublicKey,
                BigInt(nearUtils.format.parseNearAmount('1')), // Fund with 1 NEAR
            );

            console.log(
                `createResult: ${JSON.stringify(createResult, null, 2)}`,
            );
            console.log(`Derived account created: ${derivedAccountId}`);
            console.log(`Funded with 1 NEAR`);
            console.log(`Added MPC public key: ${mpcPublicKey.toString()}`);

            // Wait for account creation to propagate
            console.log(`Waiting for account creation to propagate...`);
            await new Promise((resolve) => setTimeout(resolve, 3000));

            derivedAccount = await near.account(derivedAccountId);
            const newBalance = await derivedAccount.getAccountBalance();
            console.log(
                `Derived account balance: ${nearUtils.format.formatNearAmount(
                    newBalance.available,
                )} NEAR`,
            );
            derivedAccountExists = true;
        } catch (createError) {
            console.error(`Failed to create derived account:`, createError);
            throw new Error(`Cannot proceed without derived account`);
        }
    }

    // Get fresh access key info for the derived account
    const derivedAccessKey = await near.connection.provider.query(
        `access_key/${derivedAccountId}/${mpcPublicKey.toString()}`,
        '',
    );

    console.log(`Current access key nonce: ${derivedAccessKey.nonce}`);
    console.log(`Current block hash: ${derivedAccessKey.block_hash}`);

    const derivedRecentBlockHash = nearUtils.serialize.baseDecode(
        derivedAccessKey.block_hash,
    );
    const nextNonce = derivedAccessKey.nonce + 1;

    console.log(`Using nonce: ${nextNonce}`);

    // Create transaction FROM the derived account
    const mpcTransaction = transactions.createTransaction(
        derivedAccountId, // FROM the derived account
        mpcPublicKey,
        usdtContract, // send to USDT contract
        nextNonce,
        actions,
        derivedRecentBlockHash,
    );

    console.log(
        `MPC Transaction: ${derivedAccountId} -> ${usdtContract} (ft_transfer to ${request.to}, amount: ${ftAmount})`,
    );
    console.log(`Derived account nonce: ${mpcTransaction.nonce}`);

    // Serialize the MPC transaction for signing
    const mpcSerializedTx = nearUtils.serialize.serialize(
        transactions.SCHEMA.Transaction,
        mpcTransaction,
    );

    console.log(`MPC transaction serialized: ${mpcSerializedTx.length} bytes`);

    // Hash the serialized transaction (NEAR signs the SHA-256 hash, not raw bytes)
    const crypto = require('crypto');
    const transactionHash = crypto
        .createHash('sha256')
        .update(mpcSerializedTx)
        .digest();
    console.log(
        `Transaction hash for signing: ${transactionHash.toString('hex')}`,
    );

    const hashesToSign = [Array.from(transactionHash)];

    const signature = await contract.sign({
        payloads: hashesToSign,
        path: derivationPath,
        keyType: 'Eddsa',
        signerAccount: {
            accountId: controllerAccount.accountId,
            signAndSendTransactions: async ({
                transactions: walletSelectorTransactions,
            }) => {
                const results = [];

                for (const tx of walletSelectorTransactions) {
                    const actions = tx.actions.map((a) => createAction(a));

                    const result =
                        await controllerAccount.signAndSendTransaction({
                            receiverId: tx.receiverId,
                            actions,
                        });

                    results.push(getTransactionLastResult(result));
                }

                return results;
            },
        },
    });

    console.log(`MPC signature obtained (${signature.length} signatures)`);
    console.log(`Raw MPC response:`, JSON.stringify(signature[0], null, 2));

    const ed25519Signature = signature[0];
    if (!ed25519Signature || !ed25519Signature.signature) {
        throw new Error('Invalid MPC signature received');
    }

    console.log(
        `Ed25519 signature: ${ed25519Signature.signature.length} bytes`,
    );
    console.log(
        `Signature array: [${ed25519Signature.signature
            .slice(0, 8)
            .join(', ')}...]`,
    );

    // Convert signature from number array to Uint8Array (like Solana does)
    const signatureBytes = Buffer.from(ed25519Signature.signature);
    console.log(`Signature buffer length: ${signatureBytes.length}`);
    console.log(
        `Signature hex: ${signatureBytes.toString('hex').substring(0, 16)}...`,
    );

    // Step 4: Add signature to unsigned transaction
    console.log('\nStep 4: Attaching MPC signature to transaction');

    console.log(
        `Transaction public key: ${mpcTransaction.publicKey.toString()}`,
    );
    console.log(`Expected public key: ${mpcPublicKey.toString()}`);
    console.log(`Key type: 0 (Ed25519)`);

    const signedTransaction = new transactions.SignedTransaction({
        transaction: mpcTransaction,
        signature: new transactions.Signature({
            keyType: 0, // Ed25519 = 0, secp256k1 = 1
            data: signatureBytes,
        }),
    });

    console.log('MPC signature attached');

    // Step 5: Broadcast to NEAR network
    console.log('\nStep 5: Broadcasting transaction');

    const result = await near.connection.provider.sendTransaction(
        signedTransaction,
    );

    console.log(`Transaction broadcasted: ${result.transaction.hash}`);
    console.log(`Gas used: ${result.transaction_outcome.outcome.gas_burnt}`);
    console.log(
        `Explorer: https://testnet.nearblocks.io/txns/${result.transaction.hash}`,
    );

    return {
        success: true,
        transactionHash: result.transaction.hash,
        gasUsed: result.transaction_outcome.outcome.gas_burnt,
        keyType: 'ed25519',
        signatureType: 'mpc',
    };
}
