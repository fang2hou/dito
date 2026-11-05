#!/usr/bin/env bash
# Enforce the Git Flow branch rules for a pull request (docs/CONTRIBUTING.md, Git Flow).
#
# Usage: HEAD_REF=<source branch> BASE_REF=<target branch> scripts/check-flow.sh
set -euo pipefail

head=${HEAD_REF:?set HEAD_REF to the pull request source branch}
base=${BASE_REF:?set BASE_REF to the pull request target branch}

types='feature|feat|fix|refactor|perf|docs|test|build|ci|chore'
slug='[a-z0-9][a-z0-9._-]*'
topic="^(${types})/${slug}$"
stabilization="^(fix|docs|test|build|ci|chore)/${slug}$"
release_line="^(release|hotfix)/${slug}$"

fail() {
  echo "error: $1" >&2
  echo "       $head -> $base" >&2
  exit 1
}

case $base in
  main)
    [[ $head =~ $release_line ]] ||
      fail "only release/* and hotfix/* branches can merge into main"
    ;;
  develop)
    [[ $head == main || $head =~ $release_line || $head =~ $topic ]] ||
      fail "branches into develop must be <type>/<kebab-name> (type: ${types}), release/*, hotfix/*, or main for a back-merge"
    ;;
  release/* | hotfix/*)
    [[ $head =~ $stabilization ]] ||
      fail "a release or hotfix branch only takes stabilization work: fix, docs, test, build, ci or chore branches"
    ;;
  *)
    [[ $head =~ $topic ]] ||
      fail "branches must be named <type>/<kebab-name> (type: ${types})"
    ;;
esac
echo "ok: $head -> $base"
