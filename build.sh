#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
export OPENSSL_DIR="${OPENSSL_DIR:-/opt/homebrew/opt/openssl@3}"

echo "▶ Building test app..."
cd "$ROOT/macos"
xcodebuild \
  -project TokenViewer.xcodeproj \
  -scheme TokenViewer \
  -configuration Release \
  -derivedDataPath "$ROOT/DerivedData" \
  build

APP="$ROOT/DerivedData/Build/Products/Release/TokenViewer.app"
if [ ! -d "$APP" ]; then
  echo "❌ App not found: $APP"
  exit 1
fi

echo "✅ Test app built:"
echo "  $APP"
