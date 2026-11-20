//! DOCX (Office Open XML) reader. Reads word/document.xml and its links and numbering.

use std::collections::HashMap;
use std::io::Read;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::{Block, Builder, Doc, Inline};

pub fn zip_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let mut f = zip.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == key).and_then(|a| a.unescape_value().ok()).map(|v| v.into_owned())
}

/// `<w:b/>` is on; `<w:b w:val="0"/>` or `"false"` is off.
fn on(e: &BytesStart) -> bool {
    !matches!(attr(e, b"w:val").as_deref(), Some("0" | "false" | "none"))
}

/// Relationship id -> link target.
fn read_rels(xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut r = Reader::from_str(xml);
    loop {
        match r.read_event() {
            Ok(Event::Empty(e) | Event::Start(e)) if e.local_name().as_ref() == b"Relationship" => {
                if let (Some(id), Some(t)) = (attr(&e, b"Id"), attr(&e, b"Target")) {
                    map.insert(id, t);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    map
}

/// numId -> is the first level a numbered (not bullet) list?
fn read_numbering(xml: &str) -> HashMap<String, bool> {
    let mut abstract_fmt: HashMap<String, bool> = HashMap::new();
    let mut num_to_abs: HashMap<String, String> = HashMap::new();
    let mut r = Reader::from_str(xml);
    let mut cur_abs: Option<String> = None;
    let mut cur_lvl: Option<String> = None;
    let mut cur_num: Option<String> = None;
    loop {
        match r.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) => match e.name().as_ref() {
                b"w:abstractNum" => cur_abs = attr(&e, b"w:abstractNumId"),
                b"w:lvl" => cur_lvl = attr(&e, b"w:ilvl"),
                b"w:numFmt" if cur_lvl.as_deref() == Some("0") => {
                    if let Some(a) = &cur_abs {
                        let fmt = attr(&e, b"w:val").unwrap_or_default();
                        abstract_fmt.insert(a.clone(), fmt != "bullet" && fmt != "none");
                    }
                }
                b"w:num" => cur_num = attr(&e, b"w:numId"),
                b"w:abstractNumId" => {
                    if let (Some(n), Some(v)) = (&cur_num, attr(&e, b"w:val")) {
                        num_to_abs.insert(n.clone(), v);
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:abstractNum" => cur_abs = None,
                b"w:lvl" => cur_lvl = None,
                b"w:num" => cur_num = None,
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    num_to_abs.into_iter().map(|(n, a)| (n, *abstract_fmt.get(&a).unwrap_or(&false))).collect()
}

/// Style id -> heading level, from style names like "heading 1" and "Title".
fn read_styles(xml: &str) -> HashMap<String, u8> {
    let mut map = HashMap::new();
    let mut r = Reader::from_str(xml);
    let mut cur: Option<String> = None;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"w:style" => cur = attr(&e, b"w:styleId"),
            Ok(Event::Empty(e)) if e.name().as_ref() == b"w:name" => {
                if let (Some(id), Some(name)) = (&cur, attr(&e, b"w:val")) {
                    if let Some(l) = heading_level(&name) {
                        map.insert(id.clone(), l);
                    }
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"w:style" => cur = None,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    map
}

fn heading_level(style: &str) -> Option<u8> {
    let s = style.to_ascii_lowercase().replace(' ', "");
    if s == "title" {
        return Some(1);
    }
    if s == "subtitle" {
        return Some(2);
    }
    s.strip_prefix("heading").and_then(|n| n.parse::<u8>().ok()).map(|n| n.clamp(1, 6))
}

#[derive(Default)]
struct Para {
    style: Option<String>,
    num: Option<(String, usize)>,
}

pub fn read(path: &str) -> Result<Doc, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| "this is not a DOCX file".to_string())?;
    let body = zip_entry(&mut zip, "word/document.xml").ok_or("word/document.xml is missing")?;
    let rels = zip_entry(&mut zip, "word/_rels/document.xml.rels").map(|x| read_rels(&x)).unwrap_or_default();
    let numbering = zip_entry(&mut zip, "word/numbering.xml").map(|x| read_numbering(&x)).unwrap_or_default();
    let styles = zip_entry(&mut zip, "word/styles.xml").map(|x| read_styles(&x)).unwrap_or_default();

    let mut b = Builder::default();
    let mut r = Reader::from_str(&body);
    let mut para = Para::default();
    let (mut bold, mut italic, mut code) = (false, false, false);
    let mut link: Option<String> = None;
    let mut in_text = false;
    let mut in_rpr = false;
    let mut in_ppr = false;
    // Tables: rows of cells; each cell collects plain inlines.
    let mut table_depth = 0usize;
    let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
    let mut header_row = false;

    loop {
        let ev = match r.read_event() {
            Ok(ev) => ev,
            Err(e) => return Err(format!("bad document.xml: {e}")),
        };
        match ev {
            Event::Start(e) => match e.name().as_ref() {
                b"w:p" => para = Para::default(),
                b"w:r" => {
                    bold = false;
                    italic = false;
                    code = false;
                }
                b"w:rPr" => in_rpr = true,
                b"w:pPr" => in_ppr = true,
                b"w:t" => in_text = true,
                b"w:hyperlink" => {
                    link = attr(&e, b"r:id").and_then(|id| rels.get(&id).cloned());
                }
                b"w:tbl" => {
                    table_depth += 1;
                    if table_depth == 1 {
                        b.end_para();
                        rows.clear();
                        header_row = false;
                    }
                }
                b"w:tr" if table_depth == 1 => rows.push(Vec::new()),
                b"w:tc" if table_depth == 1 => {}
                _ => {}
            },
            Event::Empty(e) => match e.name().as_ref() {
                b"w:pStyle" => para.style = attr(&e, b"w:val"),
                b"w:ilvl" => {
                    let lvl = attr(&e, b"w:val").and_then(|v| v.parse().ok()).unwrap_or(0);
                    para.num = Some((para.num.take().map(|n| n.0).unwrap_or_default(), lvl));
                }
                b"w:numId" => {
                    let id = attr(&e, b"w:val").unwrap_or_default();
                    let lvl = para.num.as_ref().map(|n| n.1).unwrap_or(0);
                    // numId 0 means "no list".
                    para.num = if id == "0" { None } else { Some((id, lvl)) };
                }
                b"w:b" if in_rpr => bold = on(&e),
                b"w:i" if in_rpr => italic = on(&e),
                b"w:rFonts" if in_rpr => {
                    let f = attr(&e, b"w:ascii").unwrap_or_default().to_ascii_lowercase();
                    code = f.contains("courier") || f.contains("consolas") || f.contains("mono");
                }
                b"w:tab" if !in_ppr => b.text(Inline::plain("\t")),
                b"w:br" | b"w:cr" => b.text(Inline::plain("\n")),
                b"w:tblHeader" => header_row = true,
                _ => {}
            },
            Event::Text(t) if in_text => {
                let text = t.unescape().map(|c| c.into_owned()).unwrap_or_default();
                b.text(Inline { text, bold, italic, code, link: link.clone() });
            }
            Event::End(e) => match e.name().as_ref() {
                b"w:t" => in_text = false,
                b"w:rPr" => in_rpr = false,
                b"w:pPr" => in_ppr = false,
                b"w:hyperlink" => link = None,
                b"w:p" if table_depth > 0 => {
                    // Paragraphs inside a table cell: join them with a space.
                    b.text(Inline::plain(" "));
                }
                b"w:p" => {
                    let style = para.style.as_deref().unwrap_or("");
                    let level = styles.get(style).copied().or_else(|| heading_level(style));
                    let low = style.to_ascii_lowercase();
                    if low == "code" || low.contains("preformatted") || low == "sourcecode" {
                        // Join code lines into one code block.
                        let text = super::plain_text(&b.take_inlines());
                        match b.doc.blocks.last_mut() {
                            Some(Block::Code(c)) => {
                                c.push('\n');
                                c.push_str(&text);
                            }
                            _ => b.push(Block::Code(text)),
                        }
                    } else if low.contains("quote") {
                        let v = b.take_inlines();
                        if !v.is_empty() {
                            match b.doc.blocks.last_mut() {
                                Some(Block::Quote(q)) => q.push(Block::Para(v)),
                                _ => b.push(Block::Quote(vec![Block::Para(v)])),
                            }
                        }
                    } else if let Some(l) = level {
                        b.end_heading(l);
                    } else if let Some((id, lvl)) = &para.num {
                        b.end_list_item(*numbering.get(id).unwrap_or(&false), *lvl);
                    } else {
                        b.end_para();
                    }
                }
                b"w:tc" if table_depth == 1 => {
                    let cell = b.take_inlines();
                    if let Some(row) = rows.last_mut() {
                        row.push(cell);
                    }
                }
                b"w:tbl" => {
                    table_depth = table_depth.saturating_sub(1);
                    if table_depth == 0 && !rows.is_empty() {
                        b.push(Block::Table { header: header_row, rows: std::mem::take(&mut rows) });
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(b.finish())
}
