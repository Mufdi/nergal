#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
BINARY="$REPO_ROOT/target/release/cluihud"

if [[ ! -f "$BINARY" ]]; then
    echo "Release binary not found. Building..."
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml"
fi

echo "Installing cluihud binary to /usr/local/bin..."
sudo install -Dm755 "$BINARY" /usr/local/bin/cluihud

DESKTOP_DIR="$HOME/.local/share/applications"
mkdir -p "$DESKTOP_DIR"
install -Dm644 "$SCRIPT_DIR/cluihud.desktop" "$DESKTOP_DIR/cluihud.desktop"
echo "Installed desktop entry to $DESKTOP_DIR/cluihud.desktop"

echo ""
echo "Running cluihud setup to configure Claude Code hooks..."
cluihud setup

echo ""
echo "Done. You can launch cluihud from your application menu or run 'cluihud' in a terminal."
