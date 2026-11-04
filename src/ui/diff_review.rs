//! Review comments on diff lines (`diffs/AnnotatableCodeView.tsx`,
//! `diffs/DiffCommentAnnotation.tsx`, `diffs/commentSubmitShortcut.ts`,
//! `reviewCommentContext.ts` `buildDiffReviewComment`): a click or a drag
//! on the line numbers selects lines and opens "Add a comment…" under
//! them; Comment adds a review comment to the composer (its chip and
//! record), drawn under its lines until deleted here or in the composer.
//!
//! Lines are counted over the patch's hunks (`isPartial`'s indices):
//! expanded unchanged context cannot be commented.
use std::collections::HashMap;

use eframe::egui::{self, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::diff::{FileDiff, Line, LineKind};
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// One comment or the open draft under a range of a file's lines.
pub(crate) struct Annotation {
    id: String,
    file: String,
    start: usize,
    end: usize,
    /// The draft's text; `None` once commented.
    draft: Option<String>,
    text: String,
    focus: bool,
}

/// The diff panel's comments by section, and the selection being dragged.
#[derive(Default)]
pub(crate) struct Review {
    /// The section the panel shows (`reviewSectionId`, its title).
    pub(crate) section: Option<(String, String)>,
    annotations: HashMap<String, Vec<Annotation>>,
    /// (file, anchor, head) of a drag on the line numbers.
    drag: Option<(String, usize, usize)>,
}

fn number(line: &Line) -> u32 {
    if line.kind == LineKind::Deletion {
        line.old
    } else {
        line.new
    }
}

fn marker(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Addition => "+",
        LineKind::Deletion => "-",
        LineKind::Context => " ",
    }
}

/// `formatDiffReviewRangeLabel`.
fn range_label(lines: &[&Line]) -> String {
    let (Some(first), Some(last)) = (lines.first(), lines.last()) else {
        return "line".into();
    };
    let same = marker(first.kind).trim();
    let marker = if !same.is_empty() && lines.iter().all(|l| l.kind == first.kind) {
        same
    } else {
        ""
    };
    let (a, b) = (number(first), number(last));
    if a == b {
        format!("{marker}{a}")
    } else {
        format!("{marker}{a} to {marker}{b}")
    }
}

/// The `@@` header and the marked lines (`buildDiffReviewComment`'s `diff`).
fn excerpt(lines: &[&Line]) -> String {
    let range = |old: bool| {
        let numbers: Vec<u32> = lines
            .iter()
            .filter(|l| {
                if old {
                    l.kind != LineKind::Addition
                } else {
                    l.kind != LineKind::Deletion
                }
            })
            .map(|l| if old { l.old } else { l.new })
            .collect();
        (numbers.first().copied().unwrap_or(0), numbers.len())
    };
    let ((old_start, old_count), (new_start, new_count)) = (range(true), range(false));
    std::iter::once(format!(
        "@@ -{old_start},{old_count} +{new_start},{new_count} @@"
    ))
    .chain(
        lines
            .iter()
            .map(|l| format!("{}{}", marker(l.kind), l.text)),
    )
    .collect::<Vec<_>>()
    .join("\n")
}

impl App {
    fn review_key(&self) -> Option<String> {
        let (section, _) = self.review.section.as_ref()?;
        Some(format!("{}\0{section}", self.selected.as_deref()?))
    }

    /// The line-number strip of a row: selecting lines by click or drag.
    /// `index` counts the file's hunk lines.
    pub(crate) fn review_gutter(&mut self, ui: &Ui, gutter: Rect, file: &str, index: usize) {
        if self.review.section.is_none() {
            return;
        }
        let response = ui.interact(
            gutter,
            ui.id().with(("review-gutter", file, index)),
            Sense::click_and_drag(),
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.drag_started()
            || response.is_pointer_button_down_on() && self.review.drag.is_none()
        {
            self.review.drag = Some((file.to_owned(), index, index));
        }
        let pressed = ui.input(|i| i.pointer.primary_down());
        if let Some((drag_file, _, head)) = &mut self.review.drag
            && drag_file == file
            && pressed
            && ui
                .input(|i| i.pointer.latest_pos())
                .is_some_and(|p| p.y >= gutter.top() && p.y < gutter.bottom())
        {
            *head = index;
        }
        // The selected rows, tinted.
        if let Some((drag_file, anchor, head)) = &self.review.drag
            && drag_file == file
            && (*anchor.min(head)..=*anchor.max(head)).contains(&index)
        {
            ui.painter().rect_filled(
                Rect::from_min_max(
                    gutter.min,
                    Pos2::new(ui.max_rect().right(), gutter.bottom()),
                ),
                0.0,
                theme::alpha(theme::primary(), 0.12),
            );
        }
        if !pressed
            && let Some((drag_file, anchor, head)) = self.review.drag.clone()
            && drag_file == file
            && (response.drag_stopped() || response.clicked() || index == head)
        {
            self.review.drag = None;
            let Some(key) = self.review_key() else {
                return;
            };
            let list = self.review.annotations.entry(key).or_default();
            list.retain(|a| a.draft.is_none());
            list.push(Annotation {
                id: uuid::Uuid::new_v4().to_string(),
                file: file.to_owned(),
                start: anchor.min(head),
                end: anchor.max(head),
                draft: Some(String::new()),
                text: String::new(),
                focus: true,
            });
        }
    }

    /// The comments and draft that end on line `index`, under its row.
    pub(crate) fn review_annotations(
        &mut self,
        ui: &mut Ui,
        file: &FileDiff,
        index: usize,
        width: f32,
    ) {
        let Some(key) = self.review_key() else {
            return;
        };
        // A comment removed from the composer leaves the diff too.
        let contexts: Vec<String> = self
            .composer_contexts
            .iter()
            .filter_map(|r| r["contextId"].as_str().map(str::to_owned))
            .collect();
        let Some(list) = self.review.annotations.get_mut(&key) else {
            return;
        };
        list.retain(|a| {
            a.draft.is_some()
                || contexts.iter().any(|id| {
                    *id == crate::ui::composer_context::kind_scoped_id("review-comment", &a.id)
                })
        });
        let here: Vec<usize> = (0..list.len())
            .filter(|&i| list[i].file == file.path && list[i].end == index)
            .collect();
        let mut submit = None;
        let mut remove = None;
        for i in here {
            let annotation = &mut list[i];
            match &mut annotation.draft {
                Some(text) => {
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(width, 8.0 + 64.0 + 6.0 + 24.0 + 8.0),
                        Sense::hover(),
                    );
                    let field = Rect::from_min_size(
                        rect.min + Vec2::new(12.0, 8.0),
                        Vec2::new(width - 24.0, 64.0),
                    );
                    crate::ui::snooze_dialog::field(ui, field, 8);
                    let mut field_ui = ui.new_child(
                        egui::UiBuilder::new().max_rect(field.shrink2(Vec2::new(10.0, 6.0))),
                    );
                    let id = ui.id().with(("review-draft", &annotation.id));
                    let edit = field_ui.add(
                        egui::TextEdit::multiline(text)
                            .id(id)
                            .frame(egui::Frame::NONE)
                            .hint_text("Add a comment…")
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .font(font(13.0, Weight::Regular)),
                    );
                    if annotation.focus {
                        edit.request_focus();
                        annotation.focus = false;
                    }
                    let trimmed = text.trim().to_owned();
                    if edit.has_focus() {
                        let (escape, send) = ui.input_mut(|i| {
                            (
                                i.consume_key(Modifiers::NONE, Key::Escape),
                                i.consume_key(Modifiers::COMMAND, Key::Enter),
                            )
                        });
                        if escape {
                            remove = Some(i);
                        } else if send && !trimmed.is_empty() {
                            submit = Some((i, trimmed.clone()));
                        }
                    }
                    let y = field.bottom() + 6.0;
                    widgets::text(
                        ui,
                        Pos2::new(field.left(), y + 5.0),
                        "⌘/Ctrl Enter to send",
                        font(10.0, Weight::Regular),
                        theme::alpha(theme::muted_foreground(), 0.7),
                        14.0,
                    );
                    let comment = Rect::from_min_size(
                        Pos2::new(field.right() - 74.0, y),
                        Vec2::new(74.0, 24.0),
                    );
                    if widgets::primary_button(
                        ui,
                        comment,
                        ("review-comment", &annotation.id),
                        "Comment",
                        !trimmed.is_empty(),
                    )
                    .clicked()
                    {
                        submit = Some((i, trimmed));
                    }
                    let cancel = Rect::from_min_size(
                        Pos2::new(comment.left() - 4.0 - 60.0, y),
                        Vec2::new(60.0, 24.0),
                    );
                    let response = ui.interact(
                        cancel,
                        ui.id().with(("review-cancel", &annotation.id)),
                        Sense::click(),
                    );
                    if response.hovered() {
                        ui.painter()
                            .rect_filled(cancel, CornerRadius::same(6), theme::accent());
                    }
                    let label_width = widgets::text_width(ui, "Cancel", font(12.0, Weight::Medium));
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(cancel.center().x - label_width / 2.0, cancel.top()),
                            cancel.max,
                        ),
                        "Cancel",
                        font(12.0, Weight::Medium),
                        theme::muted_foreground(),
                    );
                    if response.clicked() {
                        remove = Some(i);
                    }
                }
                None => {
                    let galley = widgets::galley(
                        ui,
                        &annotation.text,
                        font(14.0, Weight::Regular),
                        theme::foreground(),
                        20.0,
                        width - 24.0 - 22.0 - 32.0,
                    );
                    let height = galley.size().y.max(20.0) + 20.0;
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(width, height + 8.0), Sense::hover());
                    let card = Rect::from_min_size(
                        rect.min + Vec2::new(0.0, 4.0),
                        Vec2::new(width, height),
                    );
                    ui.painter()
                        .rect_filled(card, 0.0, theme::alpha(theme::primary(), 0.045));
                    ui.painter().rect_filled(
                        Rect::from_min_size(card.min, Vec2::new(2.0, card.height())),
                        0.0,
                        theme::alpha(theme::primary(), 0.55),
                    );
                    theme::paint_icon(
                        ui,
                        "message-circle",
                        Rect::from_min_size(card.min + Vec2::new(12.0, 12.0), Vec2::splat(14.0)),
                        14.0,
                        theme::alpha(theme::primary(), 0.7),
                    );
                    ui.painter().galley(
                        card.min + Vec2::new(36.0, 10.0),
                        galley,
                        theme::foreground(),
                    );
                    if response.hovered() {
                        let trash = Rect::from_min_size(
                            Pos2::new(card.right() - 8.0 - 24.0, card.top() + 6.0),
                            Vec2::splat(24.0),
                        );
                        if widgets::icon_button(
                            ui,
                            trash,
                            ("review-delete", &annotation.id),
                            "trash-2",
                            12.0,
                            theme::muted_foreground(),
                            "Delete comment",
                        )
                        .clicked()
                        {
                            remove = Some(i);
                        }
                    }
                }
            }
        }
        if let Some(i) = remove {
            let annotation = list.remove(i);
            if annotation.draft.is_none() {
                self.remove_review_comment(&annotation.id);
            }
        }
        if let Some((i, text)) = submit {
            let Some((section_id, section_title)) = self.review.section.clone() else {
                return;
            };
            let Some(annotation) = self
                .review
                .annotations
                .get_mut(&key)
                .and_then(|list| list.get_mut(i))
            else {
                return;
            };
            let lines: Vec<&Line> = file
                .hunks
                .iter()
                .flat_map(|h| h.lines())
                .skip(annotation.start)
                .take(annotation.end + 1 - annotation.start)
                .collect();
            let record = crate::ui::composer_context::review_comment_record(
                &crate::ui::composer_context::ReviewComment {
                    id: &annotation.id,
                    section_id: &section_id,
                    section_title: &section_title,
                    file_path: &file.path,
                    start_index: annotation.start,
                    end_index: annotation.end,
                    range_label: &range_label(&lines),
                    text: &text,
                    diff: &excerpt(&lines),
                    fence_language: "diff",
                },
            );
            annotation.draft = None;
            annotation.text = text;
            self.add_composer_context(ui.ctx(), record);
        }
    }

    /// `removeReviewComment`: the record and its chip leave the composer.
    fn remove_review_comment(&mut self, id: &str) {
        let context = crate::ui::composer_context::kind_scoped_id("review-comment", id);
        let Some(position) = self
            .composer_contexts
            .iter()
            .position(|r| r["contextId"] == context.as_str())
        else {
            return;
        };
        let record = self.composer_contexts.remove(position);
        let reference = crate::ui::composer_context::format_reference(
            "review-comment",
            &context,
            record["label"].as_str().unwrap_or_default(),
        );
        if let Some(at) = self.composer.find(&reference) {
            let end = at + reference.len();
            let end = if self.composer[end..].starts_with(' ') {
                end + 1
            } else {
                end
            };
            self.composer.replace_range(at..end, "");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_quote_their_lines() {
        let files = crate::diff::parse(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n",
        );
        let lines: Vec<&Line> = files[0].hunks.iter().flat_map(|h| h.lines()).collect();
        assert_eq!(range_label(&lines[2..3]), "+2");
        assert_eq!(range_label(&lines[0..3]), "1 to 2");
        assert_eq!(excerpt(&lines[1..3]), "@@ -2,1 +2,1 @@\n-old\n+new");
    }
}
