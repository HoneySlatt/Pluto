//! Keybinding editing (`settings/KeybindingsSettings.tsx`,
//! `KeybindingsSettings.logic.ts`): a binding's pill records a new shortcut,
//! Save upserts it with `server.upsertKeybinding`, and custom bindings offer
//! Reset to default and Remove (`server.removeKeybinding`).
//!
//! The when clause opens its builder (`when_builder`); the header adds
//! bindings (`NewKeybindingSettingsRow`) and searches them.
//!
//! Conflicts show their warning icon only.
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

/// The id of the add-binding row.
pub(crate) const NEW_ROW: &str = "new";

/// The row whose shortcut or when clause is being edited, or the new binding.
pub struct KeybindingEdit {
    row: String,
    draft: String,
    recording: bool,
    when: WhenDraft,
    when_open: bool,
    /// The new binding's command.
    command: Option<String>,
    command_open: bool,
}

impl KeybindingEdit {
    fn new(row: &KbRow) -> Self {
        Self {
            row: row.id.clone(),
            draft: row.key.clone(),
            recording: false,
            when: WhenDraft::new(When::from_value(&row.when_ast)),
            when_open: false,
            command: None,
            command_open: false,
        }
    }

    /// `useNewKeybindingDraft`: no command, unassigned, always.
    pub(crate) fn new_binding() -> Self {
        Self {
            row: NEW_ROW.into(),
            draft: String::new(),
            recording: false,
            when: WhenDraft::new(None),
            when_open: false,
            command: None,
            command_open: false,
        }
    }

    /// The placeholder row the add-binding form edits.
    pub(crate) fn new_row(&self) -> KbRow {
        KbRow {
            id: NEW_ROW.into(),
            command: self.command.clone().unwrap_or_default(),
            key: String::new(),
            when: String::new(),
            when_ast: Value::Null,
            custom: false,
            default_key: None,
            default_when: String::new(),
            conflicts: Vec::new(),
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
                self.capture_input(ui, row, input, draft, *recording);
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
            .edit_slot(&row.id)
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
            let slot = self.edit_slot(&row.id);
            if open {
                if let Some(edit) = slot {
                    edit.when_open = false;
                }
            } else {
                let mut edit = match slot.take() {
                    Some(edit) if edit.row == row.id => edit,
                    _ => KeybindingEdit::new(row),
                };
                edit.when_open = true;
                edit.recording = false;
                *slot = Some(edit);
            }
        }
        if open {
            self.when_popover(ui, row, clause);
        }
    }

    /// The builder in a popover under the clause button.
    fn when_popover(&mut self, ui: &mut Ui, row: &KbRow, clause: Rect) {
        let slot = self.edit_slot(&row.id);
        let Some(edit) = slot.as_mut() else {
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
            if row.id != NEW_ROW && !edit.dirty(row) {
                *slot = None;
            }
        }
    }

    /// The shortcut input (`Input font="mono"`): "Press shortcut" while
    /// recording, the draft otherwise; a click records, a click elsewhere stops.
    fn capture_input(
        &mut self,
        ui: &mut Ui,
        row: &KbRow,
        input: Rect,
        draft: &str,
        recording: bool,
    ) {
        let input_id = ui.id().with(("keybinding-capture", &row.id));
        let response = ui.interact(input, input_id, Sense::click());
        ui.painter().rect(
            input,
            CornerRadius::same(10),
            theme::control_surface(),
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
        let (text, color) = if recording {
            ("Press shortcut", theme::muted_foreground())
        } else if draft.is_empty() {
            ("Unassigned", theme::muted_foreground())
        } else {
            (draft, theme::foreground())
        };
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(input.left() + 11.0, input.top()), input.max),
            text,
            theme::mono(14.0),
            color,
        );
        if response.clicked()
            && let Some(edit) = self.edit_slot(&row.id)
        {
            edit.recording = true;
        }
        if !recording {
            return;
        }
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
        let elsewhere = ui
            .input(|i| i.pointer.any_click().then(|| i.pointer.interact_pos()))
            .flatten()
            .is_some_and(|pos| !input.contains(pos));
        let new = row.id == NEW_ROW;
        let slot = self.edit_slot(&row.id);
        if elsewhere && let Some(edit) = slot {
            edit.recording = false;
            if !new && !edit.dirty(row) {
                *slot = None;
            }
        }
    }

    /// `NewKeybindingSettingsRow`'s control: command select, conflict
    /// warning, shortcut input, Save and cancel, right to left from `right`.
    pub(crate) fn new_keybinding_control(
        &mut self,
        ui: &mut Ui,
        rows: &[KbRow],
        right: f32,
        center_y: f32,
    ) {
        let Some(edit) = &self.new_keybinding else {
            return;
        };
        let row = edit.new_row();
        let (draft, recording, command, command_open) = (
            edit.draft.clone(),
            edit.recording,
            edit.command.clone(),
            edit.command_open,
        );
        let when = edit.when.expression();
        let can_save = command.is_some() && !draft.trim().is_empty() && edit.when.valid;

        let cancel =
            Rect::from_min_size(Pos2::new(right - 28.0, center_y - 14.0), Vec2::splat(28.0));
        if widgets::icon_button(
            ui,
            cancel,
            "new-keybinding-cancel",
            "x",
            14.0,
            theme::muted_foreground(),
            "Cancel",
        )
        .clicked()
        {
            self.new_keybinding = None;
            return;
        }
        let label = "Save";
        let width = (widgets::text_width(ui, label, font(14.0, Weight::Medium)) + 18.0).ceil();
        let save = Rect::from_min_size(
            Pos2::new(cancel.left() - 8.0 - width, center_y - 14.0),
            Vec2::new(width, 28.0),
        );
        let response = ui.interact(
            save,
            ui.id().with("new-keybinding-save"),
            if can_save {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let fill = if response.hovered() && can_save {
            theme::primary_hover()
        } else {
            theme::primary()
        };
        ui.painter().rect_filled(
            save,
            CornerRadius::same(8),
            if can_save {
                fill
            } else {
                theme::alpha(fill, 0.64)
            },
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(save.left() + 9.0, save.top()), save.max),
            label,
            font(14.0, Weight::Medium),
            egui::Color32::from_white_alpha(if can_save { 255 } else { 163 }),
        );
        if response.clicked()
            && let Some(command) = &command
        {
            let mut payload = json!({"command": command, "key": draft});
            if !when.trim().is_empty() {
                payload["when"] = json!(when);
            }
            self.send_request("keybinding-save".into(), "server.upsertKeybinding", payload);
            self.new_keybinding = None;
            return;
        }
        let input = Rect::from_min_size(
            Pos2::new(save.left() - 8.0 - 176.0, center_y - 14.0),
            Vec2::new(176.0, 28.0),
        );
        self.capture_input(ui, &row, input, &draft, recording);
        let mut x = input.left() - 8.0;
        let conflicts = conflict_labels(rows, NEW_ROW, &draft, &when);
        if !conflicts.is_empty() {
            let icon = Rect::from_min_size(Pos2::new(x - 20.0, center_y - 10.0), Vec2::splat(20.0));
            theme::paint_icon(
                ui,
                "triangle-alert",
                icon,
                14.0,
                theme::warning_foreground(),
            );
            ui.interact(
                icon,
                ui.id().with("new-keybinding-conflict"),
                Sense::hover(),
            )
            .on_hover_text(format!("Conflicts with {}.", conflicts.join(", ")));
            x = icon.left() - 8.0;
        }
        // The command select (`SelectTrigger size="sm"`, w-56).
        let trigger = Rect::from_min_size(
            Pos2::new(x - 224.0, center_y - 14.0),
            Vec2::new(224.0, 28.0),
        );
        let response = ui.interact(
            trigger,
            ui.id().with("new-keybinding-command"),
            Sense::click(),
        );
        ui.painter().rect(
            trigger,
            CornerRadius::same(10),
            if response.hovered() {
                theme::overlay(0.06)
            } else {
                theme::control_surface()
            },
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
        let (text, color) = match &command {
            Some(command) => (command_label(command), theme::foreground()),
            None => ("Command".to_owned(), theme::muted_foreground()),
        };
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(trigger.left() + 10.0, trigger.top()),
                Pos2::new(trigger.right() - 24.0, trigger.bottom()),
            ),
            &text,
            font(14.0, Weight::Regular),
            color,
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(trigger.right() - 18.0, trigger.center().y - 6.0),
                Vec2::splat(12.0),
            ),
            12.0,
            theme::muted_foreground(),
        );
        if response.clicked()
            && let Some(edit) = &mut self.new_keybinding
        {
            edit.command_open = !edit.command_open;
        }
        if command_open {
            self.command_menu(ui, trigger, rows, command.as_deref());
        }
    }

    /// The command options (`buildKeybindingCommandOptions`) in a scrolling
    /// popup under the trigger.
    fn command_menu(&mut self, ui: &mut Ui, trigger: Rect, rows: &[KbRow], current: Option<&str>) {
        let mut commands: Vec<String> = crate::ui::settings_search::keybinding_commands()
            .iter()
            .map(|c| (*c).to_owned())
            .collect();
        for row in rows {
            if !commands.contains(&row.command) {
                commands.push(row.command.clone());
            }
        }
        commands.sort_by_key(|command| {
            crate::ui::settings::command_order(command, &command_label(command))
        });
        let item_font = font(14.0, Weight::Regular);
        let width = commands
            .iter()
            .map(|c| widgets::text_width(ui, &command_label(c), item_font.clone()))
            .fold(trigger.width() - 8.0, f32::max)
            + 8.0
            + 24.0;
        let height = (commands.len() as f32 * 28.0 + 8.0).min(288.0);
        let popup = Rect::from_min_size(
            Pos2::new(trigger.left(), trigger.bottom() + 4.0),
            Vec2::new(width, height),
        );
        let mut chosen = None;
        let mut close = false;
        egui::Area::new(ui.id().with("new-keybinding-commands"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup.min)
            .show(ui.ctx(), |ui| {
                widgets::popup_surface(ui, popup);
                let inner = popup.shrink(4.0);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                child.set_clip_rect(inner);
                egui::ScrollArea::vertical()
                    .id_salt("new-keybinding-command-list")
                    .auto_shrink([false, false])
                    .show(&mut child, |ui| {
                        let origin = ui.max_rect().min;
                        for (index, command) in commands.iter().enumerate() {
                            let item = Rect::from_min_size(
                                Pos2::new(origin.x, origin.y + index as f32 * 28.0),
                                Vec2::new(inner.width(), 28.0),
                            );
                            let response = ui.interact(
                                item,
                                ui.id().with(("command-option", index)),
                                Sense::click(),
                            );
                            if response.hovered() {
                                ui.painter().rect_filled(
                                    item,
                                    CornerRadius::same(6),
                                    theme::accent(),
                                );
                            }
                            widgets::text_in(
                                ui,
                                Rect::from_min_max(
                                    Pos2::new(item.left() + 8.0, item.top()),
                                    item.max,
                                ),
                                &command_label(command),
                                item_font.clone(),
                                theme::foreground(),
                            );
                            if current == Some(command.as_str()) {
                                theme::paint_icon(
                                    ui,
                                    "check",
                                    Rect::from_min_size(
                                        Pos2::new(item.right() - 22.0, item.top()),
                                        Vec2::new(14.0, 28.0),
                                    ),
                                    14.0,
                                    theme::foreground(),
                                );
                            }
                            if response.clicked() {
                                chosen = Some(command.clone());
                            }
                        }
                        ui.allocate_rect(
                            Rect::from_min_size(
                                origin,
                                Vec2::new(inner.width(), commands.len() as f32 * 28.0),
                            ),
                            Sense::hover(),
                        );
                    });
                let outside = ui
                    .input(|i| i.pointer.any_click().then(|| i.pointer.interact_pos()))
                    .flatten()
                    .is_some_and(|pos| !popup.contains(pos) && !trigger.contains(pos));
                close = outside;
            });
        if let Some(edit) = &mut self.new_keybinding {
            if let Some(command) = chosen {
                edit.command = Some(command);
                edit.command_open = false;
            } else if close || ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                edit.command_open = false;
            }
        }
    }

    /// The edit state of a binding row or of the add-binding row.
    fn edit_slot(&mut self, row: &str) -> &mut Option<KeybindingEdit> {
        if row == NEW_ROW {
            &mut self.new_keybinding
        } else {
            &mut self.keybinding_edit
        }
    }

    /// `captureKeybinding`: Escape restores the binding (clears the new
    /// one), another key records it.
    fn capture_keybinding(&mut self, ui: &Ui, row: &KbRow) {
        let new = row.id == NEW_ROW;
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
            let slot = self.edit_slot(&row.id);
            if key == Key::Escape {
                if new {
                    if let Some(edit) = slot {
                        edit.draft.clear();
                        edit.recording = false;
                    }
                } else {
                    *slot = None;
                }
                return;
            }
            if let Some(binding) = binding_from_key(key, modifiers)
                && let Some(edit) = slot
            {
                edit.draft = binding;
                edit.recording = false;
                if !new && !edit.dirty(row) {
                    *slot = None;
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
    ("mod+shift+b", "threadPanel.toggle", "!terminalFocus"),
    (
        "mod+t",
        "rightPanel.new",
        "rightPanelOpen && !terminalFocus",
    ),
    ("mod+d", "terminal.split", "terminalFocus"),
    ("mod+shift+d", "terminal.splitVertical", "terminalFocus"),
    ("mod+n", "terminal.new", "terminalFocus"),
    ("mod+w", "terminal.close", "terminalFocus"),
    ("mod+w", "rightPanel.close", "!terminalFocus"),
    ("mod+shift+t", "view.reopenClosed", ""),
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
    ("alt+arrowup", "thread.editQueuedMessage", "composerFocus"),
    (
        "mod+enter",
        "composer.sendAlternate",
        "composerFocus && turnRunning",
    ),
    (
        "mod+enter",
        "composer.sendBackground",
        "composerFocus && draftThreadRoute",
    ),
    (
        "mod+alt+enter",
        "composer.sendBackground",
        "composerFocus && draftThreadRoute",
    ),
    (
        "mod+alt+enter",
        "composer.sendAndNewThread",
        "composerFocus && !draftThreadRoute",
    ),
    ("mod+n", "chat.new", "!terminalFocus"),
    ("mod+shift+o", "chat.new", "!terminalFocus"),
    ("mod+shift+n", "chat.newLocal", "!terminalFocus"),
    ("mod+alt+n", "chat.newWithoutProject", "!terminalFocus"),
    ("mod+f", "chat.find", "!terminalFocus && !previewFocus"),
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
    ("mod+1", "thread.jump.1", "!modelPickerOpen && isDesktop"),
    ("mod+2", "thread.jump.2", "!modelPickerOpen && isDesktop"),
    ("mod+3", "thread.jump.3", "!modelPickerOpen && isDesktop"),
    ("mod+4", "thread.jump.4", "!modelPickerOpen && isDesktop"),
    ("mod+5", "thread.jump.5", "!modelPickerOpen && isDesktop"),
    ("mod+6", "thread.jump.6", "!modelPickerOpen && isDesktop"),
    ("mod+7", "thread.jump.7", "!modelPickerOpen && isDesktop"),
    ("mod+8", "thread.jump.8", "!modelPickerOpen && isDesktop"),
    ("mod+9", "thread.jump.9", "!modelPickerOpen && isDesktop"),
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
