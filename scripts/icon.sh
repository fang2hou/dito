#!/usr/bin/env bash
# App icon tooling around assets/icon/Dito.icon (an Icon Composer document; see docs/engineering/IMPLEMENTATION.md, Engineering Traps).
#   scripts/icon.sh compile <resources-dir>   actool: Dito.icon -> Assets.car (Liquid Glass) + Dito.icns (fallback)
#   scripts/icon.sh preview [out-dir]         ictool: render every appearance to PNG (default /tmp/dito-icon-preview)
set -euo pipefail
cd "$(dirname "$0")/.."
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
icon=assets/icon/Dito.icon
ictool="$(dirname "$DEVELOPER_DIR")/Applications/Icon Composer.app/Contents/Executables/ictool"

case "${1:-}" in
  compile)
    out="${2:?usage: icon.sh compile <resources-dir>}"
    mkdir -p "$out"
    xcrun actool "$icon" --compile "$out" --app-icon Dito \
      --development-region en --target-device mac --platform macosx --minimum-deployment-target 26.0 \
      --output-partial-info-plist "$(mktemp -d)/icon.plist" >/dev/null
    ;;
  compile-cached)
    # Same output as compile, reusing target/icon-cache unless assets/icon/Dito.icon changed
    # (actool takes ~5s; dev-runner.sh calls this on every cargo run).
    out="${2:?usage: icon.sh compile-cached <resources-dir>}"
    cache=target/icon-cache
    sum="$(find "$icon" -type f -exec shasum {} + | sort | shasum | awk '{print $1}')"
    if [[ "$(cat "$cache/.stamp" 2>/dev/null)" != "$sum" || ! -f "$cache/Assets.car" || ! -f "$cache/Dito.icns" ]]; then
      rm -rf "$cache"
      mkdir -p "$cache"
      xcrun actool "$icon" --compile "$cache" --app-icon Dito \
        --development-region en --target-device mac --platform macosx --minimum-deployment-target 26.0 \
        --output-partial-info-plist "$(mktemp -d)/icon.plist" >/dev/null
      echo "$sum" > "$cache/.stamp"
    fi
    mkdir -p "$out"
    cp "$cache/Assets.car" "$cache/Dito.icns" "$out/"
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
