#!/usr/bin/env bash
# Build the release bundle and install it to ~/Applications/Dito.app.
#
# Signing with the stable local identity keeps the Microphone and Accessibility grants across
# reinstalls, so this script creates the identity when it is missing (scripts/certs.sh, which
# shows one macOS authorization prompt) and refuses to install an ad-hoc signed build.
set -euo pipefail
cd "$(dirname "$0")/.."

identity="${DITO_CODESIGN_IDENTITY:-dito-dev}"
dest="$HOME/Applications/Dito.app"
language="${DITO_INSTALL_LANGUAGE:-en}"

scripts/certs.sh
if ! security find-identity -v -p codesigning | grep -q "\"$identity\""; then
  echo "error: signing identity \"$identity\" is still unavailable; approve the macOS prompt or see scripts/certs.sh" >&2
  exit 1
fi

CODESIGN_IDENTITY="$identity" scripts/bundle.sh
scripts/quit.sh

mkdir -p "$(dirname "$dest")"
if [[ -e $dest ]]; then
  version=$(defaults read "$dest/Contents/Info" CFBundleShortVersionString 2>/dev/null || true)
  case "$language" in
    ja) printf '%s（%s）を置き換えています。\n' "$dest" "${version:-バージョン不明}" ;;
    zh) printf '正在替换 %s（%s）。\n' "$dest" "${version:-版本未知}" ;;
    ko) printf '기존 앱 교체 중: %s (%s)\n' "$dest" "${version:-버전 알 수 없음}" ;;
    *) printf 'replacing %s (%s)\n' "$dest" "${version:-unknown version}" ;;
  esac
  rm -rf "$dest"
fi
ditto target/bundle/Dito.app "$dest"
codesign --verify --deep --strict "$dest"
open "$dest"
case "$language" in
  ja) printf '%s にインストールしました。署名 ID は「%s」です。\n' "$dest" "$identity" ;;
  zh) printf '已安装到 %s，签名身份为「%s」。\n' "$dest" "$identity" ;;
  ko) printf '%s에 설치했습니다. 코드 서명 ID는 "%s"입니다.\n' "$dest" "$identity" ;;
  *) printf 'installed %s, signed by "%s"\n' "$dest" "$identity" ;;
esac
