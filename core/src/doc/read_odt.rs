//! ODT (OpenDocument Text) reader. Reads content.xml.

use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::read_docx::zip_entry;
use super::{Block, Builder, Doc, Inline};

fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == key).and_then(|a| a.unescape_value().ok()).map(|v| v.into_owned())
}

#[derive(Clone, Copy, Default)]
struct TextStyle {
    bold: bool,
    italic: bool,
    code: bool,
}

/// Read the bold / italic / mono settings of the automatic styles (and styles.xml).
fn read_styles(xml: &str, map: &mut HashMap<String, TextStyle>) {
    let mut r = Reader::from_str(xml);
    let mut cur: Option<String> = None;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"style:style" => cur = attr(&e, b"style:name"),
            Ok(Event::Empty(e) | Event::Start(e)) if e.name().as_ref() == b"style:text-properties" => {
                if let Some(name) = &cur {
                    let st = map.entry(name.clone()).or_default();
                    if attr(&e, b"fo:font-weight").is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600)) {
                        st.bold = true;
                    }
                    if attr(&e, b"fo:font-style").is_some_and(|s| s == "italic" || s == "oblique") {
                        st.italic = true;
                    }
                    let font = attr(&e, b"style:font-name").unwrap_or_default().to_ascii_lowercase();
                    if font.contains("mono") || font.contains("courier") {
                        st.code = true;
                    }
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"style:style" => cur = None,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
}

enum Ctx {
    Para(String),
    Heading(u8),
}

fn base_style(name: &str, parents: &HashMap<String, String>) -> String {
    // Automatic styles (P1, P2, ...) point to a named parent style.
    parents.get(name).cloned().unwrap_or_else(|| name.to_owned()).to_ascii_lowercase()
}

fn read_parents(xml: &str, map: &mut HashMap<String, String>) {
    let mut r = Reader::from_str(xml);
    loop {
        match r.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) if e.name().as_ref() == b"style:style" => {
                if let (Some(n), Some(p)) = (attr(&e, b"style:name"), attr(&e, b"style:parent-style-name")) {
                    map.insert(n, p);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
}

pub fn read(path: &str) -> Result<Doc, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| "this is not an ODT file".to_string())?;
    let content = zip_entry(&mut zip, "content.xml").ok_or("content.xml is missing")?;
    let mut styles = HashMap::new();
    if let Some(s) = zip_entry(&mut zip, "styles.xml") {
        read_styles(&s, &mut styles);
    }
    read_styles(&content, &mut styles);
    let mut parents = HashMap::new();
    read_parents(&content, &mut parents);

    let mut b = Builder::default();
    let mut r = Reader::from_str(&content);
    let mut style_stack: Vec<TextStyle> = vec![TextStyle::default()];
    let mut link: Option<String> = None;
    let mut ctx: Vec<Ctx> = Vec::new();
    let mut list_depth = 0usize;
    let mut in_body = false;
    let mut table_depth = 0usize;
    let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
    let mut header = false;

    loop {
        let ev = r.read_event().map_err(|e| format!("bad content.xml: {e}"))?;
        match ev {
            Event::Start(e) => {
                let name = e.name();
                match name.as_ref() {
                    b"office:text" => in_body = true,
                    b"text:p" | b"text:h" if in_body => {
                        let style_name = attr(&e, b"text:style-name").unwrap_or_default();
                        let st = styles.get(&style_name).copied().unwrap_or_default();
                        style_stack.push(st);
                        if name.as_ref() == b"text:h" {
                            let l = attr(&e, b"text:outline-level").and_then(|v| v.parse().ok()).unwrap_or(1);
                            ctx.push(Ctx::Heading(l));
                        } else {
                            ctx.push(Ctx::Para(base_style(&style_name, &parents)));
                        }
                    }
                    b"text:span" => {
                        let base = *style_stack.last().unwrap();
                        let st = attr(&e, b"text:style-name").and_then(|n| styles.get(&n).copied()).unwrap_or_default();
                        style_stack.push(TextStyle { bold: base.bold || st.bold, italic: base.italic || st.italic, code: base.code || st.code });
                    }
                    b"text:a" => link = attr(&e, b"xlink:href"),
                    b"text:list" => list_depth += 1,
                    b"table:table" => {
                        table_depth += 1;
                        if table_depth == 1 {
                            b.end_para();
                            rows.clear();
                            header = false;
                        }
                    }
                    b"table:table-header-rows" => header = true,
                    b"table:table-row" if table_depth == 1 => rows.push(Vec::new()),
                    // Skip notes, so their text does not mix into the paragraph.
                    b"text:note" | b"office:annotation" => {
                        let end = e.to_end().into_owned();
                        let _ = r.read_to_end(end.name());
                    }
                    _ => {}
                }
            }
            Event::Empty(e) => match e.name().as_ref() {
                b"text:s" => {
                    let n = attr(&e, b"text:c").and_then(|v| v.parse().ok()).unwrap_or(1usize);
                    b.text(Inline::plain(" ".repeat(n.min(100))));
                }
                b"text:tab" => b.text(Inline::plain("\t")),
                b"text:line-break" => b.text(Inline::plain("\n")),
                _ => {}
            },
            Event::Text(t) if !ctx.is_empty() => {
                let st = *style_stack.last().unwrap();
                let text = t.unescape().map(|c| c.into_owned()).unwrap_or_default();
                b.text(Inline { text, bold: st.bold, italic: st.italic, code: st.code, link: link.clone() });
            }
            Event::End(e) => match e.name().as_ref() {
                b"text:p" | b"text:h" if !ctx.is_empty() => {
                    style_stack.pop();
                    let c = ctx.pop();
                    if table_depth > 0 {
                        b.text(Inline::plain(" "));
                    } else if list_depth > 0 {
                        b.end_list_item(false, list_depth - 1);
                    } else {
                        match c {
                            Some(Ctx::Heading(l)) => b.end_heading(l),
                            Some(Ctx::Para(st)) if st.contains("preformatted") || st.contains("source") => {
                                let text = super::plain_text(&b.take_inlines());
                                match b.doc.blocks.last_mut() {
                                    Some(Block::Code(c)) => {
                                        c.push('\n');
                                        c.push_str(&text);
                                    }
                                    _ => b.push(Block::Code(text)),
                                }
                            }
                            Some(Ctx::Para(st)) if st.contains("quot") => {
                                let v = b.take_inlines();
                                if !v.is_empty() {
                                    match b.doc.blocks.last_mut() {
                                        Some(Block::Quote(q)) => q.push(Block::Para(v)),
                                        _ => b.push(Block::Quote(vec![Block::Para(v)])),
                                    }
                                }
                            }
                            _ => b.end_para(),
                        }
                    }
                }
                b"text:span" => {
                    if style_stack.len() > 1 {
                        style_stack.pop();
                    }
                }
                b"text:a" => link = None,
                b"text:list" => list_depth = list_depth.saturating_sub(1),
                b"table:table-cell" if table_depth == 1 => {
                    let cell = b.take_inlines();
                    if let Some(row) = rows.last_mut() {
                        row.push(cell);
                    }
                }
                b"table:table" => {
                    table_depth = table_depth.saturating_sub(1);
                    if table_depth == 0 && !rows.is_empty() {
                        b.push(Block::Table { header, rows: std::mem::take(&mut rows) });
                    }
                }
                b"office:text" => in_body = false,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(b.finish())
}
