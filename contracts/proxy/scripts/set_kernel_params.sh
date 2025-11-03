#!/bin/bash

# Script to set required kernel parameters for NEAR sandbox
# This script sets the network buffer sizes required by near-sandbox-rs

echo "========================================="
echo "Setting NEAR Sandbox Kernel Parameters"
echo "========================================="

# Check current values
echo ""
echo "Current values:"
echo "net.core.rmem_max = $(sysctl -n net.core.rmem_max)"
echo "net.core.wmem_max = $(sysctl -n net.core.wmem_max)"
echo "net.ipv4.tcp_rmem = $(sysctl -n net.ipv4.tcp_rmem)"
echo "net.ipv4.tcp_wmem = $(sysctl -n net.ipv4.tcp_wmem)"
echo "net.ipv4.tcp_slow_start_after_idle = $(sysctl -n net.ipv4.tcp_slow_start_after_idle)"

echo ""
echo "Setting required values..."

# Set the parameters
sudo sysctl -w net.core.rmem_max=8388608
sudo sysctl -w net.core.wmem_max=8388608
sudo sysctl -w net.ipv4.tcp_rmem="4096 87380 8388608"
sudo sysctl -w net.ipv4.tcp_wmem="4096 16384 8388608"
sudo sysctl -w net.ipv4.tcp_slow_start_after_idle=0

echo ""
echo "New values:"
echo "net.core.rmem_max = $(sysctl -n net.core.rmem_max)"
echo "net.core.wmem_max = $(sysctl -n net.core.wmem_max)"
echo "net.ipv4.tcp_rmem = $(sysctl -n net.ipv4.tcp_rmem)"
echo "net.ipv4.tcp_wmem = $(sysctl -n net.ipv4.tcp_wmem)"
echo "net.ipv4.tcp_slow_start_after_idle = $(sysctl -n net.ipv4.tcp_slow_start_after_idle)"

echo ""
echo "========================================="
echo "Temporary configuration complete!"
echo "========================================="
echo ""
echo "To make these changes permanent across reboots, run:"
echo "  sudo ./scripts/set_kernel_params_permanent.sh"

