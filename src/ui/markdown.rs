//! Assistant markdown (`ChatMarkdown.tsx` + `.chat-markdown` CSS): block
//! margins collapse like CSS, inline code gets its padded chip, fenced code
//! uses the pierre-dark highlighter, tables use the 12px compact layout.
use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, Galley, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
    text::LayoutJob,
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::highlight::Highlighter;
use crate::theme::{self, Weight, font};
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
    Break,
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
            Event::Start(Tag::Link { dest_url, .. }) => style.link = Some(dest_url.to_string()),
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::End(TagEnd::Link) => style.link = None,
            Event::Text(t) => out.push(Inline::Text(t.to_string(), style.clone())),
            Event::Code(t) => out.push(Inline::Code(t.to_string())),
            Event::SoftBreak => out.push(Inline::Text(" ".into(), style.clone())),
            Event::HardBreak => out.push(Inline::Break),
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
            Event::Start(Tag::Link { dest_url, .. }) => style.link = Some(dest_url.to_string()),
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::End(TagEnd::Link) => style.link = None,
            Event::Text(t) => out.push(Inline::Text(t.to_string(), style.clone())),
            Event::Code(t) => out.push(Inline::Code(t.to_string())),
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
    links: Vec<Option<String>>,
}

fn layout_inlines(
    ui: &Ui,
    inlines: &[Inline],
    size: f32,
    line_height: f32,
    base_weight: Weight,
    color: Color32,
    wrap: f32,
) -> Inlines {
    let mut job = LayoutJob::default();
    let mut chips = Vec::new();
    let mut links = Vec::new();
    let mut chip = 0;
    for inline in inlines {
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
                job.append(text, 0.0, format);
                chips.push(None);
                links.push(style.link.clone());
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
                links.push(None);
            }
        }
    }
    job.wrap.max_width = wrap;
    let galley = widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job)));
    Inlines {
        galley,
        chips,
        links,
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

/// Paints inline text, chip backgrounds and handles link clicks.
fn paint_inlines(ui: &mut Ui, pos: Pos2, inlines: &Inlines, id: egui::Id) {
    let painter = ui.painter().clone();
    let sections = char_sections(&inlines.galley);
    // Rows hold one glyph per character; a trailing `\n` is omitted.
    let mut char_index = 0usize;
    for placed in &inlines.galley.rows {
        let mut run: Option<(usize, f32, f32)> = None;
        let row_top = pos.y + placed.pos.y;
        let row_height = placed.row.size.y;
        let flush = |run: &mut Option<(usize, f32, f32)>| {
            if let Some((_, left, right)) = run.take() {
                let height = 21.0f32.min(row_height);
                let top = row_top + ((row_height - height) / 2.0).round();
                let rect = Rect::from_min_max(
                    Pos2::new(pos.x + placed.pos.x + left, top),
                    Pos2::new(pos.x + placed.pos.x + right, top + height),
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
        }
        flush(&mut run);
        if placed.ends_with_newline {
            char_index += 1;
        }
    }
    let rect = Rect::from_min_size(pos, inlines.galley.size());
    painter.galley(pos, inlines.galley.clone(), theme::foreground());
    if inlines.links.iter().any(Option::is_some) {
        let response = ui.interact(rect, id, Sense::click());
        if let Some(pointer) = response.hover_pos() {
            let cursor = inlines.galley.cursor_from_pos(pointer - pos);
            let section = sections.get(cursor.index.0).copied();
            if let Some(url) = section.and_then(|s| inlines.links.get(s).cloned().flatten()) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                if response.clicked() {
                    widgets::open_external(&url);
                }
            }
        }
    }
}

pub struct Markdown<'a> {
    pub highlighter: &'a mut Highlighter,
    pub id: egui::Id,
}

impl Markdown<'_> {
    /// Renders `text` into the column starting at `ui.cursor()`; returns its height.
    pub fn show(&mut self, ui: &mut Ui, text: &str, width: f32) -> f32 {
        let blocks = parse(text);
        let top = ui.cursor().top();
        self.blocks(ui, &blocks, width, 0);
        ui.cursor().top() - top
    }

    fn blocks(&mut self, ui: &mut Ui, blocks: &[Block], width: f32, depth: usize) {
        let mut previous_bottom: Option<f32> = None;
        for (index, block) in blocks.iter().enumerate() {
            let (top, bottom) = margins(block);
            if let Some(previous) = previous_bottom {
                ui.add_space(previous.max(top));
            }
            let id = self.id.with((depth, index));
            self.block(ui, block, width, depth, id);
            previous_bottom = Some(bottom);
        }
    }

    fn block(&mut self, ui: &mut Ui, block: &Block, width: f32, depth: usize, id: egui::Id) {
        let paragraph_color = theme::alpha(theme::foreground(), 0.8);
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
                );
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, laid.galley.size().y), Sense::hover());
                paint_inlines(ui, rect.min, &laid, id);
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
                );
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, laid.galley.size().y), Sense::hover());
                paint_inlines(ui, rect.min, &laid, id);
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
                        |ui| self.blocks(ui, item, width - 20.0, depth + 1),
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
                    |ui| self.blocks(ui, blocks, width - 14.8, depth + 1),
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
        let line_height = 17.875; // 13px × leading-snug
        let inner_width = width - 2.0 - 28.8;
        let mut job = LayoutJob::default();
        for (text, color) in self.highlighter.spans(code, language) {
            job.append(
                text,
                0.0,
                widgets::format(theme::mono(13.0), *color, line_height),
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
        if widgets::icon_button(
            ui,
            copy,
            id.with("copy"),
            "copy",
            12.0,
            theme::muted_foreground(),
            "Copy code",
        )
        .clicked()
        {
            ui.ctx().copy_text(code.to_owned());
        }
        let pos = rect.min + Vec2::new(1.0 + 14.4, 1.0 + 30.0 + 12.8);
        ui.painter().galley(pos, galley, theme::foreground());
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
        let measure = |ui: &Ui, inlines: &[Inline], weight| {
            layout_inlines(ui, inlines, 12.0, cell_line, weight, color, f32::INFINITY)
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
        let paint_row = |ui: &mut Ui, cells: &[Vec<Inline>], header: bool| {
            let pad_y = if header { 8.8 } else { 7.2 };
            let weight = if header {
                Weight::SemiBold
            } else {
                Weight::Regular
            };
            let laid: Vec<Inlines> = (0..columns)
                .map(|i| {
                    let cell = cells.get(i).map(Vec::as_slice).unwrap_or(&[]);
                    layout_inlines(ui, cell, 12.0, cell_line, weight, color, widths[i] - 24.0)
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
                paint_inlines(
                    ui,
                    Pos2::new(x + offset, rect.top() + pad_y),
                    cell,
                    id.with(("cell", header, i, rect.top() as i32)),
                );
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
        // Toolbar: expand toggle (expanded by default) and copy.
        ui.add_space(2.0);
        let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
        let expand = Rect::from_min_size(bar.min, Vec2::splat(24.0));
        ui.painter()
            .rect_filled(expand, CornerRadius::same(8), theme::muted());
        theme::paint_icon(ui, "minimize-2", expand, 12.0, theme::foreground());
        let copy = Rect::from_min_size(Pos2::new(bar.right() - 24.0, bar.top()), Vec2::splat(24.0));
        if widgets::icon_button(
            ui,
            copy,
            id.with("table-copy"),
            "copy",
            12.0,
            theme::muted_foreground(),
            "Copy table",
        )
        .clicked()
        {
            let lines: Vec<String> = std::iter::once(head)
                .chain(rows.iter().map(Vec::as_slice))
                .map(|cells| {
                    cells
                        .iter()
                        .map(|c| plain(c))
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .collect();
            ui.ctx().copy_text(lines.join("\n"));
        }
    }
}

fn plain(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|i| match i {
            Inline::Text(t, _) | Inline::Code(t) => t.as_str(),
            Inline::Break => " ",
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
        _ => return None,
    })
}
