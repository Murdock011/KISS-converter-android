# Converter for Android

An Android app that converts files on the phone. No upload. No internet.

- **Images:** JPG, PNG, WebP, GIF, BMP, TIFF, ICO. Many images to one PDF.
- **Audio:** MP3, M4A (AAC), OGG, OPUS, FLAC, WAV.
- **Video:** MP4, MKV, WebM, MOV, AVI, GIF. Video to audio only.
- **Documents:** DOCX, ODT, RTF, HTML, Markdown, TXT in. PDF, DOCX, ODT, HTML,
  Markdown, TXT out. Text, headings, bold, italic, lists, links and tables are kept.
  Pictures and page layout in documents are not kept.
- **PDF:** pages to PNG or JPG.

## How it is made

| Part | Tool |
| --- | --- |
| Conversion engine | Rust (`core/`): `image` crate for pictures, FFmpeg through `ffmpeg-next` for audio and video, a small PDF writer |
| Bridge to Kotlin | UniFFI (Kotlin bindings, JNA) |
| Screens | Kotlin, Jetpack Compose, Material Design 3 (Material You colours on Android 12+) |
| Documents | Own readers and writers in Rust (`core/src/doc`): `pulldown-cmark`, `quick-xml`, `zip`, `tl`, `printpdf` |
| PDF to images | Android `PdfRenderer` |

FFmpeg is built for Android with LAME (MP3), Opus and libvpx (VP8/VP9).
MP4, MOV and AVI video use the FFmpeg MPEG-4 encoder, because H.264 encoders
(x264) are GPL. See "Next steps".

## Get the APK

GitHub Actions builds the APK on each push. Open the **Actions** tab, select
the last run of **Android APK**, and download **converter-apk**.
Install `app-release.apk` on the phone (it is signed with a debug key, for testing).

## Build on your computer

Needs: Android SDK and NDK, Rust, `cargo-ndk`, `nasm`, `pkg-config`, FFmpeg dev
headers for your computer (for the bindings step).

```bash
export ANDROID_NDK_HOME=/path/to/ndk
rustup target add aarch64-linux-android
cargo install cargo-ndk

scripts/build-ffmpeg-android.sh arm64-v8a build/ffmpeg/arm64-v8a   # about 15 minutes
scripts/build-core-android.sh build/ffmpeg arm64-v8a
cd android && ./gradlew assembleDebug
```

Test the Rust core on a computer:

```bash
cd core
cargo test
cargo run --bin convert-cli -- input.mp4 output.webm
cargo run --bin convert-cli -- --pdf out.pdf page1.jpg page2.png
```

## Next steps

- H.264 video (OpenH264 or Android MediaCodec).
- HEIC and AVIF images.
- Chinese, Japanese and Korean text in document-to-PDF (needs a CJK font).
- Pictures inside documents; old Office files (DOC); spreadsheets.
- `x86_64` and `armeabi-v7a` builds.
- Run long video conversions in a foreground service.

## Licences

FFmpeg is LGPL 2.1. LAME is LGPL. Opus and libvpx are BSD.
