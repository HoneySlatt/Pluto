//! Settings search (`settings/settingsSearch.ts`, `SettingsSidebarNav.tsx`):
//! the sidebar field filters the settings catalog; a result opens its page,
//! centers its row and pulses it (`settings-search-target-pulse`).
//!
//! The catalog drops results this client cannot reach: macOS, Windows, WSL,
//! cloud-only rows and the project overview (no project scope here). Results
//! whose row is not ported open their page only.
use eframe::egui::{
    self, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::settings::{PAGES, command_label};
use crate::ui::widgets;

const PULSE: f64 = 0.65;

/// One searchable setting and where it lives.
#[derive(Clone)]
pub(crate) struct Item {
    id: &'static str,
    title: &'static str,
    page: usize,
    /// The row (or section) the result scrolls to.
    target: &'static str,
    terms: &'static str,
    secondary: bool,
    auto_settlement: bool,
}

/// The pending scroll target and when its pulse started.
pub(crate) struct Target {
    pub id: String,
    pub pulse_from: Option<f64>,
}

/// `STATIC_KEYBINDING_COMMANDS`.
pub(crate) fn keybinding_commands() -> Vec<&'static str> {
    KEYBINDINGS.iter().map(|(command, _)| *command).collect()
}

/// `normalizeSearchText`, without the diacritic folding (the catalog is ASCII).
fn normalize(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The catalog in result order, keybinding commands alphabetical by label.
fn catalog() -> Vec<(Item, String)> {
    let mut keybindings: Vec<(Item, String)> = KEYBINDINGS
        .iter()
        .map(|(command, keys)| {
            let title = command_label(command);
            let terms = std::iter::once(*command)
                .chain(keys.iter().copied())
                .collect::<Vec<_>>()
                .join(" ");
            let item = Item {
                id: command,
                title: "",
                page: 2,
                // A command without a default binding may have no row.
                target: if keys.is_empty() {
                    "keybindings"
                } else {
                    command
                },
                terms: "",
                secondary: true,
                auto_settlement: false,
            };
            (item, format!("{title}\0{terms}"))
        })
        .collect();
    keybindings.sort_by_key(|(_, label)| label.to_lowercase());
    BEFORE
        .iter()
        .map(|item| (item.clone(), format!("{}\0{}", item.title, item.terms)))
        .chain(keybindings)
        .chain(
            AFTER
                .iter()
                .map(|item| (item.clone(), format!("{}\0{}", item.title, item.terms))),
        )
        .collect()
}

/// A search result.
pub(crate) struct Found {
    pub title: String,
    pub page: usize,
    pub target: String,
    /// The search fields after the title (section, terms), for the palette.
    pub fields: Vec<String>,
    pub secondary: bool,
}

/// `searchSettings`: every token in some field, ranked by how the title
/// matches, secondary results last.
pub(crate) fn search(query: &str, auto_settlement: bool) -> Vec<Found> {
    let query = normalize(query);
    if query.is_empty() {
        return Vec::new();
    }
    let tokens: Vec<&str> = query.split(' ').collect();
    let mut ranked: Vec<(bool, u8, usize, Found)> = catalog()
        .into_iter()
        .enumerate()
        .filter(|(_, (item, _))| auto_settlement || !item.auto_settlement)
        .filter_map(|(index, (item, text))| {
            let (title, terms) = text.split_once('\0').unwrap_or((&text, ""));
            let title_text = title.to_owned();
            let title = normalize(title);
            let fields = [
                title.clone(),
                normalize(PAGES[item.page].0),
                normalize(terms),
            ];
            if !tokens.iter().all(|t| fields.iter().any(|f| f.contains(t))) {
                return None;
            }
            let rank = if title == query {
                5
            } else if title.starts_with(&query) {
                4
            } else if title.contains(&query) {
                3
            } else if tokens.iter().all(|t| title.contains(t)) {
                2
            } else if fields.iter().any(|f| f.contains(&query)) {
                1
            } else {
                0
            };
            let target = if item.target == item.id && item.secondary {
                format!("keybinding-{}", item.id)
            } else {
                item.target.to_owned()
            };
            let found = Found {
                title: title_text,
                page: item.page,
                target,
                fields: fields[1..].to_vec(),
                secondary: item.secondary,
            };
            Some((item.secondary, rank, index, found))
        })
        .collect();
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    ranked.into_iter().map(|(_, _, _, found)| found).collect()
}

impl App {
    pub(crate) fn auto_settlement_available(&self) -> bool {
        self.state.config["environment"]["capabilities"]["threadAutoSettlement"] == true
    }

    /// The sidebar field: Search with its "/" key, or the clear button.
    pub(crate) fn settings_search_field(&mut self, ui: &mut Ui, left: f32) {
        let row = Rect::from_min_size(Pos2::new(left + 8.0, 60.0), Vec2::new(239.0, 32.0));
        let hovered = ui.rect_contains_pointer(row);
        if hovered {
            ui.painter()
                .rect_filled(row, CornerRadius::same(8), theme::accent());
        }
        theme::paint_icon(
            ui,
            "search",
            Rect::from_min_size(Pos2::new(left + 16.0, 68.0), Vec2::splat(16.0)),
            16.0,
            theme::alpha(theme::sidebar_muted(), 0.8),
        );
        let input_rect = Rect::from_min_size(Pos2::new(left + 40.0, 66.5), Vec2::new(170.0, 21.0));
        let mut search_ui = ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
        let before = self.settings_search.clone();
        let response = search_ui.add(
            egui::TextEdit::singleline(&mut self.settings_search)
                .id(egui::Id::new("settings-search"))
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font(14.0, Weight::Medium))
                .text_color(theme::sidebar_foreground())
                .hint_text(
                    egui::RichText::new("Search")
                        .font(font(14.0, Weight::Medium))
                        .color(theme::sidebar_muted()),
                )
                .desired_width(170.0),
        );
        if self.settings_search != before {
            self.settings_search_active = 0;
        }
        // "/" focuses the field unless another text field has the keyboard.
        if !response.has_focus()
            && ui.memory(|m| m.focused().is_none())
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Slash))
        {
            response.request_focus();
        }
        let searching = !self.settings_search.trim().is_empty();
        if searching {
            let clear = Rect::from_min_size(Pos2::new(left + 219.0, 66.0), Vec2::splat(20.0));
            let clear_response =
                ui.interact(clear, ui.id().with("settings-search-clear"), Sense::click());
            if clear_response.hovered() {
                ui.painter()
                    .rect_filled(clear, CornerRadius::same(6), theme::accent());
            }
            theme::paint_icon(ui, "x", clear, 12.0, theme::sidebar_muted());
            if clear_response.clicked() {
                self.settings_search.clear();
                response.request_focus();
            }
        } else {
            let kbd = Rect::from_min_size(Pos2::new(left + 219.0, 66.0), Vec2::splat(20.0));
            ui.painter()
                .rect_filled(kbd, CornerRadius::same(4), theme::background());
            let key_width = widgets::text_width(ui, "/", font(12.0, Weight::Medium));
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(kbd.center().x - key_width / 2.0, kbd.top()),
                    kbd.max,
                ),
                "/",
                font(12.0, Weight::Medium),
                theme::sidebar_muted(),
            );
        }
        if (response.has_focus() || response.lost_focus()) && searching {
            let results = search(&self.settings_search, self.auto_settlement_available());
            let (up, down, enter, escape) = ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    i.key_pressed(Key::Enter),
                    i.consume_key(Modifiers::NONE, Key::Escape),
                )
            });
            let count = results.len();
            if escape {
                self.settings_search.clear();
            } else if count > 0 {
                if down {
                    self.settings_search_active = (self.settings_search_active + 1) % count;
                }
                if up {
                    self.settings_search_active = (self.settings_search_active + count - 1) % count;
                }
                if enter && let Some(found) = results.get(self.settings_search_active) {
                    self.open_settings_result(found.page, found.target.clone());
                }
            }
        }
    }

    /// The results list that replaces the page nav while searching; returns
    /// whether it was shown.
    pub(crate) fn settings_search_results(&mut self, ui: &mut Ui, left: f32) -> bool {
        if self.settings_search.trim().is_empty() {
            return false;
        }
        let results = search(&self.settings_search, self.auto_settlement_available());
        if results.is_empty() {
            let message = "No settings found";
            let width = widgets::text_width(ui, message, font(12.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_min_size(
                    Pos2::new(left + 8.0 + (239.0 - width) / 2.0, 100.0 + 24.0),
                    Vec2::new(width + 1.0, 16.0),
                ),
                message,
                font(12.0, Weight::Regular),
                theme::sidebar_muted(),
            );
            return true;
        }
        self.settings_search_active = self.settings_search_active.min(results.len() - 1);
        let list = Rect::from_min_max(
            Pos2::new(left, 100.0),
            Pos2::new(left + 255.0, ui.max_rect().bottom() - 48.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        child.set_clip_rect(list.intersect(ui.clip_rect()));
        let mut open = None;
        let mut hovered = None;
        let active = self.settings_search_active;
        let moved = ui.input(|i| i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::ArrowDown));
        egui::ScrollArea::vertical()
            .id_salt("settings-search-results")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let origin = ui.max_rect().min;
                for (index, found) in results.iter().enumerate() {
                    let (title, page, target) = (&found.title, &found.page, &found.target);
                    let row = Rect::from_min_size(
                        Pos2::new(left + 8.0, origin.y + index as f32 * 56.0),
                        Vec2::new(239.0, 52.0),
                    );
                    let response = ui.interact(
                        row,
                        ui.id().with(("settings-result", index)),
                        Sense::click(),
                    );
                    if response.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                        hovered = Some(index);
                    }
                    let is_active = index == active;
                    if is_active {
                        ui.painter().rect_filled(
                            row,
                            CornerRadius::same(10),
                            theme::alpha(theme::sidebar_foreground(), 0.07),
                        );
                        if moved {
                            ui.scroll_to_rect(row, None);
                        }
                    }
                    let icon_color = if is_active {
                        theme::sidebar_foreground()
                    } else {
                        theme::alpha(theme::sidebar_muted(), 0.6)
                    };
                    theme::paint_icon(
                        ui,
                        PAGES[*page].1,
                        Rect::from_min_size(
                            Pos2::new(row.left() + 8.0, row.top() + 10.0),
                            Vec2::splat(14.0),
                        ),
                        14.0,
                        icon_color,
                    );
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(row.left() + 30.0, row.top() + 8.0),
                            Pos2::new(row.right() - 8.0, row.top() + 28.0),
                        ),
                        title,
                        font(14.0, Weight::Medium),
                        theme::sidebar_foreground(),
                    );
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(row.left() + 30.0, row.top() + 28.0),
                            Pos2::new(row.right() - 8.0, row.top() + 44.0),
                        ),
                        PAGES[*page].0,
                        font(11.0, Weight::Medium),
                        theme::alpha(theme::sidebar_muted(), 0.75),
                    );
                    if response.clicked() {
                        open = Some((*page, target.clone()));
                    }
                }
                ui.allocate_rect(
                    Rect::from_min_size(origin, Vec2::new(255.0, results.len() as f32 * 56.0)),
                    Sense::hover(),
                );
            });
        if let Some(index) = hovered {
            self.settings_search_active = index;
        }
        if let Some((page, target)) = open {
            self.open_settings_result(page, target);
        }
        true
    }

    /// `handleSearchResultClick`: clears the search and jumps to the row.
    pub(crate) fn open_settings_result(&mut self, page: usize, target: String) {
        self.settings_search.clear();
        self.settings_search_active = 0;
        self.open_settings_page(page);
        self.settings_target = Some(Target {
            id: target,
            pulse_from: None,
        });
    }

    /// Centers the searched row once and pulses it twice.
    pub(crate) fn settings_target_row(&mut self, ui: &Ui, id: &str, rect: Rect) {
        let Some(target) = &mut self.settings_target else {
            return;
        };
        if target.id != id {
            return;
        }
        let now = ui.input(|i| i.time);
        let from = match target.pulse_from {
            Some(from) => from,
            None => {
                ui.scroll_to_rect(rect, Some(egui::Align::Center));
                target.pulse_from = Some(now);
                now
            }
        };
        let elapsed = now - from;
        if elapsed >= PULSE * 2.0 {
            self.settings_target = None;
            return;
        }
        let phase = (elapsed % PULSE) / PULSE;
        let strength = (phase * std::f64::consts::PI).sin() as f32;
        ui.painter().rect_stroke(
            rect.expand(1.0),
            CornerRadius::same(13),
            Stroke::new(2.0, theme::alpha(theme::primary(), 0.7 * strength)),
            StrokeKind::Middle,
        );
        ui.ctx().request_repaint();
    }
}

/// Ported catalog entries before the keybinding commands.
const BEFORE: &[Item] = &[
    Item {
        id: "storage-worktrees",
        title: "Worktree cleanup",
        page: 7,
        target: "storage-worktrees",
        terms: "disk storage delete deleted archived threads old inactive merged unchanged worktrees retention days project inherit off custom",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "storage-artifacts",
        title: "Artifacts and logs",
        page: 7,
        target: "storage-artifacts",
        terms: "disk storage browser screenshots captures rotated logs cleanup retention",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "project-defaults",
        title: "Project defaults and overrides",
        page: 0,
        target: "project-defaults",
        terms: "model workspace environments projects inheritance checkout",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "default-model",
        title: "Default model",
        page: 0,
        target: "default-model",
        terms: "new thread project provider reasoning effort",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "default-permissions",
        title: "Permissions",
        page: 0,
        target: "default-permissions",
        terms: "new thread default runtime mode supervised approvals auto accept edits full access",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "color-scheme",
        title: "Color scheme",
        page: 1,
        target: "appearance",
        terms: "appearance light dark system mode",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "theme",
        title: "Themes",
        page: 1,
        target: "appearance",
        terms: "appearance colors palette custom import",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "setting-appearance-contrast",
        title: "Contrast",
        page: 1,
        target: "setting-appearance-contrast",
        terms: "colors borders interface",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "setting-glass-opacity",
        title: "Glass opacity",
        page: 1,
        target: "setting-glass-opacity",
        terms: "transparent transparency solid menus dialogs composer",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "diff-color-scheme",
        title: "Diff colors",
        page: 1,
        target: "diff-color-scheme",
        terms: "red green blue orange additions deletions changes counts palette colorblind",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "chat-width",
        title: "Chat width",
        page: 1,
        target: "chat-width",
        terms: "wide full width column layout messages composer monitor",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "panel-animations",
        title: "Panel animations",
        page: 1,
        target: "panel-animations",
        terms: "",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "environment-identification",
        title: "Environment identification",
        page: 1,
        target: "appearance-interface",
        terms: "dev nightly artwork pill label hide none",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "interface-font",
        title: "Interface font",
        page: 1,
        target: "interface-font",
        terms: "typography family size system sans",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "prompt-font",
        title: "Prompt font",
        page: 1,
        target: "prompt-font",
        terms: "typography family size composer input",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "code-font",
        title: "Code font",
        page: 1,
        target: "code-font",
        terms: "typography family size monospace code blocks diffs file previews",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "terminal-font",
        title: "Terminal font",
        page: 1,
        target: "terminal-font",
        terms: "typography family size monospace output",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "word-wrap",
        title: "Word wrap",
        page: 1,
        target: "word-wrap",
        terms: "long lines code blocks tables diffs file previews",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "project-grouping",
        title: "Project grouping",
        page: 0,
        target: "project-grouping",
        terms: "combine matching repositories environments sidebar",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "working-shelf",
        title: "Working section (beta)",
        page: 0,
        target: "working-shelf",
        terms: "hide fold running monitoring threads inbox sidebar shelf",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "auto-settle-inactive-threads",
        title: "Auto-settle inactive threads",
        page: 0,
        target: "auto-settle-inactive-threads",
        terms: "sidebar inactivity days no activity automatically",
        secondary: false,
        auto_settlement: true,
    },
    Item {
        id: "auto-settle-merged-threads",
        title: "Auto-settle merged threads",
        page: 0,
        target: "auto-settle-merged-threads",
        terms: "pull request merge closed automatically sidebar",
        secondary: false,
        auto_settlement: true,
    },
    Item {
        id: "days-before-auto-settle",
        title: "Days of inactivity before auto-settle",
        page: 0,
        target: "auto-settle-inactive-threads",
        terms: "thread timeout activity sidebar",
        secondary: false,
        auto_settlement: true,
    },
    Item {
        id: "thread-notifications",
        title: "Thread notifications",
        page: 0,
        target: "thread-notifications",
        terms: "notification sound alert completion input approval desktop",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "in-app-notifications",
        title: "In-app notifications",
        page: 0,
        target: "in-app-notifications",
        terms: "notification toast popup completion input approval failure",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "time-format",
        title: "Time format",
        page: 0,
        target: "time-format",
        terms: "timestamp clock locale system browser os 12 hour 24 hour",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "response-streaming",
        title: "Response streaming",
        page: 0,
        target: "response-streaming",
        terms: "output token paragraph buffered wait turn legacy",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "hide-whitespace-changes",
        title: "Hide whitespace changes",
        page: 0,
        target: "hide-whitespace-changes",
        terms: "diff ignore spaces edits default",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "default-diff-file-state",
        title: "Default diff file state",
        page: 0,
        target: "default-diff-file-state",
        terms: "collapsed expanded collapse expand files pull request pr code tab",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "diff-layout",
        title: "Diff layout",
        page: 0,
        target: "diff-layout",
        terms: "stacked split side by side unified inline view",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "proactive-panels",
        title: "Proactive panels",
        page: 0,
        target: "proactive-panels",
        terms: "automatically open diff pull request pr right panel agent completion",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "skills-in-slash-menu",
        title: "Show skills in slash menu",
        page: 0,
        target: "skills-in-slash-menu",
        terms: "command menu dollar $ slash /",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "composer-rich-text",
        title: "Rich text composer",
        page: 0,
        target: "composer-rich-text",
        terms: "composer rich text tiptap bold italic markdown styled wysiwyg",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "composer-collapse",
        title: "Collapse composer on scroll",
        page: 0,
        target: "composer-collapse",
        terms: "composer rest resting scroll wheel conversation timeline shrink minimize",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "send-shortcut",
        title: "Send shortcut",
        page: 0,
        target: "send-shortcut",
        terms: "enter return command ctrl multiline prompt new line composer",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "follow-up-behavior",
        title: "Follow-up behavior",
        page: 0,
        target: "follow-up-behavior",
        terms: "queue steer running turn send default behavior composer",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "provider-update-checks",
        title: "Provider update checks",
        page: 0,
        target: "provider-update-checks",
        terms: "installed cli versions newer available codex claude cursor grok opencode",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "continue-threads-after-server-update",
        title: "Continue threads after restarts",
        page: 0,
        target: "continue-threads-after-server-update",
        terms: "resume running active interrupted work restart reboot machine crash desktop update automatically",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "background-activity",
        title: "Background activity",
        page: 0,
        target: "background-activity",
        terms: "balanced performance battery saver advanced git fetch provider health refresh host power monitor idle policy",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "new-threads",
        title: "New threads",
        page: 0,
        target: "new-threads",
        terms: "default workspace mode draft local worktree",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "worktree-submodules",
        title: "Submodules",
        page: 0,
        target: "worktree-submodules",
        terms: "git submodule init recursive top-level none worktree t3.json",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "start-from-origin",
        title: "Start from origin",
        page: 0,
        target: "start-from-origin",
        terms: "new worktrees latest matching remote branch local",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "add-project-starts-in",
        title: "Add project starts in",
        page: 0,
        target: "add-project-starts-in",
        terms: "base directory folder browser path home",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "unpin-confirmation",
        title: "Unpin confirmation",
        page: 0,
        target: "unpin-confirmation",
        terms: "ask before thread pinned section",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "archive-confirmation",
        title: "Archive confirmation",
        page: 0,
        target: "archive-confirmation",
        terms: "ask before thread second click inline action",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "delete-confirmation",
        title: "Delete confirmation",
        page: 0,
        target: "delete-confirmation",
        terms: "ask before thread chat history",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "quit-confirmation",
        title: "Quit shortcut",
        page: 0,
        target: "quit-confirmation",
        terms: "confirmation desktop app exit direct hold double click press twice",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "text-generation-model",
        title: "Text generation model",
        page: 0,
        target: "text-generation-model",
        terms: "generated thread titles source control content default provider",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "diagnostics",
        title: "Diagnostics",
        page: 0,
        target: "diagnostics",
        terms: "logs traces processes resource history failures spans cpu memory",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "open-source-licenses",
        title: "Open source licenses",
        page: 0,
        target: "open-source-licenses",
        terms: "",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "legacy-plan-mode",
        title: "Plan mode (legacy)",
        page: 0,
        target: "legacy-plan-mode",
        terms: "build plan composer old",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "legacy-context-window-indicator",
        title: "Context window indicator (legacy)",
        page: 0,
        target: "legacy-context-window-indicator",
        terms: "composer meter usage tokens circle old",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "legacy-sidebar",
        title: "Sidebar (legacy)",
        page: 0,
        target: "legacy-sidebar",
        terms: "project thread tree old flat list",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "keybindings",
        title: "Keybindings",
        page: 2,
        target: "keybindings",
        terms: "keyboard shortcuts hotkeys commands bindings json",
        secondary: false,
        auto_settlement: false,
    },
];

/// Ported catalog entries after the keybinding commands.
const AFTER: &[Item] = &[
    Item {
        id: "snap-shot-enabled",
        title: "SnapShots",
        page: 3,
        target: "snap-shot-enabled",
        terms: "window capture screenshot",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "snap-shot-accessibility",
        title: "Include app text",
        page: 3,
        target: "snap-shot-enabled",
        terms: "capture accessibility data text UI structure elements privacy omit agent context",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "snap-shot-shortcut",
        title: "Capture shortcut",
        page: 3,
        target: "snap-shot-enabled",
        terms: "",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "snap-shot-sound",
        title: "Capture sound",
        page: 3,
        target: "snap-shot-enabled",
        terms: "",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "snap-shot-flash",
        title: "Capture flash",
        page: 3,
        target: "snap-shot-enabled",
        terms: "",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "snap-shot-animations",
        title: "Capture animations",
        page: 3,
        target: "snap-shot-enabled",
        terms: "",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "providers",
        title: "Providers",
        page: 4,
        target: "providers",
        terms: "agents cli codex claude cursor grok opencode antigravity google sign in sign out install subscription instances authentication api key models configuration binary path config directory endpoint arguments environment variables display name accent color custom favorite hidden auto compact",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "usage-providers",
        title: "Usage providers",
        page: 4,
        target: "usage-providers",
        terms: "usage sources CLIProxyAPI CLI proxy hub quota subscription limits management key add remove",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "provider-health-check-interval",
        title: "Health check interval",
        page: 4,
        target: "provider-health-check-interval",
        terms: "refresh availability versions auth state models background probes seconds off",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "agent-browser-access",
        title: "Agent browser access",
        page: 5,
        target: "agent-browser-access",
        terms: "allow disable enable open drive preview tools sessions project override",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "device-hosts",
        title: "Device hosts",
        page: 5,
        target: "device-hosts",
        terms: "ssh remote simulator emulator ios android mac mini identity key connection",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "agent-device-access",
        title: "Agent device access",
        page: 5,
        target: "devices",
        terms: "allow simulator emulator ios android drive tools sessions",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "device-hub",
        title: "Device hub",
        page: 5,
        target: "devices",
        terms: "simulator emulator ios android install start",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "device-platform-support",
        title: "Simulator support",
        page: 5,
        target: "devices",
        terms: "xcode android studio sdk avd runtime",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "browser-profiles",
        title: "Browser profiles",
        page: 5,
        target: "browser",
        terms: "",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "browser-default-profile",
        title: "Default browser profile",
        page: 5,
        target: "browser-profiles",
        terms: "",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "browser-default-viewport",
        title: "Default browser viewport",
        page: 5,
        target: "browser-default-viewport",
        terms: "preview size width height device desktop mobile rotate",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "browser-default-zoom",
        title: "Default browser zoom",
        page: 5,
        target: "browser-default-zoom",
        terms: "preview page scale tabs percent",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "browser-default-appearance",
        title: "Default browser appearance",
        page: 5,
        target: "browser-default-appearance",
        terms: "preview color scheme light dark system os",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "browser-recording-frame-rate",
        title: "Browser recording frame rate",
        page: 5,
        target: "browser-recording-frame-rate",
        terms: "",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "browser-recording-key-presses",
        title: "Show key presses in recordings",
        page: 5,
        target: "browser-recording-key-presses",
        terms: "browser preview keyboard shortcuts keystrokes overlay capture",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "browser-recording-mouse-presses",
        title: "Show mouse presses in recordings",
        page: 5,
        target: "browser-recording-mouse-presses",
        terms: "browser preview clicks buttons drag overlay capture",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "browser-link-target",
        title: "Open links in",
        page: 5,
        target: "browser-link-target",
        terms: "links default browser in-app browser external open",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "browser-auto-show-floating-preview",
        title: "Auto-show floating preview",
        page: 5,
        target: "browser-auto-show-floating-preview",
        terms: "agent opens browser device simulator pop into view hide",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "automatic-pull",
        title: "Automatically pull",
        page: 6,
        target: "automatic-pull",
        terms: "auto pull default branch current checkout fast forward upstream",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "pull-request-merge-method",
        title: "Default merge method",
        page: 6,
        target: "pull-request-merge-method",
        terms: "pull request merge squash rebase last selected",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "source-control",
        title: "Source control",
        page: 6,
        target: "source-control",
        terms: "version control git github gitlab forgejo gitea tea codeberg bitbucket azure devops hosting integrations credentials scan server environment",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "git-fetch-interval",
        title: "Git fetch interval",
        page: 6,
        target: "git-fetch-interval",
        terms: "automatic remote branch refresh background credentials security keys seconds off",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "bitbucket-credentials",
        title: "Bitbucket credentials",
        page: 6,
        target: "bitbucket-credentials",
        terms: "bitbucket atlassian access token api token email credentials sign in",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "source-control-writing-style",
        title: "Source control writing style",
        page: 6,
        target: "source-control-writing-style",
        terms: "repository conventions conventional commits custom instructions change descriptions request titles",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "follow-change-request-templates",
        title: "Follow change request templates",
        page: 6,
        target: "follow-change-request-templates",
        terms: "repository pr pull request description structure",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "source-control-writer-model",
        title: "Source control writer model",
        page: 6,
        target: "source-control-writer-model",
        terms: "override generated commit change request pr titles descriptions branch bookmark",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "environment-icon",
        title: "Environment icon",
        page: 8,
        target: "connections-environment",
        terms: "machine glyph sidebar mac mini studio laptop desktop server cloud vm",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "local-environment",
        title: "Local environment",
        page: 8,
        target: "connections-environment",
        terms: "turn off on disable enable local server agents remote only restart",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "network-access",
        title: "Network access",
        page: 8,
        target: "connections-environment",
        terms: "expose backend remote pairing local machine interfaces host restart",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "tailscale-https",
        title: "Tailscale HTTPS",
        page: 8,
        target: "connections-environment",
        terms: "serve magicdns endpoint remote secure network",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "connections-environment",
        title: "This machine",
        page: 8,
        target: "connections-environment",
        terms: "connections server backend local remote access administrative permissions scope pairing links qr code authorized clients sessions revoke endpoint",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "remote-environments",
        title: "Environments",
        page: 8,
        target: "remote-environments",
        terms: "add pair backend host code ssh config agent tunnel saved t3 connect",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "load-balancing",
        title: "Load balancing",
        page: 8,
        target: "load-balancing",
        terms: "automatic machine environment resources cpu memory capacity preference weight shared projects",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "github-routing",
        title: "GitHub sharing",
        page: 8,
        target: "github-routing",
        terms: "pull request trusted environments shared credentials permissions read actions",
        secondary: false,
        auto_settlement: false,
    },
    Item {
        id: "archive",
        title: "Archived threads",
        page: 9,
        target: "archive",
        terms: "restore reopen deleted history projects",
        secondary: false,
        auto_settlement: false,
    },
];

/// `STATIC_KEYBINDING_COMMANDS` with their `DEFAULT_KEYBINDINGS` keys.
const KEYBINDINGS: &[(&str, &[&str])] = &[
    ("sidebar.toggle", &["mod+b"]),
    ("navigation.back", &["mod+["]),
    ("navigation.forward", &["mod+]"]),
    ("terminal.toggle", &["mod+j"]),
    ("terminal.split", &["mod+d"]),
    ("terminal.splitVertical", &["mod+shift+d"]),
    ("terminal.new", &["mod+n"]),
    ("terminal.close", &["mod+w"]),
    ("rightPanel.toggle", &["mod+alt+b"]),
    ("rightPanel.toggleMaximized", &[]),
    ("rightPanel.close", &["mod+w"]),
    ("pullRequest.copyNumber", &["mod+shift+k"]),
    ("diff.toggle", &["mod+d"]),
    ("preview.toggle", &["mod+shift+j"]),
    ("preview.refresh", &["mod+r"]),
    ("preview.focusUrl", &["mod+l"]),
    ("preview.zoomIn", &["mod+=", "mod++"]),
    ("preview.zoomOut", &["mod+-"]),
    ("preview.resetZoom", &["mod+0"]),
    ("commandPalette.toggle", &["mod+k"]),
    ("filePicker.toggle", &["mod+p"]),
    ("projectSearch.toggle", &["mod+shift+f"]),
    ("usage.open", &["mod+u"]),
    ("theme.select", &["mod+alt+a"]),
    ("appearance.cycle", &["mod+alt+shift+a"]),
    ("themeEditor.toggle", &["mod+alt+shift+t"]),
    ("composer.stash", &["mod+s"]),
    ("composer.host", &["mod+shift+h"]),
    ("composer.effort", &["mod+shift+e"]),
    ("composer.mode", &["mod+shift+a"]),
    ("composer.workspace", &["mod+shift+x"]),
    ("composer.previousWorktree", &["mod+shift+l"]),
    ("composer.branch", &["mod+shift+g"]),
    ("chat.new", &["mod+n", "mod+shift+o"]),
    ("chat.newLocal", &["mod+shift+n"]),
    ("chat.newWithoutProject", &["mod+alt+n"]),
    ("editor.openFavorite", &["mod+o"]),
    ("usage.cost", &["c"]),
    ("usage.tokens", &["t"]),
    ("usage.limits", &["l"]),
    ("usage.period.day", &["mod+shift+1"]),
    ("usage.period.week", &["mod+shift+2"]),
    ("usage.period.month", &["mod+shift+3"]),
    ("usage.period.quarter", &["mod+shift+4"]),
    ("modelPicker.toggle", &["mod+shift+m"]),
    ("modelPicker.previousProvider", &["mod+shift+arrowup"]),
    ("modelPicker.nextProvider", &["mod+shift+arrowdown"]),
    ("modelPicker.jump.1", &["mod+1"]),
    ("modelPicker.jump.2", &["mod+2"]),
    ("modelPicker.jump.3", &["mod+3"]),
    ("modelPicker.jump.4", &["mod+4"]),
    ("modelPicker.jump.5", &["mod+5"]),
    ("modelPicker.jump.6", &["mod+6"]),
    ("modelPicker.jump.7", &["mod+7"]),
    ("modelPicker.jump.8", &["mod+8"]),
    ("modelPicker.jump.9", &["mod+9"]),
    ("thread.stop", &[]),
    ("thread.steerQueuedMessage", &["mod+shift+enter"]),
    ("thread.previous", &["mod+shift+["]),
    ("thread.next", &["mod+shift+]"]),
    ("thread.copyReference", &["mod+shift+c"]),
    ("thread.settle", &["mod+shift+s"]),
    ("thread.pin", &["mod+shift+p"]),
    ("thread.undo", &["mod+z"]),
    ("thread.jump.1", &["mod+1"]),
    ("thread.jump.2", &["mod+2"]),
    ("thread.jump.3", &["mod+3"]),
    ("thread.jump.4", &["mod+4"]),
    ("thread.jump.5", &["mod+5"]),
    ("thread.jump.6", &["mod+6"]),
    ("thread.jump.7", &["mod+7"]),
    ("thread.jump.8", &["mod+8"]),
    ("thread.jump.9", &["mod+9"]),
];
