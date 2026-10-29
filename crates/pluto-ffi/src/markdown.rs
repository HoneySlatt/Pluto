//! Assistant messages as blocks Compose lays out (the Dioxus app's
//! `markdown.rs` rendered the same parse as HTML): paragraphs, headings,
//! list items with their marker and depth, quotes, code, rules and tables,
//! each with styled spans. The text comes from an agent, so any HTML in it
//! is shown as text, and only `http(s)` and `mailto` links stay links.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum BlockKind {
    Paragraph,
    Heading { level: u8 },
    Code { language: String, text: String },
    Rule,
    Table { header: Vec<Vec<Span>>, rows: Vec<Vec<Vec<Span>>> },
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Block {
    pub kind: BlockKind,
    pub spans: Vec<Span>,
    /// List depth (0 outside lists).
    pub indent: u8,
    /// The item's marker on its first block: `•`, `3.`, `☐` or `☑`.
    pub marker: Option<String>,
    /// Blockquote depth.
    pub quote: u8,
}

#[derive(Default)]
struct Style {
    bold: u8,
    italic: u8,
    strike: u8,
    link: Vec<Option<String>>,
}

struct Builder {
    blocks: Vec<Block>,
    spans: Vec<Span>,
    style: Style,
    /// Open lists: the next number of an ordered one.
    lists: Vec<Option<u64>>,
    marker: Option<String>,
    quote: u8,
    heading: Option<u8>,
    code: Option<(String, String)>,
    table: Option<(Vec<Vec<Span>>, Vec<Vec<Vec<Span>>>, Vec<Vec<Span>>, bool)>,
}

impl Builder {
    fn push_text(&mut self, text: &str, code: bool) {
        if let Some((_, body)) = &mut self.code {
            body.push_str(text);
            return;
        }
        let span = Span {
            text: text.to_owned(),
            bold: self.style.bold > 0,
            italic: self.style.italic > 0,
            strike: self.style.strike > 0,
            code,
            link: self.style.link.last().cloned().flatten(),
        };
        // Runs of one style join.
        let same = |last: &Span| {
            (last.bold, last.italic, last.strike, last.code, &last.link)
                == (span.bold, span.italic, span.strike, span.code, &span.link)
        };
        if let Some(last) = self.spans.last_mut()
            && same(last)
        {
            last.text.push_str(text);
            return;
        }
        self.spans.push(span);
    }

    /// Ends the text gathered so far as a block of `kind`.
    fn flush(&mut self, kind: BlockKind) {
        if self.spans.is_empty() && matches!(kind, BlockKind::Paragraph) {
            return;
        }
        let spans = std::mem::take(&mut self.spans);
        self.blocks.push(Block {
            kind,
            spans,
            indent: self.lists.len() as u8,
            marker: self.marker.take(),
            quote: self.quote,
        });
    }
}

pub fn blocks(markdown: &str) -> Vec<Block> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    let mut b = Builder {
        blocks: Vec::new(),
        spans: Vec::new(),
        style: Style::default(),
        lists: Vec::new(),
        marker: None,
        quote: 0,
        heading: None,
        code: None,
        table: None,
    };
    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {}
                Tag::Heading { level, .. } => b.heading = Some(level as u8),
                Tag::BlockQuote(_) => {
                    b.flush(BlockKind::Paragraph);
                    b.quote += 1;
                }
                Tag::CodeBlock(kind) => {
                    b.flush(BlockKind::Paragraph);
                    let language = match kind {
                        CodeBlockKind::Fenced(info) => {
                            info.split_whitespace().next().unwrap_or_default().to_owned()
                        }
                        CodeBlockKind::Indented => String::new(),
                    };
                    b.code = Some((language, String::new()));
                }
                Tag::List(start) => {
                    b.flush(BlockKind::Paragraph);
                    b.lists.push(start);
                }
                Tag::Item => {
                    b.flush(BlockKind::Paragraph);
                    b.marker = Some(match b.lists.last_mut() {
                        Some(Some(n)) => {
                            let marker = format!("{n}.");
                            *n += 1;
                            marker
                        }
                        _ => "•".into(),
                    });
                }
                Tag::Emphasis => b.style.italic += 1,
                Tag::Strong => b.style.bold += 1,
                Tag::Strikethrough => b.style.strike += 1,
                Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                    b.style.link.push(safe_url(&dest_url));
                }
                Tag::Table(_) => {
                    b.flush(BlockKind::Paragraph);
                    b.table = Some((Vec::new(), Vec::new(), Vec::new(), true));
                }
                Tag::TableHead => {
                    if let Some(table) = &mut b.table {
                        table.3 = true;
                    }
                }
                Tag::TableRow => {}
                Tag::TableCell => b.spans.clear(),
                _ => {}
            },
            Event::End(end) => match end {
                TagEnd::Paragraph => b.flush(BlockKind::Paragraph),
                TagEnd::Heading(_) => {
                    let level = b.heading.take().unwrap_or(3);
                    b.flush(BlockKind::Heading { level });
                }
                TagEnd::BlockQuote(_) => {
                    b.flush(BlockKind::Paragraph);
                    b.quote = b.quote.saturating_sub(1);
                }
                TagEnd::CodeBlock => {
                    if let Some((language, mut text)) = b.code.take() {
                        if text.ends_with('\n') {
                            text.pop();
                        }
                        b.flush(BlockKind::Code { language, text });
                    }
                }
                TagEnd::List(_) => {
                    b.flush(BlockKind::Paragraph);
                    b.lists.pop();
                }
                TagEnd::Item => b.flush(BlockKind::Paragraph),
                TagEnd::Emphasis => b.style.italic = b.style.italic.saturating_sub(1),
                TagEnd::Strong => b.style.bold = b.style.bold.saturating_sub(1),
                TagEnd::Strikethrough => b.style.strike = b.style.strike.saturating_sub(1),
                TagEnd::Link | TagEnd::Image => {
                    b.style.link.pop();
                }
                TagEnd::TableCell => {
                    let cell = std::mem::take(&mut b.spans);
                    if let Some(table) = &mut b.table {
                        table.2.push(cell);
                    }
                }
                TagEnd::TableHead => {
                    if let Some(table) = &mut b.table {
                        table.0 = std::mem::take(&mut table.2);
                        table.3 = false;
                    }
                }
                TagEnd::TableRow => {
                    if let Some(table) = &mut b.table {
                        let row = std::mem::take(&mut table.2);
                        if table.3 {
                            table.0 = row;
                        } else {
                            table.1.push(row);
                        }
                    }
                }
                TagEnd::Table => {
                    if let Some((header, rows, _, _)) = b.table.take() {
                        b.spans.clear();
                        b.blocks.push(Block {
                            kind: BlockKind::Table { header, rows },
                            spans: Vec::new(),
                            indent: b.lists.len() as u8,
                            marker: b.marker.take(),
                            quote: b.quote,
                        });
                    }
                }
                _ => {}
            },
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                b.push_text(&text, false)
            }
            Event::Code(text) => b.push_text(&text, true),
            Event::InlineMath(text) | Event::DisplayMath(text) => b.push_text(&text, true),
            Event::SoftBreak => b.push_text(" ", false),
            Event::HardBreak => b.push_text("\n", false),
            Event::Rule => {
                b.flush(BlockKind::Paragraph);
                b.flush(BlockKind::Rule);
            }
            Event::TaskListMarker(done) => {
                b.marker = Some(if done { "☑" } else { "☐" }.into());
            }
            Event::FootnoteReference(name) => b.push_text(&format!("[{name}]"), false),
        }
    }
    b.flush(BlockKind::Paragraph);
    b.blocks
}

fn safe_url(url: &str) -> Option<String> {
    let lower = url.trim().to_ascii_lowercase();
    ["https://", "http://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
        .then(|| url.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_html_stays_text() {
        let out = blocks("Hi <script>alert(1)</script> [x](javascript:alert(1)) [a](https://x.dev)");
        assert_eq!(out.len(), 1);
        let text: String = out[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert!(text.contains("<script>"), "{text}");
        assert!(out[0].spans.iter().all(|s| s.link.as_deref() != Some("javascript:alert(1)")));
        assert!(out[0].spans.iter().any(|s| s.link.as_deref() == Some("https://x.dev")));
    }

    #[test]
    fn lists_quotes_code_and_tables() {
        let out = blocks(
            "# Title\n\n- one **bold**\n- two\n  1. nested\n\n> quoted\n\n```rust\nfn x() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
        );
        assert_eq!(out[0].kind, BlockKind::Heading { level: 1 });
        assert_eq!(out[1].marker.as_deref(), Some("•"));
        assert!(out[1].spans.iter().any(|s| s.bold && s.text == "bold"));
        assert_eq!((out[3].indent, out[3].marker.as_deref()), (2, Some("1.")));
        assert_eq!(out[4].quote, 1);
        assert_eq!(
            out[5].kind,
            BlockKind::Code {
                language: "rust".into(),
                text: "fn x() {}".into()
            }
        );
        let BlockKind::Table { header, rows } = &out[6].kind else {
            panic!("{:?}", out[6]);
        };
        assert_eq!((header.len(), rows.len()), (2, 1));
        assert_eq!(rows[0][1][0].text, "2");
    }
}
