#!/usr/bin/env bash
# Build target/bundle/Dito.app (release build, Liquid Glass icon, Info.plist).
# Ad-hoc signed unless CODESIGN_IDENTITY is set.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -z ${DEVELOPER_DIR:-} ]]; then
  selected_xcode="$(xcode-select -p 2>/dev/null || true)"
  if [[ -x "$selected_xcode/usr/bin/xcodebuild" ]]; then
    DEVELOPER_DIR="$selected_xcode"
  else
    DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
  fi
fi
export DEVELOPER_DIR

cargo build --release -p dito-app
app=target/bundle/Dito.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/dito "$app/Contents/MacOS/dito"

scripts/icon.sh compile "$app/Contents/Resources"

# Localize the system permission prompt (it reads InfoPlist.strings, not our UI code).
for lproj in en zh; do
  text=$([ "$lproj" = en ] \
    && echo 'NSMicrophoneUsageDescription = "Dito uses the microphone for voice input.";' \
    || echo 'NSMicrophoneUsageDescription = "Dito 使用麦克风进行语音输入。";')
  mkdir -p "$app/Contents/Resources/$lproj.lproj"
  printf '%s\n' "$text" > "$app/Contents/Resources/$lproj.lproj/InfoPlist.strings"
done

cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Dito</string>
  <key>CFBundleDisplayName</key><string>Dito</string>
  <key>CFBundleIdentifier</key><string>com.fang2hou.dito</string>
  <key>CFBundleExecutable</key><string>dito</string>
  <key>CFBundleIconFile</key><string>Dito</string>
  <key>CFBundleIconName</key><string>Dito</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>音声入力のためにマイクを使用します。</string>
</dict>
</plist>
PLIST

# without Assets.car, CFBundleIconName would name a missing icon
if [[ ! -s "$app/Contents/Resources/Assets.car" ]]; then
  /usr/libexec/PlistBuddy -c "Delete :CFBundleIconName" "$app/Contents/Info.plist"
fi

# Stable local signing identity keeps TCC permissions (mic / accessibility / input
# monitoring) across rebuilds; set up once via scripts/certs.sh. CODESIGN_IDENTITY
# (e.g. a real Apple Developer ID) still takes precedence.
if [ -z "${CODESIGN_IDENTITY:-}" ] && security find-identity -v -p codesigning | grep -q '"dito-dev"'; then
  CODESIGN_IDENTITY=dito-dev
fi
codesign --force --deep --sign "${CODESIGN_IDENTITY:--}" "$app"
echo "$app"
