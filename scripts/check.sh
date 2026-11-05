#!/usr/bin/env bash
# Local, noninteractive check entry for Dito. Mirrors the central acceptance split so a
# green run here means the same commands passed: fmt + lib/unit/mock-server tests (fast),
# plus compile-only checks of the minimal, screenshots+mock, and default app builds (full).
#
# Usage: ./scripts/check.sh {fast|full}   (repository root is resolved from this script's
#        location, so the check always runs against this checkout regardless of the caller's
#        working directory)
#
#   fast  cargo fmt --check; core/audio/platform lib tests; local fake-engine/download
#         tests against the in-process mock server; testkit mock-server tests; app bin
#         unit tests and i18n coverage built with --no-default-features --features mock.
#   full  fast plus compile checks only (--all-targets): app with no features (minimal
#         build), app with screenshots+mock features, and the default app build (pulls
#         dito-runtimes: needs cmake + Xcode Metal toolchain, slow from scratch).
#
# Intentionally NOT run here — see docs/IMPLEMENTATION.md (Testing) for the full map:
#   - dito-app --test demo_run: launches the real app binary (needs a macOS GUI session)
#   - dito-platform --test keychain: real macOS Keychain
#   - dito-platform --test loopback: posts real system key events (F20) into the live event
#     tap; needs Accessibility
#   - dito-runtimes examples and real model downloads / inference
#   - screenshot *execution* (full only compiles the screenshots build)
#   - cargo clippy and release builds: central acceptance / manual per practices.md
#
# Notes:
#   - Steps run strictly sequentially; one cargo process at a time.
#   - Set CARGO_TARGET_DIR to share artifacts across worktrees and with the main checkout.
#   - CARGO_TARGET_*_APPLE_DARWIN_RUNNER=/usr/bin/env keeps cargo from launching test
#     binaries through the signing path (avoids the dito-dev signing prompt during tests).
#     Test binaries here are pure automated tests; no signing files are involved.
set -uo pipefail

usage() {
  echo "usage: $0 {fast|full}" >&2
}

if [[ $# -ne 1 ]]; then
  usage
  exit 2
fi
case $1 in
  fast | full) mode=$1 ;;
  *)
    usage
    echo "unknown mode: $1" >&2
    exit 2
    ;;
esac

root=$(cd "$(dirname "$0")/.." && pwd) || exit 2
cd "$root" || exit 2

export CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER=${CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER:-/usr/bin/env}
export CARGO_TARGET_X86_64_APPLE_DARWIN_RUNNER=${CARGO_TARGET_X86_64_APPLE_DARWIN_RUNNER:-/usr/bin/env}

failures=()
step() {
  label=$1
  shift
  echo
  echo "==> $label"
  if "$@"; then
    echo "==> ok: $label"
  else
    echo "==> FAILED: $label" >&2
    failures+=("$label")
  fi
}

echo "project root: $root"
echo "rustc $(rustc --version 2>&1 | cut -d' ' -f2) | cargo $(cargo --version 2>&1 | cut -d' ' -f2) | mode: $mode"
[[ -n ${CARGO_TARGET_DIR:-} ]] && echo "CARGO_TARGET_DIR: $CARGO_TARGET_DIR"

step "fmt (cargo fmt --all -- --check)" cargo fmt --all -- --check
step "core/audio/platform lib tests" cargo test --locked -p dito-core -p dito-audio -p dito-platform --lib
step "local fake-engine/download tests" cargo test --locked -p dito-local --lib --test engines --test downloads
step "testkit mock-server tests" cargo test --locked -p dito-testkit --lib --test providers --test bedrock -- --test-threads=1
step "app bin unit tests (mock build)" cargo test --locked -p dito-app --no-default-features --features mock --bin dito
step "app i18n coverage (mock build)" cargo test --locked -p dito-app --no-default-features --features mock --test i18n_coverage

if [[ $mode == full ]]; then
  step "app minimal compile check (no features)" cargo check --locked -p dito-app --no-default-features --all-targets
  step "app screenshots+mock compile check" cargo check --locked -p dito-app --no-default-features --features mock,screenshots --all-targets
  step "app default compile check (local runtimes)" cargo check --locked -p dito-app --all-targets
fi

echo
if ((${#failures[@]})); then
  echo "check $mode: ${#failures[@]} step(s) failed:" "${failures[@]}" >&2
  exit 1
fi
echo "check $mode: all steps passed"
