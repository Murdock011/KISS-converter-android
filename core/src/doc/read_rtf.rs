//! A small RTF reader. It keeps text, paragraphs, bold and italic.
//! It skips fonts, colours, pictures and other "destinations".

use super::{Builder, Doc, Inline};

#[derive(Clone, Copy, Default)]
struct State {
    bold: bool,
    italic: bool,
    skip: bool,
    /// Characters to skip after a \u escape (\ucN).
    uc: usize,
}

const SKIP_DESTINATIONS: &[&str] = &[
    "fonttbl", "colortbl", "stylesheet", "info", "pict", "header", "footer", "headerl", "headerr", "footerl",
    "footerr", "listtable", "listoverridetable", "rsidtbl", "generator", "xmlnstbl", "themedata", "colorschememapping",
    "datastore", "latentstyles", "filetbl", "revtbl", "object", "fldinst", "bkmkstart", "bkmkend", "footnote",
];

/// Windows-1252 bytes 0x80..0x9F that differ from Latin-1.
fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘',
        '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    if (0x80..0xA0).contains(&b) {
        HIGH[(b - 0x80) as usize]
    } else {
        b as char
    }
}

pub fn read(data: &[u8]) -> Doc {
    let mut b = Builder::default();
    let mut stack: Vec<State> = Vec::new();
    let mut st = State { uc: 1, ..Default::default() };
    let mut pending_skip = 0usize;
    let mut i = 0usize;
    let mut text = String::new();
    // A group that starts with \* is an optional destination; skip it if unknown.
    let mut group_start = false;

    let flush = |text: &mut String, b: &mut Builder, st: &State| {
        if !text.is_empty() {
            b.text(Inline { text: std::mem::take(text), bold: st.bold, italic: st.italic, ..Default::default() });
        }
    };

    while i < data.len() {
        let c = data[i];
        match c {
            b'{' => {
                flush(&mut text, &mut b, &st);
                stack.push(st);
                group_start = true;
                i += 1;
                continue;
            }
            b'}' => {
                flush(&mut text, &mut b, &st);
                st = stack.pop().unwrap_or_default();
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&n) = data.get(i) else { break };
                if n.is_ascii_alphabetic() {
                    let start = i;
                    while i < data.len() && data[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = std::str::from_utf8(&data[start..i]).unwrap_or("");
                    let num_start = i;
                    if i < data.len() && (data[i] == b'-' || data[i].is_ascii_digit()) {
                        i += 1;
                        while i < data.len() && data[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let param: Option<i32> = std::str::from_utf8(&data[num_start..i]).ok().and_then(|s| s.parse().ok());
                    if i < data.len() && data[i] == b' ' {
                        i += 1;
                    }
                    if group_start && SKIP_DESTINATIONS.contains(&word) {
                        st.skip = true;
                    }
                    if !st.skip {
                        match word {
                            "par" | "sect" | "page" => {
                                flush(&mut text, &mut b, &st);
                                b.end_para();
                            }
                            "line" => text.push('\n'),
                            "tab" => text.push('\t'),
                            "cell" => text.push('\t'),
                            "row" => {
                                flush(&mut text, &mut b, &st);
                                b.end_para();
                            }
                            "emdash" => text.push('—'),
                            "endash" => text.push('–'),
                            "bullet" => text.push('•'),
                            "lquote" => text.push('‘'),
                            "rquote" => text.push('’'),
                            "ldblquote" => text.push('“'),
                            "rdblquote" => text.push('”'),
                            "b" => {
                                flush(&mut text, &mut b, &st);
                                st.bold = param != Some(0);
                            }
                            "i" => {
                                flush(&mut text, &mut b, &st);
                                st.italic = param != Some(0);
                            }
                            "plain" => {
                                flush(&mut text, &mut b, &st);
                                st.bold = false;
                                st.italic = false;
                            }
                            "uc" => st.uc = param.unwrap_or(1).max(0) as usize,
                            "u" => {
                                if let Some(p) = param {
                                    let cp = if p < 0 { p + 65536 } else { p } as u32;
                                    if let Some(ch) = char::from_u32(cp) {
                                        text.push(ch);
                                    }
                                    pending_skip = st.uc;
                                }
                            }
                            _ => {}
                        }
                    }
                } else {
                    i += 1;
                    match n {
                        b'*' => st.skip = true,
                        b'\'' => {
                            let hex = data.get(i..i + 2).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
                            i += 2;
                            if let Some(h) = hex {
                                if pending_skip > 0 {
                                    pending_skip -= 1;
                                } else if !st.skip {
                                    text.push(cp1252(h));
                                }
                            }
                        }
                        b'~' if !st.skip => text.push('\u{a0}'),
                        b'-' | b'_' => {}
                        b'\\' | b'{' | b'}' if !st.skip => text.push(n as char),
                        b'\n' | b'\r' if !st.skip => {
                            flush(&mut text, &mut b, &st);
                            b.end_para();
                        }
                        _ => {}
                    }
                }
            }
            b'\r' | b'\n' => i += 1,
            _ => {
                if pending_skip > 0 {
                    pending_skip -= 1;
                } else if !st.skip {
                    text.push(c as char);
                }
                i += 1;
            }
        }
        group_start = false;
    }
    flush(&mut text, &mut b, &st);
    b.finish()
}
