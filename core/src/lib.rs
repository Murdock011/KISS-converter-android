//! Conversion engine for the Android converter app.
//!
//! Kotlin calls the functions in this file through UniFFI.
//! All work happens on the device. No network is used.

mod formats;
mod image_conv;
mod media;
mod pdf;

use std::path::Path;
use std::sync::Once;

pub use formats::FileKind;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ConvertError {
    #[error("Unsupported conversion: {msg}")]
    Unsupported { msg: String },
    #[error("Cannot read the input file: {msg}")]
    Input { msg: String },
    #[error("Conversion failed: {msg}")]
    Failed { msg: String },
    #[error("Conversion was cancelled")]
    Cancelled,
}

impl From<std::io::Error> for ConvertError {
    fn from(e: std::io::Error) -> Self {
        ConvertError::Failed { msg: e.to_string() }
    }
}

impl From<image::ImageError> for ConvertError {
    fn from(e: image::ImageError) -> Self {
        ConvertError::Failed { msg: e.to_string() }
    }
}

impl From<ffmpeg_next::Error> for ConvertError {
    fn from(e: ffmpeg_next::Error) -> Self {
        ConvertError::Failed { msg: e.to_string() }
    }
}

/// Settings for one conversion.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ConvertOptions {
    /// 1 to 100. Used by lossy formats (JPG, WebP video, MP3, AAC, ...).
    pub quality: u8,
    /// Largest width or height in pixels. 0 keeps the original size.
    pub max_dimension: u32,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self { quality: 85, max_dimension: 0 }
    }
}

/// Kotlin gets progress through this interface.
/// Return `false` from `on_progress` to cancel the conversion.
#[uniffi::export(callback_interface)]
pub trait ProgressListener: Send + Sync {
    fn on_progress(&self, fraction: f32) -> bool;
}

pub(crate) struct Progress<'a>(pub Option<&'a dyn ProgressListener>);

impl Progress<'_> {
    pub fn report(&self, fraction: f32) -> Result<(), ConvertError> {
        match self.0 {
            Some(l) if !l.on_progress(fraction.clamp(0.0, 1.0)) => Err(ConvertError::Cancelled),
            _ => Ok(()),
        }
    }
}

static FFMPEG_INIT: Once = Once::new();

pub(crate) fn init_ffmpeg() {
    FFMPEG_INIT.call_once(|| {
        let _ = ffmpeg_next::init();
        ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Error);
    });
}

/// The kind of a file, from its extension (for example "mp3").
#[uniffi::export]
pub fn detect_kind(extension: String) -> FileKind {
    formats::kind_of(&extension)
}

/// The output formats the app can make from an input with this extension.
#[uniffi::export]
pub fn target_formats(input_extension: String) -> Vec<String> {
    formats::targets_for(&input_extension)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// Every input extension the app can read.
#[uniffi::export]
pub fn supported_inputs() -> Vec<String> {
    formats::all_inputs().into_iter().map(str::to_owned).collect()
}

/// Convert one file. The extensions of `input` and `output` select the formats.
#[uniffi::export]
pub fn convert(
    input: String,
    output: String,
    options: ConvertOptions,
    listener: Option<Box<dyn ProgressListener>>,
) -> Result<(), ConvertError> {
    let progress = Progress(listener.as_deref());
    let in_ext = formats::ext_of(&input);
    let out_ext = formats::ext_of(&output);

    if !Path::new(&input).exists() {
        return Err(ConvertError::Input { msg: format!("{input} does not exist") });
    }
    if !formats::targets_for(&in_ext).contains(&out_ext.as_str()) {
        return Err(ConvertError::Unsupported { msg: format!("{in_ext} to {out_ext}") });
    }

    let result = match (formats::kind_of(&in_ext), formats::kind_of(&out_ext)) {
        (FileKind::Image, FileKind::Image) => {
            image_conv::convert(&input, &output, &out_ext, &options, &progress)
        }
        (FileKind::Image, FileKind::Pdf) => {
            pdf::images_to_pdf(&[input.clone()], &output, &options, &progress)
        }
        (FileKind::Audio | FileKind::Video, _) => {
            media::convert(&input, &output, &out_ext, &options, &progress)
        }
        _ => Err(ConvertError::Unsupported { msg: format!("{in_ext} to {out_ext}") }),
    };

    if result.is_err() {
        let _ = std::fs::remove_file(&output);
    } else {
        progress.report(1.0)?;
    }
    result
}

/// Put many images into one PDF, one image on each page.
#[uniffi::export]
pub fn images_to_pdf(
    inputs: Vec<String>,
    output: String,
    options: ConvertOptions,
    listener: Option<Box<dyn ProgressListener>>,
) -> Result<(), ConvertError> {
    let progress = Progress(listener.as_deref());
    let result = pdf::images_to_pdf(&inputs, &output, &options, &progress);
    if result.is_err() {
        let _ = std::fs::remove_file(&output);
    }
    result
}

/// Version text for the "About" screen.
#[uniffi::export]
pub fn core_version() -> String {
    init_ffmpeg();
    let v = unsafe { ffmpeg_next::ffi::avcodec_version() };
    format!(
        "convert_core {} (libavcodec {}.{}.{})",
        env!("CARGO_PKG_VERSION"),
        v >> 16,
        (v >> 8) & 0xff,
        v & 0xff
    )
}
