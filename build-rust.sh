#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CORE_DIR="$SCRIPT_DIR/core"
MACOS_DIR="$SCRIPT_DIR/macos"

echo "=== Building Rust core (release) ==="
cd "$CORE_DIR"

# Detect architecture and build accordingly
ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ]; then
    RUST_TARGET="aarch64-apple-darwin"
else
    RUST_TARGET="x86_64-apple-darwin"
fi

# Check if we need cross-compilation (Rosetta Rust on arm64 Mac)
RUST_HOST=$(rustc -vV | grep host | cut -d' ' -f2)
if [ "$ARCH" = "arm64" ] && echo "$RUST_HOST" | grep -q "x86_64"; then
    echo "Cross-compiling: Rust host=$RUST_HOST, target=$RUST_TARGET"
    rustup target add "$RUST_TARGET" 2>/dev/null || true
    cargo build --release --target "$RUST_TARGET"
    LIB_DIR="target/$RUST_TARGET/release"
else
    cargo build --release
    LIB_DIR="target/release"
fi

echo ""
echo "=== Rust library built ==="
ls -lh "$LIB_DIR/libtokenviewer_core.a"

echo ""
echo "=== Verifying Swift typecheck ==="
cd "$MACOS_DIR"
swiftc -typecheck \
  -import-objc-header TokenViewer/Bridge/TokenViewer-Bridging-Header.h \
  TokenViewer/App/TokenViewerApp.swift \
  TokenViewer/ViewModels/UsageViewModel.swift \
  TokenViewer/Views/PopoverView.swift \
  TokenViewer/Views/UsageView.swift \
  TokenViewer/Views/SettingsView.swift \
  TokenViewer/Views/MainWindowView.swift \
  TokenViewer/Views/StatusBarController.swift \
  TokenViewer/Bridge/CoreBridge.swift \
  -sdk "$(xcrun --show-sdk-path)" \
  -target arm64-apple-macos14.0

echo ""
echo "=== All checks passed ==="
echo ""
echo "To build the macOS app:"
echo "  1. Install XcodeGen: brew install xcodegen"
echo "  2. cd $MACOS_DIR && xcodegen generate"
echo "  3. open TokenViewer.xcodeproj"
echo "  4. Build & Run (Cmd+R)"
echo ""
echo "Library location: $CORE_DIR/target/release/libtokenviewer_core.a"
