//! Document conversion.
//!
//! Every reader makes a simple document model (`Doc`), and every writer
//! makes a file from it. So each new format needs only one reader or one writer.
//!
//! The model keeps: headings, paragraphs, bold, italic, code, links, lists,
//! quotes, code blocks, simple tables and rules. It drops pictures and layout.

mod read_docx;
mod read_html;
mod read_md;
mod read_odt;
mod read_rtf;
mod read_txt;
mod write_docx;
mod write_html;
mod write_md;
mod write_odt;
mod write_pdf;
mod write_txt;
mod xml;

use crate::{ConvertError, Progress};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inline {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
}

impl Inline {
    pub fn plain(text: impl Into<String>) -> Self {
        Inline { text: text.into(), ..Default::default() }
    }
    fn same_style(&self, o: &Inline) -> bool {
        self.bold == o.bold && self.italic == o.italic && self.code == o.code && self.link == o.link
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(u8, Vec<Inline>),
    Para(Vec<Inline>),
    List { ordered: bool, items: Vec<Vec<Block>> },
    Quote(Vec<Block>),
    Code(String),
    /// Rows of cells. The first row is the header row when `header` is true.
    Table { header: bool, rows: Vec<Vec<Vec<Inline>>> },
    Rule,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Doc {
    pub title: Option<String>,
    pub blocks: Vec<Block>,
}

/// Join runs with the same style, and drop empty runs.
pub fn tidy(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(inlines.len());
    for i in inlines {
        if i.text.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.same_style(&i) => last.text.push_str(&i.text),
            _ => out.push(i),
        }
    }
    out
}

pub fn plain_text(inlines: &[Inline]) -> String {
    inlines.iter().map(|i| i.text.as_str()).collect()
}

fn is_blank(inlines: &[Inline]) -> bool {
    inlines.iter().all(|i| i.text.trim().is_empty())
}

/// Builds a `Doc` from a stream of "start paragraph / add text / end" calls.
/// The DOCX, ODT, RTF and HTML readers use it.
#[derive(Default)]
pub struct Builder {
    pub doc: Doc,
    inlines: Vec<Inline>,
}

impl Builder {
    pub fn text(&mut self, i: Inline) {
        self.inlines.push(i);
    }
    pub fn take_inlines(&mut self) -> Vec<Inline> {
        let v = std::mem::take(&mut self.inlines);
        let mut v = tidy(v);
        trim_edges(&mut v);
        v
    }
    pub fn end_para(&mut self) {
        let v = self.take_inlines();
        if !is_blank(&v) {
            self.doc.blocks.push(Block::Para(v));
        }
    }
    pub fn end_heading(&mut self, level: u8) {
        let mut v = self.take_inlines();
        // Headings are bold anyway; a bold mark adds nothing.
        v.iter_mut().for_each(|i| {
            i.bold = false;
            i.italic = false;
        });
        let v = tidy(v);
        if !is_blank(&v) {
            self.doc.blocks.push(Block::Heading(level.clamp(1, 6), v));
        }
    }
    /// Add a list item. Next to an open list of the same kind, it joins that list.
    pub fn end_list_item(&mut self, ordered: bool, level: usize) {
        let v = self.take_inlines();
        if is_blank(&v) {
            return;
        }
        push_list_item(&mut self.doc.blocks, ordered, level, Block::Para(v));
    }
    pub fn push(&mut self, b: Block) {
        self.end_para();
        self.doc.blocks.push(b);
    }
    pub fn finish(mut self) -> Doc {
        self.end_para();
        self.doc
    }
}

/// Put an item into the (maybe nested) list at the end of `blocks`.
pub fn push_list_item(blocks: &mut Vec<Block>, ordered: bool, level: usize, item: Block) {
    if level > 0 {
        if let Some(Block::List { items, .. }) = blocks.last_mut() {
            if let Some(last) = items.last_mut() {
                return push_list_item(last, ordered, level - 1, item);
            }
        }
    }
    if let Some(Block::List { ordered: o, items }) = blocks.last_mut() {
        if *o == ordered {
            items.push(vec![item]);
            return;
        }
    }
    blocks.push(Block::List { ordered, items: vec![vec![item]] });
}

fn trim_edges(v: &mut Vec<Inline>) {
    if let Some(first) = v.first_mut() {
        first.text = first.text.trim_start().to_owned();
    }
    if let Some(last) = v.last_mut() {
        last.text = last.text.trim_end().to_owned();
    }
    v.retain(|i| !i.text.is_empty());
}

pub fn read(path: &str, ext: &str) -> Result<Doc, ConvertError> {
    let bad = |e: String| ConvertError::Input { msg: e };
    match ext {
        "txt" => Ok(read_txt::read(&read_text(path)?)),
        "md" | "markdown" => Ok(read_md::read(&read_text(path)?)),
        "html" | "htm" => Ok(read_html::read(&read_text(path)?)),
        "rtf" => Ok(read_rtf::read(&std::fs::read(path)?)),
        "docx" => read_docx::read(path).map_err(bad),
        "odt" => read_odt::read(path).map_err(bad),
        _ => Err(ConvertError::Unsupported { msg: format!("cannot read .{ext}") }),
    }
}

/// Read a text file. Not UTF-8? Then read it as Latin-1 / Windows-1252.
fn read_text(path: &str) -> Result<String, ConvertError> {
    let bytes = std::fs::read(path)?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Ok(match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    })
}

pub fn convert(input: &str, output: &str, in_ext: &str, out_ext: &str, progress: &Progress) -> Result<(), ConvertError> {
    progress.report(0.05)?;
    let mut doc = read(input, in_ext)?;
    if doc.title.is_none() {
        doc.title = doc.blocks.iter().find_map(|b| match b {
            Block::Heading(_, i) => Some(plain_text(i)),
            _ => None,
        });
    }
    progress.report(0.5)?;
    let fail = |e: String| ConvertError::Failed { msg: e };
    match out_ext {
        "txt" => std::fs::write(output, write_txt::write(&doc))?,
        "md" => std::fs::write(output, write_md::write(&doc))?,
        "html" => std::fs::write(output, write_html::write(&doc))?,
        "docx" => write_docx::write(&doc, output).map_err(fail)?,
        "odt" => write_odt::write(&doc, output).map_err(fail)?,
        "pdf" => write_pdf::write(&doc, output).map_err(fail)?,
        _ => return Err(ConvertError::Unsupported { msg: format!("cannot write .{out_ext}") }),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
