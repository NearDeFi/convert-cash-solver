import { ethers } from 'ethers';
import bs58 from 'bs58';
import dotenv from 'dotenv';

// Load environment variables from .env.development.local
dotenv.config({ path: '.env.development.local' });

const privateKey = process.env.EVM_PRIVATE_KEY;
const wallet = new ethers.Wallet(privateKey);
const signingKey = wallet.signingKey;

// Uncompressed public key (remove first byte - 0x04 prefix)
const uncompressedPubKey = signingKey.publicKey;
const uncompressedBuffer = Buffer.from(uncompressedPubKey.slice(2), 'hex'); // Remove '0x'
const uncompressedWithoutPrefix = uncompressedBuffer.slice(1); // Remove first byte (0x04)
const base58EncodedUncompressed = bs58.encode(uncompressedWithoutPrefix);

console.log('=== UNCOMPRESSED PUBLIC KEY (without 0x04 prefix) ===');
console.log('Full hex:', uncompressedPubKey);
console.log('Without prefix hex:', uncompressedWithoutPrefix.toString('hex'));
console.log('Base58:', base58EncodedUncompressed);
console.log('Length:', uncompressedWithoutPrefix.length, 'bytes');
