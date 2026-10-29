//! Assistant markdown (`ChatMarkdown.tsx` + `.chat-markdown` CSS): block
//! margins collapse like CSS, inline code gets its padded chip, fenced code
//! uses the pierre-dark highlighter, tables use the 12px compact layout.
//! File references become file chips and external links lead with their
//! site's icon (`md_links`). GitHub alerts (`> [!NOTE]`), footnotes and
//! bare URLs follow remark-gfm and `markdownGithubAlerts.ts`; answers keep
//! the sanitized HTML of `md_html` (`<details>` as a disclosure), user
//! messages show HTML as typed.
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, Galley, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
    text::LayoutJob,
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::highlight::Highlighter;
use crate::theme::{self, Weight, font};
use crate::ui::citations::{self, Citation};
use crate::ui::md_copy;
use crate::ui::md_links::{self, FileRef};
use crate::ui::widgets;

const BODY: f32 = 14.0;
const BODY_LINE: f32 = 22.75; // 14px × 1.625
const BLOCK_MARGIN: f32 = 10.4; // .65rem
const INFO_FOREGROUND: Color32 = Color32::from_rgb(81, 162, 255);

#[derive(Clone, Debug, Default)]
pub(crate) struct Style {
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) strike: bool,
    pub(crate) link: Option<String>,
    /// Inside a `<script>` or `<style>`, whose text the sanitizer drops.
    pub(crate) hidden: bool,
    /// A footnote mark: smaller, semibold and raised (`sup`).
    pub(crate) superscript: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum Inline {
    Text(String, Style),
    Code(String),
    /// A file reference, drawn as a chip.
    File(FileRef),
    /// An assistant citation link, drawn as its chip.
    Citation(Citation),
    /// A context chip of a kind (a terminal excerpt, a skill…) and its label.
    Context {
        kind: String,
        label: String,
        id: String,
    },
    Break,
}

/// Inline code that names a file becomes its chip
/// (`resolveInlineCodeFileLinkMeta`).
fn code_inline(code: &str) -> Inline {
    md_links::inline_code_candidate(code)
        .and_then(|candidate| md_links::parse_file_link(&candidate))
        .map_or_else(|| Inline::Code(code.to_owned()), Inline::File)
}

/// A link drawn as a chip: an assistant citation, a context reference
/// (labelled by the link's text from `events`) or a file reference.
fn link_chip(href: &str, events: &[Event]) -> Option<Inline> {
    // A link to another thread (`t3-thread://v1/<id>`): its chip opens it.
    // Pluto draws it as a thread chip, where upstream draws a link with
    // the project's favicon.
    if let Some(id) = href
        .strip_prefix("t3-thread://v1/")
        .filter(|id| !id.is_empty())
    {
        let mut label = String::new();
        for event in events {
            match event {
                Event::End(TagEnd::Link) => break,
                Event::Text(t) | Event::Code(t) => label.push_str(t),
                _ => {}
            }
        }
        let id = id.to_owned();
        if label.trim().is_empty() {
            label = id.clone();
        }
        return Some(Inline::Context {
            kind: "thread".into(),
            label: crate::ui::composer_context::sanitize_label(&label, "thread"),
            id,
        });
    }
    if let Some(rest) = href.strip_prefix("t3-context://v1/")
        && let Some((kind, id)) = rest.split_once('/')
    {
        let mut label = String::new();
        for event in events {
            match event {
                Event::End(TagEnd::Link) => break,
                Event::Text(t) | Event::Code(t) => label.push_str(t),
                _ => {}
            }
        }
        return Some(Inline::Context {
            kind: kind.to_owned(),
            label: crate::ui::composer_context::sanitize_label(&label, kind),
            id: id.to_owned(),
        });
    }
    citations::parse_href(href)
        .map(Inline::Citation)
        .or_else(|| md_links::parse_file_link(href).map(Inline::File))
}

/// A chip of the inline layout and what a click on it opens.
#[derive(Clone, Debug)]
pub(crate) enum ChipRef {
    File(FileRef),
    Citation(Citation),
    Context {
        kind: String,
        label: String,
        id: String,
    },
}

/// Skips a link's content up to and past its end.
fn skip_link(events: &[Event], index: &mut usize) {
    let mut depth = 0;
    while *index < events.len() {
        let event = &events[*index];
        *index += 1;
        match event {
            Event::Start(Tag::Link { .. }) => depth += 1,
            Event::End(TagEnd::Link) if depth == 0 => break,
            Event::End(TagEnd::Link) => depth -= 1,
            _ => {}
        }
    }
}

/// Every file reference's path, for the chips' disambiguating suffixes.
fn collect_files(blocks: &[Block], out: &mut Vec<String>) {
    let inlines = |inlines: &[Inline], out: &mut Vec<String>| {
        for inline in inlines {
            if let Inline::File(file) = inline {
                out.push(file.path.clone());
            }
        }
    };
    for block in blocks {
        match block {
            Block::Paragraph(i) | Block::Heading(_, i) => inlines(i, out),
            Block::List { items, .. } => items.iter().for_each(|item| collect_files(item, out)),
            Block::Quote(blocks) | Block::Alert(_, blocks) => collect_files(blocks, out),
            Block::Details {
                summary, blocks, ..
            } => {
                inlines(summary, out);
                collect_files(blocks, out);
            }
            Block::Table { head, rows, .. } => {
                head.iter().for_each(|cell| inlines(cell, out));
                rows.iter().flatten().for_each(|cell| inlines(cell, out));
            }
            Block::Code { .. } | Block::Rule | Block::Image { .. } => {}
        }
    }
}

/// A message's blocks and its file chips' suffixes (`Markdown::parsed`).
type Parsed = (Arc<Vec<Block>>, Arc<HashMap<String, String>>);

#[derive(Clone, Debug)]
enum Block {
    Paragraph(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    List {
        start: Option<u64>,
        items: Vec<Vec<Block>>,
    },
    Code {
        language: String,
        code: String,
        /// The info string after the language (`title="…"`, a file name).
        meta: String,
    },
    Table {
        alignments: Vec<Alignment>,
        head: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Quote(Vec<Block>),
    /// A GitHub alert (`> [!NOTE]`): its kind's title over ordinary text.
    Alert(&'static str, Vec<Block>),
    /// `<details>` (`MarkdownDetails`): a disclosure under its summary.
    Details {
        summary: Vec<Inline>,
        open: bool,
        blocks: Vec<Block>,
    },
    Rule,
    /// An image alone in its paragraph (`markStandaloneImages`); images
    /// within a line of text keep their alt text.
    Image {
        src: String,
        alt: String,
    },
}

/// With `line_breaks`, every line break is a break (`remark-breaks`, the
/// `lineBreaks` of user messages).
fn parse_with(text: &str, line_breaks: bool) -> Vec<Block> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_FOOTNOTES);
    let events: Vec<Event> = Parser::new_ext(text, options)
        .map(|event| match event {
            Event::SoftBreak if line_breaks => Event::HardBreak,
            event => event,
        })
        .collect();
    // User messages show HTML as typed; answers parse the sanitized tags.
    RAW_HTML.set(!line_breaks);
    FOOTNOTES.with_borrow_mut(Footnotes::clear);
    let mut index = 0;
    let mut blocks = parse_blocks(&events, &mut index, None);
    // `remark-gfm`'s footnotes section: a rule, then the definitions in the
    // order of their first reference.
    let mut definitions = FOOTNOTES.with_borrow_mut(Footnotes::take);
    if !definitions.is_empty() {
        definitions.sort_by_key(|(number, _)| *number);
        blocks.push(Block::Rule);
        blocks.push(Block::List {
            start: Some(1),
            items: definitions.into_iter().map(|(_, blocks)| blocks).collect(),
        });
    }
    autolink_blocks(&mut blocks);
    blocks
}

thread_local! {
    /// Whether the text being parsed keeps its raw HTML's meaning.
    static RAW_HTML: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    static FOOTNOTES: std::cell::RefCell<Footnotes> = std::cell::RefCell::new(Footnotes::default());
}

/// Footnote numbers by label (first reference first), and the definitions.
#[derive(Default)]
struct Footnotes {
    numbers: Vec<String>,
    definitions: Vec<(String, Vec<Block>)>,
    /// The labels some text refers to: only their definitions show.
    referenced: Vec<String>,
}

impl Footnotes {
    fn clear(&mut self) {
        *self = Footnotes::default();
    }

    fn number(&mut self, label: &str) -> usize {
        match self.numbers.iter().position(|l| l == label) {
            Some(index) => index + 1,
            None => {
                self.numbers.push(label.to_owned());
                self.numbers.len()
            }
        }
    }

    fn take(&mut self) -> Vec<(usize, Vec<Block>)> {
        let definitions = std::mem::take(&mut self.definitions);
        let referenced = std::mem::take(&mut self.referenced);
        definitions
            .into_iter()
            .filter(|(label, _)| referenced.contains(label))
            .map(|(label, blocks)| (self.number(&label), blocks))
            .collect()
    }
}

/// A footnote reference: its number raised, link-colored (`sup > a`).
fn footnote_reference(label: &str) -> Inline {
    let number = FOOTNOTES.with_borrow_mut(|f| f.number(label));
    FOOTNOTES.with_borrow_mut(|f| f.referenced.push(label.to_owned()));
    Inline::Text(
        number.to_string(),
        Style {
            link: Some(format!("#user-content-fn-{label}")),
            superscript: true,
            ..Style::default()
        },
    )
}

/// `GITHUB_ALERT_PRESENTATIONS`' labels by kind.
fn alert_kind(kind: pulldown_cmark::BlockQuoteKind) -> &'static str {
    use pulldown_cmark::BlockQuoteKind;
    match kind {
        BlockQuoteKind::Note => "note",
        BlockQuoteKind::Tip => "tip",
        BlockQuoteKind::Important => "important",
        BlockQuoteKind::Warning => "warning",
        BlockQuoteKind::Caution => "caution",
    }
}

/// An HTML tag met among inline events: text and links keep going, the
/// allowed tags set the style; code and kbd gather their text.
fn inline_html(html: &str, style: &mut Style, out: &mut Vec<Inline>, code: &mut Option<String>) {
    use crate::ui::md_html::{Token, safe_href, tokens};
    if !RAW_HTML.get() {
        out.push(Inline::Text(html.to_owned(), style.clone()));
        return;
    }
    for token in tokens(html) {
        match &token {
            Token::Text(_) if style.hidden => {}
            Token::Text(text) => match code {
                Some(code) => code.push_str(text),
                None => out.push(Inline::Text(text.clone(), style.clone())),
            },
            Token::Open { name, .. } => match name.as_str() {
                "script" | "style" => style.hidden = true,
                "br" => out.push(Inline::Break),
                "b" | "strong" => style.bold = true,
                "i" | "em" => style.italic = true,
                "s" | "del" | "strike" => style.strike = true,
                "code" | "kbd" | "samp" => *code = Some(String::new()),
                "a" => {
                    if let Some(href) = token.attribute("href").filter(|h| safe_href(h)) {
                        style.link = Some(href.to_owned());
                    }
                }
                "img" => {
                    if let Some(alt) = token.attribute("alt").filter(|a| !a.is_empty()) {
                        out.push(Inline::Text(alt.to_owned(), style.clone()));
                    }
                }
                _ => {}
            },
            Token::Close(name) => match name.as_str() {
                "script" | "style" => style.hidden = false,
                "b" | "strong" => style.bold = false,
                "i" | "em" => style.italic = false,
                "s" | "del" | "strike" => style.strike = false,
                "a" => style.link = None,
                "code" | "kbd" | "samp" => {
                    if let Some(text) = code.take() {
                        out.push(Inline::Code(text));
                    }
                }
                _ => {}
            },
        }
    }
}

/// A raw HTML block that is not a disclosure: its paragraphs, headings and
/// images, the other tags dropped.
fn html_blocks(html: &str, blocks: &mut Vec<Block>) {
    use crate::ui::md_html::{Token, tokens};
    let mut inlines = Vec::new();
    let mut style = Style::default();
    let mut code = None;
    let mut heading: Option<u8> = None;
    let flush = |inlines: &mut Vec<Inline>, heading: Option<u8>, blocks: &mut Vec<Block>| {
        let empty = inlines.iter().all(|i| match i {
            Inline::Text(t, _) => t.trim().is_empty(),
            Inline::Break => true,
            _ => false,
        });
        if !empty {
            let content = std::mem::take(inlines);
            blocks.push(match heading {
                Some(level) => Block::Heading(level, content),
                None => Block::Paragraph(content),
            });
        }
        inlines.clear();
    };
    for token in tokens(html) {
        let block_tag = |name: &str| {
            matches!(
                name,
                "p" | "div"
                    | "section"
                    | "article"
                    | "center"
                    | "li"
                    | "tr"
                    | "table"
                    | "ul"
                    | "ol"
            )
        };
        match &token {
            Token::Open { name, .. } if block_tag(name) => flush(&mut inlines, heading, blocks),
            Token::Close(name) if block_tag(name) => flush(&mut inlines, heading, blocks),
            Token::Open { name, .. } if name.len() == 2 && name.starts_with('h') => {
                flush(&mut inlines, heading, blocks);
                heading = name[1..].parse().ok().filter(|l| (1..=6).contains(l));
            }
            Token::Close(name) if name.len() == 2 && name.starts_with('h') => {
                flush(&mut inlines, heading, blocks);
                heading = None;
            }
            Token::Open { name, .. } if name == "img" => {
                if let Some(src) = token.attribute("src").filter(|s| !s.is_empty()) {
                    flush(&mut inlines, heading, blocks);
                    blocks.push(Block::Image {
                        src: src.to_owned(),
                        alt: token.attribute("alt").unwrap_or_default().to_owned(),
                    });
                }
            }
            Token::Open { name, attributes } => {
                let tag = format!(
                    "<{name}{}>",
                    attributes
                        .iter()
                        .map(|(k, v)| format!(" {k}=\"{v}\""))
                        .collect::<String>()
                );
                inline_html(&tag, &mut style, &mut inlines, &mut code);
            }
            Token::Close(name) => {
                inline_html(&format!("</{name}>"), &mut style, &mut inlines, &mut code)
            }
            Token::Text(_) if style.hidden => {}
            Token::Text(text) => {
                // Source line breaks inside HTML are spaces.
                let text = text.replace('\n', " ");
                match &mut code {
                    Some(code) => code.push_str(&text),
                    None => inlines.push(Inline::Text(text, style.clone())),
                }
            }
        }
    }
    flush(&mut inlines, heading, blocks);
}

/// A `<details>` opening in `html`: its summary, whether it starts open,
/// and what follows the summary within the same block.
fn details_start(html: &str) -> Option<(Vec<Inline>, bool, String)> {
    let lower = html.to_ascii_lowercase();
    let at = lower.find("<details")?;
    let tag_end = lower[at..].find('>')? + at;
    let open = lower[at..tag_end].contains(" open");
    let rest = &html[tag_end + 1..];
    let rest_lower = rest.to_ascii_lowercase();
    let (summary, after) = match rest_lower.find("<summary") {
        Some(start) => {
            let body_start = rest_lower[start..]
                .find('>')
                .map_or(rest.len(), |e| start + e + 1);
            let body_end = rest_lower[body_start..]
                .find("</summary>")
                .map_or(rest.len(), |e| body_start + e);
            let after = (body_end + "</summary>".len()).min(rest.len());
            (&rest[body_start..body_end], &rest[after..])
        }
        None => ("", rest),
    };
    let mut inlines = Vec::new();
    let mut style = Style::default();
    let mut code = None;
    inline_html(summary, &mut style, &mut inlines, &mut code);
    if inlines.is_empty() {
        inlines.push(Inline::Text("Details".into(), Style::default()));
    }
    Some((inlines, open, after.to_owned()))
}

/// Bare URLs in text become links (remark-gfm's autolink literals): `http`,
/// `https` and `www.`, trailing punctuation left out.
fn autolink(list: &mut Vec<Inline>) {
    let mut out = Vec::with_capacity(list.len());
    for inline in list.drain(..) {
        let Inline::Text(text, style) = &inline else {
            out.push(inline);
            continue;
        };
        if style.link.is_some() || !(text.contains("http") || text.contains("www.")) {
            out.push(inline);
            continue;
        }
        let mut cursor = 0;
        let mut search = 0;
        while let Some(found) = ["https://", "http://", "www."]
            .iter()
            .filter_map(|p| text[search..].find(p).map(|i| i + search))
            .min()
        {
            // At a word's start only.
            let boundary = found == 0
                || text[..found]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_whitespace() || "(<*_~\"'".contains(c));
            let end = text[found..]
                .find(|c: char| c.is_whitespace() || c == '<')
                .map_or(text.len(), |e| found + e);
            let mut url_end = end;
            while url_end > found {
                let last = text[..url_end].chars().next_back().unwrap_or(' ');
                let unbalanced_paren = last == ')'
                    && text[found..url_end].matches('(').count()
                        < text[found..url_end].matches(')').count();
                if ".,:;!?\"'*_~".contains(last) || unbalanced_paren {
                    url_end -= last.len_utf8();
                } else {
                    break;
                }
            }
            let url = &text[found..url_end];
            let valid = boundary && url.contains('.') && url.len() > "www.".len() + 1;
            if !valid {
                search = found + 1;
                continue;
            }
            if found > cursor {
                out.push(Inline::Text(text[cursor..found].to_owned(), style.clone()));
            }
            let href = if url.starts_with("www.") {
                format!("http://{url}")
            } else {
                url.to_owned()
            };
            out.push(Inline::Text(
                url.to_owned(),
                Style {
                    link: Some(href),
                    ..style.clone()
                },
            ));
            cursor = url_end;
            search = url_end;
        }
        if cursor == 0 {
            out.push(inline);
        } else if cursor < text.len() {
            out.push(Inline::Text(text[cursor..].to_owned(), style.clone()));
        }
    }
    *list = out;
}

fn autolink_blocks(blocks: &mut [Block]) {
    for block in blocks {
        match block {
            Block::Paragraph(list) | Block::Heading(_, list) => autolink(list),
            Block::List { items, .. } => items.iter_mut().for_each(|item| autolink_blocks(item)),
            Block::Quote(children) | Block::Alert(_, children) => autolink_blocks(children),
            Block::Details {
                summary, blocks, ..
            } => {
                autolink(summary);
                autolink_blocks(blocks);
            }
            Block::Table { head, rows, .. } => {
                head.iter_mut().for_each(autolink);
                rows.iter_mut().flatten().for_each(autolink);
            }
            Block::Code { .. } | Block::Rule | Block::Image { .. } => {}
        }
    }
}

/// The image of a paragraph holding nothing else (whitespace aside), from
/// just after its start; moves past the paragraph when it is one.
fn standalone_image(events: &[Event], index: &mut usize) -> Option<Block> {
    let blank = |event: &Event| match event {
        Event::Text(t) => t.trim().is_empty(),
        Event::SoftBreak | Event::HardBreak => true,
        _ => false,
    };
    let mut at = *index;
    while events.get(at).is_some_and(blank) {
        at += 1;
    }
    let Some(Event::Start(Tag::Image { dest_url, .. })) = events.get(at) else {
        return None;
    };
    at += 1;
    let mut alt = String::new();
    loop {
        match events.get(at)? {
            Event::End(TagEnd::Image) => break,
            Event::Text(t) | Event::Code(t) => alt.push_str(t),
            _ => {}
        }
        at += 1;
    }
    at += 1;
    while events.get(at).is_some_and(blank) {
        at += 1;
    }
    if !matches!(events.get(at), Some(Event::End(TagEnd::Paragraph))) {
        return None;
    }
    *index = at + 1;
    Some(Block::Image {
        src: dest_url.to_string(),
        alt,
    })
}

fn parse_blocks(events: &[Event], index: &mut usize, until: Option<TagEnd>) -> Vec<Block> {
    parse_blocks_until(events, index, until, false)
}

/// Blocks until `until`, or, inside a `<details>`, until the HTML block that
/// closes it.
fn parse_blocks_until(
    events: &[Event],
    index: &mut usize,
    until: Option<TagEnd>,
    in_details: bool,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut loose: Vec<Inline> = Vec::new();
    while *index < events.len() {
        let event = &events[*index];
        *index += 1;
        match event {
            Event::End(end) if Some(*end) == until => break,
            Event::Start(Tag::Paragraph) => match standalone_image(events, index) {
                Some(image) => blocks.push(image),
                None => blocks.push(Block::Paragraph(parse_inlines(
                    events,
                    index,
                    TagEnd::Paragraph,
                ))),
            },
            Event::Start(Tag::Heading { level, .. }) => {
                let level_number = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                let inlines = parse_inlines(events, index, TagEnd::Heading(*level));
                blocks.push(Block::Heading(level_number, inlines));
            }
            Event::Start(Tag::List(start)) => {
                let mut items = Vec::new();
                while *index < events.len() {
                    match &events[*index] {
                        Event::Start(Tag::Item) => {
                            *index += 1;
                            items.push(parse_blocks(events, index, Some(TagEnd::Item)));
                        }
                        Event::End(TagEnd::List(_)) => {
                            *index += 1;
                            break;
                        }
                        _ => *index += 1,
                    }
                }
                blocks.push(Block::List {
                    start: *start,
                    items,
                });
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let (language, meta) = match kind {
                    CodeBlockKind::Fenced(info) => {
                        let info = info.trim();
                        let (language, meta) =
                            info.split_once(char::is_whitespace).unwrap_or((info, ""));
                        (language.to_owned(), meta.trim().to_owned())
                    }
                    CodeBlockKind::Indented => (String::new(), String::new()),
                };
                let mut code = String::new();
                while *index < events.len() {
                    let event = &events[*index];
                    *index += 1;
                    match event {
                        Event::Text(t) => code.push_str(t),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                if code.ends_with('\n') {
                    code.pop();
                }
                blocks.push(Block::Code {
                    language: if language.is_empty() {
                        "text".into()
                    } else {
                        language
                    },
                    code,
                    meta,
                });
            }
            Event::Start(Tag::Table(alignments)) => {
                let mut head = Vec::new();
                let mut rows = Vec::new();
                let mut row: Vec<Vec<Inline>> = Vec::new();
                while *index < events.len() {
                    let event = &events[*index];
                    *index += 1;
                    match event {
                        Event::Start(Tag::TableCell) => {
                            row.push(parse_inlines(events, index, TagEnd::TableCell))
                        }
                        Event::End(TagEnd::TableHead) => head = std::mem::take(&mut row),
                        Event::End(TagEnd::TableRow) => rows.push(std::mem::take(&mut row)),
                        Event::End(TagEnd::Table) => break,
                        _ => {}
                    }
                }
                blocks.push(Block::Table {
                    alignments: alignments.clone(),
                    head,
                    rows,
                });
            }
            Event::Start(Tag::BlockQuote(kind)) => {
                let children = parse_blocks(events, index, Some(TagEnd::BlockQuote(*kind)));
                blocks.push(match kind {
                    Some(kind) => Block::Alert(alert_kind(*kind), children),
                    None => Block::Quote(children),
                });
            }
            Event::Start(Tag::HtmlBlock) => {
                let mut html = String::new();
                while *index < events.len() {
                    let event = &events[*index];
                    *index += 1;
                    match event {
                        Event::Html(t) | Event::Text(t) => html.push_str(t),
                        Event::End(TagEnd::HtmlBlock) => break,
                        _ => {}
                    }
                }
                if !RAW_HTML.get() {
                    blocks.push(Block::Paragraph(vec![Inline::Text(
                        html.trim_end_matches('\n').to_owned(),
                        Style::default(),
                    )]));
                    continue;
                }
                let lower = html.to_ascii_lowercase();
                if in_details && !lower.contains("<details") && lower.contains("</details>") {
                    let before = &html[..lower.find("</details>").unwrap_or(0)];
                    html_blocks(before, &mut blocks);
                    break;
                }
                match details_start(&html) {
                    Some((summary, open, after)) => {
                        let after_lower = after.to_ascii_lowercase();
                        let mut children = Vec::new();
                        if let Some(close) = after_lower.find("</details>") {
                            html_blocks(&after[..close], &mut children);
                        } else {
                            html_blocks(&after, &mut children);
                            children.extend(parse_blocks_until(events, index, until, true));
                        }
                        blocks.push(Block::Details {
                            summary,
                            open,
                            blocks: children,
                        });
                    }
                    None => html_blocks(&html, &mut blocks),
                }
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                let label = label.to_string();
                let children = parse_blocks(events, index, Some(TagEnd::FootnoteDefinition));
                FOOTNOTES.with_borrow_mut(|f| f.definitions.push((label, children)));
            }
            Event::FootnoteReference(label) => {
                loose.push(footnote_reference(label));
            }
            Event::Rule => blocks.push(Block::Rule),
            // Tight list items carry inline content without a paragraph.
            Event::Text(_) | Event::Code(_) | Event::SoftBreak | Event::HardBreak => {
                *index -= 1;
                loose.extend(parse_inline_run(events, index));
                blocks.push(Block::Paragraph(std::mem::take(&mut loose)));
            }
            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. }) => {
                *index -= 1;
                blocks.push(Block::Paragraph(parse_inline_run(events, index)));
            }
            Event::TaskListMarker(done) => loose.push(Inline::Text(
                if *done { "☑ " } else { "☐ " }.into(),
                Style::default(),
            )),
            _ => {}
        }
    }
    blocks
}

/// Inline events until a block boundary (used for tight list items).
fn parse_inline_run(events: &[Event], index: &mut usize) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut style = Style::default();
    let mut html_code: Option<String> = None;
    let mut depth = 0;
    while *index < events.len() {
        match &events[*index] {
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::Start(Tag::Strong) => style.bold = true,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::Start(Tag::Link { dest_url, .. }) => {
                match link_chip(dest_url, &events[*index + 1..]) {
                    Some(chip) => {
                        *index += 1;
                        skip_link(events, index);
                        out.push(chip);
                        continue;
                    }
                    None => style.link = Some(dest_url.to_string()),
                }
            }
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::End(TagEnd::Link) => style.link = None,
            Event::Text(_) if style.hidden => {}
            Event::Text(t) => match &mut html_code {
                Some(code) => code.push_str(t),
                None => out.push(Inline::Text(t.to_string(), style.clone())),
            },
            Event::Code(t) => out.push(code_inline(t)),
            Event::SoftBreak => out.push(Inline::Text(" ".into(), style.clone())),
            Event::HardBreak => out.push(Inline::Break),
            Event::InlineHtml(t) => inline_html(t, &mut style, &mut out, &mut html_code),
            Event::FootnoteReference(label) => out.push(footnote_reference(label)),
            // A nested list (or any block) ends the item's inline run.
            Event::Start(
                Tag::List(_)
                | Tag::Item
                | Tag::Paragraph
                | Tag::CodeBlock(_)
                | Tag::BlockQuote(_)
                | Tag::Table(_)
                | Tag::Heading { .. },
            ) => break,
            Event::Start(_) => depth += 1,
            Event::End(_) if depth > 0 => depth -= 1,
            _ => break,
        }
        *index += 1;
    }
    out
}

fn parse_inlines(events: &[Event], index: &mut usize, end: TagEnd) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut style = Style::default();
    let mut html_code: Option<String> = None;
    while *index < events.len() {
        let event = &events[*index];
        *index += 1;
        match event {
            Event::End(e) if *e == end => break,
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::Start(Tag::Strong) => style.bold = true,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::Start(Tag::Link { dest_url, .. }) => {
                match link_chip(dest_url, &events[*index..]) {
                    Some(chip) => {
                        skip_link(events, index);
                        out.push(chip);
                    }
                    None => style.link = Some(dest_url.to_string()),
                }
            }
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::End(TagEnd::Link) => style.link = None,
            Event::Text(_) if style.hidden => {}
            Event::Text(t) => match &mut html_code {
                Some(code) => code.push_str(t),
                None => out.push(Inline::Text(t.to_string(), style.clone())),
            },
            Event::Code(t) => out.push(code_inline(t)),
            Event::SoftBreak => out.push(Inline::Text(" ".into(), style.clone())),
            Event::HardBreak => out.push(Inline::Break),
            Event::Html(t) | Event::InlineHtml(t) => {
                inline_html(t, &mut style, &mut out, &mut html_code)
            }
            Event::FootnoteReference(label) => out.push(footnote_reference(label)),
            _ => {}
        }
    }
    out
}

struct Inlines {
    galley: Arc<Galley>,
    /// Section index → inline code chip group.
    chips: Vec<Option<usize>>,
    /// Section index → file or citation chip group and its target.
    files: Vec<Option<(usize, ChipRef)>>,
    /// Section index → the icon a link's leading space holds.
    icons: Vec<Option<md_links::LinkIcon>>,
    links: Vec<Option<String>>,
    /// The text size (chip metrics are in em).
    size: f32,
    /// Section index → the inline it renders, and those inlines (copies).
    owners: Vec<Option<usize>>,
    source: Vec<Inline>,
}

/// `label` cut with an ellipsis to fit `max_width` in `font` (CSS `truncate`).
fn truncate_label(ui: &Ui, label: &str, font: &egui::FontId, max_width: f32) -> String {
    if widgets::text_width(ui, label, font.clone()) <= max_width {
        return label.to_owned();
    }
    let chars: Vec<char> = label.chars().collect();
    let mut keep = chars.len();
    while keep > 0 {
        keep -= 1;
        let candidate: String = chars[..keep].iter().collect::<String>() + "…";
        if widgets::text_width(ui, &candidate, font.clone()) <= max_width {
            return candidate;
        }
    }
    "…".to_owned()
}

/// A file chip's room before its label: the border, the padding, the icon
/// and the gap.
fn chip_lead(chip_size: f32) -> f32 {
    1.0 + (0.5 + 1.17 + 0.33) * chip_size
}

/// A link icon's room: `ms-[0.25em]`, 14px, `me-[0.2em]`.
fn link_icon_width(size: f32) -> f32 {
    0.25 * size + 14.0 + 0.2 * size
}

/// A transparent non-breaking space `width` wide in `font` (a section's
/// letter spacing goes before its glyph, so this never opens a paragraph).
fn spacer(ui: &Ui, font: egui::FontId, width: f32, line_height: f32) -> egui::TextFormat {
    let nbsp = ui.fonts_mut(|f| f.glyph_width(&font, '\u{a0}'));
    let mut format = widgets::format(font, Color32::TRANSPARENT, line_height);
    format.extra_letter_spacing = width - nbsp;
    format
}

/// Appends one section per call: the inline's bookkeeping (`chips`,
/// `files`, `icons`, `links`) is indexed by section, so egui's merging of
/// a section into the previous one of the same format must not happen.
fn push_section(job: &mut LayoutJob, text: &str, leading_space: f32, format: egui::TextFormat) {
    let start = job.text.len();
    job.text.push_str(text);
    job.sections.push(egui::text::LayoutSection {
        leading_space,
        byte_range: egui::text::ByteIndex(start)..egui::text::ByteIndex(job.text.len()),
        format,
    });
}

/// The job and its per-section metadata, with a line break inserted before
/// the sections (by their index without the inserted breaks) in `breaks`.
#[allow(clippy::too_many_arguments)]
fn build_inlines(
    ui: &Ui,
    inlines: &[Inline],
    size: f32,
    line_height: f32,
    base_weight: Weight,
    color: Color32,
    suffixes: &HashMap<String, String>,
    breaks: &std::collections::HashSet<usize>,
) -> (LayoutJob, Inlines, Vec<Option<usize>>) {
    let mut job = LayoutJob::default();
    let mut chips = Vec::new();
    let mut files = Vec::new();
    let mut icons = Vec::new();
    let mut links = Vec::new();
    // Where breaks were inserted (job section indices).
    let mut inserted: Vec<usize> = Vec::new();
    let newline = widgets::format(font(size, base_weight), Color32::TRANSPARENT, line_height);
    let break_before = |job: &mut LayoutJob,
                        chips: &mut Vec<Option<usize>>,
                        files: &mut Vec<Option<(usize, ChipRef)>>,
                        icons: &mut Vec<Option<md_links::LinkIcon>>,
                        links: &mut Vec<Option<String>>,
                        inserted: &mut Vec<usize>| {
        let original = chips.len() - inserted.len();
        if breaks.contains(&original) {
            inserted.push(chips.len());
            // The zero-width space marks the break as layout-only, so copies
            // drop it (`widgets::CopyCleanup`).
            push_section(job, "\u{200B}\n", 0.0, newline.clone());
            chips.push(None);
            files.push(None);
            icons.push(None);
            links.push(None);
        }
    };
    let mut chip = 0;
    let mut file_chip = 0;
    let mut previous_link: Option<String> = None;
    let mut owned: Vec<(Range<usize>, usize)> = Vec::new();
    for (inline_index, inline) in inlines.iter().enumerate() {
        let first_section = chips.len();
        // A link leads with its site's icon (`ms-[0.25em] me-[0.2em]`
        // around 14px), kept on the line of its first word.
        // The room goes before the link's first text (`leading_space`).
        let link = match inline {
            Inline::Text(_, style) => style.link.clone(),
            _ => None,
        };
        let mut link_icon = None;
        if let Some(url) = &link
            && previous_link.as_ref() != Some(url)
            && let Some(host) = md_links::external_host(url)
        {
            link_icon = Some(md_links::link_icon(&host, ui.ctx()));
        }
        previous_link = link;
        match inline {
            Inline::Text(text, style) => {
                let weight = if style.superscript {
                    Weight::SemiBold
                } else if style.bold {
                    Weight::Bold
                } else {
                    base_weight
                };
                let text_size = if style.superscript { 11.0 } else { size };
                let mut format = widgets::format(font(text_size, weight), color, line_height);
                if style.superscript {
                    format.valign = egui::Align::TOP;
                }
                format.italics = style.italic;
                if style.strike {
                    format.strikethrough = Stroke::new(1.0, color);
                }
                if style.link.is_some() {
                    format.color = INFO_FOREGROUND;
                }
                let leading = if link_icon.is_some() {
                    break_before(
                        &mut job,
                        &mut chips,
                        &mut files,
                        &mut icons,
                        &mut links,
                        &mut inserted,
                    );
                    link_icon_width(size)
                } else {
                    0.0
                };
                push_section(&mut job, text, leading, format);
                chips.push(None);
                files.push(None);
                icons.push(link_icon.take());
                links.push(style.link.clone());
            }
            Inline::File(file) => {
                // `ContextChip`: 0.86em medium text, 1px border, 0.5em
                // padding, a 1.17em icon and a 0.33em gap; it never wraps.
                let chip_size = size * 0.86;
                let chip_font = font(chip_size, Weight::Medium);
                let label = md_links::chip_label(file, suffixes).replace(' ', "\u{a0}");
                let [_, _, text] = md_links::mention_colors(theme::border(), theme::foreground());
                break_before(
                    &mut job,
                    &mut chips,
                    &mut files,
                    &mut icons,
                    &mut links,
                    &mut inserted,
                );
                push_section(
                    &mut job,
                    &label,
                    chip_lead(chip_size),
                    widgets::format(chip_font.clone(), text, line_height),
                );
                push_section(
                    &mut job,
                    "\u{a0}",
                    0.0,
                    spacer(ui, chip_font, 0.5 * chip_size + 1.0, line_height),
                );
                for _ in 0..2 {
                    chips.push(None);
                    files.push(Some((file_chip, ChipRef::File(file.clone()))));
                    icons.push(None);
                    links.push(None);
                }
                file_chip += 1;
            }
            Inline::Citation(citation) => {
                // `AssistantCitationChip`: the quote icon and the label
                // truncated at 16em, laid out like a file chip.
                let chip_size = size * 0.86;
                let chip_font = font(chip_size, Weight::Medium);
                let label = truncate_label(ui, &citation.label(), &chip_font, 16.0 * chip_size)
                    .replace(' ', "\u{a0}");
                // Inside chat text the source link takes the link color.
                let text = INFO_FOREGROUND;
                break_before(
                    &mut job,
                    &mut chips,
                    &mut files,
                    &mut icons,
                    &mut links,
                    &mut inserted,
                );
                push_section(
                    &mut job,
                    &label,
                    chip_lead(chip_size),
                    widgets::format(chip_font.clone(), text, line_height),
                );
                push_section(
                    &mut job,
                    "\u{a0}",
                    0.0,
                    spacer(ui, chip_font, 0.5 * chip_size + 1.0, line_height),
                );
                for _ in 0..2 {
                    chips.push(None);
                    files.push(Some((file_chip, ChipRef::Citation(citation.clone()))));
                    icons.push(None);
                    links.push(None);
                }
                file_chip += 1;
            }
            Inline::Context { kind, label, id } => {
                // `ContextChip` of the kind: its icon and label, laid out
                // like a file chip.
                let chip_size = size * 0.86;
                let chip_font = font(chip_size, Weight::Medium);
                let label_text = label.replace(' ', "\u{a0}");
                let (chroma, hue) = crate::ui::composer_context::kind_accent(kind);
                let [_, _, text] =
                    md_links::chip_colors(chroma, hue, theme::border(), theme::foreground());
                break_before(
                    &mut job,
                    &mut chips,
                    &mut files,
                    &mut icons,
                    &mut links,
                    &mut inserted,
                );
                push_section(
                    &mut job,
                    &label_text,
                    chip_lead(chip_size),
                    widgets::format(chip_font.clone(), text, line_height),
                );
                push_section(
                    &mut job,
                    "\u{a0}",
                    0.0,
                    spacer(ui, chip_font, 0.5 * chip_size + 1.0, line_height),
                );
                let chip_ref = ChipRef::Context {
                    kind: kind.clone(),
                    label: label.clone(),
                    id: id.clone(),
                };
                for _ in 0..2 {
                    chips.push(None);
                    files.push(Some((file_chip, chip_ref.clone())));
                    icons.push(None);
                    links.push(None);
                }
                file_chip += 1;
            }
            Inline::Code(code) => {
                // CSS: 12px mono, padding .1rem .35rem, 1px border.
                let code_font = theme::mono(12.0);
                let pad_font = theme::mono(12.0);
                let space = ui.fonts_mut(|f| f.glyph_width(&pad_font, ' '));
                let mut pad = widgets::format(pad_font, Color32::TRANSPARENT, line_height);
                pad.extra_letter_spacing = 6.6 - space;
                push_section(&mut job, " ", 0.0, pad.clone());
                push_section(
                    &mut job,
                    code,
                    0.0,
                    widgets::format(code_font, theme::foreground(), line_height),
                );
                push_section(&mut job, " ", 0.0, pad);
                chips.extend([Some(chip), Some(chip), Some(chip)]);
                files.extend([None, None, None]);
                icons.extend([None, None, None]);
                links.extend([None, None, None]);
                chip += 1;
            }
            Inline::Break => {
                push_section(
                    &mut job,
                    "\n",
                    0.0,
                    widgets::format(font(size, base_weight), color, line_height),
                );
                chips.push(None);
                files.push(None);
                icons.push(None);
                links.push(None);
            }
        }
        owned.push((first_section..chips.len(), inline_index));
    }
    // Section owners, without the inserted breaks.
    let mut owners = vec![None; chips.len()];
    for (sections, inline_index) in owned {
        for section in sections {
            if !inserted.contains(&section) {
                owners[section] = Some(inline_index);
            }
        }
    }
    // Each section's index without the inserted breaks.
    let mut original = 0;
    let origins: Vec<Option<usize>> = (0..chips.len())
        .map(|index| {
            if inserted.contains(&index) {
                None
            } else {
                original += 1;
                Some(original - 1)
            }
        })
        .collect();
    let empty = widgets::center_rows(ui.fonts_mut(|f| f.layout_job(LayoutJob::default())));
    (
        job,
        Inlines {
            galley: empty,
            chips,
            files,
            icons,
            links,
            size,
            owners,
            source: Vec::new(),
        },
        origins,
    )
}

/// Lays the inlines out. A section whose room sits before it (a link's icon,
/// a chip's padding) must not start a wrapped row, where epaint drops that
/// room: such a section gets a line break of its own, like upstream's
/// `whitespace-nowrap` keeps the icon with its word.
#[allow(clippy::too_many_arguments)]
fn layout_inlines(
    ui: &Ui,
    inlines: &[Inline],
    size: f32,
    line_height: f32,
    base_weight: Weight,
    color: Color32,
    wrap: f32,
    suffixes: &HashMap<String, String>,
) -> Inlines {
    let mut breaks = std::collections::HashSet::new();
    loop {
        let (mut job, mut laid, origins) = build_inlines(
            ui,
            inlines,
            size,
            line_height,
            base_weight,
            color,
            suffixes,
            &breaks,
        );
        job.wrap.max_width = wrap;
        laid.galley = widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job)));
        laid.source = inlines.to_vec();
        let sections = char_sections(&laid.galley);
        let mut added = false;
        let mut char_index = 0usize;
        let rows = &laid.galley.rows;
        for (row_index, placed) in rows.iter().enumerate() {
            let continues = row_index > 0 && !rows[row_index - 1].ends_with_newline;
            if continues && !placed.row.glyphs.is_empty() {
                let section = sections.get(char_index).copied().unwrap_or(0);
                let starts_section =
                    char_index == 0 || sections.get(char_index - 1).copied() != Some(section);
                let leading = laid
                    .galley
                    .job
                    .sections
                    .get(section)
                    .map_or(0.0, |s| s.leading_space);
                if starts_section
                    && leading > 0.0
                    && let Some(Some(original)) = origins.get(section)
                {
                    added |= breaks.insert(*original);
                }
            }
            char_index += placed.row.glyphs.len() + usize::from(placed.ends_with_newline);
        }
        if !added {
            return laid;
        }
    }
}

/// Section index of every character of the galley's text.
fn char_sections(galley: &Galley) -> Vec<usize> {
    galley
        .job
        .text
        .char_indices()
        .map(|(byte, _)| {
            galley
                .job
                .sections
                .iter()
                .position(|s| s.byte_range.start.0 <= byte && byte < s.byte_range.end.0)
                .unwrap_or(0)
        })
        .collect()
}

/// Registers how the galley copies as markdown (`md_copy`): each inline's
/// character range.
fn register_copy(
    ui: &Ui,
    id: egui::Id,
    inlines: &Inlines,
    kind: md_copy::Kind,
    prefixes: Vec<md_copy::Prefix>,
) {
    let text = &inlines.galley.job.text;
    let mut segments: Vec<(Range<usize>, Inline)> = Vec::new();
    let mut last_owner = None;
    for (section, owner) in inlines.galley.job.sections.iter().zip(&inlines.owners) {
        let Some(owner) = *owner else {
            continue;
        };
        let start = text[..section.byte_range.start.0].chars().count();
        let end = start
            + text[section.byte_range.start.0..section.byte_range.end.0]
                .chars()
                .count();
        match segments.last_mut() {
            Some((range, _)) if last_owner == Some(owner) => range.end = end,
            _ => {
                if let Some(inline) = inlines.source.get(owner) {
                    segments.push((start..end, inline.clone()));
                    last_owner = Some(owner);
                }
            }
        }
    }
    let recipe = md_copy::Recipe {
        kind,
        prefixes,
        segments,
    };
    ui.ctx()
        .plugin::<widgets::CopyCleanup>()
        .lock()
        .recipes
        .insert(id, Arc::new(recipe));
}

/// A cited quote marked in the galley (`::highlight(t3-assistant-citation)`:
/// the primary color at `alpha`); a quote spanning blocks is matched line
/// by line.
fn paint_quote_highlight(ui: &Ui, pos: Pos2, galley: &Galley, quote: &str, alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let text = galley.text();
    let color = theme::primary().gamma_multiply(alpha);
    for piece in quote.split('\n').map(str::trim).filter(|p| !p.is_empty()) {
        let Some((start, end)) = citations::find_quote(text, piece, 0) else {
            continue;
        };
        let range = text[..start].chars().count()..text[..end].chars().count();
        let mut char_index = 0usize;
        for placed in &galley.rows {
            let mut span: Option<(f32, f32)> = None;
            for glyph in &placed.row.glyphs {
                if range.contains(&char_index) {
                    let right = glyph.pos.x + glyph.advance_width;
                    span = Some(span.map_or((glyph.pos.x, right), |(l, _)| (l, right)));
                }
                char_index += 1;
            }
            if let Some((left, right)) = span {
                let row = placed.rect().translate(pos.to_vec2());
                ui.painter().rect_filled(
                    Rect::from_min_max(
                        Pos2::new(pos.x + placed.pos.x + left, row.top()),
                        Pos2::new(pos.x + placed.pos.x + right, row.bottom()),
                    ),
                    0.0,
                    color,
                );
            }
            if placed.ends_with_newline {
                char_index += 1;
            }
        }
    }
}

/// `foldThreadSearchCase`: lower case where it keeps one character, final
/// sigma as sigma.
pub(crate) fn fold_find_case(text: &str) -> Vec<char> {
    text.chars()
        .map(|c| {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some('ς'), None) => 'σ',
                (Some(single), None) => single,
                _ => c,
            }
        })
        .collect()
}

/// `findThreadSearchOccurrences`: where `query` (folded) occurs in `text`,
/// as char ranges, without overlaps.
pub(crate) fn find_occurrences(text: &str, query: &[char]) -> Vec<std::ops::Range<usize>> {
    let text = fold_find_case(text);
    let mut out = Vec::new();
    if query.is_empty() {
        return out;
    }
    let mut at = 0;
    while at + query.len() <= text.len() {
        if text[at..at + query.len()] == *query {
            out.push(at..at + query.len());
            at += query.len();
        } else {
            at += 1;
        }
    }
    out
}

/// Find in thread's marks on one timeline entry (`threadFindHighlights.ts`):
/// the query's occurrences in each drawn text block, counted in reading
/// order as the server counts them; `active` is the selected one.
#[derive(Clone, Debug, Default)]
pub(crate) struct FindMarks {
    query: Vec<char>,
    active: Option<usize>,
    /// Occurrences met so far in the entry.
    pub(crate) seen: usize,
    /// Where the active occurrence was drawn.
    pub(crate) active_rect: Option<Rect>,
}

/// Rects of one galley's occurrences and whether each is the active one.
pub(crate) type MarkRects = Vec<(Rect, bool)>;

impl FindMarks {
    pub(crate) fn new(query: &str, active: Option<usize>) -> Self {
        Self {
            query: fold_find_case(query.trim()),
            active,
            ..Default::default()
        }
    }

    /// Counts `text`'s occurrences without drawing them (text drawn apart
    /// from a galley, like a plan's title).
    pub(crate) fn skip(&mut self, text: &str) {
        self.seen += find_occurrences(text, &self.query).len();
    }

    /// Paints the backgrounds of the occurrences in `galley` at `pos`
    /// (`::highlight(t3-thread-find)`, the active one
    /// `t3-thread-find-active`); `recolor` then repaints their text.
    pub(crate) fn mark(&mut self, ui: &Ui, pos: Pos2, galley: &Galley) -> MarkRects {
        let mut marks = Vec::new();
        for range in find_occurrences(galley.text(), &self.query) {
            let active = self.active == Some(self.seen);
            self.seen += 1;
            let mut char_index = 0usize;
            for placed in &galley.rows {
                let mut span: Option<(f32, f32)> = None;
                for glyph in &placed.row.glyphs {
                    if range.contains(&char_index) {
                        let right = glyph.pos.x + glyph.advance_width;
                        span = Some(span.map_or((glyph.pos.x, right), |(l, _)| (l, right)));
                    }
                    char_index += 1;
                }
                if placed.ends_with_newline {
                    char_index += 1;
                }
                let Some((left, right)) = span else {
                    continue;
                };
                let row = placed.rect().translate(pos.to_vec2());
                let rect = Rect::from_min_max(
                    Pos2::new(pos.x + placed.pos.x + left, row.top()),
                    Pos2::new(pos.x + placed.pos.x + right, row.bottom()),
                );
                if active {
                    self.active_rect = Some(self.active_rect.map_or(rect, |r| r.union(rect)));
                }
                ui.painter().rect_filled(rect, 0.0, find_colors(active).0);
                marks.push((rect, active));
            }
        }
        marks
    }

    /// Repaints the marked text in the highlight's text color.
    pub(crate) fn recolor(ui: &Ui, pos: Pos2, galley: &Arc<Galley>, marks: &MarkRects) {
        for (rect, active) in marks {
            ui.painter()
                .with_clip_rect(rect.intersect(ui.clip_rect()))
                .galley_with_override_text_color(pos, galley.clone(), find_colors(*active).1);
        }
    }
}

/// The find highlight's background and text (`index.css`'s
/// `::highlight(t3-thread-find…)`).
fn find_colors(active: bool) -> (Color32, Color32) {
    let dark_text = Color32::from_rgb(33, 24, 10); // oklch(0.216164 0.028283 77.423)
    if active {
        (Color32::from_rgb(238, 165, 43), dark_text) // oklch(0.774202 0.151946 74.331)
    } else if theme::palette().dark {
        (
            Color32::from_rgb(140, 108, 31),
            Color32::from_rgb(245, 245, 245),
        ) // oklch(0.55 0.1 85)
    } else {
        (Color32::from_rgb(255, 222, 139), dark_text) // oklch(0.915 0.11 85)
    }
}

/// Paints inline text, chip backgrounds and icons, and handles link clicks;
/// returns a file or citation chip that was clicked.
fn paint_inlines(
    ui: &mut Ui,
    pos: Pos2,
    inlines: &Inlines,
    id: egui::Id,
    highlight: Option<&(String, f32)>,
    find: Option<&mut FindMarks>,
    copy: Option<(md_copy::Kind, Vec<md_copy::Prefix>)>,
) -> Option<ChipRef> {
    if let Some((kind, prefixes)) = copy {
        register_copy(ui, id, inlines, kind, prefixes);
    }
    let painter = ui.painter().clone();
    if let Some((quote, alpha)) = highlight {
        paint_quote_highlight(ui, pos, &inlines.galley, quote, *alpha);
    }
    let marks = find.map(|find| find.mark(ui, pos, &inlines.galley));
    let sections = char_sections(&inlines.galley);
    let chip_size = inlines.size * 0.86;
    let [chip_border, chip_fill, _] =
        md_links::mention_colors(theme::border(), theme::foreground());
    let [citation_border, citation_fill, _] = citations::chip_colors();
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let mut file_rects: Vec<(Rect, ChipRef)> = Vec::new();
    // Rows hold one glyph per character; a trailing `\n` is omitted.
    let mut char_index = 0usize;
    for placed in &inlines.galley.rows {
        let mut run: Option<(usize, f32, f32)> = None;
        let mut file_run: Option<(usize, f32, f32, ChipRef)> = None;
        let row_top = pos.y + placed.pos.y;
        let row_height = placed.row.size.y;
        let row_x = pos.x + placed.pos.x;
        let flush = |run: &mut Option<(usize, f32, f32)>| {
            if let Some((_, left, right)) = run.take() {
                let height = 21.0f32.min(row_height);
                let top = row_top + ((row_height - height) / 2.0).round();
                let rect = Rect::from_min_max(
                    Pos2::new(row_x + left, top),
                    Pos2::new(row_x + right, top + height),
                );
                painter.rect(
                    rect,
                    CornerRadius::same(6),
                    theme::muted(),
                    Stroke::new(1.0, theme::border()),
                    StrokeKind::Inside,
                );
            }
        };
        // A file chip: the fill, the border over it, the file-type icon.
        let mut flush_file = |file_run: &mut Option<(usize, f32, f32, ChipRef)>| {
            if let Some((_, left, right, chip)) = file_run.take() {
                let height = (1.41 * chip_size).round();
                let top = row_top + ((row_height - height) / 2.0).round();
                let rect = Rect::from_min_max(
                    Pos2::new((row_x + left).round(), top),
                    Pos2::new((row_x + right).round(), top + height),
                );
                let radius = CornerRadius::same((0.5 * chip_size).round() as u8);
                let (fill, border) = match &chip {
                    ChipRef::File(_) => (chip_fill, chip_border),
                    ChipRef::Citation(_) => (citation_fill, citation_border),
                    ChipRef::Context { kind, .. } => {
                        let (chroma, hue) = crate::ui::composer_context::kind_accent(kind);
                        let [border, fill, _] = md_links::chip_colors(
                            chroma,
                            hue,
                            theme::border(),
                            theme::foreground(),
                        );
                        (fill, border)
                    }
                };
                painter.rect_filled(rect, radius, fill);
                // A citation's source link tints on hover.
                if matches!(chip, ChipRef::Citation(_)) && pointer.is_some_and(|p| rect.contains(p))
                {
                    let hover = citations::chip_accent().gamma_multiply(0.17);
                    painter.rect_filled(rect.shrink(1.0), CornerRadius::same(4), hover);
                }
                painter.rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
                let icon_size = (1.17 * chip_size).round();
                let icon = Rect::from_min_size(
                    Pos2::new(
                        rect.left() + 1.0 + (0.5 * chip_size).round(),
                        rect.center().y - icon_size / 2.0,
                    ),
                    Vec2::splat(icon_size),
                );
                let (name, tint) = match &chip {
                    ChipRef::File(file) if !md_links::basename(&file.path).contains('.') => {
                        ("folder", theme::muted_foreground())
                    }
                    ChipRef::File(file) => crate::ui::files::file_icon(&file.path),
                    ChipRef::Citation(_) => ("quote", INFO_FOREGROUND),
                    ChipRef::Context { kind, .. } => {
                        let (chroma, hue) = crate::ui::composer_context::kind_accent(kind);
                        let [_, _, text] = md_links::chip_colors(
                            chroma,
                            hue,
                            theme::border(),
                            theme::foreground(),
                        );
                        (crate::ui::composer_context::kind_icon(kind), text)
                    }
                };
                theme::paint_icon(ui, name, icon, icon_size, tint);
                file_rects.push((rect, chip));
            }
        };
        for glyph in &placed.row.glyphs {
            let section = sections.get(char_index).copied().unwrap_or(0);
            char_index += 1;
            let chip = inlines.chips.get(section).copied().flatten();
            match (chip, &mut run) {
                (Some(c), Some((current, _, right))) if *current == c => {
                    *right = glyph.pos.x + glyph.advance_width;
                }
                (Some(c), _) => {
                    flush(&mut run);
                    run = Some((c, glyph.pos.x, glyph.pos.x + glyph.advance_width));
                }
                (None, _) => flush(&mut run),
            }
            let file = inlines.files.get(section).cloned().flatten();
            match (file, &mut file_run) {
                (Some((c, _)), Some((current, _, right, _))) if *current == c => {
                    *right = glyph.pos.x + glyph.advance_width;
                }
                (Some((c, file)), _) => {
                    flush_file(&mut file_run);
                    // The run opens on the label; its room is to the left.
                    let left = glyph.pos.x - chip_lead(chip_size);
                    file_run = Some((c, left, glyph.pos.x + glyph.advance_width, file));
                }
                (None, _) => flush_file(&mut file_run),
            }
            // A link's icon sits in its first text's leading space.
            let first_of_section =
                char_index < 2 || sections.get(char_index - 2).copied() != Some(section);
            if first_of_section && let Some(Some(icon)) = inlines.icons.get(section) {
                let rect = Rect::from_min_size(
                    Pos2::new(
                        row_x + glyph.pos.x - link_icon_width(inlines.size) + 0.25 * inlines.size,
                        row_top + (row_height - 14.0) / 2.0 + 0.125 * inlines.size / 2.0,
                    ),
                    Vec2::splat(14.0),
                );
                match icon {
                    md_links::LinkIcon::Named(name) => {
                        theme::paint_icon(ui, name, rect, 14.0, INFO_FOREGROUND);
                    }
                    md_links::LinkIcon::Image(image) => {
                        crate::ui::attachments::paint_contain(
                            ui,
                            rect,
                            &format!("link-favicon:{:p}", Arc::as_ptr(image)),
                            image,
                            Color32::WHITE,
                        );
                    }
                }
            }
        }
        flush(&mut run);
        flush_file(&mut file_run);
        if placed.ends_with_newline {
            char_index += 1;
        }
    }
    let response =
        widgets::selectable_galley(ui, id, pos, inlines.galley.clone(), theme::foreground());
    if let Some(marks) = &marks {
        FindMarks::recolor(ui, pos, &inlines.galley, marks);
    }
    // A right click on a link offers Copy Link in the text menu.
    if response.secondary_clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        let cursor = inlines.galley.cursor_from_pos(at - pos);
        let link = sections
            .get(cursor.index.0)
            .and_then(|s| inlines.links.get(*s).cloned().flatten())
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"));
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new(widgets::TEXT_MENU_REQUEST), (at, link)));
    }
    let mut opened = None;
    if let Some(pointer) = response.hover_pos() {
        if let Some((chip_rect, chip)) = file_rects.iter().find(|(r, _)| r.contains(pointer)) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            let chip_rect = *chip_rect;
            let tooltip = match chip {
                ChipRef::File(file) => match file.line {
                    Some(line) => format!("{}:{line}", file.path),
                    None => file.path.clone(),
                },
                ChipRef::Citation(_) => "View source".to_owned(),
                ChipRef::Context { label, .. } => label.clone(),
            };
            let clicked = response.clicked();
            response.on_hover_text(tooltip);
            if clicked {
                opened = Some(chip.clone());
                // Where it sits, for a popover anchored on it.
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::new(CLICKED_CHIP), chip_rect));
            }
            return opened;
        }
        let cursor = inlines.galley.cursor_from_pos(pointer - pos);
        let section = sections.get(cursor.index.0).copied();
        if let Some(url) = section.and_then(|s| inlines.links.get(s).cloned().flatten()) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if response.clicked() {
                widgets::open_external(&url);
            }
        }
    }
    opened
}

/// The temp-data key of the last clicked chip's rect.
pub(crate) const CLICKED_CHIP: &str = "markdown-clicked-chip";

/// `renderSkillInlineMarkdownChildren`: known `$skills` in text (not in
/// code or links) become skill chips.
fn skill_chip_blocks(blocks: &mut [Block]) {
    fn inlines(list: &mut Vec<Inline>) {
        let mut out = Vec::with_capacity(list.len());
        for inline in list.drain(..) {
            let Inline::Text(text, style) = &inline else {
                out.push(inline);
                continue;
            };
            if style.link.is_some() || !text.contains(crate::ui::command_menu::is_currency) {
                out.push(inline);
                continue;
            }
            // `SKILL_TOKEN_REGEX` here also matches at the text's end.
            let padded = format!("{text} ");
            let mut cursor = 0;
            for (name, range) in crate::ui::mentions::collect_skills(&padded) {
                let Some(meta) = crate::ui::cite::known_skill(&name) else {
                    continue;
                };
                if range.start > cursor {
                    out.push(Inline::Text(
                        text[cursor..range.start].to_owned(),
                        style.clone(),
                    ));
                }
                out.push(Inline::Context {
                    kind: "skill".into(),
                    label: meta.label,
                    id: name,
                });
                cursor = range.end.min(text.len());
            }
            if cursor == 0 {
                out.push(inline);
            } else if cursor < text.len() {
                out.push(Inline::Text(text[cursor..].to_owned(), style.clone()));
            }
        }
        *list = out;
    }
    for block in blocks {
        match block {
            Block::Paragraph(list) | Block::Heading(_, list) => inlines(list),
            Block::List { items, .. } => items.iter_mut().for_each(|item| skill_chip_blocks(item)),
            Block::Quote(children) | Block::Alert(_, children) => skill_chip_blocks(children),
            Block::Details {
                summary, blocks, ..
            } => {
                inlines(summary);
                skill_chip_blocks(blocks);
            }
            Block::Table { head, rows, .. } => {
                head.iter_mut().for_each(inlines);
                rows.iter_mut().flatten().for_each(inlines);
            }
            Block::Code { .. } | Block::Rule | Block::Image { .. } => {}
        }
    }
}

/// Context chips take their display kind (a pull request by its state).
fn context_display_kinds(blocks: &mut [Block], records: &[serde_json::Value]) {
    fn inlines(list: &mut [Inline], records: &[serde_json::Value]) {
        for inline in list {
            if let Inline::Context { kind, id, .. } = inline {
                let record = records.iter().find(|r| r["contextId"] == id.as_str());
                *kind = crate::ui::composer_context::display_kind(kind, record);
            }
        }
    }
    for block in blocks {
        match block {
            Block::Paragraph(list) | Block::Heading(_, list) => inlines(list, records),
            Block::List { items, .. } => items
                .iter_mut()
                .for_each(|item| context_display_kinds(item, records)),
            Block::Quote(children) | Block::Alert(_, children) => {
                context_display_kinds(children, records)
            }
            Block::Details {
                summary, blocks, ..
            } => {
                inlines(summary, records);
                context_display_kinds(blocks, records);
            }
            Block::Table { head, rows, .. } => {
                head.iter_mut().for_each(|cell| inlines(cell, records));
                rows.iter_mut()
                    .flatten()
                    .for_each(|cell| inlines(cell, records));
            }
            Block::Code { .. } | Block::Rule | Block::Image { .. } => {}
        }
    }
}

pub struct Markdown<'a> {
    pub highlighter: &'a mut Highlighter,
    pub id: egui::Id,
    /// A file chip clicked during `show`.
    pub(crate) opened_file: Option<FileRef>,
    /// A citation chip clicked during `show`.
    pub(crate) opened_citation: Option<Citation>,
    /// A cited quote to mark and its strength.
    pub(crate) highlight: Option<(String, f32)>,
    /// Find in thread's marks on this entry.
    pub(crate) find: Option<FindMarks>,
    /// The lists and quotes around the block being drawn (copies).
    prefixes: Vec<md_copy::Prefix>,
    /// Parent-folder suffixes of file chips sharing a basename.
    suffixes: Arc<HashMap<String, String>>,
    /// Inside a quote, text takes the muted color.
    quoted: bool,
    /// The workspace and thread images resolve against.
    pub(crate) media_root: Option<String>,
    pub(crate) media_thread: Option<String>,
    /// The folder relative images resolve against (`imageBaseDir`; the
    /// workspace when unset).
    pub(crate) image_base: Option<String>,
    /// Signed URLs to ask for: (cache key, `assets.createUrl` payload).
    pub(crate) asset_requests: Vec<(String, serde_json::Value)>,
    /// An image clicked (expanded preview) or right-clicked (its menu).
    pub(crate) opened_media: Option<(crate::ui::media::Source, Arc<image::RgbaImage>)>,
    pub(crate) media_menu: Option<(crate::ui::media::Source, Pos2)>,
    /// User messages: line breaks kept, the text in this color, known
    /// `$skills` as chips.
    /// A context chip clicked during `show` (its context id).
    pub(crate) opened_context: Option<String>,
    /// The message's context records, for its chips' display kinds.
    pub(crate) context_records: Vec<serde_json::Value>,
    pub(crate) line_breaks: bool,
    pub(crate) text_color: Option<Color32>,
    pub(crate) skill_chips: bool,
    /// Code blocks start wrapped (the `wordWrap` setting).
    pub(crate) word_wrap: bool,
    /// An answer in a project: a shell command can run in its terminal,
    /// and the one clicked lands here.
    pub(crate) can_run_shell: bool,
    pub(crate) run_command: Option<String>,
    /// How far right the drawn content reached from the column's left, for a
    /// bubble that fits its text.
    pub(crate) used_width: f32,
    origin_x: f32,
}

impl<'a> Markdown<'a> {
    pub fn new(highlighter: &'a mut Highlighter, id: egui::Id) -> Self {
        Self {
            highlighter,
            id,
            opened_file: None,
            opened_citation: None,
            highlight: None,
            find: None,
            prefixes: Vec::new(),
            suffixes: Arc::default(),
            quoted: false,
            media_root: None,
            media_thread: None,
            image_base: None,
            asset_requests: Vec::new(),
            opened_media: None,
            media_menu: None,
            opened_context: None,
            context_records: Vec::new(),
            line_breaks: false,
            text_color: None,
            skill_chips: false,
            word_wrap: true,
            can_run_shell: false,
            run_command: None,
            used_width: 0.0,
            origin_x: 0.0,
        }
    }

    fn note_width(&mut self, right: f32) {
        self.used_width = self.used_width.max(right - self.origin_x);
    }

    fn open_chip(&mut self, chip: ChipRef) {
        match chip {
            ChipRef::File(file) => self.opened_file = Some(file),
            ChipRef::Citation(citation) => self.opened_citation = Some(citation),
            ChipRef::Context { id, .. } => self.opened_context = Some(id),
        }
    }

    /// Renders `text` into the column starting at `ui.cursor()`; returns its height.
    pub fn show(&mut self, ui: &mut Ui, text: &str, width: f32) -> f32 {
        let (blocks, suffixes) = self.parsed(ui.ctx(), text);
        self.suffixes = suffixes;
        self.origin_x = ui.cursor().left();
        self.used_width = 0.0;
        let top = ui.cursor().top();
        self.blocks(ui, &blocks, width, 0, self.id);
        ui.cursor().top() - top
    }

    /// `text`'s blocks and chip suffixes, parsed once while it stays on
    /// screen: a frame that redraws a thread parses only what changed.
    fn parsed(&self, ctx: &egui::Context, text: &str) -> Parsed {
        use std::hash::{Hash, Hasher};
        thread_local! {
            static PARSED: std::cell::RefCell<HashMap<u64, (Parsed, u64)>> =
                Default::default();
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (text, self.line_breaks, self.skill_chips).hash(&mut hasher);
        for record in &self.context_records {
            record.to_string().hash(&mut hasher);
        }
        let key = hasher.finish();
        let frame = ctx.cumulative_frame_nr();
        if let Some(parsed) = PARSED.with_borrow_mut(|cache| {
            cache.get_mut(&key).map(|(parsed, used)| {
                *used = frame;
                parsed.clone()
            })
        }) {
            return parsed;
        }
        let mut blocks = parse_with(text, self.line_breaks);
        if self.skill_chips {
            skill_chip_blocks(&mut blocks);
        }
        if !self.context_records.is_empty() {
            context_display_kinds(&mut blocks, &self.context_records);
        }
        let mut files = Vec::new();
        collect_files(&blocks, &mut files);
        let parsed = (
            Arc::new(blocks),
            Arc::new(md_links::parent_suffixes(&files)),
        );
        PARSED.with_borrow_mut(|cache| {
            // Past this many, what the last frames did not draw goes.
            if cache.len() >= 512 {
                cache.retain(|_, (_, used)| *used + 1 >= frame);
            }
            cache.insert(key, (parsed.clone(), frame));
        });
        parsed
    }

    /// Block ids nest under `parent` so list items and quotes never collide.
    fn blocks(
        &mut self,
        ui: &mut Ui,
        blocks: &[Block],
        width: f32,
        depth: usize,
        parent: egui::Id,
    ) {
        let mut previous_bottom: Option<f32> = None;
        for (index, block) in blocks.iter().enumerate() {
            let (top, bottom) = margins(block);
            if let Some(previous) = previous_bottom {
                ui.add_space(previous.max(top));
            }
            let id = parent.with(index);
            // Code, tables, images and rules take the column's width.
            if matches!(
                block,
                Block::Code { .. } | Block::Table { .. } | Block::Image { .. } | Block::Rule
            ) {
                self.note_width(ui.cursor().left() + width);
            }
            self.block(ui, block, width, depth, id);
            previous_bottom = Some(bottom);
        }
    }

    fn block(&mut self, ui: &mut Ui, block: &Block, width: f32, depth: usize, id: egui::Id) {
        // `.chat-markdown blockquote`: `color: var(--contrast-muted-foreground)`.
        let paragraph_color = if self.quoted {
            theme::muted_foreground()
        } else {
            self.text_color
                .unwrap_or(theme::alpha(theme::foreground(), 0.8))
        };
        match block {
            Block::Paragraph(inlines) => {
                let laid = layout_inlines(
                    ui,
                    inlines,
                    BODY,
                    BODY_LINE,
                    Weight::Regular,
                    paragraph_color,
                    width,
                    &self.suffixes,
                );
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, laid.galley.size().y), Sense::hover());
                self.note_width(rect.left() + laid.galley.size().x);
                if let Some(chip) = paint_inlines(
                    ui,
                    rect.min,
                    &laid,
                    id,
                    self.highlight.as_ref(),
                    self.find.as_mut(),
                    Some((md_copy::Kind::Paragraph, self.prefixes.clone())),
                ) {
                    self.open_chip(chip);
                }
            }
            Block::Heading(level, inlines) => {
                let size = match level {
                    1 => 20.0,
                    2 => 18.0,
                    3 => 16.0,
                    _ => 14.0,
                };
                let color = if *level == 6 {
                    theme::muted_foreground()
                } else {
                    theme::foreground()
                };
                let laid = layout_inlines(
                    ui,
                    inlines,
                    size,
                    (size * 1.3).round_ui(),
                    Weight::SemiBold,
                    color,
                    width,
                    &self.suffixes,
                );
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, laid.galley.size().y), Sense::hover());
                self.note_width(rect.left() + laid.galley.size().x);
                if let Some(chip) = paint_inlines(
                    ui,
                    rect.min,
                    &laid,
                    id,
                    self.highlight.as_ref(),
                    self.find.as_mut(),
                    Some((md_copy::Kind::Heading(*level), self.prefixes.clone())),
                ) {
                    self.open_chip(chip);
                }
            }
            Block::List { start, items } => {
                for (number, item) in items.iter().enumerate() {
                    if number > 0 {
                        ui.add_space(4.0); // li + li
                    }
                    let top = ui.cursor().top();
                    let left = ui.cursor().left();
                    self.prefixes.push(md_copy::Prefix::Item {
                        id: id.with(number),
                        list: id,
                        marker: match start {
                            Some(first) => format!("{}. ", first + number as u64),
                            None => "- ".to_owned(),
                        },
                    });
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(Rect::from_min_size(
                            Pos2::new(left + 20.0, top),
                            Vec2::new(width - 20.0, f32::INFINITY),
                        )),
                        |ui| self.blocks(ui, item, width - 20.0, depth + 1, id.with(number)),
                    );
                    self.prefixes.pop();
                    let marker_center = Pos2::new(left + 6.5, top + BODY_LINE / 2.0);
                    match start {
                        Some(first) => {
                            let label = format!("{}.", first + number as u64);
                            let galley = widgets::galley(
                                ui,
                                &label,
                                font(BODY, Weight::Regular),
                                paragraph_color,
                                BODY_LINE,
                                f32::INFINITY,
                            );
                            let x = left + 20.0 - 6.0 - galley.size().x;
                            ui.painter()
                                .galley(Pos2::new(x, top), galley, paragraph_color);
                        }
                        None => match depth {
                            0 => {
                                ui.painter()
                                    .circle_filled(marker_center, 2.6, paragraph_color);
                            }
                            1 => {
                                ui.painter().circle_stroke(
                                    marker_center,
                                    2.3,
                                    Stroke::new(1.0, paragraph_color),
                                );
                            }
                            _ => {
                                ui.painter().rect_filled(
                                    Rect::from_center_size(marker_center, Vec2::splat(4.5)),
                                    0.0,
                                    paragraph_color,
                                );
                            }
                        },
                    }
                }
            }
            Block::Code {
                language,
                code,
                meta,
            } => self.code_block(ui, language, code, meta, width, id),
            Block::Table {
                alignments,
                head,
                rows,
            } => self.table(ui, alignments, head, rows, width, id),
            Block::Quote(blocks) => {
                let top = ui.cursor().top();
                let left = ui.cursor().left();
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(Rect::from_min_size(
                        Pos2::new(left + 14.8, top),
                        Vec2::new(width - 14.8, f32::INFINITY),
                    )),
                    |ui| {
                        let quoted = std::mem::replace(&mut self.quoted, true);
                        self.prefixes.push(md_copy::Prefix::Quote);
                        self.blocks(ui, blocks, width - 14.8, depth + 1, id.with("quote"));
                        self.prefixes.pop();
                        self.quoted = quoted;
                    },
                );
                let bottom = ui.cursor().top();
                ui.painter().rect_filled(
                    Rect::from_min_max(Pos2::new(left, top), Pos2::new(left + 2.0, bottom)),
                    0.0,
                    theme::border(),
                );
            }
            Block::Alert(kind, blocks) => self.alert(ui, kind, blocks, width, depth, id),
            Block::Details {
                summary,
                open,
                blocks,
            } => self.details(ui, summary, *open, blocks, width, depth, id),
            Block::Rule => {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
                ui.painter().rect_filled(rect, 0.0, theme::border());
            }
            Block::Image { src, alt } => self.image_block(ui, src, alt, width, id),
        }
    }

    /// A GitHub alert (`MarkdownBlockquote` with `data-alert`): a colored
    /// left border, the kind's icon and title, then its text, not muted.
    fn alert(
        &mut self,
        ui: &mut Ui,
        kind: &str,
        blocks: &[Block],
        width: f32,
        depth: usize,
        id: egui::Id,
    ) {
        let dark = theme::palette().dark;
        // Tailwind's 500 borders at 70%, the titles' 400 (dark) or 600.
        let (label, icon, border, title) = match kind {
            "tip" => (
                "Tip",
                "lightbulb",
                0x10b981,
                if dark { 0x34d399 } else { 0x059669 },
            ),
            "important" => (
                "Important",
                "message-square-warning",
                0xa855f7,
                if dark { 0xc084fc } else { 0x9333ea },
            ),
            "warning" => (
                "Warning",
                "triangle-alert",
                0xf59e0b,
                if dark { 0xf59e0b } else { 0xd97706 },
            ),
            "caution" => (
                "Caution",
                "octagon-alert",
                0xef4444,
                if dark { 0xf87171 } else { 0xdc2626 },
            ),
            _ => (
                "Note",
                "info",
                0x3b82f6,
                if dark { 0x60a5fa } else { 0x2563eb },
            ),
        };
        let rgb = |hex: u32| Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
        let top = ui.cursor().top();
        let left = ui.cursor().left();
        let inner = width - 14.0;
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(Rect::from_min_size(
                Pos2::new(left + 14.0, top),
                Vec2::new(inner, f32::INFINITY),
            )),
            |ui| {
                let (row, _) = ui.allocate_exact_size(Vec2::new(inner, BODY_LINE), Sense::hover());
                theme::paint_icon(
                    ui,
                    icon,
                    Rect::from_min_size(
                        Pos2::new(row.left(), row.center().y - 7.0),
                        Vec2::splat(14.0),
                    ),
                    14.0,
                    rgb(title),
                );
                widgets::text(
                    ui,
                    Pos2::new(row.left() + 14.0 + 6.0, row.top()),
                    label,
                    font(BODY, Weight::Medium),
                    rgb(title),
                    BODY_LINE,
                );
                ui.add_space(4.0);
                let quoted = std::mem::replace(&mut self.quoted, false);
                self.blocks(ui, blocks, inner, depth + 1, id.with("alert"));
                self.quoted = quoted;
            },
        );
        let bottom = ui.cursor().top();
        ui.painter().rect_filled(
            Rect::from_min_max(Pos2::new(left, top), Pos2::new(left + 2.0, bottom)),
            0.0,
            theme::alpha(rgb(border), 0.7),
        );
    }

    /// `MarkdownDetails`: a full-width row between hairlines, its chevron
    /// turning as the summary opens the indented content.
    #[allow(clippy::too_many_arguments)]
    fn details(
        &mut self,
        ui: &mut Ui,
        summary: &[Inline],
        open: bool,
        blocks: &[Block],
        width: f32,
        depth: usize,
        id: egui::Id,
    ) {
        let state = id.with("details-open");
        let expanded = ui.data(|d| d.get_temp::<bool>(state)).unwrap_or(open);
        let left = ui.cursor().left();
        let top = ui.cursor().top();
        let laid = layout_inlines(
            ui,
            summary,
            BODY,
            20.0,
            Weight::Medium,
            theme::foreground(),
            width - 24.0,
            &self.suffixes,
        );
        let height = laid.galley.size().y.max(20.0) + 16.0;
        let (row, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
        self.note_width(row.right());
        // The summary's selectable text takes the press: a click (no drag)
        // anywhere on the row still toggles it.
        let clicked = response.clicked()
            || (ui.rect_contains_pointer(row) && ui.input(|i| i.pointer.primary_clicked()));
        if clicked {
            ui.data_mut(|d| d.insert_temp(state, !expanded));
        }
        if ui.rect_contains_pointer(row) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let chevron = if expanded {
            "chevron-down"
        } else {
            "chevron-right"
        };
        theme::paint_icon(
            ui,
            chevron,
            Rect::from_min_size(Pos2::new(left, row.top() + 10.0), Vec2::splat(16.0)),
            16.0,
            theme::muted_foreground(),
        );
        if let Some(chip) = paint_inlines(
            ui,
            Pos2::new(left + 24.0, row.top() + 8.0),
            &laid,
            id.with("summary"),
            self.highlight.as_ref(),
            self.find.as_mut(),
            Some((md_copy::Kind::Paragraph, self.prefixes.clone())),
        ) {
            self.open_chip(chip);
        }
        if expanded {
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(Rect::from_min_size(
                    Pos2::new(left + 24.0, row.bottom()),
                    Vec2::new(width - 24.0, f32::INFINITY),
                )),
                |ui| self.blocks(ui, blocks, width - 24.0, depth + 1, id.with("details")),
            );
            ui.add_space(12.0);
        }
        let bottom = ui.cursor().top();
        let line = theme::alpha(theme::border(), 0.6);
        let painter = ui.painter();
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(left, top), Vec2::new(width, 1.0)),
            0.0,
            line,
        );
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(left, bottom - 1.0), Vec2::new(width, 1.0)),
            0.0,
            line,
        );
    }

    /// `ChatMarkdownImage` / `ChatMarkdownAssetImage`: a host file through a
    /// signed asset URL, framed (`rounded-lg border border-border/40`), or a
    /// direct URL, bare; at its natural size within 30rem and the column. A
    /// 16:9 slot (or the size the server read) holds its place while it
    /// loads and says "Image unavailable" if it fails. A click expands it;
    /// a right click opens its menu.
    fn image_block(&mut self, ui: &mut Ui, src: &str, alt: &str, width: f32, id: egui::Id) {
        use crate::ui::attachments::{self, MediaState};
        use crate::ui::media::{self, ImageSource};
        const MAX: f32 = 480.0; // 30rem
        let classified = media::classify(
            src,
            self.image_base.as_deref().or(self.media_root.as_deref()),
        );
        let (key, reference, framed) = match &classified {
            ImageSource::Direct(uri) => {
                (format!("md-direct:{uri}"), media::url_reference(uri), false)
            }
            ImageSource::File(path) => (
                format!(
                    "md-file:{}:{path}",
                    self.media_thread.as_deref().unwrap_or_default()
                ),
                Some(media::file_reference(path, self.media_root.as_deref())),
                true,
            ),
            ImageSource::Blocked => {
                self.image_unavailable_chip(ui, alt);
                return;
            }
        };
        if attachments::media_state(&key).is_none() {
            match &classified {
                ImageSource::Direct(uri) => attachments::load_direct(&key, uri, ui.ctx()),
                ImageSource::File(path) => {
                    if let Some(thread) = &self.media_thread {
                        attachments::mark_loading(&key);
                        self.asset_requests.push((
                            key.clone(),
                            serde_json::json!({"resource": {
                                "_tag": "media-file", "threadId": thread, "path": path,
                            }}),
                        ));
                    }
                }
                ImageSource::Blocked => {}
            }
        }
        let state = attachments::media_state(&key);
        let natural = match &state {
            Some(MediaState::Ready(image)) => Some((image.width(), image.height())),
            _ => attachments::image_size(&key),
        };
        let limit = width.min(MAX);
        let size = natural.map_or(Vec2::new(limit, (limit * 9.0 / 16.0).round()), |(w, h)| {
            let (w, h) = (w.max(1) as f32, h.max(1) as f32);
            let scale = 1.0_f32.min(limit / w).min(MAX / h);
            Vec2::new((w * scale).round(), (h * scale).round())
        });
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, size.y), Sense::hover());
        let rect = Rect::from_min_size(row.min, size);
        let radius = CornerRadius::same(8);
        let border = egui::Stroke::new(1.0, theme::alpha(theme::border(), 0.4));
        let source = media::Source {
            name: if alt.trim().is_empty() {
                "image".into()
            } else {
                alt.trim().to_owned()
            },
            reference,
            key: key.clone(),
            local_file: None,
        };
        let ready = match state {
            Some(MediaState::Ready(image)) => {
                if framed {
                    crate::ui::attachments::paint_cover(
                        ui,
                        rect.shrink(1.0),
                        &key,
                        &image,
                        CornerRadius::same(7),
                    );
                    ui.painter()
                        .rect_stroke(rect, radius, border, egui::StrokeKind::Inside);
                } else {
                    crate::ui::attachments::paint_cover(ui, rect, &key, &image, CornerRadius::ZERO);
                }
                Some(image)
            }
            state => {
                ui.painter()
                    .rect_filled(rect, radius, theme::alpha(theme::muted(), 0.6));
                ui.painter()
                    .rect_stroke(rect, radius, border, egui::StrokeKind::Inside);
                if matches!(state, Some(MediaState::Failed) | None) {
                    let label = if alt.is_empty() {
                        "Image unavailable".to_owned()
                    } else {
                        format!("Image unavailable · {alt}")
                    };
                    let label_font = theme::font(12.0, theme::Weight::Regular);
                    let text_width = widgets::text_width(ui, &label, label_font.clone())
                        .min(rect.width() - 38.0);
                    let left = rect.center().x - (text_width + 20.0) / 2.0;
                    theme::paint_icon(
                        ui,
                        "triangle-alert",
                        Rect::from_min_size(
                            Pos2::new(left, rect.center().y - 7.0),
                            Vec2::splat(14.0),
                        ),
                        14.0,
                        theme::muted_foreground(),
                    );
                    widgets::text_in(
                        ui,
                        Rect::from_min_size(
                            Pos2::new(left + 20.0, rect.center().y - 10.0),
                            Vec2::new(text_width + 1.0, 20.0),
                        ),
                        &label,
                        label_font,
                        theme::muted_foreground(),
                    );
                }
                None
            }
        };
        let response = ui
            .interact(rect, id.with("image"), Sense::click())
            .on_hover_text(source.tooltip().to_owned());
        if ready.is_some() && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomIn);
        }
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.media_menu = Some((source.clone(), at));
        }
        if response.clicked()
            && let Some(image) = ready
        {
            self.opened_media = Some((source, image));
        }
    }

    /// `ChatMarkdownImageFallback`: an image that can never load, as an
    /// inline chip (`rounded-md border border-border/40 bg-muted/40 px-2
    /// py-1 text-xs`).
    fn image_unavailable_chip(&mut self, ui: &mut Ui, alt: &str) {
        let label = if alt.is_empty() {
            "Image unavailable".to_owned()
        } else {
            format!("Image unavailable · {alt}")
        };
        let label_font = theme::font(12.0, theme::Weight::Regular);
        let text_width = widgets::text_width(ui, &label, label_font.clone());
        let (row, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
        let chip = Rect::from_min_size(row.min, Vec2::new(text_width + 16.0 + 20.0, 26.0));
        ui.painter().rect_filled(
            chip,
            CornerRadius::same(6),
            theme::alpha(theme::muted(), 0.4),
        );
        ui.painter().rect_stroke(
            chip,
            CornerRadius::same(6),
            egui::Stroke::new(1.0, theme::alpha(theme::border(), 0.4)),
            egui::StrokeKind::Inside,
        );
        theme::paint_icon(
            ui,
            "triangle-alert",
            Rect::from_min_size(
                Pos2::new(chip.left() + 8.0, chip.center().y - 7.0),
                Vec2::splat(14.0),
            ),
            14.0,
            theme::muted_foreground(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(chip.left() + 28.0, chip.top()), chip.max),
            &label,
            label_font,
            theme::muted_foreground(),
        );
    }

    fn code_block(
        &mut self,
        ui: &mut Ui,
        language: &str,
        code: &str,
        meta: &str,
        width: f32,
        id: egui::Id,
    ) {
        let line_height = theme::code_line(17.875); // 13px × leading-snug
        let inner_width = width - 2.0 - 28.8;
        // Wrapped by default (`wordWrap`), each block on its own after a
        // click; unwrapped lines scroll sideways.
        let wrap_id = id.with("wrap");
        let wrapped = ui
            .data(|d| d.get_temp::<bool>(wrap_id))
            .unwrap_or(self.word_wrap);
        let mut job = LayoutJob::default();
        // Plain until the worker has highlighted it: the same glyphs, so
        // nothing moves when the colors arrive.
        let plain = [(code.to_owned(), crate::highlight::plain())];
        let spans = self.highlighter.spans(code, language).unwrap_or(&plain);
        for (text, color) in spans {
            job.append(
                text,
                0.0,
                widgets::format(theme::code_font(), *color, line_height),
            );
        }
        job.wrap.max_width = if wrapped { inner_width } else { f32::INFINITY };
        let galley = widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job)));
        let height = 1.0 + 30.0 + 12.8 + galley.size().y + 12.8 + 1.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), theme::control_surface());
        // Header: the fence's title (or its language), then Wrap, Run, Copy.
        let header_color = theme::alpha(theme::code_foreground(), 0.72);
        let header = Rect::from_min_size(
            rect.min + Vec2::new(13.0, 7.0),
            Vec2::new((width - 13.0 - 100.0).max(40.0), 24.0),
        );
        if let Some(title) = fence_title(meta) {
            let (icon, color) = crate::ui::files::file_icon(&title);
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(header.min, Vec2::new(14.0, 24.0)),
                14.0,
                color,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(header.min + Vec2::new(20.0, 0.0), header.max),
                &title,
                theme::mono(11.0),
                header_color,
            );
        } else if let Some((icon, color)) = language_icon(language) {
            let icon_rect = Rect::from_min_size(header.min, Vec2::new(14.0, 24.0));
            theme::paint_icon(ui, icon, icon_rect, 14.0, color);
            ui.interact(icon_rect, id.with("language"), Sense::hover())
                .on_hover_text(language);
        } else {
            widgets::text_in(ui, header, language, theme::mono(11.0), header_color);
        }
        let copy = Rect::from_min_size(
            Pos2::new(rect.right() - 31.0, rect.top() + 7.0),
            Vec2::splat(24.0),
        );
        let mut left = copy.left() - 2.0;
        // "Run in terminal": a single closed shell command in an answer.
        if self.can_run_shell && runnable_shell(language, code) {
            let run = Rect::from_min_size(Pos2::new(left - 24.0, copy.top()), Vec2::splat(24.0));
            if widgets::icon_button(
                ui,
                run,
                id.with("run"),
                "play",
                12.0,
                theme::muted_foreground(),
                "Run in terminal",
            )
            .clicked()
            {
                self.run_command = Some(code.trim().to_owned());
            }
            left = run.left() - 2.0;
        }
        let wrap = Rect::from_min_size(Pos2::new(left - 24.0, copy.top()), Vec2::splat(24.0));
        let wrap_response = ui
            .interact(wrap, id.with("wrap-toggle"), Sense::click())
            .on_hover_text(if wrapped {
                "Disable line wrap"
            } else {
                "Wrap lines"
            });
        if wrapped {
            ui.painter()
                .rect_filled(wrap, CornerRadius::same(8), theme::muted());
        } else if wrap_response.hovered() {
            ui.painter()
                .rect_filled(wrap, CornerRadius::same(8), theme::accent());
        }
        theme::paint_icon(
            ui,
            "text-wrap",
            wrap,
            12.0,
            if wrapped {
                theme::foreground()
            } else {
                theme::muted_foreground()
            },
        );
        if wrap_response.clicked() {
            ui.data_mut(|d| d.insert_temp(wrap_id, !wrapped));
        }
        widgets::copy_button(
            ui,
            copy,
            id.with("copy"),
            "Copy code",
            "Copied",
            theme::muted_foreground(),
            1.2,
            false,
            || code.to_owned(),
        );
        let pos = rect.min + Vec2::new(1.0 + 14.4, 1.0 + 30.0 + 12.8);
        if wrapped || galley.size().x <= inner_width {
            let marks = self.find.as_mut().map(|find| find.mark(ui, pos, &galley));
            widgets::selectable_galley(
                ui,
                id.with("code"),
                pos,
                galley.clone(),
                theme::foreground(),
            );
            if let Some(marks) = &marks {
                FindMarks::recolor(ui, pos, &galley, marks);
            }
        } else {
            // Long lines scroll sideways inside the block.
            let area = Rect::from_min_size(pos, Vec2::new(inner_width, galley.size().y + 8.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
            egui::ScrollArea::horizontal()
                .id_salt(id.with("scroll"))
                .show(&mut child, |ui| {
                    let (inner, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
                    widgets::selectable_galley(
                        ui,
                        id.with("code"),
                        inner.min,
                        galley.clone(),
                        theme::foreground(),
                    );
                });
        }
        ui.ctx()
            .plugin::<widgets::CopyCleanup>()
            .lock()
            .recipes
            .insert(
                id.with("code"),
                Arc::new(md_copy::Recipe {
                    kind: md_copy::Kind::Code {
                        language: language.to_owned(),
                    },
                    prefixes: self.prefixes.clone(),
                    segments: Vec::new(),
                }),
            );
    }

    fn table(
        &mut self,
        ui: &mut Ui,
        alignments: &[Alignment],
        head: &[Vec<Inline>],
        rows: &[Vec<Vec<Inline>>],
        width: f32,
        id: egui::Id,
    ) {
        let color = theme::alpha(theme::foreground(), 0.8);
        let columns = head.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
        if columns == 0 {
            return;
        }
        let cell_line = 19.5; // 12px × 1.625
        let suffixes = self.suffixes.clone();
        let highlight = self.highlight.clone();
        let mut find = self.find.take();
        let mut opened = None;
        let measure = |ui: &Ui, inlines: &[Inline], weight| {
            layout_inlines(
                ui,
                inlines,
                12.0,
                cell_line,
                weight,
                color,
                f32::INFINITY,
                &suffixes,
            )
            .galley
            .size()
            .x
        };
        // Auto table layout: columns share the width by their content width.
        let mut natural = vec![0.0f32; columns];
        for (i, cell) in head.iter().enumerate() {
            natural[i] = natural[i].max(measure(ui, cell, Weight::SemiBold) + 24.0);
        }
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                natural[i] = natural[i].max(measure(ui, cell, Weight::Regular) + 24.0);
            }
        }
        let total: f32 = natural.iter().sum();
        let widths: Vec<f32> = if total < width {
            natural
                .iter()
                .map(|w| w + (width - total) * (w / total))
                .collect()
        } else {
            natural.iter().map(|w| w * width / total).collect()
        };
        let border = theme::overlay(0.036);
        let prefixes = self.prefixes.clone();
        let mut row_index = 0;
        let mut paint_row = |ui: &mut Ui, cells: &[Vec<Inline>], header: bool| {
            let row = row_index;
            row_index += 1;
            let pad_y = if header { 8.8 } else { 7.2 };
            let weight = if header {
                Weight::SemiBold
            } else {
                Weight::Regular
            };
            let laid: Vec<Inlines> = (0..columns)
                .map(|i| {
                    let cell = cells.get(i).map(Vec::as_slice).unwrap_or(&[]);
                    layout_inlines(
                        ui,
                        cell,
                        12.0,
                        cell_line,
                        weight,
                        color,
                        widths[i] - 24.0,
                        &suffixes,
                    )
                })
                .collect();
            // CSS line boxes are fractional; epaint rounds each row to whole pixels.
            let lines = laid
                .iter()
                .map(|l| l.galley.rows.len())
                .max()
                .unwrap_or(1)
                .max(1);
            let height = lines as f32 * cell_line + pad_y * 2.0 + 1.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
            let mut x = rect.left();
            for (i, cell) in laid.iter().enumerate() {
                let cell_width = cell.galley.size().x;
                let offset = match alignments.get(i) {
                    Some(Alignment::Right) => widths[i] - 12.0 - cell_width,
                    Some(Alignment::Center) => (widths[i] - cell_width) / 2.0,
                    _ => 12.0,
                };
                if let Some(chip) = paint_inlines(
                    ui,
                    Pos2::new(x + offset, rect.top() + pad_y),
                    cell,
                    id.with(("cell", header, i, rect.top() as i32)),
                    highlight.as_ref(),
                    find.as_mut(),
                    Some((
                        md_copy::Kind::Cell {
                            table: id,
                            row,
                            alignment: alignments.get(i).copied().unwrap_or(Alignment::None),
                        },
                        prefixes.clone(),
                    )),
                ) {
                    opened = Some(chip);
                }
                x += widths[i];
            }
            ui.painter().rect_filled(
                Rect::from_min_size(
                    Pos2::new(rect.left(), (rect.bottom() - 0.5).round()),
                    Vec2::new(rect.width(), 1.0),
                ),
                0.0,
                border,
            );
        };
        if !head.is_empty() {
            paint_row(ui, head, true);
        }
        for row in rows {
            paint_row(ui, row, false);
        }
        self.find = find;
        if let Some(chip) = opened {
            self.open_chip(chip);
        }
        // Toolbar: expand toggle (expanded by default) and copy.
        ui.add_space(2.0);
        let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
        let expand = Rect::from_min_size(bar.min, Vec2::splat(24.0));
        ui.painter()
            .rect_filled(expand, CornerRadius::same(8), theme::muted());
        theme::paint_icon(ui, "minimize-2", expand, 12.0, theme::foreground());
        // Copy opens "Copy as Markdown" / "Copy as CSV"; the check shows 1.2s.
        let copy = Rect::from_min_size(Pos2::new(bar.right() - 24.0, bar.top()), Vec2::splat(24.0));
        let menu_id = format!("table-copy-{id:?}");
        let copied_key = id.with("table-copied");
        let open_key = id.with("table-menu");
        let now = ui.input(|i| i.time);
        let copied = ui
            .data(|d| d.get_temp::<f64>(copied_key))
            .map(|at| 1.2 - (now - at))
            .filter(|left| *left > 0.0);
        if let Some(left) = copied {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(left));
        }
        let mut open: Option<String> = ui.data(|d| d.get_temp(open_key)).flatten();
        if widgets::icon_button(
            ui,
            copy,
            id.with("table-copy"),
            if copied.is_some() { "check" } else { "copy" },
            12.0,
            theme::muted_foreground(),
            if copied.is_some() {
                "Copied"
            } else {
                "Copy table"
            },
        )
        .clicked()
        {
            open = if open.is_some() {
                None
            } else {
                Some(menu_id.clone())
            };
        }
        let entries = [
            widgets::MenuEntry::Item(widgets::MenuItem::new("Copy as Markdown")),
            widgets::MenuEntry::Item(widgets::MenuItem::new("Copy as CSV")),
        ];
        if let Some(choice) =
            widgets::menu_aligned(ui, &mut open, &menu_id, copy, false, true, &entries)
        {
            let text = if choice == 0 {
                table_markdown(alignments, head, rows)
            } else {
                table_csv(head, rows)
            };
            ui.ctx().copy_text(text);
            ui.data_mut(|d| d.insert_temp(copied_key, now));
            open = None;
        }
        ui.data_mut(|d| d.insert_temp(open_key, open));
    }
}

/// `serializeTableElementToMarkdown`: pipe rows with the alignment separator
/// after the header, cells as inline markdown on one line.
fn table_markdown(
    alignments: &[Alignment],
    head: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
) -> String {
    let cell = |inlines: &[Inline]| {
        inline_markdown(inlines)
            .split('\n')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .replace('|', "\\|")
    };
    let row = |cells: &[Vec<Inline>]| {
        let cells: Vec<String> = cells.iter().map(|c| cell(c)).collect();
        format!("| {} |", cells.join(" | "))
    };
    let separator: Vec<&str> = (0..head.len())
        .map(|i| match alignments.get(i) {
            Some(Alignment::Center) => ":---:",
            Some(Alignment::Right) => "---:",
            _ => "---",
        })
        .collect();
    std::iter::once(row(head))
        .chain(std::iter::once(format!("| {} |", separator.join(" | "))))
        .chain(rows.iter().map(|r| row(r)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `serializeTableElementToCsv`: the cells' text, quoted when needed.
fn table_csv(head: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) -> String {
    let cell = |inlines: &[Inline]| {
        let text = plain(inlines)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if text.contains(['"', ',', '\n']) {
            format!("\"{}\"", text.replace('"', "\"\""))
        } else {
            text
        }
    };
    std::iter::once(head)
        .chain(rows.iter().map(Vec::as_slice))
        .map(|cells| cells.iter().map(|c| cell(c)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Inline markdown for copying (`serializeNode`): emphasis markers, links
/// with an http(s) target, code spans; file chips as their code span.
pub(crate) fn inline_markdown(inlines: &[Inline]) -> String {
    // `wrapInlineMarker`: surrounding whitespace stays outside the markers.
    let wrap = |text: &str, marker: &str| {
        let core = text.trim();
        if core.is_empty() {
            return text.to_owned();
        }
        let start = &text[..text.len() - text.trim_start().len()];
        let end = &text[text.trim_end().len()..];
        format!("{start}{marker}{core}{marker}{end}")
    };
    // `wrapInlineCode`: a fence longer than any backtick run inside.
    let code = |text: &str| {
        let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
        let fence = "`".repeat(if longest > 0 { longest + 1 } else { 1 });
        let pad = if text.starts_with('`') || text.ends_with('`') {
            " "
        } else {
            ""
        };
        format!("{fence}{pad}{text}{pad}{fence}")
    };
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text(text, style) => {
                let mut out = text.clone();
                if style.strike {
                    out = wrap(&out, "~~");
                }
                if style.italic {
                    out = wrap(&out, "*");
                }
                if style.bold {
                    out = wrap(&out, "**");
                }
                match &style.link {
                    Some(href) if href.starts_with("http://") || href.starts_with("https://") => {
                        let label = out.trim();
                        if label == href {
                            href.clone()
                        } else {
                            format!("[{label}]({href})")
                        }
                    }
                    _ => out,
                }
            }
            Inline::Code(text) => code(text),
            Inline::File(file) => code(&match file.line {
                Some(line) => format!("{}:{line}", file.path),
                None => file.path.clone(),
            }),
            Inline::Citation(citation) => citation.serialize(),
            Inline::Context { kind, id, .. } if kind == "skill" => format!("${id}"),
            Inline::Context { kind, label, id } => {
                // A pull request shows by its state; its link names the context kind.
                let kind = if kind.starts_with("pr-") {
                    "review-comment"
                } else {
                    kind
                };
                crate::ui::composer_context::format_reference(kind, id, label)
            }
            Inline::Break => "\n".to_owned(),
        })
        .collect()
}

fn plain(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|i| match i {
            Inline::Text(t, _) | Inline::Code(t) => t.clone(),
            Inline::File(file) => match file.line {
                Some(line) => format!("{}:{line}", file.path),
                None => file.path.clone(),
            },
            Inline::Citation(citation) => citation.text.clone(),
            Inline::Context { label, .. } => label.clone(),
            Inline::Break => " ".to_owned(),
        })
        .collect()
}

fn margins(block: &Block) -> (f32, f32) {
    match block {
        Block::Heading(..) => (20.0, 8.0),
        Block::Rule => (16.0, 16.0),
        Block::Alert(..) => (4.0, 4.0),
        Block::Details { .. } => (8.0, 8.0),
        _ => (BLOCK_MARGIN, BLOCK_MARGIN),
    }
}

trait RoundUi {
    fn round_ui(self) -> f32;
}

impl RoundUi for f32 {
    fn round_ui(self) -> f32 {
        (self * 10.0).round() / 10.0
    }
}

/// Pierre file-type icon for a fence language, with its dark-theme color
/// (`--trees-file-icon-color-*`). Languages without one show their name.
pub(crate) fn language_icon(language: &str) -> Option<(&'static str, Color32)> {
    let orange = Color32::from_rgb(0xff, 0xa3, 0x59);
    let yellow = Color32::from_rgb(0xff, 0xd4, 0x52);
    let green = Color32::from_rgb(0x5e, 0xcc, 0x71);
    let cyan = Color32::from_rgb(0x68, 0xcd, 0xf2);
    let blue = Color32::from_rgb(0x69, 0xb1, 0xff);
    let indigo = Color32::from_rgb(0x9d, 0x6a, 0xfb);
    let purple = Color32::from_rgb(0xd5, 0x68, 0xea);
    let pink = Color32::from_rgb(0xff, 0x67, 0x8d);
    let red = Color32::from_rgb(0xff, 0x67, 0x62);
    Some(match language.to_lowercase().as_str() {
        "rust" | "rs" => ("lang-rust", orange),
        "ts" | "typescript" | "mts" | "cts" => ("lang-typescript", blue),
        "js" | "javascript" | "mjs" | "cjs" => ("lang-javascript", yellow),
        "tsx" | "jsx" => ("lang-react", cyan),
        "py" | "python" => ("lang-python", blue),
        "sh" | "bash" | "zsh" | "shell" | "console" => ("lang-bash", green),
        "json" | "jsonc" => ("lang-json", orange),
        "md" | "markdown" => ("lang-markdown", green),
        "css" => ("lang-css", indigo),
        "html" => ("lang-html", orange),
        "go" => ("lang-go", cyan),
        "c" | "h" => ("lang-c", blue),
        "cpp" | "c++" | "cc" | "hpp" => ("lang-cpp", blue),
        "rb" | "ruby" => ("lang-ruby", red),
        "swift" => ("lang-swift", orange),
        "zig" => ("lang-zig", orange),
        "yml" | "yaml" => ("lang-yml", red),
        "sql" => ("lang-database", purple),
        "vue" => ("lang-vue", green),
        "svelte" => ("lang-svelte", red),
        "scss" | "sass" => ("lang-sass", pink),
        "graphql" | "gql" => ("lang-graphql", pink),
        "dockerfile" | "docker" => ("lang-docker", blue),
        "svg" => ("lang-svg", orange),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "ico" | "icns" | "tif"
        | "tiff" => ("lang-image", pink),
        _ => return None,
    })
}

/// `extractFenceTitle`: a `title=`/`file=`/`filename=` value, else the
/// first token that looks like a file name with an extension.
fn fence_title(meta: &str) -> Option<String> {
    let meta = meta.trim();
    if meta.is_empty() {
        return None;
    }
    let lower = meta.to_ascii_lowercase();
    for key in ["title=", "filename=", "file="] {
        let mut from = 0;
        while let Some(found) = lower[from..].find(key) {
            let at = from + found;
            // The key starts a token.
            if at == 0 || meta[..at].ends_with(char::is_whitespace) {
                let value = &meta[at + key.len()..];
                let parsed = match value.chars().next() {
                    Some(q @ ('"' | '\'')) => value[1..].split(q).next().map(str::to_owned),
                    _ => value.split_whitespace().next().map(str::to_owned),
                };
                if let Some(title) = parsed.filter(|t| !t.is_empty()) {
                    return Some(title);
                }
            }
            from = at + key.len();
        }
    }
    meta.split_whitespace()
        .find(|token| {
            let first = token
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '@');
            let allowed = token
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '@' | '.' | '/' | '-'));
            let extension = token.rsplit_once('.').is_some_and(|(stem, ext)| {
                !stem.is_empty()
                    && !ext.is_empty()
                    && ext.chars().all(|c| c.is_ascii_alphanumeric())
            });
            first && allowed && extension
        })
        .map(str::to_owned)
}

/// A fence "Run in terminal" can take: a shell language and one plain
/// command line (no control or format characters, no continuation).
fn runnable_shell(language: &str, code: &str) -> bool {
    matches!(language, "sh" | "bash" | "zsh" | "fish" | "shell" | "powershell" | "pwsh")
        && !code.trim().is_empty()
        && !code.trim_end().ends_with('\\')
        && !code
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{ad}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2064}' | '\u{feff}'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(inlines: &[Inline]) -> Vec<(String, Option<String>)> {
        inlines
            .iter()
            .filter_map(|i| match i {
                Inline::Text(t, style) => Some((t.clone(), style.link.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_chip_after_a_soft_break_keeps_its_own_section() {
        // egui merges a section into the previous one of the same format:
        // the text and its soft break became one section, and the file chip
        // after them took the section of the space that follows its label.
        let blocks = parse_with("Here :\n`/fixture/out/pluto-arm64-v8a.apk`", false);
        let Block::Paragraph(inlines) = &blocks[0] else {
            panic!("{blocks:?}")
        };
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut laid = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            laid = Some(layout_inlines(
                ui,
                inlines,
                14.0,
                20.0,
                Weight::Regular,
                Color32::WHITE,
                600.0,
                &HashMap::new(),
            ));
        });
        output.textures_delta.clear();
        let laid = laid.unwrap();
        let text = &laid.galley.job.text;
        let at = text.find("pluto").unwrap();
        let section = char_sections(&laid.galley)[text[..at].chars().count()];
        assert!(
            matches!(laid.files.get(section), Some(Some(_))),
            "the label's section {section} is not the chip's: {:?}",
            laid.galley.job.sections
        );
    }

    #[test]
    fn alerts_footnotes_and_bare_urls_follow_gfm() {
        let blocks = parse_with(
            "> [!WARNING]\n> Careful.\n\nSee https://example.dev/a). Done[^1].\n\n[^1]: The note.",
            false,
        );
        assert!(matches!(&blocks[0], Block::Alert("warning", children) if children.len() == 1));
        let Block::Paragraph(inlines) = &blocks[1] else {
            panic!("{blocks:?}")
        };
        let links = texts(inlines);
        assert!(links.contains(&(
            "https://example.dev/a".into(),
            Some("https://example.dev/a".into())
        )));
        assert!(links.iter().any(|(t, _)| t == "1"));
        assert!(matches!(blocks[2], Block::Rule));
        assert!(matches!(&blocks[3], Block::List { start: Some(1), items } if items.len() == 1));
        // A definition nothing refers to is left out.
        let unreferenced = parse_with("Text.\n\n[^x]: Unused.", false);
        assert_eq!(unreferenced.len(), 1);
    }

    #[test]
    fn answers_keep_sanitized_html_and_user_text_shows_it() {
        let blocks = parse_with(
            "Press <kbd>Ctrl</kbd> <b>now</b><script>x</script>\n\n<details>\n<summary>More</summary>\n\nHidden text\n\n</details>",
            false,
        );
        let Block::Paragraph(inlines) = &blocks[0] else {
            panic!("{blocks:?}")
        };
        assert!(
            inlines
                .iter()
                .any(|i| matches!(i, Inline::Code(c) if c == "Ctrl"))
        );
        assert!(
            inlines
                .iter()
                .any(|i| matches!(i, Inline::Text(t, s) if t == "now" && s.bold))
        );
        assert!(!texts(inlines).iter().any(|(t, _)| t.contains('x')));
        assert!(
            matches!(&blocks[1], Block::Details { summary, open: false, blocks }
            if texts(summary)[0].0 == "More" && blocks.len() == 1)
        );
        let user = parse_with("a <b>b</b>", true);
        let Block::Paragraph(inlines) = &user[0] else {
            panic!()
        };
        assert!(texts(inlines).iter().any(|(t, _)| t == "<b>"));
    }

    #[test]
    fn fence_titles_and_runnable_commands() {
        assert_eq!(
            fence_title(r#"title="src/main.rs" lines"#).as_deref(),
            Some("src/main.rs")
        );
        assert_eq!(fence_title("file=app.ts").as_deref(), Some("app.ts"));
        assert_eq!(
            fence_title("src/lib/a.ts {1,3}").as_deref(),
            Some("src/lib/a.ts")
        );
        assert_eq!(fence_title("showLineNumbers"), None);
        assert!(runnable_shell("bash", "cargo test --workspace"));
        assert!(!runnable_shell("bash", "cd a\ncargo test"));
        assert!(!runnable_shell("bash", "make \\"));
        assert!(!runnable_shell("rust", "cargo test"));
        assert!(!runnable_shell("Bash", "ls"));
    }
}
