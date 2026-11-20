use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use super::{tidy, Block, Doc, Inline};

/// One open container while we walk the Markdown events.
enum Frame {
    Root(Vec<Block>),
    List { ordered: bool, items: Vec<Vec<Block>> },
    Item(Vec<Block>),
    Quote(Vec<Block>),
}

impl Frame {
    fn blocks(&mut self) -> &mut Vec<Block> {
        match self {
            Frame::Root(b) | Frame::Item(b) | Frame::Quote(b) => b,
            Frame::List { .. } => unreachable!("lists hold items, not blocks"),
        }
    }
}

pub fn read(text: &str) -> Doc {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let mut stack: Vec<Frame> = vec![Frame::Root(Vec::new())];
    let mut inlines: Vec<Inline> = Vec::new();
    let (mut bold, mut italic) = (0u32, 0u32);
    let mut link: Option<String> = None;
    let mut code_block: Option<String> = None;
    let mut table: Option<(Vec<Vec<Vec<Inline>>>, bool)> = None;
    let mut in_table_head = false;

    fn flush(stack: &mut [Frame], inlines: &mut Vec<Inline>) {
        let v = tidy(std::mem::take(inlines));
        if !v.is_empty() {
            stack.last_mut().unwrap().blocks().push(Block::Para(v));
        }
    }

    for ev in Parser::new_ext(text, opts) {
        match ev {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {}
                Tag::Heading { .. } => flush(&mut stack, &mut inlines),
                Tag::BlockQuote(_) => {
                    flush(&mut stack, &mut inlines);
                    stack.push(Frame::Quote(Vec::new()));
                }
                Tag::CodeBlock(_) => code_block = Some(String::new()),
                Tag::List(start) => {
                    flush(&mut stack, &mut inlines);
                    stack.push(Frame::List { ordered: start.is_some(), items: Vec::new() });
                }
                Tag::Item => stack.push(Frame::Item(Vec::new())),
                Tag::Emphasis => italic += 1,
                Tag::Strong => bold += 1,
                Tag::Link { dest_url, .. } => link = Some(dest_url.to_string()),
                Tag::Table(_) => table = Some((Vec::new(), true)),
                Tag::TableHead => {
                    in_table_head = true;
                    if let Some((rows, _)) = table.as_mut() {
                        rows.push(Vec::new());
                    }
                }
                Tag::TableRow => {
                    if let Some((rows, _)) = table.as_mut() {
                        rows.push(Vec::new());
                    }
                }
                Tag::TableCell => inlines.clear(),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => flush(&mut stack, &mut inlines),
                TagEnd::Heading(level) => {
                    let v = tidy(std::mem::take(&mut inlines));
                    let n = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                    stack.last_mut().unwrap().blocks().push(Block::Heading(n, v));
                }
                TagEnd::BlockQuote(_) => {
                    flush(&mut stack, &mut inlines);
                    if let Some(Frame::Quote(b)) = stack.pop() {
                        stack.last_mut().unwrap().blocks().push(Block::Quote(b));
                    }
                }
                TagEnd::CodeBlock => {
                    let code = code_block.take().unwrap_or_default();
                    stack.last_mut().unwrap().blocks().push(Block::Code(code.trim_end_matches('\n').to_owned()));
                }
                TagEnd::Item => {
                    flush(&mut stack, &mut inlines);
                    if let Some(Frame::Item(b)) = stack.pop() {
                        if let Some(Frame::List { items, .. }) = stack.last_mut() {
                            items.push(b);
                        }
                    }
                }
                TagEnd::List(_) => {
                    if let Some(Frame::List { ordered, items }) = stack.pop() {
                        stack.last_mut().unwrap().blocks().push(Block::List { ordered, items });
                    }
                }
                TagEnd::Emphasis => italic = italic.saturating_sub(1),
                TagEnd::Strong => bold = bold.saturating_sub(1),
                TagEnd::Link => link = None,
                TagEnd::TableCell => {
                    let v = tidy(std::mem::take(&mut inlines));
                    if let Some((rows, _)) = table.as_mut() {
                        if let Some(r) = rows.last_mut() {
                            r.push(v);
                        }
                    }
                }
                TagEnd::TableHead => in_table_head = false,
                TagEnd::Table => {
                    if let Some((rows, header)) = table.take() {
                        stack.last_mut().unwrap().blocks().push(Block::Table { header, rows });
                    }
                }
                _ => {}
            },
            Event::Text(t) => {
                if let Some(c) = code_block.as_mut() {
                    c.push_str(&t);
                } else {
                    inlines.push(Inline { text: t.to_string(), bold: bold > 0, italic: italic > 0, code: false, link: link.clone() });
                }
            }
            Event::Code(t) => inlines.push(Inline { text: t.to_string(), bold: bold > 0, italic: italic > 0, code: true, link: link.clone() }),
            Event::SoftBreak => inlines.push(Inline { text: " ".into(), ..Default::default() }),
            Event::HardBreak => inlines.push(Inline { text: "\n".into(), ..Default::default() }),
            Event::Rule => {
                flush(&mut stack, &mut inlines);
                stack.last_mut().unwrap().blocks().push(Block::Rule);
            }
            Event::Html(h) | Event::InlineHtml(h) => {
                if h.trim().eq_ignore_ascii_case("<br>") || h.trim().eq_ignore_ascii_case("<br/>") {
                    inlines.push(Inline::plain("\n"));
                }
            }
            _ => {}
        }
    }
    let _ = in_table_head;
    flush(&mut stack, &mut inlines);
    let blocks = match stack.into_iter().next() {
        Some(Frame::Root(b)) => b,
        _ => Vec::new(),
    };
    Doc { title: None, blocks }
}
