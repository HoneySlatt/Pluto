//! Citing assistant text: the Cite button over a finished selection inside
//! one assistant message (`AssistantSelectionToolbar.tsx`,
//! `selectionActions.ts`), the citation chips of the composer and their
//! comment editor (`ComposerPromptEditorTiptap.tsx`'s citation node,
//! `AssistantCitationChip.tsx`, `AssistantCitationCommentEditor.tsx`,
//! `assistantCitationCommentDismissal.ts`), and revealing a cited source
//! with its pulse (`AssistantCitationSource.tsx`).
//!
//! The composer keeps the prompt as sent, citation links included; its
//! editor shows each link as one private-use character (`U+F0000 + n`),
//! the chip painted over it. epaint puts a section's letter spacing before
//! its glyph, so the chip's room is the spacing of the character after it,
//! and a chip ending a line keeps a space after it.

use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{
    self, Color32, CornerRadius, Galley, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
    Vec2, text::LayoutJob,
};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::citations::{self, Citation};
use crate::ui::widgets::{self, CopyCleanup};

const PLACEHOLDER_BASE: u32 = 0xF0000;
/// `CITATION_HIGHLIGHT_PEAK`: the comment-editing highlight strength.
const HIGHLIGHT_PEAK: f32 = 0.45;
const PULSE_MS: f64 = 650.0;
const HOLD_MS: f64 = 1575.0;
const FADE_MS: f64 = 450.0;
const TOTAL_MS: f64 = 1.5 * PULSE_MS + HOLD_MS + FADE_MS;

#[derive(Default)]
pub(crate) struct CiteState {
    /// Where the current primary press started.
    press: Option<Pos2>,
    /// A finished selection inside one assistant message, waiting for its
    /// copied text.
    pending: Option<Pending>,
    /// The Cite button.
    toolbar: Option<Toolbar>,
    /// The comment editor of a composer citation.
    pub(crate) comment: Option<CommentEditor>,
    /// A cited source being revealed.
    pub(crate) target: Option<Target>,
    /// The composer's editor and its chips' rects this frame.
    pub(crate) editor_id: Option<egui::Id>,
    chip_rects: Vec<Rect>,
    /// The skill chip whose details are open (`ContextChipPopover`).
    pub(crate) skill_popover: Option<(usize, String)>,
    /// The context chip whose details are open, by chip index and context id.
    pub(crate) context_popover: Option<(usize, String)>,
    /// Ctrl+Enter in the comment editor sends the prompt.
    pub(crate) send: bool,
}

struct Pending {
    message_id: String,
    pointer: Pos2,
    /// The selection's last row (the anchor of the comment editor).
    row: Rect,
    /// The message's rendered text and where the selection's first text starts.
    stream: String,
    from: usize,
    bounds: Rect,
    frames: u8,
}

struct Toolbar {
    citation: Citation,
    pos: Pos2,
    row: Rect,
}

pub(crate) enum Anchor {
    /// The cited selection's last row, right-aligned at its end.
    Selection(Rect),
    /// The chip's pencil.
    Chip,
}

pub(crate) struct CommentEditor {
    /// The chip's index among the composer's citations.
    pub(crate) index: usize,
    pub(crate) citation: Citation,
    draft: String,
    anchor: Anchor,
    /// A fresh citation goes away on Cancel or Escape.
    remove_on_cancel: bool,
    focused: bool,
}

pub(crate) struct Target {
    thread_id: String,
    pub(crate) message_id: String,
    pub(crate) quote: String,
    started: Option<f64>,
}

fn placeholder(index: usize) -> char {
    char::from_u32(PLACEHOLDER_BASE + index as u32).unwrap_or('\u{FFFD}')
}

pub(crate) fn placeholder_index(c: char) -> Option<usize> {
    let value = c as u32;
    (PLACEHOLDER_BASE..PLACEHOLDER_BASE + 0xFFFE)
        .contains(&value)
        .then(|| (value - PLACEHOLDER_BASE) as usize)
}

/// What a composer chip stands for: a citation, or a file mention
/// (`ComposerMentionNodeView`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Chip {
    Citation(Citation),
    Mention(String),
    Skill(String),
    /// A context reference (`t3-context://v1/<kind>/<id>`).
    Context {
        kind: String,
        id: String,
        label: String,
    },
}

/// A skill's chip label, description and instructions file
/// (`skillLabelFor`), from the selected provider's skills.
#[derive(Clone, Default)]
pub(crate) struct SkillMeta {
    pub(crate) label: String,
    pub(crate) description: Option<String>,
    pub(crate) path: Option<String>,
}

fn skill_metas() -> &'static Mutex<std::collections::HashMap<String, SkillMeta>> {
    static METAS: OnceLock<Mutex<std::collections::HashMap<String, SkillMeta>>> = OnceLock::new();
    METAS.get_or_init(Default::default)
}

/// The skills the composer's chips are named after.
pub(crate) fn set_skill_metas(metas: std::collections::HashMap<String, SkillMeta>) {
    if let Ok(mut current) = skill_metas().lock() {
        *current = metas;
    }
}

/// A skill the selected provider knows.
pub(crate) fn known_skill(name: &str) -> Option<SkillMeta> {
    skill_metas().lock().ok()?.get(name).cloned()
}

/// A skill's meta, or its name title-cased (`formatProviderSkillDisplayName`).
pub(crate) fn skill_meta(name: &str) -> SkillMeta {
    if let Some(meta) = skill_metas().lock().ok().and_then(|m| m.get(name).cloned()) {
        return meta;
    }
    SkillMeta {
        label: crate::ui::command_menu::title_case(name),
        ..Default::default()
    }
}

impl Chip {
    /// The chip's width at the composer's text `size`.
    fn width(&self, ui: &Ui, size: f32) -> f32 {
        match self {
            Chip::Citation(citation) => citations::chip_width(ui, &citation.label(), size, true),
            Chip::Mention(path) => mention_chip_width(ui, basename(path), size),
            Chip::Skill(name) => mention_chip_width(ui, &skill_meta(name).label, size),
            Chip::Context { label, .. } => mention_chip_width(ui, label, size),
        }
    }
}

/// The prompt's chips (citation links, then file mentions and skills
/// outside them), in order, with their byte ranges.
fn chips(prompt: &str) -> Vec<(Chip, std::ops::Range<usize>)> {
    let links = citations::collect(prompt);
    let contexts = crate::ui::composer_context::collect(prompt);
    let outside = |range: &std::ops::Range<usize>| {
        let overlaps =
            |link: &std::ops::Range<usize>| range.start < link.end && range.end > link.start;
        !links.iter().any(|(_, link)| overlaps(link))
            && !contexts.iter().any(|c| overlaps(&c.range))
    };
    let mut out: Vec<(Chip, std::ops::Range<usize>)> = crate::ui::mentions::collect(prompt)
        .into_iter()
        .filter(|(_, range)| outside(range))
        .map(|(path, range)| (Chip::Mention(path), range))
        .collect();
    let skills: Vec<(Chip, std::ops::Range<usize>)> = crate::ui::mentions::collect_skills(prompt)
        .into_iter()
        .filter(|(_, range)| {
            outside(range)
                && !out
                    .iter()
                    .any(|(_, m)| range.start < m.end && range.end > m.start)
        })
        .map(|(name, range)| (Chip::Skill(name), range))
        .collect();
    out.extend(skills);
    out.extend(contexts.into_iter().map(|c| {
        (
            Chip::Context {
                kind: c.kind,
                id: c.context_id,
                label: c.label,
            },
            c.range,
        )
    }));
    out.extend(
        links
            .into_iter()
            .map(|(c, range)| (Chip::Citation(c), range)),
    );
    out.sort_by_key(|(_, range)| range.start);
    out
}

/// The composer's prompt as its editor shows it.
pub(crate) struct Collapsed {
    pub(crate) display: String,
    /// Each placeholder's chip and its text as written in the prompt.
    pub(crate) tokens: Vec<(Chip, String)>,
}

pub(crate) fn collapse(prompt: &str) -> Collapsed {
    let mut display = String::new();
    let mut tokens = Vec::new();
    let mut cursor = 0;
    for (chip, range) in chips(prompt) {
        display.push_str(&prompt[cursor..range.start]);
        display.push(placeholder(tokens.len()));
        tokens.push((chip, prompt[range.clone()].to_owned()));
        cursor = range.end;
    }
    display.push_str(&prompt[cursor..]);
    Collapsed { display, tokens }
}

/// The prompt for an edited `display`: placeholders back to their links;
/// unknown placeholders are dropped.
pub(crate) fn expand(display: &str, tokens: &[(Chip, String)]) -> String {
    let mut out = String::new();
    for c in display.chars() {
        match placeholder_index(c) {
            Some(index) => {
                if let Some((_, source)) = tokens.get(index) {
                    out.push_str(source);
                }
            }
            None => out.push(c),
        }
    }
    out
}

/// A chip ending a line or the prompt keeps a space after it, the room
/// its chip is drawn in.
pub(crate) fn pad_chips(prompt: &mut String) {
    let ends: Vec<usize> = chips(prompt)
        .into_iter()
        .map(|(_, range)| range.end)
        .filter(|end| matches!(prompt[*end..].chars().next(), None | Some('\n')))
        .collect();
    for end in ends.into_iter().rev() {
        prompt.insert(end, ' ');
    }
}

/// The prompt byte offset of a display character index.
fn display_to_prompt(display: &str, tokens: &[(Chip, String)], index: usize) -> usize {
    display
        .chars()
        .take(index)
        .map(|c| match placeholder_index(c) {
            Some(i) => tokens.get(i).map_or(0, |(_, s)| s.len()),
            None => c.len_utf8(),
        })
        .sum()
}

/// The display character index of a prompt byte offset (inside a link:
/// after its chip).
fn prompt_to_display(prompt: &str, byte: usize) -> usize {
    let mut index = 0;
    let mut cursor = 0;
    for (_, range) in chips(prompt) {
        if byte <= range.start {
            break;
        }
        index += prompt[cursor..range.start].chars().count() + 1;
        cursor = range.end;
        if byte < range.end {
            return index;
        }
    }
    index + prompt[cursor..byte.max(cursor)].chars().count()
}

/// Composer links by placeholder for copies out of the editor.
fn copy_tokens() -> &'static Mutex<Vec<String>> {
    static TOKENS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    TOKENS.get_or_init(Default::default)
}

/// A copy from the composer carries the citation links
/// (`data-markdown-copy`), not their placeholders.
pub(crate) fn restore_copied(text: &str) -> String {
    if !text.chars().any(|c| placeholder_index(c).is_some()) {
        return text.to_owned();
    }
    let tokens = copy_tokens().lock().map(|t| t.clone()).unwrap_or_default();
    expand(
        text,
        &tokens
            .into_iter()
            .map(|source| (Chip::Mention(String::new()), source))
            .collect::<Vec<_>>(),
    )
}

/// The composer's galley: placeholders invisible, the character after each
/// spaced by its chip's width.
pub(crate) fn composer_galley(ui: &Ui, text: &str, chips: &[Chip], wrap: f32) -> Arc<Galley> {
    let font = theme::composer_font();
    let line = theme::composer_line();
    let plain = widgets::format(font.clone(), theme::foreground(), line);
    let mut job = LayoutJob::default();
    let mut run = 0;
    let mut room = 0.0;
    for (byte, c) in text.char_indices() {
        let chip = placeholder_index(c);
        if chip.is_none() && room == 0.0 {
            continue;
        }
        if run < byte {
            job.append(&text[run..byte], 0.0, plain.clone());
        }
        let mut format = plain.clone();
        format.extra_letter_spacing = room;
        if chip.is_some() {
            format.color = Color32::TRANSPARENT;
        }
        let end = byte + c.len_utf8();
        job.append(&text[byte..end], 0.0, format);
        run = end;
        room = chip.map_or(0.0, |index| {
            let width = chips
                .get(index)
                .map_or(0.0, |chip| chip.width(ui, font.size));
            // The placeholder's own advance (a fallback glyph, which
            // `glyph_width` does not report).
            let advance = ui
                .fonts_mut(|f| f.layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE))
                .size()
                .x;
            width - advance
        });
    }
    if run < text.len() || job.sections.is_empty() {
        job.append(&text[run..], 0.0, plain);
    }
    job.wrap.max_width = wrap;
    widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job)))
}

fn basename(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed)
}

/// A context chip's width (`ContextChip`: `px-[0.5em]`, the icon,
/// `gap-[0.33em]`, the label at 0.86em, a 1px border).
fn mention_chip_width(ui: &Ui, label: &str, size: f32) -> f32 {
    let chip = size * 0.86;
    let label = widgets::text_width(ui, label, font(chip, Weight::Medium));
    2.0 + 2.0 * 0.5 * chip + 1.17 * chip + 0.33 * chip + label
}

/// A context chip of `kind`'s accent (`oklch(0.62 chroma hue)`): its icon
/// (tinted when `icon_color` is None) and label.
#[allow(clippy::too_many_arguments)]
fn paint_context_chip(
    ui: &mut Ui,
    rect: Rect,
    (chroma, hue): (f32, f32),
    (icon_name, icon_color): (&str, Option<Color32>),
    label: &str,
    size: f32,
    id: egui::Id,
    tooltip: Option<&str>,
) -> egui::Response {
    let chip = size * 0.86;
    let [border, fill, text] =
        crate::ui::md_links::chip_colors(chroma, hue, theme::border(), theme::foreground());
    let radius = CornerRadius::same((0.5 * chip).round() as u8);
    let mut response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if let Some(tooltip) = tooltip {
        response = response.on_hover_text(tooltip);
    }
    let painter = ui.painter();
    painter.rect_filled(rect, radius, fill);
    if response.hovered() {
        // `hover:bg-(--context-chip-accent)/17`.
        painter.rect_filled(rect.shrink(1.0), radius, theme::alpha(fill, 0.6));
    }
    painter.rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    let icon_size = (1.17 * chip).round();
    let icon = Rect::from_min_size(
        Pos2::new(
            rect.left() + 1.0 + 0.5 * chip,
            rect.center().y - icon_size / 2.0,
        ),
        Vec2::splat(icon_size),
    );
    theme::paint_icon(ui, icon_name, icon, icon_size, icon_color.unwrap_or(text));
    let label_left = icon.right() + 0.33 * chip;
    let galley = widgets::galley_truncated(
        ui,
        label,
        font(chip, Weight::Medium),
        text,
        (1.2 * chip).round(),
        (rect.right() - 1.0 - 0.5 * chip - label_left).max(0.0),
    );
    ui.painter().galley(
        Pos2::new(label_left, rect.center().y - galley.size().y / 2.0),
        galley,
        text,
    );
    response
}

/// A mention chip (`kind="mention"`, accent `oklch(0.62 0.11 215)`): the
/// entry's icon and basename, the path as its tooltip.
fn paint_mention_chip(
    ui: &mut Ui,
    rect: Rect,
    path: &str,
    size: f32,
    id: egui::Id,
) -> egui::Response {
    let (icon, color) = if path.ends_with(['/', '\\']) {
        ("folder", theme::muted_foreground())
    } else {
        crate::ui::files::file_icon(path)
    };
    paint_context_chip(
        ui,
        rect,
        (0.11, 215.0),
        (icon, Some(color)),
        basename(path),
        size,
        id,
        Some(path),
    )
}

/// A `side="top"` popover's top: above `chip`, flipped below it when the
/// window has no room (Base UI's collision avoidance).
fn popover_top(chip: Rect, height: f32, screen: Rect) -> f32 {
    let above = chip.top() - 4.0 - height;
    if above >= screen.top() + 8.0 {
        above
    } else {
        chip.bottom() + 4.0
    }
}

/// The popover of a terminal excerpt above `chip` (`ContextChipPopover
/// width="lg" padding="compact"`): a bordered box with the terminal's name
/// and lines over the output (`max-h-80`, unwrapped). Returns its rect.
pub(crate) fn terminal_excerpt_popover(
    ui: &mut Ui,
    chip: Rect,
    record: &serde_json::Value,
) -> Option<Rect> {
    let width = 384.0;
    let text = record["text"].as_str().unwrap_or_default();
    let start = record["lineStart"].as_u64().unwrap_or(1);
    let end = record["lineEnd"].as_u64().unwrap_or(start);
    let lines = text.split('\n').count().max(1) as f32;
    let line = 19.5; // text-xs leading-relaxed
    let pre = (12.0 + lines * line + 12.0).min(320.0);
    let header = 8.0 + 20.0 + 8.0;
    let height = 1.0 + 8.0 + 1.0 + header + 1.0 + pre + 1.0 + 8.0 + 1.0;
    let screen = ui.ctx().content_rect();
    let left = (chip.center().x - width / 2.0).clamp(8.0, (screen.right() - width - 8.0).max(8.0));
    let rect = Rect::from_min_size(
        Pos2::new(left, popover_top(chip, height, screen)),
        Vec2::new(width, height),
    );
    egui::Area::new(egui::Id::new("context-excerpt-popover"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(ui.ctx(), |ui| {
            ui.allocate_exact_size(rect.size(), Sense::click());
            ui.painter().rect(
                rect,
                CornerRadius::same(10),
                Color32::from_rgb(17, 17, 17),
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(245, 245, 245, 26)),
                StrokeKind::Inside,
            );
            let card = Rect::from_min_max(
                rect.min + Vec2::new(1.0 + 12.0, 1.0 + 8.0),
                rect.max - Vec2::new(1.0 + 12.0, 1.0 + 8.0),
            );
            ui.painter().rect(
                card,
                CornerRadius::same(8),
                theme::alpha(theme::background(), 0.8),
                Stroke::new(1.0, theme::alpha(theme::border(), 0.7)),
                StrokeKind::Inside,
            );
            let top = card.top() + 1.0;
            theme::paint_icon(
                ui,
                "terminal",
                Rect::from_min_size(Pos2::new(card.left() + 13.0, top + 10.0), Vec2::splat(16.0)),
                16.0,
                Color32::from_rgb(0, 188, 125),
            );
            let range = if start == end {
                format!("Line {start}")
            } else {
                format!("Lines {start}–{end}")
            };
            let range_width = widgets::text_width(ui, &range, font(12.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(card.left() + 37.0, top + 8.0),
                    Pos2::new(card.right() - 20.0 - range_width, top + 28.0),
                ),
                record["terminalLabel"].as_str().unwrap_or_default(),
                font(14.0, Weight::Medium),
                theme::foreground(),
            );
            widgets::text(
                ui,
                Pos2::new(card.right() - 13.0 - range_width, top + 10.0),
                &range,
                font(12.0, Weight::Regular),
                theme::secondary_label(),
                16.0,
            );
            let divider = top + header;
            ui.painter().rect_filled(
                Rect::from_min_size(
                    Pos2::new(card.left() + 1.0, divider),
                    Vec2::new(card.width() - 2.0, 1.0),
                ),
                0.0,
                theme::alpha(theme::border(), 0.7),
            );
            let body = Rect::from_min_max(
                Pos2::new(card.left() + 1.0, divider + 1.0),
                card.max - Vec2::splat(1.0),
            );
            ui.painter().rect_filled(
                body,
                CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: 7,
                    se: 7,
                },
                theme::muted(),
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink(12.0)));
            child.set_clip_rect(body.shrink(1.0));
            egui::ScrollArea::both()
                .id_salt("context-excerpt-scroll")
                .auto_shrink([false, false])
                .show(&mut child, |ui| {
                    let galley = widgets::galley(
                        ui,
                        text,
                        theme::mono(12.0),
                        theme::foreground(),
                        line,
                        f32::INFINITY,
                    );
                    let size = galley.size();
                    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                    ui.painter().galley(rect.min, galley, theme::foreground());
                });
        });
    Some(rect)
}

/// The ease-in-out pulse of a revealed citation: two pulses, a hold, a fade.
fn pulse_alpha(elapsed_ms: f64) -> f32 {
    let t = (elapsed_ms / TOTAL_MS).clamp(0.0, 1.0);
    // `ease-in-out` over the whole animation.
    let eased = t * t * (3.0 - 2.0 * t);
    let at = eased * TOTAL_MS;
    let peak = HIGHLIGHT_PEAK as f64;
    let stops = [
        (0.0, 0.0),
        (0.5 * PULSE_MS, peak),
        (PULSE_MS, 0.0),
        (1.5 * PULSE_MS, peak),
        (TOTAL_MS - FADE_MS, peak),
        (TOTAL_MS, 0.0),
    ];
    for pair in stops.windows(2) {
        let ((t0, a0), (t1, a1)) = (pair[0], pair[1]);
        if at <= t1 {
            return (a0 + (a1 - a0) * (at - t0) / (t1 - t0)) as f32;
        }
    }
    0.0
}

impl App {
    fn environment_id_for_citations(&self) -> String {
        self.state.config["environment"]["environmentId"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    /// The quote to highlight in an assistant message and its strength:
    /// the citation whose comment is being edited, or a revealed source.
    pub(crate) fn citation_highlight(
        &mut self,
        ctx: &egui::Context,
        message_id: &str,
    ) -> Option<(String, f32)> {
        if let Some(editor) = &self.cite.comment
            && editor.citation.message_id == message_id
        {
            return Some((editor.citation.text.clone(), HIGHLIGHT_PEAK));
        }
        let target = self.cite.target.as_ref()?;
        if target.message_id != message_id {
            return None;
        }
        let started = target.started?;
        let elapsed = (ctx.input(|i| i.time) - started) * 1000.0;
        if elapsed >= TOTAL_MS {
            self.cite.target = None;
            return None;
        }
        ctx.request_repaint();
        Some((target.quote.clone(), pulse_alpha(elapsed)))
    }

    /// A citation chip's source link (`assistantCitationNavigation`): opens
    /// the thread and reveals the cited message with a pulse.
    pub(crate) fn open_citation_source(&mut self, citation: &Citation) {
        let known = citation.environment_id == self.environment_id_for_citations()
            && self
                .state
                .threads
                .iter()
                .any(|t| t.id == citation.thread_id);
        if !known {
            self.status = Some("Could not open the cited response".into());
            return;
        }
        if self.selected.as_deref() != Some(citation.thread_id.as_str()) {
            self.select_thread(Some(citation.thread_id.clone()));
        }
        self.cite.target = Some(Target {
            thread_id: citation.thread_id.clone(),
            message_id: citation.message_id.clone(),
            quote: citation.text.clone(),
            started: None,
        });
    }

    /// The revealed source of the open thread: its message id while it
    /// still has to scroll into view.
    pub(crate) fn citation_scroll_target(&self, thread_id: &str) -> Option<String> {
        self.cite
            .target
            .as_ref()
            .filter(|t| t.thread_id == thread_id && t.started.is_none())
            .map(|t| t.message_id.clone())
    }

    /// The revealed message is in view: its pulse starts.
    pub(crate) fn citation_revealed(&mut self, ctx: &egui::Context) {
        if let Some(target) = &mut self.cite.target {
            target.started = Some(ctx.input(|i| i.time));
        }
    }

    /// Watches the timeline's selections: a drag or multi-click finished
    /// inside one assistant message reads the selected text, then offers
    /// the Cite button at the release point. Runs after the timeline.
    pub(crate) fn cite_selection(&mut self, ui: &mut Ui, bounds: Rect) {
        let (pressed, released, pointer, escape, scrolled) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.interact_pos(),
                i.key_pressed(Key::Escape),
                i.smooth_scroll_delta != Vec2::ZERO || self.timeline_view.scrolled,
            )
        });
        let plugin = ui.ctx().plugin::<CopyCleanup>();
        if pressed {
            self.cite.press = pointer;
            let on_toolbar = self
                .cite
                .toolbar
                .as_ref()
                .zip(pointer)
                .is_some_and(|(toolbar, at)| self.toolbar_rect(ui, toolbar).contains(at));
            if !on_toolbar {
                self.cite.toolbar = None;
                self.cite.pending = None;
            }
        }
        if escape || scrolled {
            self.cite.toolbar = None;
            self.cite.pending = None;
        }
        if released
            && plugin.lock().has_range
            && let (Some(press), Some(release)) = (self.cite.press, pointer)
        {
            let sourced = plugin.lock().sourced.clone();
            let at = |point: Pos2| -> Option<usize> {
                sourced
                    .iter()
                    .position(|s| Rect::from_min_size(s.pos, s.galley.size()).contains(point))
                    .or_else(|| {
                        sourced
                            .iter()
                            .enumerate()
                            .min_by(|(_, a), (_, b)| {
                                let distance = |s: &widgets::Sourced| {
                                    Rect::from_min_size(s.pos, s.galley.size())
                                        .distance_sq_to_pos(point)
                                };
                                distance(a).total_cmp(&distance(b))
                            })
                            .map(|(index, _)| index)
                    })
            };
            let first = sourced
                .iter()
                .position(|s| Rect::from_min_size(s.pos, s.galley.size()).contains(press));
            if let (Some(first), Some(last)) = (first, at(release))
                && sourced[first].source == sourced[last].source
            {
                let message_id = sourced[first].source.clone();
                let mut stream = String::new();
                let mut from = 0;
                let top = first.min(last);
                for (index, galley) in sourced.iter().enumerate() {
                    if galley.source != message_id {
                        continue;
                    }
                    if !stream.is_empty() {
                        stream.push('\n');
                    }
                    if index == top {
                        from = stream.len();
                    }
                    stream.push_str(&galley.galley.text().replace('\u{200B}', ""));
                }
                let end = &sourced[last];
                let row = end
                    .galley
                    .rows
                    .iter()
                    .map(|placed| placed.rect().translate(end.pos.to_vec2()))
                    .min_by(|a, b| {
                        let d = |r: &Rect| (r.center().y - release.y).abs();
                        d(a).total_cmp(&d(b))
                    })
                    .unwrap_or(Rect::from_min_size(release, Vec2::ZERO));
                self.cite.pending = Some(Pending {
                    message_id,
                    pointer: release,
                    row,
                    stream,
                    from,
                    bounds,
                    frames: 0,
                });
                let mut plugin = plugin.lock();
                plugin.captured = None;
                plugin.capture = 3;
                plugin.capture_plain = true;
                self.pending_copy = true;
            }
        }
        // The selected text arrives through a captured copy.
        if let Some(pending) = &mut self.cite.pending {
            pending.frames += 1;
            let captured = if self.text_menu.is_none() {
                plugin.lock().captured.take()
            } else {
                None
            };
            if let Some(quote) = captured {
                let pending = self.cite.pending.take().expect("pending selection");
                if let Some(thread) = self.current_thread() {
                    let selector = citations::find_quote(&pending.stream, &quote, pending.from)
                        .and_then(|(start, end)| {
                            citations::create_selector(&pending.stream, start, end)
                        });
                    let (start, end, prefix, suffix) = match selector {
                        Some(s) => (s.start, s.end, s.prefix, s.suffix),
                        None => (
                            0,
                            citations::utf16_len(&citations::normalize_whitespace(&quote)) as u64,
                            String::new(),
                            String::new(),
                        ),
                    };
                    if !quote.trim().is_empty() && end > start {
                        let citation = Citation {
                            environment_id: self.environment_id_for_citations(),
                            thread_id: thread.id.clone(),
                            message_id: pending.message_id.clone(),
                            text: quote,
                            comment: None,
                            start,
                            end,
                            prefix,
                            suffix,
                        };
                        // `resolveSelectionActionPosition`: at the pointer,
                        // kept inside the timeline.
                        let b = pending.bounds;
                        let screen = ui.ctx().content_rect();
                        let pos = Pos2::new(
                            pending
                                .pointer
                                .x
                                .max(b.left())
                                .min(b.right())
                                .min(screen.right() - 8.0)
                                .max(8.0),
                            pending
                                .pointer
                                .y
                                .max(b.top())
                                .min(b.bottom())
                                .min(screen.bottom() - 8.0)
                                .max(8.0),
                        );
                        self.cite.toolbar = Some(Toolbar {
                            citation,
                            pos,
                            row: pending.row,
                        });
                    }
                }
            } else if pending.frames > 8 {
                self.cite.pending = None;
            } else {
                ui.ctx().request_repaint();
            }
        }
    }

    /// The Cite button's box (`Button size="xs" variant="glass"`: 24px,
    /// 7px padding, the 14px quote icon, a 4px gap, 12px medium text),
    /// kept inside the window.
    fn toolbar_rect(&self, ui: &Ui, toolbar: &Toolbar) -> Rect {
        let label = if citations::utf16_len(&toolbar.citation.text) > citations::MAX_TEXT_LENGTH {
            "Shorten selection"
        } else {
            "Cite"
        };
        let width = 2.0
            + 7.0
            + 14.0
            + 4.0
            + widgets::text_width(ui, label, font(12.0, Weight::Medium))
            + 7.0;
        let screen = ui.ctx().content_rect();
        let x = toolbar.pos.x.min(screen.right() - width - 8.0).max(8.0);
        let y = toolbar.pos.y.min(screen.bottom() - 24.0 - 8.0).max(8.0);
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 24.0))
    }

    /// The Cite button and the comment editor, over everything.
    pub(crate) fn cite_overlays(&mut self, ui: &mut Ui) {
        if let Some(toolbar) = self.cite.toolbar.take() {
            let rect = self.toolbar_rect(ui, &toolbar);
            let too_long =
                citations::utf16_len(&toolbar.citation.text) > citations::MAX_TEXT_LENGTH;
            let mut cite = false;
            egui::Area::new(egui::Id::new("cite-toolbar"))
                .order(egui::Order::Foreground)
                .fixed_pos(rect.min)
                .show(ui.ctx(), |ui| {
                    let (_, response) = ui.allocate_exact_size(rect.size(), Sense::click());
                    let painter = ui.painter();
                    // `shadow-sm` under the glass (`bg-popover/80`, a 3.6% white border).
                    painter.rect_filled(
                        rect.translate(Vec2::new(0.0, 1.0)),
                        CornerRadius::same(12),
                        Color32::from_black_alpha(26),
                    );
                    painter.rect(
                        rect,
                        CornerRadius::same(12),
                        Color32::from_rgba_unmultiplied(10, 10, 10, 204),
                        Stroke::new(1.0, Color32::from_white_alpha(9)),
                        StrokeKind::Inside,
                    );
                    if response.hovered() && !too_long {
                        painter.rect_filled(
                            rect.shrink(1.0),
                            CornerRadius::same(12),
                            theme::overlay(0.04),
                        );
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let alpha = if too_long { 0.64 } else { 1.0 };
                    let icon = Rect::from_min_size(
                        Pos2::new(rect.left() + 8.0, rect.center().y - 7.0),
                        Vec2::splat(14.0),
                    );
                    theme::paint_icon(
                        ui,
                        "quote",
                        icon,
                        14.0,
                        theme::alpha(theme::muted_foreground(), alpha),
                    );
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(Pos2::new(icon.right() + 4.0, rect.top()), rect.max),
                        if too_long {
                            "Shorten selection"
                        } else {
                            "Cite"
                        },
                        font(12.0, Weight::Medium),
                        theme::alpha(theme::foreground(), alpha),
                    );
                    // Its labels are `aria-label`s, not tooltips.
                    cite = response.clicked() && !too_long;
                });
            if cite {
                self.cite_toolbar(ui, toolbar);
            } else {
                self.cite.toolbar = Some(toolbar);
            }
        }
        self.citation_comment_editor(ui);
    }

    /// `citeAssistantText`: the citation goes into the composer at its
    /// caret (a space before it when needed, one after), the selection
    /// clears and the comment editor opens under the quote.
    fn cite_toolbar(&mut self, ui: &Ui, toolbar: Toolbar) {
        let accepts = matches!(
            self.composer_mode(false),
            crate::ui::composer::Mode::Normal | crate::ui::composer::Mode::Plan(_)
        );
        if !accepts {
            self.cite.toolbar = Some(toolbar);
            return;
        }
        let collapsed = collapse(&self.composer);
        let display_len = collapsed.display.chars().count();
        let caret = self
            .cite
            .editor_id
            .and_then(|id| egui::TextEdit::load_state(ui.ctx(), id))
            .and_then(|state| state.cursor.char_range())
            .map_or(display_len, |range| range.primary.index.0.min(display_len));
        let at = display_to_prompt(&collapsed.display, &collapsed.tokens, caret);
        let lead = at > 0
            && !self.composer[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        let link = format!(
            "{}{} ",
            if lead { " " } else { "" },
            toolbar.citation.serialize()
        );
        let end = if self.composer[at..].starts_with(' ') {
            at + 1
        } else {
            at
        };
        self.composer.replace_range(at..end, &link);
        let index = collapsed
            .display
            .chars()
            .take(caret)
            .filter(|c| placeholder_index(*c).is_some())
            .count();
        // The caret goes after the chip and its space.
        if let Some(id) = self.cite.editor_id {
            let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
            let after = caret + usize::from(lead) + 2;
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(after),
                )));
            state.store(ui.ctx(), id);
        }
        ui.ctx()
            .plugin::<egui::text_selection::LabelSelectionState>()
            .lock()
            .clear_selection();
        ui.ctx().plugin::<CopyCleanup>().lock().has_range = false;
        let anchor = Rect::from_min_max(
            Pos2::new(toolbar.row.left(), toolbar.row.top()),
            Pos2::new(
                toolbar
                    .pos
                    .x
                    .min(toolbar.row.right())
                    .max(toolbar.row.left()),
                toolbar.row.bottom(),
            ),
        );
        self.cite.comment = Some(CommentEditor {
            index,
            citation: toolbar.citation,
            draft: String::new(),
            anchor: Anchor::Selection(anchor),
            remove_on_cancel: true,
            focused: false,
        });
    }

    /// The composer's citation chips over its galley: their source links
    /// and comment pencils.
    pub(crate) fn composer_chips(
        &mut self,
        ui: &mut Ui,
        galley: &Galley,
        origin: Pos2,
        collapsed: &Collapsed,
    ) {
        let size = theme::composer_font().size;
        let height = (1.41 * size * 0.86).round();
        let mut rects = vec![Rect::NOTHING; collapsed.tokens.len()];
        let mut open = None;
        let mut edit = None;
        let mut mention = None;
        let mut skill = None;
        let mut context = None;
        for placed in &galley.rows {
            for glyph in &placed.row.glyphs {
                let Some(index) = placeholder_index(glyph.chr) else {
                    continue;
                };
                let Some((chip, _)) = collapsed.tokens.get(index) else {
                    continue;
                };
                let width = chip.width(ui, size);
                let row = placed.rect().translate(origin.to_vec2());
                let rect = Rect::from_min_size(
                    Pos2::new(
                        (origin.x + placed.pos.x + glyph.pos.x).round(),
                        // The editor's inline-flex node view sits 0.2em above
                        // the line's middle (measured against the reference).
                        (row.center().y - height / 2.0 - 0.2 * size).round(),
                    ),
                    Vec2::new(width.round(), height),
                );
                rects[index] = rect;
                let citation = match chip {
                    Chip::Citation(citation) => citation,
                    Chip::Mention(path) => {
                        let id = egui::Id::new(("composer-mention", index));
                        if paint_mention_chip(ui, rect, path, size, id).clicked() {
                            mention = Some(path.clone());
                        }
                        continue;
                    }
                    Chip::Skill(name) => {
                        let meta = skill_meta(name);
                        let id = egui::Id::new(("composer-skill", index));
                        let icon = ("skill-chip", None);
                        if paint_context_chip(
                            ui,
                            rect,
                            (0.16, 322.0),
                            icon,
                            &meta.label,
                            size,
                            id,
                            None,
                        )
                        .clicked()
                        {
                            skill = Some((index, name.clone()));
                        }
                        continue;
                    }
                    Chip::Context { kind, id, label } => {
                        let chip_id = egui::Id::new(("composer-context", index));
                        let record = self
                            .composer_contexts
                            .iter()
                            .find(|r| r["contextId"] == id.as_str());
                        let shown = crate::ui::composer_context::display_kind(kind, record);
                        let icon = (crate::ui::composer_context::kind_icon(&shown), None);
                        let accent = crate::ui::composer_context::kind_accent(&shown);
                        if paint_context_chip(ui, rect, accent, icon, label, size, chip_id, None)
                            .clicked()
                        {
                            context = Some((index, id.clone()));
                        }
                        continue;
                    }
                };
                let label = citation.label();
                let response = citations::paint_chip(
                    ui,
                    rect,
                    &label,
                    size,
                    egui::Id::new(("composer-citation", index)),
                    true,
                );
                if response.link.clicked() {
                    open = Some(citation.clone());
                }
                let response = response.pencil.map(|p| {
                    p.on_hover_text(if citation.comment.is_some() {
                        "Edit citation comment"
                    } else {
                        "Add comment to citation"
                    })
                });
                if response.is_some_and(|p| p.clicked()) {
                    edit = Some((index, citation.clone()));
                }
            }
        }
        self.cite.chip_rects = rects;
        if let Some(citation) = open {
            self.open_citation_source(&citation);
        }
        // `openMention`: the file in the right panel.
        if let Some(path) = mention {
            self.open_markdown_file(&path);
        }
        if let Some((index, name)) = skill {
            let same = self
                .cite
                .skill_popover
                .as_ref()
                .is_some_and(|(i, _)| *i == index);
            self.cite.skill_popover = (!same).then_some((index, name));
        }
        if let Some((index, id)) = context {
            let same = self
                .cite
                .context_popover
                .as_ref()
                .is_some_and(|(i, _)| *i == index);
            self.cite.context_popover = (!same).then_some((index, id));
        }
        self.skill_popover(ui);
        self.context_popover(ui);
        if let Some((index, citation)) = edit {
            let toggled_off = self.cite.comment.as_ref().is_some_and(|c| c.index == index);
            self.cite.comment = (!toggled_off).then(|| CommentEditor {
                index,
                draft: citation.comment.clone().unwrap_or_default(),
                citation,
                anchor: Anchor::Chip,
                remove_on_cancel: false,
                focused: false,
            });
        }
    }

    /// A sent message's terminal excerpt, opened from its chip.
    pub(crate) fn bubble_context_popover(&mut self, ui: &mut Ui) {
        let Some((chip, record)) = self.bubble_popover.clone() else {
            return;
        };
        if let Some(rect) = terminal_excerpt_popover(ui, chip, &record) {
            let outside = ui.input(|i| i.pointer.primary_pressed())
                && ui
                    .input(|i| i.pointer.interact_pos())
                    .is_some_and(|p| !rect.contains(p) && !chip.contains(p));
            let scrolled = ui.input(|i| i.smooth_scroll_delta.y != 0.0)
                && ui
                    .input(|i| i.pointer.hover_pos())
                    .is_some_and(|p| !rect.contains(p));
            if outside || scrolled || ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
            {
                self.bubble_popover = None;
            }
        }
    }

    /// A terminal excerpt's details (`TerminalContextInlineChip`'s popover):
    /// the terminal, its lines and the captured output.
    fn context_popover(&mut self, ui: &mut Ui) {
        let Some((index, id)) = self.cite.context_popover.clone() else {
            return;
        };
        let chip = self
            .cite
            .chip_rects
            .get(index)
            .copied()
            .filter(|r| r.is_finite());
        let record = self
            .composer_contexts
            .iter()
            .find(|r| r["contextId"] == id.as_str() && r["kind"] == "terminal")
            .cloned();
        let (Some(chip), Some(record)) = (chip, record) else {
            self.cite.context_popover = None;
            return;
        };
        if let Some(rect) = terminal_excerpt_popover(ui, chip, &record) {
            let outside = ui.input(|i| i.pointer.primary_pressed())
                && ui
                    .input(|i| i.pointer.interact_pos())
                    .is_some_and(|p| !rect.contains(p) && !chip.contains(p));
            if outside || ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.cite.context_popover = None;
            }
        }
    }

    /// A skill chip's details (`ComposerSkillNodeView`'s `PopoverPopup
    /// side="top" width="lg" padding="compact"`): its name, description and
    /// "View instructions"; Escape or a press outside closes it.
    fn skill_popover(&mut self, ui: &mut Ui) {
        let Some((index, name)) = self.cite.skill_popover.clone() else {
            return;
        };
        let Some(chip) = self
            .cite
            .chip_rects
            .get(index)
            .copied()
            .filter(|r| r.is_finite())
        else {
            self.cite.skill_popover = None;
            return;
        };
        let meta = skill_meta(&name);
        let width = 384.0;
        let inner = width - 2.0 - 2.0 * (12.0 + 8.0);
        let body = font(14.0, Weight::Regular);
        let description = widgets::galley(
            ui,
            meta.description
                .as_deref()
                .unwrap_or("No description is available for this skill."),
            body.clone(),
            theme::foreground(),
            20.0,
            inner,
        );
        let button = if meta.path.is_some() {
            12.0 + 28.0
        } else {
            0.0
        };
        let height =
            1.0 + 8.0 + 8.0 + 20.0 + 12.0 + description.size().y + button + 8.0 + 8.0 + 1.0;
        let screen = ui.ctx().content_rect();
        let left =
            (chip.center().x - width / 2.0).clamp(8.0, (screen.right() - width - 8.0).max(8.0));
        let rect = Rect::from_min_size(
            Pos2::new(left, popover_top(chip, height, screen)),
            Vec2::new(width, height),
        );
        let mut open_path = None;
        egui::Area::new(egui::Id::new("composer-skill-popover"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ui.ctx(), |ui| {
                ui.allocate_exact_size(rect.size(), Sense::click());
                ui.painter().rect(
                    rect,
                    CornerRadius::same(10),
                    Color32::from_rgb(17, 17, 17),
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(245, 245, 245, 26)),
                    StrokeKind::Inside,
                );
                let x = rect.left() + 1.0 + 12.0 + 8.0;
                let mut y = rect.top() + 1.0 + 8.0 + 8.0;
                widgets::text(
                    ui,
                    Pos2::new(x, y),
                    &meta.label,
                    font(14.0, Weight::Medium),
                    theme::foreground(),
                    20.0,
                );
                y += 20.0 + 12.0;
                ui.painter()
                    .galley(Pos2::new(x, y), description.clone(), theme::foreground());
                y += description.size().y + 12.0;
                if let Some(path) = &meta.path {
                    let label = "View instructions";
                    let label_font = font(14.0, Weight::Medium);
                    let width = (widgets::text_width(ui, label, label_font.clone()) + 18.0).ceil();
                    let button = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 28.0));
                    let response = widgets::outline_button(
                        ui,
                        button,
                        "skill-view-instructions",
                        None,
                        "",
                        CornerRadius::same(8),
                    );
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(button.left() + 9.0, button.top()),
                            button.max,
                        ),
                        label,
                        label_font,
                        theme::foreground(),
                    );
                    if response.clicked() {
                        open_path = Some(path.clone());
                    }
                }
            });
        let outside = ui.input(|i| i.pointer.primary_pressed())
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| !rect.contains(p) && !chip.contains(p));
        if outside || ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.cite.skill_popover = None;
        }
        if let Some(path) = open_path {
            self.cite.skill_popover = None;
            self.open_markdown_file(&path);
        }
    }

    /// Replaces the composer's `index`th citation link with `citation`, or
    /// removes it.
    fn replace_composer_citation(&mut self, index: usize, citation: Option<&Citation>) {
        if let Some((Chip::Citation(_), range)) = chips(&self.composer).into_iter().nth(index) {
            let replacement = citation.map(Citation::serialize).unwrap_or_default();
            self.composer.replace_range(range, &replacement);
        }
    }

    /// The comment editor (`PopoverPopup width="md" padding="compact"`):
    /// under the quote for a fresh citation, above its chip otherwise.
    fn citation_comment_editor(&mut self, ui: &mut Ui) {
        let Some(mut editor) = self.cite.comment.take() else {
            return;
        };
        let size = Vec2::new(320.0, 0.0);
        let input_id = egui::Id::new("citation-comment-input");
        let too_long = citations::utf16_len(&editor.draft) > citations::MAX_COMMENT_LENGTH;
        let input_height = {
            let galley = widgets::galley(
                ui,
                if editor.draft.is_empty() {
                    " "
                } else {
                    &editor.draft
                },
                font(14.0, Weight::Regular),
                theme::foreground(),
                20.0,
                294.0 - 8.0,
            );
            (galley.size().y + 12.0).clamp(64.0, 160.0)
        };
        let status_height = if too_long { 20.0 } else { 0.0 };
        let height = 2.0 + 16.0 + input_height + status_height + 8.0 + 24.0;
        let screen = ui.ctx().content_rect();
        let (right, top) = match &editor.anchor {
            Anchor::Selection(anchor) => (anchor.right(), anchor.bottom() + 4.0),
            Anchor::Chip => {
                let chip = self
                    .cite
                    .chip_rects
                    .get(editor.index)
                    .copied()
                    .unwrap_or(Rect::NOTHING);
                if !chip.is_finite() {
                    return;
                }
                (chip.right(), chip.top() - 4.0 - height)
            }
        };
        let left = (right - size.x).clamp(8.0, (screen.right() - size.x - 8.0).max(8.0));
        let top = top.clamp(8.0, (screen.bottom() - height - 8.0).max(8.0));
        let rect = Rect::from_min_size(Pos2::new(left, top), Vec2::new(size.x, height));

        // Keys go to the editor first: Enter saves, Ctrl+Enter saves and
        // sends, Shift+Enter breaks the line, Escape cancels.
        let focused = ui.memory(|m| m.has_focus(input_id));
        let (save, send, cancel) = ui.input_mut(|i| {
            let send = focused && i.consume_key(Modifiers::COMMAND, Key::Enter);
            let save = focused && !send && i.consume_key(Modifiers::NONE, Key::Enter);
            let cancel = i.consume_key(Modifiers::NONE, Key::Escape);
            (save, send, cancel)
        });
        let mut clicked_save = false;
        let mut clicked_cancel = false;
        let outside_press = ui.input(|i| i.pointer.primary_pressed())
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| !rect.contains(p));
        egui::Area::new(egui::Id::new("citation-comment-editor"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ui.ctx(), |ui| {
                ui.allocate_exact_size(rect.size(), Sense::click());
                let painter = ui.painter();
                // `shadow-[0_18px_44px_-18px_rgb(0_0_0/0.8)]`.
                painter.add(
                    egui::epaint::Shadow {
                        offset: [0, 18],
                        blur: 44,
                        spread: 0,
                        color: Color32::from_black_alpha(204),
                    }
                    .as_shape(rect.shrink(18.0), CornerRadius::same(10)),
                );
                painter.rect(
                    rect,
                    CornerRadius::same(10),
                    // `bg-popover` glass, opaque: egui has no backdrop blur.
                    Color32::from_rgb(17, 17, 17),
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(245, 245, 245, 26)),
                    StrokeKind::Inside,
                );
                let input_rect = Rect::from_min_size(
                    rect.min + Vec2::new(13.0, 9.0),
                    Vec2::new(294.0, input_height),
                );
                if editor.draft.is_empty() {
                    widgets::text(
                        ui,
                        input_rect.min + Vec2::new(4.0, 6.0),
                        "Add an optional comment...",
                        font(14.0, Weight::Regular),
                        theme::muted_foreground(),
                        20.0,
                    );
                }
                let mut child = ui.new_child(
                    egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(4.0, 6.0))),
                );
                let response = child.add(
                    egui::TextEdit::multiline(&mut editor.draft)
                        .id(input_id)
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .font(font(14.0, Weight::Regular))
                        .text_color(theme::foreground())
                        .desired_width(294.0 - 8.0)
                        .desired_rows(2)
                        .layouter(&mut |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                            let mut job = LayoutJob::default();
                            job.append(
                                text.as_str(),
                                0.0,
                                widgets::format(
                                    font(14.0, Weight::Regular),
                                    theme::foreground(),
                                    20.0,
                                ),
                            );
                            job.wrap.max_width = wrap;
                            widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job)))
                        }),
                );
                if !editor.focused {
                    response.request_focus();
                    editor.focused = true;
                }
                if too_long {
                    widgets::text(
                        ui,
                        Pos2::new(input_rect.left(), input_rect.bottom() + 4.0),
                        &format!("Comments can contain up to {} characters.", "8,000"),
                        font(12.0, Weight::Regular),
                        theme::error_foreground(),
                        16.0,
                    );
                }
                let bar_top = input_rect.bottom() + status_height + 8.0;
                let save_label = if too_long { "Shorten comment" } else { "Save" };
                let save_width =
                    widgets::text_width(ui, save_label, font(12.0, Weight::Medium)) + 16.0;
                let save_rect = Rect::from_min_size(
                    Pos2::new(rect.right() - 13.0 - save_width, bar_top),
                    Vec2::new(save_width, 24.0),
                );
                let cancel_width =
                    widgets::text_width(ui, "Cancel", font(12.0, Weight::Medium)) + 16.0;
                let cancel_rect = Rect::from_min_size(
                    Pos2::new(save_rect.left() - 8.0 - cancel_width, bar_top),
                    Vec2::new(cancel_width, 24.0),
                );
                let cancel = ui.interact(
                    cancel_rect,
                    egui::Id::new("citation-comment-cancel"),
                    Sense::click(),
                );
                let painter = ui.painter();
                // `variant="outline"`: a 2.5% white fill, 8% white border.
                painter.rect(
                    cancel_rect,
                    CornerRadius::same(8),
                    if cancel.hovered() {
                        theme::overlay(0.06)
                    } else {
                        Color32::from_white_alpha(7)
                    },
                    Stroke::new(1.0, Color32::from_white_alpha(20)),
                    StrokeKind::Inside,
                );
                widgets::text_in(
                    ui,
                    cancel_rect.shrink2(Vec2::new(8.0, 0.0)),
                    "Cancel",
                    font(12.0, Weight::Medium),
                    theme::foreground(),
                );
                let save = ui.interact(
                    save_rect,
                    egui::Id::new("citation-comment-save"),
                    Sense::click(),
                );
                let primary = theme::primary();
                let fill = if too_long {
                    primary.gamma_multiply(0.64)
                } else if save.hovered() {
                    primary.gamma_multiply(0.9)
                } else {
                    primary
                };
                ui.painter().rect(
                    save_rect,
                    CornerRadius::same(8),
                    fill,
                    Stroke::new(1.0, fill),
                    StrokeKind::Inside,
                );
                widgets::text_in(
                    ui,
                    save_rect.shrink2(Vec2::new(8.0, 0.0)),
                    save_label,
                    font(12.0, Weight::Medium),
                    Color32::WHITE,
                );
                if cancel.hovered() || (save.hovered() && !too_long) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                clicked_cancel = cancel.clicked();
                clicked_save = save.clicked() && !too_long;
            });

        let index = editor.index;
        let commit = |app: &mut App, comment: &str| {
            let next = editor.citation.with_comment(comment);
            app.replace_composer_citation(index, Some(&next));
        };
        if (save || send || clicked_save) && !too_long {
            commit(self, &editor.draft);
            self.cite.send = send;
            self.focus_composer_after_chip(ui, index);
            return;
        }
        if cancel || clicked_cancel {
            if editor.remove_on_cancel {
                self.replace_composer_citation(index, None);
            }
            self.focus_composer_after_chip(ui, index);
            return;
        }
        // A press elsewhere keeps a changed draft (`resolveAssistantCitationCommentDismissal`).
        let on_pencil = self
            .cite
            .chip_rects
            .get(index)
            .zip(ui.input(|i| i.pointer.interact_pos()))
            .is_some_and(|(chip, at)| chip.contains(at));
        if outside_press && !on_pencil {
            let saved = editor.citation.comment.clone().unwrap_or_default();
            if editor.draft.trim() == saved.trim() {
                return;
            }
            if too_long {
                self.cite.comment = Some(editor);
                return;
            }
            commit(self, &editor.draft);
            return;
        }
        self.cite.comment = Some(editor);
    }

    /// `onRestoreFocus`: the caret right after the chip.
    fn focus_composer_after_chip(&mut self, ui: &Ui, index: usize) {
        let Some(id) = self.cite.editor_id else {
            return;
        };
        let display = collapse(&self.composer).display;
        let position = display
            .chars()
            .position(|c| placeholder_index(c) == Some(index))
            .map_or(display.chars().count(), |p| p + 1);
        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(position),
            )));
        state.store(ui.ctx(), id);
        ui.memory_mut(|m| m.request_focus(id));
    }

    /// After an edit that changed the prompt's links (a pasted link turns
    /// into its chip), the caret keeps its place in the prompt.
    /// Inserts a context record's reference at the composer's caret (else
    /// at the end, `appendInlineContextReference`) and keeps the record.
    pub(crate) fn add_composer_context(&mut self, ctx: &egui::Context, record: serde_json::Value) {
        let kind = record["kind"].as_str().unwrap_or_default();
        let reference = crate::ui::composer_context::format_reference(
            kind,
            record["contextId"].as_str().unwrap_or_default(),
            record["label"].as_str().unwrap_or_default(),
        );
        let collapsed = collapse(&self.composer);
        let caret = self.cite.editor_id.and_then(|id| {
            let range = egui::TextEdit::load_state(ctx, id)?.cursor.char_range()?;
            Some(display_to_prompt(
                &collapsed.display,
                &collapsed.tokens,
                range.primary.index.0,
            ))
        });
        let at = caret.unwrap_or(self.composer.len());
        let (prompt, cursor) =
            crate::ui::composer_context::insert_reference(&self.composer, at, &reference);
        self.composer = prompt;
        self.composer_contexts.push(record);
        if let Some(id) = self.cite.editor_id {
            let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
            let index = prompt_to_display(&self.composer, cursor);
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(index),
                )));
            state.store(ctx, id);
            ctx.memory_mut(|m| m.request_focus(id));
        }
    }

    pub(crate) fn remap_composer_caret(
        &self,
        ctx: &egui::Context,
        edited: &str,
        tokens: &[(Chip, String)],
    ) {
        let Some(id) = self.cite.editor_id else {
            return;
        };
        let Some(mut state) = egui::TextEdit::load_state(ctx, id) else {
            return;
        };
        let Some(range) = state.cursor.char_range() else {
            return;
        };
        let map = |index: usize| {
            let byte = display_to_prompt(edited, tokens, index);
            egui::text::CCursor::new(prompt_to_display(&self.composer, byte))
        };
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                map(range.secondary.index.0),
                map(range.primary.index.0),
            )));
        state.store(ctx, id);
    }
}

/// Sets the links a copy out of the composer restores.
pub(crate) fn set_copy_tokens(collapsed: &Collapsed) {
    if let Ok(mut tokens) = copy_tokens().lock() {
        *tokens = collapsed.tokens.iter().map(|(_, s)| s.clone()).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(text: &str) -> String {
        Citation {
            environment_id: "e".into(),
            thread_id: "t".into(),
            message_id: "m".into(),
            text: text.into(),
            start: 0,
            end: text.len() as u64,
            ..Citation::default()
        }
        .serialize()
    }

    #[test]
    fn the_editor_shows_one_character_per_citation() {
        let mut prompt = format!("Voir {}\nfin {}", link("a"), link("b"));
        pad_chips(&mut prompt);
        assert_eq!(prompt, format!("Voir {} \nfin {} ", link("a"), link("b")));
        let collapsed = collapse(&prompt);
        assert_eq!(collapsed.display, "Voir \u{F0000} \nfin \u{F0001} ");
        assert_eq!(expand(&collapsed.display, &collapsed.tokens), prompt);
        // Deleting the first chip keeps the second.
        let edited = collapsed.display.replacen('\u{F0000}', "", 1);
        assert_eq!(
            expand(&edited, &collapsed.tokens),
            format!("Voir  \nfin {} ", link("b"))
        );
        let byte = display_to_prompt(&collapsed.display, &collapsed.tokens, 6);
        assert_eq!(prompt_to_display(&prompt, byte), 6);
        assert_eq!(prompt_to_display(&prompt, 7), 6);
    }

    #[test]
    fn the_reveal_pulses_twice_then_fades() {
        assert_eq!(pulse_alpha(0.0), 0.0);
        assert!(pulse_alpha(TOTAL_MS * 0.6) > 0.44);
        assert_eq!(pulse_alpha(TOTAL_MS), 0.0);
    }
}
