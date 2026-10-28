//! Global keybindings (`keybindings.ts` `resolveShortcutCommand`, and the
//! handlers in `ChatView.tsx`, `routes/_chat.tsx`, `Sidebar.tsx` and the usage
//! page): the server's resolved bindings, the last match winning, with their
//! when clauses evaluated against this window's state.
//!
//! Handled: the palette modes, Usage and its metric and period keys, new
//! threads, the terminal drawer and its split, new and close, the right
//! panel and diff toggles, closing a surface, thread traversal, jumps and
//! the history (back, forward), settle, pin and copying the thread's
//! reference, steering and editing queued messages, the model picker and
//! the composer's mode, effort, workspace, branch and previous worktree.
//! Not ported yet, they fall through: reopening a closed view, undo,
//! previews' keys, a right-panel terminal's split, new and close, themes,
//! stash and the alternate sends.
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
pub(crate) fn modifiers_match(shortcut: &Value, modifiers: Modifiers) -> bool {
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

/// `shortcutLabelForCommand` (`findEffectiveShortcutForCommand`): the label of
/// the last binding of `command` whose condition holds, unless a later binding
/// claims the same keys.
pub(crate) fn effective_shortcut_label(
    bindings: &Value,
    command: &str,
    context: &[(&str, bool)],
) -> Option<String> {
    effective_shortcut(bindings, command, context)
        .map(|shortcut| crate::ui::action_editor::format_shortcut(&shortcut))
}

/// `findEffectiveShortcutForCommand`: the shortcut behind
/// [`effective_shortcut_label`].
pub(crate) fn effective_shortcut(
    bindings: &Value,
    command: &str,
    context: &[(&str, bool)],
) -> Option<Value> {
    let mut claimed = std::collections::HashSet::new();
    for binding in bindings.as_array()?.iter().rev() {
        if !when_matches(&binding["whenAst"], context) {
            continue;
        }
        let shortcut = &binding["shortcut"];
        let flag = |name: &str| shortcut[name] == true;
        let conflict = (
            shortcut["key"].as_str().unwrap_or_default().to_owned(),
            flag("metaKey"),
            flag("ctrlKey") || flag("modKey"),
            flag("shiftKey"),
            flag("altKey"),
        );
        if !claimed.insert(conflict) {
            continue;
        }
        if binding["command"] == command {
            return Some(shortcut.clone());
        }
    }
    None
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
        let thread = self.current_thread();
        let terminal_open = self
            .selected
            .as_ref()
            .and_then(|id| self.drawers.get(id))
            .is_some_and(|drawer| drawer.open);
        let panel_open = thread.is_some() && self.panel_open();
        let preview_open =
            panel_open && matches!(self.panel().active_tab(), Some(PanelTab::Browser { .. }));
        let context = [
            ("terminalFocus", terminal_focus),
            ("terminalOpen", terminal_open),
            ("previewFocus", preview_open && editable),
            ("previewOpen", preview_open),
            ("rightPanelOpen", panel_open),
            (
                "turnRunning",
                thread.as_ref().is_some_and(|t| t.is_running()),
            ),
            ("draftThreadRoute", self.selected.is_none()),
            ("isWeb", false),
            ("isDesktop", true),
            ("editableFocus", editable),
            ("modelPickerOpen", model_picker),
            ("usagePageOpen", self.usage_open),
            (
                "composerFocus",
                self.cite
                    .editor_id
                    .is_some_and(|id| ui.memory(|m| m.has_focus(id))),
            ),
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
            let handled = match command.as_str() {
                "thread.steerQueuedMessage" => self.steer_next_queued(),
                "thread.editQueuedMessage" => self.edit_latest_queued(ui.ctx()),
                _ => self.run_command(&command),
            };
            if handled {
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
            // Drafts and servers without thread search leave Mod+F alone.
            "chat.find" if thread.is_some() && self.state.config["threadFind"] == true => {
                self.open_thread_find()
            }
            "sidebar.toggle" => self.toggle_sidebar(),
            "threadPanel.toggle" if self.thread_details_available() => {
                use crate::ui::thread_details::MENU;
                let open = self.open_menu.as_deref() == Some(MENU);
                self.open_menu = (!open).then(|| MENU.to_owned());
            }
            "editor.openFavorite" => return self.open_in_favorite(),
            "chat.newWithoutProject" if self.scratch_root().is_some() => {
                self.start_scratch_thread()
            }
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
            "navigation.back" | "navigation.forward" => {
                let back = command == "navigation.back";
                let (from, to) = if back {
                    (&mut self.nav_back, &mut self.nav_forward)
                } else {
                    (&mut self.nav_forward, &mut self.nav_back)
                };
                let Some(target) = from.pop() else {
                    return false;
                };
                to.push(self.selected.clone());
                self.navigating = true;
                self.select_thread(target);
                self.navigating = false;
            }
            "thread.copyReference" => {
                // `resolveThreadReferenceCopyTarget`: the thread's pull
                // request link, else its id.
                let Some(thread) = thread else {
                    return false;
                };
                let link = crate::ui::thread_pull_requests::current_link(&thread.pull_requests)
                    .and_then(|l| l["url"].as_str())
                    .map(str::to_owned)
                    .or_else(|| {
                        thread
                            .linked_pull_request
                            .as_ref()
                            .or(thread.branch_pull_request.as_ref())
                            .and_then(|pr| pr["url"].as_str().map(str::to_owned))
                    });
                let (value, title) = match link {
                    Some(url) => (url, "PR link copied"),
                    None => (thread.id.clone(), "Thread ID copied"),
                };
                if let Some(ctx) = &self.egui_ctx {
                    ctx.copy_text(value.clone());
                }
                self.toasts.add(
                    crate::ui::toasts::Toast::new(crate::ui::toasts::Kind::Success, title)
                        .description(value),
                );
            }
            "modelPicker.toggle" => self.toggle_menu("model-picker"),
            "composer.mode" | "composer.effort" => {
                let menu = if command == "composer.mode" {
                    self.composer_controls.0
                } else {
                    self.composer_controls.1
                };
                let Some(menu) = menu else {
                    return false;
                };
                self.toggle_menu(menu);
            }
            "composer.workspace" if self.selected.is_none() => {
                self.toggle_menu("composer-workspace")
            }
            "composer.branch" => self.toggle_menu(crate::ui::branch_picker::MENU),
            "composer.previousWorktree" if self.selected.is_none() => {
                let current = self.draft_workspace().and_then(|w| w.worktree.clone());
                let Some((branch, path)) = self.draft_project().and_then(|project| {
                    crate::ui::branch_picker::previous_worktree(
                        &self.state.threads,
                        &project.id,
                        current.as_deref(),
                    )
                }) else {
                    return false;
                };
                self.use_previous_worktree(branch, path);
            }
            "terminal.split" | "terminal.splitVertical" | "terminal.new" | "terminal.close"
                if thread.is_some() =>
            {
                return self.terminal_command(command);
            }
            "modelPicker.previousProvider" => self.picker_cycle = Some(-1),
            "modelPicker.nextProvider" => self.picker_cycle = Some(1),
            jump if jump.starts_with("modelPicker.jump.") => {
                let index: usize = jump["modelPicker.jump.".len()..].parse().unwrap_or(0);
                self.picker_jump = Some(index);
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

    /// Opens a menu, or closes it when it is the open one.
    fn toggle_menu(&mut self, menu: &str) {
        self.open_menu = if self.open_menu.as_deref() == Some(menu) {
            None
        } else {
            Some(menu.to_owned())
        };
    }

    /// `orderedThreads`: pinned, active, then the shelves as rendered.
    pub(crate) fn ordered_sidebar_threads(&self) -> Vec<String> {
        if self.client_setting("legacySidebarEnabled") == true {
            return self.legacy_visible_thread_ids();
        }
        let query = self.search.trim().to_lowercase();
        let scope = self.scoped_project().map(|p| p.id.clone());
        let threads: Vec<std::sync::Arc<crate::model::Thread>> = self
            .state
            .threads
            .iter()
            .filter(|t| query.is_empty() || t.title.to_lowercase().contains(&query))
            .filter(|t| scope.as_ref().is_none_or(|id| &t.project_id == id))
            .cloned()
            .collect();
        let sections =
            crate::ui::sidebar::sections(threads.iter().map(std::sync::Arc::as_ref), self.now);
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
