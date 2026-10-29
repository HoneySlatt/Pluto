//! Composer (`ChatComposer.tsx`, `ComposerSurface.tsx`, `ComposerPrimaryActions.tsx`,
//! `BranchToolbar.tsx`): prompt editor, model/traits/runtime pickers, send and
//! stop buttons, attached request banners and the checkout strip.
use eframe::egui::{
    self, Color32, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::{Value, json};

pub(crate) use pluto_core::commands::{launch_input, message_dispatch};

use crate::app::App;
use crate::model::ModelSelection;
use crate::requests::{PendingApproval, PendingUserInput};
use crate::theme::{self, Weight, font};
use crate::ui::requests_panel::OVERLAP;
use crate::ui::{cite, widgets};

const MIN_TEXT_HEIGHT: f32 = 70.0;
/// An icon-only picker: padding, 16px icon, 4px gap, chevron, padding.
const ICON_PICKER: f32 = 11.0 + 16.0 + 4.0 + 14.0 + 9.0;
/// The icon-only Build/Plan toggle.
const ICON_TOGGLE: f32 = 10.0 + 16.0 + 10.0;
/// The "…" overflow trigger: an icon between paddings.
const OVERFLOW_TRIGGER: f32 = 11.0 + 16.0 + 9.0;
/// The model picker's narrowest form (icon and a few letters), with its gap.
const MODEL_MIN: f32 = 11.0 + 22.0 + 32.0 + 4.0 + 14.0 + 9.0 + 6.0;
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
    Plan(Box<crate::timeline::ProposedPlan>),
    Approval(PendingApproval, usize),
    Question(PendingUserInput),
}

/// The title of a message's thread: its text, or its first image's name
/// (`truncate(titleSeed)`, 50 characters).
fn title_seed(
    text: &str,
    images: &[crate::ui::attachments::ComposerImage],
    files: &[crate::ui::attachments::ComposerFile],
) -> String {
    let seed = match (images.first(), files.first()) {
        (Some(image), _) if text.is_empty() => format!("Image: {}", image.name),
        // Pluto: a message of files alone.
        (None, Some(file)) if text.is_empty() => format!("File: {}", file.name),
        _ => text.to_owned(),
    };
    let chars: Vec<char> = seed.chars().collect();
    if chars.len() <= 50 {
        seed
    } else {
        format!("{}...", chars[..50].iter().collect::<String>())
    }
}

/// Width of a labelled picker (`picker`).
fn picker_width(icon: bool, label_width: f32) -> f32 {
    11.0 + if icon { 22.0 } else { 0.0 } + label_width + 4.0 + 14.0 + 9.0
}

/// Width of the labelled Build/Plan toggle (`interaction_toggle`).
pub(crate) fn toggle_width(label_width: f32) -> f32 {
    (10.0 + 16.0 + 6.0 + label_width + 10.0).ceil()
}

/// How the footer controls fit their room.
#[derive(Debug, PartialEq)]
pub(crate) struct Fit {
    /// Trailing blocks showing only their icons.
    pub icon_only: usize,
    /// The last (mode) block moved behind "…".
    pub mode_hidden: bool,
    /// The model picker's width, its gap included.
    pub model_width: f32,
    pub model_shrunk: bool,
}

/// `resolveRestingComposerControlsLayout`: the blocks after the model picker
/// (`(natural, icon-only)` widths) drop their labels from the end, then the
/// mode block moves into the overflow menu; past that the model label
/// truncates, down to `MODEL_MIN`. Not ported: the traits block also moving
/// into the overflow menu.
pub(crate) fn fit_controls(model: f32, blocks: &[(f32, f32)], overflow: f32, room: f32) -> Fit {
    let n = blocks.len();
    let width = |icon_only: usize, hidden: bool| {
        let shown = if hidden { n.saturating_sub(1) } else { n };
        let blocks: f32 = blocks[..shown]
            .iter()
            .enumerate()
            .map(|(i, (natural, icon))| if i + icon_only >= n { *icon } else { *natural })
            .sum();
        blocks + if hidden { overflow } else { 0.0 }
    };
    let (icon_only, mode_hidden) = (0..=n)
        .map(|k| (k, false))
        .chain((n > 0).then_some((n, true)))
        .find(|&(k, hidden)| model + width(k, hidden) <= room)
        .unwrap_or((n, n > 0));
    let model_width = model
        .min(room - width(icon_only, mode_hidden))
        .max(MODEL_MIN.min(model));
    Fit {
        icon_only,
        mode_hidden,
        model_width,
        model_shrunk: model_width < model,
    }
}

/// A message on its way (`startThreadTurn`): its images are persisted
/// first (`assets.persistChatAttachments`), then the commands it waits on
/// go one after another, each after the last one's receipt
/// (`persistThreadSettingsForNextTurn`), then the message itself.
pub(crate) struct TurnSend {
    pub(crate) thread_id: String,
    pub(crate) before: Vec<Value>,
    /// A `message.dispatch`, or an `orchestration.launchThread` input.
    pub(crate) message: Value,
    pub(crate) launch: bool,
    /// `UploadChatImageAttachment`s still to persist.
    pub(crate) uploads: Vec<Value>,
}

impl TurnSend {
    /// Where the server's answer lands: the launch request, or the
    /// message's command.
    pub(crate) fn send_key(&self) -> String {
        if self.launch {
            format!("launch:{}", self.thread_id)
        } else {
            format!(
                "command:{}",
                self.message["commandId"].as_str().unwrap_or_default()
            )
        }
    }

    fn message_id(&self) -> String {
        let message = if self.launch {
            &self.message["initialMessage"]
        } else {
            &self.message
        };
        message["messageId"].as_str().unwrap_or_default().to_owned()
    }

    /// Adds attachments to the message (its files, then its persisted
    /// images).
    pub(crate) fn set_attachments(&mut self, attachments: Value) {
        let slot = if self.launch {
            &mut self.message["initialMessage"]["attachments"]
        } else {
            &mut self.message["attachments"]
        };
        let mut all = slot.as_array().cloned().unwrap_or_default();
        all.extend(attachments.as_array().cloned().unwrap_or_default());
        *slot = Value::Array(all);
    }
}

impl App {
    pub(crate) fn composer_mode(&mut self, draft: bool) -> Mode {
        if draft {
            return Mode::Normal;
        }
        let Some(detail) = self
            .state
            .detail
            .clone()
            .filter(|d| Some(d.id()) == self.selected.as_deref())
        else {
            return Mode::Normal;
        };
        let Some(thread) = self.current_thread() else {
            return Mode::Normal;
        };
        // The task drawer closes on another thread.
        if self.tasks_thread.as_deref() != Some(thread.id.as_str()) {
            self.tasks_thread = Some(thread.id.clone());
            self.tasks_open = false;
        }
        let mut pending = crate::requests::derive(&detail);
        // A request arriving mid-sentence waits for the typing to pause.
        let ids: Vec<String> = pending
            .approvals
            .iter()
            .map(|a| a.request_id.clone())
            .chain(pending.user_inputs.iter().map(|i| i.request_id.clone()))
            .collect();
        let now = std::time::Instant::now();
        let held = self.typing_guard.requests(&thread.id, &ids, now).clone();
        if let Some(wait) = self.typing_guard.release_in(now)
            && let Some(ctx) = &self.egui_ctx
        {
            ctx.request_repaint_after(wait);
        }
        pending.approvals.retain(|a| !held.contains(&a.request_id));
        pending
            .user_inputs
            .retain(|i| !held.contains(&i.request_id));
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
        match self.plan_follow_up(&thread, &detail) {
            Some(plan) => Mode::Plan(Box::new(plan)),
            None => Mode::Normal,
        }
    }

    fn composer_text_height(&self, ui: &Ui, width: f32) -> f32 {
        let collapsed = cite::collapse(&self.composer);
        let chips: Vec<cite::Chip> = collapsed.tokens.iter().map(|(c, _)| c.clone()).collect();
        let galley = cite::composer_galley(ui, &collapsed.display, &chips, width - 34.0);
        galley.size().y.clamp(MIN_TEXT_HEIGHT, MAX_TEXT_HEIGHT)
    }

    /// The attached images' row above the prompt (`mb-3 flex flex-wrap
    /// gap-2` of 64px thumbnails), wrapping to the editor width.
    pub(crate) fn image_strip_height(&self, width: f32) -> f32 {
        let images = self.images_block_height(width - 34.0);
        // Pluto: file chips follow the images.
        let (_, files) = crate::ui::attachments::file_grid(self.composer_files.len(), width - 34.0);
        if images == 0.0 && files.y == 0.0 {
            return 0.0;
        }
        let gap = if images > 0.0 && files.y > 0.0 {
            8.0
        } else {
            0.0
        };
        images + gap + files.y + 12.0
    }

    /// The image thumbnails' rows within `width`.
    fn images_block_height(&self, width: f32) -> f32 {
        if self.composer_images.is_empty() {
            return 0.0;
        }
        let per_row = (((width + 8.0) / 72.0).floor() as usize).max(1);
        let rows = self.composer_images.len().div_ceil(per_row) as f32;
        rows * 64.0 + (rows - 1.0) * 8.0
    }

    fn surface_height(&self, ui: &Ui, width: f32, mode: &Mode) -> f32 {
        let strip = self.image_strip_height(width);
        // The prompt-length alert takes a line under the editor.
        let alert = if self.prompt_length_error.is_some() {
            20.0
        } else {
            0.0
        };
        match mode {
            Mode::Normal | Mode::Plan(_) => {
                17.0 + strip + self.composer_text_height(ui, width) + alert + 10.0 + 28.0 + 19.0
            }
            Mode::Question(_) => 17.0 + self.composer_text_height(ui, width) + 16.0 + 28.0 + 17.0,
            Mode::Approval(..) => 17.0 + 32.0 + 17.0,
        }
    }

    /// The attached banners' height: the notices above the queue's, above
    /// the mode's banner.
    fn banner_height(&self, ui: &Ui, width: f32, mode: &Mode) -> f32 {
        let lower = self.mode_banner_height(ui, width, mode);
        let queue = if self.selected.is_some() {
            self.queue_banner_height(self.queue_workflow().as_ref())
        } else {
            0.0
        };
        let below = if queue > 0.0 && lower > 0.0 {
            lower + queue - OVERLAP
        } else {
            lower + queue
        };
        let notices = Self::notices_height(&self.composer_notices(self.selected.is_none()));
        if below > 0.0 && notices > 0.0 {
            below + notices
        } else if notices > 0.0 {
            notices + OVERLAP
        } else {
            below
        }
    }

    fn mode_banner_height(&self, ui: &Ui, width: f32, mode: &Mode) -> f32 {
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

        // Attached banners sit behind the surface's top edge, the queue's
        // behind the mode's banner.
        self.check_queued_edit(ui.ctx());
        if !draft {
            let lower = self.mode_banner_height(ui, width, mode);
            let bottom = if lower > 0.0 {
                surface.top() + OVERLAP - lower
            } else {
                surface.top()
            };
            self.queue_banner(ui, surface, bottom);
            let queued = self.queue_workflow().is_some_and(|w| !w.queued.is_empty());
            crate::ui::requests_panel::join_banners(queued && lower > 0.0);
        }
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
        crate::ui::requests_panel::join_banners(false);
        // The notices over everything attached below them.
        let notices = self.composer_notices(draft);
        if !notices.is_empty() {
            let below = if draft {
                0.0
            } else {
                let lower = self.mode_banner_height(ui, width, mode);
                let queue = self.queue_banner_height(self.queue_workflow().as_ref());
                if queue > 0.0 && lower > 0.0 {
                    lower + queue - OVERLAP
                } else {
                    lower + queue
                }
            };
            let bottom = if below > 0.0 {
                surface.top() + OVERLAP - below
            } else {
                surface.top()
            };
            self.paint_notices(ui, surface, bottom, &notices);
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
        // `ChatComposer`'s placeholder, by state.
        let choice_only = matches!(mode, Mode::Question(input)
            if input
                .questions
                .get(self.question_index)
                .is_some_and(|q| !q.allow_custom_answer));
        // The selected provider is gone from the catalog.
        let provider_unavailable =
            !self.providers().is_empty() && self.composer_provider(draft).is_none();
        // `phase === "disconnected"`: a thread that never ran (or a draft).
        let never_ran = draft || self.current_thread().is_none_or(|t| t.runtime.is_none());
        let placeholder = match mode {
            Mode::Approval(..) => "Resolve this approval request to continue",
            Mode::Question(_) if choice_only => "Choose an option above",
            Mode::Question(_) => {
                "Type your own answer, or leave this blank to use the selected option"
            }
            Mode::Plan(_) => "Add feedback to refine the plan, or leave this blank to implement it",
            Mode::Normal if draft && self.draft_project().is_none() => {
                "Choose a project above to start a thread"
            }
            Mode::Normal if provider_unavailable => {
                "Enable a provider in Settings to send a message"
            }
            // Pluto: a chat asks like the chat sites do.
            Mode::Normal if self.image_on(draft) => "Describe the image to make",
            Mode::Normal if self.research_on(draft) => "What should I research?",
            Mode::Normal if self.chat_context(draft) => "Ask anything",
            Mode::Normal if never_ran => "Ask for changes, send follow-ups, or attach images",
            Mode::Normal => "Ask anything, @tag files/folders, $use skills, or / for commands",
        };
        // Attached images count as context, which hides the placeholder.
        if (self.composer.is_empty()
            && self.composer_images.is_empty()
            && self.composer_files.is_empty())
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
        if normal {
            self.typing_guard.focus(focused);
        }
        // The suggestion menu takes its keys first.
        if normal {
            self.refresh_skill_metas(draft);
            self.command_menu_keys(ui, editor_id, draft);
        }
        // Ctrl+Enter in a citation's comment editor sends too.
        // Ctrl or Cmd+Enter sends the other way during a turn.
        // `composerRequiresModifier`: the Send shortcut setting decides
        // whether a plain Enter sends or starts a new line.
        let shortcut = self.client_setting("sendShortcut");
        let modifier = shortcut == "mod-enter"
            || (shortcut == "mod-enter-multiline" && self.composer.contains('\n'));
        let submit = (focused
            && ui.input_mut(|i| {
                (!modifier && i.consume_key(Modifiers::NONE, Key::Enter))
                    || i.consume_key(Modifiers::COMMAND, Key::Enter)
            }))
            || std::mem::take(&mut self.cite.send);
        if focused && normal && self.composer_images.is_empty() && self.composer_files.is_empty() {
            self.recall_prompt(ui, editor_id, text_rect.width());
        }
        // Citation links are edited as chips, one character each.
        cite::pad_chips(&mut self.composer);
        let collapsed = cite::collapse(&self.composer);
        cite::set_copy_tokens(&collapsed);
        let chips: Vec<cite::Chip> = collapsed.tokens.iter().map(|(c, _)| c.clone()).collect();
        let mut display = collapsed.display.clone();
        // A large paste goes as a file (`pastedTextDisposition`), unless
        // Ctrl+Shift+V asked to keep it inline.
        if normal && focused {
            let pastes: Vec<String> = ui.input(|i| {
                let plain = i.modifiers.shift;
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Paste(text) if !plain => Some(text.clone()),
                        _ => None,
                    })
                    .collect()
            });
            for text in pastes {
                let total = crate::ui::citations::utf16_len(self.composer.trim())
                    + crate::ui::citations::utf16_len(&text);
                let large = text.len() >= crate::ui::attachments::PASTED_TEXT_FILE_BYTES
                    || total > MAX_PROMPT_CHARS;
                if !large {
                    continue;
                }
                ui.input_mut(|i| {
                    i.events
                        .retain(|e| !matches!(e, egui::Event::Paste(t) if *t == text))
                });
                let bytes = text.len() as u64;
                match self.attach_pasted_text(text) {
                    Ok(name) => self.toasts.add(
                        crate::ui::toasts::Toast::new(
                            crate::ui::toasts::Kind::Info,
                            format!("Large paste attached as {name}"),
                        )
                        .description(format!(
                            "{} · Use Ctrl+Shift+V to keep a large paste inline.",
                            crate::ui::attachments::format_size(bytes)
                        )),
                    ),
                    Err(error) => self.toasts.add(
                        crate::ui::toasts::Toast::new(
                            crate::ui::toasts::Kind::Error,
                            "Pasted text is too large to attach",
                        )
                        .description(error),
                    ),
                };
            }
        }
        let mut editor_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
        egui::ScrollArea::vertical()
            .id_salt("composer-scroll")
            .max_height(text_height)
            .show(&mut editor_ui, |ui| {
                let output = egui::TextEdit::multiline(&mut display)
                    .id(editor_id)
                    .interactive(!choice_only)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(theme::composer_font())
                    .text_color(theme::foreground())
                    .desired_width(text_rect.width())
                    .min_size(Vec2::new(text_rect.width(), text_height))
                    .layouter(&mut |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                        cite::composer_galley(ui, text.as_str(), &chips, wrap)
                    })
                    .show(ui);
                if display != collapsed.display {
                    self.typing_guard.typed(std::time::Instant::now());
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

        // `ComposerPromptLengthValidation`: kept while the prompt is too
        // long, gone once it fits.
        if self.prompt_length_error.is_some() {
            self.prompt_length_error = prompt_length_message(&self.composer);
        }
        let alert_height = if let Some(error) = &self.prompt_length_error {
            widgets::text_in(
                ui,
                Rect::from_min_size(
                    Pos2::new(surface.left() + 17.0, text_rect.bottom() + 4.0),
                    Vec2::new(text_rect.width(), 16.0),
                ),
                error,
                font(12.0, Weight::Regular),
                theme::error(),
            );
            20.0
        } else {
            0.0
        };
        // Toolbar. The actions go first: the controls fit the room they leave.
        let bar_top = text_rect.bottom()
            + alert_height
            + if matches!(mode, Mode::Question(_)) {
                16.0
            } else {
                10.0
            };
        let right = surface.right() - 17.0;
        let actions_left = match mode {
            Mode::Question(input) => {
                self.question_actions(ui, right, bar_top, input, &thread_id, submit)
            }
            Mode::Plan(plan) => self.plan_actions(ui, right, bar_top, plan, submit),
            _ => self.send_actions(ui, right, bar_top, draft, submit),
        };
        // The context window meter sits between the attach button and the actions.
        let meter = Rect::from_min_size(
            Pos2::new(actions_left - 8.0 - 28.0, bar_top),
            Vec2::splat(28.0),
        );
        let actions_left = if matches!(mode, Mode::Normal) && self.context_meter(ui, meter, draft) {
            meter.left() + 4.0
        } else {
            actions_left
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

        let mut x = surface.left() + 7.0;
        let selection = self.composer_model_selection(draft);
        let (model_label, provider_icon) = self.model_label(&selection);
        let trait_label = self.traits_label(&selection);
        let runtime = self.composer_runtime_mode(draft);
        let (runtime_text, runtime_icon) = runtime_label(&runtime);
        let plan_toggle = self.plan_mode_enabled(draft);
        let label_font = font(14.0, Weight::Medium);
        let labelled = |icon: bool, label: &str| {
            picker_width(icon, widgets::text_width(ui, label, label_font.clone()))
        };
        let mut blocks = Vec::new();
        if let Some(label) = &trait_label {
            blocks.push((7.0 + labelled(false, label) + 6.0, 7.0 + ICON_PICKER + 6.0));
        }
        let interaction = if self.composer_interaction_mode(draft) == "plan" {
            "Plan"
        } else {
            "Build"
        };
        let toggle = |icon_only: bool| {
            if !plan_toggle {
                0.0
            } else if icon_only {
                6.0 + 7.0 + ICON_TOGGLE
            } else {
                6.0 + 7.0 + toggle_width(widgets::text_width(ui, interaction, label_font.clone()))
            }
        };
        // A chat has no runtime or interaction mode to pick; it has the
        // Research toggle instead (Pluto, `chat_research`).
        let chat = self.chat_context(draft);
        let research = chat && !self.temporary_context();
        if !chat {
            blocks.push((
                7.0 + labelled(true, runtime_text) + toggle(false),
                7.0 + ICON_PICKER + toggle(true),
            ));
        } else {
            if research {
                let label = widgets::text_width(ui, "Research", label_font.clone());
                blocks.push((7.0 + toggle_width(label), 7.0 + ICON_TOGGLE));
            }
            let label = widgets::text_width(ui, "Image", label_font.clone());
            blocks.push((7.0 + toggle_width(label), 7.0 + ICON_TOGGLE));
        }
        let fit = fit_controls(
            labelled(provider_icon.is_some(), &model_label) + 6.0,
            &blocks,
            7.0 + 28.0,
            attach.left() - 8.0 - x,
        );
        let model_rect = self.picker(
            ui,
            x,
            bar_top,
            provider_icon,
            &model_label,
            "model-picker",
            fit.model_shrunk.then_some(fit.model_width - 6.0),
        );
        x = model_rect.right() + 6.0;
        let icon_only = |index: usize| index >= blocks.len() - fit.icon_only;
        let mut traits_rect = None;
        if let Some(trait_label) = &trait_label {
            self.separator(ui, x, bar_top);
            x += 7.0;
            let rect = if icon_only(0) {
                self.icon_picker(ui, x, bar_top, "brain", trait_label, "traits-picker")
            } else {
                self.picker(ui, x, bar_top, None, trait_label, "traits-picker", None)
            };
            x = rect.right() + 6.0;
            traits_rect = Some(rect);
        }
        let mode_index = blocks.len().saturating_sub(1);
        // What `composer.mode` and `composer.effort` open (`openControl`).
        self.composer_controls = (
            (!chat).then_some(if fit.mode_hidden {
                "composer-overflow"
            } else {
                "runtime-picker"
            }),
            trait_label.as_ref().map(|_| "traits-picker"),
        );
        if !chat && fit.mode_hidden {
            self.separator(ui, x, bar_top);
            x += 7.0;
            // `CompactComposerControlsMenu`: Mode and Access behind "…".
            let rect = self.icon_picker(
                ui,
                x,
                bar_top,
                "ellipsis",
                "More composer controls",
                "composer-overflow",
            );
            self.composer_overflow_menu(ui, rect, draft, plan_toggle, &runtime);
        } else if !chat {
            self.separator(ui, x, bar_top);
            x += 7.0;
            let runtime_rect = if icon_only(mode_index) {
                self.icon_picker(ui, x, bar_top, runtime_icon, runtime_text, "runtime-picker")
            } else {
                self.picker(
                    ui,
                    x,
                    bar_top,
                    Some(runtime_icon),
                    runtime_text,
                    "runtime-picker",
                    None,
                )
            };
            if plan_toggle {
                let x = runtime_rect.right() + 6.0;
                self.separator(ui, x, bar_top);
                self.interaction_toggle(ui, x + 7.0, bar_top, draft, icon_only(mode_index));
            }
            self.runtime_menu(ui, runtime_rect, draft);
        }
        if chat {
            let mut x = x;
            // The Image toggle is the last block; Research comes before it.
            let mut index = mode_index - usize::from(research);
            if research {
                self.separator(ui, x, bar_top);
                x = self.research_toggle(ui, x + 7.0, bar_top, icon_only(index) || fit.mode_hidden)
                    + 6.0;
                index += 1;
            }
            self.separator(ui, x, bar_top);
            self.image_toggle(ui, x + 7.0, bar_top, icon_only(index) || fit.mode_hidden);
        }
        self.model_picker(ui, model_rect, draft);
        if let Some(rect) = traits_rect {
            self.traits_menu(ui, rect, draft);
        }
    }

    /// The attached images: 64px thumbnails (`h-16 w-16 rounded-lg border
    /// border-border/80 bg-background`) with their remove button.
    pub(crate) fn composer_image_strip(&mut self, ui: &mut Ui, origin: Pos2, width: f32) {
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
        // Pluto: file chips under the images.
        let files_top = origin.y
            + self.images_block_height(width)
            + if self.composer_images.is_empty() {
                0.0
            } else {
                8.0
            };
        self.composer_file_chips(ui, Pos2::new(origin.x, files_top), width);
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

    /// Pluto: the attached files as chips, each with its upload state and
    /// a remove button.
    fn composer_file_chips(&mut self, ui: &mut Ui, origin: Pos2, width: f32) {
        use crate::ui::attachments::{
            FILE_CHIP, file_grid, file_kind, format_size, paint_file_chip,
        };
        let (per_row, _) = file_grid(self.composer_files.len(), width);
        let mut remove = None;
        for (index, file) in self.composer_files.iter().enumerate() {
            let chip = Rect::from_min_size(
                origin
                    + Vec2::new(
                        (index % per_row) as f32 * (FILE_CHIP.x + 8.0),
                        (index / per_row) as f32 * (FILE_CHIP.y + 8.0),
                    ),
                FILE_CHIP,
            );
            let (detail, color) = match &file.upload {
                None => ("Uploading…".to_owned(), theme::muted_foreground()),
                Some(Err(_)) => ("Upload failed".to_owned(), theme::error_foreground()),
                Some(Ok(_)) => (
                    format!("{} · {}", file_kind(&file.name), format_size(file.size)),
                    theme::muted_foreground(),
                ),
            };
            paint_file_chip(ui, chip, &file.name, &detail, color);
            if let Some(Err(error)) = &file.upload {
                ui.interact(
                    chip,
                    ui.id().with(("composer-file", &file.id)),
                    Sense::hover(),
                )
                .on_hover_text(error);
            }
            let close = Rect::from_min_size(
                Pos2::new(chip.right() - 4.0 - 22.0, chip.top() + 4.0),
                Vec2::splat(22.0),
            );
            if widgets::icon_button(
                ui,
                close,
                ("composer-file-remove", &file.id),
                "x",
                14.0,
                theme::muted_foreground(),
                &format!("Remove {}", file.name),
            )
            .clicked()
            {
                remove = Some(index);
            }
        }
        if let Some(index) = remove {
            let file = self.composer_files.remove(index);
            if let Some(Ok(attachment)) = file.upload {
                self.delete_upload(&attachment);
            }
        }
    }

    pub(crate) fn running(&self) -> bool {
        self.current_thread().is_some_and(|t| t.is_running())
    }

    /// The primary action (`ComposerPrimaryActions.tsx`): Stop while a turn
    /// runs and the composer is empty, Resume for a stopped thread, else
    /// the send button (queue or steer during a turn, update while a
    /// queued message is edited). Returns the left edge.
    fn send_actions(
        &mut self,
        ui: &mut Ui,
        right: f32,
        bar_top: f32,
        draft: bool,
        submit: bool,
    ) -> f32 {
        let send = Rect::from_min_size(Pos2::new(right - 32.0, bar_top - 2.0), Vec2::splat(32.0));
        let editing = !draft && self.editing_queued();
        let has_content = if editing {
            self.queued_edit_sendable()
        } else {
            !self.composer.trim().is_empty()
                || !self.composer_images.is_empty()
                || !self.composer_files.is_empty()
        };
        let running = !draft && self.running();
        if running && !has_content && !editing {
            let response = ui.interact(send, ui.id().with("stop"), Sense::click());
            let fill = if response.hovered() {
                DESTRUCTIVE
            } else {
                theme::alpha(DESTRUCTIVE, 0.9)
            };
            ui.painter().circle_filled(send.center(), 16.0, fill);
            theme::paint_icon(ui, "stop-square", send, 12.0, Color32::WHITE);
            if response.on_hover_text("Interrupt").clicked() {
                self.interrupt();
            }
            return send.left();
        }
        let resume = !draft && !has_content && !editing && self.can_resume();
        let can_send = (has_content || resume) && self.connected();
        let response = ui.interact(
            send,
            ui.id().with("send"),
            if can_send {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let opacity = if can_send { 1.0 } else { 0.64 };
        // A circle; Pluto stock fills it with its gradient, and the arrow takes
        // the accent's text color (white on the stock palette, dark on a
        // white accent).
        let arrow = crate::ui::backdrop::fill_action(
            ui,
            Rect::from_center_size(send.center(), Vec2::splat(32.0)),
            CornerRadius::same(16),
            response.hovered() && can_send,
            opacity,
        );
        // `resolveComposerDispatchMode`: Ctrl flips the follow-up behavior.
        let follow_up = if self.client_setting("followUpBehavior").as_str() == Some("steer") {
            "steer"
        } else {
            "queue"
        };
        let alternate = if follow_up == "queue" {
            "steer"
        } else {
            "queue"
        };
        let queuing =
            running && !editing && (follow_up == "queue") != ui.input(|i| i.modifiers.command);
        let (icon, label) = if resume {
            ("play-fill", "Resume thread")
        } else if editing {
            ("check", "Update queued message")
        } else if queuing {
            ("list-plus", "Queue message")
        } else if running {
            ("corner-up-right", "Steer message")
        } else {
            ("send-arrow", "Submit message")
        };
        let size = if icon == "send-arrow" { 14.0 } else { 16.0 };
        theme::paint_icon(ui, icon, send, size, arrow);
        let tooltip = if running && !editing {
            let shortcut = crate::ui::shortcuts::effective_shortcut_label(
                &self.state.config["keybindings"],
                "composer.sendAlternate",
                &[("composerFocus", true), ("turnRunning", true)],
            )
            .map(|label| format!(" or {label}"))
            .unwrap_or_default();
            format!("Click to {follow_up}, Ctrl/⌘-click{shortcut} to {alternate}")
        } else {
            label.to_owned()
        };
        let response = response.on_hover_text(tooltip);
        if resume {
            if response.clicked() {
                self.resume_thread();
            }
        } else if (response.clicked() || submit) && can_send {
            if editing {
                self.save_queued_edit(ui.ctx());
            } else {
                let alternate = ui.input(|i| i.modifiers.command);
                self.submit_message(draft, alternate);
            }
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
        let responding = self.responding.contains(&input.request_id) || !input.resumable;
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

    /// `interruptThreadTurn`: stops the active run and holds the queue
    /// behind it. Settled runs' background work and pull request watches
    /// are not stopped from here yet.
    fn interrupt(&mut self) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        let run = self
            .state
            .detail
            .as_ref()
            .filter(|d| d.id() == thread.id)
            .and_then(|d| crate::ui::queue::active_run(d).map(|r| r.id.clone()))
            .or_else(|| {
                thread
                    .runtime
                    .as_ref()
                    .and_then(|r| r.active_run_id.clone())
            });
        let Some(run) = run else {
            return;
        };
        let mut command = crate::model::command("run.interrupt", &thread.id);
        command["runId"] = json!(run);
        command["holdQueue"] = json!(true);
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
    /// With `max_width` the label truncates to fit it.
    #[allow(clippy::too_many_arguments)]
    fn picker(
        &mut self,
        ui: &mut Ui,
        x: f32,
        top: f32,
        icon: Option<&str>,
        label: &str,
        id: &str,
        max_width: Option<f32>,
    ) -> Rect {
        let label_font = font(14.0, Weight::Medium);
        let natural_label = widgets::text_width(ui, label, label_font.clone());
        let natural = picker_width(icon.is_some(), natural_label);
        let width = max_width.map_or(natural, |max| max.min(natural));
        let label_width = (natural_label - (natural - width)).max(0.0);
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
            Rect::from_min_size(Pos2::new(cx, top), Vec2::new(label_width, 28.0)),
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

    /// A picker showing its icon only (`data-composer-block-icon-only`),
    /// its label as the tooltip; the "…" trigger has no chevron.
    fn icon_picker(
        &mut self,
        ui: &mut Ui,
        x: f32,
        top: f32,
        icon: &str,
        label: &str,
        id: &str,
    ) -> Rect {
        let chevron = icon != "ellipsis";
        let width = if chevron {
            ICON_PICKER
        } else {
            OVERFLOW_TRIGGER
        };
        let rect = Rect::from_min_size(Pos2::new(x, top), Vec2::new(width, 28.0));
        let response = ui
            .interact(rect, ui.id().with(id), Sense::click())
            .on_hover_text(label);
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
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(Pos2::new(x + 11.0, top + 6.0), Vec2::splat(16.0)),
            16.0,
            color,
        );
        if chevron {
            theme::paint_icon(
                ui,
                "chevron-down",
                Rect::from_min_size(Pos2::new(x + 31.0, top + 7.0), Vec2::splat(14.0)),
                14.0,
                color,
            );
        }
        if response.clicked() {
            self.open_menu = if open { None } else { Some(id.to_owned()) };
        }
        rect
    }

    /// `CompactComposerControlsMenu`: the Mode (with plan mode) and Access
    /// choices of the controls that no longer fit.
    fn composer_overflow_menu(
        &mut self,
        ui: &mut Ui,
        anchor: Rect,
        draft: bool,
        plan_toggle: bool,
        runtime: &str,
    ) {
        use crate::ui::pickers::RUNTIME_MODES;
        use crate::ui::widgets::{MenuEntry, MenuItem};
        let plan = self.composer_interaction_mode(draft) == "plan";
        let mut entries = Vec::new();
        if plan_toggle {
            entries.push(MenuEntry::Header("Mode"));
            for (label, selected) in [("Chat", !plan), ("Plan", plan)] {
                entries.push(MenuEntry::Item(MenuItem {
                    checked: Some(selected),
                    ..MenuItem::new(label)
                }));
            }
            entries.push(MenuEntry::Separator);
        }
        let access = entries.len() + 1;
        entries.push(MenuEntry::Header("Access"));
        for (mode, label, _, _) in RUNTIME_MODES {
            entries.push(MenuEntry::Item(MenuItem {
                checked: Some(mode == runtime),
                ..MenuItem::new(label)
            }));
        }
        let Some(index) = widgets::menu(
            ui,
            &mut self.open_menu,
            "composer-overflow",
            anchor,
            true,
            &entries,
        ) else {
            return;
        };
        let key = self.composer_key(draft);
        if index >= access {
            self.composer_runtime
                .insert(key, RUNTIME_MODES[index - access].0.to_owned());
        } else {
            let next = if index == 1 { "default" } else { "plan" };
            self.composer_interaction.insert(key, next.into());
        }
    }

    /// `resolveNewThreadEnvMode`: the draft's pick, else the project's
    /// override, the project's own default, then the environment's.
    pub(crate) fn draft_env_mode(&self) -> String {
        let Some(project) = self.draft_project() else {
            return "local".into();
        };
        // A draft in an existing worktree works there (`resolveEffectiveEnvMode`).
        if self.draft_workspace().is_some_and(|w| w.worktree.is_some()) {
            return "local".into();
        }
        if let Some((id, mode)) = &self.draft_env_mode
            && *id == project.id
        {
            return mode.clone();
        }
        let settings = &self.state.config["settings"];
        settings["projectSettingsOverrides"][&project.id]["defaultThreadEnvMode"]
            .as_str()
            .map(str::to_owned)
            .or(project.default_thread_env_mode.clone())
            .or_else(|| settings["defaultThreadEnvMode"].as_str().map(str::to_owned))
            .filter(|mode| mode == "worktree")
            .unwrap_or_else(|| "local".into())
    }

    /// The checked-out branch the workspace status reports (`refName`).
    pub(crate) fn workspace_branch(&self) -> Option<String> {
        let cwd = self.workspace_cwd()?;
        self.vcs
            .as_ref()
            .filter(|(c, _)| *c == cwd)
            .and_then(|(_, status)| status["refName"].as_str().map(str::to_owned))
    }

    fn checkout_strip(&mut self, ui: &mut Ui, rect: Rect, draft: bool) {
        let muted = if draft {
            theme::muted_foreground()
        } else {
            theme::alpha(theme::muted_foreground(), 0.7)
        };
        // `BranchToolbarEnvModeSelector`: a draft picks where it starts.
        let worktree = draft && self.draft_env_mode() == "worktree";
        let in_worktree = if draft {
            self.draft_workspace().is_some_and(|w| w.worktree.is_some())
        } else {
            self.current_thread()
                .is_some_and(|t| t.worktree_path.is_some())
        };
        // `resolveCurrentWorkspaceLabel`, `resolveLockedWorkspaceLabel`.
        let (icon, label) = match (worktree, draft, in_worktree) {
            (true, _, _) => ("folder-git-2", "New worktree"),
            (_, true, true) => ("folder-git", "Current worktree"),
            (_, true, false) => ("folder", "Current checkout"),
            (_, false, true) => ("folder-git", "Worktree"),
            (_, false, false) => ("folder", "Local checkout"),
        };
        let left = rect.left() + 4.0;
        let label_font = font(12.0, Weight::Regular);
        theme::paint_icon(
            ui,
            icon,
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
            let control = Rect::from_min_max(
                Pos2::new(left + 1.0, rect.top() + 4.0),
                Pos2::new(left + 28.0 + width + 14.0, rect.top() + 28.0),
            );
            let response = ui
                .interact(control, ui.id().with("composer-workspace"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(label);
            if response.clicked() {
                self.open_menu = Some("composer-workspace".into());
            }
            self.workspace_menu(ui, control, worktree, in_worktree);
        }
        self.branch_control(ui, rect.top(), rect.right() - 8.0, draft);
    }

    fn workspace_menu(&mut self, ui: &mut Ui, anchor: Rect, worktree: bool, in_worktree: bool) {
        use crate::ui::widgets::{MenuEntry, MenuItem};
        let current = self.draft_workspace().and_then(|w| w.worktree.clone());
        let previous = self.draft_project().and_then(|project| {
            crate::ui::branch_picker::previous_worktree(
                &self.state.threads,
                &project.id,
                current.as_deref(),
            )
        });
        // `resolvePreviousWorktreeLabel`.
        let previous_label = previous.as_ref().map(|(branch, _)| match branch {
            Some(branch) => format!("Previous worktree ({branch})"),
            None => "Previous worktree".to_owned(),
        });
        let mut entries = vec![
            MenuEntry::Item(MenuItem {
                icon: Some(if in_worktree { "folder-git" } else { "folder" }),
                selected: !worktree,
                ..MenuItem::new(if in_worktree {
                    "Current worktree"
                } else {
                    "Current checkout"
                })
            }),
            MenuEntry::Item(MenuItem {
                icon: Some("folder-git-2"),
                selected: worktree,
                ..MenuItem::new("New worktree")
            }),
        ];
        if let Some(label) = &previous_label {
            entries.push(MenuEntry::Item(MenuItem {
                icon: Some("folder-git"),
                ..MenuItem::new(label)
            }));
        }
        let Some(index) = widgets::menu(
            ui,
            &mut self.open_menu,
            "composer-workspace",
            anchor,
            true,
            &entries,
        ) else {
            return;
        };
        match (index, previous) {
            (2, Some((branch, path))) => self.use_previous_worktree(branch, path),
            (1, _) => self.set_draft_mode("worktree"),
            _ => self.set_draft_mode("local"),
        }
    }

    pub(crate) fn providers(&self) -> &[Value] {
        self.state.config["providers"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// The driver whose mark shows for `instance_id`: Ollama's for its
    /// OpenCode companion.
    pub(crate) fn provider_driver(&self, instance_id: &str) -> Option<String> {
        let provider = self
            .providers()
            .iter()
            .find(|p| p["instanceId"] == instance_id)?;
        if crate::ui::pickers::is_ollama_companion(self.providers(), provider) {
            return Some("ollama".into());
        }
        provider["driver"].as_str().map(str::to_owned)
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
            .or_else(|| {
                crate::ui::pickers::descriptor_default(&descriptor)
                    .as_str()
                    .map(str::to_owned)
            })?;
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
        // Without a default, the last model picked (`applyStickyState`).
        if let Some(selection) = self.sticky_model(draft) {
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

    /// The server reads context records (`capabilities.inlineMessageContext`).
    pub(crate) fn inline_message_context(&self) -> bool {
        self.state.config["environment"]["capabilities"]["inlineMessageContext"] == true
    }

    pub(crate) fn composer_runtime_mode(&self, draft: bool) -> String {
        // Pluto: chats ask before anything that would write or run.
        if self.chat_context(draft) {
            return "approval-required".to_owned();
        }
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
        let chips: Vec<cite::Chip> = collapsed.tokens.iter().map(|(c, _)| c.clone()).collect();
        let galley = cite::composer_galley(ui, &collapsed.display, &chips, wrap);
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
        let detail = self.current_detail().unwrap_or_default();
        let entries = prompt_history::entries(
            detail
                .visible
                .iter()
                .filter(|row| row.item.kind == "user_message")
                .map(|row| {
                    (
                        row.item.str("messageId").unwrap_or_default(),
                        "user",
                        row.item.text(),
                    )
                }),
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

    /// `resolveComposerDispatchMode`: during a turn the follow-up behavior
    /// decides between queueing and steering; Ctrl or Cmd flips it.
    fn submit_message(&mut self, draft: bool, alternate: bool) {
        let mode = if draft || !self.running() {
            "auto"
        } else {
            let default = self.client_setting("followUpBehavior");
            let queue = default.as_str() != Some("steer");
            if queue != alternate { "queue" } else { "steer" }
        };
        self.send_message(draft, mode);
    }

    fn send_message(&mut self, draft: bool, mode: &str) {
        let text = self.composer.trim().to_owned();
        if text.is_empty() && self.composer_images.is_empty() && self.composer_files.is_empty() {
            return;
        }
        self.typing_guard.idle();
        // Too long for a provider turn: refused here, the text stays.
        if let Some(error) = prompt_length_message(&text) {
            self.prompt_length_error = Some(error);
            return;
        }
        // Pluto: files go once uploaded.
        let files = match self.file_attachments() {
            Ok(files) => files,
            Err(reason) => {
                self.notify(crate::ui::toasts::Kind::Warning, reason);
                return;
            }
        };
        let runtime = self.composer_runtime_mode(draft);
        // `assistantCitationsToPlainText`: titles read quotes, not links.
        let title = title_seed(
            crate::ui::citations::to_plain_text(&text).trim(),
            &self.composer_images,
            &self.composer_files,
        );
        // Pluto: after an image, a chat comes back to its own model.
        let mut chosen = self.composer_model_json(draft);
        if !draft
            && let Some(thread) = self.current_thread()
            && let Some(back) = self.model_after_image(&thread.id, draft)
        {
            chosen = back;
        }
        // Pluto: an image request goes to Codex (`chat_image`).
        let image = self.image_on(draft);
        let model = if image {
            match self.image_model() {
                Some(model) => model,
                None => {
                    self.notify(
                        crate::ui::toasts::Kind::Warning,
                        "Image generation needs Codex (a ChatGPT login)",
                    );
                    return;
                }
            }
        } else {
            chosen.clone()
        };
        let interaction = self.composer_interaction_mode(draft);
        let uploads = crate::ui::attachments::upload_payload(&self.composer_images);
        let inline = self.inline_message_context();
        // Pluto: a chat's Research or Image toggle leads the message with its chip.
        let (text, contexts) = if image {
            crate::ui::chat_image::image_message(&text, &self.composer_contexts)
        } else if self.research_on(draft) {
            crate::ui::chat_research::research_message(&text, &self.composer_contexts)
        } else {
            (text, self.composer_contexts.clone())
        };
        let send = if draft {
            let Some(project) = self.draft_project() else {
                return;
            };
            // Pluto: the chat space keeps the last chat model as its default,
            // so Chat mode has its own default model.
            if self.is_chats(&project) && project.default_model_selection.as_ref() != Some(&chosen)
            {
                self.dispatch(json!({
                    "type": "project.update",
                    "commandId": uuid::Uuid::new_v4().to_string(),
                    "projectId": project.id,
                    "defaultModelSelection": chosen,
                }));
            }
            let thread_id = uuid::Uuid::new_v4().to_string();
            let mut input = launch_input(
                &thread_id,
                &project.id,
                &title,
                &text,
                &model,
                &runtime,
                &interaction,
            );
            crate::ui::composer_context::apply(
                &mut input["initialMessage"],
                &text,
                &contexts,
                inline,
            );
            // New worktree mode branches off the picked base; an existing
            // worktree or a switched branch is the thread's workspace.
            let workspace = self.draft_workspace().cloned().unwrap_or_default();
            if self.draft_env_mode() == "worktree" && self.workspace_is_repo() {
                let Some(branch) = self.draft_worktree_base() else {
                    self.notify(
                        crate::ui::toasts::Kind::Warning,
                        "Select a base branch before sending in New worktree mode.",
                    );
                    return;
                };
                input["workspaceStrategy"] = json!({"type": "worktree", "baseRef": branch});
                if self.draft_start_from_origin() {
                    input["workspaceStrategy"]["startFromOrigin"] = json!(true);
                }
            } else if let Some(path) = workspace.worktree {
                input["workspaceStrategy"] =
                    json!({"type": "existing_worktree", "worktreePath": path});
                if let Some(branch) = self.workspace_branch().or(workspace.branch) {
                    input["workspaceStrategy"]["branch"] = json!(branch);
                }
            } else if let Some(branch) = self.workspace_branch().or(workspace.branch) {
                // `resolveBranchToolbarValue`: the checkout's branch first.
                input["workspaceStrategy"]["branch"] = json!(branch);
            }
            TurnSend {
                thread_id,
                before: Vec::new(),
                message: input,
                launch: true,
                uploads,
            }
        } else {
            let Some(thread) = self.current_thread() else {
                return;
            };
            let mut command = message_dispatch(&thread.id, &text, &model, mode);
            command["titleSeed"] = json!(title);
            crate::ui::composer_context::apply(&mut command, &text, &contexts, inline);
            TurnSend {
                thread_id: thread.id.clone(),
                before: self.mode_commands(&thread, &runtime, &interaction),
                message: command,
                launch: false,
                uploads,
            }
        };
        let mut send = send;
        send.set_attachments(json!(files));
        let thread_id = send.thread_id.clone();
        if image && model != chosen {
            self.chat_mode
                .image_return
                .insert(thread_id.clone(), chosen);
        }
        // The draft waits for the server: refused, it comes back.
        let key = send.send_key();
        let composer = self.composer_key(draft);
        let held = self.take_composer();
        self.hold_sent(key, composer, held);
        // Sending answers a wake as well.
        if !draft
            && let Some(thread) = self.current_thread()
            && let Some(woke) = self.woke_at(&thread)
        {
            self.acknowledge_woke(&thread.id, &woke);
        }
        self.start_turn(send);
        self.chat_mode.research = false;
        self.chat_mode.image = false;
        if draft {
            self.select_thread(Some(thread_id));
        }
    }

    /// `persistThreadSettingsForNextTurn`: the thread takes the composer's
    /// runtime and interaction modes before the message goes.
    pub(crate) fn mode_commands(
        &self,
        thread: &crate::model::Thread,
        runtime: &str,
        interaction: &str,
    ) -> Vec<Value> {
        let mut commands = Vec::new();
        if runtime != thread.runtime_mode {
            let mut command = crate::model::command("thread.runtime-mode.set", &thread.id);
            command["runtimeMode"] = json!(runtime);
            commands.push(command);
        }
        let interaction = if interaction.is_empty() {
            "default"
        } else {
            interaction
        };
        if interaction != thread.interaction_mode {
            let mut command = crate::model::command("thread.interaction-mode.set", &thread.id);
            command["interactionMode"] = json!(interaction);
            commands.push(command);
        }
        commands
    }

    /// Sends a message: persists its images, then each command it waits on,
    /// then the message (`startThreadTurn`).
    pub(crate) fn start_turn(&mut self, mut send: TurnSend) {
        if self.blocks_turn_starts() {
            self.notify(
                crate::ui::toasts::Kind::Warning,
                "Turn starts are blocked in this run",
            );
            return;
        }
        if !send.uploads.is_empty() {
            let key = format!("persist:{}", send.message_id());
            let payload = json!({
                "threadId": send.thread_id,
                "messageId": send.message_id(),
                "attachments": std::mem::take(&mut send.uploads),
            });
            self.turn_sends.insert(key.clone(), send);
            self.send_request(key, "assets.persistChatAttachments", payload);
            return;
        }
        if !send.before.is_empty() {
            let command = send.before.remove(0);
            let id = command["commandId"].as_str().unwrap_or_default().to_owned();
            self.turn_sends.insert(format!("after:{id}"), send);
            self.dispatch(command);
            return;
        }
        if send.launch {
            let key = format!("launch:{}", send.thread_id);
            self.send_request(key, "orchestration.launchThread", send.message);
        } else {
            self.dispatch(send.message);
        }
    }

    /// The persisted images of a message on its way, or a launch's outcome.
    pub(crate) fn turn_send_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        if let Some(thread) = key.strip_prefix("launch:") {
            match result {
                Ok(_) => self.sent_done(key),
                Err(error) => {
                    self.refused_send(key, thread, &format!("Could not start the thread: {error}"));
                }
            }
            return true;
        }
        if !key.starts_with("persist:") {
            return false;
        }
        let Some(mut send) = self.turn_sends.remove(key) else {
            return true;
        };
        match result {
            Ok(value) => {
                send.set_attachments(value["attachments"].clone());
                self.start_turn(send);
            }
            Err(error) => {
                let (key, thread) = (send.send_key(), send.thread_id.clone());
                self.refused_send(
                    &key,
                    &thread,
                    &format!("Could not upload the images: {error}"),
                );
            }
        }
        true
    }

    /// A message that did not go: a new thread that will not exist leaves
    /// for the draft, and the text comes back to its composer.
    fn refused_send(&mut self, key: &str, thread: &str, error: &str) {
        if key.starts_with("launch:") && self.selected.as_deref() == Some(thread) {
            self.select_thread(None);
        }
        let text = if self.give_back(key) {
            format!("{error} Your text is back in the composer.")
        } else {
            error.to_owned()
        };
        self.notify(crate::ui::toasts::Kind::Error, text);
    }

    /// A command a message waited on was received: the message goes on, or
    /// stops with the command's failure.
    /// Returns whether a message waiting on it came back to its composer.
    pub(crate) fn turn_command_done(&mut self, command_id: &str, ok: bool) -> bool {
        self.resume_queue_after(command_id, ok);
        let Some(send) = self.turn_sends.remove(&format!("after:{command_id}")) else {
            return false;
        };
        if ok {
            self.start_turn(send);
            return false;
        }
        // The mode change before it failed: the message stays here.
        self.give_back(&send.send_key())
    }
}

/// `PROVIDER_SEND_TURN_MAX_INPUT_CHARS`, in UTF-16 units like upstream.
pub(crate) const MAX_PROMPT_CHARS: usize = 120_000;

/// `composerSubmission`'s alert for a prompt over the limit.
pub(crate) fn prompt_length_message(prompt: &str) -> Option<String> {
    let length = crate::ui::citations::utf16_len(prompt.trim());
    let excess = length.checked_sub(MAX_PROMPT_CHARS).filter(|e| *e > 0)?;
    let group = |n: usize| {
        let digits = n.to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        out
    };
    Some(format!(
        "Prompt is {} {} over the {}-character limit. Shorten or split it before sending.",
        group(excess),
        if excess == 1 {
            "character"
        } else {
            "characters"
        },
        group(MAX_PROMPT_CHARS)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_controls_drop_labels_then_overflow_then_truncate() {
        // Model 150, traits (80 → 60), mode (130 → 61), "…" 43.
        let blocks = [(80.0, 60.0), (130.0, 61.0)];
        let fit = |room| fit_controls(150.0, &blocks, 43.0, room);
        assert_eq!(fit(400.0).icon_only, 0);
        assert_eq!(fit(340.0).icon_only, 1);
        assert_eq!(fit(290.0).icon_only, 2);
        assert!(!fit(290.0).mode_hidden);
        let hidden = fit(260.0);
        assert!(hidden.mode_hidden && !hidden.model_shrunk);
        let shrunk = fit(210.0);
        assert!(shrunk.mode_hidden && shrunk.model_shrunk);
        assert_eq!(shrunk.model_width, 210.0 - 60.0 - 43.0);
        assert_eq!(fit(10.0).model_width, MODEL_MIN);
    }
}
