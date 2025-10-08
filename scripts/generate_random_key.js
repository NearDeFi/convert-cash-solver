import { ethers } from 'ethers';

// Generate a random wallet
const randomWallet = ethers.Wallet.createRandom();

console.log('=== RANDOM EVM WALLET ===');
console.log('Private Key:', randomWallet.privateKey);
console.log('Address:', randomWallet.address);
console.log();

console.log('=== PUBLIC KEY INFO ===');
const signingKey = randomWallet.signingKey;
console.log('Compressed Public Key:', signingKey.compressedPublicKey);
console.log('Uncompressed Public Key:', signingKey.publicKey);
console.log();

console.log('=== MNEMONIC (if available) ===');
if (randomWallet.mnemonic) {
    console.log('Mnemonic Phrase:', randomWallet.mnemonic.phrase);
    console.log('Path:', randomWallet.mnemonic.path);
}
