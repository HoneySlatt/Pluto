//! Editing a workspace file in its Files tab (`FilePreviewPanel.tsx`
//! `EditableFileSurface`, `fileSaveCoordinator.ts`,
//! `useFileSaveCoordinator.ts`): a click in the source view puts the caret
//! there and the file becomes an editor (highlighted, with line numbers);
//! edits save with `projects.writeFile` 500ms after the last one, one save
//! at a time, the newest text winning. A file with nothing left to save
//! goes back to the read-only view once the editor loses focus.
//!
//! Not ported: line comments, truncated (over 1 MB) files stay read-only.
use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme;
use crate::ui::widgets;

const DEBOUNCE: Duration = Duration::from_millis(500); // FILE_SAVE_DEBOUNCE_MS
const KEY: &str = "file-save:";

/// One file being edited.
pub(crate) struct Edit {
    pub(crate) text: String,
    revision: u64,
    confirmed: u64,
    /// The revision on its way to the server.
    saving: Option<u64>,
    last_change: Instant,
    /// The caret to place on the first frame (a char index).
    caret: Option<usize>,
    focused_once: bool,
    /// The last save failed: the next edit tries again.
    failed: bool,
}

#[derive(Default)]
pub(crate) struct Edits {
    files: HashMap<String, Edit>,
}

impl Edits {
    /// Whether `key` (`cwd\0path`) is open in the editor.
    pub(crate) fn editing(&self, key: &str) -> bool {
        self.files.contains_key(key)
    }

    /// Opens the editor on `text` with the caret at char `caret`.
    pub(crate) fn begin(&mut self, key: String, text: String, caret: usize) {
        self.files.entry(key).or_insert(Edit {
            text,
            revision: 0,
            confirmed: 0,
            saving: None,
            last_change: Instant::now(),
            caret: Some(caret),
            focused_once: false,
            failed: false,
        });
    }
}

impl App {
    /// The editor for `key` in `rect`; `code_left` and `number_right` are
    /// the read-only view's gutter geometry, `wrap` its wrap width.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn file_editor(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cwd: &str,
        path: &str,
        number_right: f32,
        code_left: f32,
        wrap: f32,
        background: Color32,
    ) {
        let key = format!("{cwd}\0{path}");
        let Some(mut edit) = self.edits.files.remove(&key) else {
            return;
        };
        // A change from elsewhere replaces a text with nothing left to save.
        if edit.revision == edit.confirmed
            && edit.saving.is_none()
            && let Some(Ok(read)) = self.files.reads.get(&key)
            && let Some(contents) = read["contents"].as_str()
            && contents != edit.text
        {
            edit.text = contents.to_owned();
        }
        let extension = path.rsplit('.').next().unwrap_or_default().to_owned();
        let spans = self.highlighter.spans_or_plain(&edit.text, &extension);
        let line_height = crate::ui::files::code_line_height();
        let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
            let text = text.as_str();
            let mut job = egui::text::LayoutJob::default();
            // Spans of an older text still color what lines up; the rest
            // stays plain until the highlighter catches up.
            let mut at = 0;
            for (span, color) in &spans {
                let end = (at + span.len()).min(text.len());
                if at >= end || !text.is_char_boundary(end) {
                    break;
                }
                job.append(
                    &text[at..end],
                    0.0,
                    widgets::format(theme::code_font(), *color, line_height),
                );
                at = end;
            }
            if at < text.len() {
                job.append(
                    &text[at..],
                    0.0,
                    widgets::format(theme::code_font(), theme::code_foreground(), line_height),
                );
            }
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let scroll = if wrap.is_finite() {
            egui::ScrollArea::vertical()
        } else {
            egui::ScrollArea::both()
        };
        let id = ui.id().with(("file-editor", &key));
        let mut changed = false;
        let mut has_focus = false;
        scroll
            .id_salt(("file-source", path))
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let origin = ui.max_rect().min;
                let top = origin.y + 8.0;
                ui.painter().rect_filled(ui.clip_rect(), 0.0, background);
                let mut editor_ui =
                    ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
                        Pos2::new(origin.x + code_left, top),
                        Vec2::new(if wrap.is_finite() { wrap } else { 1e6 }, f32::INFINITY),
                    )));
                if let Some(caret) = edit.caret.take() {
                    let mut state =
                        egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(caret),
                        )));
                    state.store(ui.ctx(), id);
                    ui.memory_mut(|m| m.request_focus(id));
                }
                let output = egui::TextEdit::multiline(&mut edit.text)
                    .id(id)
                    .code_editor()
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .desired_width(if wrap.is_finite() {
                        wrap
                    } else {
                        f32::INFINITY
                    })
                    .font(theme::code_font())
                    .text_color(theme::code_foreground())
                    .layouter(&mut layouter)
                    .show(&mut editor_ui);
                changed = output.response.changed();
                has_focus = output.response.has_focus();
                // Line numbers beside the first row of each line.
                let mut line = 1;
                let mut new_line = true;
                for row in &output.galley.rows {
                    if new_line {
                        let number = line.to_string();
                        let width = widgets::text_width(ui, &number, theme::code_font());
                        widgets::text(
                            ui,
                            Pos2::new(
                                origin.x + number_right - width,
                                output.galley_pos.y + row.rect().top(),
                            ),
                            &number,
                            theme::code_font(),
                            theme::muted_foreground(),
                            line_height,
                        );
                        line += 1;
                    }
                    new_line = row.ends_with_newline;
                }
                let size = output.response.rect.size();
                ui.allocate_rect(
                    Rect::from_min_size(
                        origin,
                        Vec2::new(size.x + code_left + 12.0, size.y + 16.0),
                    ),
                    Sense::hover(),
                );
            });
        if has_focus {
            edit.focused_once = true;
        }
        if changed {
            edit.revision += 1;
            edit.last_change = Instant::now();
            edit.failed = false;
        }
        let unsaved = edit.revision != edit.confirmed;
        let done = !unsaved && edit.saving.is_none() && edit.focused_once && !has_focus;
        if !done {
            self.edits.files.insert(key, edit);
        }
    }

    /// `FileSaveCoordinator`, for every edited file (shown or not): one
    /// save at a time, the newest text after a quiet 500ms.
    pub(crate) fn flush_file_edits(&mut self, ctx: &egui::Context) {
        let mut saves = Vec::new();
        for (key, edit) in &mut self.edits.files {
            if edit.revision == edit.confirmed || edit.saving.is_some() || edit.failed {
                continue;
            }
            let wait = DEBOUNCE.saturating_sub(edit.last_change.elapsed());
            if !wait.is_zero() {
                ctx.request_repaint_after(wait);
                continue;
            }
            edit.saving = Some(edit.revision);
            let (cwd, path) = key.split_once('\0').unwrap_or((key, ""));
            saves.push((
                format!("{KEY}{}\0{key}", edit.revision),
                json!({"cwd": cwd, "relativePath": path, "contents": edit.text}),
            ));
        }
        for (request, payload) in saves {
            self.send_request(request, "projects.writeFile", payload);
        }
    }

    /// A save's answer: the read shows the saved text, or the failure.
    pub(crate) fn file_save_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        let Some(rest) = key.strip_prefix(KEY) else {
            return false;
        };
        let Some((revision, file)) = rest.split_once('\0') else {
            return true;
        };
        let revision: u64 = revision.parse().unwrap_or(0);
        let Some(edit) = self.edits.files.get_mut(file) else {
            return true;
        };
        edit.saving = None;
        match result {
            Ok(_) => {
                edit.confirmed = edit.confirmed.max(revision);
                // `confirmProjectFileQueryData`: the read is the saved text.
                if revision == edit.revision
                    && let Some(Ok(read)) = self.files.reads.get_mut(file)
                {
                    read["contents"] = json!(edit.text);
                }
            }
            Err(error) => {
                edit.failed = true;
                self.notify(crate::ui::toasts::Kind::Error, error.clone());
            }
        }
        true
    }
}
