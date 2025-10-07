/**
 * Quote Selection Test
 * Equivalent to the select_best_option function from Python
 * CORRECTED: Selects the option with the HIGHEST amount_out (best for user)
 */

// CORRECTED function - selects HIGHEST amount_out (best for user)
const selectBestOption = (options) => {
    if (!options || options.length === 0) {
        return null;
    }
    
    let bestOption = null;
    for (const option of options) {
        if (!bestOption || parseInt(option.amount_out) > parseInt(bestOption.amount_out)) {
            bestOption = option;
        }
    }
    return bestOption;
};

// Test data based on the original terminal output
const testQuotes = [
    {
        defuse_asset_identifier_in: 'nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near',
        defuse_asset_identifier_out: 'nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near',
        amount_in: '4685840',
        amount_out: '4685830',
        expiration_time: '2025-10-06T15:44:05.905Z',
        quote_hash: '4LP8zP9tA25dA9y4JBPWtjMgbraAAgerwa61iq7vBKAH'
    },
    {
        defuse_asset_identifier_in: 'nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near',
        defuse_asset_identifier_out: 'nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near',
        amount_in: '4685840',
        amount_out: '4685831',
        expiration_time: '2025-12-31T11:59:59Z',
        quote_hash: 'EnTg7ML29SzyiTzYX9QoMNcYsursKmUxZA46xVbzLHBd'
    }
];

// Test function
function runQuoteSelectionTest() {
    console.log('🧪 Quote Selection Test');
    console.log('========================');
    console.log('');
    
    console.log('📋 Available options:');
    testQuotes.forEach((option, index) => {
        console.log(`  Option ${index + 1}:`);
        console.log(`    amount_out: ${option.amount_out}`);
        console.log(`    quote_hash: ${option.quote_hash}`);
        console.log(`    expiration_time: ${option.expiration_time}`);
        console.log('');
    });
    
    const bestOption = selectBestOption(testQuotes);
    
    console.log('bestOption:', bestOption);
    return bestOption;
}

// Run test if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
    runQuoteSelectionTest();
}

// Export for use in other modules
export { selectBestOption, runQuoteSelectionTest };
