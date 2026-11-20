//! PDF writer for documents. It does its own line breaking and page breaking.
//!
//! It uses a font from the device (Roboto / Noto / DejaVu), so that most
//! languages show correctly. If no font is found, it falls back to Helvetica,
//! which only shows Latin characters.

use std::fs::File;
use std::io::BufWriter;

use printpdf::{
    BuiltinFont, Color, IndirectFontRef, Line, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference, Point, Rgb,
};

use super::{plain_text, Block, Doc, Inline};

const PAGE_W: f32 = 595.28; // A4 in points
const PAGE_H: f32 = 841.89;
const MARGIN: f32 = 56.69; // 20 mm
const BODY: f32 = 11.0;
const LEADING: f32 = 1.4;

fn mm(pt: f32) -> Mm {
    Mm(pt * 25.4 / 72.0)
}

const REGULAR: &[&str] = &[
    "/system/fonts/Roboto-Regular.ttf",
    "/system/fonts/NotoSans-Regular.ttf",
    "/system/fonts/DroidSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/Library/Fonts/Arial.ttf",
    "C:\\Windows\\Fonts\\arial.ttf",
];
const BOLD: &[&str] = &[
    "/system/fonts/Roboto-Bold.ttf",
    "/system/fonts/NotoSans-Bold.ttf",
    "/system/fonts/DroidSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/Library/Fonts/Arial Bold.ttf",
    "C:\\Windows\\Fonts\\arialbd.ttf",
];
const ITALIC: &[&str] = &[
    "/system/fonts/Roboto-Italic.ttf",
    "/system/fonts/NotoSans-Italic.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Oblique.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Oblique.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Italic.ttf",
    "C:\\Windows\\Fonts\\ariali.ttf",
];
const MONO: &[&str] = &[
    "/system/fonts/DroidSansMono.ttf",
    "/system/fonts/CutiveMono.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    "C:\\Windows\\Fonts\\cour.ttf",
];

/// One font: the PDF reference, and its data for measuring text.
struct FontFace {
    pdf: IndirectFontRef,
    data: Option<Vec<u8>>,
    /// Width of each char for a builtin font, in em (rough).
    builtin_width: f32,
}

impl FontFace {
    fn width(&self, text: &str, size: f32) -> f32 {
        let Some(data) = &self.data else {
            return text.chars().count() as f32 * self.builtin_width * size;
        };
        let Ok(face) = ttf_parser::Face::parse(data, 0) else {
            return text.chars().count() as f32 * 0.5 * size;
        };
        let upem = face.units_per_em() as f32;
        let fallback = face.glyph_index('?').and_then(|g| face.glyph_hor_advance(g)).unwrap_or((upem / 2.0) as u16);
        let units: f32 = text
            .chars()
            .map(|c| face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).unwrap_or(fallback) as f32)
            .sum();
        units / upem * size
    }
}

#[derive(Clone, Copy, PartialEq)]
enum FontKind {
    Regular,
    Bold,
    Italic,
    Mono,
}

struct Fonts {
    regular: FontFace,
    bold: FontFace,
    italic: FontFace,
    mono: FontFace,
}

impl Fonts {
    fn get(&self, k: FontKind) -> &FontFace {
        match k {
            FontKind::Regular => &self.regular,
            FontKind::Bold => &self.bold,
            FontKind::Italic => &self.italic,
            FontKind::Mono => &self.mono,
        }
    }
}

fn load_font(doc: &PdfDocumentReference, paths: &[&str]) -> Option<FontFace> {
    for p in paths {
        if let Ok(data) = std::fs::read(p) {
            if ttf_parser::Face::parse(&data, 0).is_err() {
                continue;
            }
            if let Ok(f) = doc.add_external_font_with_subsetting(data.as_slice(), true) {
                return Some(FontFace { pdf: f, data: Some(data), builtin_width: 0.0 });
            }
        }
    }
    None
}

fn builtin(doc: &PdfDocumentReference, f: BuiltinFont, w: f32) -> Result<FontFace, String> {
    Ok(FontFace { pdf: doc.add_builtin_font(f).map_err(|e| e.to_string())?, data: None, builtin_width: w })
}

fn load_fonts(doc: &PdfDocumentReference) -> Result<Fonts, String> {
    let regular = match load_font(doc, REGULAR) {
        Some(f) => f,
        None => builtin(doc, BuiltinFont::Helvetica, 0.5)?,
    };
    let has_ttf = regular.data.is_some();
    let other = |paths: &[&str], b: BuiltinFont, w: f32| -> Result<FontFace, String> {
        if let Some(f) = load_font(doc, paths) {
            return Ok(f);
        }
        if has_ttf {
            // Use the regular font again, so all characters still show.
            let data = std::fs::read(REGULAR.iter().find(|p| std::path::Path::new(p).exists()).unwrap()).map_err(|e| e.to_string())?;
            let pdf = doc.add_external_font_with_subsetting(data.as_slice(), true).map_err(|e| e.to_string())?;
            return Ok(FontFace { pdf, data: Some(data), builtin_width: 0.0 });
        }
        builtin(doc, b, w)
    };
    Ok(Fonts {
        bold: other(BOLD, BuiltinFont::HelveticaBold, 0.55)?,
        italic: other(ITALIC, BuiltinFont::HelveticaOblique, 0.5)?,
        mono: other(MONO, BuiltinFont::Courier, 0.6)?,
        regular,
    })
}

/// A piece of text with one style, ready to place.
#[derive(Clone)]
struct Piece {
    text: String,
    font: FontKind,
    size: f32,
    link: bool,
}

struct Layout<'a> {
    doc: &'a PdfDocumentReference,
    fonts: Fonts,
    layer: PdfLayerReference,
    /// Distance from the bottom of the page to the next baseline space.
    y: f32,
    gray: bool,
}

const LINK_BLUE: (f32, f32, f32) = (0.02, 0.39, 0.76);

impl<'a> Layout<'a> {
    fn new_page(&mut self) {
        let (p, l) = self.doc.add_page(mm(PAGE_W), mm(PAGE_H), "Layer 1");
        self.layer = self.doc.get_page(p).get_layer(l);
        self.y = PAGE_H - MARGIN;
        self.base_color();
    }

    fn need(&mut self, h: f32) {
        if self.y - h < MARGIN {
            self.new_page();
        }
    }

    fn color(&self, (r, g, b): (f32, f32, f32)) {
        self.layer.set_fill_color(Color::Rgb(Rgb::new(r, g, b, None)));
    }

    fn base_color(&self) {
        if self.gray {
            self.color((0.35, 0.35, 0.35));
        } else {
            self.color((0.0, 0.0, 0.0));
        }
    }

    fn font_for(&self, i: &Inline, bold: bool) -> FontKind {
        if i.code {
            FontKind::Mono
        } else if i.bold || bold {
            FontKind::Bold
        } else if i.italic || self.gray {
            FontKind::Italic
        } else {
            FontKind::Regular
        }
    }

    /// Break the text into lines no wider than `width`. Each line is a list of pieces.
    fn wrap(&self, pieces: &[Piece], width: f32) -> Vec<Vec<Piece>> {
        let mut lines: Vec<Vec<Piece>> = vec![Vec::new()];
        let mut line_w = 0.0f32;
        for p in pieces {
            let font = self.fonts.get(p.font);
            for (n, hard_line) in p.text.split('\n').enumerate() {
                if n > 0 {
                    lines.push(Vec::new());
                    line_w = 0.0;
                }
                let hard_line = hard_line.replace('\t', "    ");
                // Words with the space after them.
                for word in split_keep_spaces(&hard_line) {
                    let mut w = font.width(word, p.size);
                    if line_w + w > width && line_w > 0.0 {
                        lines.push(Vec::new());
                        line_w = 0.0;
                        if word.trim().is_empty() {
                            continue;
                        }
                    }
                    let mut word = word.to_owned();
                    // A single word longer than the line: cut it.
                    while w > width && word.chars().count() > 1 {
                        let mut cut = String::new();
                        for c in word.chars() {
                            let mut t = cut.clone();
                            t.push(c);
                            if font.width(&t, p.size) > width && !cut.is_empty() {
                                break;
                            }
                            cut = t;
                        }
                        let rest: String = word.chars().skip(cut.chars().count()).collect();
                        push_piece(lines.last_mut().unwrap(), &cut, p);
                        lines.push(Vec::new());
                        word = rest;
                        w = font.width(&word, p.size);
                    }
                    push_piece(lines.last_mut().unwrap(), &word, p);
                    line_w += w;
                }
            }
        }
        // Drop trailing spaces, so they do not count.
        for l in lines.iter_mut() {
            if let Some(last) = l.last_mut() {
                last.text = last.text.trim_end().to_owned();
            }
        }
        lines
    }

    fn draw_line(&self, line: &[Piece], x: f32, baseline: f32) {
        let mut cx = x;
        for p in line {
            if p.text.is_empty() {
                continue;
            }
            let font = self.fonts.get(p.font);
            if p.link {
                self.color(LINK_BLUE);
            }
            self.layer.use_text(p.text.clone(), p.size, mm(cx), mm(baseline), &font.pdf);
            if p.link {
                self.base_color();
            }
            cx += font.width(&p.text, p.size);
        }
    }

    /// Lay out one paragraph. Returns nothing; moves `y` down.
    fn para(&mut self, pieces: &[Piece], x: f32, width: f32, size: f32, space_after: f32) {
        let lh = size * LEADING;
        for line in self.wrap(pieces, width) {
            self.need(lh);
            self.y -= lh;
            self.draw_line(&line, x, self.y + (lh - size) / 2.0 + size * 0.2);
        }
        self.y -= space_after;
    }

    fn pieces(&self, v: &[Inline], size: f32, bold: bool) -> Vec<Piece> {
        v.iter()
            .map(|i| Piece {
                text: i.text.clone(),
                font: self.font_for(i, bold),
                size: if i.code { size * 0.9 } else { size },
                link: i.link.is_some(),
            })
            .collect()
    }

    fn hline(&self, x1: f32, x2: f32, y: f32, thick: f32) {
        self.layer.set_outline_thickness(thick);
        self.layer.add_line(Line {
            points: vec![(Point::new(mm(x1), mm(y)), false), (Point::new(mm(x2), mm(y)), false)],
            is_closed: false,
        });
    }

    fn vline(&self, x: f32, y1: f32, y2: f32) {
        self.layer.set_outline_thickness(0.5);
        self.layer.add_line(Line {
            points: vec![(Point::new(mm(x), mm(y1)), false), (Point::new(mm(x), mm(y2)), false)],
            is_closed: false,
        });
    }

    fn blocks(&mut self, bs: &[Block], x: f32, width: f32) {
        for b in bs {
            match b {
                Block::Heading(l, i) => {
                    let size = match l {
                        1 => 20.0,
                        2 => 16.0,
                        3 => 14.0,
                        _ => 12.0,
                    };
                    // Keep the heading with at least two lines after it.
                    self.need(size * LEADING + BODY * LEADING * 2.0 + 10.0);
                    self.y -= 10.0;
                    let p = self.pieces(i, size, true);
                    self.para(&p, x, width, size, 4.0);
                }
                Block::Para(i) => {
                    let p = self.pieces(i, BODY, false);
                    self.para(&p, x, width, BODY, 6.0);
                }
                Block::List { ordered, items } => {
                    let indent = 18.0;
                    for (n, item) in items.iter().enumerate() {
                        let marker = if *ordered { format!("{}.", n + 1) } else { "•".to_owned() };
                        self.need(BODY * LEADING);
                        let top = self.y;
                        let mw = self.fonts.regular.width(&marker, BODY);
                        let baseline = top - BODY * LEADING + (BODY * LEADING - BODY) / 2.0 + BODY * 0.2;
                        let mark = Piece { text: marker, font: FontKind::Regular, size: BODY, link: false };
                        self.draw_line(&[mark], x + indent - mw - 4.0, baseline);
                        self.blocks_tight(item, x + indent, width - indent);
                    }
                    self.y -= 4.0;
                }
                Block::Quote(inner) => {
                    let top = self.y;
                    let was = self.gray;
                    self.gray = true;
                    self.base_color();
                    self.blocks(inner, x + 14.0, width - 14.0);
                    self.gray = was;
                    self.base_color();
                    // A bar on the left, when the quote is on one page.
                    if self.y < top {
                        self.vline(x + 4.0, top - 2.0, self.y + 6.0);
                    }
                }
                Block::Code(code) => {
                    let p = vec![Piece { text: code.clone(), font: FontKind::Mono, size: 9.5, link: false }];
                    self.y -= 2.0;
                    self.para(&p, x + 8.0, width - 8.0, 9.5, 8.0);
                }
                Block::Table { header, rows } => self.table(*header, rows, x, width),
                Block::Rule => {
                    self.need(14.0);
                    self.y -= 7.0;
                    self.hline(x, x + width, self.y, 0.75);
                    self.y -= 7.0;
                }
            }
        }
    }

    /// Like `blocks`, but with less space after paragraphs (for list items).
    fn blocks_tight(&mut self, bs: &[Block], x: f32, width: f32) {
        for b in bs {
            if let Block::Para(i) = b {
                let p = self.pieces(i, BODY, false);
                self.para(&p, x, width, BODY, 2.0);
            } else {
                self.blocks(std::slice::from_ref(b), x, width);
            }
        }
    }

    fn table(&mut self, header: bool, rows: &[Vec<Vec<Inline>>], x: f32, width: f32) {
        let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if cols == 0 {
            return;
        }
        let size = 10.0;
        let lh = size * LEADING;
        let pad = 4.0;
        let cw = width / cols as f32;
        self.y -= 4.0;
        for (n, row) in rows.iter().enumerate() {
            let bold = header && n == 0;
            let cells: Vec<Vec<Vec<Piece>>> = (0..cols)
                .map(|k| {
                    let v = row.get(k).cloned().unwrap_or_default();
                    let p = self.pieces(&v, size, bold);
                    self.wrap(&p, cw - 2.0 * pad)
                })
                .collect();
            let lines = cells.iter().map(|c| c.len()).max().unwrap_or(1).max(1);
            let h = lines as f32 * lh + 2.0 * pad;
            if self.y - h < MARGIN {
                self.new_page();
            }
            let top = self.y;
            self.hline(x, x + width, top, 0.5);
            for (k, cell) in cells.iter().enumerate() {
                let cx = x + k as f32 * cw + pad;
                for (li, line) in cell.iter().enumerate() {
                    let baseline = top - pad - (li as f32 + 1.0) * lh + (lh - size) / 2.0 + size * 0.2;
                    self.draw_line(line, cx, baseline);
                }
            }
            self.y -= h;
            self.hline(x, x + width, self.y, 0.5);
            for k in 0..=cols {
                self.vline(x + k as f32 * cw, top, self.y);
            }
        }
        self.y -= 8.0;
    }
}

fn push_piece(line: &mut Vec<Piece>, text: &str, style: &Piece) {
    if text.is_empty() {
        return;
    }
    match line.last_mut() {
        Some(l) if l.font == style.font && l.size == style.size && l.link == style.link => l.text.push_str(text),
        _ => line.push(Piece { text: text.to_owned(), ..style.clone() }),
    }
}

/// "a b  c" -> ["a ", "b  ", "c"]
fn split_keep_spaces(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = false;
    for (i, c) in s.char_indices() {
        if c == ' ' {
            in_space = true;
        } else if in_space {
            out.push(&s[start..i]);
            start = i;
            in_space = false;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

pub fn write(doc: &Doc, path: &str) -> Result<(), String> {
    let title = doc.title.clone().unwrap_or_else(|| {
        doc.blocks.iter().find_map(|b| if let Block::Para(i) = b { Some(plain_text(i)) } else { None }).unwrap_or_default()
    });
    let title: String = title.chars().take(120).collect();
    let (pdf, page, layer) = PdfDocument::new(&title, mm(PAGE_W), mm(PAGE_H), "Layer 1");
    let fonts = load_fonts(&pdf)?;
    let layer = pdf.get_page(page).get_layer(layer);
    {
        let mut lay = Layout { doc: &pdf, fonts, layer, y: PAGE_H - MARGIN, gray: false };
        lay.base_color();
        lay.blocks(&doc.blocks, MARGIN, PAGE_W - 2.0 * MARGIN);
    }
    let file = File::create(path).map_err(|e| e.to_string())?;
    pdf.save(&mut BufWriter::new(file)).map_err(|e| e.to_string())
}
