//! Still image conversion with the `image` crate.

use std::fs::File;
use std::io::BufWriter;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader};

use crate::{ConvertError, ConvertOptions, Progress};

pub fn load(path: &str) -> Result<DynamicImage, ConvertError> {
    ImageReader::open(path)
        .map_err(|e| ConvertError::Input { msg: e.to_string() })?
        .with_guessed_format()
        .map_err(|e| ConvertError::Input { msg: e.to_string() })?
        .decode()
        .map_err(|e| ConvertError::Input { msg: e.to_string() })
}

pub fn fit(img: DynamicImage, max_dimension: u32) -> DynamicImage {
    if max_dimension == 0 || (img.width() <= max_dimension && img.height() <= max_dimension) {
        img
    } else {
        img.resize(max_dimension, max_dimension, FilterType::Lanczos3)
    }
}

pub fn encode_jpeg(img: &DynamicImage, quality: u8, out: impl std::io::Write) -> Result<(), ConvertError> {
    // JPEG has no alpha channel, so put the image on white first.
    let rgb = flatten_on_white(img);
    let mut enc = JpegEncoder::new_with_quality(out, quality.clamp(1, 100));
    enc.encode_image(&rgb)?;
    Ok(())
}

fn flatten_on_white(img: &DynamicImage) -> image::RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
    for (src, dst) in rgba.pixels().zip(rgb.pixels_mut()) {
        let a = src[3] as u32;
        for c in 0..3 {
            dst[c] = ((src[c] as u32 * a + 255 * (255 - a)) / 255) as u8;
        }
    }
    rgb
}

pub fn convert(
    input: &str,
    output: &str,
    out_ext: &str,
    options: &ConvertOptions,
    progress: &Progress,
) -> Result<(), ConvertError> {
    progress.report(0.05)?;
    let img = fit(load(input)?, options.max_dimension);
    progress.report(0.5)?;

    let file = BufWriter::new(File::create(output)?);
    match out_ext {
        "jpg" | "jpeg" => encode_jpeg(&img, options.quality, file)?,
        "ico" => {
            // ICO files hold at most 256 x 256 pixels.
            let small = fit(img, 256);
            let mut f = file;
            small.write_to(&mut f, ImageFormat::Ico)?;
        }
        "bmp" | "gif" => {
            // These encoders work best with plain RGB(A) 8-bit data.
            let mut f = file;
            DynamicImage::ImageRgba8(img.to_rgba8()).write_to(&mut f, format_for(out_ext)?)?;
        }
        _ => {
            let mut f = file;
            let img = match out_ext {
                // The WebP encoder takes only 8-bit RGB or RGBA.
                "webp" if img.color().has_alpha() => DynamicImage::ImageRgba8(img.to_rgba8()),
                "webp" => DynamicImage::ImageRgb8(img.to_rgb8()),
                _ => img,
            };
            img.write_to(&mut f, format_for(out_ext)?)?;
        }
    }
    Ok(())
}

fn format_for(ext: &str) -> Result<ImageFormat, ConvertError> {
    ImageFormat::from_extension(ext).ok_or_else(|| ConvertError::Unsupported { msg: ext.to_owned() })
}
