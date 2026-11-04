//! The Files surface's previews (`FilePreviewPanel.tsx`'s
//! `WorkspaceImagePreview` and `RenderedMarkdownSurface`,
//! `FileMarkdownPreview.tsx`, `DelimitedTablePreview.tsx`,
//! `fileSurfaceChrome.tsx`'s `FileSurfaceNotice`), with the path rules of
//! `packages/shared/src/filePreview.ts`, `delimitedPreview.ts` and
//! `filePreviewMode.ts`.
//!
//! An image file shows through a signed `workspace-file` asset URL with its
//! media menu; Markdown renders with the chat's renderer (its task
//! boxes tick and save); a CSV/TSV renders as a table of at most 100 rows
//! and 30 columns; a video plays in the player (`WorkspaceVideoPreview`).
//! Not ported: audio, PDF and HTML previews in place.
use std::sync::Arc;

use eframe::egui::{self, Color32, Galley, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::markdown::Markdown;
use crate::ui::widgets;

const PREVIEW_URL_REQUEST: &str = "file-preview-url";

/// `WORKSPACE_IMAGE_PREVIEW_EXTENSIONS`.
const IMAGE_EXTENSIONS: [&str; 8] = [
    ".avif", ".gif", ".ico", ".jpeg", ".jpg", ".png", ".svg", ".webp",
];

/// `isWorkspaceImagePreviewPath`.
pub(crate) fn is_image_preview_path(path: &str) -> bool {
    let lower = path
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    IMAGE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// `isMarkdownPreviewFile`.
pub(crate) fn is_markdown_preview_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".mdx")
}

/// `filePreviewDelimiter` for a workspace file (no MIME type).
pub(crate) fn preview_delimiter(path: &str) -> Option<char> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".csv") {
        Some(',')
    } else if lower.ends_with(".tsv") {
        Some('\t')
    } else {
        None
    }
}

/// `parseDelimitedPreview`: quoted delimiters, escaped quotes and multiline
/// cells; at most 100 rows, 30 cells a row and 2000 characters a cell.
/// Returns the rows and whether anything was left out.
pub(crate) fn parse_delimited(text: &str, delimiter: char) -> (Vec<Vec<String>>, bool) {
    let chars: Vec<char> = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .chars()
        .collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut cell_len = 0;
    let mut quoted = false;
    let mut truncated = false;
    let mut row_start = 0;
    let mut index = 0;
    fn end_cell(row: &mut Vec<String>, cell: &mut String, len: &mut usize, truncated: &mut bool) {
        if row.len() < 30 {
            row.push(std::mem::take(cell));
        } else {
            *truncated = true;
            cell.clear();
        }
        *len = 0;
    }
    while index < chars.len() {
        let c = chars[index];
        if c == '"' {
            if quoted && chars.get(index + 1) == Some(&'"') {
                if cell_len < 2000 {
                    cell.push('"');
                    cell_len += 1;
                } else {
                    truncated = true;
                }
                index += 2;
                continue;
            }
            if quoted || cell.is_empty() {
                quoted = !quoted;
                index += 1;
                continue;
            }
        }
        if !quoted && (c == delimiter || c == '\n' || c == '\r') {
            end_cell(&mut row, &mut cell, &mut cell_len, &mut truncated);
            if c != delimiter {
                rows.push(std::mem::take(&mut row));
                if c == '\r' && chars.get(index + 1) == Some(&'\n') {
                    index += 1;
                }
                row_start = index + 1;
                if rows.len() == 100 {
                    return (rows, truncated || index + 1 < chars.len());
                }
            }
        } else if cell_len < 2000 {
            cell.push(c);
            cell_len += 1;
        } else {
            truncated = true;
        }
        index += 1;
    }
    if row_start < chars.len() {
        end_cell(&mut row, &mut cell, &mut cell_len, &mut truncated);
        rows.push(row);
    }
    (rows, truncated || quoted)
}

/// A parsed table laid out for one width and theme.
pub(crate) struct TableLayout {
    key: (usize, usize, usize, u32, bool),
    truncated: bool,
    /// Cell galleys by row (the first is the header).
    cells: Vec<Vec<Arc<Galley>>>,
    widths: Vec<f32>,
    heights: Vec<f32>,
}

const CELL_X: f32 = 12.0; // px-3
const CELL_Y: f32 = 6.0; // py-1.5
const CELL_MAX: f32 = 320.0; // max-w-80
const TABLE_TEXT: f32 = 12.0; // text-xs
const TABLE_LINE: f32 = 16.0;

/// A table's column widths in `available` (CSS auto layout): each column
/// between its longest word and its whole text, sharing the room in
/// proportion; a table that fits grows to fill (`min-w-full`).
fn column_widths(min: &[f32], max: &[f32], available: f32) -> Vec<f32> {
    let (min_sum, max_sum): (f32, f32) = (min.iter().sum(), max.iter().sum());
    if max_sum <= available {
        let extra = available - max_sum;
        max.iter()
            .map(|w| {
                w + if max_sum > 0.0 {
                    extra * w / max_sum
                } else {
                    0.0
                }
            })
            .collect()
    } else if min_sum >= available {
        min.to_vec()
    } else {
        let share = (available - min_sum) / (max_sum - min_sum);
        min.iter()
            .zip(max)
            .map(|(lo, hi)| lo + (hi - lo) * share)
            .collect()
    }
}

/// `FileSurfaceNotice`: a warning strip; returns its height.
fn notice(ui: &Ui, rect: Rect, message: &str) -> f32 {
    let height = 29.0;
    let strip = Rect::from_min_size(rect.min, Vec2::new(rect.width(), height));
    ui.painter()
        .rect_filled(strip, 0.0, theme::alpha(theme::warning(), 0.08));
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(strip.left(), strip.bottom() - 1.0),
            Vec2::new(strip.width(), 1.0),
        ),
        0.0,
        theme::alpha(theme::warning(), 0.2),
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(strip.left() + 12.0, strip.top()),
            Pos2::new(strip.right() - 12.0, strip.bottom() - 1.0),
        ),
        message,
        font(11.0, Weight::Regular),
        theme::warning_foreground(),
    );
    height
}

/// The read's truncation notice above a source view; returns its height.
pub(crate) fn truncated_notice(ui: &Ui, rect: Rect, read: &serde_json::Value) -> f32 {
    if read["truncated"].as_bool() != Some(true) {
        return 0.0;
    }
    let bytes = read["byteLength"].as_u64().unwrap_or_default() as usize;
    notice(
        ui,
        rect,
        &format!(
            "Preview limited to the first 1 MB of a {} byte file.",
            crate::ui::content_search::grouped(bytes)
        ),
    )
}

impl App {
    /// `WorkspaceImagePreview`: the image centered at most at its natural
    /// size (`max-h-full max-w-full object-contain`, `p-4`), a spinner while
    /// it loads; a right click opens its menu.
    /// `WorkspaceVideoPreview`: the file's `media-file` URL in the player,
    /// centered (`max-w-5xl p-4`); a workspace change asks for it again.
    pub(crate) fn workspace_video(&mut self, ui: &mut Ui, rect: Rect, cwd: &str, path: &str) {
        let Some(thread) = self.selected.clone() else {
            return;
        };
        let absolute = format!("{}/{}", cwd.trim_end_matches('/'), path);
        let revision = self.workspace_mutation_id().unwrap_or_default();
        let key = format!("file-video:{thread}:{absolute}\0{revision}");
        if !crate::ui::video::requested(&key) {
            crate::ui::video::mark_requested(&key);
            self.send_request(
                format!("video-url:{key}"),
                "assets.createUrl",
                serde_json::json!({"resource": {
                    "_tag": "media-file", "threadId": thread, "path": absolute,
                }}),
            );
        }
        let area = rect.shrink(16.0);
        let limit = Vec2::new(area.width().min(1024.0), area.height());
        let size = crate::ui::video::natural_size(&key).map_or(
            Vec2::new(limit.x, (limit.x * 9.0 / 16.0).min(limit.y)),
            |(w, h)| {
                let (w, h) = (w.max(1) as f32, h.max(1) as f32);
                let scale = 1.0_f32.min(limit.x / w).min(limit.y / h);
                Vec2::new((w * scale).round(), (h * scale).round())
            },
        );
        let name = path.rsplit('/').next().unwrap_or(path);
        crate::ui::video::player(
            ui,
            &key,
            Rect::from_center_size(area.center(), size),
            name,
            false,
        );
    }

    pub(crate) fn workspace_image(&mut self, ui: &mut Ui, rect: Rect, cwd: &str, path: &str) {
        use crate::ui::attachments::{self, MediaState};
        let Some(thread) = self.selected.clone() else {
            return;
        };
        let absolute = format!("{}/{}", cwd.trim_end_matches('/'), path);
        let key = format!("workspace-file:{thread}:{absolute}");
        if attachments::media_state(&key).is_none() {
            attachments::mark_loading(&key);
            self.send_request(
                format!("attachment:{key}"),
                "assets.createUrl",
                serde_json::json!({"resource": {
                    "_tag": "workspace-file", "threadId": thread, "path": absolute,
                }}),
            );
        }
        let source = crate::ui::media::Source {
            name: path.to_owned(),
            reference: Some(crate::ui::media::file_reference(&absolute, Some(cwd))),
            key: key.clone(),
            local_file: None,
        };
        let response = ui.interact(
            rect,
            ui.id().with(("workspace-image", &key)),
            Sense::click(),
        );
        match attachments::media_state(&key) {
            Some(MediaState::Ready(image)) => {
                let area = rect.shrink(16.0);
                let scale = 1.0_f32
                    .min(area.width() / image.width().max(1) as f32)
                    .min(area.height() / image.height().max(1) as f32);
                let size = Vec2::new(image.width() as f32, image.height() as f32) * scale;
                let shown = Rect::from_center_size(area.center(), size);
                attachments::paint_contain(ui, shown, &key, &image, Color32::WHITE);
                if response.secondary_clicked()
                    && let Some(at) = response.interact_pointer_pos()
                    && shown.contains(at)
                {
                    self.open_media_menu(source, at);
                }
            }
            Some(MediaState::Failed) => {
                let message = "Unable to load workspace image.";
                let width = widgets::text_width(ui, message, font(12.0, Weight::Regular));
                widgets::text_in(
                    ui,
                    Rect::from_center_size(rect.center(), Vec2::new(width + 1.0, 20.0)),
                    message,
                    font(12.0, Weight::Regular),
                    theme::error_foreground(),
                );
                if response.secondary_clicked()
                    && let Some(at) = response.interact_pointer_pos()
                {
                    self.open_media_menu(source, at);
                }
            }
            _ => crate::ui::content_search::paint_spinner(ui, rect.center(), 20.0),
        }
    }

    /// A rendered task ticked: `setMarkdownTaskChecked` on its marker,
    /// shown at once and saved with `projects.writeFile`.
    fn toggle_markdown_task(&mut self, cwd: &str, path: &str, task: usize) {
        let key = format!("{cwd}\0{path}");
        let Some(Ok(read)) = self.files.reads.get_mut(&key) else {
            return;
        };
        let text = read["contents"].as_str().unwrap_or_default().to_owned();
        let mut options = pulldown_cmark::Options::empty();
        options.insert(pulldown_cmark::Options::ENABLE_TABLES);
        options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
        options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
        options.insert(pulldown_cmark::Options::ENABLE_GFM);
        options.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
        let Some((done, offset)) = pulldown_cmark::Parser::new_ext(&text, options)
            .into_offset_iter()
            .filter_map(|(event, range)| match event {
                pulldown_cmark::Event::TaskListMarker(done) => Some((done, range.start)),
                _ => None,
            })
            .nth(task)
        else {
            return;
        };
        let bytes = text.as_bytes();
        if bytes.get(offset) != Some(&b'[')
            || !matches!(bytes.get(offset + 1), Some(b' ' | b'x' | b'X'))
            || bytes.get(offset + 2) != Some(&b']')
        {
            return;
        }
        let next = format!(
            "{}{}{}",
            &text[..offset + 1],
            if done { ' ' } else { 'x' },
            &text[offset + 2..]
        );
        read["contents"] = serde_json::json!(next);
        self.send_request(
            format!("markdown-task:{key}"),
            "projects.writeFile",
            serde_json::json!({"cwd": cwd, "relativePath": path, "contents": next}),
        );
    }

    /// The task save's failure: the toast, and the file read again.
    pub(crate) fn markdown_task_result(
        &mut self,
        key: &str,
        result: &Result<serde_json::Value, String>,
    ) -> bool {
        let Some(file) = key.strip_prefix("markdown-task:") else {
            return false;
        };
        if let Err(error) = result {
            self.notify(crate::ui::toasts::Kind::Error, error.clone());
            self.files.forget_read(file);
        }
        true
    }

    /// `RenderedMarkdownSurface`: the document in a `max-w-4xl px-6 py-5`
    /// column, images resolved beside the file.
    pub(crate) fn rendered_markdown(&mut self, ui: &mut Ui, rect: Rect, cwd: &str, path: &str) {
        let key = format!("{cwd}\0{path}");
        let Some(Ok(read)) = self.files.reads.get(&key) else {
            return;
        };
        let text = read["contents"].as_str().unwrap_or_default().to_owned();
        let tasks = read["truncated"].as_bool() != Some(true);
        let folder = path.rsplit_once('/').map(|(folder, _)| folder);
        let base = match folder {
            Some(folder) => format!("{}/{folder}", cwd.trim_end_matches('/')),
            None => cwd.to_owned(),
        };
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let mut outputs = None;
        egui::ScrollArea::vertical()
            .id_salt(("file-markdown", path))
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let column = (rect.width() - 48.0).clamp(40.0, 896.0 - 48.0);
                let left = rect.left() + ((rect.width() - (column + 48.0)) / 2.0).max(0.0) + 24.0;
                let top = ui.cursor().top() + 20.0;
                let inner =
                    Rect::from_min_size(Pos2::new(left, top), Vec2::new(column, f32::INFINITY));
                let used = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                        let mut markdown = Markdown::new(
                            &mut self.highlighter,
                            ui.id().with(("file-markdown", path)),
                        );
                        markdown.media_root = Some(cwd.to_owned());
                        markdown.image_base = Some(base.clone());
                        markdown.media_thread = self.selected.clone();
                        markdown.task_links = tasks;
                        let height = markdown.show(ui, &text, column);
                        outputs = Some((
                            markdown.opened_file.take(),
                            std::mem::take(&mut markdown.asset_requests),
                            markdown.opened_media.take(),
                            markdown.media_menu.take(),
                        ));
                        height
                    })
                    .inner;
                ui.allocate_rect(
                    Rect::from_min_size(
                        Pos2::new(rect.left(), top - 20.0),
                        Vec2::new(rect.width(), used + 40.0),
                    ),
                    Sense::hover(),
                );
            });
        if let Some(task) = ui
            .ctx()
            .data_mut(|d| d.remove_temp::<usize>(egui::Id::new(crate::ui::markdown::TASK_CLICKED)))
        {
            self.toggle_markdown_task(cwd, path, task);
        }
        let Some((opened_file, asset_requests, opened_media, media_menu)) = outputs else {
            return;
        };
        self.markdown_media(asset_requests, opened_media, media_menu);
        // A relative link resolves beside the file.
        if let Some(file) = opened_file {
            let target = if file.path.starts_with('/') || file.path.starts_with('~') {
                file.path
            } else {
                format!("{base}/{}", file.path.trim_start_matches("./"))
            };
            self.open_markdown_file(&target);
        }
    }

    /// `DelimitedTablePreview`: a sticky header (`bg-muted/60`), striped
    /// rows (`even:bg-muted/30`), cells wrapping within 20rem.
    pub(crate) fn delimited_table(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cwd: &str,
        path: &str,
        delimiter: char,
    ) {
        let key = format!("{cwd}\0{path}");
        let Some(Ok(read)) = self.files.reads.get(&key) else {
            return;
        };
        let text = read["contents"].as_str().unwrap_or_default();
        let probe = ui.fonts_mut(|f| {
            f.layout_no_wrap(
                "0".into(),
                font(TABLE_TEXT, Weight::Regular),
                Color32::WHITE,
            )
        });
        let layout_key = (
            text.as_ptr() as usize,
            text.len(),
            Arc::as_ptr(&probe) as usize,
            rect.width().to_bits(),
            theme::palette().dark,
        );
        if self
            .files
            .table
            .as_ref()
            .is_none_or(|t| t.key != layout_key)
        {
            self.files.table = Some(lay_out_table(ui, text, delimiter, rect.width(), layout_key));
        }
        let Some(table) = &self.files.table else {
            return;
        };
        let mut body = rect;
        if table.truncated {
            let height = notice(
                ui,
                rect,
                "Table limited to the first 100 rows and 30 columns. Switch to source for the rest.",
            );
            body.min.y += height;
        }
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        egui::ScrollArea::both()
            .id_salt(("file-table", path))
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let origin = ui.max_rect().min;
                let width: f32 = table.widths.iter().sum();
                let height: f32 = table.heights.iter().sum();
                let clip = ui.clip_rect();
                let paint_row = |ui: &Ui, index: usize, y: f32| {
                    let row_height = table.heights[index];
                    let row =
                        Rect::from_min_size(Pos2::new(origin.x, y), Vec2::new(width, row_height));
                    let header = index == 0;
                    if header {
                        ui.painter().rect_filled(row, 0.0, theme::background());
                        ui.painter()
                            .rect_filled(row, 0.0, theme::alpha(theme::muted(), 0.6));
                    } else if index.is_multiple_of(2) {
                        ui.painter()
                            .rect_filled(row, 0.0, theme::alpha(theme::muted(), 0.3));
                    }
                    let mut x = origin.x;
                    for (column, cell_width) in table.widths.iter().enumerate() {
                        let line = Rect::from_min_size(
                            Pos2::new(x, row.bottom() - 1.0),
                            Vec2::new(*cell_width, 1.0),
                        );
                        let border = if header {
                            theme::border()
                        } else {
                            theme::alpha(theme::border(), 0.6)
                        };
                        ui.painter().rect_filled(line, 0.0, border);
                        if let Some(galley) = table.cells[index].get(column) {
                            // `th` aligns to the bottom, `td` to the top.
                            let text_y = if header {
                                row.bottom() - 1.0 - CELL_Y - galley.size().y
                            } else {
                                y + CELL_Y
                            };
                            ui.painter().galley(
                                Pos2::new(x + CELL_X, text_y),
                                galley.clone(),
                                theme::foreground(),
                            );
                        }
                        x += cell_width;
                    }
                };
                let mut y = origin.y + table.heights.first().copied().unwrap_or_default();
                for index in 1..table.heights.len() {
                    let row_height = table.heights[index];
                    if y + row_height >= clip.top() && y <= clip.bottom() {
                        paint_row(ui, index, y);
                    }
                    y += row_height;
                }
                if !table.heights.is_empty() {
                    paint_row(ui, 0, origin.y.max(clip.top()));
                }
                ui.allocate_rect(
                    Rect::from_min_size(origin, Vec2::new(width, height)),
                    Sense::hover(),
                );
            });
    }

    /// `openFileInPreview`: the file's signed asset URL (`workspace-file`)
    /// in a new Browser tab.
    pub(crate) fn open_file_in_preview(&mut self, cwd: &str, path: &str) {
        let Some(thread) = self.selected.clone() else {
            return;
        };
        let absolute = format!("{}/{}", cwd.trim_end_matches('/'), path);
        self.send_request(
            PREVIEW_URL_REQUEST.into(),
            "assets.createUrl",
            serde_json::json!({"resource": {
                "_tag": "workspace-file", "threadId": thread, "path": absolute,
            }}),
        );
    }

    /// The asset URL's answer: the Browser tab, or the failure's toast.
    pub(crate) fn preview_url_result(
        &mut self,
        key: &str,
        result: &Result<serde_json::Value, String>,
        ctx: &egui::Context,
    ) -> bool {
        if key != PREVIEW_URL_REQUEST {
            return false;
        }
        let url = result.as_ref().map_err(Clone::clone).and_then(|value| {
            let relative = value["relativeUrl"].as_str().ok_or("An error occurred.")?;
            url::Url::parse(self.connect.server.trim())
                .and_then(|base| base.join(relative))
                .map(|url| url.to_string())
                .map_err(|error| error.to_string())
        });
        match url {
            Ok(url) => self.open_browser_tab(ctx, Some(url)),
            Err(error) => {
                self.toasts.add(
                    crate::ui::toasts::Toast::new(
                        crate::ui::toasts::Kind::Error,
                        "Unable to open file in browser",
                    )
                    .description(error),
                );
            }
        }
        true
    }

    /// Requests a markdown widget's images and opens what it clicked.
    pub(crate) fn markdown_media(
        &mut self,
        asset_requests: Vec<(String, serde_json::Value)>,
        opened_media: Option<(crate::ui::media::Source, Arc<image::RgbaImage>)>,
        media_menu: Option<(crate::ui::media::Source, Pos2)>,
    ) {
        for (key, payload) in asset_requests {
            // A video plays from its URL; an image is fetched and decoded.
            let request = if key.starts_with("md-video:") {
                format!("video-url:{key}")
            } else {
                format!("attachment:{key}")
            };
            self.send_request(request, "assets.createUrl", payload);
        }
        if let Some((source, image)) = opened_media {
            self.open_lightbox(
                vec![crate::ui::lightbox::Item {
                    name: source.name.clone(),
                    key: source.key.clone(),
                    image,
                    source: Some(source),
                }],
                0,
            );
        }
        if let Some((source, at)) = media_menu {
            self.open_media_menu(source, at);
        }
    }
}

fn lay_out_table(
    ui: &Ui,
    text: &str,
    delimiter: char,
    available: f32,
    key: (usize, usize, usize, u32, bool),
) -> TableLayout {
    let (rows, truncated) = parse_delimited(text, delimiter);
    let columns = rows.iter().map(Vec::len).max().unwrap_or_default();
    let weight = |row: usize| {
        if row == 0 {
            Weight::Medium
        } else {
            Weight::Regular
        }
    };
    let mut min = vec![2.0 * CELL_X; columns];
    let mut max = vec![2.0 * CELL_X; columns];
    for (index, row) in rows.iter().enumerate() {
        for (column, cell) in row.iter().enumerate() {
            let cell_font = font(TABLE_TEXT, weight(index));
            let whole = cell
                .lines()
                .map(|line| widgets::text_width(ui, line, cell_font.clone()))
                .fold(0.0, f32::max);
            let word = cell
                .split_whitespace()
                .map(|word| widgets::text_width(ui, word, cell_font.clone()))
                .fold(0.0, f32::max);
            let cap = CELL_MAX - 2.0 * CELL_X;
            max[column] = max[column].max(whole.min(cap).ceil() + 2.0 * CELL_X);
            min[column] = min[column].max(word.min(cap).ceil() + 2.0 * CELL_X);
        }
    }
    let widths = column_widths(&min, &max, available);
    let mut cells = Vec::with_capacity(rows.len());
    let mut heights = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let galleys: Vec<Arc<Galley>> = row
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                widgets::galley(
                    ui,
                    cell,
                    font(TABLE_TEXT, weight(index)),
                    theme::foreground(),
                    TABLE_LINE,
                    widths[column] - 2.0 * CELL_X,
                )
            })
            .collect();
        let content = galleys
            .iter()
            .map(|g| g.size().y)
            .fold(TABLE_LINE, f32::max);
        heights.push(content + 2.0 * CELL_Y + 1.0);
        cells.push(galleys);
    }
    TableLayout {
        key,
        truncated,
        cells,
        widths,
        heights,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_kinds_follow_the_extension() {
        assert!(is_image_preview_path("docs/capture.PNG"));
        assert!(is_image_preview_path("logo.svg?v=2"));
        assert!(!is_image_preview_path("notes.bmp"));
        assert!(is_markdown_preview_file("README.md"));
        assert!(is_markdown_preview_file("page.MDX"));
        assert!(!is_markdown_preview_file("notes.markdown"));
        assert_eq!(preview_delimiter("data.CSV"), Some(','));
        assert_eq!(preview_delimiter("data.tsv"), Some('\t'));
        assert_eq!(preview_delimiter("data.txt"), None);
    }

    #[test]
    fn delimited_text_keeps_quotes_and_multiline_cells() {
        let (rows, truncated) = parse_delimited(
            "\u{feff}a,\"b,c\",\"say \"\"hi\"\"\"\r\n1,\"two\nlines\",3\n",
            ',',
        );
        assert_eq!(
            rows,
            vec![
                vec!["a".to_owned(), "b,c".into(), "say \"hi\"".into()],
                vec!["1".to_owned(), "two\nlines".into(), "3".into()],
            ]
        );
        assert!(!truncated);
        let (rows, truncated) = parse_delimited("x\ty", '\t');
        assert_eq!(rows, vec![vec!["x".to_owned(), "y".into()]]);
        assert!(!truncated);
        assert!(parse_delimited("\"open", ',').1);
    }

    #[test]
    fn delimited_text_is_bounded() {
        let long: String = (0..150).map(|i| format!("{i}\n")).collect();
        let (rows, truncated) = parse_delimited(&long, ',');
        assert_eq!(rows.len(), 100);
        assert!(truncated);
        let wide = vec!["c"; 40].join(",");
        let (rows, truncated) = parse_delimited(&wide, ',');
        assert_eq!(rows[0].len(), 30);
        assert!(truncated);
    }

    #[test]
    fn columns_share_the_room() {
        assert_eq!(
            column_widths(&[10.0, 10.0], &[30.0, 10.0], 80.0),
            vec![60.0, 20.0]
        );
        assert_eq!(
            column_widths(&[50.0, 50.0], &[90.0, 90.0], 80.0),
            vec![50.0, 50.0]
        );
        assert_eq!(
            column_widths(&[10.0, 10.0], &[50.0, 10.0], 40.0),
            vec![30.0, 10.0]
        );
    }
}
