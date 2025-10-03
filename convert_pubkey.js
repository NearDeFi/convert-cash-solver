import bs58 from 'bs58';

// Uncompressed public key from your example
const uncompressedPubKey =
    '0x04a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0';

function convertToCompressedAndBase58(uncompressedHex) {
    // Remove 0x prefix and the 04 prefix (uncompressed indicator)
    const cleanHex = uncompressedHex.replace('0x04', '');

    // Split into x and y coordinates (each 64 hex chars = 32 bytes)
    const x = cleanHex.slice(0, 64);
    const y = cleanHex.slice(64, 128);

    console.log('Original uncompressed public key:');
    console.log('Full:', uncompressedHex);
    console.log('X coordinate:', '0x' + x);
    console.log('Y coordinate:', '0x' + y);

    // Convert y coordinate to BigInt to check if it's even or odd
    const yBigInt = BigInt('0x' + y);

    // Determine compression prefix: 0x02 for even y, 0x03 for odd y
    const prefix = yBigInt % 2n === 0n ? '02' : '03';

    // Compressed public key is prefix + x coordinate
    const compressedHex = prefix + x;
    const compressedWithPrefix = '0x' + compressedHex;

    console.log('\nCompressed public key:');
    console.log('Hex:', compressedWithPrefix);
    console.log(
        'Y coordinate is:',
        yBigInt % 2n === 0n ? 'even (prefix 02)' : 'odd (prefix 03)',
    );

    // Convert to buffer and encode in base58
    const compressedBuffer = Buffer.from(compressedHex, 'hex');
    const base58Encoded = bs58.encode(compressedBuffer);

    console.log('\nBase58 encoded:');
    console.log('Base58:', base58Encoded);

    // Verify by decoding back
    const decoded = bs58.decode(base58Encoded);
    const decodedHex = '0x' + Buffer.from(decoded).toString('hex');

    console.log('\nVerification (decode base58 back to hex):');
    console.log('Decoded:', decodedHex);
    console.log('Matches compressed:', decodedHex === compressedWithPrefix);

    return {
        original: uncompressedHex,
        compressed: compressedWithPrefix,
        base58: base58Encoded,
    };
}

// Perform the conversion
const result = convertToCompressedAndBase58(uncompressedPubKey);

console.log('\n=== SUMMARY ===');
console.log('Uncompressed:', result.original);
console.log('Compressed:  ', result.compressed);
console.log('Base58:      ', result.base58);
