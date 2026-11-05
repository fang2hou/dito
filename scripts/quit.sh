#!/usr/bin/env bash
# Quit a running Dito (any build with bundle id com.fang2hou.dito) and wait until it is gone.
set -euo pipefail

bundle_id=com.fang2hou.dito

osascript -e "if application id \"$bundle_id\" is running then tell application id \"$bundle_id\" to quit" >/dev/null 2>&1 || true

for _ in $(seq 1 50); do
  pgrep -f "Contents/MacOS/dito" >/dev/null || exit 0
  sleep 0.1
done
echo "error: Dito is still running; quit it manually and retry" >&2
exit 1
