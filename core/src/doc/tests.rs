use super::*;

const MD: &str = "# Title\n\nHello **bold** and *it* with `code` and [a link](https://example.com).\n\n## Part\n\n- one\n- two\n  1. a\n  2. b\n\n> quoted text\n\n```\nfn main() {}\n```\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n---\n\nÜnïcödé и 中文\n";

fn sample() -> Doc {
    read_md::read(MD)
}

#[test]
fn markdown_reads_structure() {
    let d = sample();
    assert!(matches!(&d.blocks[0], Block::Heading(1, _)));
    let para = match &d.blocks[1] { Block::Para(i) => i, b => panic!("{b:?}") };
    assert!(para.iter().any(|i| i.bold && i.text == "bold"));
    assert!(para.iter().any(|i| i.link.as_deref() == Some("https://example.com")));
    assert!(d.blocks.iter().any(|b| matches!(b, Block::List { ordered: false, .. })));
    assert!(d.blocks.iter().any(|b| matches!(b, Block::Table { header: true, .. })));
    assert!(d.blocks.iter().any(|b| matches!(b, Block::Code(_))));
}

fn tmp(name: &str) -> String {
    let dir = std::env::temp_dir().join("convert_core_tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name).to_string_lossy().into_owned()
}

/// Write each format, read it back, and check the main text is still there.
#[test]
fn round_trips() {
    let d = sample();
    for ext in ["docx", "odt", "html", "md", "txt"] {
        let path = tmp(&format!("rt.{ext}"));
        match ext {
            "docx" => write_docx::write(&d, &path).unwrap(),
            "odt" => write_odt::write(&d, &path).unwrap(),
            "html" => std::fs::write(&path, write_html::write(&d)).unwrap(),
            "md" => std::fs::write(&path, write_md::write(&d)).unwrap(),
            _ => std::fs::write(&path, write_txt::write(&d)).unwrap(),
        }
        let back = read(&path, ext).unwrap();
        let text = write_txt::write(&back);
        for want in ["Title", "bold", "quoted text", "fn main", "Ünïcödé и 中文", "one", "two"] {
            assert!(text.contains(want), "{ext}: missing {want:?} in:\n{text}");
        }
        if ext != "txt" {
            let first = &back.blocks[0];
            assert!(matches!(first, Block::Heading(1, _)), "{ext}: first block {first:?}");
            let has_bold = back.blocks.iter().any(|b| matches!(b, Block::Para(i) if i.iter().any(|x| x.bold)));
            assert!(has_bold, "{ext}: bold lost");
        }
    }
}

#[test]
fn rtf_basic() {
    let rtf = b"{\\rtf1\\ansi{\\fonttbl{\\f0 Arial;}}\\f0 Hello {\\b bold} caf\\'e9 \\u8364?\\par Second line\\par}";
    let d = read_rtf::read(rtf);
    let t = write_txt::write(&d);
    assert!(t.contains("Hello bold café €"), "{t}");
    assert!(t.contains("Second line"));
    assert!(!t.contains("Arial"));
    assert!(matches!(&d.blocks[0], Block::Para(i) if i.iter().any(|x| x.bold && x.text == "bold")));
}

#[test]
fn html_basic() {
    let d = read_html::read("<html><head><title>T</title><style>p{}</style></head><body><h2>Head</h2><p>A &amp; B<br>next</p><ul><li>x</li><li>y</li></ul><table><tr><th>h</th></tr><tr><td>c</td></tr></table></body></html>");
    assert_eq!(d.title.as_deref(), Some("T"));
    assert!(matches!(&d.blocks[0], Block::Heading(2, _)));
    assert_eq!(plain_text(match &d.blocks[1] { Block::Para(i) => i, _ => panic!() }), "A & B\nnext");
    assert!(matches!(&d.blocks[2], Block::List { items, .. } if items.len() == 2));
    assert!(matches!(&d.blocks[3], Block::Table { header: true, .. }));
}

#[test]
fn pdf_is_written() {
    let path = tmp("doc.pdf");
    let mut d = sample();
    // Make it long, so it needs more than one page.
    for _ in 0..40 {
        d.blocks.push(Block::Para(vec![Inline::plain("Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(4))]));
    }
    write_pdf::write(&d, &path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    assert!(bytes.len() > 1000);
}
