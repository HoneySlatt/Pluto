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
use crate::ui::widgets;

const LINE: f32 = 22.75;
const MIN_TEXT_HEIGHT: f32 = 70.0;
const MAX_TEXT_HEIGHT: f32 = 200.0;
const SURFACE: Color32 = Color32::from_rgb(16, 16, 16);
const OUTLINE: Color32 = Color32::from_rgb(28, 28, 28);
const DESTRUCTIVE: Color32 = Color32::from_rgb(251, 65, 74); // --destructive
const MESSAGE_ACTION_HOVER: Color32 = Color32::from_rgb(48, 97, 218);

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
    Approval(PendingApproval, usize),
    Question(PendingUserInput),
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
        let pending = crate::requests::derive(&thread.activities);
        // Requests that are no longer pending stop counting as in flight.
        let open: std::collections::HashSet<&str> = pending
            .approvals
            .iter()
            .map(|a| a.request_id.as_str())
            .chain(pending.user_inputs.iter().map(|i| i.request_id.as_str()))
            .collect();
        self.responding.retain(|id| open.contains(id.as_str()));
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
        Mode::Normal
    }

    fn composer_text_height(&self, ui: &Ui, width: f32) -> f32 {
        let galley = widgets::galley(
            ui,
            &self.composer,
            font(14.0, Weight::Regular),
            theme::FOREGROUND,
            LINE,
            width - 34.0,
        );
        galley.size().y.clamp(MIN_TEXT_HEIGHT, MAX_TEXT_HEIGHT)
    }

    fn surface_height(&self, ui: &Ui, width: f32, mode: &Mode) -> f32 {
        match mode {
            Mode::Normal => 17.0 + self.composer_text_height(ui, width) + 10.0 + 28.0 + 19.0,
            Mode::Question(_) => 17.0 + self.composer_text_height(ui, width) + 16.0 + 28.0 + 17.0,
            Mode::Approval(..) => 17.0 + 32.0 + 17.0,
        }
    }

    fn banner_height(&self, ui: &Ui, width: f32, mode: &Mode) -> f32 {
        match mode {
            Mode::Normal => 0.0,
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
            Mode::Normal => {}
        }

        // Checkout strip, tucked 16px under the surface (`-mt-4`).
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
            Color32::from_rgb(19, 19, 19),
            Stroke::new(1.0, theme::white(0.07)),
            StrokeKind::Inside,
        );
        self.checkout_strip(
            ui,
            Rect::from_min_max(Pos2::new(strip.left(), surface.bottom()), strip.max),
            draft,
        );

        ui.painter().rect(
            surface,
            CornerRadius::same(22),
            SURFACE,
            Stroke::new(1.0, OUTLINE),
            StrokeKind::Inside,
        );

        let text_rect = Rect::from_min_size(
            surface.min + Vec2::new(17.0, 17.0),
            Vec2::new(width - 34.0, text_height),
        );
        let placeholder = match mode {
            Mode::Approval(..) => "Resolve this approval request to continue",
            Mode::Question(_) => {
                "Type your own answer, or leave this blank to use the selected option"
            }
            Mode::Normal => "Ask for changes, send follow-ups, or attach images",
        };
        if self.composer.is_empty() || matches!(mode, Mode::Approval(..)) {
            // Painted here so it shares the editor's 22.75px line box.
            widgets::text(
                ui,
                text_rect.min,
                placeholder,
                font(14.0, Weight::Regular),
                theme::alpha(theme::MUTED_FOREGROUND, 0.75),
                LINE,
            );
        }
        if matches!(mode, Mode::Approval(..)) {
            return;
        }

        // Prompt editor.
        let editor_id = ui.id().with("composer-editor");
        let focused = ui.memory(|m| m.has_focus(editor_id));
        let submit = focused && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        let mut editor_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
        egui::ScrollArea::vertical()
            .id_salt("composer-scroll")
            .max_height(text_height)
            .show(&mut editor_ui, |ui| {
                ui.add_sized(
                    Vec2::new(text_rect.width(), text_height),
                    egui::TextEdit::multiline(&mut self.composer)
                        .id(editor_id)
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .font(font(14.0, Weight::Regular))
                        .text_color(theme::FOREGROUND)
                        .desired_width(text_rect.width())
                        .layouter(&mut |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                            let mut job = egui::text::LayoutJob::default();
                            job.append(
                                text.as_str(),
                                0.0,
                                widgets::format(
                                    font(14.0, Weight::Regular),
                                    theme::FOREGROUND,
                                    LINE,
                                ),
                            );
                            job.wrap.max_width = wrap;
                            widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job)))
                        }),
                );
            });

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
        self.runtime_menu(ui, runtime_rect, draft);

        let right = surface.right() - 17.0;
        let actions_left = match mode {
            Mode::Question(input) => {
                self.question_actions(ui, right, bar_top, input, &thread_id, submit)
            }
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
            theme::MUTED_FOREGROUND,
            "Attach files",
        )
        .clicked()
        {
            self.status = Some("Attachments are not implemented yet".into());
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
        if !draft && self.running() {
            let response = ui.interact(send, ui.id().with("stop"), Sense::click());
            let fill = if response.hovered() {
                DESTRUCTIVE
            } else {
                theme::alpha(DESTRUCTIVE, 0.9)
            };
            ui.painter().circle_filled(send.center(), 16.0, fill);
            theme::paint_icon(ui, "stop-square", send, 12.0, Color32::WHITE);
            if response.on_hover_text("Stop generation").clicked() {
                self.interrupt();
            }
            return send.left();
        }
        let can_send = !self.composer.trim().is_empty() && self.connected();
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
            MESSAGE_ACTION_HOVER
        } else {
            theme::PRIMARY
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
        if (response.clicked() || submit) && can_send {
            self.send_message(draft);
        }
        send.left()
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
            MESSAGE_ACTION_HOVER
        } else {
            theme::PRIMARY
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
            theme::paint_icon(ui, "chevron-left", previous, 14.0, theme::FOREGROUND);
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
            theme::BORDER,
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
                .rect_filled(rect, CornerRadius::same(8), theme::ACCENT);
        }
        let color = if response.hovered() {
            theme::FOREGROUND
        } else {
            theme::MUTED_FOREGROUND
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
            theme::MUTED_FOREGROUND
        } else {
            theme::alpha(theme::MUTED_FOREGROUND, 0.7)
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
            .and_then(|t| t.branch)
            .unwrap_or_else(|| "main".into());
        let branch_color = theme::alpha(theme::MUTED_FOREGROUND, 0.7);
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

    pub(crate) fn providers(&self) -> Vec<Value> {
        self.state.config["providers"]
            .as_array()
            .cloned()
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
            return thread.model_selection;
        }
        let settings = &self.state.config["settings"]["defaultModelSelection"];
        if let Ok(selection) = serde_json::from_value::<ModelSelection>(settings.clone())
            && !selection.model.is_empty()
        {
            return selection;
        }
        for provider in self.providers() {
            if provider["status"] != "ready" {
                continue;
            }
            let models = provider["models"].as_array().cloned().unwrap_or_default();
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
            return thread.runtime_mode;
        }
        self.state.config["settings"]["defaultRuntimeMode"]
            .as_str()
            .unwrap_or("full-access")
            .to_owned()
    }

    fn send_message(&mut self, draft: bool) {
        let text = self.composer.trim().to_owned();
        if text.is_empty() {
            return;
        }
        let selection = self.composer_model_selection(draft);
        let runtime = self.composer_runtime_mode(draft);
        let now = now_iso();
        let title = {
            let chars: Vec<char> = text.chars().collect();
            if chars.len() <= 50 {
                text.clone()
            } else {
                format!("{}...", chars[..50].iter().collect::<String>())
            }
        };
        let mut model = json!({"instanceId": selection.instance_id, "model": selection.model});
        if let Some(options) = &selection.options {
            model["options"] = options.clone();
        }
        let (thread_id, interaction, bootstrap) = if draft {
            let Some(project) = self.draft_project() else {
                return;
            };
            let thread_id = uuid::Uuid::new_v4().to_string();
            let bootstrap = json!({"createThread": {
                "projectId": project.id,
                "title": title,
                "modelSelection": model,
                "runtimeMode": runtime,
                "interactionMode": "default",
                "branch": null,
                "worktreePath": null,
                "createdAt": now,
            }});
            (thread_id, "default".to_owned(), Some(bootstrap))
        } else {
            let Some(thread) = self.current_thread() else {
                return;
            };
            (thread.id, thread.interaction_mode, None)
        };
        let mut command = json!({
            "type": "thread.turn.start",
            "commandId": uuid::Uuid::new_v4().to_string(),
            "threadId": thread_id,
            "message": {
                "messageId": uuid::Uuid::new_v4().to_string(),
                "role": "user",
                "text": text,
                "attachments": [],
            },
            "modelSelection": model,
            "titleSeed": title,
            "runtimeMode": runtime,
            "interactionMode": if interaction.is_empty() { "default".to_owned() } else { interaction },
            "createdAt": now,
        });
        if let Some(bootstrap) = bootstrap {
            command["bootstrap"] = bootstrap;
        }
        self.dispatch(command);
        self.composer.clear();
        if draft {
            self.select_thread(Some(thread_id));
        }
    }
}
