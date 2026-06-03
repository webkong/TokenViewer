#!/usr/bin/env bash

load_version_config() {
  local default_env_file="$ROOT_DIR/script/version.env"
  local env_file="${VERSION_ENV_FILE:-$default_env_file}"

  if [[ -f "$env_file" ]]; then
    # shellcheck source=/dev/null
    source "$env_file"
  fi

  if [[ -z "${VERSION:-}" || -z "${BUILD_NUMBER:-}" ]]; then
    echo "VERSION and BUILD_NUMBER must be set, either in $env_file or as environment variables." >&2
    exit 1
  fi
}
