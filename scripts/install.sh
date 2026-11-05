#!/usr/bin/env bash
# Build the release bundle and install it to ~/Applications/Dito.app.
#
# Signing with the stable local identity keeps the Microphone and Accessibility grants across
# reinstalls, so this script creates the identity when it is missing (scripts/certs.sh, which
# shows one macOS authorization prompt) and refuses to install an ad-hoc signed build.
set -euo pipefail
cd "$(dirname "$0")/.."

identity="${DITO_CODESIGN_IDENTITY:-dito-dev}"
dest="$HOME/Applications/Dito.app"

scripts/certs.sh
if ! security find-identity -v -p codesigning | grep -q "\"$identity\""; then
  echo "error: signing identity \"$identity\" is still unavailable; approve the macOS prompt or see scripts/certs.sh" >&2
  exit 1
fi

CODESIGN_IDENTITY="$identity" scripts/bundle.sh
scripts/quit.sh

mkdir -p "$(dirname "$dest")"
if [[ -e $dest ]]; then
  echo "replacing $dest ($(defaults read "$dest/Contents/Info" CFBundleShortVersionString 2>/dev/null || echo unknown version))"
  rm -rf "$dest"
fi
ditto target/bundle/Dito.app "$dest"
codesign --verify --deep --strict "$dest"
echo "installed $dest, signed by \"$identity\""
open "$dest"
