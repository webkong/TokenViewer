#!/usr/bin/env bash
#
# Deterministic Windows asset generation (offline, no network).
#
# Produces, under windows/TokenViewer/Resources/:
#   - brand-logos/*.png    (SVG logos rasterized to 64x64, existing PNGs copied)
#   - TokenViewer.ico      (16/20/24/32/48/64/128/256 px multi-size app icon)
#
# Tools required (present on the macOS dev host / GitHub runner):
#   - sips      (macOS built-in; SVG -> PNG via ImageIO/CoreSVG)
#   - python3   (ICO container assembly)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC_LOGOS="$ROOT/macos/TokenViewer/Resources/brand-logos"
OUT_LOGOS="$ROOT/windows/TokenViewer/Resources/brand-logos"
OUT_ICO="$ROOT/windows/TokenViewer/Resources/TokenViewer.ico"
APP_LOGO="$ROOT/macos/TokenViewer/Resources/AppLogo.svg"

if ! command -v sips >/dev/null 2>&1; then
  echo "error: sips not found (required for SVG rasterization)" >&2
  exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "error: python3 not found (required for ICO assembly)" >&2
  exit 1
fi

mkdir -p "$OUT_LOGOS"

# Clear only the generated brand-logo PNGs so a source SVG that was deleted
# does not leave a stale PNG behind (source SVG/PNG files are untouched).
rm -f "$OUT_LOGOS"/*.png

# 1) Brand logos: rasterize every SVG to a 64x64 PNG; copy existing PNGs.
for svg in "$SRC_LOGOS"/*.svg; do
  name="$(basename "$svg" .svg)"
  out="$OUT_LOGOS/$name.png"
  tmp="$OUT_LOGOS/.$name.png"
  sips -s format png "$svg" --out "$tmp" >/dev/null 2>&1
  sips -z 64 64 "$tmp" --out "$out" >/dev/null 2>&1
  rm -f "$tmp"
done

for png in "$SRC_LOGOS"/*.png; do
  cp "$png" "$OUT_LOGOS/$(basename "$png")"
done

# 2) App icon: rasterize AppLogo.svg at the ICO sizes, then assemble the ICO.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
for size in 16 20 24 32 48 64 128 256; do
  full="$TMP/full-$size.png"
  sips -s format png "$APP_LOGO" --out "$full" >/dev/null 2>&1
  sips -z "$size" "$size" "$full" --out "$TMP/icon-$size.png" >/dev/null 2>&1
done

python3 - "$TMP" "$OUT_ICO" <<'PY'
import sys, os, struct
tmp, out = sys.argv[1], sys.argv[2]
sizes = [16, 20, 24, 32, 48, 64, 128, 256]
entries = []
for s in sizes:
    with open(os.path.join(tmp, f"icon-{s}.png"), "rb") as f:
        entries.append((s, f.read()))

# ICO header: reserved(0), type(1=icon), count.
header = struct.pack("<HHH", 0, 1, len(entries))
offset = 6 + 16 * len(entries)
directory = b""
for s, data in entries:
    # 256 is encoded as 0 in the width/height bytes.
    w = s if s < 256 else 0
    h = s if s < 256 else 0
    directory += struct.pack("<BBBBHHII", w, h, 0, 0, 1, 32, len(data), offset)
    offset += len(data)

with open(out, "wb") as f:
    f.write(header + directory + b"".join(d for _, d in entries))
PY

echo "Generated $(find "$OUT_LOGOS" -name '*.png' | wc -l | tr -d ' ') brand-logo PNG(s) and $OUT_ICO"
