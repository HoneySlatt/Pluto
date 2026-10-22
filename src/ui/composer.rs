//! Composer (`ChatComposer.tsx`, `ComposerSurface.tsx`, `ComposerPrimaryActions.tsx`,
//! `BranchToolbar.tsx`): prompt editor, model/traits/runtime pickers, send and
//! stop buttons, attached request banners and the checkout strip.
use eframe::egui::{
    self, Color32, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::{ModelSelection, now_iso};
use crate::requests::{PendingApproval, PendingUserInput};
use crate::theme::{self, Weight, font};
use crate::ui::requests_panel::OVERLAP;
use crate::ui::{cite, widgets};

const MIN_TEXT_HEIGHT: f32 = 70.0;
const MAX_TEXT_HEIGHT: f32 = 200.0;
/// The composer card (gray 16 on the stock palette).
fn card_surface() -> Color32 {
    theme::shade(16)
}
fn outline() -> Color32 {
    theme::shade(28)
}
const DESTRUCTIVE: Color32 = Color32::from_rgb(251, 65, 74); // --destructive
pub(crate) fn message_action_hover() -> Color32 {
    theme::primary_hover()
}

pub fn runtime_label(mode: &str) -> (&'static str, &'static str) {
    match mode {
        "approval-required" => ("Supervised", "lock"),
        "auto-accept-edits" => ("Auto-accept edits", "pen-line"),
        "auto" => ("Auto", "sparkles"),
        _ => ("Full access", "lock-open"),
    }
}

/// What the composer is waiting on, in upstream priority order.
#[derive(Clone)]
pub(crate) enum Mode {
    Normal,
    /// After a settled plan-mode turn proposed a plan (`showPlanFollowUpPrompt`).
    Plan(Box<crate::model::ProposedPlan>),
    Approval(PendingApproval, usize),
    Question(PendingUserInput),
}

/// The title of a message's thread: its text, or its first image's name
/// (`truncate(titleSeed)`, 50 characters).
fn title_seed(text: &str, images: &[crate::ui::attachments::ComposerImage]) -> String {
    let seed = match images.first() {
        Some(image) if text.is_empty() => format!("Image: {}", image.name),
        _ => text.to_owned(),
    };
    let chars: Vec<char> = seed.chars().collect();
    if chars.len() <= 50 {
        seed
    } else {
        format!("{}...", chars[..50].iter().collect::<String>())
    }
}

/// A `thread.turn.start` for an existing thread (the caller adds a
/// bootstrap for a draft).
pub(crate) fn turn_start_command(
    thread_id: &str,
    text: &str,
    images: &[crate::ui::attachments::ComposerImage],
    model: &serde_json::Value,
    runtime: &str,
    interaction: &str,
) -> serde_json::Value {
    json!({
        "type": "thread.turn.start",
        "commandId": uuid::Uuid::new_v4().to_string(),
        "threadId": thread_id,
        "message": {
            "messageId": uuid::Uuid::new_v4().to_string(),
            "role": "user",
            "text": text,
            "attachments": crate::ui::attachments::upload_payload(images),
        },
        "modelSelection": model,
        "titleSeed": title_seed(text, images),
        "runtimeMode": runtime,
        "interactionMode": if interaction.is_empty() { "default" } else { interaction },
        "createdAt": now_iso(),
    })
}

impl App {
    pub(crate) fn composer_mode(&mut self, draft: bool) -> Mode {
        if draft {
            return Mode::Normal;
        }
        let Some(thread) = self
            .state
            .detail
            .as_ref()
            .filter(|t| Some(&t.id) == self.selected.as_ref())
        else {
            return Mode::Normal;
        };
        // The task drawer closes on another thread.
        if self.tasks_thread.as_deref() != Some(thread.id.as_str()) {
            self.tasks_thread = Some(thread.id.clone());
            self.tasks_open = false;
        }
        let pending = crate::requests::derive(&thread.activities);
        // Requests that are no longer pending stop counting as in flight.
        let open: std::collections::HashSet<&str> = pending
            .approvals
            .iter()
            .map(|a| a.request_id.as_str())
            .chain(pending.user_inputs.iter().map(|i| i.request_id.as_str()))
            .collect();
        self.responding.retain(|id| open.contains(id.as_str()));
        if !pending.approvals.is_empty() || !pending.user_inputs.is_empty() {
            self.tasks_open = false;
        }
        if let Some(approval) = pending.approvals.first() {
            return Mode::Approval(approval.clone(), pending.approvals.len());
        }
        if let Some(input) = pending.user_inputs.first() {
            if self.question_request.as_deref() != Some(input.request_id.as_str()) {
                self.question_request = Some(input.request_id.clone());
                self.question_drafts.clear();
                self.question_index = 0;
                self.question_collapsed = None;
            }
            self.question_index = self
                .question_index
                .min(input.questions.len().saturating_sub(1));
            return Mode::Question(input.clone());
        }
        self.question_request = None;
        match self.plan_follow_up(thread) {
            Some(plan) => Mode::Plan(Box::new(plan)),
            None => Mode::Normal,
        }
    }

    fn composer_text_height(&self, ui: &Ui, width: f32) -> f32 {
        let collapsed = cite::collapse(&self.composer);
        let labels: Vec<String> = collapsed.tokens.iter().map(|(c, _)| c.label()).collect();
        let galley = cite::composer_galley(ui, &collapsed.display, &labels, width - 34.0);
        galley.size().y.clamp(MIN_TEXT_HEIGHT, MAX_TEXT_HEIGHT)
    }

    /// The attached images' row above the prompt (`mb-3 flex flex-wrap
    /// gap-2` of 64px thumbnails), wrapping to the editor width.
    fn image_strip_height(&self, width: f32) -> f32 {
        if self.composer_images.is_empty() {
            return 0.0;
        }
        let per_row = (((width - 34.0 + 8.0) / 72.0).floor() as usize).max(1);
        let rows = self.composer_images.len().div_ceil(per_row) as f32;
        rows * 64.0 + (rows - 1.0) * 8.0 + 12.0
    }

    fn surface_height(&self, ui: &Ui, width: f32, mode: &Mode) -> f32 {
        let strip = self.image_strip_height(width);
        match mode {
            Mode::Normal | Mode::Plan(_) => {
                17.0 + strip + self.composer_text_height(ui, width) + 10.0 + 28.0 + 19.0
            }
            Mode::Question(_) => 17.0 + self.composer_text_height(ui, width) + 16.0 + 28.0 + 17.0,
            Mode::Approval(..) => 17.0 + 32.0 + 17.0,
        }
    }

    fn banner_height(&self, ui: &Ui, width: f32, mode: &Mode) -> f32 {
        match mode {
            Mode::Normal => self
                .composer_tasks()
                .map_or(0.0, |plan| self.tasks_banner_height(ui, &plan, width)),
            Mode::Plan(_) => crate::ui::plan_card::PLAN_BANNER,
            Mode::Approval(approval, _) => self.approval_banner_height(approval),
            Mode::Question(input) => self.question_banner_height(ui, input, width),
        }
    }

    /// Top of the composer surface when pinned to the bottom of `body`.
    pub(crate) fn composer_layout_top(&self, ui: &Ui, body: Rect, width: f32, mode: &Mode) -> f32 {
        body.bottom() - 20.0 - 32.0 - self.surface_height(ui, width, mode)
    }

    /// Height the composer overlay covers above the window bottom, banner included.
    pub(crate) fn composer_overlay(&self, ui: &Ui, body: Rect, width: f32, mode: &Mode) -> f32 {
        let banner = (self.banner_height(ui, width, mode) - OVERLAP).max(0.0);
        // Measured: the list keeps 26px more than the composer overlay (pt-2 + clearance).
        body.bottom() - self.composer_layout_top(ui, body, width, mode) + banner + 26.0
    }

    pub(crate) fn composer(
        &mut self,
        ui: &mut Ui,
        top_left: Pos2,
        width: f32,
        draft: bool,
        mode: &Mode,
    ) {
        let height = self.surface_height(ui, width, mode);
        let text_height = match mode {
            Mode::Approval(..) => 32.0,
            _ => self.composer_text_height(ui, width),
        };
        let surface = Rect::from_min_size(top_left, Vec2::new(width, height));
        let thread_id = self.selected.clone().unwrap_or_default();

        // Attached banners sit behind the surface's top edge.
        match mode {
            Mode::Approval(approval, count) => {
                self.approval_banner(ui, surface, approval, *count, &thread_id)
            }
            Mode::Question(input) => self.question_banner(ui, surface, input, &thread_id),
            Mode::Plan(plan) => self.plan_banner(ui, surface, plan),
            Mode::Normal => match self.composer_tasks().filter(|_| !draft) {
                Some(plan) => self.tasks_banner(ui, surface, &plan),
                None => self.tasks_open = false,
            },
        }

        // Checkout strip, tucked 16px under the surface (`-mt-4`); only for a
        // repository (`shouldShowComposerContextStrip`, one environment).
        if self.workspace_is_repo() {
            let strip = Rect::from_min_max(
                Pos2::new(surface.left() + 22.0, surface.bottom() - 16.0),
                Pos2::new(surface.right() - 22.0, surface.bottom() + 32.0),
            );
            let strip_corners = CornerRadius {
                nw: 0,
                ne: 0,
                sw: 16,
                se: 16,
            };
            ui.painter().rect(
                strip,
                strip_corners,
                theme::shade(19),
                Stroke::new(1.0, theme::overlay(0.07)),
                StrokeKind::Inside,
            );
            self.checkout_strip(
                ui,
                Rect::from_min_max(Pos2::new(strip.left(), surface.bottom()), strip.max),
                draft,
            );
        }

        ui.painter().rect(
            surface,
            CornerRadius::same(22),
            card_surface(),
            Stroke::new(1.0, outline()),
            StrokeKind::Inside,
        );

        self.take_incoming_images();
        // Answers to agent questions take no attachments here.
        let normal = matches!(mode, Mode::Normal | Mode::Plan(_));
        let strip_height = if normal {
            self.image_strip_height(width)
        } else {
            0.0
        };
        if strip_height > 0.0 {
            self.composer_image_strip(ui, surface.min + Vec2::new(17.0, 17.0), width - 34.0);
        }
        let text_rect = Rect::from_min_size(
            surface.min + Vec2::new(17.0, 17.0 + strip_height),
            Vec2::new(width - 34.0, text_height),
        );
        let placeholder = match mode {
            Mode::Approval(..) => "Resolve this approval request to continue",
            Mode::Question(_) => {
                "Type your own answer, or leave this blank to use the selected option"
            }
            Mode::Normal => "Ask for changes, send follow-ups, or attach images",
            Mode::Plan(_) => "Add feedback to refine the plan, or leave this blank to implement it",
        };
        // Attached images count as context, which hides the placeholder.
        if (self.composer.is_empty() && self.composer_images.is_empty())
            || matches!(mode, Mode::Approval(..))
        {
            // Painted here so it shares the editor's 22.75px line box.
            widgets::text(
                ui,
                text_rect.min,
                placeholder,
                theme::composer_font(),
                theme::alpha(theme::muted_foreground(), 0.75),
                theme::composer_line(),
            );
        }
        if matches!(mode, Mode::Approval(..)) {
            return;
        }

        // Prompt editor.
        let editor_id = ui.id().with("composer-editor");
        self.cite.editor_id = Some(editor_id);
        let focused = ui.memory(|m| m.has_focus(editor_id));
        // The suggestion menu takes its keys first.
        if normal {
            self.command_menu_keys(ui, editor_id, draft);
        }
        // Ctrl+Enter in a citation's comment editor sends too.
        let submit = (focused && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)))
            || std::mem::take(&mut self.cite.send);
        if focused && normal && self.composer_images.is_empty() {
            self.recall_prompt(ui, editor_id, text_rect.width());
        }
        // Citation links are edited as chips, one character each.
        cite::pad_chips(&mut self.composer);
        let collapsed = cite::collapse(&self.composer);
        cite::set_copy_tokens(&collapsed);
        let labels: Vec<String> = collapsed.tokens.iter().map(|(c, _)| c.label()).collect();
        let mut display = collapsed.display.clone();
        let mut editor_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
        egui::ScrollArea::vertical()
            .id_salt("composer-scroll")
            .max_height(text_height)
            .show(&mut editor_ui, |ui| {
                let output = egui::TextEdit::multiline(&mut display)
                    .id(editor_id)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(theme::composer_font())
                    .text_color(theme::foreground())
                    .desired_width(text_rect.width())
                    .min_size(Vec2::new(text_rect.width(), text_height))
                    .layouter(&mut |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                        cite::composer_galley(ui, text.as_str(), &labels, wrap)
                    })
                    .show(ui);
                if display != collapsed.display {
                    self.composer = cite::expand(&display, &collapsed.tokens);
                    if cite::collapse(&self.composer).display != display {
                        self.remap_composer_caret(ui.ctx(), &display, &collapsed.tokens);
                    }
                }
                self.composer_chips(ui, &output.galley, output.galley_pos, &collapsed);
            });

        // An image paste: egui only pastes text, and drops a paste the text
        // clipboard cannot serve, but still reports the V key's release.
        let pasted = normal
            && focused
            && ui.input(|i| {
                i.events.iter().any(|event| {
                    matches!(event, egui::Event::Key { key: Key::V, pressed: false, modifiers, .. }
                        if modifiers.command)
                })
            });
        if pasted {
            crate::ui::attachments::paste_clipboard_image(&self.incoming_images, ui.ctx());
        }
        // Image files dropped on the composer.
        let dropped = ui.input(|i| i.raw.dropped_files.clone());
        let over = ui
            .input(|i| i.pointer.latest_pos())
            .is_none_or(|pos| surface.contains(pos));
        if normal && !dropped.is_empty() && over {
            crate::ui::attachments::add_dropped_files(&self.incoming_images, ui.ctx(), dropped);
        }

        if normal {
            self.composer_command_menu(ui, surface, editor_id, draft);
        }

        // Toolbar.
        let bar_top = text_rect.bottom()
            + if matches!(mode, Mode::Question(_)) {
                16.0
            } else {
                10.0
            };
        let mut x = surface.left() + 7.0;
        let selection = self.composer_model_selection(draft);
        let (model_label, provider_icon) = self.model_label(&selection);
        let model_rect = self.picker(ui, x, bar_top, provider_icon, &model_label, "model-picker");
        if provider_icon.is_some()
            && let Some(badge) = self.instance_badge(&selection.instance_id)
        {
            // Ringed with `--contrast-input`.
            let icon = Rect::from_min_size(
                Pos2::new(model_rect.left() + 11.0, bar_top + 6.0),
                Vec2::splat(16.0),
            );
            crate::ui::instance_badge::paint(ui, icon, 2.0, &badge, theme::input(), 1.0);
        }
        x = model_rect.right() + 6.0;
        let mut traits_rect = None;
        if let Some(trait_label) = self.traits_label(&selection) {
            self.separator(ui, x, bar_top);
            x += 7.0;
            let rect = self.picker(ui, x, bar_top, None, &trait_label, "traits-picker");
            x = rect.right() + 6.0;
            traits_rect = Some(rect);
        }
        self.separator(ui, x, bar_top);
        x += 7.0;
        let runtime = self.composer_runtime_mode(draft);
        let (runtime_text, runtime_icon) = runtime_label(&runtime);
        let runtime_rect = self.picker(
            ui,
            x,
            bar_top,
            Some(runtime_icon),
            runtime_text,
            "runtime-picker",
        );
        self.model_picker(ui, model_rect, draft);
        if let Some(rect) = traits_rect {
            self.traits_menu(ui, rect, draft);
        }
        if self.plan_mode_enabled(draft) {
            let x = runtime_rect.right() + 6.0;
            self.separator(ui, x, bar_top);
            self.interaction_toggle(ui, x + 7.0, bar_top, draft);
        }
        self.runtime_menu(ui, runtime_rect, draft);

        let right = surface.right() - 17.0;
        let actions_left = match mode {
            Mode::Question(input) => {
                self.question_actions(ui, right, bar_top, input, &thread_id, submit)
            }
            Mode::Plan(plan) => self.plan_actions(ui, right, bar_top, plan, submit),
            _ => self.send_actions(ui, right, bar_top, draft, submit),
        };
        let attach = Rect::from_min_size(
            Pos2::new(actions_left - 8.0 - 28.0, bar_top),
            Vec2::splat(28.0),
        );
        if widgets::icon_button(
            ui,
            attach,
            "attach",
            "paperclip",
            16.0,
            theme::muted_foreground(),
            "Attach files",
        )
        .clicked()
            && normal
        {
            crate::ui::attachments::pick_files(&self.incoming_images, ui.ctx());
        }
    }

    /// The attached images: 64px thumbnails (`h-16 w-16 rounded-lg border
    /// border-border/80 bg-background`) with their remove button.
    fn composer_image_strip(&mut self, ui: &mut Ui, origin: Pos2, width: f32) {
        let per_row = (((width + 8.0) / 72.0).floor() as usize).max(1);
        let mut remove = None;
        let mut preview = None;
        for (index, image) in self.composer_images.iter().enumerate() {
            let cell = Rect::from_min_size(
                origin
                    + Vec2::new(
                        (index % per_row) as f32 * 72.0,
                        (index / per_row) as f32 * 72.0,
                    ),
                Vec2::splat(64.0),
            );
            let radius = CornerRadius::same(8);
            let cell_response = ui
                .interact(
                    cell,
                    ui.id().with(("composer-image", &image.id)),
                    Sense::click(),
                )
                .on_hover_text(format!("Preview {}", image.name));
            if cell_response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomIn);
            }
            if cell_response.clicked() {
                preview = Some(index);
            }
            ui.painter().rect_filled(cell, radius, theme::background());
            crate::ui::attachments::paint_cover(
                ui,
                cell.shrink(1.0),
                &format!("composer:{}", image.id),
                &image.preview,
                CornerRadius::same(7),
            );
            ui.painter().rect_stroke(
                cell,
                radius,
                Stroke::new(1.0, theme::alpha(theme::border(), 0.8)),
                StrokeKind::Inside,
            );
            // `variant="media-close" size="icon-xs"` at `right-1 top-1`.
            let close = Rect::from_min_size(
                Pos2::new(cell.right() - 4.0 - 24.0, cell.top() + 4.0),
                Vec2::splat(24.0),
            );
            let response = ui
                .interact(
                    close,
                    ui.id().with(("composer-image-remove", &image.id)),
                    Sense::click(),
                )
                .on_hover_text(format!("Remove {}", image.name));
            let fill = if response.hovered() { 0.8 } else { 0.65 };
            ui.painter().rect(
                close,
                CornerRadius::same(6),
                theme::alpha(Color32::BLACK, fill),
                Stroke::new(1.0, theme::alpha(Color32::WHITE, 0.2)),
                StrokeKind::Outside,
            );
            theme::paint_icon(ui, "x", close, 14.0, Color32::WHITE);
            if response.clicked() {
                remove = Some(index);
            }
        }
        if let Some(index) = remove {
            self.composer_images.remove(index);
        } else if let Some(index) = preview {
            // The full images, decoded from the attachments' bytes.
            let items = self
                .composer_images
                .iter()
                .filter_map(|image| {
                    Some(crate::ui::lightbox::Item {
                        name: image.name.clone(),
                        key: format!("composer:{}", image.id),
                        image: std::sync::Arc::new(crate::ui::attachments::decode(&image.bytes)?),
                        source: None,
                    })
                })
                .collect();
            self.open_lightbox(items, index);
        }
    }

    fn running(&self) -> bool {
        self.current_thread().is_some_and(|t| t.is_running())
    }

    /// Send button, or Stop while a turn runs. Returns the left edge.
    fn send_actions(
        &mut self,
        ui: &mut Ui,
        right: f32,
        bar_top: f32,
        draft: bool,
        submit: bool,
    ) -> f32 {
        let send = Rect::from_min_size(Pos2::new(right - 32.0, bar_top - 2.0), Vec2::splat(32.0));
        let has_content = !self.composer.trim().is_empty() || !self.composer_images.is_empty();
        let running = !draft && self.running();
        if running {
            // Stop, then Send while there is something to queue (`gap-2`).
            let stop = if has_content {
                send.translate(Vec2::new(-32.0 - 8.0, 0.0))
            } else {
                send
            };
            let response = ui.interact(stop, ui.id().with("stop"), Sense::click());
            let fill = if response.hovered() {
                DESTRUCTIVE
            } else {
                theme::alpha(DESTRUCTIVE, 0.9)
            };
            ui.painter().circle_filled(stop.center(), 16.0, fill);
            theme::paint_icon(ui, "stop-square", stop, 12.0, Color32::WHITE);
            if response.on_hover_text("Stop generation").clicked() {
                if let Some(thread) = self.selected.clone() {
                    self.drain_queue(&thread);
                }
                self.interrupt();
            }
            if !has_content {
                return stop.left();
            }
        }
        let can_send = has_content && self.connected();
        let response = ui.interact(
            send,
            ui.id().with("send"),
            if can_send {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let fill = if response.hovered() && can_send {
            message_action_hover()
        } else {
            theme::primary()
        };
        let opacity = if can_send { 1.0 } else { 0.64 };
        ui.painter()
            .circle_filled(send.center(), 16.0, theme::alpha(fill, opacity));
        theme::paint_icon(
            ui,
            "send-arrow",
            send,
            14.0,
            theme::alpha(Color32::WHITE, opacity),
        );
        let response = if running {
            response.on_hover_text("Queue message")
        } else {
            response
        };
        if (response.clicked() || submit) && can_send {
            self.submit_message(draft);
        }
        if running {
            send.left() - 8.0 - 32.0
        } else {
            send.left()
        }
    }

    /// Previous / Next / Submit for a pending question. Returns the left edge.
    fn question_actions(
        &mut self,
        ui: &mut Ui,
        right: f32,
        bar_top: f32,
        input: &PendingUserInput,
        thread_id: &str,
        submit: bool,
    ) -> f32 {
        let responding = self.responding.contains(&input.request_id);
        let last = self.question_index + 1 >= input.questions.len();
        let label = if responding {
            "Submitting..."
        } else if last {
            "Submit"
        } else {
            "Next"
        };
        let enabled = !responding && self.question_can_advance(input);
        let label_font = font(14.0, Weight::Medium);
        let width = (widgets::text_width(ui, label, label_font.clone()) + 24.0).ceil();
        let pill = Rect::from_min_size(Pos2::new(right - width, bar_top), Vec2::new(width, 28.0));
        let response = ui.interact(
            pill,
            ui.id().with("question-next"),
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let fill = if response.hovered() && enabled {
            message_action_hover()
        } else {
            theme::primary()
        };
        let opacity = if enabled { 1.0 } else { 0.64 };
        ui.painter()
            .rect_filled(pill, CornerRadius::same(14), theme::alpha(fill, opacity));
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(pill.left() + 12.0, pill.top()), pill.max),
            label,
            label_font,
            theme::alpha(Color32::WHITE, opacity),
        );
        if (response.clicked() || submit) && enabled {
            self.advance_question(input, thread_id);
        }
        let mut left = pill.left();
        if self.question_index > 0 {
            let previous =
                Rect::from_min_size(Pos2::new(left - 8.0 - 28.0, bar_top), Vec2::splat(28.0));
            let response = widgets::outline_button(
                ui,
                previous,
                "question-previous",
                None,
                "",
                CornerRadius::same(8),
            );
            theme::paint_icon(ui, "chevron-left", previous, 14.0, theme::foreground());
            if response.on_hover_text("Previous question").clicked() && !responding {
                self.previous_question(input);
            }
            left = previous.left();
        }
        left
    }

    fn interrupt(&mut self) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        let mut command = crate::model::command("thread.turn.interrupt", &thread.id);
        if let Some(turn) = thread
            .session
            .as_ref()
            .and_then(|s| s.active_turn_id.clone())
        {
            command["turnId"] = json!(turn);
        }
        self.dispatch(command);
    }

    fn separator(&self, ui: &Ui, x: f32, bar_top: f32) {
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(x, bar_top + 6.0), Vec2::new(1.0, 16.0)),
            0.0,
            theme::border(),
        );
    }

    /// Ghost picker button: optional 16px icon, 14px medium label, chevron.
    fn picker(
        &mut self,
        ui: &mut Ui,
        x: f32,
        top: f32,
        icon: Option<&str>,
        label: &str,
        id: &str,
    ) -> Rect {
        let label_font = font(14.0, Weight::Medium);
        let label_width = widgets::text_width(ui, label, label_font.clone());
        let width = 11.0 + if icon.is_some() { 22.0 } else { 0.0 } + label_width + 4.0 + 14.0 + 9.0;
        let rect = Rect::from_min_size(Pos2::new(x, top), Vec2::new(width, 28.0));
        let response = ui.interact(rect, ui.id().with(id), Sense::click());
        let open = self.open_menu.as_deref() == Some(id);
        if response.hovered() || open {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), theme::accent());
        }
        let color = if response.hovered() {
            theme::foreground()
        } else {
            theme::muted_foreground()
        };
        let mut cx = x + 11.0;
        if let Some(icon) = icon {
            // Brand glyphs keep their own color (Claude's orange).
            let tint = match icon {
                "claude" => Color32::from_rgb(0xd9, 0x77, 0x57),
                _ => color,
            };
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(Pos2::new(cx, top + 6.0), Vec2::splat(16.0)),
                16.0,
                tint,
            );
            cx += 22.0;
        }
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(cx, top), rect.max),
            label,
            label_font,
            color,
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(cx + label_width + 4.0, top + 7.0),
                Vec2::splat(14.0),
            ),
            14.0,
            color,
        );
        if response.clicked() {
            self.open_menu = if self.open_menu.as_deref() == Some(id) {
                None
            } else {
                Some(id.to_owned())
            };
        }
        rect
    }

    fn checkout_strip(&mut self, ui: &mut Ui, rect: Rect, draft: bool) {
        let muted = if draft {
            theme::muted_foreground()
        } else {
            theme::alpha(theme::muted_foreground(), 0.7)
        };
        let label = if draft {
            "Current checkout"
        } else {
            "Local checkout"
        };
        let left = rect.left() + 4.0;
        let label_font = font(12.0, Weight::Regular);
        theme::paint_icon(
            ui,
            "folder",
            Rect::from_min_size(Pos2::new(left + 8.0, rect.top() + 10.0), Vec2::splat(12.0)),
            12.0,
            muted,
        );
        let width = widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(left + 24.0, rect.top() + 4.0),
                Pos2::new(rect.center().x, rect.top() + 28.0),
            ),
            label,
            label_font.clone(),
            muted,
        );
        if draft {
            theme::paint_icon(
                ui,
                "chevron-down",
                Rect::from_min_size(
                    Pos2::new(left + 28.0 + width, rect.top() + 10.0),
                    Vec2::splat(12.0),
                ),
                12.0,
                theme::alpha(muted, 0.5),
            );
        }
        let branch = self
            .current_thread()
            .and_then(|t| t.branch.clone())
            .unwrap_or_else(|| "main".into());
        let branch_color = theme::alpha(theme::muted_foreground(), 0.7);
        let branch_width = widgets::text_width(ui, &branch, label_font.clone());
        let right = rect.right() - 8.0;
        let start = right - 7.0 - 12.0 - 2.0 - branch_width - 2.0 - 12.0 - 7.0;
        theme::paint_icon(
            ui,
            "git-branch",
            Rect::from_min_size(Pos2::new(start + 7.0, rect.top() + 10.0), Vec2::splat(12.0)),
            12.0,
            branch_color,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(start + 21.0, rect.top() + 4.0),
                Pos2::new(right, rect.top() + 28.0),
            ),
            &branch,
            label_font,
            branch_color,
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(start + 23.0 + branch_width, rect.top() + 10.0),
                Vec2::splat(12.0),
            ),
            12.0,
            branch_color,
        );
    }

    pub(crate) fn providers(&self) -> &[Value] {
        self.state.config["providers"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(crate) fn provider_driver(&self, instance_id: &str) -> Option<String> {
        self.providers()
            .iter()
            .find(|p| p["instanceId"] == instance_id)
            .and_then(|p| p["driver"].as_str().map(str::to_owned))
    }

    pub(crate) fn model_entry(&self, selection: &ModelSelection) -> Option<Value> {
        self.providers()
            .iter()
            .find(|p| p["instanceId"] == selection.instance_id.as_str())
            .and_then(|p| {
                p["models"]
                    .as_array()?
                    .iter()
                    .find(|m| m["slug"] == selection.model.as_str())
                    .cloned()
            })
    }

    pub(crate) fn model_label(&self, selection: &ModelSelection) -> (String, Option<&'static str>) {
        let label = self
            .model_entry(selection)
            .and_then(|m| {
                m["displayName"]
                    .as_str()
                    .or(m["name"].as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| selection.model.clone());
        let icon = self
            .provider_driver(&selection.instance_id)
            .and_then(|driver| crate::ui::pickers::provider_icon(&driver))
            .map(|(icon, _)| icon);
        (label, icon)
    }

    /// Label of the first select trait (reasoning effort), like `TraitsPicker`.
    pub(crate) fn traits_label(&self, selection: &ModelSelection) -> Option<String> {
        let model = self.model_entry(selection)?;
        let descriptor = model["capabilities"]["optionDescriptors"]
            .as_array()?
            .iter()
            .find(|d| d["type"] == "select")?
            .clone();
        let id = descriptor["id"].as_str()?;
        // Options arrive as `[{id, value}]` (the contract) or as an id map.
        let chosen = selection
            .options
            .as_ref()
            .and_then(|o| match o {
                Value::Array(list) => list.iter().find(|e| e["id"] == id).map(|e| &e["value"]),
                _ => o.get(id),
            })
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| descriptor["currentValue"].as_str().map(str::to_owned))?;
        descriptor["options"]
            .as_array()?
            .iter()
            .find(|o| o["id"] == chosen.as_str())
            .and_then(|o| o["label"].as_str().map(str::to_owned))
    }

    pub(crate) fn composer_model_selection(&self, draft: bool) -> ModelSelection {
        if let Some(selection) = self.composer_selection.get(&self.composer_key(draft)) {
            return selection.clone();
        }
        if !draft && let Some(thread) = self.current_thread() {
            return thread.model_selection.clone();
        }
        // A draft starts on its project's default model.
        let settings = match self.draft_project().filter(|_| draft) {
            Some(project) => self.project_default_model(&project),
            None => self.state.config["settings"]["defaultModelSelection"].clone(),
        };
        if let Ok(selection) = serde_json::from_value::<ModelSelection>(settings)
            && !selection.model.is_empty()
        {
            return selection;
        }
        for provider in self.providers() {
            if provider["status"] != "ready" {
                continue;
            }
            let models = provider["models"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default();
            if let Some(model) = models
                .iter()
                .find(|m| m["isDefault"] == true)
                .or(models.first())
            {
                return ModelSelection {
                    instance_id: provider["instanceId"].as_str().unwrap_or_default().into(),
                    model: model["slug"].as_str().unwrap_or_default().into(),
                    options: None,
                };
            }
        }
        ModelSelection::default()
    }

    pub(crate) fn composer_runtime_mode(&self, draft: bool) -> String {
        if let Some(mode) = self.composer_runtime.get(&self.composer_key(draft)) {
            return mode.clone();
        }
        if !draft && let Some(thread) = self.current_thread() {
            return thread.runtime_mode.clone();
        }
        self.state.config["settings"]["defaultRuntimeMode"]
            .as_str()
            .unwrap_or("full-access")
            .to_owned()
    }

    /// The composer's model selection as sent on the wire.
    pub(crate) fn composer_model_json(&self, draft: bool) -> serde_json::Value {
        let selection = self.composer_model_selection(draft);
        let mut model = json!({"instanceId": selection.instance_id, "model": selection.model});
        if let Some(options) = &selection.options {
            model["options"] = options.clone();
        }
        model
    }

    /// Sends the composer's message; during a turn of an existing thread the
    /// "Queue" follow-up behavior queues it instead (`followUpBehavior`).
    /// Prompt recall (`composerPromptHistory.ts`): ArrowUp on an empty
    /// composer, or from the first row of a recalled prompt, brings back the
    /// open thread's previous prompt; ArrowDown from the last row walks
    /// forward, emptying the composer past the newest. The caret lands at
    /// the end; anywhere else the arrows move it as usual.
    fn recall_prompt(&mut self, ui: &mut Ui, editor_id: egui::Id, wrap: f32) {
        use crate::ui::prompt_history::{self, Direction};
        let pressed = ui.input(|i| {
            if !i.modifiers.is_none() {
                None
            } else if i.key_pressed(Key::ArrowUp) {
                Some((Direction::Backward, Key::ArrowUp))
            } else if i.key_pressed(Key::ArrowDown) {
                Some((Direction::Forward, Key::ArrowDown))
            } else {
                None
            }
        });
        let (Some((direction, key)), Some(thread)) = (pressed, self.current_thread()) else {
            return;
        };
        // A recall belongs to the thread it was made in.
        let position = self
            .prompt_recall
            .as_ref()
            .filter(|(id, _)| *id == thread.id)
            .map(|(_, position)| position.clone());
        if position.is_none() && !self.composer.is_empty() {
            return;
        }
        let collapsed = cite::collapse(&self.composer);
        let labels: Vec<String> = collapsed.tokens.iter().map(|(c, _)| c.label()).collect();
        let galley = cite::composer_galley(ui, &collapsed.display, &labels, wrap);
        let row = egui::TextEdit::load_state(ui.ctx(), editor_id)
            .and_then(|state| state.cursor.char_range())
            .map_or(0, |range| galley.layout_from_cursor(range.primary).row);
        let on_edge = match direction {
            Direction::Backward => row == 0,
            Direction::Forward => row + 1 >= galley.rows.len(),
        };
        if !on_edge {
            return;
        }
        let entries = prompt_history::entries(
            thread
                .messages
                .iter()
                .map(|m| (m.id.as_str(), m.role.as_str(), m.text.as_str())),
        );
        let Some(step) =
            prompt_history::step(direction, &entries, position.as_ref(), &self.composer)
        else {
            return;
        };
        ui.input_mut(|i| i.consume_key(Modifiers::NONE, key));
        self.composer = step.prompt;
        self.prompt_recall = step.position.map(|position| (thread.id.clone(), position));
        let end = cite::collapse(&self.composer).display.chars().count();
        let mut state = egui::TextEdit::load_state(ui.ctx(), editor_id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(end),
            )));
        state.store(ui.ctx(), editor_id);
    }

    fn submit_message(&mut self, draft: bool) {
        if !draft
            && self.running()
            && self.client_setting("followUpBehavior") != "steer"
            && let Some(thread) = self.current_thread()
        {
            let model = self.composer_model_json(false);
            let runtime = self.composer_runtime_mode(false);
            self.enqueue_composer(&thread, model, runtime);
            return;
        }
        self.send_message(draft);
    }

    fn send_message(&mut self, draft: bool) {
        let text = self.composer.trim().to_owned();
        if text.is_empty() && self.composer_images.is_empty() {
            return;
        }
        let runtime = self.composer_runtime_mode(draft);
        let now = now_iso();
        // `assistantCitationsToPlainText`: titles read quotes, not links.
        let title = title_seed(
            crate::ui::citations::to_plain_text(&text).trim(),
            &self.composer_images,
        );
        let model = self.composer_model_json(draft);
        let interaction = self.composer_interaction_mode(draft);
        let (thread_id, bootstrap) = if draft {
            let Some(project) = self.draft_project() else {
                return;
            };
            let thread_id = uuid::Uuid::new_v4().to_string();
            let bootstrap = json!({"createThread": {
                "projectId": project.id,
                "title": title,
                "modelSelection": model,
                "runtimeMode": runtime,
                "interactionMode": interaction,
                "branch": null,
                "worktreePath": null,
                "createdAt": now,
            }});
            (thread_id, Some(bootstrap))
        } else {
            let Some(thread) = self.current_thread() else {
                return;
            };
            (thread.id.clone(), None)
        };
        let mut command = turn_start_command(
            &thread_id,
            &text,
            &self.composer_images,
            &model,
            &runtime,
            &interaction,
        );
        if let Some(bootstrap) = bootstrap {
            command["bootstrap"] = bootstrap;
        }
        self.dispatch(command);
        self.composer.clear();
        self.composer_images.clear();
        if draft {
            self.select_thread(Some(thread_id));
        }
    }
}
