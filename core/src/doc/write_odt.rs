//! ODT writer. Makes a small, valid OpenDocument Text package.

use std::io::Write;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::xml::esc;
use super::{Block, Doc, Inline};

fn text_with_breaks(s: &str) -> String {
    let mut out = String::new();
    for (n, line) in s.split('\n').enumerate() {
        if n > 0 {
            out.push_str("<text:line-break/>");
        }
        for (k, part) in line.split('\t').enumerate() {
            if k > 0 {
                out.push_str("<text:tab/>");
            }
            // Keep runs of spaces: XML would fold them into one.
            let mut spaces = 0;
            for c in part.chars() {
                if c == ' ' {
                    spaces += 1;
                    continue;
                }
                flush_spaces(&mut out, &mut spaces);
                out.push_str(&esc(&c.to_string()));
            }
            flush_spaces(&mut out, &mut spaces);
        }
    }
    out
}

fn flush_spaces(out: &mut String, n: &mut usize) {
    match *n {
        0 => {}
        1 => out.push(' '),
        k => out.push_str(&format!(" <text:s text:c=\"{}\"/>", k - 1)),
    }
    *n = 0;
}

fn spans(v: &[Inline]) -> String {
    let mut out = String::new();
    for i in v {
        let style = match (i.bold, i.italic, i.code) {
            (_, _, true) => Some("TCode"),
            (true, true, _) => Some("TBoldItalic"),
            (true, false, _) => Some("TBold"),
            (false, true, _) => Some("TItalic"),
            _ => None,
        };
        let mut t = text_with_breaks(&i.text);
        if let Some(s) = style {
            t = format!(r##"<text:span text:style-name="{s}">{t}</text:span>"##);
        }
        if let Some(url) = &i.link {
            t = format!(r##"<text:a xlink:type="simple" xlink:href="{}">{t}</text:a>"##, esc(url));
        }
        out.push_str(&t);
    }
    out
}

fn blocks(bs: &[Block], para_style: &str, out: &mut String) {
    for b in bs {
        match b {
            Block::Heading(l, i) => out.push_str(&format!(
                r##"<text:h text:style-name="Heading_20_{l}" text:outline-level="{l}">{}</text:h>"##,
                spans(i)
            )),
            Block::Para(i) => out.push_str(&format!(r##"<text:p text:style-name="{para_style}">{}</text:p>"##, spans(i))),
            Block::List { ordered, items } => {
                let style = if *ordered { "LNumbered" } else { "LBullet" };
                out.push_str(&format!(r##"<text:list text:style-name="{style}">"##));
                for item in items {
                    out.push_str("<text:list-item>");
                    blocks(item, "List_20_Paragraph", out);
                    out.push_str("</text:list-item>");
                }
                out.push_str("</text:list>");
            }
            Block::Quote(inner) => blocks(inner, "Quotations", out),
            Block::Code(c) => {
                for line in c.split('\n') {
                    out.push_str(&format!(r##"<text:p text:style-name="Preformatted_20_Text">{}</text:p>"##, text_with_breaks(line)));
                }
            }
            Block::Table { header, rows } => {
                let cols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
                out.push_str(&format!(
                    r##"<table:table table:style-name="Tbl"><table:table-column table:number-columns-repeated="{cols}"/>"##
                ));
                for (n, r) in rows.iter().enumerate() {
                    let head = *header && n == 0;
                    if head {
                        out.push_str("<table:table-header-rows>");
                    }
                    out.push_str("<table:table-row>");
                    for k in 0..cols {
                        let mut cell: Vec<Inline> = r.get(k).cloned().unwrap_or_default();
                        if head {
                            cell.iter_mut().for_each(|i| i.bold = true);
                        }
                        out.push_str(&format!(
                            r##"<table:table-cell table:style-name="Cell" office:value-type="string"><text:p text:style-name="Table_20_Contents">{}</text:p></table:table-cell>"##,
                            spans(&cell)
                        ));
                    }
                    out.push_str("</table:table-row>");
                    if head {
                        out.push_str("</table:table-header-rows>");
                    }
                }
                out.push_str("</table:table>");
            }
            Block::Rule => out.push_str(r##"<text:p text:style-name="Rule"/>"##),
        }
    }
}

const NS: &str = r##"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0""##;

fn styles_xml() -> String {
    let mut headings = String::new();
    for (l, size) in [(1, "20pt"), (2, "16pt"), (3, "14pt"), (4, "13pt"), (5, "12pt"), (6, "11pt")] {
        headings.push_str(&format!(
            r##"<style:style style:name="Heading_20_{l}" style:display-name="Heading {l}" style:family="paragraph" style:parent-style-name="Standard" style:next-style-name="Standard" style:default-outline-level="{l}"><style:paragraph-properties fo:margin-top="0.17in" fo:margin-bottom="0.08in" fo:keep-with-next="always"/><style:text-properties fo:font-size="{size}" fo:font-weight="bold"/></style:style>"##
        ));
    }
    format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {NS} office:version="1.3"><office:styles>
<style:default-style style:family="paragraph"><style:paragraph-properties fo:margin-bottom="0.08in" fo:line-height="115%"/><style:text-properties style:font-name="Liberation Sans" fo:font-family="'Liberation Sans', Arial, sans-serif" fo:font-size="11pt"/></style:default-style>
<style:style style:name="Standard" style:family="paragraph"/>
<style:style style:name="Text_20_body" style:display-name="Text body" style:family="paragraph" style:parent-style-name="Standard"/>
<style:style style:name="List_20_Paragraph" style:display-name="List Paragraph" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:margin-bottom="0.03in"/></style:style>
<style:style style:name="Quotations" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:margin-left="0.4in"/><style:text-properties fo:font-style="italic" fo:color="#555555"/></style:style>
<style:style style:name="Preformatted_20_Text" style:display-name="Preformatted Text" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:margin-bottom="0in" fo:background-color="#f2f2f2"/><style:text-properties fo:font-family="'Liberation Mono', 'Courier New', monospace" fo:font-size="10pt"/></style:style>
<style:style style:name="Table_20_Contents" style:display-name="Table Contents" style:family="paragraph" style:parent-style-name="Standard"/>
<style:style style:name="Rule" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:border-bottom="0.5pt solid #000000" fo:padding="0in"/></style:style>
{headings}
</office:styles></office:document-styles>"##
    )
}

fn content_xml(doc: &Doc) -> String {
    let mut body = String::new();
    blocks(&doc.blocks, "Text_20_body", &mut body);
    let mut bullet_levels = String::new();
    let mut number_levels = String::new();
    for l in 1..=10 {
        let indent = format!("{:.2}in", 0.25 * l as f32);
        let props = format!(
            r##"<style:list-level-properties text:list-level-position-and-space-mode="label-alignment"><style:list-level-label-alignment text:label-followed-by="listtab" fo:text-indent="-0.25in" fo:margin-left="{indent}"/></style:list-level-properties>"##
        );
        let bullet = ["•", "◦", "▪"][(l - 1) % 3];
        bullet_levels.push_str(&format!(r##"<text:list-level-style-bullet text:level="{l}" text:bullet-char="{bullet}">{props}</text:list-level-style-bullet>"##));
        number_levels.push_str(&format!(r##"<text:list-level-style-number text:level="{l}" style:num-suffix="." style:num-format="1">{props}</text:list-level-style-number>"##));
    }
    format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {NS} office:version="1.3"><office:automatic-styles>
<style:style style:name="TBold" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>
<style:style style:name="TItalic" style:family="text"><style:text-properties fo:font-style="italic"/></style:style>
<style:style style:name="TBoldItalic" style:family="text"><style:text-properties fo:font-weight="bold" fo:font-style="italic"/></style:style>
<style:style style:name="TCode" style:family="text"><style:text-properties fo:font-family="'Liberation Mono', 'Courier New', monospace"/></style:style>
<style:style style:name="Tbl" style:family="table"><style:table-properties style:width="6.5in" table:align="margins"/></style:style>
<style:style style:name="Cell" style:family="table-cell"><style:table-cell-properties fo:padding="0.04in" fo:border="0.5pt solid #000000"/></style:style>
<text:list-style style:name="LBullet">{bullet_levels}</text:list-style>
<text:list-style style:name="LNumbered">{number_levels}</text:list-style>
</office:automatic-styles><office:body><office:text>{body}</office:text></office:body></office:document-content>"##
    )
}

pub fn write(doc: &Doc, path: &str) -> Result<(), String> {
    let manifest = r##"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
<manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/>
<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
<manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
<manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"##;
    let meta = format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<office:document-meta {NS} office:version="1.3"><office:meta><dc:title>{}</dc:title><meta:generator>Converter</meta:generator></office:meta></office:document-meta>"##,
        esc(doc.title.as_deref().unwrap_or(""))
    );

    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    // The "mimetype" file must be first and not compressed.
    zip.start_file("mimetype", SimpleFileOptions::default().compression_method(CompressionMethod::Stored))
        .map_err(|e| e.to_string())?;
    zip.write_all(b"application/vnd.oasis.opendocument.text").map_err(|e| e.to_string())?;
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let content = content_xml(doc);
    let styles = styles_xml();
    for (name, data) in [
        ("META-INF/manifest.xml", manifest),
        ("content.xml", content.as_str()),
        ("styles.xml", styles.as_str()),
        ("meta.xml", meta.as_str()),
    ] {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}
