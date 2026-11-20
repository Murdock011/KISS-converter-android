#!/usr/bin/env bash
# Build the Rust core for Android and put the .so files in the app.
#
# Usage: scripts/build-core-android.sh <ffmpeg-root> [abi ...]
#   ffmpeg-root has one folder per ABI, made by build-ffmpeg-android.sh.
# Needs: ANDROID_NDK_HOME and the Rust Android targets (rustup target add ...).
#
# We call cargo with our own NDK settings (not cargo-ndk), because bindgen
# needs the API level in its clang target. NDK 26+ headers stop with
# "Unversioned target triples are not supported" without it, and cargo-ndk
# overwrites the bindgen arguments.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FFROOT="$(realpath "${1:?ffmpeg-root}")"; shift
ABIS=("${@:-arm64-v8a}")
JNI="$ROOT/android/app/src/main/jniLibs"
API=24
NDK="${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME}"
TC="$NDK/toolchains/llvm/prebuilt/linux-x86_64"
SYSROOT="$TC/sysroot"

cd "$ROOT/core"
for abi in "${ABIS[@]}"; do
  case "$abi" in
    arm64-v8a)   rust=aarch64-linux-android;   clang=aarch64-linux-android ;;
    armeabi-v7a) rust=armv7-linux-androideabi; clang=armv7a-linux-androideabi ;;
    x86_64)      rust=x86_64-linux-android;    clang=x86_64-linux-android ;;
    *) echo "unknown abi $abi"; exit 1 ;;
  esac
  echo "== Rust core for $abi ($rust)"
  up="$(echo "$rust" | tr 'a-z-' 'A-Z_')"
  low="${rust//-/_}"
  export "CC_$low=$TC/bin/$clang$API-clang"
  export "CXX_$low=$TC/bin/$clang$API-clang++"
  export "AR_$low=$TC/bin/llvm-ar"
  export "CARGO_TARGET_${up}_LINKER=$TC/bin/$clang$API-clang"
  export "CARGO_TARGET_${up}_AR=$TC/bin/llvm-ar"
  export "BINDGEN_EXTRA_CLANG_ARGS_$low=--sysroot=$SYSROOT --target=$clang$API"
  # ffmpeg-sys-next reads the sysroot from this cargo-ndk variable.
  export CARGO_NDK_SYSROOT_PATH="$SYSROOT"
  FFMPEG_DIR="$FFROOT/$abi" PKG_CONFIG_ALLOW_CROSS=1 \
    cargo build --release --lib --target "$rust" --features static-ffmpeg

  mkdir -p "$JNI/$abi"
  cp "target/$rust/release/libconvert_core.so" "$JNI/$abi/"
  "$TC/bin/llvm-strip" --strip-unneeded "$JNI/$abi/libconvert_core.so"

  # A shared library links even with missing symbols, and then fails to load
  # on the phone. Check that each needed symbol comes from an Android system library.
  so="$JNI/$abi/libconvert_core.so"
  libdir="$SYSROOT/usr/lib/$clang/$API"
  [ -d "$libdir" ] || libdir="$SYSROOT/usr/lib/${clang/armv7a/arm}/$API"
  needed=$("$TC/bin/llvm-readelf" -d "$so" | sed -n 's/.*Shared library: \[\(.*\)\]/\1/p')
  echo "NEEDED: $needed"
  provided=$(mktemp)
  for lib in $needed; do
    if [ -f "$libdir/$lib" ]; then
      "$TC/bin/llvm-nm" -D --defined-only "$libdir/$lib" | awk '{print $NF}' | sed 's/@.*//' >> "$provided"
    else
      echo "ERROR: $lib is not an Android system library"; exit 1
    fi
  done
  sort -u -o "$provided" "$provided"
  missing=$("$TC/bin/llvm-nm" -D --undefined-only "$so" | awk '$1=="U"{print $2}' | sed 's/@.*//' | sort -u | comm -23 - "$provided")
  if [ -n "$missing" ]; then
    echo "ERROR: libconvert_core.so needs symbols that no system library has:"
    echo "$missing" | head -50
    exit 1
  fi
  echo "OK: all symbols of libconvert_core.so are found"
done
ls -la "$JNI"/*
