use super::xml::esc;
use super::{Block, Doc, Inline};

pub fn inlines(v: &[Inline]) -> String {
    let mut out = String::new();
    for i in v {
        let mut t = esc(&i.text).replace('\n', "<br>\n");
        if i.code {
            t = format!("<code>{t}</code>");
        }
        if i.italic {
            t = format!("<em>{t}</em>");
        }
        if i.bold {
            t = format!("<strong>{t}</strong>");
        }
        if let Some(url) = &i.link {
            t = format!("<a href=\"{}\">{t}</a>", esc(url));
        }
        out.push_str(&t);
    }
    out
}

fn blocks(bs: &[Block], out: &mut String) {
    for b in bs {
        match b {
            Block::Heading(l, i) => out.push_str(&format!("<h{l}>{}</h{l}>\n", inlines(i))),
            Block::Para(i) => out.push_str(&format!("<p>{}</p>\n", inlines(i))),
            Block::List { ordered, items } => {
                let tag = if *ordered { "ol" } else { "ul" };
                out.push_str(&format!("<{tag}>\n"));
                for item in items {
                    // A list item with one paragraph needs no <p>.
                    if let [Block::Para(i)] = item.as_slice() {
                        out.push_str(&format!("<li>{}</li>\n", inlines(i)));
                    } else {
                        out.push_str("<li>");
                        blocks(item, out);
                        out.push_str("</li>\n");
                    }
                }
                out.push_str(&format!("</{tag}>\n"));
            }
            Block::Quote(inner) => {
                out.push_str("<blockquote>\n");
                blocks(inner, out);
                out.push_str("</blockquote>\n");
            }
            Block::Code(c) => out.push_str(&format!("<pre><code>{}</code></pre>\n", esc(c))),
            Block::Table { header, rows } => {
                out.push_str("<table>\n");
                for (n, r) in rows.iter().enumerate() {
                    let cell = if *header && n == 0 { "th" } else { "td" };
                    out.push_str("<tr>");
                    for c in r {
                        out.push_str(&format!("<{cell}>{}</{cell}>", inlines(c)));
                    }
                    out.push_str("</tr>\n");
                }
                out.push_str("</table>\n");
            }
            Block::Rule => out.push_str("<hr>\n"),
        }
    }
}

pub fn write(doc: &Doc) -> String {
    let title = esc(doc.title.as_deref().unwrap_or("Document"));
    let mut body = String::new();
    blocks(&doc.blocks, &mut body);
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
body {{ font-family: system-ui, sans-serif; line-height: 1.5; max-width: 46em; margin: 2em auto; padding: 0 1em; }}
pre {{ background: #f4f4f4; padding: .75em; overflow-x: auto; }}
code {{ font-family: ui-monospace, monospace; }}
blockquote {{ border-left: 4px solid #ccc; margin-left: 0; padding-left: 1em; color: #555; }}
table {{ border-collapse: collapse; }}
td, th {{ border: 1px solid #ccc; padding: .3em .6em; text-align: left; }}
</style>
</head>
<body>
{body}</body>
</html>
"#
    )
}
