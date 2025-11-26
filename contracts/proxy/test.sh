#!/bin/bash

# Script to build and test the NEAR smart contract with sandbox

set -e  # Exit on error

# Array of test files to run
TESTS=("test half_redemptions" "test_vault_deposit" "test_multi_lender_queue" "test_fifo_redemption_queue" "test_single_lender_queue" "test_solver_borrow")

# Function to show usage
show_usage() {
    echo "Usage: ./test.sh [OPTIONS] [TEST_NAME]"
    echo ""
    echo "Options:"
    echo "  (no args)           Build and run all tests (quiet)"
    echo "  -v, --verbose       Run tests with output (--nocapture)"
    echo "  -d, --debug         Run tests with sandbox debug logging"
    echo "  -t, --test NAME     Run specific test with output"
    echo "  -a, --array         Run all tests in the TESTS array"
    echo "  --no-build          Skip building, just run tests"
    echo "  -h, --help          Show this help message"
    echo ""
    echo "Examples:"
    echo "  ./test.sh                              # Build and run all tests"
    echo "  ./test.sh -v                           # Run all tests with output"
    echo "  ./test.sh -t test_vault_initialization # Run specific test"
    echo "  ./test.sh -a                            # Run all tests in TESTS array"
    echo "  ./test.sh --no-build -v                # Skip build, run with output"
}

# Parse arguments
SKIP_BUILD=false
VERBOSE=false
DEBUG=false
TEST_NAME=""
RUN_ARRAY=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_usage
            exit 0
            ;;
        --no-build)
            SKIP_BUILD=true
            shift
            ;;
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        -d|--debug)
            DEBUG=true
            shift
            ;;
        -t|--test)
            TEST_NAME="$2"
            shift 2
            ;;
        -a|--array)
            RUN_ARRAY=true
            VERBOSE=true  # Automatically enable verbose mode for array tests
            shift
            ;;
        *)
            TEST_NAME="$1"
            shift
            ;;
    esac
done

# Build the contracts
if [ "$SKIP_BUILD" == false ]; then
    echo "========================================="
    echo "Building NEAR Smart Contracts"
    echo "========================================="

    # Check if cargo-near is installed
    if ! command -v cargo-near &> /dev/null; then
        echo "cargo-near is not installed. Installing..."
        cargo install cargo-near
    fi

    # Build the main proxy contract
    echo "Building proxy contract WASM..."
    cargo near build non-reproducible-wasm
    
    echo ""
    
    # Build the mock FT contract
    if [ -d "../mock_ft" ]; then
        echo "Building mock_ft contract WASM..."
        cd ../mock_ft
        cargo near build non-reproducible-wasm
        cd ../proxy
        echo "✅ mock_ft contract built successfully"
    else
        echo "⚠️  mock_ft contract not found at ../mock_ft (skipping)"
    fi
    
    echo ""
fi

echo "========================================="
echo "Running Tests"
echo "========================================="

# Run tests with appropriate flags
if [ "$RUN_ARRAY" == true ]; then
    echo "Running tests from TESTS array: ${TESTS[*]}"
    for test in "${TESTS[@]}"; do
        echo ""
        echo "Running test: $test"
        if [ "$DEBUG" == true ]; then
            NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test "$test" -- --nocapture
        elif [ "$VERBOSE" == true ]; then
            cargo test "$test" -- --nocapture
        else
            cargo test "$test"
        fi
    done
elif [ -n "$TEST_NAME" ]; then
    echo "Running specific test: $TEST_NAME"
    if [ "$DEBUG" == true ]; then
        NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test "$TEST_NAME" -- --nocapture
    else
        cargo test "$TEST_NAME" -- --nocapture
    fi
elif [ "$DEBUG" == true ]; then
    echo "Running tests with debug logging..."
    NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test -- --nocapture
elif [ "$VERBOSE" == true ]; then
    echo "Running tests with output..."
    cargo test -- --nocapture
else
    echo "Running tests (quiet mode)..."
    cargo test
fi

echo ""
echo "========================================="
echo "Tests Complete!"
echo "========================================="

