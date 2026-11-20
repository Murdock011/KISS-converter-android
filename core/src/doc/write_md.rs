use super::{Block, Doc, Inline};

fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '>' | '#' | '|') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

pub fn inlines(v: &[Inline]) -> String {
    let mut out = String::new();
    for i in v {
        // Keep spaces outside the markers, or Markdown does not see them.
        let lead = i.text.len() - i.text.trim_start().len();
        let trail = i.text.len() - i.text.trim_end().len();
        let core = i.text.trim();
        if core.is_empty() {
            out.push_str(&i.text.replace('\n', "  \n"));
            continue;
        }
        let mut t = if i.code {
            let ticks = if core.contains('`') { "``" } else { "`" };
            format!("{ticks}{core}{ticks}")
        } else {
            escape(core).replace('\n', "  \n")
        };
        if i.bold && i.italic {
            t = format!("***{t}***");
        } else if i.bold {
            t = format!("**{t}**");
        } else if i.italic {
            t = format!("*{t}*");
        }
        if let Some(url) = &i.link {
            t = format!("[{t}](<{url}>)");
        }
        out.push_str(&i.text[..lead]);
        out.push_str(&t);
        out.push_str(&i.text[i.text.len() - trail..]);
    }
    out
}

pub fn write(doc: &Doc) -> String {
    let mut out = String::new();
    blocks(&doc.blocks, &mut out);
    out.trim_end().to_owned() + "\n"
}

fn blocks(bs: &[Block], out: &mut String) {
    for b in bs {
        match b {
            Block::Heading(l, i) => out.push_str(&format!("{} {}\n\n", "#".repeat(*l as usize), inlines(i).replace("  \n", " "))),
            Block::Para(i) => out.push_str(&format!("{}\n\n", inlines(i))),
            Block::List { ordered, items } => {
                for (n, item) in items.iter().enumerate() {
                    let mark = if *ordered { format!("{}. ", n + 1) } else { "- ".to_owned() };
                    let mut inner = String::new();
                    blocks(item, &mut inner);
                    let inner = inner.trim_end();
                    for (k, line) in inner.lines().enumerate() {
                        if k == 0 {
                            out.push_str(&format!("{mark}{line}\n"));
                        } else if line.is_empty() {
                            out.push('\n');
                        } else {
                            out.push_str(&format!("{}{line}\n", " ".repeat(mark.len())));
                        }
                    }
                }
                out.push('\n');
            }
            Block::Quote(inner) => {
                let mut s = String::new();
                blocks(inner, &mut s);
                for line in s.trim_end().lines() {
                    out.push_str(&format!("> {line}\n").replace("> \n", ">\n"));
                }
                out.push('\n');
            }
            Block::Code(code) => out.push_str(&format!("```\n{code}\n```\n\n")),
            Block::Table { header, rows } => {
                let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
                if cols == 0 {
                    continue;
                }
                let line = |r: &Vec<Vec<Inline>>| {
                    let mut cells: Vec<String> = r.iter().map(|c| inlines(c).replace('\n', " ")).collect();
                    cells.resize(cols, String::new());
                    format!("| {} |\n", cells.join(" | "))
                };
                let (head, body) = if *header { (line(&rows[0]), &rows[1..]) } else { (format!("|{}\n", "  |".repeat(cols)), &rows[..]) };
                out.push_str(&head);
                out.push_str(&format!("|{}\n", " --- |".repeat(cols)));
                for r in body {
                    out.push_str(&line(r));
                }
                out.push('\n');
            }
            Block::Rule => out.push_str("---\n\n"),
        }
    }
}
