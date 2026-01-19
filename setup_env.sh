#!/bin/bash
# setup.sh - Provisioning for Bevy CI Runner (Jules)

# 1. Exit immediately if a command exits with a non-zero status
set -e

echo ">>> Starting Environment Setup..."

# 2. Set Up Persistent Environment Variables for this session
# We also append them to .bashrc so they persist for future interactive sessions
export SCCACHE_DIR=/home/brandtb/.cache/sccache
export RUSTC_WRAPPER=sccache
export PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig:$PKG_CONFIG_PATH

# Ensure directories exist
mkdir -p "$SCCACHE_DIR"

# 3. Install System Dependencies
# Added 'libxkbcommon-x11-dev' (Critical for Bevy/Wayland resolution)
# Added 'lld' and 'clang' (Faster linking)
# Added 'wayland-protocols' (often needed for build scripts)
echo ">>> Installing System Packages..."
sudo apt-get update
sudo apt-get install -y \
    g++ \
    pkg-config \
    libx11-dev \
    libasound2-dev \
    libudev-dev \
    libwayland-dev \
    libxkbcommon-dev \
    libxkbcommon-x11-dev \
    wayland-protocols \
    libxi-dev \
    libxrandr-dev \
    lld \
    clang \
    curl

# 4. Install Rust (if not present)
if ! command -v cargo &> /dev/null; then
    echo ">>> Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo ">>> Rust is already installed."
fi

# 5. Install sccache (The Compiler Cache)
if ! command -v sccache &> /dev/null; then
    echo ">>> Installing sccache..."
    cargo install sccache
else
    echo ">>> sccache is already installed."
fi

# 6. Configure the Runner Environment (Persistent)
# This assumes the runner uses .bashrc. If it uses a specific .env file 
# for the runner service, you might need to append to that file instead.
echo ">>> Persisting Environment Variables..."
CONFIG_FILE="$HOME/.bashrc"

# Function to safely append env var if missing
append_if_missing() {
    grep -qF "$1" "$2" || echo "$1" >> "$2"
}

append_if_missing 'export SCCACHE_DIR=/home/brandtb/.cache/sccache' "$CONFIG_FILE"
append_if_missing 'export RUSTC_WRAPPER=sccache' "$CONFIG_FILE"
# Use the fast LLD linker by default for all Rust builds
append_if_missing 'export RUSTFLAGS="-C link-arg=-fuse-ld=lld"' "$CONFIG_FILE"

echo ">>> Setup Complete!"
echo "    SCCACHE_DIR: $SCCACHE_DIR"
echo "    Cache Stats: $(sccache -s | grep 'Cache location' || echo 'Not running yet')"