use super::{plain_text, Block, Doc};

pub fn write(doc: &Doc) -> String {
    let mut out = String::new();
    blocks(&doc.blocks, 0, &mut out);
    out.trim_end().to_owned() + "\n"
}

fn indent(depth: usize) -> String {
    "    ".repeat(depth)
}

fn blocks(bs: &[Block], depth: usize, out: &mut String) {
    for b in bs {
        let pad = indent(depth);
        match b {
            Block::Heading(level, i) => {
                let t = plain_text(i);
                out.push_str(&format!("{pad}{t}\n"));
                if *level <= 2 {
                    let ch = if *level == 1 { '=' } else { '-' };
                    out.push_str(&format!("{pad}{}\n", ch.to_string().repeat(t.chars().count().max(3))));
                }
                out.push('\n');
            }
            Block::Para(i) => {
                let t = plain_text(i).replace('\n', &format!("\n{pad}"));
                out.push_str(&format!("{pad}{t}\n\n"));
            }
            Block::List { ordered, items } => {
                for (n, item) in items.iter().enumerate() {
                    let mark = if *ordered { format!("{}. ", n + 1) } else { "- ".to_owned() };
                    let mut inner = String::new();
                    blocks(item, 0, &mut inner);
                    let inner = inner.trim_end();
                    for (k, line) in inner.lines().enumerate() {
                        if k == 0 {
                            out.push_str(&format!("{pad}{mark}{line}\n"));
                        } else if line.is_empty() {
                            out.push('\n');
                        } else {
                            out.push_str(&format!("{pad}{}{line}\n", " ".repeat(mark.len())));
                        }
                    }
                }
                out.push('\n');
            }
            Block::Quote(inner) => {
                let mut s = String::new();
                blocks(inner, 0, &mut s);
                for line in s.trim_end().lines() {
                    out.push_str(&format!("{pad}> {line}\n"));
                }
                out.push('\n');
            }
            Block::Code(code) => {
                for line in code.lines() {
                    out.push_str(&format!("{pad}    {line}\n"));
                }
                out.push('\n');
            }
            Block::Table { rows, .. } => {
                for row in rows {
                    let cells: Vec<String> = row.iter().map(|c| plain_text(c)).collect();
                    out.push_str(&format!("{pad}{}\n", cells.join("\t")));
                }
                out.push('\n');
            }
            Block::Rule => out.push_str(&format!("{pad}----------\n\n")),
        }
    }
}
