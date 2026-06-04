#!/bin/bash
set -e

ROOT="$(cd "$(dirname "$0")" && pwd)"

LIB="$ROOT/core/target/aarch64-apple-darwin/release/libtokenviewer_core.a"
NEWEST_SRC=$(find "$ROOT/core/src" -name "*.rs" -newer "$LIB" 2>/dev/null | head -1)

if [ -z "$NEWEST_SRC" ] && [ -f "$LIB" ]; then
    echo "⏭ Rust unchanged, skipping"
else
    echo "▶ Building Rust core..."
    cd "$ROOT/core"
    cargo build --release --target aarch64-apple-darwin 2>&1 | grep -E "^error" || true
fi

echo "▶ Building Swift app..."
cd "$ROOT/macos"
BUILD_LOG=$(xcodebuild -scheme TokenViewer -configuration Release -derivedDataPath "$ROOT/DerivedData" build 2>&1)
echo "$BUILD_LOG" | grep -E "(BUILD|error:)" | head -5
echo "$BUILD_LOG" | grep -q "BUILD SUCCEEDED" || { echo "❌ Build failed"; exit 1; }

echo "▶ Launching..."
pkill -f "TokenViewer.app" 2>/dev/null || true; sleep 0.5
APP=$(find "$ROOT/DerivedData/Build/Products/Release" -name "TokenViewer.app" -maxdepth 1 2>/dev/null | head -1)
if [ -z "$APP" ]; then echo "❌ App not found"; exit 1; fi
echo "  $APP"
APP_BIN="$APP/Contents/MacOS/TokenViewer"
if [ ! -x "$APP_BIN" ]; then echo "❌ App binary not found"; exit 1; fi
if [[ "$*" == *"--skip-sync"* ]]; then
    TV_SKIP_SYNC=1 "$APP_BIN" >/dev/null 2>&1 &
else
    "$APP_BIN" >/dev/null 2>&1 &
fi
echo "✅ Done"
