#!/usr/bin/env sh
set -e

echo "==> Installing Intelligence Programming Language..."

if [ -n "$TERMUX_VERSION" ] || [ -d "/data/data/com.termux/files/usr" ]; then
    BIN_DIR="/data/data/com.termux/files/usr/bin"
else
    BIN_DIR="${PREFIX:-/usr/local}/bin"
fi

mkdir -p "$BIN_DIR"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

git clone --depth 1 https://github.com/Tahurae/Intelligence.git "$TMP_DIR"
cd "$TMP_DIR"

if command -v cargo >/dev/null 2>&1; then
    cargo build --release
    
    if [ -f "target/release/intelligence" ]; then
        cp target/release/intelligence "$BIN_DIR/intelligence"
    elif [ -f "target/release/ilang" ]; then
        cp target/release/ilang "$BIN_DIR/intelligence"
    fi
    
    chmod +x "$BIN_DIR/intelligence"

    if command -v gcc >/dev/null 2>&1; then
        gcc extensions/editor_nano.c extensions/persistence_disk.c -o "$BIN_DIR/note" 2>/dev/null || true
        chmod +x "$BIN_DIR/note" 2>/dev/null || true
    fi

else
    echo "Error: Rust/Cargo is required to build Intelligence from source."
    exit 1
fi
