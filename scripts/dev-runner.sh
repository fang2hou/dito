#!/usr/bin/env bash
# Cargo runner (wired in .cargo/config.toml): re-sign the freshly built binary with
# the local dev identity before executing it, so TCC grants survive rebuilds.
# Falls back to running unsigned when the identity is not set up (scripts/certs.sh).
#
# The main app runs through a minimal .app wrapper launched via LaunchServices (`open`):
#   - a bare executable has no Info.plist, and macOS then denies the microphone silently
#     (no usage description, no prompt);
#   - TCC prompts are only attributed to LaunchServices-launched apps; a shell-exec'd
#     binary is attributed to its parent terminal instead, which suppresses the prompt
#     (and "open System Settings" alike).
# The wrapper is signed with the dev bundle id (com.fang2hou.dito-dev), so its TCC grants are
# its own and never mix with the installed app's. stdout/stderr are captured to a file via
# `open --stdout` and tailed back, so `--log-sink` output still reaches the caller.
set -euo pipefail

bin="$1"; shift

identity="${DITO_CODESIGN_IDENTITY:-dito-dev}"
# Debug builds have their own id (dito_core::identity), so permissions, config and keys never mix with the
# installed app.
bundle_id=com.fang2hou.dito-dev
have_identity() { security find-identity -v -p codesigning | grep -q "\"$identity\""; }

if [[ "$(basename "$bin")" == dito ]]; then
  # A wrapper per run, named by this runner's pid: a second instance must not rewrite the bundle a running one
  # executes. TCC keys its grants on the bundle id and the signature, not the path, so they still hold.
  bundles="$(dirname "$bin")/../bundle"
  for stale in "$bundles"/dev-*; do
    [[ -d "$stale" ]] || continue
    kill -0 "${stale##*/dev-}" 2>/dev/null || rm -rf "$stale"
  done
  app="$bundles/dev-$$/DitoDev.app"
  mkdir -p "$app/Contents/MacOS"
  cp "$bin" "$app/Contents/MacOS/dito"
  # Liquid Glass icon, same pipeline as bundle.sh (cached; see docs/IMPLEMENTATION.md, Engineering Traps).
  # With CFBundleIconName present the system draws the icon from Assets.car in every
  # appearance, and system.rs::set_app_icon keeps out of the way.
  "$(dirname "$0")/icon.sh" compile-cached "$app/Contents/Resources"
  # Localize the system permission prompt (it reads InfoPlist.strings, not our UI code).
  for lproj in en zh; do
    text=$([ "$lproj" = en ] \
      && printf '%s\n' 'NSMicrophoneUsageDescription = "Dito uses the microphone for voice input.";' \
      'NSAudioCaptureUsageDescription = "Dito records system audio to transcribe meetings.";' \
      || printf '%s\n' 'NSMicrophoneUsageDescription = "Dito 使用麦克风进行语音输入。";' \
      'NSAudioCaptureUsageDescription = "Dito 录制系统音频以转写会议。";')
    mkdir -p "$app/Contents/Resources/$lproj.lproj"
    printf '%s\n' "$text" > "$app/Contents/Resources/$lproj.lproj/InfoPlist.strings"
  done
  cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Dito Dev</string>
  <key>CFBundleDisplayName</key><string>Dito Dev</string>
  <key>CFBundleIdentifier</key><string>com.fang2hou.dito-dev</string>
  <key>CFBundleExecutable</key><string>dito</string>
  <key>CFBundleIconFile</key><string>Dito</string>
  <key>CFBundleIconName</key><string>Dito</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>音声入力のためにマイクを使用します。</string>
  <key>NSAudioCaptureUsageDescription</key><string>会議の書き起こしのためにシステム音声を録音します。</string>
</dict>
</plist>
PLIST
  version=$("$(dirname "$0")/workspace-version.sh")
  plutil -replace CFBundleVersion -string "$version" "$app/Contents/Info.plist"
  plutil -replace CFBundleShortVersionString -string "$version" "$app/Contents/Info.plist"
  # without Assets.car, CFBundleIconName would name a missing icon
  if [[ ! -s "$app/Contents/Resources/Assets.car" ]]; then
    /usr/libexec/PlistBuddy -c "Delete :CFBundleIconName" "$app/Contents/Info.plist"
  fi
  if have_identity; then
    codesign --force --identifier "$bundle_id" --sign "$identity" "$app"
  else
    echo "dev-signing: identity \"$identity\" not found; run scripts/certs.sh to stop TCC permission resets" >&2
  fi

  log="$(mktemp -t dito-dev)"
  tail -F "$log" 2>/dev/null &
  tail_pid=$!
  # Stops the instance this run started and no other: besides `tail`, it is the one process that holds this run's
  # log open (as its stdout). Matching by name would also stop the instances of other runs and other worktrees.
  cleanup() {
    for pid in $(lsof -t "$log" 2>/dev/null); do
      [[ "$pid" == "$tail_pid" ]] || kill "$pid" 2>/dev/null || true
    done
    kill "$tail_pid" 2>/dev/null || true
    rm -rf "$(dirname "$app")" "$log"
  }
  trap cleanup TERM INT EXIT
  # LaunchServices starts the app with a clean environment, so the log filter is handed over explicitly.
  env_args=()
  if [[ -n "${RUST_LOG:-}" ]]; then env_args=(--env "RUST_LOG=$RUST_LOG"); fi
  # The wrapper has no film files of its own: the app plays them from the checkout.
  env_args+=(--env "DITO_INTRO_DIR=$(cd "$(dirname "$0")/.." && pwd)/assets/intro")
  if [[ -n "${DITO_INTRO:-}" ]]; then env_args+=(--env "DITO_INTRO=$DITO_INTRO"); fi
  open -W -n "$app" --stdout "$log" ${env_args[@]+"${env_args[@]}"} --args "$@"
  rc=$?
  cleanup
  trap - EXIT
  exit "$rc"
fi

if have_identity; then
  # Stable identifier + stable cert = stable designated requirement = stable TCC entry.
  codesign --force --identifier "$bundle_id" --sign "$identity" "$bin"
else
  echo "dev-signing: identity \"$identity\" not found; run scripts/certs.sh to stop TCC permission resets" >&2
fi

exec "$bin" "$@"
