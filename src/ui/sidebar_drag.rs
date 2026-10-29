//! Dragging sidebar rows (`Sidebar.tsx`'s drop plans, `threadContextDrag.ts`):
//! a row dropped on another section pins, unpins, settles, un-settles or
//! wakes it; a row taken out of the list and dropped on the composer becomes
//! a thread context there. While dragging, the target section is marked and a
//! card follows the pointer with what the drop will do. Pluto keeps its own
//! order inside a section (working threads first), so a drop never reorders.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};

use crate::app::App;
use crate::model::Thread;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Zone {
    Pinned,
    Active,
    Snoozed,
    Settled,
}

pub(crate) struct Drag {
    pub thread: String,
    pub from: Zone,
}

/// Where a thread sits.
pub(crate) fn zone_of(thread: &Thread, now: chrono::DateTime<chrono::Utc>) -> Zone {
    if thread.settled_override.as_deref() == Some("settled") {
        Zone::Settled
    } else if crate::snooze::effective_snoozed(thread, now) {
        Zone::Snoozed
    } else if thread.pinned_at.is_some() {
        Zone::Pinned
    } else {
        Zone::Active
    }
}

/// `resolveSidebarDropVerb`: what a drop from `from` on `to` does, with its
/// badge's icon.
pub(crate) fn verb(from: Zone, to: Zone) -> Option<(&'static str, &'static str)> {
    match (from, to) {
        (a, b) if a == b => None,
        (_, Zone::Pinned) if from != Zone::Settled && from != Zone::Snoozed => Some(("Pin", "pin")),
        (_, Zone::Settled) => Some(("Settle", "circle-check")),
        (Zone::Pinned, Zone::Active) => Some(("Unpin", "pin-off")),
        (Zone::Settled, Zone::Active) => Some(("Un-settle", "undo-2")),
        (Zone::Snoozed, Zone::Active) => Some(("Wake", "alarm-clock-off")),
        _ => None,
    }
}

impl App {
    /// A row's response: a drag past the threshold picks it up.
    pub(crate) fn sidebar_row_drag(&mut self, response: &egui::Response, thread: &Thread) {
        if response.drag_started() && self.renaming.is_none() {
            self.sidebar_drag = Some(Drag {
                thread: thread.id.clone(),
                from: zone_of(thread, self.now),
            });
        }
    }

    /// The zone under `pointer` among this frame's sections.
    fn zone_at(&self, pointer: Pos2) -> Option<Zone> {
        self.sidebar_zones
            .iter()
            .find(|(_, rect)| rect.contains(pointer))
            .map(|(zone, _)| *zone)
    }

    /// While dragging: the target's mark and the card at the pointer; on
    /// release, the drop. `list` is the thread list's rect.
    pub(crate) fn sidebar_drag_frame(&mut self, ui: &mut Ui, list: Rect) {
        let Some(drag) = &self.sidebar_drag else {
            return;
        };
        let Some(thread) = self
            .state
            .threads
            .iter()
            .find(|t| t.id == drag.thread)
            .cloned()
        else {
            self.sidebar_drag = None;
            return;
        };
        let from = drag.from;
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.sidebar_drag = None;
            return;
        }
        let pointer = ui.input(|i| i.pointer.latest_pos());
        let released = ui.input(|i| !i.pointer.primary_down());
        // Out of the list sideways, it carries the thread as context.
        let context = pointer.is_some_and(|p| p.x < list.left() || p.x > list.right());
        let target = pointer.filter(|_| !context).and_then(|p| self.zone_at(p));
        let action = target.and_then(|to| verb(from, to));
        let over_composer = context
            && pointer
                .zip(self.composer_rect)
                .is_some_and(|(p, rect)| rect.contains(p));
        if released {
            self.sidebar_drag = None;
            if over_composer {
                self.drop_thread_context(&thread);
            } else if let (Some(to), Some(_)) = (target, action) {
                self.apply_drop(&thread, from, to);
            }
            return;
        }
        ui.ctx().request_repaint();
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("sidebar-drag"),
        ));
        // The target section, marked in the primary color.
        if let (Some(to), Some(_)) = (target, action)
            && let Some((_, rect)) = self.sidebar_zones.iter().find(|(zone, _)| *zone == to)
        {
            let primary = theme::primary();
            painter.rect(
                *rect,
                CornerRadius::same(8),
                theme::alpha(primary, 0.05),
                Stroke::new(1.0, theme::alpha(primary, 0.4)),
                StrokeKind::Inside,
            );
            // The shelves name themselves; the cards' sections get a label.
            let label = match to {
                Zone::Pinned => "Pinned",
                Zone::Active => "Active",
                Zone::Snoozed | Zone::Settled => "",
            };
            painter.text(
                Pos2::new(rect.left() + 8.0, rect.top() + 4.0),
                egui::Align2::LEFT_TOP,
                label,
                font(12.0, Weight::Medium),
                primary,
            );
        }
        if over_composer && let Some(rect) = self.composer_rect {
            painter.rect(
                rect,
                CornerRadius::same(24),
                theme::alpha(theme::accent(), 0.45),
                Stroke::new(1.0, theme::alpha(theme::primary(), 0.7)),
                StrokeKind::Inside,
            );
        }
        let Some(pointer) = pointer else {
            return;
        };
        // The card: the title and, in the list, the drop's badge; outside it
        // the context ghost (`ThreadContextDragGhost`).
        let title = if thread.title.trim().is_empty() {
            "Thread".to_owned()
        } else {
            thread.title.trim().to_owned()
        };
        let small = font(14.0, Weight::Regular);
        let title_width = widgets::text_width(ui, &title, small.clone()).min(200.0);
        let badge = action.filter(|_| !context);
        let badge_width = badge.map_or(0.0, |(label, _)| {
            8.0 + 12.0
                + 4.0
                + widgets::text_width(ui, label, font(11.0, Weight::Medium))
                + 6.0
                + 12.0
        });
        let rect = Rect::from_min_size(
            pointer + Vec2::new(12.0, 12.0),
            Vec2::new(12.0 + 16.0 + 8.0 + title_width + badge_width + 12.0, 36.0),
        );
        painter.add(
            egui::epaint::Shadow {
                offset: [0, 6],
                blur: 16,
                spread: 0,
                color: Color32::from_black_alpha(80),
            }
            .as_shape(rect, CornerRadius::same(10)),
        );
        painter.rect(
            rect,
            CornerRadius::same(10),
            theme::popover(),
            Stroke::new(1.0, theme::border()),
            StrokeKind::Inside,
        );
        let child = ui.new_child(
            egui::UiBuilder::new()
                .layer_id(painter.layer_id())
                .max_rect(rect),
        );
        theme::paint_icon(
            &child,
            "messages-square",
            Rect::from_center_size(
                Pos2::new(rect.left() + 20.0, rect.center().y),
                Vec2::splat(16.0),
            ),
            16.0,
            theme::secondary_label(),
        );
        widgets::text_in(
            &child,
            Rect::from_min_size(
                Pos2::new(rect.left() + 36.0, rect.top()),
                Vec2::new(title_width, rect.height()),
            ),
            &title,
            small,
            theme::foreground(),
        );
        if let Some((label, icon)) = badge {
            let primary = theme::primary();
            let b = Rect::from_min_size(
                Pos2::new(rect.right() - badge_width, rect.center().y - 10.0),
                Vec2::new(badge_width - 12.0, 20.0),
            );
            painter.rect(
                b,
                CornerRadius::same(6),
                theme::alpha(primary, 0.1),
                Stroke::new(1.0, theme::alpha(primary, 0.4)),
                StrokeKind::Inside,
            );
            theme::paint_icon(
                &child,
                icon,
                Rect::from_center_size(
                    Pos2::new(b.left() + 6.0 + 6.0, b.center().y),
                    Vec2::splat(12.0),
                ),
                12.0,
                primary,
            );
            widgets::text_in(
                &child,
                Rect::from_min_max(Pos2::new(b.left() + 22.0, b.top()), b.max),
                label,
                font(11.0, Weight::Medium),
                primary,
            );
        }
    }

    /// Runs a drop's commands.
    fn apply_drop(&mut self, thread: &Thread, from: Zone, to: Zone) {
        match (from, to) {
            (_, Zone::Pinned) => {
                let pin = self.pin_command(&thread.id);
                self.dispatch_titled(pin, "Failed to pin thread");
            }
            (_, Zone::Settled) => {
                let settle = crate::model::command("thread.settle", &thread.id);
                self.park(&thread.id, settle, crate::ui::park::Park::Settle, &[]);
            }
            (Zone::Pinned, Zone::Active) => self.unpin(thread),
            (Zone::Settled, Zone::Active) => {
                let mut command = crate::model::command("thread.unsettle", &thread.id);
                command["reason"] = serde_json::json!("user");
                self.dispatch_titled(command, "Failed to un-settle thread");
            }
            (Zone::Snoozed, Zone::Active) => {
                let mut command = crate::model::command("thread.unsnooze", &thread.id);
                command["reason"] = serde_json::json!("user");
                self.dispatch_titled(command, "Failed to wake thread");
            }
            _ => {}
        }
    }

    /// A thread dropped on the composer: its context chip at the end.
    fn drop_thread_context(&mut self, thread: &Thread) {
        if self.selected.as_deref() == Some(thread.id.as_str()) {
            return;
        }
        let environment = self.state.config["environment"]["environmentId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let record =
            crate::ui::composer_context::thread_record(&thread.id, &environment, &thread.title);
        let id = record["contextId"].as_str().unwrap_or_default().to_owned();
        let reference = crate::ui::composer_context::format_reference(
            "thread",
            &id,
            record["label"].as_str().unwrap_or_default(),
        );
        if !self
            .composer_contexts
            .iter()
            .any(|r| r["contextId"] == id.as_str())
        {
            self.composer_contexts.push(record);
        }
        if !self.composer.is_empty() && !self.composer.ends_with(char::is_whitespace) {
            self.composer.push(' ');
        }
        self.composer.push_str(&reference);
        self.composer.push(' ');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_read_like_upstream() {
        assert_eq!(verb(Zone::Active, Zone::Pinned).map(|v| v.0), Some("Pin"));
        assert_eq!(verb(Zone::Pinned, Zone::Active).map(|v| v.0), Some("Unpin"));
        assert_eq!(
            verb(Zone::Settled, Zone::Active).map(|v| v.0),
            Some("Un-settle")
        );
        assert_eq!(verb(Zone::Snoozed, Zone::Active).map(|v| v.0), Some("Wake"));
        assert_eq!(
            verb(Zone::Pinned, Zone::Settled).map(|v| v.0),
            Some("Settle")
        );
        assert_eq!(verb(Zone::Active, Zone::Active), None);
        assert_eq!(verb(Zone::Active, Zone::Snoozed), None);
    }
}
