use super::{Block, Doc, Inline};

/// Paragraphs are split by empty lines. Single line breaks are kept.
pub fn read(text: &str) -> Doc {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let blocks = text
        .split("\n\n")
        .map(|p| p.trim_matches('\n'))
        .filter(|p| !p.trim().is_empty())
        .map(|p| Block::Para(vec![Inline::plain(p)]))
        .collect();
    Doc { title: None, blocks }
}
