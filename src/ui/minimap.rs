//! The timeline's minimap (`MessagesTimeline`'s `TimelineMinimap`,
//! `timelineMinimapItems.ts`): one marker per user message on a rail in the
//! left gutter, those on screen lit; hovering the strip widens the marker
//! under the pointer and its neighbours and shows that turn's prompt and
//! answer; a click scrolls to it. Hidden below two turns, and inert when
//! the gutter is too narrow.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// A user message's row: its top and height in the timeline's content, and
/// the turn's texts.
pub(crate) struct Item {
    pub top: f32,
    pub height: f32,
    pub user: Option<String>,
    pub assistant: Option<String>,
}

/// `TIMELINE_MINIMAP_PERSISTENT_GUTTER`: from this gutter it always shows.
const PERSISTENT_GUTTER: f32 = 48.0;
const SPACING: f32 = 8.0;

/// Whitespace collapsed, `None` when empty (`timelineMinimapItems.ts`).
pub(crate) fn preview_text(text: &str) -> Option<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

/// The marker's width scale by its distance to the hovered one.
fn scale(distance: Option<usize>) -> f32 {
    match distance {
        Some(0) => 1.0,
        Some(1) => 0.667,
        Some(2) => 0.417,
        _ => 0.333,
    }
}

/// Draws the minimap in `viewport`'s left `gutter`; the scroll offset a
/// click asks for. `scroll_top` and the viewport's height say what is on
/// screen.
pub(crate) fn show(
    ui: &mut Ui,
    viewport: Rect,
    gutter: f32,
    items: &[Item],
    scroll_top: f32,
) -> Option<f32> {
    let n = items.len();
    if n < 2 {
        return None;
    }
    let strip_width = (gutter.floor() - 12.0).clamp(0.0, 40.0);
    let height = ((n - 1) as f32 * SPACING)
        .min(viewport.height() - 288.0)
        .max(SPACING);
    let top = viewport.center().y - height / 2.0;
    let left = viewport.left() + 12.0;
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let hit = Rect::from_min_max(
        Pos2::new(left, top - 6.0),
        Pos2::new(left + strip_width, top + height + 6.0),
    );
    let open_id = egui::Id::new("timeline-minimap-open");
    let was_open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    // While the preview shows, the strip widens to reach it (`22rem`).
    let reach = if was_open {
        Rect::from_min_max(hit.min, Pos2::new(left + 352.0, hit.max.y))
    } else {
        hit
    };
    let hovered = strip_width > 0.0 && pointer.is_some_and(|p| reach.contains(p));
    ui.data_mut(|d| d.insert_temp(open_id, hovered));
    let opacity = if gutter >= PERSISTENT_GUTTER || hovered {
        1.0
    } else {
        0.0
    };
    if opacity == 0.0 {
        return None;
    }
    let active = pointer.filter(|_| hovered).map(|p| {
        let t = ((p.y - top) / height).clamp(0.0, 1.0);
        (t * (n - 1) as f32).round() as usize
    });
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("timeline-minimap"),
    ));
    // The rail.
    painter.rect_filled(
        Rect::from_min_size(Pos2::new(left + 12.0, top), Vec2::new(1.0, height)),
        0.0,
        theme::alpha(theme::border(), 0.15),
    );
    let view_bottom = scroll_top + viewport.height();
    for (index, item) in items.iter().enumerate() {
        let y = top + index as f32 / (n - 1) as f32 * height;
        let distance = active.map(|a| a.abs_diff(index));
        let width = 24.0 * scale(distance);
        let marker = Rect::from_min_size(Pos2::new(left + 12.0, y - 1.0), Vec2::new(width, 2.0));
        let color = if distance == Some(0) {
            theme::alpha(theme::muted_foreground(), 0.75)
        } else {
            theme::alpha(theme::muted_foreground(), 0.35)
        };
        painter.rect_filled(marker, CornerRadius::same(1), color);
        let in_view = item.top < view_bottom && item.top + item.height.max(1.0) > scroll_top;
        if in_view {
            painter.rect_filled(
                marker,
                CornerRadius::same(1),
                theme::alpha(theme::foreground(), 0.9),
            );
        }
    }
    let response = ui.interact(hit, egui::Id::new("timeline-minimap-strip"), Sense::click());
    let response = if active.is_some() {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };
    // The active turn's preview, beside its marker.
    if let Some(index) = active {
        let item = &items[index];
        let user = item.user.clone().unwrap_or_else(|| "User message".into());
        let inner = 320.0 - 24.0;
        let user_galley = widgets::galley_truncated(
            ui,
            &user,
            font(14.0, Weight::Medium),
            theme::foreground(),
            20.0,
            inner,
        );
        let answer = item.assistant.as_ref().map(|text| {
            let mut job = egui::text::LayoutJob::single_section(
                text.clone(),
                widgets::format(font(14.0, Weight::Regular), theme::muted_foreground(), 20.0),
            );
            job.wrap.max_width = inner;
            job.wrap.max_rows = 3;
            job.wrap.overflow_character = Some('…');
            ui.fonts_mut(|f| f.layout_job(job))
        });
        let box_height = 12.0 + 20.0 + answer.as_ref().map_or(0.0, |g| 4.0 + g.size().y) + 12.0;
        let y = top + index as f32 / (n - 1) as f32 * height;
        let box_top = if index == 0 {
            y
        } else if index == n - 1 {
            y - box_height
        } else {
            y - box_height / 2.0
        };
        let rect = Rect::from_min_size(
            Pos2::new(left + 32.0, box_top),
            Vec2::new(320.0, box_height),
        );
        painter.add(
            egui::epaint::Shadow {
                offset: [0, 10],
                blur: 25,
                spread: 0,
                color: Color32::from_black_alpha(60),
            }
            .as_shape(rect, CornerRadius::same(12)),
        );
        painter.rect(
            rect,
            CornerRadius::same(12),
            widgets::menu_surface(),
            Stroke::new(1.0, widgets::menu_outline()),
            StrokeKind::Inside,
        );
        painter.galley(
            rect.min + Vec2::new(12.0, 12.0),
            user_galley,
            theme::foreground(),
        );
        if let Some(answer) = answer {
            painter.galley(
                rect.min + Vec2::new(12.0, 12.0 + 20.0 + 4.0),
                answer,
                theme::muted_foreground(),
            );
        }
    }
    // A click scrolls its turn to 24px under the top.
    if response.clicked()
        && let Some(index) = active
    {
        return Some((items[index].top - 24.0).max(0.0));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_collapse_whitespace_and_markers_narrow_with_distance() {
        assert_eq!(
            preview_text("  Fix\n the   build "),
            Some("Fix the build".into())
        );
        assert_eq!(preview_text(" \n "), None);
        assert_eq!(scale(Some(0)), 1.0);
        assert_eq!(scale(Some(3)), 0.333);
        assert_eq!(scale(None), 0.333);
    }
}
