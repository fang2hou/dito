#!/usr/bin/env bash
# Print the workspace version from Cargo.toml, the one place a release sets it.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"$/\1/p' Cargo.toml)
if [[ -z $version ]]; then
  echo "error: no workspace version in Cargo.toml" >&2
  exit 1
fi
printf '%s\n' "$version"
