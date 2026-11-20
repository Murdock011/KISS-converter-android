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
API=24
for abi in "${ABIS[@]}"; do
  echo "== Rust core for $abi"
  case "$abi" in
    arm64-v8a)   triple=aarch64-linux-android ;;
    armeabi-v7a) triple=armv7a-linux-androideabi ;;
    x86_64)      triple=x86_64-linux-android ;;
    *) echo "unknown abi $abi"; exit 1 ;;
  esac
  # bindgen (in ffmpeg-sys-next) needs a target with the API level,
  # or the NDK headers stop with "Unversioned target triples are not supported".
  rust_triple="${triple/armv7a-linux-androideabi/armv7_linux_androideabi}"
  export "BINDGEN_EXTRA_CLANG_ARGS_${rust_triple//-/_}=--target=$triple$API"
  FFMPEG_DIR="$FFROOT/$abi" PKG_CONFIG_ALLOW_CROSS=1 \
    cargo ndk -t "$abi" -P "$API" -o "$JNI" build --release --lib --features static-ffmpeg
done
ls -la "$JNI"/*
