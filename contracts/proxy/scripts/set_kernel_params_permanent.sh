#!/bin/bash

# Script to permanently set required kernel parameters for NEAR sandbox
# These settings will persist across reboots

echo "========================================="
echo "Permanently Setting NEAR Sandbox Kernel Parameters"
echo "========================================="

# Create the sysctl configuration file
echo ""
echo "Creating /etc/sysctl.d/99-near-sandbox.conf..."

sudo tee /etc/sysctl.d/99-near-sandbox.conf > /dev/null << EOF
# NEAR Sandbox required kernel parameters
# These settings are required for near-sandbox-rs to function properly

# Maximum socket receive buffer size
net.core.rmem_max = 8388608

# Maximum socket write buffer size
net.core.wmem_max = 8388608

# TCP read buffer sizes (min, default, max)
net.ipv4.tcp_rmem = 4096 87380 8388608

# TCP write buffer sizes (min, default, max)
net.ipv4.tcp_wmem = 4096 16384 8388608

# Disable TCP slow start after idle (improves performance for bursty workloads)
net.ipv4.tcp_slow_start_after_idle = 0
EOF

echo "Configuration file created."

# Apply the settings
echo ""
echo "Applying settings..."
sudo sysctl -p /etc/sysctl.d/99-near-sandbox.conf

echo ""
echo "========================================="
echo "Permanent configuration complete!"
echo "========================================="
echo ""
echo "Current values:"
echo "net.core.rmem_max = $(sysctl -n net.core.rmem_max)"
echo "net.core.wmem_max = $(sysctl -n net.core.wmem_max)"
echo "net.ipv4.tcp_rmem = $(sysctl -n net.ipv4.tcp_rmem)"
echo "net.ipv4.tcp_wmem = $(sysctl -n net.ipv4.tcp_wmem)"
echo "net.ipv4.tcp_slow_start_after_idle = $(sysctl -n net.ipv4.tcp_slow_start_after_idle)"
echo ""
echo "These settings will now persist across reboots."

