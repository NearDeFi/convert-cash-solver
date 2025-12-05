#!/bin/bash

# Local test runner that preserves reproducible WASM artifacts
# Combines reproducible builds with Matt's test execution functionality

set -euo pipefail

# Array of test files to run (all tests in tests/ directory)
# Note: sandbox_test.rs contains multiple tests with specific names
TESTS=(
    "test_borrow_with_redemption"
    "test_complex_multi_lender_scenario"
    "test_fifo_redemption_queue"
    "test_half_redemptions"
    "test_lender_profit"
    "test_multi_lender_queue"
    "test_multi_solver"
    "test_single_lender_queue"
    "test_solver_borrow"
    "test_vault_deposit"
    "test_withdrawals"
    # Tests from sandbox_test.rs
    "test_mock_ft_deployment_only"
    "test_contract_deployment"
    "test_approve_codehash"
    "test_vault_initialization"
    "test_vault_conversion_functions"
    # Tests for solver borrow limits
    "test_solver_borrow_exceeds_pool_size"
    "test_solver_borrow_exact_pool_size"
    "test_solver_borrow_empty_pool"
    # Tests for repayment validation
    "test_partial_repayment_less_than_principal"
    "test_repayment_exact_principal_no_yield"
    "test_repayment_with_yield"
    "test_repayment_with_extra_yield"
    # NEP-621 Rounding Direction Security Tests
    "test_deposit_shares_round_down"
    "test_micro_transaction_attack_prevention"
    "test_small_amount_precision"
    "test_yield_calculation_rounding"
    "test_redemption_rounds_down"
)

show_usage() {
    cat <<'USAGE'
Usage: ./test_local_repro.sh [OPTIONS] [TEST_NAME]

Options:
  (no args)           Build reproducible WASM artifacts and run tests from TESTS array (quiet)
  -v, --verbose       Run tests with output (--nocapture)
  -d, --debug         Run tests with sandbox debug logging
  -t, --test NAME     Run specific test with output
  -a, --array         Same as default: run all tests in the TESTS array (from Matt's test.sh)
  --no-build          Skip reproducible build step
  --keep-going        Do not exit immediately if tests fail
  -h, --help          Show this help message
  
Note: By default, only tests in TESTS array are run (excludes tests in development).

Examples:
  ./test_local_repro.sh                              # Build reproducible and run all tests
  ./test_local_repro.sh --no-build -v                # Skip build, run with output
  ./test_local_repro.sh -t test_vault_initialization # Run specific test
  ./test_local_repro.sh -a                           # Run all tests in TESTS array
USAGE
}

SKIP_BUILD=false
VERBOSE=false
DEBUG=false
KEEP_GOING=false
TEST_NAME=""
RUN_ARRAY=false

while [[ $# -gt 0 ]]; do
    case "$1" in
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
        --keep-going)
            KEEP_GOING=true
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

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROXY_DIR="$ROOT_DIR/proxy"
MOCK_FT_DIR="$ROOT_DIR/mock_ft"

check_git_clean() {
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "⚠️  Warning: git workspace has uncommitted changes."
        echo "    reproducible builds expect a clean tree."
    fi
}

build_reproducible() {
    echo "========================================="
    echo "Reproducible Build: Proxy Contract"
    echo "========================================="
    
    # Check if cargo-near is installed
    if ! command -v cargo-near &> /dev/null; then
        echo "cargo-near is not installed. Installing..."
        cargo install cargo-near
    fi
    
    (
        cd "$PROXY_DIR"
        cargo near build reproducible-wasm --variant force_bulk_memory
    )
    echo ""
    echo "========================================="
    echo "Reproducible Build: mock_ft Contract"
    echo "========================================="
    (
        cd "$MOCK_FT_DIR"
        cargo near build reproducible-wasm --variant force_bulk_memory
    )
    echo ""
}

run_tests() {
    echo "========================================="
    echo "Running Tests"
    echo "========================================="
    cd "$PROXY_DIR"

    local exit_code=0

    run_cargo_suite() {
        local mode="$1"; shift
        local cmd=("$@")
        echo ""
        echo ">> Running ${mode}..."
        set +e
        "${cmd[@]}"
        local status=$?
        set -e
        if [[ $status -ne 0 ]]; then
            echo "❌ ${mode} failed with status $status"
            if ! $KEEP_GOING; then
                exit $status
            else
                echo "⚠️  Continuing because --keep-going was set"
                exit_code=$status
            fi
        fi
    }

    # Run specific test by name
    if [[ -n "$TEST_NAME" ]]; then
        echo "Running specific test: $TEST_NAME"
        if [ "$DEBUG" == true ]; then
            run_cargo_suite "custom test ($TEST_NAME)" NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test "$TEST_NAME" -- --nocapture
        else
            run_cargo_suite "custom test ($TEST_NAME)" cargo test "$TEST_NAME" -- --nocapture
        fi
    # Run tests from TESTS array (default behavior - excludes tests in development)
    else
        echo "Running tests from TESTS array: ${TESTS[*]}"
        for test in "${TESTS[@]}"; do
            echo ""
            echo "Running test: $test"
            if [ "$DEBUG" == true ]; then
                run_cargo_suite "test $test (debug)" NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test "$test" -- --nocapture
            elif [ "$VERBOSE" == true ]; then
                run_cargo_suite "test $test (verbose)" cargo test "$test" -- --nocapture
            else
                run_cargo_suite "test $test" cargo test "$test"
            fi
        done
    fi

    echo ""
    echo "========================================="
    echo "Tests Complete!"
    echo "========================================="

    if [[ $exit_code -ne 0 ]]; then
        exit $exit_code
    fi
}

if ! $SKIP_BUILD; then
    check_git_clean
    build_reproducible
else
    echo "⚠️  Skipping build step (--no-build). Using existing WASM artifacts."
fi

run_tests
