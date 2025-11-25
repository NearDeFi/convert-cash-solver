#!/bin/bash

# Local test runner that preserves reproducible WASM artifacts

set -euo pipefail

show_usage() {
    cat <<'USAGE'
Usage: ./test_local_repro.sh [OPTIONS] [TEST_NAME]

Options:
  (no args)           Build reproducible WASM artifacts and run all tests (quiet)
  -v, --verbose       Run tests with output (--nocapture)
  -d, --debug         Run tests with sandbox debug logging
  -t, --test NAME     Run specific test with output
  --no-build          Skip reproducible build step
  --keep-going        Do not exit immediately if tests fail
  -h, --help          Show this help message

Examples:
  ./test_local_repro.sh
  ./test_local_repro.sh --no-build -v
  ./test_local_repro.sh -t sandbox_test::test_mock_ft_deployment_only
USAGE
}

SKIP_BUILD=false
VERBOSE=false
DEBUG=false
KEEP_GOING=false
TEST_NAME=""

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

    if [[ -n "$TEST_NAME" ]]; then
        if $DEBUG; then
            run_cargo_suite "custom test ($TEST_NAME)" NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test "$TEST_NAME" -- --nocapture
        else
            run_cargo_suite "custom test ($TEST_NAME)" cargo test "$TEST_NAME" -- --nocapture
        fi
    else
        # lib/unit tests
        if $DEBUG; then
            run_cargo_suite "unit tests (debug)" NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test --lib -- --nocapture
        elif $VERBOSE; then
            run_cargo_suite "unit tests (verbose)" cargo test --lib -- --nocapture
        else
            run_cargo_suite "unit tests" cargo test --lib
        fi

        # integration tests (each file)
        local suites=(
            sandbox_test
            test_fifo_redemption_queue
            test_lender_profit
            test_lender_redemption_queue
            test_multi_lender_redemption_queue
            test_solver_borrow
            test_vault_deposit
            test_withdrawals
        )

        for suite in "${suites[@]}"; do
            if $DEBUG; then
                run_cargo_suite "integration test $suite (debug)" NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test --test "$suite" -- --nocapture
            elif $VERBOSE; then
                run_cargo_suite "integration test $suite (verbose)" cargo test --test "$suite" -- --nocapture
            else
                run_cargo_suite "integration test $suite" cargo test --test "$suite"
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

