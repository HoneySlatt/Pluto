//! The Add/Edit Action dialog for a project's actions, shared by the Project
//! settings page and the chat header. Ported from
//! `components/projectScriptEditor.tsx`, `components/settings/useProjectScriptSettings.ts`,
//! `projectScripts.ts` and `lib/projectScriptKeybindings.ts`.
//!
//! Actions are saved in the project's override entry
//! (`projectSettingsOverrides[project].defaultProjectScripts`); a shortcut is
//! upserted (or removed) with the keybinding RPCs, as the desktop app does.

use eframe::egui::{
    self, Color32, CornerRadius, Event, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::{Map, Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::keybindings::{binding_from_key, shortcut_input};
use crate::ui::widgets;

/// `SCRIPT_ICONS`: id, label, Lucide icon.
pub(crate) const SCRIPT_ICONS: [(&str, &str, &str); 6] = [
    ("play", "Play", "play"),
    ("test", "Test", "flask-conical"),
    ("lint", "Lint", "list-checks"),
    ("configure", "Configure", "wrench"),
    ("build", "Build", "hammer"),
    ("debug", "Debug", "bug"),
];

/// `ScriptIcon`: the Lucide icon of a script icon id.
pub(crate) fn script_icon(id: &str) -> &'static str {
    SCRIPT_ICONS
        .iter()
        .find(|(value, ..)| *value == id)
        .map_or("play", |(.., icon)| icon)
}

/// `MAX_SCRIPT_ID_LENGTH`.
const MAX_SCRIPT_ID: usize = 24;
const WIDTH: f32 = 512.0;
const DESTRUCTIVE: Color32 = Color32::from_rgb(251, 65, 74);

/// `normalizeScriptId`.
fn normalize_script_id(value: &str) -> String {
    let mut out = String::new();
    for c in value.trim().to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let cleaned = out.trim_matches('-').to_owned();
    if cleaned.is_empty() {
        return "script".into();
    }
    if cleaned.len() <= MAX_SCRIPT_ID {
        return cleaned;
    }
    let cut = cleaned[..MAX_SCRIPT_ID].trim_end_matches('-').to_owned();
    if cut.is_empty() { "script".into() } else { cut }
}

/// `nextProjectScriptId`: the name's slug, suffixed `-2`, `-3`… while taken.
pub(crate) fn next_script_id(name: &str, taken: &[String]) -> String {
    let base = normalize_script_id(name);
    if !taken.contains(&base) {
        return base;
    }
    for suffix in 2..10_000 {
        let candidate = format!("{base}-{suffix}");
        let safe = if candidate.len() <= MAX_SCRIPT_ID {
            candidate
        } else {
            let keep = MAX_SCRIPT_ID
                .saturating_sub(suffix.to_string().len() + 1)
                .max(1);
            format!("{}-{suffix}", &base[..keep.min(base.len())])
        };
        if !taken.contains(&safe) {
            return safe;
        }
    }
    base
}

/// `commandForProjectScript`: `script.<id>.run` while the id fits.
pub(crate) fn script_command(id: &str) -> Option<String> {
    (!id.is_empty() && id.len() <= MAX_SCRIPT_ID).then(|| format!("script.{id}.run"))
}

/// `keybindingValueForCommand`: the last resolved binding of `command`.
pub(crate) fn keybinding_for(bindings: &Value, command: &str) -> Option<String> {
    bindings
        .as_array()?
        .iter()
        .rev()
        .find(|binding| binding["command"] == command)
        .map(|binding| shortcut_input(&binding["shortcut"]))
}

/// `shortcutLabelForCommand` (non-Mac `formatShortcutLabel`): "Ctrl+Shift+L".
pub(crate) fn shortcut_label_for(bindings: &Value, command: &str) -> Option<String> {
    let binding = bindings
        .as_array()?
        .iter()
        .rev()
        .find(|binding| binding["command"] == command)?;
    let shortcut = &binding["shortcut"];
    let key = shortcut["key"].as_str().unwrap_or_default();
    let key_label = match key {
        " " => "Space".to_owned(),
        "escape" => "Esc".to_owned(),
        "arrowup" => "Up".to_owned(),
        "arrowdown" => "Down".to_owned(),
        "arrowleft" => "Left".to_owned(),
        "arrowright" => "Right".to_owned(),
        key if key.chars().count() == 1 => key.to_uppercase(),
        key => {
            let mut chars = key.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    };
    let mut parts = Vec::new();
    if shortcut["ctrlKey"] == true || shortcut["modKey"] == true {
        parts.push("Ctrl".to_owned());
    }
    if shortcut["altKey"] == true {
        parts.push("Alt".to_owned());
    }
    if shortcut["shiftKey"] == true {
        parts.push("Shift".to_owned());
    }
    if shortcut["metaKey"] == true {
        parts.push("Meta".to_owned());
    }
    parts.push(key_label);
    Some(parts.join("+"))
}

pub(crate) struct ActionEditor {
    project_id: String,
    /// The edited script; None adds one.
    script_id: Option<String>,
    name: String,
    command: String,
    icon: String,
    run_on_worktree_create: bool,
    wait_for_setup: bool,
    keybinding: String,
    preview_url: String,
    auto_open_preview: bool,
    error: Option<String>,
    icon_picker: bool,
    recording: bool,
    delete_confirm: bool,
    focused: bool,
}

impl App {
    /// The project's actions (`resolveProjectScripts` once settings are
    /// folded): its override, else the environment defaults.
    pub(crate) fn project_scripts(&self, project_id: &str) -> Vec<Value> {
        let settings = &self.state.config["settings"];
        settings["projectSettingsOverrides"][project_id]["defaultProjectScripts"]
            .as_array()
            .or(settings["defaultProjectScripts"].as_array())
            .cloned()
            .unwrap_or_default()
    }

    /// `ProjectScriptEditorDialog` for a new action, or `editorRequestForScript`.
    pub(crate) fn open_action_editor(&mut self, project_id: &str, script: Option<&Value>) {
        let text = |key: &str| {
            script
                .and_then(|s| s[key].as_str())
                .unwrap_or_default()
                .to_owned()
        };
        let run = script.is_some_and(|s| s["runOnWorktreeCreate"] == true);
        let id = script.and_then(|s| s["id"].as_str()).map(str::to_owned);
        let keybinding = id
            .as_deref()
            .and_then(script_command)
            .and_then(|command| keybinding_for(&self.state.config["keybindings"], &command))
            .unwrap_or_default();
        self.action_editor = Some(ActionEditor {
            project_id: project_id.to_owned(),
            script_id: id,
            name: text("name"),
            command: text("command"),
            icon: script
                .and_then(|s| s["icon"].as_str())
                .unwrap_or("play")
                .to_owned(),
            run_on_worktree_create: run,
            wait_for_setup: run && script.is_some_and(|s| s["async"] == false),
            keybinding,
            preview_url: text("previewUrl"),
            auto_open_preview: script.is_some_and(|s| s["autoOpenPreview"] == true),
            error: None,
            icon_picker: false,
            recording: false,
            delete_confirm: false,
            focused: false,
        });
    }

    /// Replaces the project's action list in its override entry, then syncs
    /// the shortcut of `script_id` (`persist`).
    fn save_project_scripts(
        &mut self,
        project_id: &str,
        scripts: Vec<Value>,
        script_id: &str,
        keybinding: Option<String>,
    ) {
        let mut entry: Map<String, Value> =
            self.state.config["settings"]["projectSettingsOverrides"][project_id]
                .as_object()
                .cloned()
                .unwrap_or_default();
        entry.insert(
            "defaultProjectScripts".into(),
            Value::Array(scripts.clone()),
        );
        self.save_project_overrides(project_id, entry);
        let Some(command) = script_command(script_id) else {
            return;
        };
        let previous = keybinding_for(&self.state.config["keybindings"], &command);
        match keybinding.filter(|k| !k.is_empty()) {
            Some(next) => {
                let mut payload = json!({"key": next, "command": command});
                if let Some(previous) = previous.filter(|p| *p != next) {
                    payload["replace"] = json!({"key": previous, "command": command});
                }
                self.send_request("keybinding-save".into(), "server.upsertKeybinding", payload);
            }
            None => {
                // Keep a binding another list still uses.
                let settings = &self.state.config["settings"];
                let uses = |list: &Value| {
                    list.as_array()
                        .is_some_and(|l| l.iter().any(|s| s["id"] == script_id))
                };
                let kept = scripts.iter().any(|s| s["id"] == script_id);
                let elsewhere = uses(&settings["defaultProjectScripts"])
                    || settings["projectSettingsOverrides"]
                        .as_object()
                        .is_some_and(|all| {
                            all.iter().any(|(id, entry)| {
                                id != project_id && uses(&entry["defaultProjectScripts"])
                            })
                        });
                if let Some(previous) = previous
                    && (kept || !elsewhere)
                {
                    self.send_request(
                        "keybinding-remove".into(),
                        "server.removeKeybinding",
                        json!({"key": previous, "command": command}),
                    );
                }
            }
        }
    }

    /// The dialog over `screen`.
    pub(crate) fn action_editor_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(mut editor) = self.action_editor.take() else {
            return;
        };
        let editing = editor.script_id.is_some();
        let help = "Press a shortcut. Use Backspace to clear. Shortcuts are environment-wide. Projects using the same action share its shortcut.";
        let help_lines = widgets::galley(
            ui,
            help,
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
            16.0,
            WIDTH - 50.0,
        )
        .size()
        .y;
        let description =
            "Actions are project-scoped commands you can run from the top bar or keybindings.";
        let description_height = widgets::galley(
            ui,
            description,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            20.0,
            WIDTH - 50.0,
        )
        .size()
        .y;
        let error_height = if editor.error.is_some() {
            16.0 + 20.0
        } else {
            0.0
        };
        let body = 22.0
            + 32.0
            + 20.0
            + 22.0
            + 32.0
            + 6.0
            + help_lines
            + 20.0
            + 22.0
            + 72.0
            + 20.0
            + 22.0
            + 32.0
            + 6.0
            + 16.0
            + 16.0
            + 3.0 * 38.0
            + 2.0 * 16.0
            + error_height;
        let height = 25.0 + 20.0 + 8.0 + description_height + 20.0 + body + 20.0 + 32.0 + 17.0;
        let d = Rect::from_center_size(screen.center(), Vec2::new(WIDTH, height));
        let mut close = false;
        let mut save = false;
        let mut delete = false;
        egui::Area::new(ui.id().with("action-editor"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if response.clicked()
                    && !ui.rect_contains_pointer(d)
                    && !editor.icon_picker
                    && !editor.delete_confirm
                {
                    close = true;
                }
                ui.painter().rect(
                    d,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let close_rect = Rect::from_min_size(
                    Pos2::new(d.right() - 41.0, d.top() + 9.0),
                    Vec2::splat(32.0),
                );
                if widgets::icon_button(
                    ui,
                    close_rect,
                    "action-editor-close",
                    "x",
                    16.0,
                    theme::muted_foreground(),
                    "",
                )
                .clicked()
                {
                    close = true;
                }
                let left = d.left() + 25.0;
                let right = d.right() - 25.0;
                let width = right - left;
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    if editing { "Edit Action" } else { "Add Action" },
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                let galley = widgets::galley(
                    ui,
                    description,
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                    width,
                );
                ui.painter().galley(
                    Pos2::new(left, d.top() + 53.0),
                    galley,
                    theme::muted_foreground(),
                );
                let mut y = d.top() + 53.0 + description_height + 20.0;

                // Name, with the icon button.
                label(ui, Pos2::new(left, y), "Name");
                y += 22.0;
                let icon_button = Rect::from_min_size(Pos2::new(left, y), Vec2::new(36.0, 32.0));
                let response =
                    ui.interact(icon_button, ui.id().with("action-icon"), Sense::click());
                ui.painter().rect(
                    icon_button,
                    CornerRadius::same(8),
                    if response.hovered() {
                        theme::overlay(0.06)
                    } else {
                        theme::control_surface()
                    },
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                theme::paint_icon(
                    ui,
                    script_icon(&editor.icon),
                    icon_button,
                    18.0,
                    theme::foreground(),
                );
                if response.clicked() {
                    editor.icon_picker = !editor.icon_picker;
                }
                let name_rect = Rect::from_min_max(
                    Pos2::new(icon_button.right() + 8.0, y),
                    Pos2::new(right, y + 32.0),
                );
                let name = input(
                    ui,
                    name_rect,
                    "action-name",
                    &mut editor.name,
                    "Test",
                    false,
                );
                if !editor.focused {
                    name.request_focus();
                    editor.focused = true;
                }
                y += 32.0 + 20.0;

                // Keybinding: a read-only field recording the next shortcut.
                label(ui, Pos2::new(left, y), "Keybinding");
                y += 22.0;
                let key_rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 32.0));
                let key_response =
                    ui.interact(key_rect, ui.id().with("action-keybinding"), Sense::click());
                if key_response.clicked() {
                    editor.recording = true;
                    ui.memory_mut(|m| m.surrender_focus(name.id));
                } else if ui.input(|i| i.pointer.any_pressed())
                    && !ui.rect_contains_pointer(key_rect)
                {
                    editor.recording = false;
                }
                ui.painter().rect(
                    key_rect,
                    CornerRadius::same(10),
                    theme::control_surface(),
                    Stroke::new(
                        1.0,
                        if editor.recording {
                            theme::alpha(theme::primary(), 0.6)
                        } else {
                            theme::input()
                        },
                    ),
                    StrokeKind::Inside,
                );
                let key_inner = Rect::from_min_max(
                    Pos2::new(key_rect.left() + 12.0, key_rect.top()),
                    Pos2::new(key_rect.right() - 12.0, key_rect.bottom()),
                );
                if editor.keybinding.is_empty() {
                    widgets::text_in(
                        ui,
                        key_inner,
                        "Press shortcut",
                        font(14.0, Weight::Regular),
                        theme::muted_foreground(),
                    );
                } else {
                    widgets::text_in(
                        ui,
                        key_inner,
                        &editor.keybinding,
                        font(14.0, Weight::Regular),
                        theme::foreground(),
                    );
                }
                if editor.recording {
                    // `captureKeybinding`: everything but Tab is captured.
                    let pressed: Vec<(Key, egui::Modifiers)> = ui.input(|i| {
                        i.events
                            .iter()
                            .filter_map(|event| match event {
                                Event::Key {
                                    key,
                                    pressed: true,
                                    modifiers,
                                    ..
                                } => Some((*key, *modifiers)),
                                _ => None,
                            })
                            .collect()
                    });
                    for (key, modifiers) in pressed {
                        match key {
                            Key::Tab => editor.recording = false,
                            Key::Backspace | Key::Delete => editor.keybinding.clear(),
                            _ => {
                                if let Some(binding) = binding_from_key(key, modifiers) {
                                    editor.keybinding = binding;
                                }
                            }
                        }
                    }
                }
                y += 32.0 + 6.0;
                let galley = widgets::galley(
                    ui,
                    help,
                    font(12.0, Weight::Regular),
                    theme::muted_foreground(),
                    16.0,
                    width,
                );
                ui.painter()
                    .galley(Pos2::new(left, y), galley, theme::muted_foreground());
                y += help_lines + 20.0;

                // Command (a textarea).
                label(ui, Pos2::new(left, y), "Command");
                y += 22.0;
                let command_rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 72.0));
                input(
                    ui,
                    command_rect,
                    "action-command",
                    &mut editor.command,
                    "bun test",
                    true,
                );
                y += 72.0 + 20.0;

                label(ui, Pos2::new(left, y), "Preview URL (optional)");
                y += 22.0;
                input(
                    ui,
                    Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 32.0)),
                    "action-preview",
                    &mut editor.preview_url,
                    "http://localhost:5173",
                    false,
                );
                y += 32.0 + 6.0;
                widgets::text(
                    ui,
                    Pos2::new(left, y),
                    "Open this URL in the in-app preview when this action runs.",
                    font(12.0, Weight::Regular),
                    theme::muted_foreground(),
                    16.0,
                );
                y += 16.0 + 16.0;

                let has_preview = !editor.preview_url.trim().is_empty();
                let run = editor.run_on_worktree_create;
                let toggles: [(&str, bool, bool); 3] = [
                    (
                        "Run automatically on worktree creation",
                        editor.run_on_worktree_create,
                        true,
                    ),
                    (
                        "Wait for it to finish before the agent starts",
                        editor.wait_for_setup,
                        run,
                    ),
                    (
                        "Open preview automatically when this action runs",
                        editor.auto_open_preview,
                        has_preview,
                    ),
                ];
                for (index, (text, on, enabled)) in toggles.into_iter().enumerate() {
                    let row = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 38.0));
                    if toggle_row(ui, row, index, text, on, enabled) {
                        match index {
                            0 => editor.run_on_worktree_create = !on,
                            1 => editor.wait_for_setup = !on,
                            _ => editor.auto_open_preview = !on,
                        }
                    }
                    y += 38.0 + 16.0;
                }
                y -= 16.0;
                if let Some(error) = &editor.error {
                    y += 16.0;
                    widgets::text(
                        ui,
                        Pos2::new(left, y),
                        error,
                        font(14.0, Weight::Regular),
                        DESTRUCTIVE,
                        20.0,
                    );
                    y += 20.0;
                }

                // Footer: Delete (editing) at the start; Cancel and Save at the end.
                let footer = y + 20.0;
                let save_label = if editing {
                    "Save changes"
                } else {
                    "Save action"
                };
                let save_rect = footer_rect(ui, right, footer, save_label);
                let cancel_rect = footer_rect(ui, save_rect.left() - 8.0, footer, "Cancel");
                if footer_button(ui, save_rect, save_label, Kind::Primary) {
                    save = true;
                }
                if footer_button(ui, cancel_rect, "Cancel", Kind::Outline) {
                    close = true;
                }
                if editing {
                    let delete_width =
                        widgets::text_width(ui, "Delete", font(14.0, Weight::Medium)) + 24.0;
                    let delete_rect = Rect::from_min_size(
                        Pos2::new(left, footer),
                        Vec2::new(delete_width.ceil(), 32.0),
                    );
                    if footer_button(ui, delete_rect, "Delete", Kind::Destructive) {
                        editor.delete_confirm = true;
                    }
                }

                // The icon popover (`SCRIPT_ICONS` in three columns).
                if editor.icon_picker {
                    let cell = Vec2::new(71.6, 58.0);
                    let popover = Rect::from_min_size(
                        Pos2::new(icon_button.left(), icon_button.bottom() + 4.0),
                        Vec2::new(3.0 * cell.x + 2.0 * 8.0 + 34.0, 2.0 * cell.y + 8.0 + 34.0),
                    );
                    let area = egui::Area::new(ui.id().with("action-icon-picker"))
                        .order(egui::Order::Tooltip)
                        .fixed_pos(popover.min)
                        .show(ui.ctx(), |ui| {
                            let (rect, _) = ui.allocate_exact_size(popover.size(), Sense::hover());
                            widgets::popup_surface(ui, rect);
                            for (index, (id, text, icon)) in SCRIPT_ICONS.iter().enumerate() {
                                let min = Pos2::new(
                                    rect.left() + 17.0 + (index % 3) as f32 * (cell.x + 8.0),
                                    rect.top() + 17.0 + (index / 3) as f32 * (cell.y + 8.0),
                                );
                                let button = Rect::from_min_size(min, cell);
                                let response = ui.interact(
                                    button,
                                    ui.id().with(("script-icon", id)),
                                    Sense::click(),
                                );
                                let selected = editor.icon == *id;
                                if selected {
                                    ui.painter().rect(
                                        button,
                                        CornerRadius::same(6),
                                        theme::alpha(theme::primary(), 0.1),
                                        Stroke::new(1.0, theme::alpha(theme::primary(), 0.3)),
                                        StrokeKind::Inside,
                                    );
                                } else {
                                    ui.painter().rect_filled(
                                        button,
                                        CornerRadius::same(6),
                                        if response.hovered() {
                                            theme::alpha(theme::accent(), 0.6)
                                        } else {
                                            theme::overlay(0.035)
                                        },
                                    );
                                }
                                theme::paint_icon(
                                    ui,
                                    icon,
                                    Rect::from_center_size(
                                        Pos2::new(button.center().x, button.top() + 17.0),
                                        Vec2::splat(16.0),
                                    ),
                                    16.0,
                                    theme::foreground(),
                                );
                                let w = widgets::text_width(ui, text, font(12.0, Weight::Regular));
                                widgets::text(
                                    ui,
                                    Pos2::new(button.center().x - w / 2.0, button.top() + 34.0),
                                    text,
                                    font(12.0, Weight::Regular),
                                    theme::foreground(),
                                    16.0,
                                );
                                if response.clicked() {
                                    editor.icon = (*id).to_owned();
                                    editor.icon_picker = false;
                                }
                            }
                        });
                    if ui.input(|i| i.pointer.any_pressed())
                        && !area.response.contains_pointer()
                        && !ui.rect_contains_pointer(icon_button)
                    {
                        editor.icon_picker = false;
                    }
                }
            });

        // The delete confirmation (`AlertDialog`).
        if editor.delete_confirm {
            match self.delete_action_confirm(ui, screen, &editor.name) {
                Some(true) => delete = true,
                Some(false) => editor.delete_confirm = false,
                None => {}
            }
        }
        let escape = ui.input(|i| i.key_pressed(Key::Escape));
        if escape && !editor.recording {
            if editor.icon_picker {
                editor.icon_picker = false;
            } else if editor.delete_confirm {
                editor.delete_confirm = false;
            } else {
                close = true;
            }
        }
        if delete {
            if let Some(id) = editor.script_id.clone() {
                let scripts: Vec<Value> = self
                    .project_scripts(&editor.project_id)
                    .into_iter()
                    .filter(|s| s["id"] != id.as_str())
                    .collect();
                self.save_project_scripts(&editor.project_id, scripts, &id, None);
            }
            return;
        }
        if save {
            match self.submit_action(&editor) {
                Ok(()) => return,
                Err(error) => editor.error = Some(error),
            }
        }
        if !close {
            self.action_editor = Some(editor);
        }
    }

    /// `submit` and `useProjectScriptSettings.submit`: validate, derive the id,
    /// build the script and save the list.
    fn submit_action(&mut self, editor: &ActionEditor) -> Result<(), String> {
        let name = editor.name.trim();
        let command = editor.command.trim();
        if name.is_empty() {
            return Err("Name is required.".into());
        }
        if command.is_empty() {
            return Err("Command is required.".into());
        }
        let current = self.project_scripts(&editor.project_id);
        let settings = &self.state.config["settings"];
        let mut taken: Vec<String> = current
            .iter()
            .chain(
                settings["defaultProjectScripts"]
                    .as_array()
                    .into_iter()
                    .flatten(),
            )
            .chain(
                settings["projectSettingsOverrides"]
                    .as_object()
                    .into_iter()
                    .flat_map(|all| all.values())
                    .flat_map(|entry| {
                        entry["defaultProjectScripts"]
                            .as_array()
                            .into_iter()
                            .flatten()
                    }),
            )
            .filter_map(|s| s["id"].as_str().map(str::to_owned))
            .collect();
        taken.dedup();
        let id = editor
            .script_id
            .clone()
            .unwrap_or_else(|| next_script_id(name, &taken));
        let keybinding = editor.keybinding.trim().to_owned();
        if !keybinding.is_empty() && script_command(&id).is_none() {
            return Err("Invalid keybinding.".into());
        }
        // `buildProjectScript`.
        let preview = editor.preview_url.trim();
        let mut script = json!({
            "id": id,
            "name": name,
            "command": command,
            "icon": editor.icon,
            "runOnWorktreeCreate": editor.run_on_worktree_create,
        });
        if editor.run_on_worktree_create && editor.wait_for_setup {
            script["async"] = json!(false);
        }
        if !preview.is_empty() {
            script["previewUrl"] = json!(preview);
            script["autoOpenPreview"] = json!(editor.auto_open_preview);
        }
        // Only one action runs on worktree creation.
        let mut next: Vec<Value> = current
            .into_iter()
            .map(|existing| {
                if existing["id"] == id.as_str() {
                    script.clone()
                } else if editor.run_on_worktree_create {
                    let mut other = existing;
                    other["runOnWorktreeCreate"] = json!(false);
                    other
                } else {
                    existing
                }
            })
            .collect();
        if editor.script_id.is_none() {
            next.push(script);
        }
        let project = editor.project_id.clone();
        self.save_project_scripts(&project, next, &id, Some(keybinding));
        Ok(())
    }

    /// `Delete action "<name>"?`; returns the choice once made.
    fn delete_action_confirm(&mut self, ui: &mut Ui, screen: Rect, name: &str) -> Option<bool> {
        // `AlertDialogPopup`: the header, then the footer band.
        let width = WIDTH;
        let title = format!("Delete action \"{name}\"?");
        let d = Rect::from_center_size(screen.center(), Vec2::new(width, 163.0));
        let mut choice = None;
        egui::Area::new(ui.id().with("action-delete-confirm"))
            .order(egui::Order::Tooltip)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, _) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 120.0 / 255.0),
                );
                ui.painter().rect(
                    d,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let left = d.left() + 25.0;
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    &title,
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 53.0),
                    "This action cannot be undone.",
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                );
                // `AlertDialogFooter`: a lighter band under a hairline.
                let band = Rect::from_min_max(
                    Pos2::new(d.left() + 1.0, d.top() + 98.0),
                    d.max - Vec2::splat(1.0),
                );
                ui.painter().rect_filled(
                    band,
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 17,
                        se: 17,
                    },
                    theme::overlay(0.016),
                );
                ui.painter().rect_filled(
                    Rect::from_min_size(band.min, Vec2::new(band.width(), 1.0)),
                    0.0,
                    theme::overlay(0.07),
                );
                let footer = d.top() + 114.0;
                let delete = footer_rect(ui, d.right() - 25.0, footer, "Delete action");
                let cancel = footer_rect(ui, delete.left() - 8.0, footer, "Cancel");
                if footer_button(ui, delete, "Delete action", Kind::DestructiveFill) {
                    choice = Some(true);
                }
                if footer_button(ui, cancel, "Cancel", Kind::Outline) {
                    choice = Some(false);
                }
            });
        choice
    }
}

/// `Label`: 14px medium.
fn label(ui: &Ui, pos: Pos2, text: &str) {
    widgets::text(
        ui,
        pos,
        text,
        font(14.0, Weight::Medium),
        theme::foreground(),
        16.0,
    );
}

/// The dialog `Input` (32px) or `Textarea`; the placeholder is painted like
/// the settings fields (egui's hint text renders lighter).
fn input(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    text: &mut String,
    placeholder: &str,
    multiline: bool,
) -> egui::Response {
    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        theme::control_surface(),
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let inner = if multiline {
        Rect::from_min_max(
            rect.min + Vec2::new(12.0, 8.0),
            rect.max - Vec2::new(12.0, 8.0),
        )
    } else {
        Rect::from_min_max(
            Pos2::new(rect.left() + 12.0, rect.top() + 1.0),
            Pos2::new(rect.right() - 12.0, rect.bottom() - 1.0),
        )
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    let edit = if multiline {
        egui::TextEdit::multiline(text).desired_rows(3)
    } else {
        egui::TextEdit::singleline(text).vertical_align(egui::Align::Center)
    };
    let response = child.add(
        edit.id(egui::Id::new(id))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(font(14.0, Weight::Regular))
            .text_color(theme::foreground())
            .desired_width(inner.width())
            .min_size(Vec2::new(
                0.0,
                if multiline { 20.0 } else { inner.height() },
            )),
    );
    if text.is_empty() {
        let line = Rect::from_min_size(
            inner.min,
            Vec2::new(inner.width(), if multiline { 20.0 } else { inner.height() }),
        );
        widgets::text_in(
            ui,
            line,
            placeholder,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
        );
    }
    response
}

/// A switch row (`label` with `bg-white/[0.035]`); dimmed and inert when disabled.
fn toggle_row(ui: &mut Ui, rect: Rect, index: usize, text: &str, on: bool, enabled: bool) -> bool {
    let fade = if enabled { 1.0 } else { 0.6 };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), theme::overlay(0.035 * fade));
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(rect.left() + 13.0, rect.top()), rect.max),
        text,
        font(14.0, Weight::Regular),
        theme::alpha(theme::foreground(), fade),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    if !enabled {
        child.disable();
        child.set_opacity(0.6);
    }
    let clicked = crate::ui::settings::switch(
        &child,
        rect.right() - 13.0,
        rect.center().y,
        on,
        &format!("action-toggle-{index}"),
    );
    clicked && enabled
}

fn footer_rect(ui: &Ui, right: f32, top: f32, label: &str) -> Rect {
    let width = (widgets::text_width(ui, label, font(14.0, Weight::Medium)) + 24.0).ceil();
    Rect::from_min_size(Pos2::new(right - width, top), Vec2::new(width, 32.0))
}

#[derive(Clone, Copy)]
enum Kind {
    Primary,
    Outline,
    Destructive,
    DestructiveFill,
}

/// A dialog footer `Button`.
fn footer_button(ui: &Ui, rect: Rect, label: &str, kind: Kind) -> bool {
    let response = ui.interact(rect, ui.id().with(("action-footer", label)), Sense::click());
    let hovered = response.hovered();
    let color = match kind {
        Kind::Primary => {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(8),
                if hovered {
                    theme::primary_hover()
                } else {
                    theme::primary()
                },
            );
            Color32::WHITE
        }
        Kind::DestructiveFill => {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(8),
                if hovered {
                    theme::alpha(DESTRUCTIVE, 0.9)
                } else {
                    DESTRUCTIVE
                },
            );
            Color32::WHITE
        }
        Kind::Outline | Kind::Destructive => {
            let destructive = matches!(kind, Kind::Destructive);
            ui.painter().rect(
                rect,
                CornerRadius::same(8),
                if hovered {
                    theme::overlay(0.06)
                } else {
                    theme::control_surface()
                },
                Stroke::new(
                    1.0,
                    if destructive {
                        theme::alpha(DESTRUCTIVE, 0.32)
                    } else {
                        theme::input()
                    },
                ),
                StrokeKind::Inside,
            );
            if destructive {
                theme::error_foreground()
            } else {
                theme::foreground()
            }
        }
    };
    let width = widgets::text_width(ui, label, font(14.0, Weight::Medium));
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.center().x - width / 2.0, rect.top()),
            rect.max,
        ),
        label,
        font(14.0, Weight::Medium),
        color,
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_ids_follow_upstream() {
        assert_eq!(next_script_id("Run Tests!", &[]), "run-tests");
        assert_eq!(
            next_script_id("Run Tests", &["run-tests".into()]),
            "run-tests-2"
        );
        assert_eq!(next_script_id("  ", &[]), "script");
        // Cut at 24, then the trailing dash goes.
        assert_eq!(
            next_script_id("a very long action name that keeps going", &[]),
            "a-very-long-action-name"
        );
        assert_eq!(script_command("lint").as_deref(), Some("script.lint.run"));
        assert_eq!(script_command(&"x".repeat(25)), None);
        let bindings =
            json!([{"command": "script.lint.run", "shortcut": {"key": "l", "modKey": true}}]);
        assert_eq!(
            keybinding_for(&bindings, "script.lint.run").as_deref(),
            Some("mod+l")
        );
    }
}
