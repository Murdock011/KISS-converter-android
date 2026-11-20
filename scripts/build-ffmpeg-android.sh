#!/usr/bin/env bash
# Build static FFmpeg for Android, with LAME (MP3), Opus and libvpx (VP8/VP9).
#
# Usage: scripts/build-ffmpeg-android.sh <abi> <out-dir>
#   abi:     arm64-v8a | armeabi-v7a | x86_64
#   out-dir: install prefix (gets include/ and lib/)
#
# Needs: ANDROID_NDK_HOME, curl, make, nasm (x86_64 only), pkg-config.
set -euo pipefail

ABI="${1:?abi}"
PREFIX="$(realpath -m "${2:?out-dir}")"
API=24
FFMPEG_VERSION=7.1
LAME_VERSION=3.100
OPUS_VERSION=1.5.2
VPX_VERSION=1.15.0

NDK="${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME}"
TC="$NDK/toolchains/llvm/prebuilt/linux-x86_64"
SYSROOT="$TC/sysroot"
JOBS="$(nproc)"

case "$ABI" in
  arm64-v8a)   TRIPLE=aarch64-linux-android;   CLANG=aarch64-linux-android$API;   FF_ARCH=aarch64; FF_CPU=armv8-a; VPX_TARGET=arm64-android-gcc ;;
  armeabi-v7a) TRIPLE=arm-linux-androideabi;   CLANG=armv7a-linux-androideabi$API; FF_ARCH=arm;   FF_CPU=armv7-a; VPX_TARGET=armv7-android-gcc ;;
  x86_64)      TRIPLE=x86_64-linux-android;    CLANG=x86_64-linux-android$API;    FF_ARCH=x86_64; FF_CPU=x86-64; VPX_TARGET=x86_64-android-gcc ;;
  *) echo "unknown abi $ABI"; exit 1 ;;
esac

export CC="$TC/bin/$CLANG-clang"
export CXX="$TC/bin/$CLANG-clang++"
export AR="$TC/bin/llvm-ar"
export AS="$CC"
export LD="$TC/bin/ld.lld"
export NM="$TC/bin/llvm-nm"
export RANLIB="$TC/bin/llvm-ranlib"
export STRIP="$TC/bin/llvm-strip"
export CFLAGS="-O2 -fPIC"
export CXXFLAGS="-O2 -fPIC"
export LDFLAGS="-Wl,-z,max-page-size=16384"
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig"

WORK="$(realpath -m build/ffmpeg-src/$ABI)"
mkdir -p "$WORK" "$PREFIX"
cd "$WORK"

fetch() { # url dir
  if [ ! -d "$2" ]; then
    curl -fsSL "$1" -o src.tar
    mkdir "$2" && tar -xf src.tar -C "$2" --strip-components=1 && rm src.tar
  fi
}

echo "== LAME $LAME_VERSION"
fetch "https://downloads.sourceforge.net/project/lame/lame/$LAME_VERSION/lame-$LAME_VERSION.tar.gz" lame
( cd lame
  # Old config.sub does not know Android triples.
  curl -fsSL "https://git.savannah.gnu.org/cgit/config.git/plain/config.sub" -o config.sub || true
  curl -fsSL "https://git.savannah.gnu.org/cgit/config.git/plain/config.guess" -o config.guess || true
  ./configure --host="$TRIPLE" --prefix="$PREFIX" --enable-static --disable-shared \
    --disable-frontend --disable-decoder --with-pic >/dev/null
  make -j"$JOBS" >/dev/null && make install >/dev/null )

echo "== Opus $OPUS_VERSION"
fetch "https://downloads.xiph.org/releases/opus/opus-$OPUS_VERSION.tar.gz" opus
( cd opus
  ./configure --host="$TRIPLE" --prefix="$PREFIX" --enable-static --disable-shared \
    --disable-doc --disable-extra-programs --with-pic >/dev/null
  make -j"$JOBS" >/dev/null && make install >/dev/null )

echo "== libvpx $VPX_VERSION"
fetch "https://github.com/webmproject/libvpx/archive/refs/tags/v$VPX_VERSION.tar.gz" libvpx
( cd libvpx
  VPX_EXTRA=()
  VPX_AS="$CC"
  if [ "$ABI" = x86_64 ]; then VPX_EXTRA+=(--disable-avx512); VPX_AS=nasm; fi
  LD="$CC" AS="$VPX_AS" ./configure --target="$VPX_TARGET" --prefix="$PREFIX" --enable-static --disable-shared \
    --enable-pic --disable-examples --disable-tools --disable-docs --disable-unit-tests \
    --disable-vp8-decoder --disable-vp9-decoder --enable-vp8 --enable-vp9 \
    --enable-realtime-only --disable-webm-io --disable-libyuv "${VPX_EXTRA[@]}" >/dev/null
  make -j"$JOBS" >/dev/null && make install >/dev/null
  # Bionic has pthread built in; there is no libpthread to link.
  sed -i 's/-lpthread//g' "$PREFIX/lib/pkgconfig/vpx.pc" )

echo "== FFmpeg $FFMPEG_VERSION"
fetch "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" ffmpeg
( cd ffmpeg
  FF_EXTRA=()
  [ "$ABI" = x86_64 ] && FF_EXTRA+=(--disable-asm)   # avoid text relocations
  [ "$ABI" = armeabi-v7a ] && FF_EXTRA+=(--enable-neon)
  ./configure \
    --prefix="$PREFIX" \
    --target-os=android --arch="$FF_ARCH" --cpu="$FF_CPU" --enable-cross-compile \
    --cc="$CC" --cxx="$CXX" --ar="$AR" --nm="$NM" --ranlib="$RANLIB" --strip="$STRIP" \
    --sysroot="$SYSROOT" --pkg-config=pkg-config --pkg-config-flags=--static \
    --extra-cflags="-I$PREFIX/include -fPIC" --extra-ldflags="-L$PREFIX/lib $LDFLAGS" \
    --enable-pic --enable-static --disable-shared \
    --disable-programs --disable-doc --disable-avdevice --disable-postproc \
    --disable-network --disable-debug --disable-vulkan --disable-mediacodec --disable-jni \
    --enable-libmp3lame --enable-libopus --enable-libvpx \
    "${FF_EXTRA[@]}" >/dev/null
  make -j"$JOBS" >/dev/null && make install >/dev/null )

echo "Done. FFmpeg for $ABI is in $PREFIX"
ls "$PREFIX/lib"
