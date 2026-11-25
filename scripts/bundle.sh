#!/usr/bin/env bash
# Build target/bundle/Dito.app and its application resources.
# Ad-hoc signed unless CODESIGN_IDENTITY is set.
set -euo pipefail
cd "$(dirname "$0")/.."
profile="${DITO_BUILD_PROFILE:-release}"
cargo build --locked --profile "$profile" --target-dir target -p dito-app
app=target/bundle/Dito.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$profile/dito" "$app/Contents/MacOS/dito"

scripts/icon.sh compile "$app/Contents/Resources"

# The first-run intro film, one file per language (played from Contents/Resources/intro).
mkdir -p "$app/Contents/Resources/intro"
cp assets/intro/*.mp4 "$app/Contents/Resources/intro/"

# shellcheck source=scripts/permission-strings.sh
source scripts/permission-strings.sh
write_permission_strings "$app/Contents/Resources"

cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>ja</string>
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
  <key>NSAudioCaptureUsageDescription</key><string>会議の書き起こしのためにシステム音声を録音します。</string>
</dict>
</plist>
PLIST

version=$(scripts/workspace-version.sh)
plutil -replace CFBundleVersion -string "$version" "$app/Contents/Info.plist"
plutil -replace CFBundleShortVersionString -string "$version" "$app/Contents/Info.plist"

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
