//! The chat header's project actions (`components/ProjectScriptsControl.tsx`,
//! toolbar presentation) and running one (`runProjectScript` in
//! `components/ChatView.tsx`).
//!
//! With actions, the header shows the preferred one (the last run, else the
//! first that is not a setup action) and a menu of all of them; running opens
//! the bottom terminal drawer and types the command into its active terminal
//! (queued until that terminal has attached). Not ported: opening a second
//! terminal when the active one is busy, and the script environment
//! variables (`projectScriptRuntimeEnv`).

use eframe::egui::{CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::Value;

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::action_editor::{script_command, script_icon, shortcut_label_for};
use crate::ui::widgets;

const MENU_ID: &str = "project-actions-menu";

/// `primaryProjectScript` with the preferred (last run) action first.
fn primary_script<'a>(scripts: &'a [Value], preferred: Option<&str>) -> Option<&'a Value> {
    preferred
        .and_then(|id| scripts.iter().find(|s| s["id"] == id))
        .or_else(|| scripts.iter().find(|s| s["runOnWorktreeCreate"] != true))
        .or(scripts.first())
}

impl App {
    /// The header control from `right`; returns its left edge. With no
    /// actions it is the "Add action" button.
    pub(crate) fn project_actions_control(
        &mut self,
        ui: &mut Ui,
        right: f32,
        project_id: &str,
        compact: bool,
    ) -> f32 {
        let scripts = self.project_scripts(project_id);
        let preferred = self.last_script.get(project_id).cloned();
        let Some(primary) = primary_script(&scripts, preferred.as_deref()).cloned() else {
            let label = if compact { "" } else { "Add action" };
            let width = if compact {
                24.0
            } else {
                widgets::outline_button_width(ui, true, label)
            };
            let button =
                Rect::from_min_size(Pos2::new(right - width, 14.0), Vec2::new(width, 24.0));
            if widgets::outline_button(
                ui,
                button,
                "add-action",
                Some(("plus", theme::muted_foreground())),
                label,
                CornerRadius::same(8),
            )
            .clicked()
            {
                self.open_action_editor(project_id, None);
            }
            return button.left();
        };

        // `Group`: run the primary action | the actions menu.
        let chevron = Rect::from_min_size(Pos2::new(right - 24.0, 14.0), Vec2::splat(24.0));
        let name = primary["name"].as_str().unwrap_or_default().to_owned();
        let label = if compact { String::new() } else { name.clone() };
        let width = if compact {
            25.0
        } else {
            widgets::outline_button_width(ui, true, &label)
        };
        let main = Rect::from_min_size(
            Pos2::new(chevron.left() - 1.0 - width, 14.0),
            Vec2::new(width, 24.0),
        );
        let run = widgets::outline_button(
            ui,
            main,
            ("project-action", "run"),
            Some((
                script_icon(primary["icon"].as_str().unwrap_or("play")),
                theme::muted_foreground(),
            )),
            &label,
            CornerRadius {
                nw: 8,
                sw: 8,
                ne: 0,
                se: 0,
            },
        )
        .on_hover_text(format!("Run {name}"));
        let menu = widgets::outline_button(
            ui,
            chevron,
            ("project-action", "menu"),
            None,
            "",
            CornerRadius {
                nw: 0,
                sw: 0,
                ne: 8,
                se: 8,
            },
        );
        theme::paint_icon(ui, "chevron-down", chevron, 16.0, theme::muted_foreground());
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(main.right(), 14.0), Vec2::new(1.0, 24.0)),
            0.0,
            theme::input(),
        );
        if run.clicked() {
            self.run_project_script(project_id, &primary);
        }
        if menu.clicked() {
            self.open_menu = if self.open_menu.as_deref() == Some(MENU_ID) {
                None
            } else {
                Some(MENU_ID.to_owned())
            };
        }
        self.project_actions_menu(ui, chevron, project_id, &scripts);
        main.left()
    }

    /// The actions menu: each action (its shortcut, or the edit gear while
    /// hovered), then "Add action".
    fn project_actions_menu(
        &mut self,
        ui: &mut Ui,
        anchor: Rect,
        project_id: &str,
        scripts: &[Value],
    ) {
        if self.open_menu.as_deref() != Some(MENU_ID) {
            return;
        }
        let label_font = font(14.0, Weight::Regular);
        let labels: Vec<String> = scripts
            .iter()
            .map(|s| {
                let name = s["name"].as_str().unwrap_or_default();
                if s["runOnWorktreeCreate"] == true {
                    format!("{name} (setup)")
                } else {
                    name.to_owned()
                }
            })
            .collect();
        let bindings = self.state.config["keybindings"].clone();
        let shortcuts: Vec<Option<String>> = scripts
            .iter()
            .map(|s| {
                script_command(s["id"].as_str().unwrap_or_default())
                    .and_then(|c| shortcut_label_for(&bindings, &c))
            })
            .collect();
        let content = labels
            .iter()
            .zip(&shortcuts)
            .map(|(label, shortcut)| {
                widgets::text_width(ui, label, label_font.clone())
                    + 8.0
                    + shortcut.as_ref().map_or(24.0, |s| {
                        widgets::text_width(ui, s, font(12.0, Weight::Medium)).max(24.0)
                    })
            })
            .chain(std::iter::once(widgets::text_width(
                ui,
                "Add action",
                label_font.clone(),
            )))
            .fold(0.0, f32::max);
        let width = (6.0 + 16.0 + 6.0 + content + 8.0 + 10.0).max(160.0).ceil();
        let height = 2.0 + 8.0 + scripts.len() as f32 * 32.0 + 28.0;
        let rect = Rect::from_min_size(
            Pos2::new(anchor.right() - width, anchor.bottom() + 4.0),
            Vec2::new(width, height),
        );
        let mut run: Option<usize> = None;
        let mut edit: Option<usize> = None;
        let mut add = false;
        let mut open = self.open_menu.clone();
        widgets::popup(ui, &mut open, MENU_ID, rect, anchor, |ui, inner| {
            let mut y = inner.top() + 4.0;
            for (index, script) in scripts.iter().enumerate() {
                let item = Rect::from_min_size(
                    Pos2::new(inner.left() + 4.0, y),
                    Vec2::new(inner.width() - 8.0, 32.0),
                );
                let response =
                    ui.interact(item, ui.id().with(("action-item", index)), Sense::click());
                let hovered = ui.rect_contains_pointer(item);
                if hovered {
                    ui.painter()
                        .rect_filled(item, CornerRadius::same(6), theme::accent());
                }
                theme::paint_icon(
                    ui,
                    script_icon(script["icon"].as_str().unwrap_or("play")),
                    Rect::from_min_size(
                        Pos2::new(item.left() + 6.0, item.center().y - 8.0),
                        Vec2::splat(16.0),
                    ),
                    16.0,
                    theme::alpha(theme::foreground(), 0.8),
                );
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(item.left() + 28.0, item.top()),
                        Pos2::new(item.right() - 40.0, item.bottom()),
                    ),
                    &labels[index],
                    label_font.clone(),
                    theme::foreground(),
                );
                // The shortcut yields its slot to the edit button on hover.
                let gear = Rect::from_min_size(
                    Pos2::new(item.right() - 8.0 - 24.0, item.center().y - 12.0),
                    Vec2::splat(24.0),
                );
                let mut gear_clicked = false;
                if hovered {
                    gear_clicked = widgets::icon_button(
                        ui,
                        gear,
                        ("action-item-edit", index),
                        "settings",
                        14.0,
                        theme::muted_foreground(),
                        "",
                    )
                    .clicked();
                } else if let Some(shortcut) = &shortcuts[index] {
                    let w = widgets::text_width(ui, shortcut, font(12.0, Weight::Medium));
                    widgets::text(
                        ui,
                        Pos2::new(gear.right() - w, item.center().y - 8.0),
                        shortcut,
                        font(12.0, Weight::Medium),
                        theme::muted_foreground(),
                        16.0,
                    );
                }
                if gear_clicked {
                    edit = Some(index);
                } else if response.clicked() {
                    run = Some(index);
                }
                y += 32.0;
            }
            let item = Rect::from_min_size(
                Pos2::new(inner.left() + 4.0, y),
                Vec2::new(inner.width() - 8.0, 28.0),
            );
            let response = ui.interact(item, ui.id().with("action-item-add"), Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(item, CornerRadius::same(6), theme::accent());
            }
            theme::paint_icon(
                ui,
                "plus",
                Rect::from_min_size(
                    Pos2::new(item.left() + 6.0, item.center().y - 8.0),
                    Vec2::splat(16.0),
                ),
                16.0,
                theme::alpha(theme::foreground(), 0.8),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(item.left() + 28.0, item.top()), item.max),
                "Add action",
                label_font.clone(),
                theme::foreground(),
            );
            add = response.clicked();
        });
        self.open_menu = open;
        if run.is_some() || edit.is_some() || add {
            self.open_menu = None;
        }
        if let Some(index) = run {
            self.run_project_script(project_id, &scripts[index]);
        }
        if let Some(index) = edit {
            self.open_action_editor(project_id, Some(&scripts[index]));
        }
        if add {
            self.open_action_editor(project_id, None);
        }
    }

    /// `runProjectScript`: remember it, open the drawer, and type the command
    /// into the active terminal.
    pub(crate) fn run_project_script(&mut self, project_id: &str, script: &Value) {
        let Some(thread) = self.selected.clone() else {
            return;
        };
        if let Some(id) = script["id"].as_str() {
            self.last_script
                .insert(project_id.to_owned(), id.to_owned());
        }
        let terminal = self.open_terminal_drawer(&thread);
        let command = script["command"].as_str().unwrap_or_default();
        self.queue_terminal_input(&thread, &terminal, format!("{command}\r"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_primary_action_follows_upstream() {
        let scripts = vec![
            json!({"id": "setup", "runOnWorktreeCreate": true}),
            json!({"id": "test", "runOnWorktreeCreate": false}),
        ];
        assert_eq!(primary_script(&scripts, None).unwrap()["id"], "test");
        assert_eq!(
            primary_script(&scripts, Some("setup")).unwrap()["id"],
            "setup"
        );
        assert_eq!(primary_script(&scripts[..1], None).unwrap()["id"], "setup");
    }
}
