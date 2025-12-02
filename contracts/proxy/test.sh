#!/bin/bash

# Script to build and test the NEAR smart contract with sandbox

set -e  # Exit on error

# Array of test files to run
TESTS=(
    "test_borrow_with_redemption"
    "test_complex_multi_lender_scenario"
    "test_fifo_redemption_queue"
    "test_half_redemptions"
    "test_lender_profit"
    "test_multi_lender_queue"
    "test_multi_solver"
    "test_partial_repayment"
    "test_rounding_nep621"
    "test_single_lender_queue"
    "test_solver_borrow"
    "test_solver_borrow_empty_pool"
    "test_solver_borrow_exact_pool"
    "test_solver_borrow_exceeds_pool"
    "test_vault_deposit"
    "test_withdrawals"
    "sandbox_test"
)

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

    # Build the main proxy contract (non-reproducible for faster builds)
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
    
    # Build each test binary sequentially to avoid overloading the system
    echo "========================================="
    echo "Building Test Binaries (Sequential)"
    echo "========================================="
    
    BUILD_SUCCESSES=0
    BUILD_FAILURES=0
    FAILED_TESTS=()
    
    for test in "${TESTS[@]}"; do
        echo -n "Building $test... "
        if cargo build --test "$test" 2>/dev/null; then
            echo "✅"
            BUILD_SUCCESSES=$((BUILD_SUCCESSES + 1))
        else
            echo "❌"
            BUILD_FAILURES=$((BUILD_FAILURES + 1))
            FAILED_TESTS+=("$test")
        fi
    done
    
    echo ""
    echo "========================================="
    echo "Build Summary"
    echo "========================================="
    echo "✅ Successful: $BUILD_SUCCESSES"
    echo "❌ Failed: $BUILD_FAILURES"
    
    if [ ${#FAILED_TESTS[@]} -gt 0 ]; then
        echo ""
        echo "Failed tests:"
        for failed in "${FAILED_TESTS[@]}"; do
            echo "  - $failed"
        done
    fi
    
    echo ""
fi

echo "========================================="
echo "Running Tests"
echo "========================================="

# Run tests with appropriate flags
# Use --test-threads=1 to run tests sequentially (less resource-intensive)
if [ "$RUN_ARRAY" == true ]; then
    echo "Running tests from TESTS array (one at a time)..."
    for test in "${TESTS[@]}"; do
        echo ""
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo "Running test: $test"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        if [ "$DEBUG" == true ]; then
            NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test --test "$test" -- --nocapture --test-threads=1
        elif [ "$VERBOSE" == true ]; then
            cargo test --test "$test" -- --nocapture --test-threads=1
        else
            cargo test --test "$test" -- --test-threads=1
        fi
    done
elif [ -n "$TEST_NAME" ]; then
    echo "Running specific test: $TEST_NAME"
    if [ "$DEBUG" == true ]; then
        NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test --test "$TEST_NAME" -- --nocapture --test-threads=1
    else
        cargo test --test "$TEST_NAME" -- --nocapture --test-threads=1
    fi
elif [ "$DEBUG" == true ]; then
    echo "Running tests with debug logging..."
    NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test -- --nocapture --test-threads=1
elif [ "$VERBOSE" == true ]; then
    echo "Running tests with output..."
    cargo test -- --nocapture --test-threads=1
else
    echo "Running tests (quiet mode)..."
    cargo test -- --test-threads=1
fi

echo ""
echo "========================================="
echo "Tests Complete!"
echo "========================================="

