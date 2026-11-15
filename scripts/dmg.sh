#!/usr/bin/env bash
# Build the release bundle and pack it into target/dist/Dito-v<version>-macos-<arch>.dmg
# for distribution outside the App Store. The app is ad-hoc signed by default: recipients
# clear Gatekeeper once via System Settings > Privacy & Security > Open Anyway. Set
# CODESIGN_IDENTITY to a Developer ID application certificate for a prompt-free install
# (notarization still has to happen separately before the certificate helps anyone).
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"$/\1/p' Cargo.toml)
if [[ -z $version ]]; then
  echo "error: no workspace version in Cargo.toml" >&2
  exit 1
fi

# Distribution defaults to ad-hoc: the local dito-dev identity is trusted only on this
# machine, so it would not help recipients and just stamps a misleading signer.
identity="${CODESIGN_IDENTITY:--}"
CODESIGN_IDENTITY="$identity" scripts/bundle.sh

staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
ditto target/bundle/Dito.app "$staging/Dito.app"
ln -s /Applications "$staging/Applications"

dmg="target/dist/Dito-v$version-macos-$(uname -m).dmg"
mkdir -p target/dist
rm -f "$dmg"
# diskutil prints a progress bar to stderr; keep it quiet unless the command fails.
if ! err=$(diskutil image create from --format UDZO --volumeName Dito "$staging" "$dmg" 2>&1 >/dev/null); then
  echo "error: diskutil image create failed: $err" >&2
  exit 1
fi
codesign --sign "$identity" "$dmg"
hdiutil verify "$dmg" >/dev/null
echo "$dmg"
