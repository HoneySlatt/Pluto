//! The prompt stash (`promptStashStore.ts`, `ComposerStashBadge.tsx`,
//! `ComposerStashMenu.tsx`): Mod+S puts the composer's prompt, images,
//! uploaded files and contexts aside; with an empty composer it brings the
//! only one back, or opens the list. Newest first, 20 at most, kept in
//! `stash.json` beside the client settings. A "Stash" tab above the
//! composer counts them and opens the list: each row restores its prompt
//! (appended after a blank line) or deletes it.
//!
//! Deliberate deviations: native storage has no browser quota, so images
//! are kept as they are (no recompression or size budget), and uploaded
//! files are not checked for their 24 h expiry before a restore.

use eframe::egui::{self, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::drafts::ComposerDraft;
use crate::ui::toasts::{Kind, Toast};
use crate::ui::widgets;

const LIMIT: usize = 20;
/// The tab above the composer: 5 + 16 + 5.
pub(crate) const TAB: f32 = 26.0;

pub(crate) struct StashEntry {
    pub id: String,
    pub created_at: String,
    pub draft: ComposerDraft,
}

#[derive(Default)]
pub(crate) struct Stash {
    pub entries: Vec<StashEntry>,
    pub open: bool,
    pub highlighted: usize,
    /// When the last stash landed: the count pulses 1.2 s.
    pub pulsed: Option<std::time::Instant>,
}

fn stash_path(settings: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    Some(settings?.with_file_name("stash.json"))
}

pub(crate) fn load(settings: Option<&std::path::Path>) -> Stash {
    let value: Value = stash_path(settings)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let entries = value["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(StashEntry {
                id: entry["id"].as_str()?.to_owned(),
                created_at: entry["createdAt"].as_str()?.to_owned(),
                draft: crate::ui::drafts::read_draft(&entry["draft"]),
            })
        })
        .collect();
    Stash {
        entries,
        ..Default::default()
    }
}

/// "Fix the build and…" (90 characters), or what it carries.
fn snippet(draft: &ComposerDraft) -> String {
    let text: String = draft.text.split_whitespace().collect::<Vec<_>>().join(" ");
    if !text.is_empty() {
        return if text.chars().count() > 90 {
            format!("{}…", text.chars().take(90).collect::<String>())
        } else {
            text
        };
    }
    let plural = |n: usize, noun: &str| format!("({n} {noun}{})", if n == 1 { "" } else { "s" });
    match (draft.images.len(), draft.files.len()) {
        (0, 0) => "(empty)".into(),
        (n, 0) => plural(n, "image"),
        (0, n) => plural(n, "file"),
        (a, b) => plural(a + b, "attachment"),
    }
}

impl App {
    fn save_stash(&mut self) {
        let Some(path) = stash_path(self.client_settings_path.as_deref()) else {
            return;
        };
        let mut encoded = Default::default();
        let entries: Vec<Value> = self
            .stash
            .entries
            .iter()
            .map(|entry| {
                json!({
                    "id": entry.id,
                    "createdAt": entry.created_at,
                    "draft": crate::ui::drafts::draft_entry(
                        &entry.draft.text,
                        &entry.draft.images,
                        &entry.draft.files,
                        &entry.draft.contexts,
                        &mut encoded,
                    ),
                })
            })
            .collect();
        let text = json!({"version": 1, "entries": entries}).to_string();
        let temp = path.with_extension("json.tmp");
        let written = std::fs::write(&temp, text).is_ok() && std::fs::rename(&temp, &path).is_ok();
        if !written {
            self.toasts.add(
                Toast::new(Kind::Error, "Could not stash this prompt")
                    .description("The stash file could not be written."),
            );
        }
    }

    /// `composer.stash` (Mod+S).
    pub(crate) fn stash_shortcut(&mut self) {
        let empty = self.composer.trim().is_empty()
            && self.composer_images.is_empty()
            && self.composer_files.is_empty();
        if empty {
            if self.stash.entries.len() == 1 {
                self.restore_stash(0);
            } else {
                self.stash.open = !self.stash.open;
                self.stash.highlighted = 0;
            }
            return;
        }
        if self
            .composer_files
            .iter()
            .any(|f| !matches!(f.upload, Some(Ok(_))))
        {
            self.toasts.add(Toast::new(
                Kind::Error,
                "Wait for file uploads before stashing this prompt",
            ));
            return;
        }
        let mut draft = self.take_composer();
        draft.text = draft.text.trim().to_owned();
        self.stash.entries.insert(
            0,
            StashEntry {
                id: uuid::Uuid::new_v4().to_string(),
                created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                draft,
            },
        );
        if self.stash.entries.len() > LIMIT {
            self.stash.entries.truncate(LIMIT);
            self.toasts.add(
                Toast::new(Kind::Warning, "Oldest stashed prompt discarded").description(
                    "The stash holds 20 prompts; the oldest was removed to make room.",
                ),
            );
        }
        self.stash.pulsed = Some(std::time::Instant::now());
        self.save_stash();
    }

    /// Brings entry `index` back: its text after the composer's, its
    /// attachments up to the limit.
    fn restore_stash(&mut self, index: usize) {
        if index >= self.stash.entries.len() {
            return;
        }
        let entry = self.stash.entries.remove(index);
        let restored = entry.draft;
        if !restored.text.is_empty() {
            self.composer = if self.composer.trim().is_empty() {
                restored.text
            } else {
                format!("{}\n\n{}", self.composer.trim_end(), restored.text)
            };
        }
        let mut refused = Vec::new();
        for image in restored.images {
            if self.composer_images.len() + self.composer_files.len() < 100 {
                self.composer_images.push(image);
            } else {
                refused.push(image.name);
            }
        }
        for file in restored.files {
            if self.composer_images.len() + self.composer_files.len() < 100 {
                self.composer_files.push(file);
            } else {
                refused.push(file.name);
            }
        }
        self.composer_contexts.extend(restored.contexts);
        if !refused.is_empty() {
            self.toasts.add(
                Toast::new(Kind::Warning, "Some attachments were not restored").description(
                    format!(
                        "{} could not be restored: the composer is at its 100-attachment limit.",
                        refused.join(", ")
                    ),
                ),
            );
        }
        if self.stash.entries.is_empty() {
            self.stash.open = false;
        }
        self.stash.highlighted = self
            .stash
            .highlighted
            .min(self.stash.entries.len().saturating_sub(1));
        self.save_stash();
    }

    fn delete_stash(&mut self, index: usize) {
        if index < self.stash.entries.len() {
            self.stash.entries.remove(index);
            if self.stash.entries.is_empty() {
                self.stash.open = false;
            }
            self.stash.highlighted = self
                .stash
                .highlighted
                .min(self.stash.entries.len().saturating_sub(1));
            self.save_stash();
        }
    }

    /// The "Stash N" tab at the right, its bottom on `bottom` (the top of
    /// what is attached above the composer), and the list above it.
    pub(crate) fn stash_tab(&mut self, ui: &mut Ui, surface: Rect, bottom: f32) {
        if self.stash.entries.is_empty() {
            self.stash.open = false;
            return;
        }
        let count = self.stash.entries.len().to_string();
        let small = font(12.0, Weight::Medium);
        let width = 24.0
            + widgets::text_width(ui, "Stash", small.clone())
            + 6.0
            + widgets::tabular_width(ui, &count, small.clone())
            + 10.0;
        let tab = Rect::from_min_max(
            Pos2::new(surface.right() - 22.0 - width, bottom - TAB),
            Pos2::new(surface.right() - 22.0, bottom),
        );
        let response = ui
            .interact(tab, ui.id().with("stash-tab"), Sense::click())
            .on_hover_text(format!("Stashed prompts: {count}. Open stash."));
        let pulsing = self
            .stash
            .pulsed
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(1200));
        if pulsing {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        let lit = response.hovered() || self.stash.open || pulsing;
        ui.painter().rect(
            tab,
            CornerRadius {
                nw: 16,
                ne: 16,
                sw: 0,
                se: 0,
            },
            theme::shade(16),
            Stroke::new(1.0, theme::shade(28)),
            egui::StrokeKind::Inside,
        );
        let color = if lit {
            theme::foreground()
        } else {
            theme::muted_foreground()
        };
        theme::paint_icon(
            ui,
            "bookmark",
            Rect::from_center_size(
                Pos2::new(tab.left() + 14.0, tab.center().y),
                Vec2::splat(12.0),
            ),
            12.0,
            color,
        );
        let label_x = tab.left() + 24.0;
        widgets::text(
            ui,
            Pos2::new(label_x, tab.top() + 5.0),
            "Stash",
            small.clone(),
            color,
            16.0,
        );
        let count_x = label_x + widgets::text_width(ui, "Stash", small.clone()) + 6.0;
        widgets::tabular_text(
            ui,
            Pos2::new(count_x, tab.top() + 5.0),
            &count,
            small,
            if pulsing { theme::primary() } else { color },
            16.0,
        );
        if response.clicked() {
            self.stash.open = !self.stash.open;
            self.stash.highlighted = 0;
        }
        if self.stash.open {
            self.stash_menu(ui, surface, tab);
        }
    }

    fn stash_menu(&mut self, ui: &mut Ui, surface: Rect, tab: Rect) {
        let row_height = 28.0;
        let rows = self.stash.entries.len();
        let list_height = (rows as f32 * (row_height + 1.0)).min(384.0);
        let width = (surface.width() - 44.0).min(560.0);
        let rect = Rect::from_min_max(
            Pos2::new(
                surface.right() - 22.0 - width,
                tab.top() - 8.0 - list_height - 36.0,
            ),
            Pos2::new(surface.right() - 22.0, tab.top() - 8.0),
        );
        // Keys: Escape, the arrows, Enter, Mod+Backspace.
        let (escape, up, down, enter, delete) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::Escape),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::COMMAND, Key::Backspace),
            )
        });
        if escape {
            self.stash.open = false;
            return;
        }
        if up {
            self.stash.highlighted = (self.stash.highlighted + rows - 1) % rows;
        }
        if down {
            self.stash.highlighted = (self.stash.highlighted + 1) % rows;
        }
        if enter {
            self.restore_stash(self.stash.highlighted);
            return;
        }
        if delete {
            self.delete_stash(self.stash.highlighted);
            return;
        }
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("stash-menu"),
        ));
        widgets::popup_surface(ui, rect);
        painter.rect(
            rect,
            CornerRadius::same(10),
            widgets::menu_surface(),
            Stroke::new(1.0, widgets::menu_outline()),
            egui::StrokeKind::Inside,
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .layer_id(painter.layer_id())
                .max_rect(rect),
        );
        // Header: the stash and its count.
        let header = Rect::from_min_size(
            rect.min + Vec2::new(8.0, 6.0),
            Vec2::new(rect.width() - 16.0, 24.0),
        );
        theme::paint_icon(
            &child,
            "bookmark",
            Rect::from_center_size(
                Pos2::new(header.left() + 10.0, header.center().y),
                Vec2::splat(12.0),
            ),
            12.0,
            theme::muted_foreground(),
        );
        widgets::text_in(
            &child,
            Rect::from_min_max(Pos2::new(header.left() + 22.0, header.top()), header.max),
            &format!("Stash  {rows}"),
            font(12.0, Weight::Medium),
            theme::muted_foreground(),
        );
        let mut restore = None;
        let mut remove = None;
        let list = Rect::from_min_max(
            Pos2::new(rect.left() + 4.0, header.bottom() + 4.0),
            Pos2::new(rect.right() - 4.0, rect.bottom() - 4.0),
        );
        let mut list_ui = child.new_child(egui::UiBuilder::new().max_rect(list));
        egui::ScrollArea::vertical()
            .id_salt("stash-list")
            .max_height(list.height())
            .show(&mut list_ui, |ui| {
                for (index, entry) in self.stash.entries.iter().enumerate() {
                    let (row, response) =
                        ui.allocate_exact_size(Vec2::new(list.width(), row_height), Sense::click());
                    if response.hovered() {
                        self.stash.highlighted = index;
                    }
                    if self.stash.highlighted == index {
                        ui.painter()
                            .rect_filled(row, CornerRadius::same(6), theme::accent());
                    }
                    theme::paint_icon(
                        ui,
                        "file-text",
                        Rect::from_center_size(
                            Pos2::new(row.left() + 14.0, row.center().y),
                            Vec2::splat(14.0),
                        ),
                        14.0,
                        theme::muted_foreground(),
                    );
                    // Right: attachment counts, the age, delete.
                    let close = Rect::from_center_size(
                        Pos2::new(row.right() - 14.0, row.center().y),
                        Vec2::splat(24.0),
                    );
                    let close_response = ui
                        .interact(
                            close,
                            ui.id().with(("stash-delete", &entry.id)),
                            Sense::click(),
                        )
                        .on_hover_text("Delete stashed prompt");
                    theme::paint_icon(
                        ui,
                        "x",
                        Rect::from_center_size(close.center(), Vec2::splat(14.0)),
                        14.0,
                        if close_response.hovered() {
                            theme::foreground()
                        } else {
                            theme::muted_foreground()
                        },
                    );
                    let age = widgets::relative_time(&entry.created_at, self.now);
                    let age_width = widgets::tabular_width(ui, &age, font(12.0, Weight::Regular));
                    let mut right = close.left() - 6.0;
                    widgets::tabular_text(
                        ui,
                        Pos2::new(right - age_width, row.center().y - 8.0),
                        &age,
                        font(12.0, Weight::Regular),
                        theme::muted_foreground(),
                        16.0,
                    );
                    right -= age_width + 10.0;
                    let attachments = entry.draft.images.len() + entry.draft.files.len();
                    if attachments > 0 && !entry.draft.text.is_empty() {
                        let label = attachments.to_string();
                        let w = widgets::tabular_width(ui, &label, font(12.0, Weight::Regular));
                        widgets::tabular_text(
                            ui,
                            Pos2::new(right - w, row.center().y - 8.0),
                            &label,
                            font(12.0, Weight::Regular),
                            theme::muted_foreground(),
                            16.0,
                        );
                        theme::paint_icon(
                            ui,
                            "file",
                            Rect::from_center_size(
                                Pos2::new(right - w - 10.0, row.center().y),
                                Vec2::splat(12.0),
                            ),
                            12.0,
                            theme::muted_foreground(),
                        );
                        right -= w + 24.0;
                    }
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(row.left() + 28.0, row.top()),
                            Pos2::new(right, row.bottom()),
                        ),
                        &snippet(&entry.draft),
                        font(13.0, Weight::Regular),
                        theme::alpha(theme::foreground(), 0.8),
                    );
                    if close_response.clicked() {
                        remove = Some(index);
                    } else if response.clicked() {
                        restore = Some(index);
                    }
                    ui.add_space(1.0);
                }
            });
        // A press outside the list and the tab closes it.
        let outside = ui.input(|i| i.pointer.any_pressed())
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| !rect.contains(p) && !tab.contains(p));
        if let Some(index) = remove {
            self.delete_stash(index);
        } else if let Some(index) = restore {
            self.restore_stash(index);
        } else if outside {
            self.stash.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_read_like_upstream() {
        let mut draft = ComposerDraft::default();
        assert_eq!(snippet(&draft), "(empty)");
        draft.text = "  fix   the\nbuild ".into();
        assert_eq!(snippet(&draft), "fix the build");
        draft.text = "x".repeat(95);
        assert_eq!(snippet(&draft).chars().count(), 91);
    }
}
