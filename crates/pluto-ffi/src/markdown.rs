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
    Heading {
        level: u8,
    },
    /// A code block, its tokens colored (`.k`, `.s`, `.f`, `.c`).
    Code {
        language: String,
        text: String,
        tokens: Vec<CodeToken>,
    },
    Rule,
    /// An image (`![alt](src)`), on its own row; the core loads it
    /// (`Core::markdown_image`), by itself only when `loads_by_itself`
    /// (ADR-0022), otherwise on a tap.
    Image {
        src: String,
        alt: String,
        loads_by_itself: bool,
    },
    Table {
        header: Vec<Vec<Span>>,
        rows: Vec<Vec<Vec<Span>>>,
    },
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
    table: Option<Table>,
    /// An image being read: its source and alt text.
    image: Option<(String, String)>,
}

/// A table being read: its header, its rows, the row being read, and
/// whether that row is the header's.
type Table = (Vec<Vec<Span>>, Vec<Vec<Vec<Span>>>, Vec<Vec<Span>>, bool);

impl Builder {
    fn push_text(&mut self, text: &str, code: bool) {
        if let Some((_, alt)) = &mut self.image {
            alt.push_str(text);
            return;
        }
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
            indent: self.lists.len().min(255) as u8,
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
        image: None,
    };
    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {}
                Tag::Heading { level, .. } => b.heading = Some(level as u8),
                Tag::BlockQuote(_) => {
                    b.flush(BlockKind::Paragraph);
                    b.quote = b.quote.saturating_add(1);
                }
                Tag::CodeBlock(kind) => {
                    b.flush(BlockKind::Paragraph);
                    let language = match kind {
                        CodeBlockKind::Fenced(info) => info
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_owned(),
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
                Tag::Emphasis => b.style.italic = b.style.italic.saturating_add(1),
                Tag::Strong => b.style.bold = b.style.bold.saturating_add(1),
                Tag::Strikethrough => b.style.strike = b.style.strike.saturating_add(1),
                Tag::Link { dest_url, .. } => {
                    b.style.link.push(safe_url(&dest_url));
                }
                // The text before it stays its paragraph; the image gets
                // its own row.
                Tag::Image { dest_url, .. } => {
                    b.flush(BlockKind::Paragraph);
                    b.image = Some((dest_url.to_string(), String::new()));
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
                        let tokens = highlight(&language, &text);
                        b.flush(BlockKind::Code {
                            language,
                            text,
                            tokens,
                        });
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
                TagEnd::Link => {
                    b.style.link.pop();
                }
                TagEnd::Image => {
                    if let Some((src, alt)) = b.image.take() {
                        b.blocks.push(Block {
                            kind: BlockKind::Image {
                                src,
                                alt,
                                loads_by_itself: true,
                            },
                            spans: Vec::new(),
                            indent: b.lists.len().min(255) as u8,
                            marker: b.marker.take(),
                            quote: b.quote,
                        });
                    }
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
                            indent: b.lists.len().min(255) as u8,
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

/// A colored run of a code block, in UTF-16 units (Kotlin's indices).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CodeToken {
    pub start: u32,
    pub end: u32,
    pub kind: TokenKind,
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum TokenKind {
    Keyword,
    String,
    Function,
    Comment,
}

/// Words that read as keywords across the languages agents write.
const KEYWORDS: &[&str] = &[
    "abstract",
    "and",
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "chan",
    "class",
    "const",
    "continue",
    "crate",
    "def",
    "default",
    "defer",
    "del",
    "do",
    "done",
    "dyn",
    "elif",
    "else",
    "enum",
    "esac",
    "except",
    "export",
    "extends",
    "extern",
    "false",
    "fi",
    "final",
    "finally",
    "fn",
    "for",
    "from",
    "fun",
    "func",
    "function",
    "go",
    "if",
    "impl",
    "implements",
    "import",
    "in",
    "inherit",
    "interface",
    "is",
    "lambda",
    "let",
    "local",
    "loop",
    "match",
    "mod",
    "module",
    "move",
    "mut",
    "new",
    "nil",
    "None",
    "not",
    "null",
    "object",
    "of",
    "or",
    "override",
    "package",
    "pass",
    "private",
    "protected",
    "pub",
    "public",
    "raise",
    "rec",
    "ref",
    "return",
    "select",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "switch",
    "then",
    "this",
    "throw",
    "throws",
    "trait",
    "true",
    "True",
    "False",
    "try",
    "type",
    "typeof",
    "undefined",
    "unsafe",
    "use",
    "val",
    "var",
    "void",
    "when",
    "where",
    "while",
    "with",
    "yield",
];

/// The comment markers of `language`: a line one, and whether `/* */`
/// comments exist.
fn comments(language: &str) -> (&'static [&'static str], bool) {
    match language.to_ascii_lowercase().as_str() {
        "python" | "py" | "sh" | "bash" | "zsh" | "shell" | "console" | "ruby" | "rb" | "yaml"
        | "yml" | "toml" | "perl" | "r" | "make" | "makefile" | "dockerfile" | "conf" | "ini"
        | "fish" | "elixir" | "ex" | "nim" | "julia" => (&["#"], false),
        "nix" => (&["#"], true),
        "sql" | "lua" | "haskell" | "hs" | "elm" => (&["--"], false),
        "" | "text" | "txt" | "plain" | "plaintext" | "markdown" | "md" | "diff" | "patch"
        | "json" => (&[], false),
        _ => (&["//"], true),
    }
}

/// Colors a code block lightly, like the spec's `.k`, `.s`, `.f` and `.c`:
/// comments, strings, keywords, and names called. Plain text and diffs
/// stay plain.
pub fn highlight(language: &str, text: &str) -> Vec<CodeToken> {
    let lower = language.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "" | "text" | "txt" | "plain" | "plaintext" | "diff" | "patch" | "markdown" | "md"
    ) {
        return Vec::new();
    }
    let (line_comments, block_comments) = comments(&lower);
    let rust = matches!(lower.as_str(), "rust" | "rs");
    let backticks = matches!(
        lower.as_str(),
        "js" | "javascript"
            | "ts"
            | "typescript"
            | "tsx"
            | "jsx"
            | "sh"
            | "bash"
            | "zsh"
            | "shell"
            | "go"
    );
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    // UTF-16 offset of each char index.
    let mut utf16 = Vec::with_capacity(chars.len() + 1);
    let mut at = 0u32;
    for (_, c) in &chars {
        utf16.push(at);
        at += c.len_utf16() as u32;
    }
    utf16.push(at);
    let starts_with = |i: usize, pattern: &str| text[chars[i].0..].starts_with(pattern);
    let mut tokens = Vec::new();
    let mut push = |from: usize, to: usize, kind: TokenKind| {
        if to > from {
            tokens.push(CodeToken {
                start: utf16[from],
                end: utf16[to],
                kind,
            });
        }
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if line_comments.iter().any(|m| starts_with(i, m)) {
            let end = (i..chars.len())
                .find(|&j| chars[j].1 == '\n')
                .unwrap_or(chars.len());
            push(i, end, TokenKind::Comment);
            i = end;
            continue;
        }
        if block_comments && starts_with(i, "/*") {
            let end = text[chars[i].0 + 2..]
                .find("*/")
                .map(|at| chars.partition_point(|(byte, _)| *byte < chars[i].0 + 2 + at + 2))
                .unwrap_or(chars.len());
            push(i, end, TokenKind::Comment);
            i = end;
            continue;
        }
        let quote = c == '"'
            || (c == '\'' && !(rust && !char_literal(&chars, i)))
            || (c == '`' && backticks);
        if quote {
            let mut j = i + 1;
            while j < chars.len() && chars[j].1 != c {
                if chars[j].1 == '\\' {
                    j += 1;
                }
                // A string does not run past its line, except backticks.
                if chars.get(j).is_some_and(|(_, d)| *d == '\n') && c != '`' {
                    break;
                }
                j += 1;
            }
            let end = (j + 1).min(chars.len());
            push(i, end, TokenKind::String);
            i = end;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < chars.len() && (chars[j].1.is_alphanumeric() || chars[j].1 == '_') {
                j += 1;
            }
            let word = &text[chars[i].0..chars.get(j).map_or(text.len(), |(b, _)| *b)];
            // `name(` or Rust's `name!(`.
            let mut k = j;
            if rust && chars.get(k).is_some_and(|(_, d)| *d == '!') {
                k += 1;
            }
            if KEYWORDS.contains(&word) {
                push(i, j, TokenKind::Keyword);
            } else if chars.get(k).is_some_and(|(_, d)| *d == '(') {
                push(i, j, TokenKind::Function);
            }
            i = j;
            continue;
        }
        i += 1;
    }
    tokens
}

/// Whether the quote at `i` opens a Rust char literal (`'a'`, `'\n'`),
/// not a lifetime (`'a`).
fn char_literal(chars: &[(usize, char)], i: usize) -> bool {
    matches!(
        (chars.get(i + 1), chars.get(i + 2)),
        (Some((_, '\\')), _) | (Some(_), Some((_, '\'')))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_colors_comments_strings_keywords_and_calls() {
        let kinds = |language: &str, text: &str| -> Vec<(String, TokenKind)> {
            let units: Vec<u16> = text.encode_utf16().collect();
            highlight(language, text)
                .into_iter()
                .map(|t| {
                    (
                        String::from_utf16_lossy(&units[t.start as usize..t.end as usize]),
                        t.kind,
                    )
                })
                .collect()
        };
        assert_eq!(
            kinds("rust", "let é = f('x'); // done\nfn g(s: &'a str) {}"),
            vec![
                ("let".into(), TokenKind::Keyword),
                ("f".into(), TokenKind::Function),
                ("'x'".into(), TokenKind::String),
                ("// done".into(), TokenKind::Comment),
                ("fn".into(), TokenKind::Keyword),
                ("g".into(), TokenKind::Function),
            ]
        );
        assert_eq!(
            kinds("python", "def f(): # \"no\"\n    return \"a\\\"b\""),
            vec![
                ("def".into(), TokenKind::Keyword),
                ("f".into(), TokenKind::Function),
                ("# \"no\"".into(), TokenKind::Comment),
                ("return".into(), TokenKind::Keyword),
                ("\"a\\\"b\"".into(), TokenKind::String),
            ]
        );
        assert!(highlight("diff", "+ fn x()").is_empty());
    }

    #[test]
    fn agent_html_stays_text() {
        let out =
            blocks("Hi <script>alert(1)</script> [x](javascript:alert(1)) [a](https://x.dev)");
        assert_eq!(out.len(), 1);
        let text: String = out[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert!(text.contains("<script>"), "{text}");
        assert!(
            out[0]
                .spans
                .iter()
                .all(|s| s.link.as_deref() != Some("javascript:alert(1)"))
        );
        assert!(
            out[0]
                .spans
                .iter()
                .any(|s| s.link.as_deref() == Some("https://x.dev"))
        );
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
                text: "fn x() {}".into(),
                tokens: vec![
                    CodeToken {
                        start: 0,
                        end: 2,
                        kind: TokenKind::Keyword
                    },
                    CodeToken {
                        start: 3,
                        end: 4,
                        kind: TokenKind::Function
                    },
                ],
            }
        );
        let BlockKind::Table { header, rows } = &out[6].kind else {
            panic!("{:?}", out[6]);
        };
        assert_eq!((header.len(), rows.len()), (2, 1));
        assert_eq!(rows[0][1][0].text, "2");
    }
}
