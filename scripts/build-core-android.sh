#!/usr/bin/env bash
# Build the Rust core for Android and put the .so files in the app.
#
# Usage: scripts/build-core-android.sh <ffmpeg-root> [abi ...]
#   ffmpeg-root has one folder per ABI, made by build-ffmpeg-android.sh.
# Needs: cargo-ndk, ANDROID_NDK_HOME, the Rust Android targets.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FFROOT="$(realpath "${1:?ffmpeg-root}")"; shift
ABIS=("${@:-arm64-v8a}")
JNI="$ROOT/android/app/src/main/jniLibs"

cd "$ROOT/core"
for abi in "${ABIS[@]}"; do
  echo "== Rust core for $abi"
  FFMPEG_DIR="$FFROOT/$abi" PKG_CONFIG_ALLOW_CROSS=1 \
    cargo ndk -t "$abi" -P 24 -o "$JNI" build --release --lib --features static-ffmpeg
done
ls -la "$JNI"/*
