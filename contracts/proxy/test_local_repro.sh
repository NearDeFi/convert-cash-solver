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

    if [[ -n "$TEST_NAME" ]]; then
        echo "Running specific test: $TEST_NAME"
        if $DEBUG; then
            NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test "$TEST_NAME" -- --nocapture
        else
            cargo test "$TEST_NAME" -- --nocapture
        fi
    elif $DEBUG; then
        echo "Running tests with debug logging..."
        NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test -- --nocapture
    elif $VERBOSE; then
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
}

if ! $SKIP_BUILD; then
    check_git_clean
    build_reproducible
else
    echo "⚠️  Skipping build step (--no-build). Using existing WASM artifacts."
fi

run_tests

