//! Global keybindings (`keybindings.ts` `resolveShortcutCommand`, and the
//! handlers in `ChatView.tsx`, `routes/_chat.tsx`, `Sidebar.tsx` and the usage
//! page): the server's resolved bindings, the last match winning, with their
//! when clauses evaluated against this window's state.
//!
//! Handled: the palette modes, Usage and its metric and period keys, new
//! threads, the terminal drawer, the right panel and diff toggles, closing a surface, thread
//! traversal and jumps, settle and pin. Other commands (the terminal drawer,
//! previews, composer controls, themes) are not ported and fall through.
use eframe::egui::{self, Key, Modifiers, Ui};
use serde_json::{Value, json};

use crate::app::{App, PanelTab};
use crate::ui::palette::View;

/// The shortcut `key` a press produces (`resolveEventKeys`).
fn event_key(key: Key) -> Option<String> {
    match crate::ui::keybindings::binding_from_key(key, Modifiers::NONE)?.as_str() {
        "space" => Some(" ".into()),
        "esc" => Some("escape".into()),
        token => Some(token.to_owned()),
    }
}

/// `matchesShortcutModifiers` off macOS: `mod` is Ctrl and nothing maps to Meta.
fn modifiers_match(shortcut: &Value, modifiers: Modifiers) -> bool {
    let flag = |name: &str| shortcut[name] == true;
    !flag("metaKey")
        && modifiers.ctrl == (flag("ctrlKey") || flag("modKey"))
        && modifiers.shift == flag("shiftKey")
        && modifiers.alt == flag("altKey")
}

/// `evaluateWhenNode`: unknown variables are false.
fn when_matches(node: &Value, context: &[(&str, bool)]) -> bool {
    match node["type"].as_str() {
        None => true,
        Some("identifier") => match node["name"].as_str().unwrap_or_default() {
            "true" => true,
            "false" => false,
            name => context.iter().any(|(n, on)| *n == name && *on),
        },
        Some("not") => !when_matches(&node["node"], context),
        Some("and") => {
            when_matches(&node["left"], context) && when_matches(&node["right"], context)
        }
        Some("or") => when_matches(&node["left"], context) || when_matches(&node["right"], context),
        Some(_) => false,
    }
}

/// `resolveShortcutCommand`: the last binding whose condition holds and
/// whose shortcut matches the press.
fn resolve(
    bindings: &Value,
    key: &str,
    modifiers: Modifiers,
    context: &[(&str, bool)],
) -> Option<String> {
    bindings.as_array()?.iter().rev().find_map(|binding| {
        (when_matches(&binding["whenAst"], context)
            && binding["shortcut"]["key"].as_str() == Some(key)
            && modifiers_match(&binding["shortcut"], modifiers))
        .then(|| binding["command"].as_str().unwrap_or_default().to_owned())
    })
}

impl App {
    /// Runs the command bound to a key pressed this frame, consuming it.
    pub(crate) fn run_shortcuts(&mut self, ui: &Ui) {
        let presses: Vec<(Key, Modifiers)> = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
                .collect()
        });
        if presses.is_empty() {
            return;
        }
        let terminal_focus = self.terminal_focused();
        let editable = ui.memory(|m| m.focused().is_some()) && !terminal_focus;
        let model_picker = self
            .open_menu
            .as_deref()
            .is_some_and(|menu| menu.contains("model"));
        let context = [
            ("terminalFocus", terminal_focus),
            ("terminalOpen", false),
            ("previewFocus", false),
            ("previewOpen", false),
            ("isWeb", false),
            ("isDesktop", true),
            ("editableFocus", editable),
            ("modelPickerOpen", model_picker),
            ("usagePageOpen", self.usage_open),
        ];
        for (key, modifiers) in presses {
            let Some(token) = event_key(key) else {
                continue;
            };
            // Plain keys only count where a binding asks for them (the usage
            // page's c/t/l) and never while typing.
            if !modifiers.ctrl && !modifiers.alt && editable {
                continue;
            }
            let Some(command) = resolve(
                &self.state.config["keybindings"],
                &token,
                modifiers,
                &context,
            ) else {
                continue;
            };
            if self.run_command(&command) {
                ui.ctx().input_mut(|i| i.consume_key(modifiers, key));
            }
        }
    }

    /// Runs a keybinding command; returns whether it was handled.
    fn run_command(&mut self, command: &str) -> bool {
        // The open palette keeps only the commands that switch its mode.
        let palette_command = matches!(
            command,
            "commandPalette.toggle" | "filePicker.toggle" | "projectSearch.toggle" | "usage.open"
        );
        if self.palette.open && !palette_command {
            return false;
        }
        let thread = self.current_thread();
        match command {
            "commandPalette.toggle" => self.toggle_palette(View::Root),
            "filePicker.toggle" => self.toggle_palette(View::Files),
            "projectSearch.toggle" => self.toggle_palette(View::Content),
            "usage.open" => {
                self.palette = Default::default();
                self.open_usage();
            }
            // With several projects the new-thread shortcut asks where.
            "chat.new" if self.state.projects.len() > 1 => self.toggle_palette(View::NewThreadIn),
            "chat.new" | "chat.newLocal" => self.start_draft(None),
            "terminal.toggle" if thread.is_some() => self.toggle_terminal_drawer(),
            "rightPanel.toggle" if thread.is_some() => {
                if let Some(panel) = self.panel_mut() {
                    panel.open = !panel.open;
                }
            }
            "rightPanel.close" => {
                let Some(panel) = self.panel_mut().filter(|p| p.open && !p.tabs.is_empty()) else {
                    return false;
                };
                let index = panel.active.min(panel.tabs.len() - 1);
                panel.close(index);
                if panel.tabs.is_empty() {
                    panel.open = false;
                }
            }
            "diff.toggle" if thread.is_some() => {
                if let Some(panel) = self.panel_mut() {
                    if panel.open && panel.active_tab() == Some(&PanelTab::Diff) {
                        panel.open = false;
                    } else {
                        panel.open = true;
                        panel.show(PanelTab::Diff, |t| *t == PanelTab::Diff);
                    }
                }
            }
            "thread.previous" | "thread.next" => {
                let ids = self.ordered_sidebar_threads();
                let target = match self.selected.as_ref() {
                    None if command == "thread.previous" => ids.last().cloned(),
                    None => ids.first().cloned(),
                    Some(current) => {
                        let Some(index) = ids.iter().position(|id| id == current) else {
                            return false;
                        };
                        if command == "thread.previous" {
                            index.checked_sub(1).and_then(|i| ids.get(i)).cloned()
                        } else {
                            ids.get(index + 1).cloned()
                        }
                    }
                };
                let Some(target) = target else {
                    return false;
                };
                self.select_thread(Some(target));
            }
            jump if jump.starts_with("thread.jump.") => {
                let index: usize = jump["thread.jump.".len()..].parse().unwrap_or(0);
                let Some(target) = self
                    .ordered_sidebar_threads()
                    .get(index.wrapping_sub(1))
                    .cloned()
                else {
                    return false;
                };
                self.select_thread(Some(target));
            }
            "thread.settle" if self.capability("threadSettlement") => {
                let Some(thread) = thread else {
                    return false;
                };
                if thread.settled_override.as_deref() == Some("settled") {
                    let mut cmd = crate::model::command("thread.unsettle", &thread.id);
                    cmd["reason"] = json!("user");
                    self.dispatch(cmd);
                } else {
                    self.dispatch(crate::model::command("thread.settle", &thread.id));
                }
            }
            "thread.pin" if self.capability("threadPinning") => {
                let Some(thread) = thread else {
                    return false;
                };
                let kind = if thread.pinned_at.is_some() {
                    "thread.unpin"
                } else {
                    "thread.pin"
                };
                self.dispatch(crate::model::command(kind, &thread.id));
            }
            "usage.cost" | "usage.tokens" | "usage.limits" if self.usage_open => {
                let metric = ["usage.cost", "usage.tokens", "usage.limits"]
                    .iter()
                    .position(|c| *c == command)
                    .unwrap_or(0);
                self.usage.metric = metric;
                if metric == 2 {
                    self.usage.now = chrono::Utc::now();
                    self.refresh_limits(true);
                }
            }
            period if period.starts_with("usage.period.") && self.usage_open => {
                let Some(index) = ["day", "week", "month", "quarter"]
                    .iter()
                    .position(|p| period.ends_with(p))
                else {
                    return false;
                };
                if self.usage.metric == 2 {
                    return false;
                }
                self.usage.period = index;
                self.usage.window = None;
            }
            _ => return false,
        }
        true
    }

    /// `orderedThreads`: pinned, active, then the shelves as rendered.
    pub(crate) fn ordered_sidebar_threads(&self) -> Vec<String> {
        let query = self.search.trim().to_lowercase();
        let scope = self.scoped_project().map(|p| p.id.clone());
        let threads: Vec<crate::model::Thread> = self
            .state
            .threads
            .iter()
            .filter(|t| query.is_empty() || t.title.to_lowercase().contains(&query))
            .filter(|t| scope.as_ref().is_none_or(|id| &t.project_id == id))
            .cloned()
            .collect();
        let sections = crate::ui::sidebar::sections(&threads, self.now);
        let snoozed = sections
            .snoozed
            .iter()
            .filter(|t| self.snoozed_expanded || self.selected.as_deref() == Some(t.id.as_str()));
        let settled = sections.settled.iter().filter(|_| self.settled_expanded);
        sections
            .pinned
            .iter()
            .chain(sections.active.iter())
            .chain(snoozed)
            .chain(settled)
            .map(|t| t.id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_matching_binding_wins_under_its_condition() {
        let bindings = json!([
            {"command": "diff.toggle", "shortcut": {"key": "d", "modKey": true},
             "whenAst": {"type": "not", "node": {"type": "identifier", "name": "terminalFocus"}}},
            {"command": "terminal.split", "shortcut": {"key": "d", "modKey": true},
             "whenAst": {"type": "identifier", "name": "terminalFocus"}},
        ]);
        let ctrl = Modifiers {
            ctrl: true,
            command: true,
            ..Modifiers::NONE
        };
        let typing = [("terminalFocus", false)];
        let terminal = [("terminalFocus", true)];
        assert_eq!(
            resolve(&bindings, "d", ctrl, &typing).as_deref(),
            Some("diff.toggle")
        );
        assert_eq!(
            resolve(&bindings, "d", ctrl, &terminal).as_deref(),
            Some("terminal.split")
        );
        assert_eq!(resolve(&bindings, "d", Modifiers::NONE, &typing), None);
        assert_eq!(event_key(Key::Space).as_deref(), Some(" "));
    }
}
