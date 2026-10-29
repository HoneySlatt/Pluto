//! Undoing a thread's park (`threadUndo.ts`, `showThreadUndoNotice.ts`,
//! `SidebarThreadUndoNotice.tsx`): once the server took a settle, snooze,
//! unpin or archive, the sidebar's footer says "Settled 1 thread, Ctrl+Z to
//! undo" for 5 s; Mod+Z or the link sends the commands that put it back.
//! Consecutive entries of one action group together and undo together. A
//! later opposite action (unsettling a settled thread…) expires its undo.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::Value;

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const WINDOW: Duration = Duration::from_secs(5);
/// The notice's height in the footer, its gap included.
pub(crate) const NOTICE: f32 = 6.0 + 16.0 + 6.0 + 8.0;

/// What a successful command can undo.
#[derive(Clone, Debug)]
pub(crate) struct Undoable {
    /// "Settled", "Snoozed", "Unpinned", "Archived".
    pub action: &'static str,
    /// The claim's kind (`settle`, `snooze`, `pin`, `archive`).
    pub kind: &'static str,
    pub thread: String,
    /// Sent in order to undo it.
    pub commands: Vec<Value>,
    pub failure: &'static str,
}

#[derive(Default)]
pub(crate) struct Undo {
    claims: HashMap<(&'static str, String), u64>,
    live: Vec<(u64, Undoable)>,
    next: u64,
    deadline: Option<Instant>,
}

impl Undo {
    /// `ThreadUndo.invalidate`: an opposite action expires the undo.
    pub(crate) fn invalidate(&mut self, command_type: &str, thread: &str) {
        let kinds: &[&'static str] = match command_type {
            "thread.unsettle" => &["settle"],
            "thread.pin" | "thread.pin.reorder" => &["pin"],
            "thread.settle" => &["pin", "snooze"],
            "thread.unsnooze" => &["snooze"],
            "thread.unarchive" => &["archive"],
            _ => &[],
        };
        for kind in kinds {
            self.claims.remove(&(*kind, thread.to_owned()));
        }
    }

    /// `showThreadUndoNotice`: a new entry, restarting the shared timer.
    pub(crate) fn push(&mut self, undoable: Undoable) {
        self.next += 1;
        self.claims
            .insert((undoable.kind, undoable.thread.clone()), self.next);
        self.live.push((self.next, undoable));
        self.deadline = Some(Instant::now() + WINDOW);
    }

    /// The live group: the newest entries of the newest action, still claimed.
    fn group(&mut self) -> Vec<Undoable> {
        if self.deadline.is_some_and(|at| Instant::now() >= at) {
            self.live.clear();
            self.deadline = None;
        }
        let claims = &self.claims;
        self.live
            .retain(|(token, u)| claims.get(&(u.kind, u.thread.clone())) == Some(token));
        let Some(action) = self.live.last().map(|(_, u)| u.action) else {
            return Vec::new();
        };
        self.live
            .iter()
            .rev()
            .take_while(|(_, u)| u.action == action)
            .map(|(_, u)| u.clone())
            .collect()
    }

    /// Takes the group out, so it undoes once.
    fn take_group(&mut self) -> Vec<Undoable> {
        let group = self.group();
        let count = group.len();
        let keep = self.live.len() - count;
        self.live.truncate(keep);
        for undoable in &group {
            self.claims
                .remove(&(undoable.kind, undoable.thread.clone()));
        }
        group
    }
}

impl App {
    /// `thread.undo` (Mod+Z): true when something was undone.
    pub(crate) fn undo_last(&mut self) -> bool {
        let group = self.undo.take_group();
        if group.is_empty() {
            return false;
        }
        for undoable in group {
            for command in undoable.commands {
                self.dispatch_titled(command, undoable.failure);
            }
        }
        true
    }

    /// The footer's notice, its top at `top`; its height, or 0.
    pub(crate) fn undo_notice(&mut self, ui: &mut Ui, left: f32, right: f32, bottom: f32) -> f32 {
        let group = self.undo.group();
        let Some(first) = group.first() else {
            return 0.0;
        };
        if let Some(at) = self.undo.deadline {
            ui.ctx()
                .request_repaint_after(at.saturating_duration_since(Instant::now()));
        }
        let count = group.len();
        let noun = if count == 1 { "thread" } else { "threads" };
        let text = format!("{} {count} {noun}, ", first.action);
        let bindings = self.state.config["keybindings"].clone();
        let link = crate::ui::shortcuts::effective_shortcut_label(&bindings, "thread.undo", &[])
            .map_or_else(|| "Undo".to_owned(), |key| format!("{key} to undo"));
        let rect = Rect::from_min_max(
            Pos2::new(left, bottom - NOTICE + 8.0),
            Pos2::new(right, bottom),
        );
        ui.painter().rect(
            rect,
            CornerRadius::same(10),
            theme::control_surface(),
            Stroke::new(1.0, theme::sidebar_border()),
            StrokeKind::Inside,
        );
        let small = font(11.0, Weight::Regular);
        let text_width = widgets::text_width(ui, &text, small.clone());
        let y = rect.top() + 6.0;
        widgets::text(
            ui,
            Pos2::new(rect.left() + 8.0, y),
            &text,
            small,
            theme::sidebar_muted(),
            16.0,
        );
        let link_font = font(11.0, Weight::Medium);
        let link_width = widgets::text_width(ui, &link, link_font.clone());
        let link_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 8.0 + text_width, y),
            Vec2::new(link_width, 16.0),
        );
        let response = ui
            .interact(link_rect, ui.id().with("undo-notice"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        widgets::text(
            ui,
            link_rect.min,
            &link,
            link_font,
            theme::foreground(),
            16.0,
        );
        if response.hovered() {
            ui.painter().line_segment(
                [
                    Pos2::new(link_rect.left(), link_rect.bottom() - 1.0),
                    Pos2::new(link_rect.right(), link_rect.bottom() - 1.0),
                ],
                Stroke::new(1.0, theme::foreground()),
            );
        }
        if response.clicked() {
            self.undo_last();
        }
        NOTICE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(thread: &str) -> Undoable {
        Undoable {
            action: "Settled",
            kind: "settle",
            thread: thread.into(),
            commands: vec![crate::model::command("thread.unsettle", thread)],
            failure: "Failed to undo settle",
        }
    }

    #[test]
    fn a_group_of_one_action_undoes_once_and_an_opposite_action_expires_it() {
        let mut undo = Undo::default();
        undo.push(Undoable {
            action: "Snoozed",
            kind: "snooze",
            ..settled("s")
        });
        undo.push(settled("a"));
        undo.push(settled("b"));
        assert_eq!(undo.group().len(), 2);
        // Unsettling one by hand expires its undo.
        undo.invalidate("thread.unsettle", "b");
        assert_eq!(undo.group().len(), 1);
        let taken = undo.take_group();
        assert_eq!(taken[0].thread, "a");
        // Then the older group shows, until its time runs out.
        assert_eq!(undo.group()[0].action, "Snoozed");
        undo.deadline = Some(Instant::now());
        assert!(undo.group().is_empty());
    }
}
