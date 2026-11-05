#!/usr/bin/env bash
# Make sure the Xcode Metal Toolchain is installed (GPUI shaders need it). Idempotent.
set -euo pipefail

if xcrun metal --version >/dev/null 2>&1; then
  echo "Metal toolchain already installed"
  exit 0
fi
xcodebuild -downloadComponent MetalToolchain
