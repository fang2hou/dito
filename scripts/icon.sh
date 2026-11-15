#!/usr/bin/env bash
# App icon tooling around assets/icon/Dito.icon (an Icon Composer document; see docs/IMPLEMENTATION.md,
# Engineering Traps).
#   scripts/icon.sh compile <resources-dir>   build the Liquid Glass icon, with a flat ICNS fallback
#   scripts/icon.sh preview [out-dir]         ictool: render every appearance to PNG (default /tmp/dito-icon-preview)
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -z ${DEVELOPER_DIR:-} ]]; then
  DEVELOPER_DIR="$(xcode-select -p 2>/dev/null || true)"
  full_xcode=/Applications/Xcode.app/Contents/Developer
  if [[ ! -x "$DEVELOPER_DIR/usr/bin/actool" && -x "$full_xcode/usr/bin/actool" ]]; then
    DEVELOPER_DIR="$full_xcode"
  fi
fi
export DEVELOPER_DIR
icon=assets/icon/Dito.icon
actool="$DEVELOPER_DIR/usr/bin/actool"
ictool="$(dirname "$DEVELOPER_DIR")/Applications/Icon Composer.app/Contents/Executables/ictool"

compile_glass() {
  xcrun actool "$icon" --compile "$1" --app-icon Dito \
    --development-region en --target-device mac --platform macosx --minimum-deployment-target 26.0 \
    --output-partial-info-plist "$2"
}

compile_flat() {
  local work="$1" artwork=assets/icon/AppIcon-1024.png size
  mkdir -p "$work/Dito.iconset"
  for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$artwork" --out "$work/Dito.iconset/icon_${size}x${size}.png" >/dev/null || return
    sips -z "$((size * 2))" "$((size * 2))" "$artwork" \
      --out "$work/Dito.iconset/icon_${size}x${size}@2x.png" >/dev/null || return
  done
  iconutil -c icns -o "$work/Dito.icns" "$work/Dito.iconset"
}

compile_icon() (
  out="$1"
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  mkdir -p "$work/glass" "$out"
  if [[ -x "$actool" ]] && compile_glass "$work/glass" "$work/icon.plist" >"$work/actool.log" 2>&1 &&
    [[ -s "$work/glass/Assets.car" ]]; then
    if [[ ! -s "$work/glass/Dito.icns" ]]; then
      compile_flat "$work"
      cp "$work/Dito.icns" "$work/glass/Dito.icns"
    fi
    cp "$work/glass/Assets.car" "$work/glass/Dito.icns" "$out/"
  else
    echo "Using the flat app icon (the layered icon requires a compatible Xcode)." >&2
    if [[ -s "$work/actool.log" ]]; then
      cat "$work/actool.log" >&2
    fi
    compile_flat "$work"
    cp "$work/Dito.icns" "$out/"
    # a stale Assets.car would bring the layered icon back
    rm -f "$out/Assets.car"
  fi
)

case "${1:-}" in
  compile)
    compile_icon "${2:?usage: icon.sh compile <resources-dir>}"
    ;;
  compile-cached)
    # Same output as compile, reusing target/icon-cache unless assets/icon/Dito.icon, the flat
    # artwork or the selected actool changed (actool takes ~5s; dev-runner.sh calls this on every cargo run).
    out="${2:?usage: icon.sh compile-cached <resources-dir>}"
    cache=target/icon-cache
    source_sum="$(find "$icon" assets/icon/AppIcon-1024.png -type f -exec shasum {} + | sort | shasum | awk '{print $1}')"
    sum="$source_sum-$(/usr/bin/stat -f '%m%z' "$actool" 2>/dev/null || echo no-actool)"
    if [[ "$(cat "$cache/.stamp" 2>/dev/null)" != "$sum" || ! -e "$cache/Dito.icns" ]]; then
      rm -rf "$cache"
      compile_icon "$cache"
      echo "$sum" > "$cache/.stamp"
    fi
    mkdir -p "$out"
    if [[ -e "$cache/Assets.car" ]]; then
      cp "$cache/Assets.car" "$cache/Dito.icns" "$out/"
    else
      # a stale Assets.car from an earlier layered compile would bring the layered icon back
      rm -f "$out/Assets.car"
      cp "$cache/Dito.icns" "$out/"
    fi
    ;;
  preview)
    out="${2:-/tmp/dito-icon-preview}"
    mkdir -p "$out"
    for rendition in Default Dark ClearLight ClearDark TintedLight TintedDark; do
      tint=()
      [[ $rendition == Tinted* ]] && tint=(--tint-color 0.1 --tint-strength 0.6)
      "$ictool" "$icon" --export-image --output-file "$out/$rendition.png" --platform macOS --rendition "$rendition" \
        --width 1024 --height 1024 --scale 1 --design-generation 27 ${tint[@]+"${tint[@]}"} >/dev/null
    done
    echo "$out"
    ;;
  *)
    echo "usage: scripts/icon.sh compile <resources-dir> | preview [out-dir]" >&2
    exit 2
    ;;
esac
