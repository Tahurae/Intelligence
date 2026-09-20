#!/usr/bin/env sh
set -e

echo "==> Installing Intelligence Programming Language & Tools..."

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

    # Compile built-in note utilities promised in documentation
    if command -v gcc >/dev/null 2>&1; then
        gcc extensions/note_main.c -o "$BIN_DIR/note"
        gcc extensions/notes_main.c -o "$BIN_DIR/notes"
        chmod +x "$BIN_DIR/note" "$BIN_DIR/notes"
    fi

    echo "==> Successfully installed 'intelligence', 'note', and 'notes' to $BIN_DIR!"
else
    echo "Error: Rust/Cargo is required to build Intelligence from source."
    exit 1
fi
