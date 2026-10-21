//! Assistant markdown (`ChatMarkdown.tsx` + `.chat-markdown` CSS): block
//! margins collapse like CSS, inline code gets its padded chip, fenced code
//! uses the pierre-dark highlighter, tables use the 12px compact layout.
//! File references become file chips and external links lead with their
//! site's icon (`md_links`).
use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, Galley, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
    text::LayoutJob,
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::highlight::Highlighter;
use crate::theme::{self, Weight, font};
use crate::ui::md_links::{self, FileRef};
use crate::ui::widgets;

const BODY: f32 = 14.0;
const BODY_LINE: f32 = 22.75; // 14px × 1.625
const BLOCK_MARGIN: f32 = 10.4; // .65rem
const INFO_FOREGROUND: Color32 = Color32::from_rgb(81, 162, 255);

#[derive(Clone, Debug, Default)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    link: Option<String>,
}

#[derive(Clone, Debug)]
enum Inline {
    Text(String, Style),
    Code(String),
    /// A file reference, drawn as a chip.
    File(FileRef),
    Break,
}

/// Inline code that names a file becomes its chip
/// (`resolveInlineCodeFileLinkMeta`).
fn code_inline(code: &str) -> Inline {
    md_links::inline_code_candidate(code)
        .and_then(|candidate| md_links::parse_file_link(&candidate))
        .map_or_else(|| Inline::Code(code.to_owned()), Inline::File)
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
            Block::Quote(blocks) => collect_files(blocks, out),
            Block::Table { head, rows, .. } => {
                head.iter().for_each(|cell| inlines(cell, out));
                rows.iter().flatten().for_each(|cell| inlines(cell, out));
            }
            Block::Code { .. } | Block::Rule => {}
        }
    }
}

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
    },
    Table {
        alignments: Vec<Alignment>,
        head: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Quote(Vec<Block>),
    Rule,
}

fn parse(text: &str) -> Vec<Block> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let events: Vec<Event> = Parser::new_ext(text, options).collect();
    let mut index = 0;
    parse_blocks(&events, &mut index, None)
}

fn parse_blocks(events: &[Event], index: &mut usize, until: Option<TagEnd>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut loose: Vec<Inline> = Vec::new();
    while *index < events.len() {
        let event = &events[*index];
        *index += 1;
        match event {
            Event::End(end) if Some(*end) == until => break,
            Event::Start(Tag::Paragraph) => blocks.push(Block::Paragraph(parse_inlines(
                events,
                index,
                TagEnd::Paragraph,
            ))),
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
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_owned()
                    }
                    CodeBlockKind::Indented => String::new(),
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
            Event::Start(Tag::BlockQuote(_)) => blocks.push(Block::Quote(parse_blocks(
                events,
                index,
                Some(TagEnd::BlockQuote(None)),
            ))),
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
    let mut depth = 0;
    while *index < events.len() {
        match &events[*index] {
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::Start(Tag::Strong) => style.bold = true,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::Start(Tag::Link { dest_url, .. }) => match md_links::parse_file_link(dest_url) {
                Some(file) => {
                    *index += 1;
                    skip_link(events, index);
                    out.push(Inline::File(file));
                    continue;
                }
                None => style.link = Some(dest_url.to_string()),
            },
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::End(TagEnd::Link) => style.link = None,
            Event::Text(t) => out.push(Inline::Text(t.to_string(), style.clone())),
            Event::Code(t) => out.push(code_inline(t)),
            Event::SoftBreak => out.push(Inline::Text(" ".into(), style.clone())),
            Event::HardBreak => out.push(Inline::Break),
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
    while *index < events.len() {
        let event = &events[*index];
        *index += 1;
        match event {
            Event::End(e) if *e == end => break,
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::Start(Tag::Strong) => style.bold = true,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::Start(Tag::Link { dest_url, .. }) => match md_links::parse_file_link(dest_url) {
                Some(file) => {
                    skip_link(events, index);
                    out.push(Inline::File(file));
                }
                None => style.link = Some(dest_url.to_string()),
            },
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::End(TagEnd::Link) => style.link = None,
            Event::Text(t) => out.push(Inline::Text(t.to_string(), style.clone())),
            Event::Code(t) => out.push(code_inline(t)),
            Event::SoftBreak => out.push(Inline::Text(" ".into(), style.clone())),
            Event::HardBreak => out.push(Inline::Break),
            Event::Html(t) | Event::InlineHtml(t) => {
                out.push(Inline::Text(t.to_string(), style.clone()))
            }
            _ => {}
        }
    }
    out
}

struct Inlines {
    galley: Arc<Galley>,
    /// Section index → inline code chip group.
    chips: Vec<Option<usize>>,
    /// Section index → file chip group and its file.
    files: Vec<Option<(usize, FileRef)>>,
    /// Section index → the icon a link's leading space holds.
    icons: Vec<Option<md_links::LinkIcon>>,
    links: Vec<Option<String>>,
    /// The text size (chip metrics are in em).
    size: f32,
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
                        files: &mut Vec<Option<(usize, FileRef)>>,
                        icons: &mut Vec<Option<md_links::LinkIcon>>,
                        links: &mut Vec<Option<String>>,
                        inserted: &mut Vec<usize>| {
        let original = chips.len() - inserted.len();
        if breaks.contains(&original) {
            inserted.push(chips.len());
            // The zero-width space marks the break as layout-only, so copies
            // drop it (`widgets::CopyCleanup`).
            job.append("\u{200B}\n", 0.0, newline.clone());
            chips.push(None);
            files.push(None);
            icons.push(None);
            links.push(None);
        }
    };
    let mut chip = 0;
    let mut file_chip = 0;
    let mut previous_link: Option<String> = None;
    for inline in inlines {
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
                let weight = if style.bold {
                    Weight::Bold
                } else {
                    base_weight
                };
                let mut format = widgets::format(font(size, weight), color, line_height);
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
                job.append(text, leading, format);
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
                job.append(
                    &label,
                    chip_lead(chip_size),
                    widgets::format(chip_font.clone(), text, line_height),
                );
                job.append(
                    "\u{a0}",
                    0.0,
                    spacer(ui, chip_font, 0.5 * chip_size + 1.0, line_height),
                );
                for _ in 0..2 {
                    chips.push(None);
                    files.push(Some((file_chip, file.clone())));
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
                job.append(" ", 0.0, pad.clone());
                job.append(
                    code,
                    0.0,
                    widgets::format(code_font, theme::foreground(), line_height),
                );
                job.append(" ", 0.0, pad);
                chips.extend([Some(chip), Some(chip), Some(chip)]);
                files.extend([None, None, None]);
                icons.extend([None, None, None]);
                links.extend([None, None, None]);
                chip += 1;
            }
            Inline::Break => {
                job.append(
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

/// Paints inline text, chip backgrounds and icons, and handles link clicks;
/// returns a file chip that was clicked.
fn paint_inlines(ui: &mut Ui, pos: Pos2, inlines: &Inlines, id: egui::Id) -> Option<FileRef> {
    let painter = ui.painter().clone();
    let sections = char_sections(&inlines.galley);
    let chip_size = inlines.size * 0.86;
    let [chip_border, chip_fill, _] =
        md_links::mention_colors(theme::border(), theme::foreground());
    let mut file_rects: Vec<(Rect, FileRef)> = Vec::new();
    // Rows hold one glyph per character; a trailing `\n` is omitted.
    let mut char_index = 0usize;
    for placed in &inlines.galley.rows {
        let mut run: Option<(usize, f32, f32)> = None;
        let mut file_run: Option<(usize, f32, f32, FileRef)> = None;
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
        let mut flush_file = |file_run: &mut Option<(usize, f32, f32, FileRef)>| {
            if let Some((_, left, right, file)) = file_run.take() {
                let height = (1.41 * chip_size).round();
                let top = row_top + ((row_height - height) / 2.0).round();
                let rect = Rect::from_min_max(
                    Pos2::new((row_x + left).round(), top),
                    Pos2::new((row_x + right).round(), top + height),
                );
                let radius = CornerRadius::same((0.5 * chip_size).round() as u8);
                painter.rect_filled(rect, radius, chip_fill);
                painter.rect_stroke(
                    rect,
                    radius,
                    Stroke::new(1.0, chip_border),
                    StrokeKind::Inside,
                );
                let icon_size = (1.17 * chip_size).round();
                let icon = Rect::from_min_size(
                    Pos2::new(
                        rect.left() + 1.0 + (0.5 * chip_size).round(),
                        rect.center().y - icon_size / 2.0,
                    ),
                    Vec2::splat(icon_size),
                );
                let directory = !md_links::basename(&file.path).contains('.');
                let (name, tint) = if directory {
                    ("folder", theme::muted_foreground())
                } else {
                    crate::ui::files::file_icon(&file.path)
                };
                theme::paint_icon(ui, name, icon, icon_size, tint);
                file_rects.push((rect, file));
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
    let mut opened = None;
    if let Some(pointer) = response.hover_pos() {
        if let Some((_, file)) = file_rects.iter().find(|(r, _)| r.contains(pointer)) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            let path = match file.line {
                Some(line) => format!("{}:{line}", file.path),
                None => file.path.clone(),
            };
            let clicked = response.clicked();
            response.on_hover_text(path);
            if clicked {
                opened = Some(file.clone());
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

pub struct Markdown<'a> {
    pub highlighter: &'a mut Highlighter,
    pub id: egui::Id,
    /// A file chip clicked during `show`.
    pub(crate) opened_file: Option<FileRef>,
    /// Parent-folder suffixes of file chips sharing a basename.
    suffixes: HashMap<String, String>,
    /// Inside a quote, text takes the muted color.
    quoted: bool,
}

impl<'a> Markdown<'a> {
    pub fn new(highlighter: &'a mut Highlighter, id: egui::Id) -> Self {
        Self {
            highlighter,
            id,
            opened_file: None,
            suffixes: HashMap::new(),
            quoted: false,
        }
    }

    /// Renders `text` into the column starting at `ui.cursor()`; returns its height.
    pub fn show(&mut self, ui: &mut Ui, text: &str, width: f32) -> f32 {
        let blocks = parse(text);
        let mut files = Vec::new();
        collect_files(&blocks, &mut files);
        self.suffixes = md_links::parent_suffixes(&files);
        let top = ui.cursor().top();
        self.blocks(ui, &blocks, width, 0, self.id);
        ui.cursor().top() - top
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
            self.block(ui, block, width, depth, id);
            previous_bottom = Some(bottom);
        }
    }

    fn block(&mut self, ui: &mut Ui, block: &Block, width: f32, depth: usize, id: egui::Id) {
        // `.chat-markdown blockquote`: `color: var(--contrast-muted-foreground)`.
        let paragraph_color = if self.quoted {
            theme::muted_foreground()
        } else {
            theme::alpha(theme::foreground(), 0.8)
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
                if let Some(file) = paint_inlines(ui, rect.min, &laid, id) {
                    self.opened_file = Some(file);
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
                if let Some(file) = paint_inlines(ui, rect.min, &laid, id) {
                    self.opened_file = Some(file);
                }
            }
            Block::List { start, items } => {
                for (number, item) in items.iter().enumerate() {
                    if number > 0 {
                        ui.add_space(4.0); // li + li
                    }
                    let top = ui.cursor().top();
                    let left = ui.cursor().left();
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(Rect::from_min_size(
                            Pos2::new(left + 20.0, top),
                            Vec2::new(width - 20.0, f32::INFINITY),
                        )),
                        |ui| self.blocks(ui, item, width - 20.0, depth + 1, id.with(number)),
                    );
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
            Block::Code { language, code } => self.code_block(ui, language, code, width, id),
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
                        self.blocks(ui, blocks, width - 14.8, depth + 1, id.with("quote"));
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
            Block::Rule => {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
                ui.painter().rect_filled(rect, 0.0, theme::border());
            }
        }
    }

    fn code_block(&mut self, ui: &mut Ui, language: &str, code: &str, width: f32, id: egui::Id) {
        let line_height = theme::code_line(17.875); // 13px × leading-snug
        let inner_width = width - 2.0 - 28.8;
        let mut job = LayoutJob::default();
        for (text, color) in self.highlighter.spans(code, language) {
            job.append(
                text,
                0.0,
                widgets::format(theme::code_font(), *color, line_height),
            );
        }
        job.wrap.max_width = inner_width; // wrapping is on by default upstream
        let galley = widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job)));
        let height = 1.0 + 30.0 + 12.8 + galley.size().y + 12.8 + 1.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), theme::control_surface());
        // Header: language, wrap toggle (pressed), copy.
        let header_color = theme::alpha(theme::code_foreground(), 0.72);
        let header = Rect::from_min_size(rect.min + Vec2::new(13.0, 7.0), Vec2::new(200.0, 24.0));
        if let Some((icon, color)) = language_icon(language) {
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(header.min, Vec2::new(14.0, 24.0)),
                14.0,
                color,
            );
        } else {
            widgets::text_in(ui, header, language, theme::mono(11.0), header_color);
        }
        let copy = Rect::from_min_size(
            Pos2::new(rect.right() - 31.0, rect.top() + 7.0),
            Vec2::splat(24.0),
        );
        let wrap = copy.translate(Vec2::new(-26.0, 0.0));
        ui.painter()
            .rect_filled(wrap, CornerRadius::same(8), theme::muted());
        theme::paint_icon(ui, "text-wrap", wrap, 12.0, theme::foreground());
        widgets::copy_button(
            ui,
            copy,
            id.with("copy"),
            "Copy code",
            "Copied",
            theme::muted_foreground(),
            1.2,
            || code.to_owned(),
        );
        let pos = rect.min + Vec2::new(1.0 + 14.4, 1.0 + 30.0 + 12.8);
        widgets::selectable_galley(ui, id.with("code"), pos, galley, theme::foreground());
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
        let mut paint_row = |ui: &mut Ui, cells: &[Vec<Inline>], header: bool| {
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
                if let Some(file) = paint_inlines(
                    ui,
                    Pos2::new(x + offset, rect.top() + pad_y),
                    cell,
                    id.with(("cell", header, i, rect.top() as i32)),
                ) {
                    opened = Some(file);
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
        if opened.is_some() {
            self.opened_file = opened;
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
fn inline_markdown(inlines: &[Inline]) -> String {
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
            Inline::Break => " ".to_owned(),
        })
        .collect()
}

fn margins(block: &Block) -> (f32, f32) {
    match block {
        Block::Heading(..) => (20.0, 8.0),
        Block::Rule => (16.0, 16.0),
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
