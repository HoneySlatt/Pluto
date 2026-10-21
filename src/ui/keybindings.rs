//! Keybinding editing (`settings/KeybindingsSettings.tsx`,
//! `KeybindingsSettings.logic.ts`): a binding's pill records a new shortcut,
//! Save upserts it with `server.upsertKeybinding`, and custom bindings offer
//! Reset to default and Remove (`server.removeKeybinding`).
//!
//! The when clause opens its builder (`when_builder`).
//!
//! Not ported: adding bindings and the header search; conflicts show their
//! warning icon only.
use eframe::egui::{
    self, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::settings::command_label;
use crate::ui::when_builder::{self, When, WhenDraft};
use crate::ui::widgets;

/// One resolved binding as the page lists it (`KeybindingRow`).
pub(crate) struct KbRow {
    pub id: String,
    pub command: String,
    pub key: String,
    pub when: String,
    pub when_ast: Value,
    /// `Custom` (not a default); project script bindings count too.
    pub custom: bool,
    pub default_key: Option<String>,
    pub default_when: String,
    pub conflicts: Vec<String>,
}

/// The row whose shortcut or when clause is being edited.
pub struct KeybindingEdit {
    row: String,
    draft: String,
    recording: bool,
    when: WhenDraft,
    when_open: bool,
}

impl KeybindingEdit {
    fn new(row: &KbRow) -> Self {
        Self {
            row: row.id.clone(),
            draft: row.key.clone(),
            recording: false,
            when: WhenDraft::new(When::from_value(&row.when_ast)),
            when_open: false,
        }
    }

    fn dirty(&self, row: &KbRow) -> bool {
        self.draft != row.key || self.when.expression() != row.when
    }
}

/// `DEFAULT_KEYBINDINGS` as (key, command, when).
pub(crate) fn default_whens() -> &'static [(&'static str, &'static str, &'static str)] {
    DEFAULTS
}

/// `shortcutToKeybindingInput`: `{modKey, shiftKey, key: "k"}` reads `mod+shift+k`.
pub(crate) fn shortcut_input(shortcut: &Value) -> String {
    let mut parts = Vec::new();
    for (flag, name) in [
        ("modKey", "mod"),
        ("metaKey", "meta"),
        ("ctrlKey", "ctrl"),
        ("altKey", "alt"),
        ("shiftKey", "shift"),
    ] {
        if shortcut[flag] == true {
            parts.push(name.to_owned());
        }
    }
    parts.push(match shortcut["key"].as_str().unwrap_or_default() {
        " " => "space".to_owned(),
        "escape" => "esc".to_owned(),
        key => key.to_owned(),
    });
    parts.join("+")
}

/// `KeybindingPill` labels for a binding string.
pub(crate) fn pill_parts(key: &str) -> Vec<String> {
    key.split('+')
        .map(|part| match part {
            "mod" => "Ctrl".to_owned(),
            "shift" => "\u{21e7}".to_owned(),
            "alt" => "Alt".to_owned(),
            "ctrl" => "\u{2303}".to_owned(),
            part if part.chars().count() == 1 => part.to_uppercase(),
            part => part.to_owned(),
        })
        .collect()
}

fn conflicts_with_when(left: &str, right: &str) -> bool {
    left.is_empty() || right.is_empty() || left == right
}

/// `keybindingConflictLabels`: other rows on the same key whose conditions overlap.
fn conflict_labels(rows: &[KbRow], id: &str, key: &str, when: &str) -> Vec<String> {
    if key.trim().is_empty() {
        return Vec::new();
    }
    let mut labels: Vec<String> = rows
        .iter()
        .filter(|row| row.id != id && row.key == key && conflicts_with_when(&row.when, when))
        .map(|row| command_label(&row.command))
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

/// `buildKeybindingRows` without the query: source, defaults and conflicts.
pub(crate) fn build_rows(bindings: &Value, when_expression: fn(&Value) -> String) -> Vec<KbRow> {
    let mut rows: Vec<KbRow> = bindings
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, binding)| {
            let command = binding["command"].as_str().unwrap_or_default().to_owned();
            let key = shortcut_input(&binding["shortcut"]);
            let when = when_expression(&binding["whenAst"]);
            let is_default = DEFAULTS
                .iter()
                .any(|(k, c, w)| *c == command && *k == key && *w == when);
            // `defaultBindingForBinding`: the exact default, else one with the
            // same condition, else any default of the command.
            let default = DEFAULTS
                .iter()
                .find(|(k, c, w)| *c == command && *k == key && *w == when)
                .or_else(|| {
                    DEFAULTS
                        .iter()
                        .find(|(_, c, w)| *c == command && *w == when)
                })
                .or_else(|| DEFAULTS.iter().find(|(_, c, _)| *c == command));
            KbRow {
                id: format!("{command}\0{key}\0{when}\0{index}"),
                custom: command.starts_with("script.") || !is_default,
                default_key: default.map(|(k, _, _)| (*k).to_owned()),
                default_when: default.map(|(_, _, w)| (*w).to_owned()).unwrap_or_default(),
                when_ast: binding["whenAst"].clone(),
                command,
                key,
                when,
                conflicts: Vec::new(),
            }
        })
        .collect();
    for index in 0..rows.len() {
        let labels = conflict_labels(&rows, &rows[index].id, &rows[index].key, &rows[index].when);
        rows[index].conflicts = labels;
    }
    rows
}

/// `keybindingFromKeyboardEvent` (non-Mac): Ctrl records as `mod`;
/// modifier-only presses record nothing.
pub(crate) fn binding_from_key(key: Key, modifiers: Modifiers) -> Option<String> {
    let token = match key {
        Key::Space => "space".to_owned(),
        Key::Escape => "esc".to_owned(),
        Key::ArrowUp => "arrowup".to_owned(),
        Key::ArrowDown => "arrowdown".to_owned(),
        Key::ArrowLeft => "arrowleft".to_owned(),
        Key::ArrowRight => "arrowright".to_owned(),
        Key::Enter => "enter".to_owned(),
        Key::Tab => "tab".to_owned(),
        Key::Backspace => "backspace".to_owned(),
        Key::Delete => "delete".to_owned(),
        Key::Home => "home".to_owned(),
        Key::End => "end".to_owned(),
        Key::PageUp => "pageup".to_owned(),
        Key::PageDown => "pagedown".to_owned(),
        Key::Minus => "-".to_owned(),
        key => {
            let symbol = key.symbol_or_name();
            if symbol.chars().count() == 1 || symbol.len() <= 3 && symbol.starts_with('F') {
                symbol.to_lowercase()
            } else {
                return None;
            }
        }
    };
    let mut parts = Vec::new();
    if modifiers.ctrl || modifiers.command {
        parts.push("mod".to_owned());
    }
    if modifiers.alt {
        parts.push("alt".to_owned());
    }
    if modifiers.shift {
        parts.push("shift".to_owned());
    }
    parts.push(token);
    Some(parts.join("+"))
}

/// `rowKeybindingTarget`: the binding a save replaces or a removal drops.
fn target(row: &KbRow) -> Value {
    let mut target = json!({"command": row.command, "key": row.key});
    if !row.when.trim().is_empty() {
        target["when"] = json!(row.when);
    }
    target
}

/// `KeybindingPill` chips ending at `right`; returns their left edge.
pub(crate) fn paint_pill(ui: &Ui, key: &str, right: f32, center_y: f32) -> f32 {
    let chip_font = font(12.0, Weight::Medium);
    let parts = pill_parts(key);
    let widths: Vec<f32> = parts
        .iter()
        .map(|k| {
            (widgets::text_width(ui, k, chip_font.clone()) + 8.0)
                .max(20.0)
                .ceil()
        })
        .collect();
    let total = widths.iter().sum::<f32>() + 4.0 * (widths.len().saturating_sub(1)) as f32;
    let mut chip_x = right - total;
    for (part, width) in parts.iter().zip(&widths) {
        let chip = Rect::from_min_size(Pos2::new(chip_x, center_y - 10.0), Vec2::new(*width, 20.0));
        ui.painter()
            .rect_filled(chip, CornerRadius::same(4), theme::overlay(0.03));
        let text_width = widgets::text_width(ui, part, chip_font.clone());
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(chip.center().x - text_width / 2.0, chip.top()),
                chip.max,
            ),
            part,
            chip_font.clone(),
            theme::muted_foreground(),
        );
        chip_x += width + 4.0;
    }
    right - total
}

impl App {
    /// The row's right side: conflict warning, hover menu and the shortcut
    /// pill, capture input and Save (`KeybindingSettingsRow`'s control).
    pub(crate) fn keybinding_control(
        &mut self,
        ui: &mut Ui,
        row_rect: Rect,
        row: &KbRow,
        right: f32,
        center_y: f32,
    ) {
        let editing = self
            .keybinding_edit
            .as_ref()
            .filter(|edit| edit.row == row.id)
            .map(|edit| {
                (
                    edit.draft.clone(),
                    edit.recording,
                    edit.dirty(row),
                    edit.when.valid,
                )
            });
        let mut x;
        match &editing {
            Some((draft, recording, dirty, valid)) if *recording || *dirty => {
                let input = Rect::from_min_size(
                    Pos2::new(right - 176.0, center_y - 14.0),
                    Vec2::new(176.0, 28.0),
                );
                let input_id = ui.id().with(("keybinding-capture", &row.id));
                let response = ui.interact(input, input_id, Sense::click());
                ui.painter().rect(
                    input,
                    CornerRadius::same(10),
                    theme::control_surface(),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let (text, color) = if *recording {
                    ("Press shortcut", theme::muted_foreground())
                } else {
                    (draft.as_str(), theme::foreground())
                };
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(input.left() + 11.0, input.top()), input.max),
                    text,
                    theme::mono(14.0),
                    color,
                );
                if response.clicked()
                    && let Some(edit) = &mut self.keybinding_edit
                {
                    edit.recording = true;
                }
                if *recording {
                    ui.memory_mut(|m| {
                        m.request_focus(input_id);
                        m.set_focus_lock_filter(
                            input_id,
                            egui::EventFilter {
                                tab: true,
                                horizontal_arrows: true,
                                vertical_arrows: true,
                                escape: true,
                            },
                        );
                    });
                    self.capture_keybinding(ui, row);
                    // A click elsewhere ends the recording.
                    let elsewhere = ui
                        .input(|i| i.pointer.any_click().then(|| i.pointer.interact_pos()))
                        .flatten()
                        .is_some_and(|pos| !input.contains(pos));
                    if elsewhere && let Some(edit) = &mut self.keybinding_edit {
                        edit.recording = false;
                        if !edit.dirty(row) {
                            self.keybinding_edit = None;
                        }
                    }
                }
                x = input.left() - 6.0;
                if !*recording && *dirty {
                    let label = "Save";
                    let width =
                        (widgets::text_width(ui, label, font(14.0, Weight::Medium)) + 18.0).ceil();
                    let save = Rect::from_min_size(
                        Pos2::new(x - width, center_y - 14.0),
                        Vec2::new(width, 28.0),
                    );
                    let response = ui.interact(
                        save,
                        ui.id().with(("keybinding-save", &row.id)),
                        Sense::click(),
                    );
                    let fill = if response.hovered() {
                        theme::primary_hover()
                    } else {
                        theme::primary()
                    };
                    ui.painter().rect_filled(save, CornerRadius::same(8), fill);
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(Pos2::new(save.left() + 9.0, save.top()), save.max),
                        label,
                        font(14.0, Weight::Medium),
                        egui::Color32::WHITE,
                    );
                    let when = self
                        .keybinding_edit
                        .as_ref()
                        .map(|edit| edit.when.expression())
                        .unwrap_or_default();
                    if response.clicked() && *valid && !draft.trim().is_empty() {
                        let mut payload = json!({
                            "command": row.command,
                            "key": draft,
                            "replace": target(row),
                        });
                        if !when.trim().is_empty() {
                            payload["when"] = json!(when);
                        }
                        self.send_request(
                            "keybinding-save".into(),
                            "server.upsertKeybinding",
                            payload,
                        );
                        self.keybinding_edit = None;
                    }
                    x = save.left() - 6.0;
                }
            }
            _ => {
                // The pill is a ghost button overhanging the content edge.
                let chips_left = paint_pill(ui, &row.key, right, center_y);
                let button = Rect::from_min_max(
                    Pos2::new(chips_left - 7.0, center_y - 14.0),
                    Pos2::new(right + 6.0, center_y + 14.0),
                );
                let response = ui.interact(
                    button,
                    ui.id().with(("keybinding-pill", &row.id)),
                    Sense::click(),
                );
                if response.hovered() {
                    ui.painter().rect(
                        button,
                        CornerRadius::same(8),
                        theme::accent(),
                        Stroke::new(1.0, theme::alpha(theme::border(), 0.7)),
                        StrokeKind::Inside,
                    );
                    paint_pill(ui, &row.key, right, center_y);
                }
                if response.clicked() {
                    let mut edit = KeybindingEdit::new(row);
                    edit.recording = true;
                    self.keybinding_edit = Some(edit);
                }
                x = button.left() - 6.0;
            }
        }

        // Reset to default and Remove, shown while the row is hovered.
        let can_reset = row.custom && row.default_key.is_some();
        let can_remove = row.custom;
        let menu_id = format!("keybinding-menu:{}", row.id);
        if can_reset || can_remove {
            let button =
                Rect::from_min_size(Pos2::new(x - 28.0, center_y - 14.0), Vec2::splat(28.0));
            let menu_open = self.open_menu.as_deref() == Some(menu_id.as_str());
            let shown = ui.rect_contains_pointer(row_rect) || menu_open;
            if shown
                && widgets::icon_button(
                    ui,
                    button,
                    ("keybinding-actions", &row.id),
                    "ellipsis",
                    14.0,
                    theme::muted_foreground(),
                    "",
                )
                .clicked()
            {
                self.open_menu = (!menu_open).then(|| menu_id.clone());
            }
            let mut entries = Vec::new();
            if can_reset {
                entries.push(widgets::MenuEntry::Item(widgets::MenuItem::new(
                    "Reset to default",
                )));
            }
            if can_remove {
                entries.push(widgets::MenuEntry::Item(widgets::MenuItem {
                    destructive: true,
                    ..widgets::MenuItem::new("Remove")
                }));
            }
            let chosen = widgets::menu_aligned(
                ui,
                &mut self.open_menu,
                &menu_id,
                button,
                false,
                true,
                &entries,
            );
            match chosen {
                Some(0) if can_reset => {
                    let mut payload = json!({
                        "command": row.command,
                        "key": row.default_key,
                        "replace": target(row),
                    });
                    if !row.default_when.trim().is_empty() {
                        payload["when"] = json!(row.default_when);
                    }
                    self.send_request("keybinding-save".into(), "server.upsertKeybinding", payload);
                }
                Some(_) => {
                    self.send_request(
                        "keybinding-remove".into(),
                        "server.removeKeybinding",
                        target(row),
                    );
                }
                None => {}
            }
            x = button.left() - 6.0;
        }

        // `KeybindingConflictWarning`.
        let draft_key = editing
            .as_ref()
            .map_or(row.key.as_str(), |(d, ..)| d.as_str());
        let conflicts = if editing.is_some() {
            Vec::new()
        } else {
            row.conflicts.clone()
        };
        if !conflicts.is_empty() && !draft_key.is_empty() {
            let icon = Rect::from_min_size(Pos2::new(x - 20.0, center_y - 10.0), Vec2::splat(20.0));
            let description = if conflicts.len() == 1 {
                format!("Conflicts with {}.", conflicts[0])
            } else {
                let shown = conflicts
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                let more = if conflicts.len() > 3 {
                    ", and more"
                } else {
                    ""
                };
                format!("Conflicts with {shown}{more}.")
            };
            let response = ui.interact(
                icon,
                ui.id().with(("keybinding-conflict", &row.id)),
                Sense::hover(),
            );
            if response.hovered() {
                ui.painter().rect_filled(
                    icon,
                    CornerRadius::same(4),
                    theme::alpha(theme::warning_foreground(), 0.1),
                );
            }
            theme::paint_icon(
                ui,
                "triangle-alert",
                icon,
                14.0,
                theme::warning_foreground(),
            );
            response.on_hover_text(format!(
                "{description} The most recent matching binding wins when both conditions can apply."
            ));
        }
    }

    /// `KeybindingRowWhen`: "When" and the clause button, which opens the
    /// builder popover (`WhenClauseControl`).
    pub(crate) fn keybinding_when(&mut self, ui: &mut Ui, row: &KbRow, left: f32, top: f32) {
        widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(left, top), Vec2::new(40.0, 24.0)),
            "When",
            font(12.0, Weight::Regular),
            theme::alpha(theme::muted_foreground(), 0.7),
        );
        let editing = self
            .keybinding_edit
            .as_ref()
            .filter(|edit| edit.row == row.id);
        let expression = editing.map_or(row.when.clone(), |edit| edit.when.expression());
        let open = editing.is_some_and(|edit| edit.when_open);
        let label = if expression.is_empty() {
            "Always"
        } else {
            expression.as_str()
        };
        let mono = egui::FontId::monospace(11.0);
        let label_width = widgets::text_width(ui, label, mono.clone());
        let clause = Rect::from_min_size(
            Pos2::new(left + 38.0, top + 2.0),
            Vec2::new(label_width + 6.0 + 4.0 + 14.0 + 6.0, 20.0),
        );
        let response = ui.interact(
            clause,
            ui.id().with(("keybinding-when", &row.id)),
            Sense::click(),
        );
        if response.hovered() || open {
            ui.painter()
                .rect_filled(clause, CornerRadius::same(6), theme::accent());
        }
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(clause.left() + 6.0, clause.top()), clause.max),
            label,
            mono,
            if expression.is_empty() {
                theme::muted_foreground()
            } else {
                theme::foreground()
            },
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(clause.left() + 6.0 + label_width + 2.0, clause.top()),
                Vec2::new(14.0, 20.0),
            ),
            14.0,
            theme::muted_foreground(),
        );
        if response.clicked() {
            if open {
                if let Some(edit) = &mut self.keybinding_edit {
                    edit.when_open = false;
                }
            } else {
                let mut edit = match self.keybinding_edit.take() {
                    Some(edit) if edit.row == row.id => edit,
                    _ => KeybindingEdit::new(row),
                };
                edit.when_open = true;
                edit.recording = false;
                self.keybinding_edit = Some(edit);
            }
        }
        if open {
            self.when_popover(ui, row, clause);
        }
    }

    /// The builder in a popover under the clause button.
    fn when_popover(&mut self, ui: &mut Ui, row: &KbRow, clause: Rect) {
        let Some(edit) = &mut self.keybinding_edit else {
            return;
        };
        let height = when_builder::builder_height(ui, &edit.when);
        let popup = Rect::from_min_size(
            Pos2::new(clause.left(), clause.bottom() + 6.0),
            Vec2::new(when_builder::WIDTH, height),
        );
        let mut close = false;
        egui::Area::new(ui.id().with(("when-popover", &row.id)))
            .order(egui::Order::Foreground)
            .fixed_pos(popup.min)
            .show(ui.ctx(), |ui| {
                ui.painter().rect(
                    popup,
                    CornerRadius::same(10),
                    theme::popover(),
                    Stroke::new(1.0, theme::alpha(theme::foreground(), 0.1)),
                    StrokeKind::Inside,
                );
                let had_menu = edit.when.menu_open();
                when_builder::builder(ui, popup, &mut edit.when);
                let outside = ui
                    .input(|i| i.pointer.any_click().then(|| i.pointer.interact_pos()))
                    .flatten()
                    .is_some_and(|pos| !popup.contains(pos) && !clause.contains(pos));
                if outside && !had_menu {
                    close = true;
                }
            });
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            close = true;
        }
        if close {
            edit.when_open = false;
            if !edit.dirty(row) {
                self.keybinding_edit = None;
            }
        }
    }

    /// `captureKeybinding`: Escape restores the binding, another key records it.
    fn capture_keybinding(&mut self, ui: &Ui, row: &KbRow) {
        let events = ui.input(|i| i.events.clone());
        for event in events {
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if key == Key::Escape {
                self.keybinding_edit = None;
                return;
            }
            if let Some(binding) = binding_from_key(key, modifiers)
                && let Some(edit) = &mut self.keybinding_edit
            {
                edit.draft = binding;
                edit.recording = false;
                if !edit.dirty(row) {
                    self.keybinding_edit = None;
                }
                return;
            }
        }
    }
}

/// `DEFAULT_KEYBINDINGS` (`shared/keybindings.ts`): key, command, when.
const DEFAULTS: &[(&str, &str, &str)] = &[
    ("mod+b", "sidebar.toggle", ""),
    ("mod+[", "navigation.back", "!terminalFocus"),
    ("mod+]", "navigation.forward", "!terminalFocus"),
    ("mod+j", "terminal.toggle", ""),
    ("mod+alt+b", "rightPanel.toggle", ""),
    ("mod+d", "terminal.split", "terminalFocus"),
    ("mod+shift+d", "terminal.splitVertical", "terminalFocus"),
    ("mod+n", "terminal.new", "terminalFocus"),
    ("mod+w", "terminal.close", "terminalFocus"),
    ("mod+w", "rightPanel.close", "!terminalFocus"),
    ("mod+d", "diff.toggle", "!terminalFocus"),
    ("mod+shift+j", "preview.toggle", ""),
    ("mod+r", "preview.refresh", "previewFocus"),
    ("mod+l", "preview.focusUrl", "previewFocus"),
    ("mod+=", "preview.zoomIn", "previewFocus"),
    ("mod++", "preview.zoomIn", "previewFocus"),
    ("mod+-", "preview.zoomOut", "previewFocus"),
    ("mod+0", "preview.resetZoom", "previewFocus"),
    ("mod+k", "commandPalette.toggle", "!terminalFocus"),
    ("mod+p", "filePicker.toggle", "!terminalFocus"),
    ("mod+shift+f", "projectSearch.toggle", "!terminalFocus"),
    ("mod+u", "usage.open", "!terminalFocus"),
    ("mod+alt+a", "theme.select", "!terminalFocus"),
    ("mod+alt+shift+a", "appearance.cycle", "!terminalFocus"),
    ("mod+alt+shift+t", "themeEditor.toggle", ""),
    ("mod+s", "composer.stash", "!terminalFocus"),
    (
        "mod+shift+enter",
        "thread.steerQueuedMessage",
        "!terminalFocus",
    ),
    ("mod+n", "chat.new", "!terminalFocus"),
    ("mod+shift+o", "chat.new", "!terminalFocus"),
    ("mod+shift+n", "chat.newLocal", "!terminalFocus"),
    ("mod+alt+n", "chat.newWithoutProject", "!terminalFocus"),
    ("mod+shift+m", "modelPicker.toggle", "!terminalFocus"),
    ("mod+shift+h", "composer.host", "!terminalFocus"),
    ("mod+shift+e", "composer.effort", "!terminalFocus"),
    ("mod+shift+a", "composer.mode", "!terminalFocus"),
    ("mod+shift+x", "composer.workspace", "!terminalFocus"),
    ("mod+shift+g", "composer.branch", "!terminalFocus"),
    ("mod+shift+l", "composer.previousWorktree", "!terminalFocus"),
    ("mod+shift+k", "pullRequest.copyNumber", "!terminalFocus"),
    (
        "mod+shift+arrowup",
        "modelPicker.previousProvider",
        "modelPickerOpen",
    ),
    (
        "mod+shift+arrowdown",
        "modelPicker.nextProvider",
        "modelPickerOpen",
    ),
    ("mod+o", "editor.openFavorite", ""),
    ("mod+shift+[", "thread.previous", ""),
    ("mod+shift+]", "thread.next", ""),
    ("mod+shift+c", "thread.copyReference", "!terminalFocus"),
    ("mod+shift+s", "thread.settle", "!terminalFocus"),
    ("mod+shift+p", "thread.pin", "!terminalFocus"),
    ("mod+z", "thread.undo", "!terminalFocus && !editableFocus"),
    ("c", "usage.cost", "usagePageOpen"),
    ("t", "usage.tokens", "usagePageOpen"),
    ("l", "usage.limits", "usagePageOpen"),
    ("mod+shift+1", "usage.period.day", "usagePageOpen"),
    ("mod+shift+2", "usage.period.week", "usagePageOpen"),
    ("mod+shift+3", "usage.period.month", "usagePageOpen"),
    ("mod+shift+4", "usage.period.quarter", "usagePageOpen"),
    ("mod+1", "thread.jump.1", "isDesktop"),
    ("mod+2", "thread.jump.2", "isDesktop"),
    ("mod+3", "thread.jump.3", "isDesktop"),
    ("mod+4", "thread.jump.4", "isDesktop"),
    ("mod+5", "thread.jump.5", "isDesktop"),
    ("mod+6", "thread.jump.6", "isDesktop"),
    ("mod+7", "thread.jump.7", "isDesktop"),
    ("mod+8", "thread.jump.8", "isDesktop"),
    ("mod+9", "thread.jump.9", "isDesktop"),
    (
        "mod+1",
        "modelPicker.jump.1",
        "modelPickerOpen && isDesktop",
    ),
    (
        "mod+2",
        "modelPicker.jump.2",
        "modelPickerOpen && isDesktop",
    ),
    (
        "mod+3",
        "modelPicker.jump.3",
        "modelPickerOpen && isDesktop",
    ),
    (
        "mod+4",
        "modelPicker.jump.4",
        "modelPickerOpen && isDesktop",
    ),
    (
        "mod+5",
        "modelPicker.jump.5",
        "modelPickerOpen && isDesktop",
    ),
    (
        "mod+6",
        "modelPicker.jump.6",
        "modelPickerOpen && isDesktop",
    ),
    (
        "mod+7",
        "modelPicker.jump.7",
        "modelPickerOpen && isDesktop",
    ),
    (
        "mod+8",
        "modelPicker.jump.8",
        "modelPickerOpen && isDesktop",
    ),
    (
        "mod+9",
        "modelPicker.jump.9",
        "modelPickerOpen && isDesktop",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_round_trip_to_binding_strings() {
        let shortcut = json!({"key": "a", "modKey": true, "altKey": true, "shiftKey": true});
        assert_eq!(shortcut_input(&shortcut), "mod+alt+shift+a");
        assert_eq!(shortcut_input(&json!({"key": " "})), "space");
        assert_eq!(
            pill_parts("mod+alt+shift+a"),
            ["Ctrl", "Alt", "\u{21e7}", "A"]
        );
    }

    #[test]
    fn key_presses_record_like_the_web_client() {
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            binding_from_key(Key::Y, ctrl_shift).as_deref(),
            Some("mod+shift+y")
        );
        assert_eq!(
            binding_from_key(Key::F5, Modifiers::NONE).as_deref(),
            Some("f5")
        );
        assert_eq!(
            binding_from_key(Key::OpenBracket, Modifiers::COMMAND).as_deref(),
            Some("mod+[")
        );
        assert_eq!(binding_from_key(Key::Insert, Modifiers::NONE), None);
    }

    #[test]
    fn defaults_are_not_custom_and_overlaps_conflict() {
        let bindings = json!([
            {"command": "sidebar.toggle", "shortcut": {"key": "b", "modKey": true}},
            {"command": "terminal.toggle", "shortcut": {"key": "b", "modKey": true}},
        ]);
        let rows = build_rows(&bindings, |_| String::new());
        assert!(!rows[0].custom);
        assert!(rows[1].custom);
        assert_eq!(rows[1].default_key.as_deref(), Some("mod+j"));
        assert_eq!(rows[0].conflicts, ["Terminal: Toggle"]);
    }
}
