#!/usr/bin/env bash
# Build the release bundle and pack it into target/dist/Dito-v<version>-macos-<arch>.dmg
# for distribution outside the App Store. The app is ad-hoc signed by default: recipients
# clear Gatekeeper once via System Settings > Privacy & Security > Open Anyway. Set
# CODESIGN_IDENTITY to a Developer ID application certificate for a prompt-free install
# (notarization still has to happen separately before the certificate helps anyone).
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(scripts/workspace-version.sh)
if ! command -v figcraft-cli >/dev/null; then
  echo "error: figcraft-cli is required to export the DMG background" >&2
  exit 1
fi

# Distribution defaults to ad-hoc: the local dito-dev identity is trusted only on this
# machine, so it would not help recipients and just stamps a misleading signer.
identity="${CODESIGN_IDENTITY:--}"
CODESIGN_IDENTITY="$identity" scripts/bundle.sh

dmg="target/dist/Dito-v$version-macos-$(uname -m).dmg"
mkdir -p target/dist
work=$(mktemp -d "$PWD/target/dist/.dmg.XXXXXXXX")
mount="$work/volume"
mounted=false
cleanup() {
  if "$mounted" && ! diskutil eject "$mount" >/dev/null; then
    echo "error: eject $mount before removing $work" >&2
    return
  fi
  rm -rf "$work"
}
trap cleanup EXIT

staging="$work/staging"
mkdir -p "$staging/.background"
ditto target/bundle/Dito.app "$staging/Dito.app"
ln -s /Applications "$staging/Applications"

# Finder supplies the real icons, so hide their mockups in the export copy.
cat > "$work/export.jsonl" <<'JSON'
{"command":"select.byName","params":{"query":"folder-"}}
{"command":"object.toggleVisible","params":{"visible":false}}
{"command":"select.byName","params":{"query":"app-icon"}}
{"command":"object.toggleVisible","params":{"visible":false}}
JSON
figcraft-cli run "$work/export.jsonl" --in design/dmg-background.figcraft --out "$work/background.figcraft" >/dev/null
for scale in 1 2; do
  dpi=$((72 * scale))
  figcraft-cli export "$work/background.figcraft" --node '00.01 DMG Background' \
    --scale "$scale" --out "$work/background@${scale}x.png" >/dev/null
  sips -s format tiff -s dpiWidth "$dpi" -s dpiHeight "$dpi" \
    "$work/background@${scale}x.png" --out "$work/background@${scale}x.tiff" >/dev/null
done
tiffutil -cathidpicheck "$work/background@1x.tiff" "$work/background@2x.tiff" \
  -out "$staging/.background/background.tiff" >/dev/null

# diskutil prints a progress bar to stderr; keep it quiet unless the command fails.
create_image() {
  local err
  if ! err=$(diskutil image create from "$@" 2>&1 >/dev/null); then
    echo "error: diskutil image create failed: $err" >&2
    return 1
  fi
}

writable="$work/Dito.asif"
create_image --format ASIF --volumeName Dito "$staging" "$writable"
diskutil image attach --nobrowse --mountPoint "$mount" "$writable" >/dev/null
mounted=true
# Creating an image from a folder filters out .DS_Store.
cp design/dmg-layout.dsstore "$mount/.DS_Store"
diskutil eject "$mount" >/dev/null
mounted=false

output="$work/${dmg##*/}"
create_image --format UDZO "$writable" "$output"
codesign --sign "$identity" "$output"
hdiutil verify "$output" >/dev/null
mv -f "$output" "$dmg"
echo "$dmg"
