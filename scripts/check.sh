#!/usr/bin/env bash
# Local, noninteractive check entry for Dito.
#
# Usage: ./scripts/check.sh {fast|full}   (always runs against the checkout that holds this script)
#
#   fast  cargo fmt --check, then one cargo test run over the crates that need no models or system
#         services (lib tests, the mock-server integration tests, app unit tests, i18n coverage and
#         the headless demo_run smoke test), then the Bedrock test on a single thread. Nothing here
#         shows a window, takes focus or touches the menu bar.
#   full  fast, plus compile checks (--all-targets) of the app with no features, with
#         mock+screenshots, and with the default features (pulls dito-runtimes: needs cmake and
#         the Xcode Metal toolchain).
#
# Not run here, see docs/IMPLEMENTATION.md (Testing): dito-platform --test keychain and --test loopback (ignored;
# Keychain, Accessibility), dito-runtimes tests and examples (real models), screenshot rendering, clippy
# (mise run lint:rust) and release builds.
#
# Notes:
#   - Steps run sequentially and all run even after a failure; the exit code is 1 if any failed.
#   - Set CARGO_TARGET_DIR to share artifacts across worktrees.
#   - The Apple runner override keeps cargo from launching test binaries through the signing path.
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
  local label=$1
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
if [[ -n ${CARGO_TARGET_DIR:-} ]]; then
  echo "CARGO_TARGET_DIR: $CARGO_TARGET_DIR"
fi

# One invocation: cargo builds dito-core once per feature set, so splitting this up multiplies compile time.
test_args=(
  --locked
  -p dito-core -p dito-audio -p dito-platform -p dito-local -p dito-testkit -p dito-app
  --no-default-features --features dito-app/mock
)

step "fmt (cargo fmt --all -- --check)" cargo fmt --all -- --check
step "tests" cargo test "${test_args[@]}" --lib --bin dito \
  --test engines --test downloads --test providers --test configured_asr --test i18n_coverage --test demo_run
# bedrock.rs sets AWS_* environment variables from every test, which is only sound on one thread.
step "bedrock tests (single thread)" cargo test "${test_args[@]}" --test bedrock -- --test-threads=1

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
