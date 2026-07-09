#!/usr/bin/env bash
# Downloads the native libvosk library into ./lib and a small English Vosk
# model into ./models. Both are required to build and run with voice input.
set -euo pipefail

cd "$(dirname "$0")/.."

VOSK_VERSION="0.3.42"
MODEL="vosk-model-small-en-us-0.15"

case "$(uname -s)-$(uname -m)" in
  Darwin-*) LIB_PKG="vosk-osx-${VOSK_VERSION}" ;;
  Linux-x86_64) LIB_PKG="vosk-linux-x86_64-${VOSK_VERSION}" ;;
  Linux-aarch64) LIB_PKG="vosk-linux-aarch64-${VOSK_VERSION}" ;;
  *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

mkdir -p lib models
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if ! ls lib/libvosk.* >/dev/null 2>&1; then
  echo "Downloading ${LIB_PKG}..."
  curl -sSLf -o "$tmp/libvosk.zip" \
    "https://github.com/alphacep/vosk-api/releases/download/v${VOSK_VERSION}/${LIB_PKG}.zip"
  unzip -oq "$tmp/libvosk.zip" -d "$tmp"
  cp "$tmp/${LIB_PKG}"/libvosk.* lib/
  if [ "$(uname -s)" = "Darwin" ]; then
    # The shipped dylib's install name is a bare "libvosk.dylib"; rewrite it
    # to @rpath so the binary finds it via the rpath set in build.rs.
    install_name_tool -id @rpath/libvosk.dylib lib/libvosk.dylib
    codesign -f -s - lib/libvosk.dylib
  fi
else
  echo "libvosk already present in lib/"
fi

if [ ! -d "models/${MODEL}" ]; then
  echo "Downloading ${MODEL}..."
  curl -sSLf -o "$tmp/model.zip" "https://alphacephei.com/vosk/models/${MODEL}.zip"
  unzip -oq "$tmp/model.zip" -d models
else
  echo "model already present in models/"
fi

echo "Done."
