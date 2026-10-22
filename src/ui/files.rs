//! The Files surface (`FileBrowserPanel.tsx`, `FilePreviewPanel.tsx`,
//! `ReadOnlySourcePreview.tsx`, `FileBreadcrumbs.tsx`, `fileSurfaceChrome.tsx`).
//!
//! Ported: the project tree, read directory by directory with
//! `projects.listEntries` (folders first, ignored entries dimmed), its search
//! (`projects.searchEntries`), refresh and expand/collapse all; opening a
//! file in the tab (`projects.readFile`) with the breadcrumb, a read-only
//! highlighted source view with line numbers, line wrapping and the tree
//! toggle; the compact "Open in" picker; the image, Markdown and table
//! previews (`file_previews`) and the rendered/source toggle.
//!
//! Not ported: editing and saving, comments and drag-to-mention. Files are
//! only read when the user opens them.
use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::{App, PanelTab};
use crate::theme::{self, Weight, font};
use crate::ui::{file_previews, widgets};

const SEARCH_HEADER: f32 = 36.0;
const ROW: f32 = 24.0;
const INDENT: f32 = 19.0;
const TREE_WIDTH: f32 = 256.0;
const CHEVRON: Color32 = Color32::from_rgb(0x84, 0x84, 0x8a);
const CODE_BACKGROUND: Color32 = Color32::from_rgb(17, 17, 17);
/// The code line (20px at the default code size).
fn code_line_height() -> f32 {
    theme::code_line(20.0)
}

/// One entry of a listing: its workspace-relative path and whether it is a
/// directory or ignored.
#[derive(Clone)]
struct Entry {
    path: String,
    directory: bool,
    ignored: bool,
}

/// The Files surface's reads and view state.
pub struct FilesState {
    /// Listings by `cwd\0directory` (empty directory: the root).
    listings: HashMap<String, Result<Vec<Entry>, String>>,
    /// Search results by `cwd\0query`.
    searches: HashMap<String, Result<Vec<Entry>, String>>,
    /// File reads by `cwd\0path`.
    pub(crate) reads: HashMap<String, Result<Value, String>>,
    requested: HashSet<String>,
    /// Expanded folders by `cwd\0path`.
    expanded: HashSet<String>,
    pub query: String,
    pub wrap: bool,
    pub tree: bool,
    /// The revealed line (path, line) and whether it was scrolled to.
    reveal: Option<(String, usize, bool)>,
    /// The open file's lines laid out.
    rendered: Option<Rendered>,
    /// Markdown and CSV/TSV files shown rendered (`t3code.renderMarkdown`,
    /// `t3code.renderTable`).
    pub render_markdown: bool,
    pub render_table: bool,
    /// The open CSV/TSV laid out as a table.
    pub(crate) table: Option<crate::ui::file_previews::TableLayout>,
}

/// A read file laid out: one galley per line, where each starts and how
/// tall it is, for the read, wrap width and theme in `layout`.
pub(crate) struct Rendered {
    identity: (usize, usize),
    layout: ((usize, usize), usize, u32, bool, u32, u32),
    highlighted: bool,
    galleys: Vec<std::sync::Arc<egui::Galley>>,
    tops: Vec<f32>,
    heights: Vec<f32>,
    width: f32,
}

impl Default for FilesState {
    fn default() -> Self {
        Self {
            listings: HashMap::new(),
            searches: HashMap::new(),
            reads: HashMap::new(),
            requested: HashSet::new(),
            expanded: HashSet::new(),
            query: String::new(),
            wrap: true,
            tree: true,
            reveal: None,
            rendered: None,
            render_markdown: false,
            render_table: true,
            table: None,
        }
    }
}

fn entries(value: &Value) -> Vec<Entry> {
    value["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            Some(Entry {
                path: e["path"].as_str()?.trim_end_matches('/').to_owned(),
                directory: e["kind"] == "directory",
                ignored: e["ignored"] == true,
            })
        })
        .collect()
}

/// Folders first, then names, case-insensitively (pierre trees' order).
fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase()))
    });
}

/// The pierre file-type icon of a path, with its color (`PierreEntryIcon`).
pub(crate) fn file_icon(path: &str) -> (&'static str, Color32) {
    let extension = name_of(path).rsplit('.').next().unwrap_or_default();
    crate::ui::markdown::language_icon(extension)
        .unwrap_or(("file-text", Color32::from_rgb(0xad, 0xad, 0xb1)))
}

fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// A visible tree row.
struct TreeRow {
    entry: Entry,
    depth: usize,
    open: bool,
}

impl FilesState {
    /// Stores an answer; returns whether the key was one of ours.
    pub(crate) fn receive(&mut self, key: &str, result: Result<Value, String>) -> bool {
        if let Some(rest) = key.strip_prefix("files-list:") {
            let listing = result.map(|v| {
                let mut list = entries(&v);
                sort_entries(&mut list);
                list
            });
            self.listings.insert(rest.to_owned(), listing);
        } else if let Some(rest) = key.strip_prefix("files-search:") {
            self.searches
                .insert(rest.to_owned(), result.map(|v| entries(&v)));
        } else if let Some(rest) = key.strip_prefix("files-read:") {
            self.reads.insert(rest.to_owned(), result);
        } else {
            return false;
        }
        true
    }

    /// The tree rows under `directory`, recursing into expanded folders.
    fn rows(&self, cwd: &str, directory: &str, depth: usize, out: &mut Vec<TreeRow>) {
        let Some(Ok(list)) = self.listings.get(&format!("{cwd}\0{directory}")) else {
            return;
        };
        for entry in list {
            let open = entry.directory && self.expanded.contains(&format!("{cwd}\0{}", entry.path));
            out.push(TreeRow {
                entry: entry.clone(),
                depth,
                open,
            });
            if open {
                self.rows(cwd, &entry.path, depth + 1, out);
            }
        }
    }
}

impl App {
    pub(crate) fn files_request(&mut self, key: String, method: &'static str, payload: Value) {
        if self.files.requested.insert(key.clone()) {
            self.send_request(key, method, payload);
        }
    }

    /// File paths matching `query` (`useProjectFilePickerQuery`: kind "file",
    /// empty queries allowed, optionally images only); `None` while the
    /// search is pending.
    pub(crate) fn search_files(
        &mut self,
        cwd: &str,
        query: &str,
        image_only: bool,
    ) -> Option<Result<Vec<String>, String>> {
        let kind = if image_only { "image" } else { "file" };
        let key = format!("{cwd}\0{kind}\0{query}");
        let mut params = json!({"cwd": cwd, "query": query, "limit": 200, "kind": "file"});
        if image_only {
            params["imageOnly"] = json!(true);
        }
        self.files_request(
            format!("files-search:{key}"),
            "projects.searchEntries",
            params,
        );
        let found = self.files.searches.get(&key)?;
        Some(
            found
                .as_ref()
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|e| !e.directory)
                        .map(|e| e.path.clone())
                        .collect()
                })
                .map_err(Clone::clone),
        )
    }

    /// `rightPanelStore.openFile`: the file's tab replaces the standalone
    /// explorer, reusing an open tab of the same file.
    pub(crate) fn open_file(&mut self, path: String) {
        let Some(panel) = self.panel_mut() else {
            return;
        };
        panel
            .tabs
            .retain(|t| !matches!(t, PanelTab::Files { file: None }));
        panel.open = true;
        panel.show(
            PanelTab::Files {
                file: Some(path.clone()),
            },
            |t| {
                *t == PanelTab::Files {
                    file: Some(path.clone()),
                }
            },
        );
        // The tree reveals the file: its folders open.
        if let Some(cwd) = self.workspace_cwd() {
            let mut folder = String::new();
            for part in path
                .split('/')
                .rev()
                .skip(1)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                if !folder.is_empty() {
                    folder.push('/');
                }
                folder.push_str(part);
                self.files.expanded.insert(format!("{cwd}\0{folder}"));
                self.files_list(&cwd, &folder);
            }
        }
    }

    /// `openFile` with a line: the file view centers and selects it.
    pub(crate) fn open_file_at(&mut self, path: String, line: usize) {
        self.open_file(path.clone());
        self.files.reveal = Some((path, line, false));
    }

    fn files_list(&mut self, cwd: &str, directory: &str) {
        self.files_request(
            format!("files-list:{cwd}\0{directory}"),
            "projects.listEntries",
            json!({"cwd": cwd, "directoryPath": directory}),
        );
    }

    /// The surface in `body` (below the tab bar), previewing `file`.
    pub(crate) fn files_surface(&mut self, ui: &mut Ui, body: Rect, file: Option<String>) {
        let Some(cwd) = self.workspace_cwd() else {
            widgets::text(
                ui,
                body.min + Vec2::new(16.0, 16.0),
                "Open a project to browse its files.",
                font(14.0, Weight::Regular),
                theme::muted_foreground(),
                20.0,
            );
            return;
        };
        self.files_list(&cwd, "");
        let Some(path) = file else {
            self.file_tree(ui, body, &cwd, None);
            return;
        };

        // Breadcrumb and the preview toolbar.
        let toolbar = Rect::from_min_size(body.min, Vec2::new(body.width(), 28.0));
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(toolbar.left(), toolbar.bottom() - 1.0),
                Vec2::new(toolbar.width(), 1.0),
            ),
            0.0,
            theme::alpha(theme::border(), 0.6),
        );
        let project = self
            .current_thread()
            .and_then(|t| self.state.projects.iter().find(|p| p.id == t.project_id))
            .map(|p| p.title.clone())
            .unwrap_or_default();
        let small = font(12.0, Weight::Regular);
        let mut x = toolbar.left() + 14.0;
        widgets::text(
            ui,
            Pos2::new(x, toolbar.top() + 6.0),
            &project,
            small.clone(),
            theme::muted_foreground(),
            16.0,
        );
        x += widgets::text_width(ui, &project, small.clone()) + 4.0;
        theme::paint_icon(
            ui,
            "chevron-right",
            Rect::from_min_size(Pos2::new(x, toolbar.top() + 7.0), Vec2::splat(14.0)),
            14.0,
            theme::alpha(theme::muted_foreground(), 0.6),
        );
        x += 14.0 + 6.0;
        let mut segments: Vec<&str> = path.split('/').collect();
        let name = segments.pop().unwrap_or_default().to_owned();
        for segment in segments {
            widgets::text(
                ui,
                Pos2::new(x, toolbar.top() + 6.0),
                segment,
                small.clone(),
                theme::muted_foreground(),
                16.0,
            );
            x += widgets::text_width(ui, segment, small.clone()) + 4.0;
            theme::paint_icon(
                ui,
                "chevron-right",
                Rect::from_min_size(Pos2::new(x, toolbar.top() + 7.0), Vec2::splat(14.0)),
                14.0,
                theme::alpha(theme::muted_foreground(), 0.6),
            );
            x += 14.0 + 6.0;
        }
        // The current file is a `px-0.5` chip.
        widgets::text(
            ui,
            Pos2::new(x + 2.0, toolbar.top() + 6.0),
            &name,
            font(12.0, Weight::Medium),
            theme::foreground(),
            16.0,
        );

        // What the file shows: an image, the rendered document or table
        // (a revealed line wins until the toggle), else its source.
        let image = file_previews::is_image_preview_path(&path);
        let markdown = file_previews::is_markdown_preview_file(&path);
        let delimiter = file_previews::preview_delimiter(&path);
        let revealing = self
            .files
            .reveal
            .as_ref()
            .is_some_and(|(revealed, ..)| *revealed == path);
        let rendered = if markdown {
            self.files.render_markdown && !revealing
        } else {
            delimiter.is_some() && self.files.render_table && !revealing
        };
        let read = self.files.reads.get(&format!("{cwd}\0{path}"));
        let raw_text = !image && !rendered && matches!(read, Some(Ok(_)));
        let mut buttons = vec![(
            "files-tree",
            "folder-tree",
            if self.files.tree {
                "Hide file explorer"
            } else {
                "Show file explorer"
            },
            self.files.tree,
        )];
        if raw_text {
            buttons.push((
                "files-wrap",
                "text-wrap",
                if self.files.wrap {
                    "Disable word wrap"
                } else {
                    "Enable word wrap"
                },
                self.files.wrap,
            ));
        }
        if markdown || delimiter.is_some() {
            let (icon, tip) = match (rendered, markdown) {
                (true, true) => ("code-xml", "Show markdown source"),
                (false, true) => ("eye", "Show rendered markdown"),
                (true, false) => ("code-xml", "Show source"),
                (false, false) => ("table-2", "Show table"),
            };
            buttons.push(("files-rendered", icon, tip, rendered));
        }
        let mut right = toolbar.right() - 12.0;
        for (id, icon, tip, active) in buttons {
            let button =
                Rect::from_min_size(Pos2::new(right - 28.0, toolbar.top()), Vec2::splat(28.0));
            right = button.left() - 8.0;
            if active {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(10), theme::accent());
            }
            if widgets::icon_button(ui, button, id, icon, 14.0, theme::foreground(), tip).clicked()
            {
                match id {
                    "files-tree" => self.files.tree = !self.files.tree,
                    "files-wrap" => self.files.wrap = !self.files.wrap,
                    _ => {
                        if markdown {
                            self.files.render_markdown = !rendered;
                        } else {
                            self.files.render_table = !rendered;
                        }
                        // Pressed, the toggle handles the reveal.
                        if !rendered && revealing {
                            self.files.reveal = None;
                        }
                        crate::app::save_ui_state(self);
                    }
                }
            }
        }
        // "Open in editor" split button.
        let split = Rect::from_min_size(
            Pos2::new(right - 50.0, toolbar.top() + 2.0),
            Vec2::new(50.0, 24.0),
        );
        // The compact `OpenInPicker`: the file in the preferred editor, or
        // the chevron's list.
        let main_half =
            Rect::from_min_max(split.min, Pos2::new(split.left() + 25.0, split.bottom()));
        let menu_half = Rect::from_min_max(Pos2::new(split.left() + 26.0, split.top()), split.max);
        let main_response = ui.interact(main_half, ui.id().with("files-open-in"), Sense::click());
        let menu_response = ui.interact(
            menu_half,
            ui.id().with("files-open-in-menu"),
            Sense::click(),
        );
        ui.painter().rect(
            split,
            CornerRadius::same(8),
            theme::control_surface(),
            eframe::egui::Stroke::new(1.0, theme::input()),
            eframe::egui::StrokeKind::Inside,
        );
        for (half, response) in [(main_half, &main_response), (menu_half, &menu_response)] {
            if response.hovered() {
                ui.painter().rect_filled(
                    half.shrink(1.0),
                    CornerRadius::same(7),
                    theme::overlay(0.05),
                );
            }
        }
        let preferred = self.preferred_editor();
        let absolute = format!("{}/{}", cwd.trim_end_matches('/'), path);
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(split.left() + 25.0, split.top()),
                Vec2::new(1.0, split.height()),
            ),
            0.0,
            theme::input(),
        );
        if let Some((icon, brand)) = preferred.and_then(crate::ui::open_in::editor_icon) {
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(
                    Pos2::new(split.left() + 6.0, split.center().y - 7.0),
                    Vec2::splat(14.0),
                ),
                14.0,
                if brand {
                    theme::foreground()
                } else {
                    theme::muted_foreground()
                },
            );
        }
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(split.left() + 30.0, split.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            16.0,
            theme::muted_foreground(),
        );
        if main_response.clicked()
            && let Some(editor) = preferred
        {
            self.open_in_editor(&absolute, editor);
        }
        if menu_response.clicked() {
            self.toggle_open_in_menu(Some(absolute), split.right_bottom() + Vec2::new(0.0, 4.0));
        }

        // Source (and the tree beside it).
        let content = Rect::from_min_max(Pos2::new(body.left(), toolbar.bottom() + 12.0), body.max);
        let source = if self.files.tree {
            let tree = Rect::from_min_max(
                Pos2::new(content.right() - TREE_WIDTH, content.top()),
                content.max,
            );
            ui.painter().rect_filled(
                Rect::from_min_size(tree.min, Vec2::new(1.0, tree.height())),
                0.0,
                theme::alpha(theme::border(), 0.6),
            );
            let tree_inner = Rect::from_min_max(Pos2::new(tree.left() + 1.0, tree.top()), tree.max);
            self.file_tree(ui, tree_inner, &cwd, Some(&path));
            Rect::from_min_max(content.min, Pos2::new(tree.left(), content.bottom()))
        } else {
            content
        };
        if image {
            self.workspace_image(ui, source, &cwd, &path);
            return;
        }
        // The read must be in before a rendered view takes over.
        let key = format!("{cwd}\0{path}");
        match (rendered, delimiter, self.files.reads.get(&key)) {
            (true, None, Some(Ok(_))) => self.rendered_markdown(ui, source, &cwd, &path),
            (true, Some(delimiter), Some(Ok(_))) => {
                self.delimited_table(ui, source, &cwd, &path, delimiter);
            }
            _ => self.file_source(ui, source, &cwd, &path),
        }
    }

    /// The read-only source view (pierre's file view): line numbers and
    /// highlighted code, wrapped or scrolled sideways.
    fn file_source(&mut self, ui: &mut Ui, rect: Rect, cwd: &str, path: &str) {
        let key = format!("{cwd}\0{path}");
        self.files_request(
            format!("files-read:{key}"),
            "projects.readFile",
            json!({"cwd": cwd, "relativePath": path}),
        );
        let value = match self.files.reads.get(&key) {
            None => {
                widgets::text(
                    ui,
                    rect.min + Vec2::new(16.0, 12.0),
                    "Loading file…",
                    font(12.0, Weight::Regular),
                    theme::muted_foreground(),
                    16.0,
                );
                return;
            }
            Some(Err(error)) => {
                let message = if error.contains("binary_file") {
                    "This file is binary and cannot be shown.".to_owned()
                } else {
                    format!("Could not read this file: {error}")
                };
                widgets::text(
                    ui,
                    rect.min + Vec2::new(16.0, 12.0),
                    &message,
                    font(12.0, Weight::Regular),
                    theme::muted_foreground(),
                    16.0,
                );
                return;
            }
            Some(Ok(value)) => value,
        };
        let mut rect = rect;
        rect.min.y += file_previews::truncated_notice(ui, rect, value);
        let contents = value["contents"].as_str().unwrap_or_default();
        // The read's text stays put until the file is read again.
        let identity = (contents.as_ptr() as usize, contents.len());
        let line_count = match &self.files.rendered {
            Some(rendered) if rendered.identity == identity => rendered.galleys.len(),
            _ => contents.split('\n').count(),
        };
        let digits = line_count.to_string().len().max(1) as f32;
        let digit = widgets::text_width(ui, "0", theme::code_font());
        let number_right = 25.0 + (digits - 1.0) * digit;
        let code_left = number_right + 17.0;
        let wrap_width = if self.files.wrap {
            (rect.width() - code_left - 12.0).max(40.0)
        } else {
            f32::INFINITY
        };
        let extension = name_of(path).rsplit('.').next().unwrap_or_default();
        // Lines are laid out once per read, width and theme; plain lines
        // until the highlighter answers. epaint rebuilds its font atlas now
        // and then, which outdates kept galleys: a probe galley it lays out
        // again then tells.
        let probe =
            ui.fonts_mut(|f| f.layout_no_wrap("0".into(), theme::code_font(), Color32::WHITE));
        let layout = (
            identity,
            std::sync::Arc::as_ptr(&probe) as usize,
            wrap_width.to_bits(),
            theme::palette().dark,
            theme::code_font().size.to_bits(),
            code_line_height().to_bits(),
        );
        let fresh = self.files.rendered.as_ref().is_some_and(|rendered| {
            rendered.layout == layout
                && (rendered.highlighted || self.highlighter.spans(contents, extension).is_none())
        });
        if !fresh {
            let spans = self
                .highlighter
                .spans(contents, extension)
                .map(<[_]>::to_vec);
            let highlighted = spans.is_some();
            let spans =
                spans.unwrap_or_else(|| vec![(contents.to_owned(), crate::highlight::plain())]);
            // Split the spans into lines.
            let mut lines: Vec<Vec<(String, Color32)>> = vec![Vec::new()];
            for (text, color) in spans {
                let mut parts = text.split('\n').peekable();
                while let Some(part) = parts.next() {
                    if !part.is_empty() {
                        lines
                            .last_mut()
                            .expect("one line")
                            .push((part.to_owned(), color));
                    }
                    if parts.peek().is_some() {
                        lines.push(Vec::new());
                    }
                }
            }
            let galleys: Vec<std::sync::Arc<egui::Galley>> = lines
                .iter()
                .map(|line| {
                    let mut job = egui::text::LayoutJob::default();
                    for (text, color) in line {
                        job.append(
                            text,
                            0.0,
                            widgets::format(theme::code_font(), *color, code_line_height()),
                        );
                    }
                    if line.is_empty() {
                        job.append(
                            " ",
                            0.0,
                            widgets::format(
                                theme::code_font(),
                                theme::code_foreground(),
                                code_line_height(),
                            ),
                        );
                    }
                    job.wrap.max_width = wrap_width;
                    ui.fonts_mut(|f| f.layout_job(job))
                })
                .collect();
            let heights: Vec<f32> = galleys
                .iter()
                .map(|g| g.size().y.max(code_line_height()))
                .collect();
            let tops = heights
                .iter()
                .scan(0.0, |top, height| {
                    let line_top = *top;
                    *top += height;
                    Some(line_top)
                })
                .collect();
            let width = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max);
            self.files.rendered = Some(Rendered {
                identity,
                layout,
                highlighted,
                galleys,
                tops,
                heights,
                width,
            });
        }
        let Some(rendered) = &self.files.rendered else {
            return;
        };
        let reveal = self
            .files
            .reveal
            .as_ref()
            .filter(|(revealed, _, _)| revealed == path)
            .map(|&(_, line, scrolled)| (line, scrolled));
        let mut revealed = false;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let scroll = if self.files.wrap {
            egui::ScrollArea::vertical()
        } else {
            egui::ScrollArea::both()
        };
        scroll
            .id_salt(("file-source", path))
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let origin = ui.max_rect().min;
                let height = rendered.heights.iter().sum::<f32>() + 16.0;
                let width = rendered.width + code_left + 12.0;
                let area = Rect::from_min_size(origin, Vec2::new(width.max(rect.width()), height));
                ui.painter().rect_filled(
                    Rect::from_min_size(origin, Vec2::new(area.width(), height)),
                    0.0,
                    theme::stock_or(CODE_BACKGROUND, |p| {
                        theme::mix(p.background, p.overlay, 0.03)
                    }),
                );
                let top = origin.y + 8.0;
                if let Some((line, scrolled)) = reveal
                    && let Some(index) = line.checked_sub(1).filter(|i| *i < rendered.tops.len())
                {
                    // Pierre's selected line, as rendered on the stock theme.
                    let row = Rect::from_min_size(
                        Pos2::new(origin.x, top + rendered.tops[index]),
                        Vec2::new(area.width(), rendered.heights[index]),
                    );
                    let gutter = Rect::from_min_max(
                        row.min,
                        Pos2::new(origin.x + number_right + 7.0, row.bottom()),
                    );
                    ui.painter().rect_filled(
                        gutter,
                        0.0,
                        theme::stock_or(Color32::from_rgb(37, 69, 102), |p| {
                            theme::mix(p.background, p.primary, 0.35)
                        }),
                    );
                    ui.painter().rect_filled(
                        Rect::from_min_max(Pos2::new(gutter.right() + 1.0, row.top()), row.max),
                        0.0,
                        theme::stock_or(Color32::from_rgb(32, 48, 68), |p| {
                            theme::mix(p.background, p.primary, 0.2)
                        }),
                    );
                    if !scrolled {
                        ui.scroll_to_rect(row, Some(egui::Align::Center));
                        revealed = true;
                    }
                }
                // Only the lines in view are drawn.
                let clip = ui.clip_rect();
                let first = rendered
                    .tops
                    .partition_point(|t| top + t <= clip.top())
                    .saturating_sub(1);
                for index in first..rendered.galleys.len() {
                    let y = top + rendered.tops[index];
                    if y > clip.bottom() {
                        break;
                    }
                    let number = (index + 1).to_string();
                    let number_width = widgets::text_width(ui, &number, theme::code_font());
                    let number_color = if reveal.is_some_and(|(line, _)| line == index + 1) {
                        theme::stock_or(Color32::from_rgb(114, 182, 255), |p| p.primary)
                    } else {
                        theme::muted_foreground()
                    };
                    widgets::text(
                        ui,
                        Pos2::new(origin.x + number_right - number_width, y),
                        &number,
                        theme::code_font(),
                        number_color,
                        code_line_height(),
                    );
                    ui.painter().galley(
                        Pos2::new(origin.x + code_left, y),
                        rendered.galleys[index].clone(),
                        theme::code_foreground(),
                    );
                }
                ui.allocate_rect(area, Sense::hover());
            });
        if revealed && let Some(reveal) = &mut self.files.reveal {
            reveal.2 = true;
        }
    }

    /// The project tree in `rect`: the search header and the rows.
    fn file_tree(&mut self, ui: &mut Ui, rect: Rect, cwd: &str, selected: Option<&str>) {
        let header = Rect::from_min_size(rect.min, Vec2::new(rect.width(), SEARCH_HEADER));
        let refresh = Rect::from_min_size(
            Pos2::new(header.left() + 8.0, header.top() + 6.0),
            Vec2::splat(24.0),
        );
        if widgets::icon_button(
            ui,
            refresh,
            "files-refresh",
            "refresh-cw",
            14.0,
            theme::muted_foreground(),
            "Refresh files",
        )
        .clicked()
        {
            let prefix = format!("{cwd}\0");
            self.files.listings.retain(|k, _| !k.starts_with(&prefix));
            self.files.searches.retain(|k, _| !k.starts_with(&prefix));
            self.files.requested.retain(|k| !k.contains(&prefix));
        }
        let toggle = Rect::from_min_size(
            Pos2::new(header.right() - 32.0, header.top() + 6.0),
            Vec2::splat(24.0),
        );
        let any_open = self
            .files
            .expanded
            .iter()
            .any(|k| k.starts_with(&format!("{cwd}\0")));
        if widgets::icon_button(
            ui,
            toggle,
            "files-expand",
            if any_open {
                "chevrons-down-up"
            } else {
                "chevrons-up-down"
            },
            14.0,
            theme::muted_foreground(),
            if any_open {
                "Collapse all folders"
            } else {
                "Expand all folders"
            },
        )
        .clicked()
        {
            if any_open {
                self.files
                    .expanded
                    .retain(|k| !k.starts_with(&format!("{cwd}\0")));
            } else {
                // Expand the folders read so far.
                let folders: Vec<String> = self
                    .files
                    .listings
                    .iter()
                    .filter(|(k, _)| k.starts_with(&format!("{cwd}\0")))
                    .filter_map(|(_, v)| v.as_ref().ok())
                    .flatten()
                    .filter(|e| e.directory)
                    .map(|e| e.path.clone())
                    .collect();
                for folder in folders {
                    self.files.expanded.insert(format!("{cwd}\0{folder}"));
                    self.files_list(cwd, &folder);
                }
            }
        }
        // Search input.
        let input = Rect::from_min_max(
            Pos2::new(refresh.right() + 4.0, header.top() + 4.0),
            Pos2::new(toggle.left() - 4.0, header.bottom() - 4.0),
        );
        let inner = Rect::from_min_max(
            input.min + Vec2::new(10.0, 4.0),
            input.max - Vec2::new(10.0, 4.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        child.add(
            egui::TextEdit::singleline(&mut self.files.query)
                .id_salt("files-search")
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font(14.0, Weight::Regular))
                .text_color(theme::foreground())
                .desired_width(inner.width())
                .vertical_align(egui::Align::Center),
        );
        if self.files.query.is_empty() {
            widgets::text(
                ui,
                Pos2::new(inner.left(), inner.top()),
                "Search files",
                font(14.0, Weight::Regular),
                theme::muted_foreground(),
                20.0,
            );
        }

        // Rows: the search results (flat) or the tree.
        let query = self.files.query.trim().to_owned();
        let rows: Vec<TreeRow> = if query.is_empty() {
            let mut rows = Vec::new();
            self.files.rows(cwd, "", 0, &mut rows);
            rows
        } else {
            let key = format!("{cwd}\0{query}");
            self.files_request(
                format!("files-search:{key}"),
                "projects.searchEntries",
                json!({"cwd": cwd, "query": query, "limit": 200}),
            );
            match self.files.searches.get(&key) {
                Some(Ok(found)) => found
                    .iter()
                    .map(|entry| TreeRow {
                        entry: entry.clone(),
                        depth: 0,
                        open: false,
                    })
                    .collect(),
                _ => Vec::new(),
            }
        };
        let list = Rect::from_min_max(Pos2::new(rect.left(), header.bottom()), rect.max);
        if let Some(Err(error)) = self.files.listings.get(&format!("{cwd}\0")) {
            widgets::text(
                ui,
                list.min + Vec2::new(16.0, 12.0),
                &format!("Could not list files: {error}"),
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
                16.0,
            );
            return;
        }
        let mut toggle_folder: Option<String> = None;
        let mut open_file: Option<String> = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        child.set_clip_rect(list.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical()
            .id_salt(("files-tree", cwd))
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let origin = ui.max_rect().min;
                let item_left = origin.x + 16.0;
                let item_right = list.right() - 16.0;
                let mut y = origin.y + 4.0;
                for (index, row) in rows.iter().enumerate() {
                    let item =
                        Rect::from_min_max(Pos2::new(item_left, y), Pos2::new(item_right, y + ROW));
                    y += ROW;
                    let response =
                        ui.interact(item, ui.id().with(("files-row", index)), Sense::click());
                    let is_selected =
                        !row.entry.directory && selected == Some(row.entry.path.as_str());
                    if is_selected {
                        ui.painter().rect_filled(
                            item,
                            CornerRadius::same(5),
                            theme::alpha(theme::foreground(), 0.12),
                        );
                    } else if response.hovered() {
                        ui.painter().rect_filled(
                            item,
                            CornerRadius::same(5),
                            theme::alpha(theme::foreground(), 0.07),
                        );
                    }
                    let slot = Rect::from_min_size(
                        Pos2::new(item.left() + 6.0 + row.depth as f32 * INDENT, item.top()),
                        Vec2::new(16.0, ROW),
                    );
                    let fade = if row.entry.ignored { 0.5 } else { 1.0 };
                    let label = if query.is_empty() {
                        name_of(&row.entry.path)
                    } else {
                        row.entry.path.as_str()
                    };
                    if row.entry.directory {
                        theme::paint_icon(
                            ui,
                            if row.open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            },
                            slot.translate(Vec2::new(0.0, 1.0)),
                            20.0,
                            theme::alpha(CHEVRON, fade),
                        );
                    } else {
                        let (icon, color) = file_icon(&row.entry.path);
                        theme::paint_icon(ui, icon, slot, 16.0, theme::alpha(color, fade));
                    }
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(slot.right() + 5.0, item.top()),
                            Pos2::new(item.right() - 4.0, item.bottom()),
                        ),
                        label,
                        font(12.0, Weight::Regular),
                        theme::alpha(theme::foreground(), fade),
                    );
                    if response.clicked() {
                        if row.entry.directory {
                            toggle_folder = Some(row.entry.path.clone());
                        } else {
                            open_file = Some(row.entry.path.clone());
                        }
                    }
                }
                ui.allocate_rect(
                    Rect::from_min_max(origin, Pos2::new(item_right, y + 8.0)),
                    Sense::hover(),
                );
            });
        if let Some(folder) = toggle_folder {
            let key = format!("{cwd}\0{folder}");
            if !self.files.expanded.remove(&key) {
                self.files.expanded.insert(key);
                self.files_list(cwd, &folder);
            }
        }
        if let Some(path) = open_file
            && let Some(panel) = self.panel_mut()
        {
            let index = panel.active.min(panel.tabs.len().saturating_sub(1));
            if let Some(tab @ PanelTab::Files { .. }) = panel.tabs.get_mut(index) {
                *tab = PanelTab::Files { file: Some(path) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listings_put_folders_first() {
        let value = json!({"entries": [
            {"path": "README.md", "kind": "file"},
            {"path": "src", "kind": "directory"},
            {"path": "a.txt", "kind": "file", "ignored": true}
        ]});
        let mut list = entries(&value);
        sort_entries(&mut list);
        let paths: Vec<&str> = list.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["src", "a.txt", "README.md"]);
        assert!(list[1].ignored);
    }
}
