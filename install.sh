#!/bin/sh
set -e

REPO="Tahurae/ingenious"
INSTALL_DIR="${PREFIX:-/usr/local}/bin"

echo "Installing ilang..."

# Detect Architecture
ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

case "$ARCH" in
    x86_64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Download pre-compiled release binary
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/ilang-${OS}-${ARCH}"

mkdir -p "$INSTALL_DIR"
curl -sSL "$DOWNLOAD_URL" -o "$INSTALL_DIR/ilang"
chmod +x "$INSTALL_DIR/ilang"

echo "ilang installed successfully to $INSTALL_DIR/ilang!"
echo "Run 'ilang --version' or 'ilang note_memory.i' to get started."
