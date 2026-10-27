//! Command palette (`CommandPalette.tsx`, `CommandPalette.logic.ts`): root
//! actions and recent threads, "New thread in…", "Add project" with local
//! folder browsing (`filesystem.browse`), and the "Go to file" mode
//! (`files/ProjectFilePicker.tsx`), and project content search (see
//! `content_search`). Actions whose screens are not ported yet
//! say so when run.
use eframe::egui::{
    self, Color32, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::{Value, json};
use std::sync::mpsc::Receiver;

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
const ROW_DESCRIBED: f32 = 48.0;
const LIST_MAX: f32 = 319.0;
fn surface() -> Color32 {
    theme::shade(12)
}

#[derive(Clone, Debug, PartialEq)]
pub enum View {
    Root,
    NewThreadIn,
    AddProject,
    Browse,
    /// The file picker, a mode of its own: Esc closes it.
    Files,
    /// Project content search, also a mode of its own.
    Content,
    /// The project icon file picker (`ProjectFaviconPickerDialog.tsx`) for
    /// a project id: image files only, Esc closes it.
    Favicon(String),
}

/// The native chooser's answer: (project id, chosen path), `None` on cancel,
/// or why the path cannot be used.
type PickedFavicon = Result<Option<(String, String)>, String>;

#[derive(Default)]
pub struct PaletteState {
    pub open: bool,
    pub query: String,
    pub stack: Vec<View>,
    pub highlight: usize,
    pub focus: bool,
    /// The browse view shows a highlight only after arrow-key navigation.
    pub keyboard_moved: bool,
    /// The native image chooser opened from the favicon picker, while it is
    /// open: the chosen file, `None` on cancel, or why it could not open.
    picking_favicon: Option<Receiver<PickedFavicon>>,
}

impl PaletteState {
    fn view(&self) -> View {
        self.stack.last().cloned().unwrap_or(View::Root)
    }

    /// Modes replace the palette rather than nest in it: no back button.
    fn in_mode(&self) -> bool {
        matches!(self.view(), View::Files | View::Content | View::Favicon(_))
    }
}

#[derive(Clone)]
enum Icon {
    Lucide(&'static str),
    Project(Box<Project>),
    Brand(&'static str, Color32),
    /// A pierre file-type icon for the path.
    File(String),
}

#[derive(Clone)]
enum Action {
    NewThreadIn(String),
    Push(View),
    OpenThread(String),
    BrowseInto(String),
    AddPath(String),
    OpenFile(String),
    /// Sets the project's favicon path: (project id, path).
    SetFavicon(String, String),
    OpenSetting(usize, String),
    Call(fn(&mut App)),
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
    /// The normalized query whose subsequence is emphasized (file rows).
    fuzzy: Option<String>,
    /// Sorts after every other match (keybinding settings).
    secondary: bool,
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
            fuzzy: None,
            secondary: false,
            action,
        }
    }

    fn height(&self) -> f32 {
        if self.thread.is_some() {
            ROW_TALL
        } else if self.subtitle.is_some() {
            ROW_DESCRIBED
        } else {
            ROW
        }
    }
}

struct Group {
    label: String,
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
        self.content_search.reset();
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
            picking_favicon: None,
        };
    }

    fn status_badge(thread: &Thread) -> Option<(&'static str, Color32)> {
        if thread.has_pending_approvals {
            Some(("Pending Approval", theme::warning_foreground()))
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
            .filter(|t| t.archived_at.is_none() && self.in_mode(t))
            .map(std::sync::Arc::as_ref)
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

    /// The visible groups, and the message shown when there are none.
    fn palette_groups(&mut self) -> (Vec<Group>, Option<String>) {
        let query = self.palette.query.trim().to_owned();
        let view = self.palette.view();
        let mut searchable = false;
        let mut empty = None;
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
                if self.scratch_root().is_some() {
                    let mut scratch = Item::new(
                        "message-square-dashed",
                        "New thread without a project",
                        Action::Call(App::start_scratch_thread),
                    );
                    scratch.shortcut = Some("Ctrl+Alt+N");
                    actions.push(scratch);
                }
                let mut file = Item::new("file-search", "Go to file", Action::Push(View::Files));
                file.shortcut = Some("Ctrl+P");
                actions.push(file);
                let mut search = Item::new(
                    "text-search",
                    "Search project contents",
                    Action::Push(View::Content),
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
                    Action::Call(App::open_pull_requests),
                ));
                // Pluto: a design session (`crate::ui::design_home`).
                actions.push(Item::new(
                    "palette",
                    "New design",
                    Action::Call(App::open_design_home),
                ));
                let mut usage = Item::new(
                    "chart-no-axes-column",
                    "Open usage",
                    Action::Call(App::open_usage),
                );
                usage.shortcut = Some("Ctrl+U");
                actions.push(usage);
                actions.push(Item::new(
                    "settings",
                    "Open settings",
                    Action::Call(App::open_settings),
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
                            label: "Actions".into(),
                            items: actions,
                        },
                        Group {
                            label: "Recent Threads".into(),
                            items: self.thread_items(Some(12)),
                        },
                    ]
                } else {
                    let projects = self
                        .state
                        .projects
                        .iter()
                        .filter(|p| !self.is_chats(p))
                        .map(|p| {
                            let mut item =
                                Item::new("folder", &p.title, Action::NewThreadIn(p.id.clone()));
                            item.icon = Icon::Project(Box::new(p.clone()));
                            item.subtitle = Some(format!("Local · {}", p.workspace_root));
                            item.terms.push(normalize(&p.workspace_root));
                            item
                        })
                        .collect();
                    vec![
                        Group {
                            label: "Actions".into(),
                            items: actions,
                        },
                        Group {
                            label: "Projects".into(),
                            items: projects,
                        },
                        Group {
                            label: "Settings".into(),
                            items: crate::ui::settings_search::search(
                                &query,
                                self.auto_settlement_available(),
                            )
                            .into_iter()
                            .map(|found| {
                                let section = crate::ui::settings::PAGES[found.page].0;
                                let mut item = Item::new(
                                    "settings",
                                    &found.title,
                                    Action::OpenSetting(found.page, found.target),
                                );
                                item.subtitle = Some(format!("Settings · {section}"));
                                item.terms.extend(found.fields);
                                item.secondary = found.secondary;
                                item
                            })
                            .collect(),
                        },
                        Group {
                            label: "Threads".into(),
                            items: self.thread_items(None),
                        },
                    ]
                }
            }
            View::NewThreadIn => {
                searchable = true;
                // The Scratch project shows once, as "No project" at the end.
                let mut items: Vec<Item> = self
                    .state
                    .projects
                    .iter()
                    .filter(|p| !self.is_scratch(p) && !self.is_chats(p))
                    .map(|p| {
                        let mut item =
                            Item::new("folder", &p.title, Action::NewThreadIn(p.id.clone()));
                        item.icon = Icon::Project(Box::new(p.clone()));
                        item.subtitle = Some(format!("Local · {}", p.workspace_root));
                        item
                    })
                    .collect();
                if self.scratch_root().is_some() {
                    let mut scratch = Item::new(
                        "message-square-dashed",
                        "No project",
                        Action::Call(App::start_scratch_thread),
                    );
                    scratch.shortcut = Some("Ctrl+Alt+N");
                    items.push(scratch);
                }
                vec![Group {
                    label: "Projects".into(),
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
                github.icon = Icon::Brand("github", theme::foreground());
                github.subtitle = Some("Clone GitHub owner/repo".into());
                vec![Group {
                    label: "Sources".into(),
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
                    label: "Directories".into(),
                    items,
                }]
            }
            View::Content => Vec::new(),
            View::Favicon(project_id) => {
                let target = self
                    .state
                    .project(&project_id)
                    .map(|p| (p.workspace_root.clone(), p.title.clone()));
                let searching = !query.trim().is_empty();
                match target {
                    None => Vec::new(),
                    Some((cwd, project)) => match self.search_files(&cwd, &query, true) {
                        None => {
                            empty = Some(if searching {
                                "Searching project files…".to_owned()
                            } else {
                                "Indexing project files…".to_owned()
                            });
                            Vec::new()
                        }
                        Some(Err(error)) => {
                            empty = Some(error);
                            Vec::new()
                        }
                        Some(Ok(paths)) => {
                            if paths.is_empty() {
                                empty = Some(if searching {
                                    "No matching image files.".to_owned()
                                } else {
                                    "No image files found.".to_owned()
                                });
                            }
                            let fuzzy = fuzzy_query(&query);
                            let items = paths
                                .into_iter()
                                .map(|path| {
                                    let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
                                    let action =
                                        Action::SetFavicon(project_id.clone(), path.clone());
                                    let mut item = Item::new("file", &name, action);
                                    item.icon = Icon::File(path.clone());
                                    item.subtitle = Some(path);
                                    item.fuzzy = (!fuzzy.is_empty()).then(|| fuzzy.clone());
                                    item
                                })
                                .collect();
                            vec![Group {
                                label: project,
                                items,
                            }]
                        }
                    },
                }
            }
            View::Files => {
                // `useActiveProjectTarget`; drafts have no right panel here yet.
                let target = self.current_thread().and_then(|thread| {
                    let project = self.state.project(&thread.project_id)?.title.clone();
                    Some((self.workspace_cwd()?, project))
                });
                let searching = !query.is_empty();
                match target {
                    None => {
                        empty = Some("Open a project to search its files.".to_owned());
                        Vec::new()
                    }
                    Some((cwd, project)) => match self.search_files(&cwd, &query, false) {
                        None => {
                            empty = Some(if searching {
                                "Searching workspace files…".to_owned()
                            } else {
                                "Indexing workspace files…".to_owned()
                            });
                            Vec::new()
                        }
                        Some(Err(error)) => {
                            empty = Some(error);
                            Vec::new()
                        }
                        Some(Ok(paths)) => {
                            if paths.is_empty() {
                                empty = Some(if searching {
                                    "No matching files.".to_owned()
                                } else {
                                    "No files found.".to_owned()
                                });
                            }
                            let fuzzy = fuzzy_query(&query);
                            let items = paths
                                .into_iter()
                                .map(|path| {
                                    let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
                                    let mut item =
                                        Item::new("file", &name, Action::OpenFile(path.clone()));
                                    item.icon = Icon::File(path.clone());
                                    item.subtitle = Some(path);
                                    item.fuzzy = (!fuzzy.is_empty()).then(|| fuzzy.clone());
                                    item
                                })
                                .collect();
                            vec![Group {
                                label: project,
                                items,
                            }]
                        }
                    },
                }
            }
        };
        if query.is_empty() || !searchable {
            return (
                groups.into_iter().filter(|g| !g.items.is_empty()).collect(),
                empty,
            );
        }
        let tokens: Vec<String> = normalize(&query)
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let phrase = tokens.join(" ");
        let filtered = groups
            .into_iter()
            .map(|group| {
                let mut items: Vec<Item> = group
                    .items
                    .into_iter()
                    .filter(|item| {
                        let haystack = item.terms.join(" ");
                        tokens.iter().all(|t| haystack.contains(t.as_str()))
                    })
                    .collect();
                items.sort_by_key(|item| (item.secondary, -match_rank(item, &phrase, &tokens)));
                Group {
                    label: group.label,
                    items,
                }
            })
            .filter(|g| !g.items.is_empty())
            .collect();
        (filtered, empty)
    }

    /// Global shortcuts that open the palette or start threads.
    pub(crate) fn palette_shortcuts(&mut self, ui: &Ui) {
        self.run_shortcuts(ui);
    }

    pub(crate) fn command_palette(&mut self, ui: &mut Ui, screen: Rect) {
        if !self.palette.open {
            return;
        }
        if let Some(picked) = &self.palette.picking_favicon
            && let Ok(picked) = picked.try_recv()
        {
            self.palette.picking_favicon = None;
            match picked {
                Ok(Some((project_id, path))) => {
                    self.run_palette_action(Action::SetFavicon(project_id, path));
                    return;
                }
                Ok(None) => {}
                Err(error) => self.status = Some(format!("Could not open image picker: {error}")),
            }
        }
        let (groups, empty) = self.palette_groups();
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
        let in_submenu = !self.palette.stack.is_empty() && !self.palette.in_mode();
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
        // The empty state (`py-10` around one `text-sm` line), as measured.
        let list_height = if groups.is_empty() && empty.is_some() {
            88.0
        } else {
            list_height
        };
        let view_is_files = matches!(self.palette.view(), View::Files | View::Favicon(_));
        // The file list keeps `CommandList`'s full `p-2` below its rows.
        let bottom_pad = if view_is_files && !groups.is_empty() {
            8.0
        } else {
            4.0
        };
        let height = if self.palette.view() == View::Content {
            420.0 // `h-105`
        } else {
            SEARCH + 4.0 + list_height + bottom_pad + FOOTER
        };
        let rect = Rect::from_min_size(
            Pos2::new(screen.center().x - WIDTH / 2.0, TOP),
            Vec2::new(WIDTH, height),
        );
        let highlight = self.palette.highlight;
        let keyboard_moved = self.palette.keyboard_moved;
        let view = self.palette.view();
        let mut query = std::mem::take(&mut self.palette.query);
        let typed_from = query.clone();
        let focus = std::mem::take(&mut self.palette.focus);
        let mut hovered_row = None;
        let mut clicked_row = None;
        let mut back = false;
        let mut close = false;
        let content_target = self.content_search_target();
        let mut opened = false;
        // `onPickExternal`: only a local server can read a path on this machine.
        let pick_external = match self.palette.view() {
            View::Favicon(project_id) if self.server_is_local() => self
                .state
                .project(&project_id)
                .map(|project| (project_id.clone(), project.workspace_root.clone())),
            _ => None,
        };
        let picking = self.palette.picking_favicon.is_some();
        let mut start_pick = false;
        egui::Area::new(ui.id().with("palette"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                // Backdrop (upstream also blurs the app behind).
                let (backdrop, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if response.clicked() && !ui.rect_contains_pointer(rect) {
                    close = true;
                }
                ui.painter().rect(
                    rect,
                    CornerRadius::same(18),
                    surface(),
                    Stroke::new(1.0, theme::input()),
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
                    theme::paint_icon(ui, "arrow-left", icon_rect, 16.0, theme::foreground());
                } else {
                    theme::paint_icon(ui, "search", icon_rect, 16.0, theme::muted_foreground());
                }
                let mut input_right = search.right() - 8.0;
                if view == View::Content {
                    self.content_search_options(ui, search);
                    input_right = search.right() - 120.0; // `pe-30`
                }
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
                        .rect_filled(kbd, CornerRadius::same(4), theme::muted());
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(kbd.min + Vec2::new(4.0, 0.0), kbd.max),
                        "Enter",
                        font(12.0, Weight::Medium),
                        theme::muted_foreground(),
                    );
                    input_right = add.left() - 8.0;
                }
                let input = Rect::from_min_max(
                    Pos2::new(search.left() + 32.0, search.top() + 8.0),
                    Pos2::new(input_right, search.bottom() - 8.0),
                );
                let placeholder = match view {
                    View::Root => "Search commands, projects, and threads...".to_owned(),
                    View::Browse => "Enter project path (e.g. ~/projects/my-app)".to_owned(),
                    View::Files => "Search files…".to_owned(),
                    View::Favicon(_) => "Search image files…".to_owned(),
                    View::Content => match &content_target {
                        Some((_, project)) => format!("Search in {project}"),
                        None => "Search project contents…".to_owned(),
                    },
                    _ => "Search...".to_owned(),
                };
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(input));
                let response = child.add(
                    egui::TextEdit::singleline(&mut query)
                        .id(egui::Id::new("palette-input"))
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .font(font(14.0, Weight::Regular))
                        .text_color(theme::foreground())
                        .hint_text(
                            egui::RichText::new(placeholder)
                                .font(font(14.0, Weight::Regular))
                                .color(theme::muted_foreground()),
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

                if view == View::Content {
                    let body = Rect::from_min_max(
                        Pos2::new(rect.left() + 1.0, search.bottom() + 6.0),
                        Pos2::new(rect.right() - 1.0, rect.bottom() - FOOTER),
                    );
                    opened = self.content_search_body(ui, body, &query, (up, down, enter));
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
                                &group.label,
                                font(12.0, Weight::Medium),
                                theme::muted_foreground(),
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
                                // Typed queries keep their first result unmarked
                                // (Enter still runs it) until the arrows move.
                                let unmarked = matches!(view, View::Browse | View::Files)
                                    || view == View::Root && !query.trim().is_empty();
                                if index == highlight && (!unmarked || keyboard_moved) {
                                    ui.painter().rect_filled(
                                        row,
                                        CornerRadius::same(6),
                                        theme::accent(),
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
                if groups.is_empty()
                    && let Some(message) = &empty
                {
                    let width = widgets::text_width(ui, message, font(14.0, Weight::Regular));
                    widgets::text_in(
                        ui,
                        Rect::from_min_size(
                            Pos2::new(list.center().x - width / 2.0, list.top() + 31.0),
                            Vec2::new(width + 1.0, 20.0),
                        ),
                        message,
                        font(14.0, Weight::Regular),
                        theme::muted_foreground(),
                    );
                }

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
                    theme::overlay(0.025),
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
                        .rect_filled(chip, CornerRadius::same(4), theme::overlay(0.078));
                    if icon {
                        theme::paint_icon(ui, content, chip, 12.0, theme::foreground());
                    } else {
                        widgets::text_in(
                            ui,
                            Rect::from_min_max(chip.min + Vec2::new(4.0, 0.0), chip.max),
                            content,
                            font(12.0, Weight::Medium),
                            theme::foreground(),
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
                        theme::muted_foreground(),
                    );
                    x + width
                };
                x = key_chip(ui, x, "arrow-up", true) + 4.0;
                x = key_chip(ui, x, "arrow-down", true) + 4.0;
                x = label(ui, x, "Navigate") + 12.0;
                if matches!(view, View::Files | View::Content) {
                    x = key_chip(ui, x, "Enter", false) + 4.0;
                    x = label(ui, x, "Open file") + 12.0;
                } else if matches!(view, View::Favicon(_)) {
                    x = key_chip(ui, x, "Enter", false) + 4.0;
                    x = label(ui, x, "Select icon") + 12.0;
                } else if view != View::Browse {
                    x = key_chip(ui, x, "Enter", false) + 4.0;
                    x = label(ui, x, "Select") + 12.0;
                }
                if in_submenu {
                    x = key_chip(ui, x, "Backspace", false) + 4.0;
                    x = label(ui, x, "Back") + 12.0;
                }
                x = key_chip(ui, x, "Esc", false) + 4.0;
                let escape = if matches!(view, View::Files | View::Content) {
                    "Back"
                } else {
                    "Close"
                };
                label(ui, x, escape);
                // `CommandFooterAction`: "Open in {fileManagerName}".
                if pick_external.is_some() {
                    let text = "Open in Files";
                    let font = font(12.0, Weight::Medium);
                    let width = widgets::text_width(ui, text, font.clone()) + 16.0;
                    let button = Rect::from_min_size(
                        Pos2::new(footer.right() - 16.0 - width, y),
                        Vec2::new(width, 20.0),
                    );
                    let response = ui.interact(
                        button,
                        ui.id().with("favicon-pick-external"),
                        if picking {
                            Sense::hover()
                        } else {
                            Sense::click()
                        },
                    );
                    let color = if picking {
                        theme::alpha(theme::muted_foreground(), 0.64)
                    } else if response.hovered() {
                        theme::foreground()
                    } else {
                        theme::muted_foreground()
                    };
                    if !picking && response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    widgets::text_in(ui, button.shrink2(Vec2::new(8.0, 0.0)), text, font, color);
                    start_pick = response.clicked();
                }
            });
        if start_pick && let Some((project_id, cwd)) = pick_external {
            self.palette.picking_favicon = Some(pick_favicon(ui.ctx(), project_id, &cwd));
        }
        if query != typed_from {
            self.palette.keyboard_moved = false;
            self.palette.highlight = 0;
        }
        self.palette.query = query;
        if let Some(index) = hovered_row {
            self.palette.highlight = index;
        }
        if let Some(index) = clicked_row
            && let Some(item) = rows.get(index)
        {
            run = Some(item.action.clone());
        }
        if close || opened {
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
                if view == View::Content {
                    self.content_search.reset();
                }
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
            Action::OpenFile(path) => {
                self.palette = PaletteState::default();
                self.open_file(path);
            }
            Action::SetFavicon(project_id, path) => {
                self.palette = PaletteState::default();
                self.dispatch(json!({
                    "type": "project.update",
                    "commandId": uuid::Uuid::new_v4().to_string(),
                    "projectId": project_id,
                    "faviconPath": path,
                    "projectIcon": null,
                }));
            }
            Action::OpenSetting(page, target) => {
                self.palette = PaletteState::default();
                self.open_settings();
                self.open_settings_result(page, target);
            }
            Action::Call(open) => {
                self.palette = PaletteState::default();
                open(self);
            }
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
        }));
        self.select_thread(None);
        self.draft_project_id = Some(project_id);
    }
}

/// `rankCommandPaletteItemMatch`: the first search field holding every
/// token ranks the item, earlier fields and closer matches first. Thread
/// titles share a tier so their recency order stands.
fn match_rank(item: &Item, phrase: &str, tokens: &[String]) -> i32 {
    let fields = item.terms.iter().filter(|t| !t.is_empty());
    for (index, field) in fields.enumerate() {
        let field = normalize(field);
        if !tokens.iter().all(|t| field.contains(t.as_str())) {
            continue;
        }
        let field_rank = if field == phrase {
            3
        } else if field.starts_with(phrase) {
            2
        } else if field.contains(phrase) {
            1
        } else {
            0
        };
        if index == 0 && item.thread.is_some() {
            return 1_000 + i32::from(field_rank == 3);
        }
        return 1_000 - index as i32 * 100 + field_rank;
    }
    0
}

pub(crate) fn relative_ago(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
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
        Icon::Lucide(name) => {
            theme::paint_icon(ui, name, icon_rect, 16.0, theme::muted_foreground())
        }
        Icon::Project(project) => project_icon::paint(ui, icon_rect, project),
        Icon::Brand(name, color) => theme::paint_icon(ui, name, icon_rect, 16.0, *color),
        Icon::File(path) => {
            let (name, color) = crate::ui::files::file_icon(path);
            theme::paint_icon(ui, name, icon_rect, 16.0, color);
        }
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
            theme::alpha(theme::muted_foreground(), 0.7),
        );
        right -= 22.0;
    }
    if let Some(shortcut) = item.shortcut {
        let mut job = egui::text::LayoutJob::default();
        let mut format =
            widgets::format(font(12.0, Weight::Medium), theme::muted_foreground(), 16.0);
        format.extra_letter_spacing = 1.2;
        job.append(shortcut, 0.0, format);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let width = galley.size().x - 1.2;
        ui.painter().galley(
            Pos2::new(right - width, row.center().y - 8.0),
            galley,
            theme::muted_foreground(),
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
            theme::alpha(theme::muted_foreground(), 0.7),
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
    let title_rect =
        Rect::from_min_max(Pos2::new(x, title_top), Pos2::new(right, title_top + 20.0));
    let title_width = match &item.fuzzy {
        Some(query) => fuzzy_text(ui, title_rect, &item.title, query, 14.0),
        None => widgets::text_in(
            ui,
            title_rect,
            &item.title,
            font(14.0, Weight::Regular),
            theme::foreground(),
        ),
    };
    if let Some(bold) = &item.bold {
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(x + title_width, title_top),
                Pos2::new(right, title_top + 20.0),
            ),
            bold,
            font(14.0, Weight::SemiBold),
            theme::foreground(),
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
            theme::alpha(theme::warning_foreground(), 0.12),
            Stroke::new(1.0, theme::alpha(theme::warning_foreground(), 0.5)),
            StrokeKind::Inside,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(chip.min + Vec2::new(6.0, 0.0), chip.max),
            badge,
            font(11.0, Weight::Medium),
            theme::warning_foreground(),
        );
    }
    let sub_top = title_top + 20.0;
    let muted = theme::alpha(theme::muted_foreground(), 0.7);
    if let Some(meta) = &item.thread {
        // Project · Local · branch · provider glyph (`ThreadCommandSubtitle`).
        let small = font(12.0, Weight::Regular);
        let dot_color = theme::alpha(theme::muted_foreground(), 0.5);
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
                theme::alpha(theme::muted_foreground(), 0.7),
            );
        }
    } else if let Some(subtitle) = &item.subtitle {
        let rect = Rect::from_min_max(
            Pos2::new(row.left() + 32.0, sub_top),
            Pos2::new(right, sub_top + 16.0),
        );
        match &item.fuzzy {
            Some(query) => {
                fuzzy_text(ui, rect, subtitle, query, 12.0);
            }
            None => {
                widgets::text_in(ui, rect, subtitle, font(12.0, Weight::Regular), muted);
            }
        }
    }
}

/// The file picker's query normalization (`getProjectFilePickerMatches`):
/// trimmed, leading `@./` dropped, lowercased, without whitespace.
fn fuzzy_query(raw: &str) -> String {
    raw.trim()
        .trim_start_matches(['@', '.', '/'])
        .to_lowercase()
        .split_whitespace()
        .collect()
}

/// `findMatchIndices`: the first ordered subsequence of `query` in
/// `value`, as char indices; empty when `value` does not contain it.
fn match_indices(value: &str, query: &str) -> Vec<usize> {
    let query: Vec<char> = query.chars().collect();
    let mut indices = Vec::new();
    for (index, c) in value.to_lowercase().chars().enumerate() {
        if query.get(indices.len()) == Some(&c) {
            indices.push(index);
            if indices.len() == query.len() {
                return indices;
            }
        }
    }
    Vec::new()
}

/// `HighlightedFuzzyText`: muted text with the matched characters in
/// semibold foreground, truncated to `rect`; returns its width.
fn fuzzy_text(ui: &Ui, rect: Rect, value: &str, query: &str, size: f32) -> f32 {
    let line = widgets::line_height_for(size);
    let indices = match_indices(value, query);
    let mut job = egui::text::LayoutJob::default();
    for (index, c) in value.chars().enumerate() {
        let format = if indices.contains(&index) {
            widgets::format(font(size, Weight::SemiBold), theme::foreground(), line)
        } else {
            widgets::format(font(size, Weight::Regular), theme::muted_foreground(), line)
        };
        job.append(c.encode_utf8(&mut [0; 4]), 0.0, format);
    }
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width());
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let width = galley.size().x;
    let pos = Pos2::new(rect.left(), (rect.center().y - line / 2.0).round());
    ui.painter().galley(pos, galley, theme::muted_foreground());
    width
}

/// `PROJECT_FAVICON_EXTENSIONS`, without the dot.
const FAVICON_EXTENSIONS: [&str; 9] = [
    "avif", "gif", "ico", "jpeg", "jpg", "png", "svg", "webp", "icns",
];

/// `desktopBridge.pickProjectFavicon`: the portal's file chooser, off the UI
/// thread, starting in the project's folder. Any image on this machine may
/// be chosen; the server serves one outside the project as an external
/// override.
fn pick_favicon(ctx: &egui::Context, project_id: String, cwd: &str) -> Receiver<PickedFavicon> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    let cwd = cwd.to_owned();
    std::thread::spawn(move || {
        let picked = rfd::FileDialog::new()
            .set_title("Choose project icon")
            .set_directory(&cwd)
            .add_filter("Images", &FAVICON_EXTENSIONS)
            .pick_file();
        // rfd cannot tell a cancel from a missing portal (it loads libdbus
        // at runtime), so both read as no choice.
        let picked = picked
            .map(|path| match path.to_str() {
                Some(path) => Ok((project_id, path.to_owned())),
                None => Err("the image path is not valid UTF-8.".to_owned()),
            })
            .transpose();
        let _ = sender.send(picked);
        ctx.request_repaint();
    });
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_queries_normalize_like_upstream() {
        assert_eq!(fuzzy_query("  ./Src App "), "srcapp");
        assert_eq!(fuzzy_query("@README"), "readme");
    }

    #[test]
    fn match_indices_take_the_first_subsequence() {
        assert_eq!(match_indices("src/app.rs", "sapp"), vec![0, 4, 5, 6]);
        assert_eq!(match_indices("README.md", "rdm"), vec![0, 3, 4]);
        assert!(match_indices("theme.rs", "xyz").is_empty());
        assert!(match_indices("theme.rs", "").is_empty());
    }
}
