# YIK - Yeet Into Kernel

**YIK** (Yeet Into Kernel) is a terminal user interface (TUI) application for
Linux that allows you to easily select and switch between different kernel
versions using `kexec`.

## Installation

### From Source

1. **Prerequisites**:
   - Rust toolchain (1.70.0 or later)

2. **Clone and build**:
   ```bash
   git clone https://github.com/kaffarell/yik.git
   cd yik
   cargo build --release
   ```

3. **Install**:
   ```bash
   sudo cp target/release/yik /usr/local/bin/
   ```

### System Requirements

- Linux system with `kexec` support
- `/boot` directory with kernel files (`vmlinuz-*` pattern)
- Corresponding initrd files (`initrd.img-*` or `initramfs-*` pattern)
- `sudo` privileges

## Usage

### Basic Usage

Simply run YIK:

```bash
yik
```

### Interface Controls

**Kernel Selection Screen:**
- **↑/↓ or j/k** - Navigate through kernel versions
- **Enter** - Select kernel version and load it
- **q/Esc** - Quit application

**Confirmation Dialog:**
- **Y/Enter** - Proceed with kernel switch
- **n/Esc** - Cancel and return to kernel selection

**Error Dialog:**
- **Enter/Esc/q** - Return to kernel selection

### What Happens When You Select a Kernel

1. **Kernel Loading**: YIK executes:
   ```bash
   sudo kexec -l /boot/vmlinuz-<version> \
             --initrd=/boot/initrd.img-<version> \
             --command-line="$(cat /proc/cmdline)"
   ```

2. **Execution**: If you confirm with Y, YIK executes:
   ```bash
   sudo kexec -e
   ```

## Building Static Binary

For a portable static binary:

```bash
# Install musl target (if using rustup)
rustup target add x86_64-unknown-linux-musl

# Build static binary
cargo build --release --target x86_64-unknown-linux-musl

# Binary will be at: target/x86_64-unknown-linux-musl/release/yik
```
