#!/usr/bin/env bash
# Enforce the branch rules for a pull request (docs/CONTRIBUTING.md, Branch Flow).
#
# Usage: HEAD_REF=<source branch> BASE_REF=<target branch> scripts/check-flow.sh
set -euo pipefail

head=${HEAD_REF:?set HEAD_REF to the pull request source branch}
base=${BASE_REF:?set BASE_REF to the pull request target branch}

types='feature|feat|fix|refactor|perf|docs|design|test|build|ci|chore'
slug='[a-z0-9][a-z0-9._-]*'
topic="^(${types})/${slug}$"

fail() {
  echo "error: $1" >&2
  echo "       $head -> $base" >&2
  exit 1
}

[[ $head =~ $topic ]] ||
  fail "branches must be named <type>/<kebab-name> (type: ${types})"
echo "ok: $head -> $base"
