#!/usr/bin/env bash
# Resolve path data, never shell code. Relative output paths are opt-in.
normalize_path() {
  local value="$1" mode="${2:-absolute}" part result="" rest
  case "$value" in
    '~'|'~/'*|'$HOME'|'$HOME/'*|'${HOME}'|'${HOME}/'*)
      case "${HOME:-}" in
        /*) ;;
        *) printf 'HOME must be absolute\n' >&2; return 2 ;;
      esac
      case "$HOME" in *'$'*|*'~'*|*[[:cntrl:]]*) printf 'HOME must be expanded\n' >&2; return 2 ;; esac
      case "$value" in
        '~'|'~/'*) value="$HOME${value:1}" ;;
        '$HOME'|'$HOME/'*) value="$HOME${value:5}" ;;
        '${HOME}'|'${HOME}/'*) value="$HOME${value:7}" ;;
      esac ;;
  esac
  case "$value" in
    ''|*'$'*|*'~'*|*[[:cntrl:]]*) printf 'Invalid or unresolved path\n' >&2; return 2 ;;
  esac
  if [[ "$value" != /* ]]; then
    [[ "$mode" == relative ]] || { printf 'Path must be absolute\n' >&2; return 2; }
    value="$PWD/$value"
  fi
  case "$value" in *'$'*|*'~'*|*[[:cntrl:]]*) printf 'Invalid working directory\n' >&2; return 2 ;; esac
  rest="${value#/}"
  while [[ -n "$rest" ]]; do
    part="${rest%%/*}"
    if [[ "$rest" == */* ]]; then rest="${rest#*/}"; else rest=""; fi
    case "$part" in
      ''|.) ;;
      ..) result="${result%/*}" ;;
      *) result="$result/$part" ;;
    esac
  done
  printf '%s\n' "${result:-/}"
}
