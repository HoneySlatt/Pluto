//! The action sweep (`Sidebar.tsx`'s sweep gesture): press a row's Settle,
//! Un-settle or Wake and drag along the list to apply it to every row
//! between, in the origin's section. Swept rows take the selected surface
//! and show the verb; the release applies it, a press that never moved runs
//! the button's own click.

use eframe::egui::{self, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};

use crate::app::App;
use crate::model::Thread;
use crate::theme::{self, Weight, font};
use crate::ui::sidebar_drag::{Zone, zone_of};
use crate::ui::widgets;

pub(crate) struct Sweep {
    /// `settle`, `unsettle` or `unsnooze`.
    pub action: &'static str,
    pub zone: Zone,
    pub origin: String,
    pub keys: Vec<String>,
}

/// The badge a swept row shows, and its icon.
fn verb(action: &str) -> (&'static str, &'static str) {
    match action {
        "settle" => ("Settle", "circle-check"),
        "unsettle" => ("Un-settle", "undo-2"),
        _ => ("Wake", "alarm-clock-off"),
    }
}

impl App {
    /// An action button's response: a drag starts the sweep from its row.
    pub(crate) fn sweep_button(
        &mut self,
        response: &egui::Response,
        thread: &Thread,
        action: &'static str,
    ) {
        if response.drag_started() {
            self.sidebar_sweep = Some(Sweep {
                action,
                zone: zone_of(thread, self.now),
                origin: thread.id.clone(),
                keys: vec![thread.id.clone()],
            });
        }
    }

    /// Whether `thread` is in the sweep.
    pub(crate) fn swept(&self, thread: &str) -> bool {
        self.sidebar_sweep
            .as_ref()
            .is_some_and(|s| s.keys.iter().any(|k| k == thread))
    }

    /// The swept row's verb badge, over its status slot.
    pub(crate) fn sweep_badge(&self, ui: &Ui, row: Rect) {
        let Some(sweep) = &self.sidebar_sweep else {
            return;
        };
        let (label, icon) = verb(sweep.action);
        let primary = theme::primary();
        let width =
            6.0 + 12.0 + 4.0 + widgets::text_width(ui, label, font(11.0, Weight::Medium)) + 6.0;
        let badge = Rect::from_min_size(
            Pos2::new(row.right() - 10.0 - width, row.top() + 8.0),
            Vec2::new(width, 20.0),
        );
        let painter = ui.painter();
        painter.rect_filled(badge.expand(2.0), CornerRadius::same(6), theme::muted());
        painter.rect(
            badge,
            CornerRadius::same(6),
            theme::alpha(primary, 0.1),
            Stroke::new(1.0, theme::alpha(primary, 0.4)),
            StrokeKind::Inside,
        );
        theme::paint_icon(
            ui,
            icon,
            Rect::from_center_size(
                Pos2::new(badge.left() + 12.0, badge.center().y),
                Vec2::splat(12.0),
            ),
            12.0,
            primary,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(badge.left() + 22.0, badge.top()), badge.max),
            label,
            font(11.0, Weight::Medium),
            primary,
        );
    }

    /// While sweeping: the rows between the origin and the pointer; on
    /// release, the action on them. `list` is the thread list's viewport.
    pub(crate) fn sweep_frame(&mut self, ui: &Ui, list: Rect) {
        let Some(sweep) = &self.sidebar_sweep else {
            return;
        };
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.sidebar_sweep = None;
            return;
        }
        let capable = if sweep.action == "unsnooze" {
            self.capability("threadSnooze")
        } else {
            self.capability("threadSettlement")
        };
        // `sidebarThreadKeyAtY`: clamped to the viewport; a gap between
        // rows counts as the row above.
        let rows: Vec<&(String, Zone, Rect)> = self
            .sidebar_rows
            .iter()
            .filter(|(_, zone, _)| *zone == sweep.zone)
            .collect();
        if let Some(pointer) = ui.input(|i| i.pointer.latest_pos())
            && let Some(origin) = rows.iter().position(|(id, ..)| *id == sweep.origin)
        {
            let y = pointer.y.clamp(list.top(), list.bottom());
            let under = rows
                .iter()
                .rposition(|(.., rect)| rect.top() <= y)
                .unwrap_or(0);
            let (from, to) = (origin.min(under), origin.max(under));
            let keys = rows[from..=to]
                .iter()
                .map(|(id, ..)| id.clone())
                .filter(|_| capable)
                .collect();
            if let Some(sweep) = &mut self.sidebar_sweep {
                sweep.keys = keys;
            }
        }
        if ui.input(|i| i.pointer.primary_down()) {
            ui.ctx().request_repaint();
            return;
        }
        let Some(sweep) = self.sidebar_sweep.take() else {
            return;
        };
        let threads: Vec<_> = sweep
            .keys
            .iter()
            .filter_map(|id| self.state.threads.iter().find(|t| &t.id == id).cloned())
            .collect();
        match sweep.action {
            "settle" => {
                let ids: Vec<String> = threads.iter().map(|t| t.id.clone()).collect();
                for thread in threads
                    .iter()
                    .filter(|t| t.settled_override.as_deref() != Some("settled"))
                {
                    let settle = crate::model::command("thread.settle", &thread.id);
                    self.park(&thread.id, settle, crate::ui::park::Park::Settle, &ids);
                }
            }
            kind => {
                let (command, failure) = if kind == "unsnooze" {
                    ("thread.unsnooze", "Failed to wake thread")
                } else {
                    ("thread.unsettle", "Failed to un-settle thread")
                };
                for thread in &threads {
                    let mut cmd = crate::model::command(command, &thread.id);
                    cmd["reason"] = serde_json::json!("user");
                    self.dispatch_titled(cmd, failure);
                }
            }
        }
    }
}
