#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

system_language=$(defaults read -g AppleLanguages 2>/dev/null |
  awk '{gsub(/[()",]/, " "); if (NF) {print $1; exit}}' || true)
system_language="${system_language:-${LC_ALL:-${LC_MESSAGES:-${LANG:-en}}}}"
language="${system_language%%[-_.@]*}"

message() {
  case "$language" in
    zh) printf '%s\n' "$2" ;;
    ja) printf '%s\n' "$3" ;;
    ko) printf '%s\n' "$4" ;;
    *) printf '%s\n' "$1" ;;
  esac
}

if [[ -t 0 ]]; then
  case "$language" in
    zh) default_choice=3 ;;
    ja) default_choice=2 ;;
    ko) default_choice=4 ;;
    *) default_choice=1 ;;
  esac
  message "Choose the installer language." "请选择安装语言。" \
    "インストーラーの言語を選んでください。" "설치 언어를 선택하세요."
  printf '%s\n' '1) English' '2) 日本語' '3) 简体中文' '4) 한국어'
  while true; do
    read -r -p "$(message "Language" "语言" "言語" "언어") [$default_choice]: " choice
    case "${choice:-$default_choice}" in
      1) language=en; break ;;
      2) language=ja; break ;;
      3) language=zh; break ;;
      4) language=ko; break ;;
      *) message "Enter 1, 2, 3, or 4." "请输入 1、2、3 或 4。" \
        "1、2、3、4 のいずれかを入力してください。" "1, 2, 3, 4 중 하나를 입력하세요." >&2 ;;
    esac
  done
fi

if [[ $(uname -s) != Darwin ]]; then
  message "Dito can only be installed on macOS." \
    "Dito 只能安装在 macOS 上。" \
    "Dito のインストールには macOS が必要です。" \
    "Dito를 설치하려면 macOS가 필요합니다." >&2
  exit 1
fi

if ! command -v "${CMAKE:-cmake}" >/dev/null; then
  if ! command -v brew >/dev/null; then
    message "CMake is missing. Install CMake or make Homebrew available, then rerun this script." \
      "未找到 CMake 或可用的 Homebrew。请安装 CMake，或配置好 Homebrew 后重新运行此脚本。" \
      "CMake が見つかりません。CMake をインストールするか、Homebrew を使えるようにしてから再実行してください。" \
      "CMake가 없습니다. CMake를 설치하거나 Homebrew를 사용할 수 있게 설정한 뒤 다시 실행하세요." >&2
    exit 1
  fi
  if ! cmake_prefix=$(brew --prefix --installed cmake 2>/dev/null) || [[ ! -x "$cmake_prefix/bin/cmake" ]]; then
    message "Installing CMake with Homebrew…" "正在通过 Homebrew 安装 CMake…" \
      "Homebrew で CMake をインストールしています…" "Homebrew로 CMake를 설치하는 중입니다…"
    brew install cmake
    cmake_prefix=$(brew --prefix cmake)
  fi
  export PATH="$cmake_prefix/bin:$PATH"
  export CMAKE="$cmake_prefix/bin/cmake"
  if [[ ! -x "$CMAKE" ]]; then
    message "CMake is still unavailable after Homebrew installation." \
      "Homebrew 安装结束后仍找不到 CMake。" \
      "Homebrew でのインストール後も CMake が見つかりません。" \
      "Homebrew 설치가 끝났지만 CMake를 찾을 수 없습니다." >&2
    exit 1
  fi
fi

rust_minimum=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^rust-version = "\([0-9.]*\)"$/\1/p' Cargo.toml)
: "${rust_minimum:?missing workspace rust-version in Cargo.toml}"

version_at_least_minimum() {
  awk -v actual="$1" -v minimum="$rust_minimum" 'BEGIN {
    split(actual, a, ".")
    split(minimum, m, ".")
    for (i = 1; i <= 3; i++) {
      if (a[i] + 0 != m[i] + 0) exit !(a[i] + 0 > m[i] + 0)
    }
  }'
}

has_compatible_rust() {
  local rustc_version cargo_version
  rustc_version=$(RUSTUP_AUTO_INSTALL=0 "$@" rustc --version 2>/dev/null) &&
    [[ $rustc_version =~ ^rustc[[:space:]]([0-9]+\.[0-9]+\.[0-9]+) ]] &&
    version_at_least_minimum "${BASH_REMATCH[1]}" &&
    cargo_version=$(RUSTUP_AUTO_INSTALL=0 "$@" cargo --version 2>/dev/null) &&
    [[ $cargo_version =~ ^cargo[[:space:]]([0-9]+\.[0-9]+\.[0-9]+) ]] &&
    version_at_least_minimum "${BASH_REMATCH[1]}"
}

find_compatible_rustup_toolchain() {
  local toolchain
  command -v rustup >/dev/null || return 1
  while read -r toolchain _; do
    if has_compatible_rust rustup run "$toolchain"; then
      printf '%s\n' "$toolchain"
      return 0
    fi
  done < <(rustup toolchain list 2>/dev/null)
  return 1
}

install_command=(bash scripts/install.sh)
if ! has_compatible_rust; then
  export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
  if ! has_compatible_rust; then
    if ! toolchain=$(find_compatible_rustup_toolchain); then
      message "Dito requires Rust and Cargo $rust_minimum or later." \
        "编译 Dito 需要 Rust 和 Cargo $rust_minimum 或更新版本。" \
        "Dito のビルドには Rust と Cargo $rust_minimum 以降が必要です。" \
        "Dito를 빌드하려면 Rust와 Cargo $rust_minimum 이상이 필요합니다." >&2
      if [[ ! -t 0 ]]; then
        message "Run this script in Terminal to install Rust interactively." \
          "请在终端重新运行此脚本，以确认是否安装 Rust。" \
          "Rust をインストールする場合は、ターミナルでこのスクリプトを再実行してください。" \
          "Rust를 설치하려면 터미널에서 이 스크립트를 다시 실행하세요." >&2
        exit 1
      fi
      read -r -p "$(message "Install the latest stable Rust with the official rustup installer? [y/N]" \
        "要用官方 rustup 安装最新稳定版 Rust 吗？输入 y 或「是」确认，按回车跳过。" \
        "公式の rustup で Rust の最新安定版をインストールしますか？ y または「はい」で続行、Return キーでスキップします。" \
        "공식 rustup으로 Rust 최신 안정 버전을 설치할까요? y 또는 ‘예’를 입력하면 설치하고, Return 키만 누르면 건너뜁니다.") " answer
      case "$answer" in
        [Yy]|[Yy][Ee][Ss]|是|はい|예|네) ;;
        *)
          message "Rust installation skipped." "已跳过 Rust 安装。" \
            "Rust のインストールをスキップしました。" "Rust 설치를 건너뛰었습니다."
          exit 1
          ;;
      esac
      if command -v rustup >/dev/null; then
        rustup toolchain install stable --profile minimal
      else
        curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs |
          sh -s -- -y --profile minimal --default-toolchain stable --no-modify-path
      fi
      if ! has_compatible_rust rustup run stable; then
        message "The installed Rust toolchain does not meet the minimum version." \
          "安装后的 Rust 工具链仍未达到最低版本要求。" \
          "インストールした Rust ツールチェーンは必要なバージョンを満たしていません。" \
          "설치한 Rust 도구 모음이 최소 버전 요구 사항을 충족하지 않습니다." >&2
        exit 1
      fi
      toolchain=stable
    fi
    install_command=(rustup run "$toolchain" bash scripts/install.sh)
  fi
fi

has_macos_sdk() {
  xcode-select -p >/dev/null 2>&1 &&
    xcrun --sdk macosx --find clang >/dev/null 2>&1 &&
    [[ -d "$(xcrun --sdk macosx --show-sdk-path 2>/dev/null)" ]]
}

if ! has_macos_sdk; then
  message "Dito needs Apple's Command Line Tools to build." \
    "编译 Dito 需要先安装 Apple Command Line Tools。" \
    "Dito のビルドには、Apple のコマンドラインツールが必要です。" \
    "Dito를 빌드하려면 Apple Command Line Tools가 필요합니다." >&2
  if [[ -t 0 ]] && xcode-select --install; then
    read -r -p "$(message "Finish the Apple installer, then press Return to continue." \
      "请在系统窗口完成安装，然后按回车继续。" \
      "システムのインストール画面で操作を完了してから、Return キーを押してください。" \
      "시스템 설치 창에서 설치를 마친 뒤 Return 키를 누르세요.") " _
  fi
  if ! has_macos_sdk; then
    message "Run xcode-select --install and rerun this script after installation." \
      "请运行 xcode-select --install。安装完成后，重新运行此脚本。" \
      "xcode-select --install を実行し、インストールが終わってからこのスクリプトを再実行してください。" \
      "xcode-select --install을 실행하고, 설치가 끝나면 이 스크립트를 다시 실행하세요." >&2
    exit 1
  fi
fi

export DITO_BUILD_PROFILE=local
message "Building and installing Dito…" "正在编译并安装 Dito…" \
  "Dito をビルドしてインストールしています…" "Dito를 빌드하고 설치하는 중입니다…"
DITO_INSTALL_LANGUAGE="$language" "${install_command[@]}" || {
  status=$?
  message "Dito installation failed. See the output above." \
    "Dito 安装失败，请查看上方的错误详情。" \
    "Dito をインストールできませんでした。上のエラーを確認してください。" \
    "Dito를 설치하지 못했습니다. 위의 오류를 확인하세요." >&2
  exit "$status"
}
