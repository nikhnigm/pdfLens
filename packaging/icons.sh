#!/usr/bin/env bash
# Regenerate the app icons from assets/app-icon/pdfcraft.svg and the Windows PDF file icon
# from assets/app-icon/pdfcraft-document.svg. Both are master vectors.
#
# Needs: resvg (brew install resvg / cargo install resvg) and python3 (stdlib only, for the .ico).
# On macOS, iconutil also writes the .icns. The outputs are committed, so building and packaging
# never need these tools. After running it, update the sha256 values in ATTRIBUTION.toml, then
# `cargo xtask assets --write`.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
SVG="$DIR/pdfcraft.svg"
ID="ai.storyteller.pdfcraft"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v resvg >/dev/null || { echo "error: resvg not found (brew install resvg)" >&2; exit 1; }

# The master is a full-bleed 512 tile (rx=112): right for Windows and Linux. macOS icons follow
# Apple's grid instead: an 824 px body centred on a transparent 1024 canvas.
MAC="$TMP/macos.svg"
{
  echo '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 1024 1024">'
  # resvg on Windows needs a native path for the href (cygpath turns /d/... into D:/...).
  echo '<image x="100" y="100" width="824" height="824" xlink:href="'"$(cygpath -m "$SVG" 2>/dev/null || echo "$SVG")"'"/>'
  echo '</svg>'
} >"$MAC"

render() {
  local src="$1" size="$2" out="$3"
  resvg -w "$size" -h "$size" "$src" "$out" </dev/null
}

# 16 and 24 px app icons come from hand-fitted masters (pdfcraft-16.svg, pdfcraft-24.svg) that sit
# on the pixel grid; every other size is rendered from the main master.
app_src() {
  case "$1" in
    16|24) echo "$DIR/pdfcraft-$1.svg" ;;
    *) echo "$SVG" ;;
  esac
}

# 1024 px PNG on Apple's grid (also the runtime Dock icon on macOS, see apps/pdfcraft/src/main.rs).
render "$MAC" 1024 "$DIR/pdfcraft-1024.png"

# Linux hicolor theme (full bleed; hicolor/256x256 is also the runtime icon on Windows and Linux).
for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$DIR/hicolor/${s}x${s}/apps"
  render "$(app_src "$s")" "$s" "$DIR/hicolor/${s}x${s}/apps/$ID.png"
done
mkdir -p "$DIR/hicolor/scalable/apps"
cp "$SVG" "$DIR/hicolor/scalable/apps/$ID.svg"

# Windows .ico: PNG-compressed entries, 16-256 px. The document icon has its own resource
# in pdfcraft.exe; the app icon remains the first/default icon.
for name in pdfcraft pdfcraft-document; do
  ICO_PNGS=()
  for s in 16 20 24 32 40 48 64 128 256; do
    src="$DIR/$name.svg"
    [ "$name" = pdfcraft ] && src="$(app_src "$s")"
    render "$src" "$s" "$TMP/$name-$s.png"
    ICO_PNGS+=("$TMP/$name-$s.png")
  done
  python3 - "$DIR/$name.ico" "${ICO_PNGS[@]}" <<'PY'
import struct, sys
out, pngs = sys.argv[1], sys.argv[2:]
blobs = [open(p, "rb").read() for p in pngs]
head = struct.pack("<HHH", 0, 1, len(blobs))
entries, data, offset = b"", b"", 6 + 16 * len(blobs)
for b in blobs:
    w, h = struct.unpack(">II", b[16:24])  # IHDR
    entries += struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, 32, len(b), offset)
    data += b
    offset += len(b)
open(out, "wb").write(head + entries + data)
PY
done

# macOS .icns.
if command -v iconutil >/dev/null; then
  SET="$TMP/pdfcraft.iconset"
  mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    render "$MAC" "$s" "$SET/icon_${s}x${s}.png"
    render "$MAC" $((s * 2)) "$SET/icon_${s}x${s}@2x.png"
  done
  iconutil -c icns -o "$DIR/pdfcraft.icns" "$SET"
elif python3 -c "import PIL" 2>/dev/null; then
  # Elsewhere, Pillow writes the .icns from the 1024 px Apple-grid PNG.
  python3 -c 'import sys; from PIL import Image; Image.open(sys.argv[1]).save(sys.argv[2])'     "$DIR/pdfcraft-1024.png" "$DIR/pdfcraft.icns"
else
  echo "warning: neither iconutil (macOS) nor Pillow found; pdfcraft.icns not regenerated" >&2
fi
echo "icons written to $DIR"
