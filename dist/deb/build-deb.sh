#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$(dirname "$SCRIPT_DIR")")"
BINARY="$REPO_ROOT/target/release/cluihud"
VERSION="0.1.0"
PKG_NAME="cluihud_${VERSION}_amd64"
BUILD_DIR="$SCRIPT_DIR/$PKG_NAME"

if [[ ! -f "$BINARY" ]]; then
    echo "Release binary not found. Run 'cargo build --release' first."
    exit 1
fi

rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR/DEBIAN"
mkdir -p "$BUILD_DIR/usr/local/bin"
mkdir -p "$BUILD_DIR/usr/share/applications"

cp "$BINARY" "$BUILD_DIR/usr/local/bin/cluihud"
chmod 755 "$BUILD_DIR/usr/local/bin/cluihud"

cp "$REPO_ROOT/dist/cluihud.desktop" "$BUILD_DIR/usr/share/applications/cluihud.desktop"

INSTALLED_SIZE=$(du -sk "$BUILD_DIR/usr" | cut -f1)

cat > "$BUILD_DIR/DEBIAN/control" << EOF
Package: cluihud
Version: $VERSION
Section: devel
Priority: optional
Architecture: amd64
Depends: libxkbcommon-x11-0, libxcb1, libvulkan1
Installed-Size: $INSTALLED_SIZE
Maintainer: Felipe <felipe@cluihud.dev>
Description: Desktop wrapper for Claude Code
 GPU-accelerated desktop application that wraps the Claude Code CLI
 with plan editing, task management, and hook integration.
EOF

dpkg-deb --build --root-owner-group "$BUILD_DIR"

rm -rf "$BUILD_DIR"
echo "Built $SCRIPT_DIR/${PKG_NAME}.deb"
