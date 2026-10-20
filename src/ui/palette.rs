//! Command palette (`CommandPalette.tsx`, `CommandPalette.logic.ts`): root
//! actions and recent threads, "New thread in…", "Add project" with local
//! folder browsing (`filesystem.browse`). Actions whose screens are not ported
//! yet say so when run.
use eframe::egui::{
    self, Color32, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::{Project, Thread};
use crate::theme::{self, Weight, font};
use crate::ui::{pickers, project_icon, widgets};

const WIDTH: f32 = 576.0;
const TOP: f32 = 90.0;
const SEARCH: f32 = 53.0; // 7px padding + 36px row + 10px, measured
const FOOTER: f32 = 40.0;
const GROUP: f32 = 28.0;
const ROW: f32 = 32.0;
const ROW_TALL: f32 = 50.0;
const LIST_MAX: f32 = 322.0;
const SURFACE: Color32 = Color32::from_rgb(12, 12, 12);

#[derive(Clone, Debug, PartialEq)]
pub enum View {
    Root,
    NewThreadIn,
    AddProject,
    Browse,
}

#[derive(Default)]
pub struct PaletteState {
    pub open: bool,
    pub query: String,
    pub stack: Vec<View>,
    pub highlight: usize,
    pub focus: bool,
    /// The browse view shows a highlight only after arrow-key navigation.
    pub keyboard_moved: bool,
}

impl PaletteState {
    fn view(&self) -> View {
        self.stack.last().cloned().unwrap_or(View::Root)
    }
}

#[derive(Clone)]
enum Icon {
    Lucide(&'static str),
    Project(Project),
    Brand(&'static str, Color32),
}

#[derive(Clone)]
enum Action {
    NewThreadIn(String),
    Push(View),
    OpenThread(String),
    BrowseInto(String),
    AddPath(String),
    Missing(&'static str),
}

/// Subtitle data of a thread row.
#[derive(Clone)]
struct ThreadMeta {
    project: Option<Project>,
    branch: Option<String>,
    provider: Option<(&'static str, Color32)>,
}

#[derive(Clone)]
struct Item {
    icon: Icon,
    title: String,
    bold: Option<String>,
    subtitle: Option<String>,
    thread: Option<ThreadMeta>,
    shortcut: Option<&'static str>,
    submenu: bool,
    status: Option<(&'static str, Color32)>,
    time: Option<String>,
    badge: Option<&'static str>,
    terms: Vec<String>,
    action: Action,
}

impl Item {
    fn new(icon: &'static str, title: &str, action: Action) -> Self {
        Self {
            icon: Icon::Lucide(icon),
            title: title.into(),
            bold: None,
            subtitle: None,
            thread: None,
            shortcut: None,
            submenu: false,
            status: None,
            time: None,
            badge: None,
            terms: vec![title.to_lowercase()],
            action,
        }
    }

    fn height(&self) -> f32 {
        if self.subtitle.is_some() || self.thread.is_some() {
            ROW_TALL
        } else {
            ROW
        }
    }
}

struct Group {
    label: &'static str,
    items: Vec<Item>,
}

/// Directory listing results, keyed by the requested partial path.
pub enum Browse {
    Loading,
    Loaded {
        parent: String,
        entries: Vec<(String, String)>,
    },
    Failed(String),
}

impl Browse {
    pub fn from_result(result: Result<Value, String>) -> Self {
        match result {
            Ok(value) => Browse::Loaded {
                parent: value["parentPath"].as_str().unwrap_or_default().to_owned(),
                entries: value["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| {
                        Some((
                            e["name"].as_str()?.to_owned(),
                            e["fullPath"].as_str()?.to_owned(),
                        ))
                    })
                    .collect(),
            },
            Err(message) => Browse::Failed(message),
        }
    }
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
}

impl App {
    pub(crate) fn toggle_palette(&mut self, view: View) {
        if self.palette.open && self.palette.view() == view {
            self.palette = PaletteState::default();
            return;
        }
        self.palette = PaletteState {
            open: true,
            query: if view == View::Browse {
                "~/".into()
            } else {
                String::new()
            },
            stack: if view == View::Root {
                Vec::new()
            } else {
                vec![view]
            },
            highlight: 0,
            focus: true,
            keyboard_moved: false,
        };
    }

    fn status_badge(thread: &Thread) -> Option<(&'static str, Color32)> {
        if thread.has_pending_approvals {
            Some(("Pending Approval", theme::WARNING_FOREGROUND))
        } else if thread.has_pending_user_input {
            Some(("Awaiting Input", theme::INDIGO_300))
        } else if thread.is_running() {
            Some(("Working", theme::SKY_400))
        } else {
            None
        }
    }

    fn thread_items(&self, limit: Option<usize>) -> Vec<Item> {
        let mut threads: Vec<&Thread> = self
            .state
            .threads
            .iter()
            .filter(|t| t.archived_at.is_none())
            .collect();
        threads.sort_by(|a, b| {
            let at = |t: &Thread| {
                t.latest_user_message_at
                    .clone()
                    .unwrap_or(t.updated_at.clone())
            };
            at(b).cmp(&at(a))
        });
        threads
            .into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .map(|thread| {
                let project = self.state.project(&thread.project_id);
                let mut item = Item::new(
                    "message-square",
                    &thread.title,
                    Action::OpenThread(thread.id.clone()),
                );
                item.thread = Some(ThreadMeta {
                    project: project.cloned(),
                    branch: thread.branch.clone(),
                    provider: self
                        .provider_driver(&thread.model_selection.instance_id)
                        .and_then(|driver| pickers::provider_icon(&driver)),
                });
                item.status = Self::status_badge(thread);
                item.time = Some(relative_ago(
                    thread
                        .latest_user_message_at
                        .as_deref()
                        .unwrap_or(&thread.updated_at),
                    self.now,
                ));
                item.terms.extend(project.map(|p| normalize(&p.title)));
                item.terms.extend(thread.branch.as_deref().map(normalize));
                item.terms.push(thread.id.clone());
                item
            })
            .collect()
    }

    fn palette_groups(&mut self) -> (Vec<Group>, bool) {
        let query = self.palette.query.trim().to_owned();
        let view = self.palette.view();
        let mut searchable = false;
        let groups = match view {
            View::Root => {
                let current = self
                    .current_thread()
                    .and_then(|t| self.state.project(&t.project_id).cloned())
                    .or_else(|| self.draft_project());
                let mut actions = Vec::new();
                if let Some(project) = &current {
                    let mut item = Item::new(
                        "square-pen",
                        "New thread in ",
                        Action::NewThreadIn(project.id.clone()),
                    );
                    item.bold = Some(project.title.clone());
                    item.shortcut = Some("Ctrl+Shift+O");
                    item.terms.push(normalize(&project.title));
                    actions.push(item);
                }
                let mut submenu = Item::new(
                    "square-pen",
                    "New thread in...",
                    Action::Push(View::NewThreadIn),
                );
                submenu.submenu = true;
                actions.push(submenu);
                let mut scratch = Item::new(
                    "message-square-dashed",
                    "New thread without a project",
                    Action::Missing("Threads without a project"),
                );
                scratch.shortcut = Some("Ctrl+Alt+N");
                actions.push(scratch);
                let mut file =
                    Item::new("file-search", "Go to file", Action::Missing("Go to file"));
                file.shortcut = Some("Ctrl+P");
                actions.push(file);
                let mut search = Item::new(
                    "text-search",
                    "Search project contents",
                    Action::Missing("Project search"),
                );
                search.shortcut = Some("Ctrl+Shift+F");
                actions.push(search);
                actions.push(Item::new(
                    "folder-git-2",
                    "New project",
                    Action::Missing("New project"),
                ));
                actions.push(Item::new(
                    "folder-plus",
                    "Add project",
                    Action::Push(View::AddProject),
                ));
                let mut theme_item =
                    Item::new("palette", "Change theme", Action::Missing("Themes"));
                theme_item.shortcut = Some("Ctrl+Alt+A");
                theme_item.submenu = true;
                actions.push(theme_item);
                let mut appearance = Item::new(
                    "monitor",
                    "Change appearance",
                    Action::Missing("Appearance"),
                );
                appearance.shortcut = Some("Ctrl+Alt+Shift+A");
                appearance.submenu = true;
                actions.push(appearance);
                let mut editor = Item::new(
                    "palette",
                    "Toggle theme editor",
                    Action::Missing("The theme editor"),
                );
                editor.shortcut = Some("Ctrl+Alt+Shift+T");
                actions.push(editor);
                actions.push(Item::new(
                    "git-pull-request-arrow",
                    "Open pull requests",
                    Action::Missing("Pull requests"),
                ));
                let mut usage = Item::new(
                    "chart-no-axes-column",
                    "Open usage",
                    Action::Missing("Usage"),
                );
                usage.shortcut = Some("Ctrl+U");
                actions.push(usage);
                actions.push(Item::new(
                    "settings",
                    "Open settings",
                    Action::Missing("Settings"),
                ));
                if let Some(project) = &current {
                    let mut settings = Item::new(
                        "folder",
                        "Project settings",
                        Action::Missing("Project settings"),
                    );
                    settings.subtitle = Some(project.title.clone());
                    actions.push(settings);
                }
                searchable = true;
                if query.is_empty() {
                    vec![
                        Group {
                            label: "Actions",
                            items: actions,
                        },
                        Group {
                            label: "Recent Threads",
                            items: self.thread_items(Some(12)),
                        },
                    ]
                } else {
                    let projects = self
                        .state
                        .projects
                        .iter()
                        .map(|p| {
                            let mut item =
                                Item::new("folder", &p.title, Action::NewThreadIn(p.id.clone()));
                            item.icon = Icon::Project(p.clone());
                            item.subtitle = Some(format!("Local · {}", p.workspace_root));
                            item.terms.push(normalize(&p.workspace_root));
                            item
                        })
                        .collect();
                    vec![
                        Group {
                            label: "Actions",
                            items: actions,
                        },
                        Group {
                            label: "Projects",
                            items: projects,
                        },
                        Group {
                            label: "Threads",
                            items: self.thread_items(None),
                        },
                    ]
                }
            }
            View::NewThreadIn => {
                searchable = true;
                let items = self
                    .state
                    .projects
                    .iter()
                    .map(|p| {
                        let mut item =
                            Item::new("folder", &p.title, Action::NewThreadIn(p.id.clone()));
                        item.icon = Icon::Project(p.clone());
                        item.subtitle = Some(format!("Local · {}", p.workspace_root));
                        item
                    })
                    .collect();
                vec![Group {
                    label: "Projects",
                    items,
                }]
            }
            View::AddProject => {
                searchable = true;
                let mut new_project = Item::new(
                    "folder-git-2",
                    "New project",
                    Action::Missing("New project"),
                );
                new_project.subtitle = Some("Start a new Git repository from a name".into());
                let mut local =
                    Item::new("folder-plus", "Local folder", Action::Push(View::Browse));
                local.subtitle = Some("Browse a folder on disk".into());
                let mut git = Item::new("link", "Git URL", Action::Missing("Cloning from a URL"));
                git.subtitle = Some("Clone from a remote URL".into());
                let mut github = Item::new(
                    "github",
                    "GitHub repository",
                    Action::Missing("Cloning from GitHub"),
                );
                github.icon = Icon::Brand("github", theme::FOREGROUND);
                github.subtitle = Some("Clone GitHub owner/repo".into());
                vec![Group {
                    label: "Sources",
                    items: vec![new_project, local, git, github],
                }]
            }
            View::Browse => {
                let path = if query.is_empty() {
                    "~/".to_owned()
                } else {
                    self.palette.query.clone()
                };
                let key = format!("browse:{path}");
                if !self.browse.contains_key(&key) {
                    self.browse.insert(key.clone(), Browse::Loading);
                    self.send_request(
                        key.clone(),
                        "filesystem.browse",
                        json!({"partialPath": path}),
                    );
                }
                let items = match self.browse.get(&key) {
                    Some(Browse::Loaded { parent, entries }) => {
                        let parent = parent.trim_end_matches('/').to_owned();
                        let base = match path.rfind('/') {
                            Some(index) => path[..=index].to_owned(),
                            None => format!("{parent}/"),
                        };
                        // Hidden folders appear only once the typed segment starts with a dot.
                        let show_hidden =
                            path.rsplit('/').next().is_some_and(|s| s.starts_with('.'));
                        entries
                            .iter()
                            .filter(|(name, _)| show_hidden || !name.starts_with('.'))
                            .map(|(name, _full)| {
                                Item::new(
                                    "folder",
                                    name,
                                    Action::BrowseInto(format!("{base}{name}/")),
                                )
                            })
                            .collect()
                    }
                    Some(Browse::Failed(message)) => {
                        let mut item = Item::new(
                            "circle-alert",
                            "Could not read this folder",
                            Action::BrowseInto(path.clone()),
                        );
                        item.subtitle = Some(message.clone());
                        vec![item]
                    }
                    _ => Vec::new(),
                };
                vec![Group {
                    label: "Directories",
                    items,
                }]
            }
        };
        if query.is_empty() || !searchable {
            return (
                groups.into_iter().filter(|g| !g.items.is_empty()).collect(),
                searchable,
            );
        }
        let tokens: Vec<String> = normalize(&query)
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let filtered = groups
            .into_iter()
            .map(|group| Group {
                label: group.label,
                items: group
                    .items
                    .into_iter()
                    .filter(|item| {
                        let haystack = item.terms.join(" ");
                        tokens.iter().all(|t| haystack.contains(t.as_str()))
                    })
                    .collect(),
            })
            .filter(|g| !g.items.is_empty())
            .collect();
        (filtered, searchable)
    }

    /// Global shortcuts that open the palette or start threads.
    pub(crate) fn palette_shortcuts(&mut self, ui: &Ui) {
        if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::K)) {
            self.toggle_palette(View::Root);
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::O)) {
            self.start_draft(None);
        }
    }

    pub(crate) fn command_palette(&mut self, ui: &mut Ui, screen: Rect) {
        if !self.palette.open {
            return;
        }
        let (groups, _) = self.palette_groups();
        let rows: Vec<&Item> = groups.iter().flat_map(|g| g.items.iter()).collect();
        let count = rows.len();
        self.palette.highlight = self.palette.highlight.min(count.saturating_sub(1));

        // Keyboard navigation.
        let (up, down, enter, escape, backspace) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
                i.key_pressed(Key::Backspace),
            )
        });
        if (up || down) && count > 0 {
            self.palette.keyboard_moved = true;
        }
        if down && count > 0 {
            self.palette.highlight = (self.palette.highlight + 1) % count;
        }
        if up && count > 0 {
            self.palette.highlight = (self.palette.highlight + count - 1) % count;
        }
        let mut run: Option<Action> = None;
        if enter {
            if self.palette.view() == View::Browse && !self.palette.keyboard_moved {
                run = Some(Action::AddPath(self.palette.query.clone()));
            } else if let Some(item) = rows.get(self.palette.highlight) {
                run = Some(item.action.clone());
            }
        }
        let in_submenu = !self.palette.stack.is_empty();
        if escape {
            self.palette = PaletteState::default();
            return;
        }
        if backspace && in_submenu && self.palette.query.is_empty() {
            self.palette.stack.pop();
            self.palette.highlight = 0;
        }

        let list_height: f32 = groups
            .iter()
            .map(|g| GROUP + g.items.iter().map(Item::height).sum::<f32>())
            .sum::<f32>()
            .min(LIST_MAX);
        let height = SEARCH + 4.0 + list_height + 4.0 + FOOTER;
        let rect = Rect::from_min_size(
            Pos2::new(screen.center().x - WIDTH / 2.0, TOP),
            Vec2::new(WIDTH, height),
        );
        let highlight = self.palette.highlight;
        let keyboard_moved = self.palette.keyboard_moved;
        let view = self.palette.view();
        let mut query = std::mem::take(&mut self.palette.query);
        let focus = std::mem::take(&mut self.palette.focus);
        let mut hovered_row = None;
        let mut clicked_row = None;
        let mut back = false;
        let mut close = false;
        egui::Area::new(ui.id().with("palette"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                // Backdrop (upstream also blurs the app behind).
                let (backdrop, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    Color32::from_rgba_unmultiplied(10, 10, 10, 200),
                );
                if response.clicked() && !ui.rect_contains_pointer(rect) {
                    close = true;
                }
                ui.painter().rect(
                    rect,
                    CornerRadius::same(18),
                    SURFACE,
                    Stroke::new(1.0, theme::INPUT),
                    StrokeKind::Inside,
                );

                // Search row.
                let search = Rect::from_min_size(
                    rect.min + Vec2::new(9.0, 7.0),
                    Vec2::new(WIDTH - 18.0, 36.0),
                );
                let icon_rect = Rect::from_min_size(
                    Pos2::new(search.left() + 8.0, search.top()),
                    Vec2::new(16.0, 36.0),
                );
                if in_submenu {
                    let back_button = icon_rect.expand2(Vec2::new(4.0, -6.0));
                    if ui
                        .interact(back_button, ui.id().with("palette-back"), Sense::click())
                        .clicked()
                    {
                        back = true;
                    }
                    theme::paint_icon(ui, "arrow-left", icon_rect, 16.0, theme::FOREGROUND);
                } else {
                    theme::paint_icon(ui, "search", icon_rect, 16.0, theme::MUTED_FOREGROUND);
                }
                let mut input_right = search.right() - 8.0;
                if view == View::Browse {
                    let add = Rect::from_min_size(
                        Pos2::new(search.right() - 2.0 - 79.0, search.top() + 6.0),
                        Vec2::new(79.0, 24.0),
                    );
                    if widgets::outline_button(
                        ui,
                        add,
                        "palette-add",
                        None,
                        "Add",
                        CornerRadius::same(8),
                    )
                    .clicked()
                    {
                        run = Some(Action::AddPath(query.clone()));
                    }
                    let kbd = Rect::from_min_size(
                        Pos2::new(add.left() + 35.0, add.top() + 2.0),
                        Vec2::new(38.0, 20.0),
                    );
                    ui.painter()
                        .rect_filled(kbd, CornerRadius::same(4), theme::MUTED);
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(kbd.min + Vec2::new(4.0, 0.0), kbd.max),
                        "Enter",
                        font(12.0, Weight::Medium),
                        theme::MUTED_FOREGROUND,
                    );
                    input_right = add.left() - 8.0;
                }
                let input = Rect::from_min_max(
                    Pos2::new(search.left() + 32.0, search.top() + 8.0),
                    Pos2::new(input_right, search.bottom() - 8.0),
                );
                let placeholder = match view {
                    View::Root => "Search commands, projects, and threads...",
                    View::Browse => "Enter project path (e.g. ~/projects/my-app)",
                    _ => "Search...",
                };
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(input));
                let response = child.add(
                    egui::TextEdit::singleline(&mut query)
                        .id(egui::Id::new("palette-input"))
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .font(font(14.0, Weight::Regular))
                        .text_color(theme::FOREGROUND)
                        .hint_text(
                            egui::RichText::new(placeholder)
                                .font(font(14.0, Weight::Regular))
                                .color(theme::MUTED_FOREGROUND),
                        )
                        .desired_width(input.width()),
                );
                if focus || !response.has_focus() {
                    response.request_focus();
                }
                if focus && let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id)
                {
                    // Prefilled paths are edited from their end.
                    let end = egui::text::CCursor::new(query.chars().count());
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(end)));
                    state.store(ui.ctx(), response.id);
                }

                // List.
                let list = Rect::from_min_size(
                    Pos2::new(rect.left() + 9.0, rect.top() + SEARCH + 4.0),
                    Vec2::new(WIDTH - 18.0, list_height),
                );
                let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list));
                egui::ScrollArea::vertical()
                    .id_salt(("palette-list", format!("{view:?}")))
                    .show(&mut list_ui, |ui| {
                        let mut index = 0;
                        for group in &groups {
                            let (header, _) = ui.allocate_exact_size(
                                Vec2::new(list.width(), GROUP),
                                Sense::hover(),
                            );
                            widgets::text_in(
                                ui,
                                Rect::from_min_max(header.min + Vec2::new(8.0, 0.0), header.max),
                                group.label,
                                font(12.0, Weight::Medium),
                                theme::MUTED_FOREGROUND,
                            );
                            for item in &group.items {
                                let (row, _) = ui.allocate_exact_size(
                                    Vec2::new(list.width(), item.height()),
                                    Sense::hover(),
                                );
                                let response = ui.interact(
                                    row,
                                    ui.id().with(("palette-row", index)),
                                    Sense::click(),
                                );
                                if response.hovered()
                                    && ui.input(|i| i.pointer.delta() != Vec2::ZERO)
                                {
                                    hovered_row = Some(index);
                                }
                                if index == highlight && (view != View::Browse || keyboard_moved) {
                                    ui.painter().rect_filled(
                                        row,
                                        CornerRadius::same(6),
                                        theme::ACCENT,
                                    );
                                    if down || up {
                                        response.scroll_to_me(None);
                                    }
                                }
                                paint_item(ui, row, item);
                                if response.clicked() {
                                    clicked_row = Some(index);
                                }
                                index += 1;
                            }
                        }
                    });

                // Footer hints.
                let footer = Rect::from_min_max(
                    Pos2::new(rect.left() + 1.0, rect.bottom() - FOOTER),
                    Pos2::new(rect.right() - 1.0, rect.bottom() - 1.0),
                );
                ui.painter().rect_filled(
                    footer,
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 17,
                        se: 17,
                    },
                    theme::white(0.025),
                );
                let mut x = footer.left() + 16.0;
                let y = footer.top() + 10.0;
                let key_chip = |ui: &Ui, x: f32, content: &str, icon: bool| -> f32 {
                    let width = if icon {
                        20.0
                    } else {
                        widgets::text_width(ui, content, font(12.0, Weight::Medium)) + 8.0
                    };
                    let chip = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width.ceil(), 20.0));
                    ui.painter()
                        .rect_filled(chip, CornerRadius::same(4), theme::white(0.078));
                    if icon {
                        theme::paint_icon(ui, content, chip, 12.0, theme::FOREGROUND);
                    } else {
                        widgets::text_in(
                            ui,
                            Rect::from_min_max(chip.min + Vec2::new(4.0, 0.0), chip.max),
                            content,
                            font(12.0, Weight::Medium),
                            theme::FOREGROUND,
                        );
                    }
                    chip.right()
                };
                let label = |ui: &Ui, x: f32, text: &str| -> f32 {
                    let width = widgets::text_in(
                        ui,
                        Rect::from_min_max(Pos2::new(x, y), Pos2::new(footer.right(), y + 20.0)),
                        text,
                        font(14.0, Weight::Regular),
                        theme::MUTED_FOREGROUND,
                    );
                    x + width
                };
                x = key_chip(ui, x, "arrow-up", true) + 4.0;
                x = key_chip(ui, x, "arrow-down", true) + 6.0;
                x = label(ui, x, "Navigate") + 12.0;
                if view != View::Browse {
                    x = key_chip(ui, x, "Enter", false) + 6.0;
                    x = label(ui, x, "Select") + 12.0;
                }
                if in_submenu {
                    x = key_chip(ui, x, "Backspace", false) + 6.0;
                    x = label(ui, x, "Back") + 12.0;
                }
                x = key_chip(ui, x, "Esc", false) + 6.0;
                label(ui, x, "Close");
            });
        self.palette.query = query;
        if let Some(index) = hovered_row {
            self.palette.highlight = index;
        }
        if let Some(index) = clicked_row
            && let Some(item) = rows.get(index)
        {
            run = Some(item.action.clone());
        }
        if close {
            self.palette = PaletteState::default();
            return;
        }
        if back {
            self.palette.stack.pop();
            self.palette.query.clear();
            self.palette.highlight = 0;
            self.palette.focus = true;
        }
        if let Some(action) = run {
            self.run_palette_action(action);
        }
    }

    fn run_palette_action(&mut self, action: Action) {
        match action {
            Action::NewThreadIn(project) => {
                self.palette = PaletteState::default();
                self.start_draft(Some(project));
            }
            Action::Push(view) => {
                self.palette.query = if view == View::Browse {
                    "~/".into()
                } else {
                    String::new()
                };
                self.palette.stack.push(view);
                self.palette.highlight = 0;
                self.palette.focus = true;
            }
            Action::OpenThread(id) => {
                self.palette = PaletteState::default();
                self.select_thread(Some(id));
            }
            Action::BrowseInto(path) => {
                self.palette.query = path;
                self.palette.highlight = 0;
                self.palette.focus = true;
            }
            Action::AddPath(path) => self.add_project(path.trim()),
            Action::Missing(what) => {
                self.palette = PaletteState::default();
                self.status = Some(format!("{what} is not implemented yet"));
            }
        }
    }

    /// `handleAddProjectForEnvironment`: reuse a project at that path or create one.
    fn add_project(&mut self, raw: &str) {
        let path = raw.trim_end_matches('/').to_owned();
        if path.is_empty() || path == "~" && raw.is_empty() {
            return;
        }
        self.palette = PaletteState::default();
        if let Some(existing) = self
            .state
            .projects
            .iter()
            .find(|p| p.workspace_root.trim_end_matches('/') == path)
        {
            let id = existing.id.clone();
            self.start_draft(Some(id));
            return;
        }
        let title = path
            .rsplit('/')
            .find(|s| !s.is_empty())
            .unwrap_or(&path)
            .to_owned();
        let project_id = uuid::Uuid::new_v4().to_string();
        self.dispatch(json!({
            "type": "project.create",
            "commandId": uuid::Uuid::new_v4().to_string(),
            "projectId": project_id,
            "title": title,
            "workspaceRoot": path,
            "createWorkspaceRootIfMissing": true,
            "defaultModelSelection": null,
            "createdAt": crate::model::now_iso(),
        }));
        self.select_thread(None);
        self.draft_project_id = Some(project_id);
    }
}

fn relative_ago(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let label = widgets::relative_time(iso, now);
    if label == "now" {
        "just now".into()
    } else {
        format!("{label} ago")
    }
}

fn paint_item(ui: &Ui, row: Rect, item: &Item) {
    let icon_rect = Rect::from_min_size(
        Pos2::new(row.left() + 8.0, row.center().y - 8.0),
        Vec2::splat(16.0),
    );
    match &item.icon {
        Icon::Lucide(name) => theme::paint_icon(ui, name, icon_rect, 16.0, theme::MUTED_FOREGROUND),
        Icon::Project(project) => project_icon::paint(ui, icon_rect, project),
        Icon::Brand(name, color) => theme::paint_icon(ui, name, icon_rect, 16.0, *color),
    }
    let mut right = row.right() - 8.0;
    if item.submenu {
        theme::paint_icon(
            ui,
            "chevron-right",
            Rect::from_min_size(
                Pos2::new(right - 14.0, row.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            16.0,
            theme::alpha(theme::MUTED_FOREGROUND, 0.7),
        );
        right -= 22.0;
    }
    if let Some(shortcut) = item.shortcut {
        let mut job = egui::text::LayoutJob::default();
        let mut format = widgets::format(font(12.0, Weight::Medium), theme::MUTED_FOREGROUND, 16.0);
        format.extra_letter_spacing = 1.2;
        job.append(shortcut, 0.0, format);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let width = galley.size().x - 1.2;
        ui.painter().galley(
            Pos2::new(right - width, row.center().y - 8.0),
            galley,
            theme::MUTED_FOREGROUND,
        );
        right -= width + 12.0;
    }
    if let Some(time) = &item.time {
        let width = widgets::text_width(ui, time, font(12.0, Weight::Regular));
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(right - width, row.top()),
                Pos2::new(right + 1.0, row.bottom()),
            ),
            time,
            font(12.0, Weight::Regular),
            theme::alpha(theme::MUTED_FOREGROUND, 0.7),
        );
        right -= width + 12.0;
    }
    let title_top = row.top() + 6.0;
    let mut x = row.left() + 32.0;
    if let Some((label, color)) = item.status {
        ui.painter()
            .circle_filled(Pos2::new(x + 3.0, title_top + 10.0), 2.5, color);
        let width = widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(x + 10.0, title_top),
                Pos2::new(right, title_top + 20.0),
            ),
            label,
            font(11.0, Weight::Regular),
            color,
        );
        x += 10.0 + width + 6.0;
    }
    let title_width = widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(x, title_top), Pos2::new(right, title_top + 20.0)),
        &item.title,
        font(14.0, Weight::Regular),
        theme::FOREGROUND,
    );
    if let Some(bold) = &item.bold {
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(x + title_width, title_top),
                Pos2::new(right, title_top + 20.0),
            ),
            bold,
            font(14.0, Weight::SemiBold),
            theme::FOREGROUND,
        );
    }
    if let Some(badge) = item.badge {
        let width = widgets::text_width(ui, badge, font(11.0, Weight::Medium)) + 12.0;
        let chip = Rect::from_min_size(
            Pos2::new(right - width, row.center().y - 10.0),
            Vec2::new(width, 20.0),
        );
        ui.painter().rect(
            chip,
            CornerRadius::same(4),
            theme::alpha(theme::WARNING_FOREGROUND, 0.12),
            Stroke::new(1.0, theme::alpha(theme::WARNING_FOREGROUND, 0.5)),
            StrokeKind::Inside,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(chip.min + Vec2::new(6.0, 0.0), chip.max),
            badge,
            font(11.0, Weight::Medium),
            theme::WARNING_FOREGROUND,
        );
    }
    let sub_top = title_top + 20.0;
    let muted = theme::alpha(theme::MUTED_FOREGROUND, 0.7);
    if let Some(meta) = &item.thread {
        // Project · Local · branch · provider glyph (`ThreadCommandSubtitle`).
        let small = font(12.0, Weight::Regular);
        let dot_color = theme::alpha(theme::MUTED_FOREGROUND, 0.5);
        let text = |ui: &Ui, x: f32, value: &str, color: Color32| -> f32 {
            let rect = Rect::from_min_max(Pos2::new(x, sub_top), Pos2::new(right, sub_top + 16.0));
            x + widgets::text_in(ui, rect, value, small.clone(), color)
        };
        let glyph_rect =
            |x: f32| Rect::from_min_size(Pos2::new(x, sub_top + 2.0), Vec2::splat(12.0));
        let mut x = row.left() + 32.0;
        if let Some(project) = &meta.project {
            project_icon::paint(ui, glyph_rect(x), project);
            x = text(ui, x + 16.0, &project.title, muted);
            x = text(ui, x + 4.0, "·", dot_color) + 5.0;
        }
        x = text(ui, x, "Local", muted);
        if let Some(branch) = &meta.branch {
            x = text(ui, x + 3.0, "·", dot_color) + 5.0;
            theme::paint_icon(ui, "folder", glyph_rect(x), 12.0, muted);
            x = text(ui, x + 16.0, branch, muted);
        }
        if let Some((icon, _)) = meta.provider {
            x = text(ui, x + 3.0, "·", dot_color) + 4.0;
            theme::paint_icon(
                ui,
                icon,
                glyph_rect(x),
                12.0,
                theme::alpha(theme::MUTED_FOREGROUND, 0.7),
            );
        }
    } else if let Some(subtitle) = &item.subtitle {
        let rect = Rect::from_min_max(
            Pos2::new(row.left() + 32.0, sub_top),
            Pos2::new(right, sub_top + 16.0),
        );
        widgets::text_in(ui, rect, subtitle, font(12.0, Weight::Regular), muted);
    }
}
