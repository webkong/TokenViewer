#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SIGNING_DIR="${SIGNING_DIR:-$ROOT_DIR/signing}"
KEY_NAME="${SELF_SIGNED_KEY_NAME:-tokenviewer-internal-codesign}"
COMMON_NAME="${SELF_SIGNED_COMMON_NAME:-TokenViewer Internal Code Signing}"
P12_PASSWORD="${SELF_SIGNED_P12_PASSWORD:-}"
KEYCHAIN_PASSWORD="${SELF_SIGNED_KEYCHAIN_PASSWORD:-}"
KEYCHAIN_DIR="${SELF_SIGNED_KEYCHAIN_DIR:-$HOME/Library/Keychains}"

OPENSSL_CONFIG="$SIGNING_DIR/$KEY_NAME-openssl.cnf"
PRIVATE_KEY_PATH="$SIGNING_DIR/$KEY_NAME.key"
CERT_PATH="$SIGNING_DIR/$KEY_NAME.cer"
P12_PATH="$SIGNING_DIR/$KEY_NAME.p12"
KEYCHAIN_PATH="$KEYCHAIN_DIR/$KEY_NAME.keychain-db"
ENV_PATH="$SIGNING_DIR/$KEY_NAME.env"

usage() {
  cat <<EOF
Usage: $0 <command>

Commands:
  generate   Create or refresh a self-signed code signing certificate and .p12 bundle
  import     Import the .p12 bundle into a project-local keychain for codesign
  identity   Print the common name that build scripts should use

Environment:
  SELF_SIGNED_COMMON_NAME=...      Certificate common name
  SELF_SIGNED_KEY_NAME=...         Base filename prefix inside signing/
  SELF_SIGNED_P12_PASSWORD=...     Password for the exported .p12 bundle
  SELF_SIGNED_KEYCHAIN_PASSWORD=... Password used for the project keychain
EOF
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

load_existing_env() {
  if [[ -f "$ENV_PATH" ]]; then
    # shellcheck source=/dev/null
    source "$ENV_PATH"
    COMMON_NAME="${SELF_SIGNED_COMMON_NAME:-$COMMON_NAME}"
    P12_PATH="${SELF_SIGNED_P12_PATH:-$P12_PATH}"
    P12_PASSWORD="${SELF_SIGNED_P12_PASSWORD:-$P12_PASSWORD}"
    KEYCHAIN_PATH="${SELF_SIGNED_KEYCHAIN_PATH:-$KEYCHAIN_PATH}"
    KEYCHAIN_PASSWORD="${SELF_SIGNED_KEYCHAIN_PASSWORD:-$KEYCHAIN_PASSWORD}"
  fi
}

ensure_passwords() {
  if [[ -z "$P12_PASSWORD" ]]; then
    P12_PASSWORD="$(openssl rand -hex 16)"
  fi

  if [[ -z "$KEYCHAIN_PASSWORD" ]]; then
    KEYCHAIN_PASSWORD="$(openssl rand -hex 16)"
  fi
}

write_openssl_config() {
  mkdir -p "$SIGNING_DIR"
  cat >"$OPENSSL_CONFIG" <<EOF
[ req ]
default_bits = 2048
prompt = no
default_md = sha256
distinguished_name = dn
x509_extensions = v3_codesign

[ dn ]
CN = $COMMON_NAME

[ v3_codesign ]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid,issuer
EOF
}

write_env_file() {
  cat >"$ENV_PATH" <<EOF
SELF_SIGNED_COMMON_NAME=$(printf '%q' "$COMMON_NAME")
SELF_SIGNED_P12_PATH=$(printf '%q' "$P12_PATH")
SELF_SIGNED_P12_PASSWORD=$(printf '%q' "$P12_PASSWORD")
SELF_SIGNED_KEYCHAIN_PATH=$(printf '%q' "$KEYCHAIN_PATH")
SELF_SIGNED_KEYCHAIN_PASSWORD=$(printf '%q' "$KEYCHAIN_PASSWORD")
EOF
  chmod 600 "$ENV_PATH"
}

generate_certificate() {
  require_command openssl
  load_existing_env
  ensure_passwords
  write_openssl_config

  openssl req \
    -new \
    -newkey rsa:2048 \
    -nodes \
    -x509 \
    -days 3650 \
    -config "$OPENSSL_CONFIG" \
    -extensions v3_codesign \
    -keyout "$PRIVATE_KEY_PATH" \
    -out "$CERT_PATH"

  openssl pkcs12 \
    -export \
    -inkey "$PRIVATE_KEY_PATH" \
    -in "$CERT_PATH" \
    -name "$COMMON_NAME" \
    -out "$P12_PATH" \
    -keypbe PBE-SHA1-3DES \
    -certpbe PBE-SHA1-3DES \
    -passout "pass:$P12_PASSWORD"

  chmod 600 "$PRIVATE_KEY_PATH" "$P12_PATH"
  write_env_file

  echo "Generated self-signed code signing bundle:"
  echo "  Certificate: $CERT_PATH"
  echo "  Private key: $PRIVATE_KEY_PATH"
  echo "  PKCS#12:     $P12_PATH"
  echo "  Env file:    $ENV_PATH"
}

ensure_keychain() {
  require_command security
  mkdir -p "$KEYCHAIN_DIR"

  if [[ ! -f "$KEYCHAIN_PATH" ]]; then
    security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
  fi

  security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
  security set-keychain-settings -lut 21600 "$KEYCHAIN_PATH"
}

ensure_keychain_in_search_list() {
  local current_keychains=()
  local line

  while IFS= read -r line; do
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%\"}"
    line="${line#\"}"

    if [[ -n "$line" ]]; then
      current_keychains+=("$line")
    fi
  done < <(security list-keychains -d user)

  for existing in "${current_keychains[@]}"; do
    if [[ "$existing" == "$KEYCHAIN_PATH" ]]; then
      return 0
    fi
  done

  security list-keychains -d user -s "$KEYCHAIN_PATH" "${current_keychains[@]}"
}

import_certificate() {
  require_command security
  load_existing_env

  if [[ ! -f "$P12_PATH" ]]; then
    echo "Missing PKCS#12 bundle at $P12_PATH. Run '$0 generate' first." >&2
    exit 1
  fi

  ensure_passwords
  ensure_keychain

  security import "$P12_PATH" \
    -k "$KEYCHAIN_PATH" \
    -f pkcs12 \
    -P "$P12_PASSWORD" \
    -T /usr/bin/codesign \
    -T /usr/bin/security >/dev/null

  security set-key-partition-list \
    -S apple-tool:,apple:,codesign: \
    -s \
    -k "$KEYCHAIN_PASSWORD" \
    "$KEYCHAIN_PATH" >/dev/null

  security add-trusted-cert \
    -d \
    -r trustRoot \
    -k "$KEYCHAIN_PATH" \
    "$CERT_PATH" >/dev/null

  ensure_keychain_in_search_list

  write_env_file

  echo "Imported signing identity into project keychain:"
  echo "  Keychain: $KEYCHAIN_PATH"
  echo "  Identity: $COMMON_NAME"
}

print_identity() {
  echo "$COMMON_NAME"
}

case "${1:-}" in
  generate)
    generate_certificate
    ;;
  import)
    import_certificate
    ;;
  identity)
    print_identity
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
