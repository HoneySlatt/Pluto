//! The live activity row (`MessagesTimeline.tsx`'s `LiveWorkEntryTimelineRow`,
//! `ThinkingTimelineRow` and `LiveActivityRow`): while a turn works, its
//! trailing tool calls collapse into one row that names the running call (or
//! the last one, past tense, until the next starts), under the latest
//! thought's first sentence, and "Thinking" stands in when nothing runs. A
//! click opens the calls. The labels sweep with upstream's highlight
//! (`live-tool-shine`, `live-activity-focus`: a 4.5rem band across the text
//! every 2.2s in 30 steps).
//!
//! Not ported: question answers and thread-read targets in the label, and the
//! reasoning label's markdown (it reads as plain text).

use eframe::egui::{Color32, CornerRadius, CursorIcon, Pos2, Rect, Response, Sense, Ui, Vec2};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::timeline::{self, WorkEntry};
use crate::ui::widgets;

const LINE: f32 = 22.75;
/// `WorkLogRow`: `py-0.5` around a `min-h-6` line.
const ROW: f32 = 28.0;
/// The highlight band (`--live-activity-focus-width`).
const BAND: f32 = 72.0;
const PERIOD: f32 = 2.2;
const STEPS: f32 = 30.0;

/// The band's mask: `(offset, opacity)` stops.
const MASK: [(f32, f32); 7] = [
    (0.0, 0.0),
    (10.8, 0.12),
    (25.2, 0.55),
    (36.0, 1.0),
    (46.8, 0.55),
    (61.2, 0.12),
    (72.0, 0.0),
];

fn mask(offset: f32) -> f32 {
    for pair in MASK.windows(2) {
        let ((a, va), (b, vb)) = (pair[0], pair[1]);
        if offset >= a && offset <= b {
            return va + (vb - va) * (offset - a) / (b - a);
        }
    }
    0.0
}

/// Paints `text` truncated within `rect`, in `base`, with the sweeping band
/// of `highlight` when `sweep` is on.
fn label(ui: &Ui, rect: Rect, text: &str, base: Color32, highlight: Color32, sweep: bool) {
    let font = font(14.0, Weight::Regular);
    sweep_label(ui, rect, text, font, LINE, base, highlight, sweep);
}

/// [`label`] at any type size; returns the text's width.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sweep_label(
    ui: &Ui,
    rect: Rect,
    text: &str,
    font: eframe::egui::FontId,
    line: f32,
    base: Color32,
    highlight: Color32,
    sweep: bool,
) -> f32 {
    let galley = widgets::galley_truncated(ui, text, font.clone(), base, line, rect.width());
    let pos = Pos2::new(rect.left(), rect.center().y - line / 2.0);
    let width = galley.size().x;
    ui.painter().galley(pos, galley, base);
    if !sweep {
        return width;
    }
    let time = ui.input(|i| i.time) as f32;
    let step = ((time % PERIOD) / PERIOD * STEPS).floor() / STEPS;
    let band_left = pos.x - BAND + step * (width + 2.0 * BAND);
    let text_rect = Rect::from_min_size(pos, Vec2::new(width, line));
    let lit = widgets::galley_truncated(ui, text, font, highlight, line, rect.width());
    const STRIP: f32 = 4.0;
    let mut x = band_left;
    while x < band_left + BAND {
        let strip = Rect::from_min_max(Pos2::new(x, pos.y), Pos2::new(x + STRIP, pos.y + line))
            .intersect(text_rect);
        let opacity = mask(x + STRIP / 2.0 - band_left);
        if strip.width() > 0.0 && opacity > 0.0 {
            ui.painter()
                .with_clip_rect(strip.intersect(ui.clip_rect()))
                .galley(pos, lit.clone(), theme::alpha(highlight, opacity));
        }
        x += STRIP;
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs_f32(PERIOD / STEPS));
    width
}

/// `ContextCompactionTimelineRow`: the label between two hairlines (`py-1
/// gap-3 text-xs`), sweeping while the compaction runs.
pub(crate) fn compaction_row(ui: &mut Ui, text: &str, active: bool, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
    let muted = theme::muted_foreground();
    let label_font = font(12.0, Weight::Regular);
    let text_width =
        widgets::text_width(ui, text, label_font.clone()).min(width - 12.0 - 6.0 - 2.0 * 24.0);
    let content = 12.0 + 6.0 + text_width;
    let left = rect.center().x - content / 2.0;
    let line = theme::alpha(theme::border(), 0.7);
    let y = rect.center().y;
    for (from, to) in [
        (rect.left(), left - 12.0),
        (left + content + 12.0, rect.right()),
    ] {
        if to > from {
            ui.painter().rect_filled(
                Rect::from_min_max(Pos2::new(from, y - 0.5), Pos2::new(to, y + 0.5)),
                0.0,
                line,
            );
        }
    }
    theme::paint_icon(
        ui,
        "minimize-2",
        Rect::from_center_size(Pos2::new(left + 6.0, y), Vec2::splat(12.0)),
        12.0,
        muted,
    );
    sweep_label(
        ui,
        Rect::from_min_max(
            Pos2::new(left + 18.0, rect.top()),
            Pos2::new(left + content + 1.0, rect.bottom()),
        ),
        text,
        label_font,
        16.0,
        muted,
        theme::foreground(),
        active,
    );
}

impl App {
    /// `LiveActivityRow` in a `WorkLogRow`: the icon box, then the label.
    fn live_line(&self, ui: &Ui, rect: Rect, icon: &str, text: &str, failed: bool, sweep: bool) {
        let icon_color = if failed {
            theme::alpha(Color32::from_rgb(251, 65, 74), 0.4)
        } else {
            theme::muted_foreground()
        };
        crate::ui::chat::paint_work_icon(
            ui,
            icon,
            Rect::from_center_size(
                Pos2::new(rect.left() + 2.0 + 12.0, rect.center().y),
                Vec2::splat(16.0),
            ),
            16.0,
            icon_color,
        );
        let text_rect = Rect::from_min_max(
            Pos2::new(rect.left() + 2.0 + 24.0 + 6.0, rect.top()),
            Pos2::new(rect.right() - 2.0, rect.bottom()),
        );
        label(
            ui,
            text_rect,
            text,
            theme::secondary_label(),
            theme::foreground(),
            sweep && !failed,
        );
    }

    /// `LiveWorkEntryTimelineRow`: the latest thought, then the live call.
    pub(crate) fn live_work_row(
        &mut self,
        ui: &mut Ui,
        entry: &WorkEntry,
        thought: Option<&str>,
        active: bool,
        width: f32,
    ) -> Response {
        let thought_line = thought
            .map(timeline::live_thought_line)
            .filter(|line| !line.is_empty());
        let thought_galley = thought_line.as_ref().map(|line| {
            widgets::galley(
                ui,
                line,
                font(14.0, Weight::Regular),
                theme::secondary_label(),
                LINE,
                width - 4.0 - 24.0 - 6.0,
            )
        });
        // `line-clamp-4`.
        let thought_height = thought_galley
            .as_ref()
            .map_or(0.0, |g| 4.0 + g.size().y.clamp(24.0, 4.0 * LINE));
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(width, thought_height + ROW), Sense::hover());
        let response = ui
            .interact(rect, ui.id().with(("live-work", &entry.id)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), theme::overlay(0.008));
        }
        if let Some(galley) = thought_galley {
            theme::paint_icon(
                ui,
                "brain",
                Rect::from_center_size(
                    Pos2::new(rect.left() + 2.0 + 12.0, rect.top() + 2.0 + 12.0),
                    Vec2::splat(16.0),
                ),
                16.0,
                theme::muted_foreground(),
            );
            let clip = Rect::from_min_size(
                Pos2::new(rect.left() + 2.0 + 24.0 + 6.0, rect.top() + 2.0),
                Vec2::new(galley.size().x, galley.size().y.min(4.0 * LINE)),
            );
            ui.painter()
                .with_clip_rect(clip.intersect(ui.clip_rect()))
                .galley(clip.min, galley, theme::secondary_label());
        }
        let line = Rect::from_min_max(
            Pos2::new(rect.left(), rect.top() + thought_height),
            rect.max,
        );
        // While the thought itself streams, the status says Thinking.
        let thought_is_status = thought_line.is_some()
            && entry.item_type.as_deref() == Some("reasoning")
            && entry.detail.as_deref() == thought;
        let text = if thought_is_status {
            if active { "Thinking" } else { "Thought" }.to_owned()
        } else {
            let label = timeline::live_label(entry, active);
            // The live row names a read thread without linking it.
            match self.thread_read_label(entry, &label) {
                Some((prefix, title, _)) => format!("{prefix} \u{201c}{title}\u{201d}"),
                None => label,
            }
        };
        let failed = timeline::indicates_failure(entry);
        self.live_line(
            ui,
            line,
            crate::ui::chat::work_icon(entry),
            &text,
            failed,
            active,
        );
        if let Some(item) = entry.item.as_deref() {
            self.tool_image_icon(
                ui,
                Rect::from_min_size(line.min + Vec2::new(2.0, 0.0), Vec2::new(24.0, ROW)),
                item,
                !active,
            );
        }
        response
    }

    /// `ThinkingTimelineRow`: a button when it stands for a failed group.
    pub(crate) fn thinking_row(&mut self, ui: &mut Ui, opens: bool, width: f32) -> Response {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, ROW), Sense::hover());
        let response = ui.interact(
            rect,
            ui.id().with("live-thinking"),
            if opens {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        self.live_line(ui, rect, "brain", "Thinking", false, true);
        if opens {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_fades_in_and_out() {
        assert_eq!(mask(0.0), 0.0);
        assert_eq!(mask(36.0), 1.0);
        assert!((mask(18.0) - 0.335).abs() < 0.01);
        assert_eq!(mask(80.0), 0.0);
    }
}
