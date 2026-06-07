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

SELF_SIGNED_ENV_FILE="${SELF_SIGNED_ENV_FILE:-$ROOT_DIR/signing/tokenviewer-internal-codesign.env}"
CODE_SIGN_IDENTITY="${CODE_SIGN_IDENTITY:--}"
DEVELOPMENT_TEAM="${DEVELOPMENT_TEAM:-}"
KEYCHAIN_ARGS=()

load_self_signed_env() {
  [[ -f "$SELF_SIGNED_ENV_FILE" ]] && source "$SELF_SIGNED_ENV_FILE" || true
}

prepare_self_signed_identity() {
  load_self_signed_env

  if [[ "$CODE_SIGN_IDENTITY" != "-" && -n "$CODE_SIGN_IDENTITY" ]]; then
    return 0
  fi

  if [[ -z "${SELF_SIGNED_COMMON_NAME:-}" || -z "${SELF_SIGNED_P12_PATH:-}" || -z "${SELF_SIGNED_P12_PASSWORD:-}" ]]; then
    return 1
  fi

  if [[ ! -f "$SELF_SIGNED_P12_PATH" ]]; then
    return 1
  fi

  if [[ -z "${SELF_SIGNED_KEYCHAIN_PATH:-}" || -z "${SELF_SIGNED_KEYCHAIN_PASSWORD:-}" ]]; then
    return 1
  fi

  "$ROOT_DIR/script/self_signed_codesign.sh" import >/dev/null
  CODE_SIGN_IDENTITY="$SELF_SIGNED_COMMON_NAME"
  KEYCHAIN_ARGS=(--keychain "$SELF_SIGNED_KEYCHAIN_PATH")
  return 0
}

require_stable_code_sign_identity() {
  if [[ "$CODE_SIGN_IDENTITY" != "-" && -n "$CODE_SIGN_IDENTITY" ]]; then
    return 0
  fi

  if prepare_self_signed_identity; then
    return 0
  fi

  cat >&2 <<EOF
error: A stable signing identity is required for release builds.

Use either:
  CODE_SIGN_IDENTITY="Apple Development: Your Name (TEAMID)" DEVELOPMENT_TEAM=TEAMID $0 build-dmg
or import the project self-signed identity for internal testing:
  ./script/self_signed_codesign.sh generate
  ./script/self_signed_codesign.sh import
EOF
  exit 1
}

DMG_PATH="$RELEASE_DIR/$APP_DISPLAY_NAME.dmg"
ZIP_PATH="$RELEASE_DIR/$APP_DISPLAY_NAME-$VERSION.zip"

PKG_PATH="$RELEASE_DIR/$APP_DISPLAY_NAME-$VERSION-Installer.pkg"
PKG_ALIAS_PATH="$RELEASE_DIR/$APP_DISPLAY_NAME-Installer.pkg"

# ─── Helpers ──────────────────────────────────────────────────────────────────

usage() {
  cat <<EOF
Usage: $0 <command>

Commands:
  build-rust      Build Rust core (aarch64-apple-darwin release)
  build-app       Build Xcode release .app
  build-zip       Build .app and create zip
  build-pkg       Build .app and create PKG installer (auto-removes quarantine)
  build-dmg       Build .app and create DMG
  build-website   Build website/dist
  push-website    Push built website to GitHub Pages branch
  push-release    Upload DMG + ZIP to GitHub Release $RELEASE_TAG
  all             build-dmg + build-website + push-website + push-release

Environment:
  VERSION=x.y.z           Overrides script/version.env
  BUILD_NUMBER=NNNNN       Overrides script/version.env
  CODE_SIGN_IDENTITY=...   Codesign identity (default: project self-signed identity when available)
  DEVELOPMENT_TEAM=...     Team ID to pass through to Xcode signing
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

app_signature_report() {
  codesign -dv --verbose=4 "$1" 2>&1 || return 1
}

validate_non_adhoc_signature() {
  local app="$1"
  local context="$2"
  local signature_info

  signature_info="$(app_signature_report "$app")" || {
    echo "Missing or invalid code signature for $context: $app" >&2
    return 1
  }

  if grep -q "Signature=adhoc" <<<"$signature_info"; then
    echo "Refusing to package ad-hoc signed app for $context: $app" >&2
    echo "Configure Xcode signing or pass CODE_SIGN_IDENTITY / DEVELOPMENT_TEAM explicitly." >&2
    return 1
  fi

  if ! grep -q "^Authority=" <<<"$signature_info"; then
    echo "Refusing to package app without signing authority for $context: $app" >&2
    return 1
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
  require_stable_code_sign_identity

  # Patch version into project.yml settings
  cd "$MACOS_DIR"

  echo "▶ Building Xcode app (v$VERSION / $BUILD_NUMBER)..."
  mkdir -p "$RELEASE_DIR"
  rm -rf "$RELEASE_DIR/$APP_DISPLAY_NAME.app"
  local derived_data_path
  derived_data_path="$(mktemp -d "$DIST_DIR/xcode-build.XXXXXX")"

  local xcode_sign_args=()
  local skip_xcode_signing=0
  if [[ -n "$CODE_SIGN_IDENTITY" && "$CODE_SIGN_IDENTITY" != "${SELF_SIGNED_COMMON_NAME:-}" ]]; then
    xcode_sign_args+=("CODE_SIGN_IDENTITY=$CODE_SIGN_IDENTITY")
    xcode_sign_args+=("CODE_SIGN_STYLE=Manual")
  else
    skip_xcode_signing=1
  fi
  if [[ -n "$DEVELOPMENT_TEAM" ]]; then
    xcode_sign_args+=("DEVELOPMENT_TEAM=$DEVELOPMENT_TEAM")
  fi
  if [[ "$skip_xcode_signing" == "1" ]]; then
    xcode_sign_args+=("CODE_SIGNING_ALLOWED=NO")
    xcode_sign_args+=("CODE_SIGNING_REQUIRED=NO")
  fi

  xcodebuild \
    -scheme "$SCHEME" \
    -configuration Release \
    -derivedDataPath "$derived_data_path" \
    MARKETING_VERSION="$VERSION" \
    CURRENT_PROJECT_VERSION="$BUILD_NUMBER" \
    "${xcode_sign_args[@]}" \
    build

  local built_app
  built_app="$(find "$derived_data_path/Build/Products/Release" -name "*.app" -maxdepth 1 | head -1)"
  if [[ -z "$built_app" ]]; then
    echo "App not found after build" >&2; exit 1
  fi

  cp -R "$built_app" "$RELEASE_DIR/"
  rm -rf "$derived_data_path"
  echo "✓ Built: $RELEASE_DIR/$APP_DISPLAY_NAME.app"
}

# ─── Codesign ─────────────────────────────────────────────────────────────────

codesign_app() {
  local app="$RELEASE_DIR/$APP_DISPLAY_NAME.app"
  codesign --force --deep --sign "$CODE_SIGN_IDENTITY" \
    ${KEYCHAIN_ARGS[@]+"${KEYCHAIN_ARGS[@]}"} \
    "$app" >/dev/null
  echo "✓ Signed with: $CODE_SIGN_IDENTITY"

  validate_non_adhoc_signature "$app" "release app"
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

# ─── PKG ──────────────────────────────────────────────────────────────────────

build_pkg() {
  require_command pkgbuild

  build_app
  codesign_app

  local work_dir; work_dir="$(mktemp -d)"
  local root_dir="$work_dir/root"
  local scripts_dir="$work_dir/scripts"
  mkdir -p "$root_dir/Applications" "$scripts_dir"
  cp -R "$RELEASE_DIR/$APP_DISPLAY_NAME.app" "$root_dir/Applications/"

  # preinstall: quit running app
  cat >"$scripts_dir/preinstall" <<PREINSTALL
#!/bin/bash
APP_PROCESS="$APP_DISPLAY_NAME"
if /usr/bin/pgrep -x "\$APP_PROCESS" >/dev/null 2>&1; then
  /usr/bin/pkill -x "\$APP_PROCESS" 2>/dev/null || true
  /bin/sleep 0.5
fi
exit 0
PREINSTALL

  # postinstall: remove quarantine + relaunch the app after a successful install.
  cat >"$scripts_dir/postinstall" <<POSTINSTALL
#!/bin/bash
set -euo pipefail
APP_PATH="/Applications/$APP_DISPLAY_NAME.app"

console_user() { /usr/bin/stat -f "%Su" /dev/console 2>/dev/null || true; }
run_as_user() {
  local user uid
  user="\$(console_user)"
  [[ -n "\$user" && "\$user" != "root" ]] || return 1
  uid="\$(/usr/bin/id -u "\$user")" || return 1
  /bin/launchctl asuser "\$uid" /usr/bin/sudo -u "\$user" "\$@"
}

# Remove quarantine so Gatekeeper doesn't block the app
if [[ -d "\$APP_PATH" ]]; then
  /usr/bin/xattr -dr com.apple.quarantine "\$APP_PATH" 2>/dev/null || true
fi

# Relaunch the installed app for interactive installs and updates.
[[ -d "\$APP_PATH" ]] && run_as_user /usr/bin/open "\$APP_PATH" >/dev/null 2>&1 || true
exit 0
POSTINSTALL

  chmod 755 "$scripts_dir/preinstall" "$scripts_dir/postinstall"
  rm -f "$PKG_PATH"
  rm -f "$PKG_ALIAS_PATH"

  # Disable bundle relocation so the app always lands in /Applications
  pkgbuild --analyze --root "$root_dir" "$work_dir/component.plist" >/dev/null 2>&1 || true
  if [[ -f "$work_dir/component.plist" ]]; then
    /usr/libexec/PlistBuddy -c "Set :0:BundleIsRelocatable false" "$work_dir/component.plist" 2>/dev/null || true
  fi

  local component_args=()
  if [[ -f "$work_dir/component.plist" ]]; then
    component_args=(--component-plist "$work_dir/component.plist")
  fi

  pkgbuild \
    --root "$root_dir" \
    --scripts "$scripts_dir" \
    --identifier "com.tokenviewer.app.pkg" \
    --version "$VERSION" \
    --install-location "/" \
    "${component_args[@]}" \
    "$PKG_PATH"

  cp -f "$PKG_PATH" "$PKG_ALIAS_PATH"

  rm -rf "$work_dir"
  echo "✓ PKG: $PKG_PATH"
  echo "✓ PKG alias: $PKG_ALIAS_PATH"

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
    ${bg_args[@]+"${bg_args[@]}"} \
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
      local assets=("$DMG_PATH" "$ZIP_PATH")
      [[ -f "$PKG_PATH" ]] && assets+=("$PKG_PATH")
      [[ -f "$PKG_ALIAS_PATH" ]] && assets+=("$PKG_ALIAS_PATH")
      gh release upload "$RELEASE_TAG" "${assets[@]}" \
        --repo "$GITHUB_RELEASE_REPO" --clobber
    else
      local assets=("$DMG_PATH" "$ZIP_PATH")
      [[ -f "$PKG_PATH" ]] && assets+=("$PKG_PATH")
      [[ -f "$PKG_ALIAS_PATH" ]] && assets+=("$PKG_ALIAS_PATH")
      gh release create "$RELEASE_TAG" "${assets[@]}" \
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
      "$DMG_PATH" "$ZIP_PATH" "$PKG_ALIAS_PATH"
  fi
  echo "✓ Released $RELEASE_TAG"
}

# ─── Main ─────────────────────────────────────────────────────────────────────

case "${1:-}" in
  build-rust)    build_rust ;;
  build-app)     build_app ;;
  build-zip)     build_zip ;;
  build-pkg)     build_pkg ;;
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
