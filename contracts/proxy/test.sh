#!/bin/bash

# Script to build and test the NEAR smart contract with sandbox

set -e  # Exit on error

echo "========================================="
echo "Building NEAR Smart Contract"
echo "========================================="

# Check if cargo-near is installed
if ! command -v cargo-near &> /dev/null; then
    echo "cargo-near is not installed. Installing..."
    cargo install cargo-near
fi

# Build the contract
echo "Building contract WASM..."
cargo near build

echo ""
echo "========================================="
echo "Running Tests"
echo "========================================="

# Run tests with options
if [ "$1" == "--verbose" ] || [ "$1" == "-v" ]; then
    echo "Running tests with verbose output..."
    NEAR_ENABLE_SANDBOX_LOG=1 cargo test -- --nocapture
elif [ "$1" == "--debug" ] || [ "$1" == "-d" ]; then
    echo "Running tests with debug logging..."
    NEAR_ENABLE_SANDBOX_LOG=1 NEAR_SANDBOX_LOG=debug cargo test -- --nocapture
elif [ "$1" == "--test" ]; then
    echo "Running specific test: $2"
    cargo test "$2" -- --nocapture
else
    echo "Running tests..."
    cargo test
fi

echo ""
echo "========================================="
echo "Tests Complete!"
echo "========================================="

