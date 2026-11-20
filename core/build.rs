//! 1. Pass FFmpeg version flags (DEP_FFMPEG_*) on as `feature = "ffmpeg_x_y"` cfgs,
//!    the same way `ffmpeg-next` does.
//! 2. For Android, link the extra codec libraries that the static FFmpeg needs.
use std::env;

fn main() {
    for (name, value) in env::vars() {
        if let Some(flag) = name.strip_prefix("DEP_FFMPEG_") {
            let flag = flag.to_lowercase();
            if value == "true" {
                println!(r#"cargo:rustc-cfg=feature="{flag}""#);
            }
            println!(r#"cargo:rustc-check-cfg=cfg(feature, values("{flag}"))"#);
        }
    }

    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        if let Ok(dir) = env::var("FFMPEG_DIR") {
            // Link args go at the end of the linker command, after libav*.a,
            // so the static linker can resolve the codec symbols.
            for lib in ["mp3lame", "opus", "vpx"] {
                let path = format!("{dir}/lib/lib{lib}.a");
                if std::path::Path::new(&path).exists() {
                    println!("cargo:rustc-link-arg={path}");
                }
            }
            println!("cargo:rustc-link-arg=-lz");
            println!("cargo:rustc-link-arg=-lm");
            println!("cargo:rustc-link-arg=-llog");
            println!("cargo:rustc-link-arg=-landroid");
            // Make the .so work with 16 KB memory pages (Android 15+).
            println!("cargo:rustc-link-arg=-Wl,-z,max-page-size=16384");
        }
    }
}
