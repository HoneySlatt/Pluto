//! Per-thread actions (`threadActionMenu.logic.ts`, `useThreadActions.ts`):
//! the sidebar context menu, inline rename and the delete confirmation.
//! Project settings are not ported yet.
use eframe::egui::{
    self, Color32, CornerRadius, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::{Thread, command};
use crate::theme::{self, Weight, font};
use crate::ui::context_menu::{self, Item};
use crate::ui::widgets;

/// A pending destructive confirmation (`dialogs.confirm`).
pub struct Confirm {
    pub title: String,
    pub message: String,
    pub confirm_label: &'static str,
    pub command: Value,
    /// Further commands sent with `command` (bulk actions).
    pub more: Vec<Value>,
}

impl App {
    /// Opens the context menu for `thread` at the pointer.
    pub(crate) fn open_thread_menu(&mut self, thread_id: &str, at: Pos2) {
        self.open_menu = None;
        self.thread_menu = Some((thread_id.to_owned(), context_menu::State::new(at)));
    }

    pub(crate) fn start_rename(&mut self, thread: &Thread) {
        self.renaming = Some((thread.id.clone(), thread.title.clone()));
        self.rename_focus = true;
    }

    /// Whether the server advertises an environment capability.
    pub(crate) fn capability(&self, name: &str) -> bool {
        self.state.config["environment"]["capabilities"][name] == true
    }

    /// The legacy sidebar's thread menu (`LegacySidebar.tsx`).
    fn legacy_thread_menu_items(thread: &Thread) -> Vec<Item> {
        let mut items = Vec::new();
        if let Some(branch) = &thread.branch {
            items.push(Item::new(
                "new-thread-on-branch",
                format!("New thread on {branch}"),
            ));
        }
        items.extend([
            Item::new("rename", "Rename thread"),
            Item::new("mark-unread", "Mark unread"),
            Item::new("copy-path", "Copy Path"),
            Item::new("copy-thread-id", "Copy Thread ID"),
            Item::new("project-settings", "Project settings"),
            Item {
                destructive: true,
                ..Item::new("delete", "Delete").icon("trash")
            },
        ]);
        items
    }

    /// `buildThreadActionMenuItems` for a sidebar row.
    fn thread_menu_items(&self, thread: &Thread) -> Vec<Item> {
        if self.client_setting("legacySidebarEnabled") == true {
            return Self::legacy_thread_menu_items(thread);
        }
        let now = self.now;
        // Pluto: a chat is pinned when any of its versions is.
        let pinned = if self.is_chat_thread(thread) {
            self.chat_pinned(&thread.id)
        } else {
            thread.pinned_at.is_some()
        };
        let settled = thread.settled_override.as_deref() == Some("settled");
        let snoozed = crate::snooze::effective_snoozed(thread, now);
        let mut items = Vec::new();
        if let Some(branch) = &thread.branch {
            items.push(
                Item::new("new-thread-on-branch", format!("New thread on {branch}"))
                    .icon("message-square-plus"),
            );
        }
        // Pluto: a design session beside a code thread (`design_home`).
        if !self.is_chat_thread(thread) {
            items.push(Item::new("new-design-session", "New design session").icon("palette"));
        }
        if self.capability("threadPinning") {
            items.push(if pinned {
                Item::new("unpin", "Unpin thread").icon("pin-off")
            } else {
                Item::new("pin", "Pin thread").icon("pin")
            });
        }
        if self.capability("threadSettlement") {
            items.push(if settled {
                Item::new("unsettle", "Un-settle thread").icon("circle-check")
            } else {
                Item::new("settle", "Settle thread").icon("circle-check")
            });
        }
        if self.capability("threadSnooze") {
            items.push(if snoozed {
                Item::new("unsnooze", "Wake thread").icon("clock")
            } else {
                let mut children: Vec<Item> =
                    crate::snooze::presets(now.with_timezone(&chrono::Local))
                        .iter()
                        .map(|preset| {
                            Item::new(
                                format!("snooze:{}", preset.id),
                                format!("{} ({})", preset.label, preset.when),
                            )
                        })
                        .collect();
                children.push(Item::new("snooze:custom", "Custom…").separated());
                Item {
                    disabled: !crate::snooze::can_snooze(thread, now),
                    children,
                    ..Item::new("snooze", "Snooze").icon("clock")
                }
            });
        }
        items.push(
            Item::new("rename", "Rename thread")
                .icon("pencil")
                .separated(),
        );
        if self.capability("threadTitleRegeneration") {
            items.push(Item::new("regenerate-title", "Regenerate title").icon("refresh-cw"));
        }
        items.push(Item::new("mark-unread", "Mark unread").icon("mail-open"));
        // Pluto: a chat can be saved as Markdown (`chat_export`).
        if self.is_chat_thread(thread) {
            items.push(Item::new("export-markdown", "Export as Markdown").icon("download"));
        }
        if let Some(project) = self.state.project(&thread.project_id) {
            let label = if self.project_scope.as_deref() == Some(project.id.as_str()) {
                "Show all projects".to_owned()
            } else {
                format!("Filter by {}", project.title)
            };
            items.push(Item::new("filter-by-project", label).icon("folder-tree"));
        }
        if self.capability("threadAutoSettleOptOut") {
            let enabled = thread.auto_settle_disabled_at.is_none();
            items.push(Item {
                children: vec![
                    Item {
                        checked: Some(enabled),
                        ..Item::new("auto-settle:enabled", "Enabled")
                    },
                    Item {
                        checked: Some(!enabled),
                        ..Item::new("auto-settle:disabled", "Disabled")
                    },
                ],
                ..Item::new("auto-settle", "Auto-settle behavior").icon("timer")
            });
        }
        let mut copy = vec![Item::new("copy-path", "Path").icon("folder")];
        if thread.branch.is_some() {
            copy.push(Item::new("copy-branch", "Branch").icon("git-branch"));
        }
        copy.push(Item::new("copy-thread-id", "Thread ID").icon("hash"));
        items.push(Item {
            children: copy,
            ..Item::new("copy", "Copy").icon("copy").separated()
        });
        items.push(Item::new("project-settings", "Project settings").icon("settings"));
        items.push(Item {
            // Archive rejects a thread with an active turn.
            disabled: thread.is_running(),
            ..Item::new("archive", "Archive thread")
                .icon("archive")
                .separated()
        });
        items.push(Item {
            destructive: true,
            ..Item::new("delete", "Delete").icon("trash")
        });
        items
    }

    pub(crate) fn thread_menu(&mut self, ui: &mut Ui) {
        let Some((thread_id, state)) = self.thread_menu.take() else {
            return;
        };
        let Some(thread) = self
            .state
            .threads
            .iter()
            .find(|t| t.id == thread_id)
            .cloned()
        else {
            return;
        };
        let items = self.thread_menu_items(&thread);
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.thread_menu = Some((thread_id, state));
        }
        let Some(clicked) = clicked else {
            return;
        };
        let id = thread.id.as_str();
        if let Some(preset) = clicked.strip_prefix("snooze:") {
            let now = self.now.with_timezone(&chrono::Local);
            match crate::snooze::presets(now).iter().find(|p| p.id == preset) {
                Some(preset) => {
                    let mut cmd = command("thread.snooze", id);
                    cmd["snoozedUntil"] = json!(crate::snooze::until_iso(preset));
                    self.dispatch(cmd);
                }
                None => self.request_custom_snooze(vec![thread.id.clone()]),
            }
            return;
        }
        match clicked.as_str() {
            // Branch and worktree carry-over is not ported: the draft opens
            // in the thread's project on its current checkout.
            "new-thread-on-branch" => self.start_draft(Some(thread.project_id.clone())),
            "new-design-session" => self.open_linked_design(&thread),
            // Pluto: a chat's versions are pinned together (`chat_versions`).
            "pin" | "unpin" => {
                let pinned = self.chat_family(id).into_iter().filter(|version| {
                    self.state
                        .threads
                        .iter()
                        .any(|t| t.id == *version && t.pinned_at.is_some())
                });
                let action = clicked.as_str();
                let versions: Vec<String> = if action == "pin" {
                    vec![id.to_owned()]
                } else {
                    pinned.collect()
                };
                for version in versions {
                    self.dispatch(command(&format!("thread.{action}"), &version));
                }
            }
            "settle" => self.dispatch(command("thread.settle", id)),
            "unsettle" => {
                let mut cmd = command("thread.unsettle", id);
                cmd["reason"] = json!("user");
                self.dispatch(cmd);
            }
            "unsnooze" => {
                let mut cmd = command("thread.unsnooze", id);
                cmd["reason"] = json!("user");
                self.dispatch(cmd);
            }
            "auto-settle:enabled" | "auto-settle:disabled" => {
                let mut cmd = command("thread.auto-settle.set", id);
                cmd["enabled"] = json!(clicked == "auto-settle:enabled");
                self.dispatch(cmd);
            }
            "rename" => self.start_rename(&thread),
            "regenerate-title" => {
                let mut cmd = command("thread.metadata.update", id);
                cmd["regenerateTitle"] = json!(true);
                self.dispatch(cmd);
            }
            "mark-unread" => self.mark_unread(&thread),
            "filter-by-project" => {
                // Picking the scoped project again goes back to all projects.
                self.project_scope = if self.project_scope.as_deref() == Some(&thread.project_id) {
                    None
                } else {
                    Some(thread.project_id.clone())
                };
            }
            "copy-path" => {
                let path = thread
                    .worktree_path
                    .clone()
                    .or_else(|| {
                        self.state
                            .project(&thread.project_id)
                            .map(|p| p.workspace_root.clone())
                    })
                    .unwrap_or_default();
                ui.ctx().copy_text(path);
            }
            "copy-branch" => ui
                .ctx()
                .copy_text(thread.branch.clone().unwrap_or_default()),
            "copy-thread-id" => ui.ctx().copy_text(thread.id.clone()),
            "project-settings" => {
                let project = thread.project_id.clone();
                self.open_project_settings(&project);
            }
            // Pluto: a chat goes with all its versions (`chat_versions`).
            "export-markdown" => self.request_chat_export(id),
            "archive" => {
                for version in self.chat_family(id) {
                    self.dispatch(command("thread.archive", &version));
                }
            }
            // `confirmThreadDelete` asks first unless it was turned off.
            "delete" if self.client_settings["confirmThreadDelete"] == false => {
                let family = self.chat_family(id);
                let open = self
                    .selected
                    .as_ref()
                    .is_some_and(|selected| family.contains(selected));
                for version in family {
                    self.dispatch(command("thread.delete", &version));
                }
                if open {
                    self.start_draft(None);
                }
            }
            "delete" => {
                let more = self
                    .chat_family(id)
                    .into_iter()
                    .filter(|version| version != id)
                    .map(|version| command("thread.delete", &version))
                    .collect();
                self.confirm = Some(Confirm {
                    title: format!("Delete thread \"{}\"?", thread.title),
                    message: "This permanently clears conversation history for this thread.".into(),
                    confirm_label: "Delete",
                    command: command("thread.delete", id),
                    more,
                });
            }
            _ => {}
        }
    }

    /// Inline title editor for a thread being renamed. Returns true while active.
    pub(crate) fn rename_editor(&mut self, ui: &mut Ui, thread_id: &str, rect: Rect) -> bool {
        let Some((renaming_id, mut title)) = self.renaming.clone() else {
            return false;
        };
        if renaming_id != thread_id {
            return false;
        }
        ui.painter().rect(
            rect,
            CornerRadius::same(4),
            theme::card(),
            Stroke::new(1.0, theme::foreground()),
            StrokeKind::Inside,
        );
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(4.0, 0.0))));
        let response = child.add(
            egui::TextEdit::singleline(&mut title)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font(14.0, Weight::Medium))
                .text_color(theme::foreground())
                .desired_width(rect.width() - 8.0)
                .vertical_align(egui::Align::Center),
        );
        if self.rename_focus {
            response.request_focus();
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                let all = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(title.chars().count()),
                );
                state.cursor.set_char_range(Some(all));
                state.store(ui.ctx(), response.id);
            }
            self.rename_focus = false;
        }
        let escape = ui.input(|i| i.key_pressed(Key::Escape));
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        if escape {
            self.renaming = None;
        } else if enter || response.lost_focus() {
            self.renaming = None;
            let original = self
                .state
                .threads
                .iter()
                .find(|t| t.id == thread_id)
                .map(|t| t.title.clone())
                .unwrap_or_default();
            let trimmed = title.trim();
            if !trimmed.is_empty() && trimmed != original {
                // Pluto: a chat's versions share its title.
                for version in self.chat_family(thread_id) {
                    let mut cmd = command("thread.metadata.update", &version);
                    cmd["title"] = json!(trimmed);
                    self.dispatch(cmd);
                }
            }
        } else {
            self.renaming = Some((renaming_id, title));
        }
        true
    }

    /// Modal confirmation over the whole window.
    pub(crate) fn confirm_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(confirm) = &self.confirm else {
            return;
        };
        let mut decision = None;
        egui::Area::new(ui.id().with("confirm-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                // The backdrop swallows clicks so the app behind stays inert.
                let (backdrop, _) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(backdrop, 0.0, Color32::from_black_alpha(140));
                let width = 400.0;
                let message = widgets::galley(
                    ui,
                    &confirm.message,
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                    width - 48.0,
                );
                let title = widgets::galley(
                    ui,
                    &confirm.title,
                    font(16.0, Weight::SemiBold),
                    theme::foreground(),
                    24.0,
                    width - 48.0,
                );
                let height = 24.0 + title.size().y + 8.0 + message.size().y + 24.0 + 32.0 + 24.0;
                let dialog = Rect::from_center_size(backdrop.center(), Vec2::new(width, height));
                ui.painter().rect(
                    dialog,
                    CornerRadius::same(14),
                    theme::card(),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let title_height = title.size().y;
                ui.painter().galley(
                    dialog.min + Vec2::new(24.0, 24.0),
                    title,
                    theme::foreground(),
                );
                ui.painter().galley(
                    dialog.min + Vec2::new(24.0, 24.0 + title_height + 8.0),
                    message,
                    theme::muted_foreground(),
                );
                let confirm_width =
                    (widgets::text_width(ui, confirm.confirm_label, font(14.0, Weight::Medium))
                        + 24.0)
                        .ceil();
                let confirm_rect = Rect::from_min_size(
                    Pos2::new(
                        dialog.right() - 24.0 - confirm_width,
                        dialog.bottom() - 24.0 - 32.0,
                    ),
                    Vec2::new(confirm_width, 32.0),
                );
                let confirm_response =
                    ui.interact(confirm_rect, ui.id().with("confirm-ok"), Sense::click());
                let destructive = Color32::from_rgb(251, 65, 74);
                ui.painter().rect_filled(
                    confirm_rect,
                    CornerRadius::same(8),
                    if confirm_response.hovered() {
                        destructive
                    } else {
                        theme::alpha(destructive, 0.9)
                    },
                );
                widgets::text_in(
                    ui,
                    Rect::from_min_max(confirm_rect.min + Vec2::new(12.0, 0.0), confirm_rect.max),
                    confirm.confirm_label,
                    font(14.0, Weight::Medium),
                    Color32::WHITE,
                );
                let cancel_width =
                    (widgets::text_width(ui, "Cancel", font(14.0, Weight::Medium)) + 24.0).ceil();
                let cancel_rect = Rect::from_min_size(
                    Pos2::new(confirm_rect.left() - 8.0 - cancel_width, confirm_rect.top()),
                    Vec2::new(cancel_width, 32.0),
                );
                let cancel_response =
                    ui.interact(cancel_rect, ui.id().with("confirm-cancel"), Sense::click());
                ui.painter().rect(
                    cancel_rect,
                    CornerRadius::same(8),
                    if cancel_response.hovered() {
                        theme::overlay(0.06)
                    } else {
                        theme::control_surface()
                    },
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                widgets::text_in(
                    ui,
                    Rect::from_min_max(cancel_rect.min + Vec2::new(12.0, 0.0), cancel_rect.max),
                    "Cancel",
                    font(14.0, Weight::Medium),
                    theme::foreground(),
                );
                if confirm_response.clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    decision = Some(true);
                } else if cancel_response.clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    decision = Some(false);
                }
            });
        match decision {
            Some(true) => {
                if let Some(confirm) = self.confirm.take() {
                    if confirm.command["type"] == crate::ui::memory_settings::CLEAR_COMMAND {
                        self.clear_memories();
                        return;
                    }
                    if confirm.command["type"] == crate::ui::chat_projects::CLEAR_MEMORY_COMMAND {
                        let project = confirm.command["projectId"].as_str().unwrap_or_default();
                        self.clear_project_memory(project);
                        return;
                    }
                    // Pluto: deleting the open chat project leaves for a new chat.
                    if confirm.command["type"] == "project.delete"
                        && self
                            .open_chat_project()
                            .is_some_and(|p| confirm.command["projectId"] == p.id.as_str())
                    {
                        self.dispatch(confirm.command);
                        self.new_chat();
                        return;
                    }
                    let deleted = confirm.command["type"] == "thread.delete";
                    let thread_id = confirm.command["threadId"].as_str().map(str::to_owned);
                    let deleted_selected = deleted
                        && confirm
                            .more
                            .iter()
                            .any(|c| c["threadId"].as_str() == self.selected.as_deref());
                    self.dispatch(confirm.command);
                    for more in confirm.more {
                        self.dispatch(more);
                    }
                    if deleted_selected {
                        self.start_draft(None);
                    }
                    if deleted && thread_id == self.selected {
                        self.start_draft(None);
                    }
                    if self.settings_page == Some(crate::ui::settings::ARCHIVE_PAGE) {
                        self.request_archived();
                    }
                }
            }
            Some(false) => self.confirm = None,
            None => {}
        }
    }
}
