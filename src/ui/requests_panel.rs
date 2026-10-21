//! Requests attached above the composer: the approval banner
//! (`ComposerPendingApprovalPanel.tsx`, `ComposerPendingApprovalActions.tsx`)
//! and the question card (`ComposerPendingUserInputPanel.tsx`), drawn on the
//! attached `ComposerBanner` surface.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::json;

use crate::app::App;
use crate::requests::{self, PendingApproval, PendingUserInput, Question};
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// Banners slide 17px under the composer surface (`--chat-composer-attachment-overlap`).
pub const OVERLAP: f32 = 17.0;
const INSET: f32 = 22.0; // --chat-composer-drawer-inset
fn warning() -> Color32 {
    theme::warning()
}
fn warning_surface() -> Color32 {
    theme::stock_or(Color32::from_rgb(35, 27, 15), |p| {
        theme::mix(p.background, p.warning, 0.1)
    })
}
fn warning_outline() -> Color32 {
    theme::stock_or(Color32::from_rgb(96, 63, 10), |p| {
        theme::mix(p.background, p.warning, 0.35)
    })
}
fn neutral_surface() -> Color32 {
    theme::shade(16)
}
fn neutral_outline() -> Color32 {
    theme::shade(28)
}
/// The question body's scroll limit (`ComposerBanner.Scroll`:
/// `max-h-[min(24rem,40dvh)]`).
fn max_body_height(ui: &Ui) -> f32 {
    (ui.ctx().content_rect().height() * 0.4).min(384.0)
}

/// One option's wrapped label and description (`px-2.5 py-2`, the text
/// column beside the 20px shortcut slot) and its row height.
struct OptionLayout {
    label: std::sync::Arc<egui::Galley>,
    description: Option<std::sync::Arc<egui::Galley>>,
    height: f32,
}

fn option_layouts(ui: &Ui, question: &Question, width: f32) -> Vec<OptionLayout> {
    let text_width = width - 10.0 - 8.0 - 20.0 - 10.0;
    question
        .options
        .iter()
        .map(|option| {
            let label = widgets::galley(
                ui,
                &option.label,
                font(14.0, Weight::Medium),
                theme::foreground(),
                20.0,
                text_width,
            );
            let description = option
                .description
                .as_ref()
                .filter(|d| **d != option.label)
                .map(|d| {
                    widgets::galley(
                        ui,
                        d,
                        font(11.0, Weight::Regular),
                        theme::secondary_label(),
                        16.0,
                        text_width,
                    )
                });
            let height =
                8.0 + label.size().y + description.as_ref().map_or(0.0, |d| 2.0 + d.size().y) + 8.0;
            OptionLayout {
                label,
                description,
                height,
            }
        })
        .collect()
}

/// The question body's full height at `width`: the question, the
/// multi-select hint, the options (`mt-2 space-y-0.5`) and `pb-1`.
fn body_height(ui: &Ui, question: &Question, width: f32) -> f32 {
    let text = widgets::galley(
        ui,
        &question.question,
        font(14.0, Weight::Regular),
        theme::foreground(),
        20.0,
        width,
    );
    let options: f32 = option_layouts(ui, question, width)
        .iter()
        .map(|o| o.height)
        .sum::<f32>()
        + 2.0 * question.options.len().saturating_sub(1) as f32;
    text.size().y + if question.multi_select { 20.0 } else { 0.0 } + 8.0 + options + 4.0
}

/// The body's visible height and text width: past the scroll limit the
/// content clears the scrollbar (`pe-2`).
fn body_metrics(ui: &Ui, question: &Question, width: f32) -> (f32, f32, f32) {
    let max = max_body_height(ui);
    let full = body_height(ui, question, width);
    if full <= max {
        return (full, full, width);
    }
    let narrow = width - 8.0;
    (max, body_height(ui, question, narrow), narrow)
}

fn banner_surface(ui: &Ui, rect: Rect, fill: Color32, outline: Color32) {
    let corners = CornerRadius {
        nw: 16,
        ne: 16,
        sw: 0,
        se: 0,
    };
    ui.painter().rect(
        rect,
        corners,
        fill,
        Stroke::new(1.0, outline),
        StrokeKind::Inside,
    );
}

fn detail_lines(approval: &PendingApproval) -> usize {
    approval
        .detail
        .as_deref()
        .map_or(1, |d| d.lines().count().clamp(1, 5))
}

impl App {
    pub(crate) fn approval_banner_height(&self, approval: &PendingApproval) -> f32 {
        12.0 + 16.0 + 4.0 + 16.0 * detail_lines(approval) as f32 + 12.0 + OVERLAP
    }

    pub(crate) fn question_banner_height(
        &self,
        ui: &Ui,
        input: &PendingUserInput,
        width: f32,
    ) -> f32 {
        let Some(question) = input.questions.get(self.question_index) else {
            return 0.0;
        };
        let mut height = 4.0 + 24.0;
        if self.question_collapsed.as_deref() != Some(question.id.as_str()) {
            height += body_metrics(ui, question, width - 2.0 * INSET - 40.0).0;
        }
        height + 4.0 + OVERLAP
    }

    /// Approval banner ending `OVERLAP` px under the top of the composer `surface`.
    pub(crate) fn approval_banner(
        &mut self,
        ui: &mut Ui,
        surface: Rect,
        approval: &PendingApproval,
        count: usize,
        thread_id: &str,
    ) {
        let height = self.approval_banner_height(approval);
        let rect = Rect::from_min_max(
            Pos2::new(surface.left() + INSET, surface.top() + OVERLAP - height),
            Pos2::new(surface.right() - INSET, surface.top() + OVERLAP),
        );
        banner_surface(ui, rect, warning_surface(), warning_outline());
        let top = rect.top() + 12.0;
        theme::paint_icon(
            ui,
            "shield",
            Rect::from_min_size(Pos2::new(rect.left() + 16.0, top + 2.0), Vec2::splat(16.0)),
            16.0,
            warning(),
        );
        let content_left = rect.left() + 12.0 + 24.0 + 8.0;
        let content_height = 16.0 + 4.0 + 16.0 * detail_lines(approval) as f32;

        // Actions: primary decline/accept, the rest behind "…".
        let responding = self.responding.contains(&approval.request_id);
        let options = approval.options();
        let primary: Vec<_> = options
            .iter()
            .filter(|o| o.decision == "decline" || o.decision == "accept")
            .collect();
        let more: Vec<_> = options
            .iter()
            .filter(|o| o.decision != "decline" && o.decision != "accept")
            .collect();
        let button_top = (top + (content_height - 24.0) / 2.0).round();
        let mut x = rect.right() - 12.0;
        let mut decision = None;
        if !more.is_empty() {
            let button = Rect::from_min_size(Pos2::new(x - 24.0, button_top), Vec2::splat(24.0));
            let response = widgets::outline_button(
                ui,
                button,
                "approval-more",
                None,
                "",
                CornerRadius::same(8),
            );
            theme::paint_icon(ui, "ellipsis", button, 14.0, theme::foreground());
            if response.clicked() && !responding {
                self.open_menu = Some("approval-more".into());
            }
            let items: Vec<widgets::MenuEntry> = more
                .iter()
                .map(|o| widgets::MenuEntry::Item(widgets::MenuItem::new(&o.label)))
                .collect();
            let anchor =
                Rect::from_min_max(Pos2::new(button.right() - 200.0, button.top()), button.max);
            if let Some(index) = widgets::menu(
                ui,
                &mut self.open_menu,
                "approval-more",
                anchor,
                true,
                &items,
            ) {
                decision = Some(more[index].decision.clone());
            }
            x = button.left() - 6.0;
        }
        for option in primary.iter().rev() {
            let label_width = widgets::text_width(ui, &option.label, font(12.0, Weight::Medium));
            let button_width = (label_width + 16.0).ceil();
            let button = Rect::from_min_size(
                Pos2::new(x - button_width, button_top),
                Vec2::new(button_width, 24.0),
            );
            let clicked = if option.decision == "accept" {
                widgets::primary_button(
                    ui,
                    button,
                    ("approval", &option.decision),
                    &option.label,
                    !responding,
                )
                .clicked()
            } else {
                widgets::outline_button(
                    ui,
                    button,
                    ("approval", &option.decision),
                    None,
                    &option.label,
                    CornerRadius::same(8),
                )
                .clicked()
            };
            if clicked && !responding {
                decision = Some(option.decision.clone());
            }
            x = button.left() - 6.0;
        }

        // Label row and detail.
        let label = approval.label();
        let label_width = widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(content_left, top), Pos2::new(x - 8.0, top + 16.0)),
            label,
            font(11.0, Weight::Medium),
            warning(),
        );
        if let Some(app) = &approval.app_name {
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(content_left + label_width + 8.0, top),
                    Pos2::new(x - 40.0, top + 16.0),
                ),
                app,
                font(11.0, Weight::Regular),
                theme::muted_foreground(),
            );
        }
        if count > 1 {
            let counter = format!("1/{count}");
            let counter_width = widgets::text_width(ui, &counter, font(11.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(x - 8.0 - counter_width, top),
                    Pos2::new(x - 8.0, top + 16.0),
                ),
                &counter,
                font(11.0, Weight::Regular),
                theme::muted_foreground(),
            );
        }
        let detail = approval.detail.as_deref().unwrap_or(label);
        let clip = Rect::from_min_max(
            Pos2::new(content_left, top + 20.0),
            Pos2::new(x - 8.0, top + content_height),
        );
        let detail_galley = if approval.request_kind == "mcp-elicitation" {
            widgets::galley(
                ui,
                detail,
                font(12.0, Weight::Regular),
                theme::foreground(),
                16.0,
                clip.width(),
            )
        } else {
            widgets::galley(
                ui,
                detail,
                theme::mono(12.0),
                theme::foreground(),
                16.0,
                f32::INFINITY,
            )
        };
        ui.painter()
            .with_clip_rect(clip)
            .galley(clip.min, detail_galley, theme::foreground());

        if let Some(decision) = decision {
            let mut command = crate::model::command("thread.approval.respond", thread_id);
            command["requestId"] = json!(approval.request_id);
            command["decision"] = json!(decision);
            self.dispatch_reply(command, &approval.request_id);
        }
    }

    /// Question card ending `OVERLAP` px under the top of the composer `surface`.
    pub(crate) fn question_banner(
        &mut self,
        ui: &mut Ui,
        surface: Rect,
        input: &PendingUserInput,
        thread_id: &str,
    ) {
        let Some(question) = input.questions.get(self.question_index).cloned() else {
            return;
        };
        let height = self.question_banner_height(ui, input, surface.width());
        let rect = Rect::from_min_max(
            Pos2::new(surface.left() + INSET, surface.top() + OVERLAP - height),
            Pos2::new(surface.right() - INSET, surface.top() + OVERLAP),
        );
        banner_surface(ui, rect, neutral_surface(), neutral_outline());
        let collapsed = self.question_collapsed.as_deref() == Some(question.id.as_str());
        let responding = self.responding.contains(&input.request_id);

        // Header row toggles the card.
        let header = Rect::from_min_size(
            rect.min + Vec2::new(4.0, 4.0),
            Vec2::new(rect.width() - 8.0, 24.0),
        );
        let mut right = header.right();
        let mut dismissed = false;
        if input.dismissible {
            let dismiss =
                Rect::from_min_size(Pos2::new(right - 24.0, header.top()), Vec2::splat(24.0));
            if widgets::icon_button(
                ui,
                dismiss,
                "question-dismiss",
                "x",
                14.0,
                theme::muted_foreground(),
                "Dismiss question without answering",
            )
            .clicked()
            {
                dismissed = true;
            }
            right = dismiss.left() - 4.0;
        }
        let toggle = Rect::from_min_size(Pos2::new(right - 24.0, header.top()), Vec2::splat(24.0));
        theme::paint_icon(
            ui,
            if collapsed {
                "chevron-up"
            } else {
                "chevron-down"
            },
            toggle,
            14.0,
            theme::muted_foreground(),
        );
        right = toggle.left() - 4.0;
        if input.questions.len() > 1 {
            let counter = format!("{}/{}", self.question_index + 1, input.questions.len());
            let counter_width = widgets::text_width(ui, &counter, font(10.0, Weight::Medium));
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(right - counter_width, header.top()),
                    Pos2::new(right, header.bottom()),
                ),
                &counter,
                font(10.0, Weight::Medium),
                theme::muted_foreground(),
            );
            right -= counter_width + 4.0;
        }
        let header_response = ui.interact(
            Rect::from_min_max(header.min, Pos2::new(right, header.bottom())),
            ui.id().with("question-header"),
            Sense::click(),
        );
        let content_left = header.left() + 28.0;
        let header_width = widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(content_left, header.top()),
                Pos2::new(right, header.bottom()),
            ),
            &question.header,
            font(12.0, Weight::Medium),
            theme::muted_foreground(),
        );
        if collapsed {
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(content_left + header_width + 4.0, header.top()),
                    Pos2::new(right, header.bottom()),
                ),
                &question.question,
                font(12.0, Weight::Regular),
                theme::secondary_label(),
            );
        }
        if header_response.clicked()
            || ui.rect_contains_pointer(toggle) && ui.input(|i| i.pointer.primary_clicked())
        {
            self.question_collapsed = if collapsed {
                None
            } else {
                Some(question.id.clone())
            };
        }

        let mut selected_value = None;
        if !collapsed {
            let (visible, content_height, body_width) =
                body_metrics(ui, &question, rect.right() - 8.0 - content_left);
            let body = Rect::from_min_size(
                Pos2::new(content_left, header.bottom()),
                Vec2::new(rect.right() - 4.0 - content_left, visible),
            );
            let draft = self
                .question_drafts
                .get(&question.id)
                .cloned()
                .unwrap_or_default();
            let custom_active = !draft.custom.trim().is_empty() || !self.composer.trim().is_empty();
            let layouts = option_layouts(ui, &question, body_width);
            let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
            egui::ScrollArea::vertical()
                .id_salt(("question-body", &question.id))
                .max_height(visible)
                .auto_shrink([false, true])
                .show(&mut body_ui, |ui| {
                    let (content, _) = ui
                        .allocate_exact_size(Vec2::new(body_width, content_height), Sense::hover());
                    let mut y = content.top();
                    let text = widgets::galley(
                        ui,
                        &question.question,
                        font(14.0, Weight::Regular),
                        theme::alpha(theme::foreground(), 0.85),
                        20.0,
                        body_width,
                    );
                    let text_height = text.size().y;
                    ui.painter()
                        .galley(Pos2::new(content_left, y), text, theme::foreground());
                    y += text_height;
                    if question.multi_select {
                        widgets::text(
                            ui,
                            Pos2::new(content_left, y + 4.0),
                            "Select one or more options.",
                            font(12.0, Weight::Regular),
                            theme::secondary_label(),
                            16.0,
                        );
                        y += 20.0;
                    }
                    y += 8.0;
                    for (index, (option, layout)) in
                        question.options.iter().zip(layouts).enumerate()
                    {
                        if index > 0 {
                            y += 2.0;
                        }
                        let row = Rect::from_min_size(
                            Pos2::new(content_left, y),
                            Vec2::new(body_width, layout.height),
                        );
                        let response = ui.interact(
                            row,
                            ui.id().with(("question-option", &question.id, index)),
                            if responding {
                                Sense::hover()
                            } else {
                                Sense::click()
                            },
                        );
                        let selected = !custom_active && draft.selected.contains(&option.value);
                        if selected {
                            ui.painter().rect_filled(
                                row,
                                CornerRadius::same(6),
                                theme::overlay(0.0165),
                            );
                        } else if response.hovered() {
                            ui.painter().rect_filled(
                                row,
                                CornerRadius::same(6),
                                theme::overlay(0.009),
                            );
                        }
                        let color = if selected {
                            theme::foreground()
                        } else {
                            theme::alpha(theme::foreground(), 0.85)
                        };
                        let label_height = layout.label.size().y;
                        ui.painter().galley(
                            Pos2::new(row.left() + 10.0, row.top() + 8.0),
                            layout.label,
                            color,
                        );
                        if let Some(description) = layout.description {
                            ui.painter().galley(
                                Pos2::new(row.left() + 10.0, row.top() + 8.0 + label_height + 2.0),
                                description,
                                theme::secondary_label(),
                            );
                        }
                        let slot = Rect::from_min_size(
                            Pos2::new(row.right() - 10.0 - 20.0, row.center().y - 10.0),
                            Vec2::splat(20.0),
                        );
                        if selected {
                            theme::paint_icon(ui, "check", slot, 14.0, theme::primary());
                        } else if index < 9 {
                            let digit = (index + 1).to_string();
                            let digit_width = widgets::text_width(ui, &digit, theme::mono(10.0));
                            widgets::text_in(
                                ui,
                                Rect::from_min_max(
                                    Pos2::new(slot.center().x - digit_width / 2.0, slot.top()),
                                    slot.max,
                                ),
                                &digit,
                                theme::mono(10.0),
                                theme::muted_foreground(),
                            );
                        }
                        if response.clicked() {
                            selected_value = Some(option.value.clone());
                        }
                        y = row.bottom();
                    }
                });
            // Number keys pick options while the prompt editor is not focused.
            let editor_focused = ui.memory(|m| m.focused().is_some());
            if !responding && !editor_focused {
                for (index, key) in [
                    egui::Key::Num1,
                    egui::Key::Num2,
                    egui::Key::Num3,
                    egui::Key::Num4,
                    egui::Key::Num5,
                    egui::Key::Num6,
                    egui::Key::Num7,
                    egui::Key::Num8,
                    egui::Key::Num9,
                ]
                .iter()
                .enumerate()
                {
                    if index < question.options.len() && ui.input(|i| i.key_pressed(*key)) {
                        selected_value = Some(question.options[index].value.clone());
                    }
                }
            }
        }

        if let Some(value) = selected_value {
            let draft = self.question_drafts.entry(question.id.clone()).or_default();
            requests::toggle_option(&question, draft, &value);
            self.composer.clear();
            if !question.multi_select {
                self.advance_question(input, thread_id);
            }
        }
        if dismissed && !responding {
            let mut command = crate::model::command("thread.user-input.dismiss", thread_id);
            command["requestId"] = json!(input.request_id);
            self.dispatch_reply(command, &input.request_id);
        }
    }

    /// Whether the active question has an answer (options or composer text).
    pub(crate) fn question_can_advance(&self, input: &PendingUserInput) -> bool {
        let Some(question) = input.questions.get(self.question_index) else {
            return false;
        };
        let mut draft = self
            .question_drafts
            .get(&question.id)
            .cloned()
            .unwrap_or_default();
        if !self.composer.trim().is_empty() {
            draft.custom = self.composer.clone();
        }
        requests::resolve_answer(question, Some(&draft)).is_some()
    }

    /// Next question, or submits every answer on the last one.
    pub(crate) fn advance_question(&mut self, input: &PendingUserInput, thread_id: &str) {
        let Some(question) = input.questions.get(self.question_index) else {
            return;
        };
        if !self.composer.trim().is_empty() {
            let draft = self.question_drafts.entry(question.id.clone()).or_default();
            draft.custom = std::mem::take(&mut self.composer);
        }
        if !self.question_can_advance_with_drafts(question) {
            return;
        }
        if self.question_index + 1 < input.questions.len() {
            self.question_index += 1;
            let next = &input.questions[self.question_index];
            self.composer = self
                .question_drafts
                .get(&next.id)
                .map(|d| d.custom.clone())
                .unwrap_or_default();
            return;
        }
        let Some(answers) = requests::build_answers(&input.questions, &self.question_drafts) else {
            return;
        };
        let mut command = crate::model::command("thread.user-input.respond", thread_id);
        command["requestId"] = json!(input.request_id);
        command["answers"] = serde_json::Value::Object(answers);
        self.dispatch_reply(command, &input.request_id);
        self.composer.clear();
    }

    pub(crate) fn previous_question(&mut self, input: &PendingUserInput) {
        if self.question_index == 0 {
            return;
        }
        if let Some(question) = input.questions.get(self.question_index) {
            let draft = self.question_drafts.entry(question.id.clone()).or_default();
            draft.custom = std::mem::take(&mut self.composer);
        }
        self.question_index -= 1;
        let previous = &input.questions[self.question_index];
        self.composer = self
            .question_drafts
            .get(&previous.id)
            .map(|d| d.custom.clone())
            .unwrap_or_default();
    }

    fn question_can_advance_with_drafts(&self, question: &Question) -> bool {
        requests::resolve_answer(question, self.question_drafts.get(&question.id)).is_some()
    }
}
