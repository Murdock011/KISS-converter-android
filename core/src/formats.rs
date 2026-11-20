//! Which formats the app reads, and what it can make from each one.

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FileKind {
    Image,
    Audio,
    Video,
    Pdf,
    Unknown,
}

pub const IMAGE_IN: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tiff", "tif", "ico"];
pub const IMAGE_OUT: &[&str] = &["jpg", "png", "webp", "gif", "bmp", "tiff", "ico", "pdf"];

pub const AUDIO_IN: &[&str] = &["mp3", "m4a", "aac", "ogg", "opus", "flac", "wav", "wma", "amr"];
pub const AUDIO_OUT: &[&str] = &["mp3", "m4a", "ogg", "opus", "flac", "wav"];

pub const VIDEO_IN: &[&str] = &["mp4", "mkv", "webm", "mov", "avi", "3gp", "m4v", "flv", "ts", "wmv"];
pub const VIDEO_OUT: &[&str] = &["mp4", "mkv", "webm", "mov", "avi", "gif"];

pub fn ext_of(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn norm(ext: &str) -> String {
    let e = ext.trim_start_matches('.').to_ascii_lowercase();
    match e.as_str() {
        "jpeg" => "jpg".into(),
        "tif" => "tiff".into(),
        _ => e,
    }
}

pub fn kind_of(ext: &str) -> FileKind {
    let e = norm(ext);
    let e = e.as_str();
    if e == "pdf" {
        FileKind::Pdf
    } else if IMAGE_IN.contains(&e) || IMAGE_OUT.contains(&e) {
        FileKind::Image
    } else if AUDIO_IN.contains(&e) || AUDIO_OUT.contains(&e) {
        FileKind::Audio
    } else if VIDEO_IN.contains(&e) || VIDEO_OUT.contains(&e) {
        FileKind::Video
    } else {
        FileKind::Unknown
    }
}

pub fn targets_for(input_ext: &str) -> Vec<&'static str> {
    let e = norm(input_ext);
    let keep_other = |list: &[&'static str]| -> Vec<&'static str> {
        list.iter().copied().filter(|t| *t != e).collect()
    };
    match kind_of(&e) {
        FileKind::Image => keep_other(IMAGE_OUT),
        FileKind::Audio => keep_other(AUDIO_OUT),
        FileKind::Video => {
            // Video files can become other video files, a GIF, or audio only.
            let mut v = keep_other(VIDEO_OUT);
            v.extend(AUDIO_OUT.iter().copied());
            v
        }
        // PDF to images is done by Android's PdfRenderer in the app.
        FileKind::Pdf | FileKind::Unknown => Vec::new(),
    }
}

pub fn all_inputs() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    v.extend(IMAGE_IN);
    v.extend(AUDIO_IN);
    v.extend(VIDEO_IN);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(kind_of("JPEG"), FileKind::Image);
        assert_eq!(kind_of(".mp3"), FileKind::Audio);
        assert_eq!(kind_of("mkv"), FileKind::Video);
        assert_eq!(kind_of("pdf"), FileKind::Pdf);
        assert_eq!(kind_of("xyz"), FileKind::Unknown);
    }

    #[test]
    fn targets_skip_self() {
        assert!(!targets_for("png").contains(&"png"));
        assert!(targets_for("png").contains(&"pdf"));
        assert!(targets_for("mp4").contains(&"mp3"));
        assert!(targets_for("mp4").contains(&"gif"));
        assert!(!targets_for("mp3").contains(&"mp4"));
    }
}
