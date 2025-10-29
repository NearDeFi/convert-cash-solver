# NEAR Sandbox Setup Scripts

These scripts configure the required Linux kernel parameters for running `near-sandbox-rs`.

## Required Kernel Parameters

The NEAR sandbox requires these specific kernel parameters:

-   `net.core.rmem_max = 8388608` - Maximum socket receive buffer size
-   `net.core.wmem_max = 8388608` - Maximum socket write buffer size
-   `net.ipv4.tcp_rmem = 4096 87380 8388608` - TCP read buffer sizes (min, default, max)
-   `net.ipv4.tcp_wmem = 4096 16384 8388608` - TCP write buffer sizes (min, default, max)
-   `net.ipv4.tcp_slow_start_after_idle = 0` - Disable TCP slow start after idle

## Scripts

### `set_kernel_params.sh` - Temporary Configuration

Sets the kernel parameters for the current session only (lost on reboot).

**Usage:**

```bash
./scripts/set_kernel_params.sh
```

**When to use:**

-   Quick testing
-   You don't have permanent sudo access
-   First-time setup to verify it works

### `set_kernel_params_permanent.sh` - Permanent Configuration

Creates a persistent sysctl configuration file that survives reboots.

**Usage:**

```bash
./scripts/set_kernel_params_permanent.sh
```

**When to use:**

-   You regularly run NEAR sandbox tests
-   You want a one-time setup
-   Recommended for development machines

## Verification

After running either script, verify the settings:

```bash
# Check receive buffer max
sysctl net.core.rmem_max
# Should output: net.core.rmem_max = 8388608

# Check write buffer max
sysctl net.core.wmem_max
# Should output: net.core.wmem_max = 8388608

# Check TCP read buffer
sysctl net.ipv4.tcp_rmem
# Should output: net.ipv4.tcp_rmem = 4096 87380 8388608

# Check TCP write buffer
sysctl net.ipv4.tcp_wmem
# Should output: net.ipv4.tcp_wmem = 4096 16384 8388608

# Check TCP slow start after idle
sysctl net.ipv4.tcp_slow_start_after_idle
# Should output: net.ipv4.tcp_slow_start_after_idle = 0
```

## Troubleshooting

### Permission Denied

If you get a permission error, make sure the scripts are executable:

```bash
chmod +x scripts/*.sh
```

### Sudo Password Required

Both scripts require sudo access. You'll be prompted for your password.

### Changes Not Persisting

If you used `set_kernel_params.sh` (temporary), the changes will be lost on reboot.
Use `set_kernel_params_permanent.sh` for persistent changes.

## Manual Configuration

If you prefer to set these manually without scripts:

**Temporary:**

```bash
sudo sysctl -w net.core.rmem_max=8388608
sudo sysctl -w net.core.wmem_max=8388608
sudo sysctl -w net.ipv4.tcp_rmem="4096 87380 8388608"
sudo sysctl -w net.ipv4.tcp_wmem="4096 16384 8388608"
sudo sysctl -w net.ipv4.tcp_slow_start_after_idle=0
```

**Permanent:**

```bash
sudo tee /etc/sysctl.d/99-near-sandbox.conf > /dev/null << EOF
net.core.rmem_max = 8388608
net.core.wmem_max = 8388608
net.ipv4.tcp_rmem = 4096 87380 8388608
net.ipv4.tcp_wmem = 4096 16384 8388608
net.ipv4.tcp_slow_start_after_idle = 0
EOF
sudo sysctl -p /etc/sysctl.d/99-near-sandbox.conf
```

## Why Are These Required?

The NEAR sandbox simulates a blockchain network locally and requires larger network buffers to handle the traffic between nodes and RPC endpoints efficiently. Without these settings, you'll see errors like:

```
ERROR: net.core.rmem_max is set to 212992, expected 8388608
ERROR: net.core.wmem_max is set to 212992, expected 8388608
ERROR: net.ipv4.tcp_rmem is set to 4096 131072 6291456, expected 4096 87380 8388608
ERROR: net.ipv4.tcp_wmem is set to 4096 16384 4194304, expected 4096 16384 8388608
ERROR: net.ipv4.tcp_slow_start_after_idle is set to 1, expected 0
```

## More Information

-   [near-sandbox-rs Documentation](https://docs.rs/near-sandbox)
-   [Linux sysctl Documentation](https://www.kernel.org/doc/Documentation/sysctl/net.txt)
