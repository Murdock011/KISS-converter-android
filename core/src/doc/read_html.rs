use super::xml::decode_entities;
use super::{push_list_item, tidy, Block, Builder, Doc, Inline};

#[derive(Clone, Default)]
struct Style {
    bold: bool,
    italic: bool,
    code: bool,
    pre: bool,
    link: Option<String>,
}

struct Walker<'a> {
    parser: &'a tl::Parser<'a>,
    b: Builder,
    title: Option<String>,
}

fn collapse_ws(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() && c != '\u{a0}' {
            if !space {
                o.push(' ');
            }
            space = true;
        } else {
            o.push(c);
            space = false;
        }
    }
    o
}

impl<'a> Walker<'a> {
    fn children(&mut self, tag: &tl::HTMLTag, style: &Style) {
        for h in tag.children().top().iter() {
            self.node(*h, style);
        }
    }

    fn text_of(&self, tag: &tl::HTMLTag) -> String {
        decode_entities(&tag.inner_text(self.parser))
    }

    fn node(&mut self, h: tl::NodeHandle, style: &Style) {
        let Some(node) = h.get(self.parser) else { return };
        match node {
            tl::Node::Raw(bytes) => {
                let raw = decode_entities(&bytes.as_utf8_str());
                let text = if style.pre { raw } else { collapse_ws(&raw) };
                if !text.is_empty() {
                    self.b.text(Inline { text, bold: style.bold, italic: style.italic, code: style.code, link: style.link.clone() });
                }
            }
            tl::Node::Comment(_) => {}
            tl::Node::Tag(tag) => self.tag(tag, style),
        }
    }

    fn tag(&mut self, tag: &tl::HTMLTag, style: &Style) {
        let name = tag.name().as_utf8_str().to_ascii_lowercase();
        let mut s = style.clone();
        match name.as_str() {
            "script" | "style" | "noscript" | "template" | "svg" => {}
            "head" => {
                // Only the <title> is useful here.
                for h in tag.children().all(self.parser) {
                    if let Some(t) = h.as_tag() {
                        if t.name().as_utf8_str().eq_ignore_ascii_case("title") {
                            let t = collapse_ws(&self.text_of(t)).trim().to_owned();
                            if !t.is_empty() {
                                self.title = Some(t);
                            }
                        }
                    }
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.b.end_para();
                self.children(tag, &s);
                self.b.end_heading(name[1..].parse().unwrap_or(1));
            }
            "p" | "div" | "section" | "article" | "header" | "footer" | "main" | "nav" | "aside" | "figure" | "figcaption" | "dd" | "dt" | "address" => {
                self.b.end_para();
                self.children(tag, &s);
                self.b.end_para();
            }
            "br" => self.b.text(Inline::plain("\n")),
            "hr" => self.b.push(Block::Rule),
            "b" | "strong" => {
                s.bold = true;
                self.children(tag, &s)
            }
            "i" | "em" | "cite" | "var" => {
                s.italic = true;
                self.children(tag, &s)
            }
            "code" | "kbd" | "samp" | "tt" => {
                s.code = true;
                self.children(tag, &s)
            }
            "a" => {
                s.link = tag
                    .attributes()
                    .get("href")
                    .flatten()
                    .map(|v| decode_entities(&v.as_utf8_str()))
                    .filter(|v| !v.starts_with('#') && !v.starts_with("javascript:"));
                self.children(tag, &s)
            }
            "pre" => {
                let text = self.text_of(tag);
                self.b.push(Block::Code(text.trim_matches('\n').to_owned()));
            }
            "blockquote" => {
                self.b.end_para();
                let inner = self.sub_doc(tag, &s);
                self.b.push(Block::Quote(inner));
            }
            "ul" | "ol" => {
                self.b.end_para();
                let ordered = name == "ol";
                let mut items: Vec<Vec<Block>> = Vec::new();
                for h in tag.children().top().iter() {
                    if let Some(li) = h.get(self.parser).and_then(|n| n.as_tag()) {
                        if li.name().as_utf8_str().eq_ignore_ascii_case("li") {
                            items.push(self.sub_doc(li, &s));
                        }
                    }
                }
                if !items.is_empty() {
                    self.b.push(Block::List { ordered, items });
                }
            }
            "li" => {
                // A <li> outside a list.
                let inner = self.sub_doc(tag, &s);
                self.b.end_para();
                for blk in inner {
                    push_list_item(&mut self.b.doc.blocks, false, 0, blk);
                }
            }
            "table" => {
                self.b.end_para();
                let mut rows = Vec::new();
                let mut header = false;
                for h in tag.children().all(self.parser) {
                    let Some(tr) = h.as_tag() else { continue };
                    if !tr.name().as_utf8_str().eq_ignore_ascii_case("tr") {
                        continue;
                    }
                    let mut row = Vec::new();
                    for c in tr.children().top().iter() {
                        let Some(td) = c.get(self.parser).and_then(|n| n.as_tag()) else { continue };
                        let n = td.name().as_utf8_str().to_ascii_lowercase();
                        if n == "td" || n == "th" {
                            if n == "th" && rows.is_empty() {
                                header = true;
                            }
                            let mut w = Walker { parser: self.parser, b: Builder::default(), title: None };
                            w.children(td, &s);
                            let mut cell = tidy(w.b.take_inlines());
                            for i in cell.iter_mut() {
                                i.text = i.text.replace('\n', " ");
                            }
                            row.push(cell);
                        }
                    }
                    if !row.is_empty() {
                        rows.push(row);
                    }
                }
                if !rows.is_empty() {
                    self.b.push(Block::Table { header, rows });
                }
            }
            "img" => {
                if let Some(alt) = tag.attributes().get("alt").flatten() {
                    let alt = decode_entities(&alt.as_utf8_str());
                    if !alt.trim().is_empty() {
                        self.b.text(Inline { text: format!("[{alt}]"), italic: true, ..Default::default() });
                    }
                }
            }
            "title" => {}
            _ => self.children(tag, &s),
        }
    }

    fn sub_doc(&mut self, tag: &tl::HTMLTag, s: &Style) -> Vec<Block> {
        let mut w = Walker { parser: self.parser, b: Builder::default(), title: None };
        w.children(tag, s);
        w.b.finish().blocks
    }
}

pub fn read(html: &str) -> Doc {
    let Ok(dom) = tl::parse(html, tl::ParserOptions::default()) else {
        return super::read_txt::read(html);
    };
    let parser = dom.parser();
    let mut w = Walker { parser, b: Builder::default(), title: None };
    let root = Style::default();
    for h in dom.children() {
        w.node(*h, &root);
    }
    let title = w.title.take();
    let mut doc = w.b.finish();
    doc.title = title;
    doc
}
