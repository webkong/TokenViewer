#!/usr/bin/env bash
set -euo pipefail

APP_DISPLAY_NAME="TokenViewer"
BUNDLE_ID="com.tokenviewer.app"
SCHEME="TokenViewer"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MACOS_DIR="$ROOT_DIR/macos"
WEBSITE_DIR="$ROOT_DIR/website"
DIST_DIR="$ROOT_DIR/dist"
RELEASE_DIR="$DIST_DIR/release"

source "$ROOT_DIR/script/version_utils.sh"
load_version_config

WEBSITE_REPO_URL="${WEBSITE_REPO_URL:-git@github.com:webkong/TokenViewer.git}"
WEBSITE_BRANCH="${WEBSITE_BRANCH:-gh-pages}"
GITHUB_RELEASE_REPO="${GITHUB_RELEASE_REPO:-webkong/TokenViewer}"
RELEASE_TAG="${RELEASE_TAG:-v$VERSION}"
RELEASE_NOTES_FILE="${RELEASE_NOTES_FILE:-$ROOT_DIR/docs/releases/$RELEASE_TAG.md}"

DMG_PATH="$RELEASE_DIR/$APP_DISPLAY_NAME.dmg"
ZIP_PATH="$RELEASE_DIR/$APP_DISPLAY_NAME-$VERSION.zip"
CODE_SIGN_IDENTITY="${CODE_SIGN_IDENTITY:--}"

# ─── Helpers ──────────────────────────────────────────────────────────────────

usage() {
  cat <<EOF
Usage: $0 <command>

Commands:
  build-rust      Build Rust core (aarch64-apple-darwin release)
  build-app       Build Xcode release .app
  build-zip       Build .app and create zip
  build-dmg       Build .app and create DMG
  build-website   Build website/dist
  push-website    Push built website to GitHub Pages branch
  push-release    Upload DMG + ZIP to GitHub Release $RELEASE_TAG
  all             build-dmg + build-website + push-website + push-release

Environment:
  VERSION=x.y.z           Overrides script/version.env
  BUILD_NUMBER=NNNNN       Overrides script/version.env
  CODE_SIGN_IDENTITY=...   Codesign identity (default: ad-hoc '-')
  GITHUB_TOKEN=...         Required for push-release when gh not installed
  RELEASE_TAG=vX.Y.Z
  RELEASE_NOTES_FILE=docs/releases/vX.Y.Z.md
  SKIP_RUST_BUILD=1        Skip cargo build before Xcode build
  SKIP_DMG_BUILD=1         Skip DMG rebuild before push-release
  SKIP_WEBSITE_BUILD=1     Skip website rebuild before push-website
EOF
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

require_release_notes() {
  if [[ ! -f "$RELEASE_NOTES_FILE" ]]; then
    echo "Missing release notes file: $RELEASE_NOTES_FILE" >&2
    echo "Create docs/releases/$RELEASE_TAG.md before releasing." >&2
    exit 1
  fi
}

# ─── Build Rust ───────────────────────────────────────────────────────────────

build_rust() {
  require_command cargo
  echo "▶ Building Rust core (aarch64-apple-darwin)..."
  export PATH="$HOME/.cargo/bin:$PATH"
  cd "$ROOT_DIR/core"
  cargo build --release --target aarch64-apple-darwin
  echo "✓ Rust core built"
}

# ─── Build Xcode App ──────────────────────────────────────────────────────────

build_app() {
  if [[ "${SKIP_RUST_BUILD:-0}" != "1" ]]; then
    build_rust
  fi

  require_command xcodebuild

  # Patch version into project.yml settings
  cd "$MACOS_DIR"

  echo "▶ Building Xcode app (v$VERSION / $BUILD_NUMBER)..."
  mkdir -p "$RELEASE_DIR"

  xcodebuild \
    -scheme "$SCHEME" \
    -configuration Release \
    -derivedDataPath "$DIST_DIR/xcode-build" \
    MARKETING_VERSION="$VERSION" \
    CURRENT_PROJECT_VERSION="$BUILD_NUMBER" \
    build

  local built_app
  built_app="$(find "$DIST_DIR/xcode-build/Build/Products/Release" -name "*.app" -maxdepth 1 | head -1)"
  if [[ -z "$built_app" ]]; then
    echo "App not found after build" >&2; exit 1
  fi

  cp -R "$built_app" "$RELEASE_DIR/"
  echo "✓ Built: $RELEASE_DIR/$APP_DISPLAY_NAME.app"
}

# ─── Codesign ─────────────────────────────────────────────────────────────────

codesign_app() {
  local app="$RELEASE_DIR/$APP_DISPLAY_NAME.app"
  codesign --force --deep --sign "$CODE_SIGN_IDENTITY" "$app" >/dev/null
  echo "✓ Signed with: $CODE_SIGN_IDENTITY"
}

# ─── ZIP ──────────────────────────────────────────────────────────────────────

build_zip() {
  build_app
  codesign_app

  cd "$RELEASE_DIR"
  rm -f "$ZIP_PATH"
  zip -qr "$ZIP_PATH" "$APP_DISPLAY_NAME.app"
  echo "✓ ZIP: $ZIP_PATH"
}

# ─── DMG ──────────────────────────────────────────────────────────────────────

build_dmg() {
  require_command create-dmg

  build_zip

  local bg_png="$RELEASE_DIR/dmg-bg.png"
  python3 "$ROOT_DIR/script/gen_dmg_bg.py" "$bg_png" 2>/dev/null || true

  rm -f "$DMG_PATH"

  local bg_args=()
  [[ -f "$bg_png" ]] && bg_args=(--background "$bg_png")

  create-dmg \
    --volname "$APP_DISPLAY_NAME" \
    "${bg_args[@]}" \
    --window-pos 200 120 \
    --window-size 660 400 \
    --icon-size 128 \
    --icon "$APP_DISPLAY_NAME.app" 200 180 \
    --hide-extension "$APP_DISPLAY_NAME.app" \
    --app-drop-link 460 180 \
    "$DMG_PATH" \
    "$RELEASE_DIR/$APP_DISPLAY_NAME.app"

  echo "✓ DMG: $DMG_PATH"
}

# ─── Website ──────────────────────────────────────────────────────────────────

build_website() {
  require_command node
  echo "▶ Building website..."
  cd "$WEBSITE_DIR"
  if [[ -f package-lock.json ]]; then
    npm ci --cache ./.npm-cache
  else
    npm install --cache ./.npm-cache
  fi
  npm run build
  echo "✓ Website: $WEBSITE_DIR/dist"
}

push_website() {
  require_command git
  require_command rsync

  [[ "${SKIP_WEBSITE_BUILD:-0}" != "1" ]] && build_website

  local publish_dir="$DIST_DIR/website-repo"
  mkdir -p "$DIST_DIR"

  if [[ ! -d "$publish_dir/.git" ]]; then
    rm -rf "$publish_dir"
    if ! git clone "$WEBSITE_REPO_URL" -b "$WEBSITE_BRANCH" "$publish_dir" 2>/dev/null; then
      mkdir -p "$publish_dir"
      git -C "$publish_dir" init -b "$WEBSITE_BRANCH"
      git -C "$publish_dir" remote add origin "$WEBSITE_REPO_URL"
    fi
  fi

  git -C "$publish_dir" fetch origin "$WEBSITE_BRANCH" >/dev/null 2>&1 || true
  if git -C "$publish_dir" rev-parse --verify "origin/$WEBSITE_BRANCH" >/dev/null 2>&1; then
    git -C "$publish_dir" checkout "$WEBSITE_BRANCH"
    git -C "$publish_dir" reset --hard "origin/$WEBSITE_BRANCH"
  else
    git -C "$publish_dir" checkout -B "$WEBSITE_BRANCH"
  fi

  rsync -a --delete --exclude ".git" "$WEBSITE_DIR/dist/" "$publish_dir/"

  git -C "$publish_dir" add -A
  if git -C "$publish_dir" diff --cached --quiet; then
    echo "No website changes to push."
    return
  fi
  git -C "$publish_dir" commit -m "Deploy website v$VERSION"
  git -C "$publish_dir" push -u origin "$WEBSITE_BRANCH"
  echo "✓ Pushed website to $WEBSITE_REPO_URL ($WEBSITE_BRANCH)"
}

# ─── GitHub Release ───────────────────────────────────────────────────────────

push_release() {
  require_release_notes
  [[ "${SKIP_DMG_BUILD:-0}" != "1" ]] && build_dmg

  local notes
  notes="$(cat "$RELEASE_NOTES_FILE")"

  if command -v gh >/dev/null 2>&1; then
    local notes_file; notes_file="$(mktemp)"
    printf '%s\n' "$notes" >"$notes_file"

    if gh release view "$RELEASE_TAG" --repo "$GITHUB_RELEASE_REPO" >/dev/null 2>&1; then
      gh release edit "$RELEASE_TAG" --repo "$GITHUB_RELEASE_REPO" \
        --title "$APP_DISPLAY_NAME $VERSION" --notes-file "$notes_file"
      gh release upload "$RELEASE_TAG" "$DMG_PATH" "$ZIP_PATH" \
        --repo "$GITHUB_RELEASE_REPO" --clobber
    else
      gh release create "$RELEASE_TAG" "$DMG_PATH" "$ZIP_PATH" \
        --repo "$GITHUB_RELEASE_REPO" \
        --title "$APP_DISPLAY_NAME $VERSION" --notes-file "$notes_file"
    fi
    rm -f "$notes_file"
  else
    # Fallback: GitHub API via curl
    if [[ -z "${GITHUB_TOKEN:-}" ]]; then
      echo "GITHUB_TOKEN required when gh is not installed." >&2; exit 1
    fi
    python3 "$ROOT_DIR/script/upload_release.py" \
      "$GITHUB_RELEASE_REPO" "$RELEASE_TAG" \
      "$APP_DISPLAY_NAME $VERSION" "$RELEASE_NOTES_FILE" \
      "$DMG_PATH" "$ZIP_PATH"
  fi
  echo "✓ Released $RELEASE_TAG"
}

# ─── Main ─────────────────────────────────────────────────────────────────────

case "${1:-}" in
  build-rust)    build_rust ;;
  build-app)     build_app ;;
  build-zip)     build_zip ;;
  build-dmg)     build_dmg ;;
  build-website) build_website ;;
  push-website)  push_website ;;
  push-release)  push_release ;;
  all)
    build_dmg
    build_website
    push_website
    push_release
    ;;
  *) usage; exit 1 ;;
esac
