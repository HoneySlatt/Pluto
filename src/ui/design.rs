//! Pluto's Design surface: a right-panel tab for the pages the thread's
//! agents publish with `html_render` (`crate::ui::html_render`), in the
//! spirit of claude.ai's artifacts. Upstream has no such surface; it opens a
//! page full screen (`HtmlRenderDialog`). The panel's maximize control
//! (`RightPanelMaximizeControl`) gives the surface the chat's width too.
//!
//! Pages with the same title are versions of one design. The surface
//! follows the newest page until one is pinned (from a chat page's "Open in
//! Design" button, the version arrows or the design menu). The page fills
//! the surface at full width or at a tablet or phone width, scrolls inside
//! itself and takes the pointer and keys, through one preview-browser tab
//! that navigates as the shown page changes.
//!
//! In a design project (a workspace with `.pluto/design.json`, see
//! `crate::ui::design_home`), the HTML files the agents write are pages too,
//! loaded through their signed `workspace-file` URL (which also serves the
//! files beside and below them) and reloaded each time an agent changes
//! them, and the thread opens in the studio layout: the surface beside a
//! narrow chat, after OpenDesign's Studio.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::{App, PanelTab};
use crate::html_render::HtmlRender;
use crate::theme::{self, Weight, font};
use crate::ui::widgets::{self, MenuEntry, MenuItem};

/// `PreviewChromeRow`'s height and the gap below it, as in the Browser.
const CHROME: f32 = 28.0;
const CHROME_GAP: f32 = 12.0;
const MENU: &str = "design-pages";
/// A project file's page id: this prefix, then its absolute path.
const FILE_PREFIX: &str = "file:";
/// The studio's chat column, until the user drags the panel's edge.
pub(crate) const STUDIO_CHAT: f32 = 420.0;

/// The width the page is laid out at.
#[derive(Clone, Copy, Default, PartialEq)]
enum Width {
    #[default]
    Full,
    Tablet,
    Phone,
}

impl Width {
    const ALL: [(Width, &'static str, &'static str); 3] = [
        (Width::Full, "monitor", "Full width"),
        (Width::Tablet, "tablet", "Tablet (768px)"),
        (Width::Phone, "smartphone", "Phone (390px)"),
    ];

    fn css(self) -> Option<f32> {
        match self {
            Width::Full => None,
            Width::Tablet => Some(768.0),
            Width::Phone => Some(390.0),
        }
    }
}

#[derive(Default)]
pub(crate) struct DesignState {
    /// The page each thread pins, by attachment id; none follows the newest.
    pinned: HashMap<String, String>,
    width: Width,
    /// The surface's preview-browser tab and the page it loaded.
    tab: Option<u64>,
    loaded: Option<String>,
    /// The URL the tab loaded, so a reloaded file navigates again.
    loaded_url: Option<String>,
    /// Drawn this frame, and whether the tab is casting.
    drawn: bool,
    shown: bool,
    /// Workspaces known to be design projects or not (`None` while asked).
    projects: HashMap<String, Option<bool>>,
    /// Threads already opened in the studio layout once.
    studio_threads: HashSet<String>,
    /// The studio's chat width, as the user dragged it (0 until then).
    pub(crate) studio_chat: f32,
    /// Project files' signed URLs by absolute path, with the change they
    /// were signed after.
    files: HashMap<String, FileUrl>,
    /// The checkpoint each project file shows, by absolute path; none
    /// shows the file as it is now.
    file_version: HashMap<String, String>,
    /// File contents at checkpoints, by `<ref>\0<path>`; `None` while asked.
    versions: HashMap<String, Option<Result<String, String>>>,
    /// The version the tab shows in place of its document, by the same key.
    shown_version: Option<String>,
    /// Comment mode: the pointer picks elements instead of using the page.
    commenting: bool,
    /// The last point inspected while commenting, in CSS pixels.
    inspected_at: Option<Pos2>,
}

#[derive(Default)]
struct FileUrl {
    /// The change the newest request answers.
    marker: String,
    url: Option<Result<String, ()>>,
}

/// An HTML file a `file_change` item touched, absolute.
fn changed_html_files(item: &crate::model::TurnItem, cwd: &str) -> Vec<String> {
    let mut paths: Vec<String> = item
        .get("changes")
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["path"].as_str().map(str::to_owned))
        .collect();
    if let Some(file) = item.str("fileName") {
        paths.push(file.to_owned());
    }
    let cwd = cwd.trim_end_matches('/');
    let mut out: Vec<String> = Vec::new();
    for path in paths {
        let absolute = if path.starts_with('/') {
            path
        } else {
            format!("{cwd}/{}", path.trim_start_matches("./"))
        };
        let lower = absolute.to_ascii_lowercase();
        if (lower.ends_with(".html") || lower.ends_with(".htm"))
            && absolute.starts_with(&format!("{cwd}/"))
            && !out.contains(&absolute)
        {
            out.push(absolute);
        }
    }
    out
}

impl App {
    /// The thread's pages, oldest first: those published with
    /// `html_render`, and in a design project the HTML files agents
    /// changed, each where it last changed. A file's page id is
    /// `FILE_PREFIX` and its path, its title the project-relative path.
    pub(crate) fn thread_pages(&self) -> Vec<HtmlRender> {
        let Some(detail) = self.current_detail() else {
            return Vec::new();
        };
        let cwd = self
            .workspace_cwd()
            .filter(|cwd| self.design.projects.get(cwd) == Some(&Some(true)));
        let mut pages: Vec<HtmlRender> = Vec::new();
        for row in &detail.visible {
            if let Some(render) = crate::html_render::from_tool_item(&row.item) {
                pages.push(render);
                continue;
            }
            let Some(cwd) = &cwd else { continue };
            if row.item.kind != "file_change" {
                continue;
            }
            for path in changed_html_files(&row.item, cwd) {
                let id = format!("{FILE_PREFIX}{path}");
                pages.retain(|p| p.attachment_id != id);
                let title = path
                    .strip_prefix(&format!("{}/", cwd.trim_end_matches('/')))
                    .unwrap_or(&path)
                    .to_owned();
                pages.push(HtmlRender {
                    attachment_id: id,
                    title,
                    height: 0.0,
                    heights: Vec::new(),
                });
            }
        }
        pages
    }

    /// The newest change to a project file: its `file_change` item and time.
    fn file_marker(&self, path: &str) -> String {
        let (Some(detail), Some(cwd)) = (self.current_detail(), self.workspace_cwd()) else {
            return String::new();
        };
        detail
            .visible
            .iter()
            .rev()
            .find(|row| {
                row.item.kind == "file_change"
                    && changed_html_files(&row.item, &cwd)
                        .iter()
                        .any(|p| p == path)
            })
            .map(|row| format!("{}@{}", row.item.id, row.item.updated_at))
            .unwrap_or_default()
    }

    /// Asks for a page's URL: a published page's once, a project file's
    /// again after each change.
    fn request_page_url(&mut self, page: &HtmlRender) {
        let Some(path) = page.attachment_id.strip_prefix(FILE_PREFIX) else {
            self.request_html_render_url(page);
            return;
        };
        let marker = self.file_marker(path);
        let known = self.design.files.get(path).map(|f| f.marker.clone());
        if known.as_deref() == Some(marker.as_str()) {
            return;
        }
        let Some(thread) = self.selected.clone() else {
            return;
        };
        // A new change shows the file as it is now.
        self.design.file_version.remove(path);
        let entry = self.design.files.entry(path.to_owned()).or_default();
        entry.marker = marker.clone();
        self.send_request(
            format!("design-file:{path}\u{0}{marker}"),
            "assets.createUrl",
            json!({"resource": {"_tag": "workspace-file", "threadId": thread, "path": path}}),
        );
    }

    /// A page's URL, as `html_render_url` answers; a project file keeps its
    /// previous URL until the one after its latest change arrives.
    fn page_url(&self, page: &HtmlRender) -> Option<Result<String, ()>> {
        match page.attachment_id.strip_prefix(FILE_PREFIX) {
            Some(path) => self.design.files.get(path)?.url.clone(),
            None => self.html_render_url(&page.attachment_id),
        }
    }

    /// A project file's versions: the ready checkpoints that captured a
    /// change to it, oldest first, as (checkpoint id, Git ref).
    fn file_checkpoints(&self, relative: &str) -> Vec<(String, String)> {
        let Some(detail) = self.current_detail() else {
            return Vec::new();
        };
        let mut checkpoints: Vec<&crate::model::Checkpoint> = detail
            .checkpoints
            .iter()
            .filter(|c| {
                c.status == "ready"
                    && !c.git_ref.is_empty()
                    && c.files.iter().any(|f| f.path == relative)
            })
            .collect();
        checkpoints.sort_by(|a, b| a.captured_at.cmp(&b.captured_at));
        checkpoints
            .into_iter()
            .map(|c| (c.id.clone(), c.git_ref.clone()))
            .collect()
    }

    /// Marks a workspace as a design project (it was just made one).
    pub(crate) fn mark_design_project(&mut self, cwd: &str) {
        self.design.projects.insert(cwd.to_owned(), Some(true));
    }

    /// Whether the open thread works in a design project, asking the server
    /// for the workspace's profile the first time.
    pub(crate) fn studio_active(&mut self) -> bool {
        let Some(cwd) = self.workspace_cwd() else {
            return false;
        };
        if self.current_thread().is_none() {
            return false;
        }
        match self.design.projects.get(&cwd) {
            Some(known) => *known == Some(true),
            None => {
                self.design.projects.insert(cwd.clone(), None);
                self.send_request(
                    format!("design-profile:{cwd}"),
                    "projects.readFile",
                    json!({"cwd": cwd, "relativePath": ".pluto/design.json"}),
                );
                false
            }
        }
    }

    /// Opens a design project's thread in the studio layout the first time
    /// it shows: the panel on the Design surface, beside a narrow chat.
    pub(crate) fn apply_studio(&mut self) {
        let Some(thread) = self.selected.clone() else {
            return;
        };
        if self.design.studio_threads.contains(&thread) || !self.studio_active() {
            return;
        }
        self.design.studio_threads.insert(thread);
        self.open_design(None);
    }

    /// The studio's panel width for a workspace this wide.
    pub(crate) fn studio_panel_width(&self, workspace: f32) -> f32 {
        let chat = if self.design.studio_chat > 0.0 {
            self.design.studio_chat
        } else {
            STUDIO_CHAT
        };
        (workspace - chat).clamp(360.0, (workspace - 360.0).max(360.0))
    }

    /// The design requests' answers; true when `key` was one of them.
    pub(crate) fn design_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        if let Some(cwd) = key.strip_prefix("design-profile:") {
            let design = result.as_ref().is_ok_and(|value| {
                value["contents"]
                    .as_str()
                    .is_some_and(|contents| contents.contains("\"version\""))
            });
            self.design.projects.insert(cwd.to_owned(), Some(design));
            return true;
        }
        if let Some(version) = key.strip_prefix("design-version:") {
            let contents = result
                .as_ref()
                .map(|value| value["oldContents"].as_str().unwrap_or_default().to_owned())
                .map_err(Clone::clone);
            self.design
                .versions
                .insert(version.to_owned(), Some(contents));
            return true;
        }
        let Some(rest) = key.strip_prefix("design-file:") else {
            return false;
        };
        let (path, marker) = rest.split_once('\u{0}').unwrap_or((rest, ""));
        let base = url::Url::parse(self.connect.server.trim()).ok();
        let url = result.as_ref().ok().and_then(|value| {
            let relative = value["relativeUrl"].as_str()?;
            Some(base?.join(relative).ok()?.to_string())
        });
        if let Some(entry) = self.design.files.get_mut(path)
            && entry.marker == marker
        {
            entry.url = Some(url.ok_or(()));
        }
        true
    }

    /// Opens the Design surface, pinned to `page` or following the newest.
    pub(crate) fn open_design(&mut self, page: Option<String>) {
        if let Some(thread) = self.selected.clone() {
            match page {
                Some(page) => self.design.pinned.insert(thread, page),
                None => self.design.pinned.remove(&thread),
            };
        }
        if let Some(panel) = self.panel_mut() {
            panel.open = true;
            panel.show(PanelTab::Design, |t| *t == PanelTab::Design);
        }
    }

    /// The Design surface below the panel's tab bar.
    pub(crate) fn design_surface(&mut self, ui: &mut Ui, body: Rect) {
        let pages = self.thread_pages();
        let thread = self.selected.clone().unwrap_or_default();
        let pinned = self.design.pinned.get(&thread).cloned();
        let current = pinned
            .as_ref()
            .and_then(|id| pages.iter().position(|p| &p.attachment_id == id))
            .or_else(|| pages.len().checked_sub(1));
        let Some(current) = current else {
            empty_state(ui, body);
            return;
        };
        let page = pages[current].clone();
        // The design's versions: its pages with the same title, or for a
        // project file the checkpoints that captured it (the newest one is
        // the file as it is now).
        let file_path = page
            .attachment_id
            .strip_prefix(FILE_PREFIX)
            .map(str::to_owned);
        let checkpoints = if file_path.is_some() {
            self.file_checkpoints(&page.title)
        } else {
            Vec::new()
        };
        let versions: Vec<usize> = (0..pages.len())
            .filter(|&i| pages[i].title == page.title)
            .collect();
        let shown_checkpoint = file_path
            .as_ref()
            .and_then(|path| self.design.file_version.get(path))
            .and_then(|id| checkpoints.iter().position(|(c, _)| c == id));
        let (version, version_count) = if checkpoints.len() > 1 {
            (
                shown_checkpoint.unwrap_or(checkpoints.len() - 1),
                checkpoints.len(),
            )
        } else {
            (
                versions.iter().position(|&i| i == current).unwrap_or(0),
                versions.len(),
            )
        };
        // A file version to show: Some(checkpoint) or Some(None) for now.
        let mut select_file_version: Option<Option<String>> = None;

        let chrome = Rect::from_min_size(
            Pos2::new(body.left() + 8.0, body.top()),
            Vec2::new(body.width() - 16.0, CHROME),
        );
        let mut select: Option<Option<String>> = None;
        let mut x = chrome.left();
        for (icon, tip, step) in [
            ("chevron-left", "Previous version", -1i64),
            ("chevron-right", "Next version", 1i64),
        ] {
            let button = Rect::from_min_size(Pos2::new(x, chrome.top()), Vec2::splat(CHROME));
            x += CHROME;
            let target = usize::try_from(version as i64 + step)
                .ok()
                .filter(|&v| v < version_count);
            match target {
                Some(target) if checkpoints.len() > 1 => {
                    if widgets::icon_button(
                        ui,
                        button,
                        ("design-version", icon),
                        icon,
                        16.0,
                        theme::muted_foreground(),
                        tip,
                    )
                    .clicked()
                    {
                        select_file_version = Some(
                            (target + 1 < checkpoints.len()).then(|| checkpoints[target].0.clone()),
                        );
                    }
                }
                Some(target) => {
                    let target = versions[target];
                    if widgets::icon_button(
                        ui,
                        button,
                        ("design-version", icon),
                        icon,
                        16.0,
                        theme::muted_foreground(),
                        tip,
                    )
                    .clicked()
                    {
                        select = Some(Some(pages[target].attachment_id.clone()));
                    }
                }
                None => theme::paint_icon(
                    ui,
                    icon,
                    button,
                    16.0,
                    theme::alpha(theme::muted_foreground(), 0.4),
                ),
            }
        }

        // Right side: widths, pin, reload, open in the browser.
        let mut right = chrome.right();
        // The next button leftward, 2px apart.
        let button = |right: &mut f32| {
            *right -= CHROME;
            let rect = Rect::from_min_size(Pos2::new(*right, chrome.top()), Vec2::splat(CHROME));
            *right -= 2.0;
            rect
        };
        let open = button(&mut right);
        let reload = button(&mut right);
        let pin = button(&mut right);
        let comment = button(&mut right);
        right -= 6.0;
        let width_buttons: Vec<(Width, &str, &str, Rect)> = Width::ALL
            .iter()
            .rev()
            .map(|(width, icon, tip)| (*width, *icon, *tip, button(&mut right)))
            .collect();
        let url = self.page_url(&page);
        if widgets::icon_button(
            ui,
            open,
            "design-open",
            "external-link",
            16.0,
            theme::muted_foreground(),
            "Open in browser",
        )
        .clicked()
            && let Some(Ok(url)) = &url
        {
            widgets::open_external(url);
        }
        if widgets::icon_button(
            ui,
            reload,
            "design-reload",
            "refresh-cw",
            16.0,
            theme::muted_foreground(),
            "Reload",
        )
        .clicked()
            && let (Some(engine), Some(tab)) = (&self.preview.engine, self.design.tab)
        {
            engine.reload(tab);
        }
        // Comment mode (OpenDesign's comment tool): a click on an element
        // adds it to the message as an element reference.
        if self.design.commenting {
            ui.painter()
                .rect_filled(comment, CornerRadius::same(8), theme::accent());
        }
        if widgets::icon_button(
            ui,
            comment,
            "design-comment",
            "message-square-plus",
            16.0,
            if self.design.commenting {
                theme::foreground()
            } else {
                theme::muted_foreground()
            },
            if self.design.commenting {
                "Stop commenting"
            } else {
                "Comment on an element"
            },
        )
        .clicked()
        {
            self.design.commenting = !self.design.commenting;
            self.design.inspected_at = None;
        }
        if pinned.is_some() {
            ui.painter()
                .rect_filled(pin, CornerRadius::same(8), theme::accent());
        }
        let pin_tip = if pinned.is_some() {
            "Pinned: follow the newest page"
        } else {
            "Pin this page"
        };
        if widgets::icon_button(
            ui,
            pin,
            "design-pin",
            "pin",
            16.0,
            if pinned.is_some() {
                theme::foreground()
            } else {
                theme::muted_foreground()
            },
            pin_tip,
        )
        .clicked()
        {
            select = Some(match pinned {
                Some(_) => None,
                None => Some(page.attachment_id.clone()),
            });
        }
        for (width, icon, tip, rect) in width_buttons {
            let active = self.design.width == width;
            if active {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), theme::accent());
            }
            if widgets::icon_button(
                ui,
                rect,
                ("design-width", icon),
                icon,
                16.0,
                if active {
                    theme::foreground()
                } else {
                    theme::muted_foreground()
                },
                tip,
            )
            .clicked()
            {
                self.design.width = width;
            }
        }

        // The title opens the menu of the thread's designs.
        let title_rect = Rect::from_min_max(
            Pos2::new(x + 6.0, chrome.top()),
            Pos2::new(right - 6.0, chrome.bottom()),
        );
        let designs: Vec<usize> = (0..pages.len())
            .rev()
            .filter(|&i| !pages[i + 1..].iter().any(|p| p.title == pages[i].title))
            .collect();
        let title_response = ui.interact(title_rect, ui.id().with("design-title"), Sense::click());
        let title_width = widgets::text_in(
            ui,
            title_rect,
            &page.title,
            font(13.0, Weight::Medium),
            theme::foreground(),
        );
        let mut after = title_rect.left() + title_width + 6.0;
        if version_count > 1 {
            let label = format!("v{} of {}", version + 1, version_count);
            after += widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(after, title_rect.top()), title_rect.max),
                &label,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            ) + 4.0;
        }
        if designs.len() > 1 {
            theme::paint_icon(
                ui,
                "chevron-down",
                Rect::from_min_size(
                    Pos2::new(after, title_rect.center().y - 7.0),
                    Vec2::splat(14.0),
                ),
                14.0,
                theme::muted_foreground(),
            );
            if title_response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if title_response.clicked() {
                self.open_menu = Some(MENU.into());
            }
            let entries: Vec<MenuEntry> = designs
                .iter()
                .map(|&i| {
                    MenuEntry::Item(MenuItem {
                        icon: Some(if pages[i].title == page.title {
                            "check"
                        } else {
                            "palette"
                        }),
                        icon_size: 16.0,
                        ..MenuItem::new(&pages[i].title)
                    })
                })
                .collect();
            if let Some(index) =
                widgets::menu(ui, &mut self.open_menu, MENU, title_rect, false, &entries)
            {
                select = Some(Some(pages[designs[index]].attachment_id.clone()));
            }
        }

        if let Some(choice) = select {
            match choice {
                Some(id) => self.design.pinned.insert(thread.clone(), id),
                None => self.design.pinned.remove(&thread),
            };
        }
        if let (Some(choice), Some(path)) = (select_file_version, &file_path) {
            match choice {
                Some(id) => self.design.file_version.insert(path.clone(), id),
                None => self.design.file_version.remove(path),
            };
        }
        // An earlier version of a file: its contents at that checkpoint.
        let old_version = file_path.as_ref().and_then(|path| {
            let id = self.design.file_version.get(path)?;
            let (_, git_ref) = checkpoints.iter().find(|(c, _)| c == id)?;
            Some((format!("{git_ref}\u{0}{}", page.title), git_ref.clone()))
        });
        if let Some((key, git_ref)) = &old_version
            && !self.design.versions.contains_key(key)
            && let Some(cwd) = self.workspace_cwd()
        {
            self.design.versions.insert(key.clone(), None);
            self.send_request(
                format!("design-version:{key}"),
                "review.getDiffFileContents",
                json!({
                    "cwd": cwd,
                    "sourceKind": "working-tree",
                    "changeType": "change",
                    "baseRef": git_ref,
                    "headRef": null,
                    "oldPath": page.title,
                    "newPath": page.title,
                }),
            );
        }

        // The page.
        let area = Rect::from_min_max(
            Pos2::new(body.left(), chrome.bottom() + CHROME_GAP),
            body.max,
        );
        let width = self
            .design
            .width
            .css()
            .map_or(area.width(), |w| w.min(area.width()));
        let frame = Rect::from_min_size(
            Pos2::new((area.center().x - width / 2.0).round(), area.top()),
            Vec2::new(width.round(), area.height()),
        );
        if self.design.width != Width::Full {
            ui.painter().rect_filled(area, 0.0, theme::overlay(0.02));
            ui.painter().rect_stroke(
                frame.expand(1.0),
                0.0,
                Stroke::new(1.0, theme::border()),
                StrokeKind::Inside,
            );
        }
        ui.painter().rect_filled(frame, 0.0, theme::background());
        self.request_page_url(&page);
        let note = |ui: &Ui, text: &str| {
            let text_width = widgets::text_width(ui, text, font(13.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_center_size(frame.center(), Vec2::new(text_width + 1.0, 18.0)),
                text,
                font(13.0, Weight::Regular),
                theme::muted_foreground(),
            );
        };
        let url = match self.page_url(&page) {
            Some(Ok(url)) => url,
            Some(Err(())) => {
                note(ui, &format!("Unable to load {}", page.title));
                return;
            }
            None => {
                note(ui, "Loading…");
                return;
            }
        };
        let ctx = ui.ctx().clone();
        let failure = self
            .preview_engine(&ctx)
            .failure
            .lock()
            .ok()
            .and_then(|f| f.clone());
        if let Some(failure) = failure {
            note(ui, &failure);
            return;
        }
        let (tab, fresh) = match self.design.tab {
            Some(tab) => (tab, false),
            None => {
                self.preview.next_tab += 1;
                let tab = self.preview.next_tab;
                self.design.tab = Some(tab);
                self.design.shown = true;
                (tab, true)
            }
        };
        let navigate = self.design.loaded.as_deref() != Some(page.attachment_id.as_str())
            || self.design.loaded_url.as_deref() != Some(url.as_str());
        self.design.loaded = Some(page.attachment_id.clone());
        self.design.loaded_url = Some(url.clone());
        self.design.drawn = true;
        let Some(engine) = &self.preview.engine else {
            return;
        };
        // Back to the file as it is now: its own document again.
        let back_to_now = old_version.is_none() && self.design.shown_version.is_some();
        if fresh {
            engine.open_render(tab, url);
        } else if navigate || back_to_now {
            engine.navigate(tab, url);
        }
        if navigate || back_to_now {
            self.design.shown_version = None;
        }
        engine.resize(
            tab,
            frame.width().floor().max(1.0) as u32,
            frame.height().floor().max(1.0) as u32,
            ui.ctx().pixels_per_point(),
        );
        for link in engine.take_links(tab) {
            widgets::open_external(&link);
        }
        let picked = engine.take_picked(tab);
        let state = engine.tab(tab).unwrap_or_default();
        // An earlier version replaces the loaded document, so its relative
        // assets still resolve against the file's URL.
        if let Some((key, _)) = &old_version
            && self.design.shown_version.as_ref() != Some(key)
            && !state.loading
            && state.frame.is_some()
        {
            match self.design.versions.get(key) {
                Some(Some(Ok(html))) => {
                    engine.set_content(tab, html.clone());
                    self.design.shown_version = Some(key.clone());
                }
                Some(Some(Err(error))) => {
                    note(ui, &format!("Could not read this version: {error}"));
                    return;
                }
                _ => {}
            }
        }
        if let Some(error) = &state.error {
            note(ui, error);
            return;
        }
        for element in picked {
            if let Some(record) = element_record(&element, &page) {
                self.add_composer_context(ui.ctx(), record);
            }
        }
        let mut page_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(frame)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        page_ui.set_clip_rect(frame.intersect(ui.clip_rect()));
        let commenting = self.design.commenting;
        if !self.page_view(&mut page_ui, frame, tab, &state, true, !commenting) || state.loading {
            if state.frame.is_none() {
                note(ui, "Loading…");
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
        if commenting {
            self.comment_overlay(ui, frame, tab, &state);
        }
    }

    /// Comment mode over the page: the element under the pointer outlined
    /// with its tag, and a click adding it to the message.
    fn comment_overlay(
        &mut self,
        ui: &mut Ui,
        frame: Rect,
        tab: u64,
        state: &crate::preview::TabState,
    ) {
        let response = ui.interact(frame, ui.id().with("design-comment-layer"), Sense::click());
        let Some(engine) = &self.preview.engine else {
            return;
        };
        let pointer = response.hover_pos().map(|pos| (pos - frame.min).to_pos2());
        if let Some(at) = pointer {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            if self.design.inspected_at != Some(at) {
                self.design.inspected_at = Some(at);
                engine.inspect(tab, at.x, at.y, false);
            }
            if response.clicked() {
                engine.inspect(tab, at.x, at.y, true);
            }
        }
        let Some(element) = state.inspected.as_ref().filter(|_| pointer.is_some()) else {
            return;
        };
        let number = |key: &str| element[key].as_f64().unwrap_or(0.0) as f32;
        let outline = Rect::from_min_size(
            frame.min + Vec2::new(number("x"), number("y")),
            Vec2::new(number("width"), number("height")),
        );
        let painter = ui.painter_at(frame);
        let color = theme::primary();
        painter.rect_filled(outline, 2.0, theme::alpha(color, 0.12));
        painter.rect_stroke(outline, 2.0, Stroke::new(2.0, color), StrokeKind::Outside);
        let tag = element["tag"].as_str().unwrap_or_default();
        let tag_width = widgets::text_width(ui, tag, font(11.0, Weight::Medium)) + 10.0;
        let chip_top = if outline.top() - 20.0 >= frame.top() {
            outline.top() - 20.0
        } else {
            outline.top() + 2.0
        };
        let chip = Rect::from_min_size(
            Pos2::new(outline.left().max(frame.left()), chip_top),
            Vec2::new(tag_width, 18.0),
        );
        painter.rect_filled(chip, CornerRadius::same(4), color);
        painter.text(
            chip.center(),
            egui::Align2::CENTER_CENTER,
            tag,
            font(11.0, Weight::Medium),
            theme::primary_foreground(),
        );
    }

    /// Pauses the surface's page while it is not shown.
    pub(crate) fn design_end_frame(&mut self) {
        let drawn = std::mem::take(&mut self.design.drawn);
        if let (Some(engine), Some(tab)) = (&self.preview.engine, self.design.tab)
            && drawn != self.design.shown
        {
            engine.set_visible(tab, drawn);
            self.design.shown = drawn;
        }
    }
}

/// No page yet: the palette, a title and what shows here.
fn empty_state(ui: &Ui, body: Rect) {
    let center = body.center();
    let media = Rect::from_center_size(Pos2::new(center.x, center.y - 48.0), Vec2::splat(36.0));
    ui.painter().rect(
        media,
        CornerRadius::same(8),
        theme::card(),
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );
    theme::paint_icon(ui, "palette", media, 18.0, theme::muted_foreground());
    let title = "No designs yet";
    let title_width = widgets::text_width(ui, title, font(16.0, Weight::SemiBold));
    widgets::text_in(
        ui,
        Rect::from_min_size(
            Pos2::new(center.x - title_width / 2.0, media.bottom() + 20.0),
            Vec2::new(title_width + 1.0, 24.0),
        ),
        title,
        font(16.0, Weight::SemiBold),
        theme::foreground(),
    );
    let mut job = egui::text::LayoutJob::single_section(
        "Pages the agent publishes with html_render show here, newest first. Ask it for a mockup, a chart or a page to iterate on.".to_owned(),
        widgets::format(font(13.0, Weight::Regular), theme::muted_foreground(), 20.0),
    );
    job.wrap.max_width = 340.0f32.min(body.width() - 48.0);
    job.halign = egui::Align::Center;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(
        Pos2::new(center.x, media.bottom() + 20.0 + 24.0 + 4.0),
        galley,
        theme::muted_foreground(),
    );
}

/// An `element` context record (`ElementContextRecord`) for an element
/// picked on `page`: the agent gets the page's project path (or published
/// page's title), the selector, the HTML and the notable styles.
fn element_record(element: &Value, page: &HtmlRender) -> Option<Value> {
    let tag = element["tag"].as_str().filter(|t| !t.is_empty())?;
    let text = element["text"].as_str().unwrap_or_default();
    let selector = element["selector"].as_str().filter(|s| !s.is_empty());
    let label = if text.is_empty() {
        selector.map_or_else(|| format!("<{tag}>"), str::to_owned)
    } else {
        let short: String = text.chars().take(40).collect();
        let ellipsis = if text.chars().count() > 40 { "…" } else { "" };
        format!("{tag} \u{201c}{short}{ellipsis}\u{201d}")
    };
    let page_url = if page.attachment_id.starts_with(FILE_PREFIX) {
        page.title.clone()
    } else {
        format!("html_render page \u{201c}{}\u{201d}", page.title)
    };
    let id = format!("element_{}", uuid::Uuid::new_v4().simple());
    Some(json!({
        "version": 1,
        "contextId": id,
        "kind": "element",
        "label": crate::ui::composer_context::sanitize_label(&label, "element"),
        "pageUrl": page_url,
        "pageTitle": element["title"].as_str().map(str::to_owned).or(Some(page.title.clone())),
        "tagName": tag,
        "selector": selector,
        "htmlPreview": element["html"].as_str().unwrap_or_default(),
        "componentName": null,
        "source": null,
        "styles": element["styles"].as_str().unwrap_or_default(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picked_elements_become_element_records() {
        let element = json!({"tag": "div", "text": "Pro 12 € per month, billed yearly for teams",
            "selector": "section.plans > div.card.pro", "html": "<div class=\"card pro\">…</div>",
            "styles": "border-radius: 12px;", "title": "Pricing"});
        let file = HtmlRender {
            attachment_id: format!("{FILE_PREFIX}/designs/p/pricing-page.html"),
            title: "pricing-page.html".into(),
            height: 0.0,
            heights: Vec::new(),
        };
        let record = element_record(&element, &file).unwrap();
        assert_eq!(record["kind"], "element");
        assert_eq!(record["pageUrl"], "pricing-page.html");
        assert_eq!(
            record["label"],
            "div \u{201c}Pro 12 € per month, billed yearly for te…\u{201d}"
        );
        assert!(
            record["contextId"]
                .as_str()
                .unwrap()
                .starts_with("element_")
        );
        assert!(element_record(&json!({"tag": ""}), &file).is_none());
        let legacy = crate::ui::composer_context::legacy_text("Make it stand out", &[record]);
        assert!(legacy.contains("<element_context>\n- <div>:\n  url: pricing-page.html\n  selector: section.plans > div.card.pro"));
    }

    #[test]
    fn changed_html_files_resolve_against_the_project() {
        let item: crate::model::TurnItem = serde_json::from_value(json!({
            "id": "f", "threadId": "t", "ordinal": 0, "status": "completed",
            "updatedAt": "2026-10-27T00:00:00Z", "type": "file_change",
            "fileName": "/designs/p/pricing-page.html",
            "changes": [{"path": "deck.html"}, {"path": "notes.md"}, {"path": "/elsewhere/x.html"}],
        }))
        .unwrap();
        assert_eq!(
            changed_html_files(&item, "/designs/p"),
            vec!["/designs/p/deck.html", "/designs/p/pricing-page.html"]
        );
    }
}
