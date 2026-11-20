#!/usr/bin/env bash
# Verify a release tag before publishing: it must be v<workspace version> and point at a
# commit that is already on main. Needs full history (fetch-depth: 0).
#
# Usage: TAG=v0.1.0 scripts/check-release-tag.sh
set -euo pipefail
cd "$(dirname "$0")/.."

tag=${TAG:?set TAG to the release tag}
version=$(scripts/workspace-version.sh)

fail() {
  echo "error: $1" >&2
  exit 1
}

[[ $tag == "v$version" ]] || fail "tag $tag does not match workspace version $version (expected v$version)"
git merge-base --is-ancestor "$tag^{commit}" origin/main ||
  fail "$tag is not on main; merge the release branch into main first"
echo "ok: $tag matches workspace version $version and is on main"
