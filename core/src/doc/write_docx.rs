//! DOCX writer. Makes a small, valid Word package.

use std::io::Write;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::xml::esc;
use super::{Block, Doc, Inline};

struct Ctx {
    body: String,
    /// External link targets; the index gives the relationship id.
    links: Vec<String>,
    /// One <w:num> per list; true = numbered.
    nums: Vec<bool>,
}

impl Ctx {
    fn runs(&mut self, v: &[Inline]) -> String {
        let mut out = String::new();
        for i in v {
            let mut rpr = String::new();
            if i.link.is_some() {
                rpr.push_str(r#"<w:rStyle w:val="Hyperlink"/>"#);
            }
            if i.code {
                rpr.push_str(r#"<w:rFonts w:ascii="Courier New" w:hAnsi="Courier New" w:cs="Courier New"/>"#);
            }
            if i.bold {
                rpr.push_str("<w:b/>");
            }
            if i.italic {
                rpr.push_str("<w:i/>");
            }
            let mut run = String::from("<w:r>");
            if !rpr.is_empty() {
                run.push_str(&format!("<w:rPr>{rpr}</w:rPr>"));
            }
            for (n, line) in i.text.split('\n').enumerate() {
                if n > 0 {
                    run.push_str("<w:br/>");
                }
                for (k, part) in line.split('\t').enumerate() {
                    if k > 0 {
                        run.push_str("<w:tab/>");
                    }
                    if !part.is_empty() {
                        run.push_str(&format!(r#"<w:t xml:space="preserve">{}</w:t>"#, esc(part)));
                    }
                }
            }
            run.push_str("</w:r>");
            if let Some(url) = &i.link {
                self.links.push(url.clone());
                let id = format!("rIdL{}", self.links.len());
                out.push_str(&format!(r#"<w:hyperlink r:id="{id}">{run}</w:hyperlink>"#));
            } else {
                out.push_str(&run);
            }
        }
        out
    }

    fn para(&mut self, ppr: &str, v: &[Inline]) {
        let runs = self.runs(v);
        if ppr.is_empty() {
            self.body.push_str(&format!("<w:p>{runs}</w:p>"));
        } else {
            self.body.push_str(&format!("<w:p><w:pPr>{ppr}</w:pPr>{runs}</w:p>"));
        }
    }

    fn blocks(&mut self, bs: &[Block], quote: bool, list: Option<(usize, usize)>) {
        for b in bs {
            let mut ppr = String::new();
            if quote {
                ppr.push_str(r#"<w:pStyle w:val="Quote"/>"#);
            }
            if let Some((num, lvl)) = list {
                if !quote {
                    ppr.push_str(r#"<w:pStyle w:val="ListParagraph"/>"#);
                }
                ppr.push_str(&format!(r#"<w:numPr><w:ilvl w:val="{lvl}"/><w:numId w:val="{num}"/></w:numPr>"#));
            }
            match b {
                Block::Heading(l, i) => self.para(&format!(r#"<w:pStyle w:val="Heading{l}"/>"#), i),
                Block::Para(i) => self.para(&ppr, i),
                Block::List { ordered, items } => {
                    self.nums.push(*ordered);
                    let num = self.nums.len();
                    let lvl = list.map(|(_, l)| l + 1).unwrap_or(0).min(8);
                    for item in items {
                        // The first paragraph gets the number or bullet; the rest are indented text.
                        let (first, rest) = item.split_first().map(|(f, r)| (Some(f), r)).unwrap_or((None, &[]));
                        if let Some(f) = first {
                            self.blocks(std::slice::from_ref(f), quote, Some((num, lvl)));
                        }
                        for r in rest {
                            match r {
                                Block::List { .. } => self.blocks(std::slice::from_ref(r), quote, Some((num, lvl))),
                                _ => {
                                    let ind = 720 * (lvl + 1);
                                    if let Block::Para(i) = r {
                                        self.para(&format!(r#"<w:ind w:left="{ind}"/>"#), i);
                                    } else {
                                        self.blocks(std::slice::from_ref(r), quote, None);
                                    }
                                }
                            }
                        }
                    }
                }
                Block::Quote(inner) => self.blocks(inner, true, list),
                Block::Code(code) => {
                    let i = Inline { text: code.clone(), code: true, ..Default::default() };
                    self.para(r#"<w:pStyle w:val="Code"/>"#, &[i]);
                }
                Block::Table { header, rows } => {
                    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
                    let w = 9000 / cols;
                    self.body.push_str(r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="5000" w:type="pct"/></w:tblPr><w:tblGrid>"#);
                    for _ in 0..cols {
                        self.body.push_str(&format!(r#"<w:gridCol w:w="{w}"/>"#));
                    }
                    self.body.push_str("</w:tblGrid>");
                    for (n, r) in rows.iter().enumerate() {
                        let head = *header && n == 0;
                        self.body.push_str("<w:tr>");
                        if head {
                            self.body.push_str("<w:trPr><w:tblHeader/></w:trPr>");
                        }
                        for k in 0..cols {
                            let mut cell: Vec<Inline> = r.get(k).cloned().unwrap_or_default();
                            if head {
                                cell.iter_mut().for_each(|i| i.bold = true);
                            }
                            self.body.push_str(&format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr>"#));
                            self.para("", &cell);
                            self.body.push_str("</w:tc>");
                        }
                        self.body.push_str("</w:tr>");
                    }
                    self.body.push_str("</w:tbl><w:p/>");
                }
                Block::Rule => self.body.push_str(
                    r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="auto"/></w:pBdr></w:pPr></w:p>"#,
                ),
            }
        }
    }
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
</Relationships>"#;

fn styles() -> String {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="Calibri" w:cs="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault>
<w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="276" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>
"#,
    );
    let sizes = [40, 32, 28, 26, 24, 22];
    for (n, sz) in sizes.iter().enumerate() {
        let l = n + 1;
        s.push_str(&format!(
            r#"<w:style w:type="paragraph" w:styleId="Heading{l}"><w:name w:val="heading {l}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="120"/><w:outlineLvl w:val="{}"/></w:pPr><w:rPr><w:b/><w:sz w:val="{sz}"/></w:rPr></w:style>
"#,
            l - 1
        ));
    }
    s.push_str(r#"<w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:ind w:left="720"/></w:pPr><w:rPr><w:i/><w:color w:val="595959"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Code"><w:name w:val="Code"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/><w:shd w:val="clear" w:color="auto" w:fill="F2F2F2"/></w:pPr><w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New" w:cs="Courier New"/><w:sz w:val="20"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="60"/><w:contextualSpacing/></w:pPr></w:style>
<w:style w:type="character" w:styleId="Hyperlink"><w:name w:val="Hyperlink"/><w:rPr><w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr></w:style>
<w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders></w:tblPr></w:style>
</w:styles>"#);
    s
}

fn numbering(nums: &[bool]) -> String {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
"#,
    );
    for (abs, ordered) in [(0, false), (1, true)] {
        s.push_str(&format!(r#"<w:abstractNum w:abstractNumId="{abs}"><w:multiLevelType w:val="hybridMultilevel"/>"#));
        for lvl in 0..9 {
            let ind = 720 * (lvl + 1);
            let (fmt, text) = if ordered {
                ("decimal", format!("%{}.", lvl + 1))
            } else {
                ("bullet", ["•", "◦", "▪"][lvl % 3].to_owned())
            };
            s.push_str(&format!(
                r#"<w:lvl w:ilvl="{lvl}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{ind}" w:hanging="360"/></w:pPr></w:lvl>"#
            ));
        }
        s.push_str("</w:abstractNum>\n");
    }
    for (n, ordered) in nums.iter().enumerate() {
        let abs = if *ordered { 1 } else { 0 };
        // Each numbered list starts again at 1.
        s.push_str(&format!(
            r#"<w:num w:numId="{}"><w:abstractNumId w:val="{abs}"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/></w:lvlOverride></w:num>
"#,
            n + 1
        ));
    }
    s.push_str("</w:numbering>");
    s
}

pub fn write(doc: &Doc, path: &str) -> Result<(), String> {
    let mut ctx = Ctx { body: String::new(), links: Vec::new(), nums: Vec::new() };
    ctx.blocks(&doc.blocks, false, None);

    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>"#,
        ctx.body
    );
    let mut rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rIdNumbering" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
"#,
    );
    for (n, url) in ctx.links.iter().enumerate() {
        rels.push_str(&format!(
            r#"<Relationship Id="rIdL{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="{}" TargetMode="External"/>
"#,
            n + 1,
            esc(url)
        ));
    }
    rels.push_str("</Relationships>");
    let core = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title><dc:creator>Converter</dc:creator></cp:coreProperties>"#,
        esc(doc.title.as_deref().unwrap_or(""))
    );

    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let parts: [(&str, &str); 7] = [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", &document),
        ("word/_rels/document.xml.rels", &rels),
        ("word/styles.xml", &styles()),
        ("word/numbering.xml", &numbering(&ctx.nums)),
        ("docProps/core.xml", &core),
    ];
    for (name, data) in parts {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}
