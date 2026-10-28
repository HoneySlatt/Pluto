//! Proposed plans (`chat/ProposedPlanCard.tsx`, `proposedPlan.ts`): the
//! timeline card with its "Plan" badge, title, the plan's text (folded to
//! 26rem behind a fade past 900 characters or 20 lines, with "Expand plan"
//! / "Collapse plan"), and its menu: Copy to clipboard, Download as
//! markdown (through the desktop portal's save dialog) and Save to
//! workspace (`projects.writeFile`, a path relative to the workspace).
use std::collections::HashSet;

use eframe::egui::{
    self, Color32, CornerRadius, Mesh, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::json;

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::timeline::ProposedPlan;
use crate::ui::context_menu::{self, Item};
use crate::ui::markdown::Markdown;
use crate::ui::toasts::{Kind, Toast};
use crate::ui::widgets;

/// `proposedPlanTitle`: the first Markdown heading's text.
pub(crate) fn proposed_plan_title(plan: &str) -> Option<String> {
    plan.lines().find_map(heading_text)
}

/// A heading line's text (`^\s{0,3}#{1,6}\s+(.+)$`).
fn heading_text(line: &str) -> Option<String> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let hashes = rest.len() - rest.trim_start_matches('#').len();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let after = &rest[hashes..];
    if !after.starts_with([' ', '\t']) {
        return None;
    }
    Some(after.trim().to_owned()).filter(|text| !text.is_empty())
}

/// `stripDisplayedPlanMarkdown`: without its leading heading (shown as the
/// card's title) and a "Summary" heading under it.
pub(crate) fn strip_displayed_plan_markdown(plan: &str) -> String {
    let lines: Vec<&str> = plan.trim_end().lines().collect();
    let mut source: &[&str] = &lines;
    if source.first().is_some_and(|l| heading_text(l).is_some()) {
        source = &source[1..];
    }
    while source.first().is_some_and(|l| l.trim().is_empty()) {
        source = &source[1..];
    }
    if source
        .first()
        .and_then(|l| heading_text(l))
        .is_some_and(|h| h.to_lowercase() == "summary")
    {
        source = &source[1..];
        while source.first().is_some_and(|l| l.trim().is_empty()) {
            source = &source[1..];
        }
    }
    source.join("\n")
}

/// `buildCollapsedProposedPlanPreviewMarkdown`: the first `max_lines`
/// non-blank lines, then "..." when more follows.
pub(crate) fn collapsed_preview(plan: &str, max_lines: usize) -> String {
    let stripped = strip_displayed_plan_markdown(plan);
    let mut preview: Vec<&str> = Vec::new();
    let mut visible = 0;
    let mut more = false;
    for line in stripped.trim_end().lines().map(str::trim_end) {
        let shown = !line.trim().is_empty();
        if shown && visible >= max_lines {
            more = true;
            break;
        }
        preview.push(line);
        if shown {
            visible += 1;
        }
    }
    while preview.last().is_some_and(|l| l.trim().is_empty()) {
        preview.pop();
    }
    if preview.is_empty() {
        return proposed_plan_title(plan).unwrap_or_else(|| "Plan preview unavailable.".into());
    }
    if more {
        preview.extend(["", "..."]);
    }
    preview.join("\n")
}

/// `PLAN_IMPLEMENTATION_PROMPT_PREFIX` and `buildPlanImplementationPrompt`.
pub(crate) fn implementation_prompt(plan: &str) -> String {
    format!("PLEASE IMPLEMENT THIS PLAN:\n{}", plan.trim())
}

/// `buildPlanImplementationThreadTitle`.
pub(crate) fn implementation_thread_title(plan: &str) -> String {
    proposed_plan_title(plan).map_or_else(|| "Implement plan".into(), |t| format!("Implement {t}"))
}

/// `buildProposedPlanMarkdownFilename` (`sanitizePlanFileSegment`).
pub(crate) fn markdown_filename(plan: &str) -> String {
    let title = proposed_plan_title(plan).unwrap_or_else(|| "plan".into());
    let lower: String = title
        .to_lowercase()
        .chars()
        .filter(|c| !"`'\".,!?()[]{}".contains(*c))
        .collect();
    let mut segment = String::new();
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            segment.push(c);
        } else if !segment.ends_with('-') {
            segment.push('-');
        }
    }
    let segment = segment.trim_matches('-');
    format!("{}.md", if segment.is_empty() { "plan" } else { segment })
}

/// `normalizePlanMarkdownForExport`.
fn export_contents(plan: &str) -> String {
    format!("{}\n", plan.trim_end())
}

/// The cards' folds, menu and save dialog.
#[derive(Default)]
pub(crate) struct PlanState {
    expanded: HashSet<String>,
    menu: Option<(Box<ProposedPlan>, context_menu::State)>,
    save: Option<SaveDialog>,
    /// The plan last copied and when (`useCopyToClipboard`'s 2s "Copied!").
    copied: Option<(String, f64)>,
    /// The Implement button's menu.
    implement_menu: Option<context_menu::State>,
}

struct SaveDialog {
    plan: Box<ProposedPlan>,
    path: String,
    saving: bool,
    focus: bool,
}

const SAVE_REQUEST: &str = "plan-save";
const COLLAPSED_HEIGHT: f32 = 416.0; // max-h-104
const PADDING: f32 = 20.0; // sm:p-5

impl App {
    /// `ProposedPlanTimelineRow`: the card in a `px-1 py-0.5` row.
    pub(crate) fn plan_row(&mut self, ui: &mut Ui, plan: &ProposedPlan, width: f32) {
        let left = ui.cursor().left() + 4.0;
        let top = ui.cursor().top() + 2.0;
        let card_width = width - 8.0;
        let inner_left = left + PADDING;
        let inner_width = card_width - 2.0 * PADDING;
        let background = ui.painter().add(Shape::Noop);

        // Header: the badge, the title and the actions button.
        let header_top = top + PADDING;
        let badge_font = font(12.0, Weight::Medium);
        let badge_width = widgets::text_width(ui, "Plan", badge_font.clone()) + 6.0 + 2.0;
        let badge = Rect::from_min_size(
            Pos2::new(inner_left, header_top + 3.0),
            Vec2::new(badge_width, 18.0),
        );
        ui.painter()
            .rect_filled(badge, CornerRadius::same(6), theme::overlay(0.08));
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(badge.left() + 4.0, badge.top()), badge.max),
            "Plan",
            badge_font,
            theme::foreground(),
        );
        let actions = Rect::from_min_size(
            Pos2::new(inner_left + inner_width - 24.0, header_top),
            Vec2::splat(24.0),
        );
        let title =
            proposed_plan_title(&plan.plan_markdown).unwrap_or_else(|| "Proposed plan".into());
        let title_left = badge.right() + 8.0;
        let galley = widgets::galley_truncated(
            ui,
            &title,
            font(14.0, Weight::Medium),
            theme::foreground(),
            20.0,
            (actions.left() - 12.0 - title_left).max(1.0),
        );
        // The title is the plan's first searchable text (`searchablePlanSegments`).
        let mut marks = self.find_marks(&plan.id);
        let title_pos = Pos2::new(title_left, header_top + 2.0);
        let title_marks = marks.as_mut().map(|marks| {
            if galley.text() == title {
                marks.mark(ui, title_pos, &galley)
            } else {
                marks.skip(&title);
                Vec::new()
            }
        });
        ui.painter()
            .galley(title_pos, galley.clone(), theme::foreground());
        if let Some(title_marks) = &title_marks {
            crate::ui::markdown::FindMarks::recolor(ui, title_pos, &galley, title_marks);
        }
        let response = ui.interact(
            actions,
            ui.id().with(("plan-actions", &plan.id)),
            Sense::click(),
        );
        ui.painter().rect(
            actions,
            CornerRadius::same(8),
            if response.hovered() {
                theme::overlay(0.06)
            } else {
                theme::control_surface()
            },
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
        theme::paint_icon(
            ui,
            "ellipsis",
            Rect::from_center_size(actions.center(), Vec2::splat(16.0)),
            16.0,
            theme::foreground(),
        );
        let response = response.on_hover_text("Plan actions");
        if response.clicked() {
            self.plans.menu = match self.plans.menu.take() {
                Some((open, _)) if open.id == plan.id => None,
                _ => Some((
                    Box::new(plan.clone()),
                    context_menu::State::dropdown(actions.right_bottom() + Vec2::new(0.0, 4.0))
                        .aligned_end(),
                )),
            };
        }

        // The plan's text, folded when long.
        let can_collapse =
            plan.plan_markdown.len() > 900 || plan.plan_markdown.split('\n').count() > 20;
        let expanded = self.plans.expanded.contains(&plan.id);
        let collapsed = can_collapse && !expanded;
        let text = if collapsed {
            collapsed_preview(&plan.plan_markdown, 10)
        } else {
            strip_displayed_plan_markdown(&plan.plan_markdown)
        };
        let text_top = header_top + 24.0 + 16.0;
        let visible_height = if collapsed {
            COLLAPSED_HEIGHT
        } else {
            f32::INFINITY
        };
        let column = Rect::from_min_size(
            Pos2::new(inner_left, text_top),
            Vec2::new(inner_width, f32::INFINITY),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(column));
        child.set_clip_rect(
            Rect::from_min_size(column.min, Vec2::new(inner_width, visible_height))
                .intersect(ui.clip_rect()),
        );
        let workspace = self.workspace_cwd();
        let active = self
            .find_paint
            .as_ref()
            .is_some_and(|paint| paint.row_id == plan.id);
        let mut markdown = Markdown::new(&mut self.highlighter, ui.id().with(("plan", &plan.id)));
        markdown.media_root = workspace;
        markdown.media_thread = self.selected.clone();
        markdown.find = marks;
        let text_height = markdown.show(&mut child, &text, inner_width);
        let marks = markdown.find.take();
        let opened_file = markdown.opened_file.take();
        let asset_requests = std::mem::take(&mut markdown.asset_requests);
        let opened_media = markdown.opened_media.take();
        let media_menu = markdown.media_menu.take();
        // The active match outside the preview opens the plan (`beforematch`).
        let found = self.found_mark(marks);
        if active && collapsed && found.is_none_or(|rect| rect.bottom() > text_top + visible_height)
        {
            self.plans.expanded.insert(plan.id.clone());
            ui.ctx().request_repaint();
        }
        self.markdown_media(asset_requests, opened_media, media_menu);
        if let Some(file) = opened_file {
            self.open_markdown_file(&file.path);
        }
        let shown_height = text_height.min(visible_height);
        let mut bottom = text_top + shown_height;
        if collapsed {
            // `bg-linear-to-t from-card/95 via-card/80 to-transparent`, 6rem.
            let fade = Rect::from_min_max(
                Pos2::new(left + 1.0, bottom - 96.0),
                Pos2::new(left + card_width - 1.0, bottom),
            );
            let card = theme::card();
            let stops = [
                (fade.top(), theme::alpha(card, 0.0)),
                (fade.center().y, theme::alpha(card, 0.8)),
                (fade.bottom(), theme::alpha(card, 0.95)),
            ];
            let mut mesh = Mesh::default();
            for (y, color) in stops {
                mesh.colored_vertex(Pos2::new(fade.left(), y), color);
                mesh.colored_vertex(Pos2::new(fade.right(), y), color);
            }
            for row in 0..2u32 {
                let base = row * 2;
                mesh.add_triangle(base, base + 1, base + 2);
                mesh.add_triangle(base + 1, base + 2, base + 3);
            }
            ui.painter().add(Shape::mesh(mesh));
        }
        if can_collapse {
            let label = if expanded {
                "Collapse plan"
            } else {
                "Expand plan"
            };
            let label_font = font(14.0, Weight::Medium);
            let button_width = widgets::text_width(ui, label, label_font.clone()).ceil() + 18.0;
            let button = Rect::from_min_size(
                Pos2::new(
                    (inner_left + (inner_width - button_width) / 2.0).round(),
                    bottom + 16.0,
                ),
                Vec2::new(button_width, 28.0),
            );
            let response = ui.interact(
                button,
                ui.id().with(("plan-expand", &plan.id)),
                Sense::click(),
            );
            ui.painter().rect(
                button,
                CornerRadius::same(8),
                if response.hovered() {
                    theme::overlay(0.06)
                } else {
                    theme::control_surface()
                },
                Stroke::new(1.0, theme::input()),
                StrokeKind::Inside,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(button.left() + 9.0, button.top()), button.max),
                label,
                label_font,
                theme::foreground(),
            );
            if response.clicked() {
                if !self.plans.expanded.remove(&plan.id) {
                    self.plans.expanded.insert(plan.id.clone());
                }
                self.keep_scroll = true;
            }
            bottom = button.bottom();
        }
        let card = Rect::from_min_max(
            Pos2::new(left, top),
            Pos2::new(left + card_width, bottom + PADDING),
        );
        ui.painter().set(
            background,
            Shape::Rect(egui::epaint::RectShape::new(
                card,
                CornerRadius::same(24),
                theme::alpha(theme::card(), 0.7),
                Stroke::new(1.0, theme::alpha(theme::border(), 0.8)),
                StrokeKind::Inside,
            )),
        );
        ui.allocate_rect(
            Rect::from_min_max(
                ui.cursor().min,
                Pos2::new(left + card_width + 4.0, card.bottom() + 2.0),
            ),
            Sense::hover(),
        );
    }

    /// The card's actions menu.
    pub(crate) fn plan_menu(&mut self, ui: &mut Ui) {
        let Some((plan, state)) = self.plans.menu.take() else {
            return;
        };
        let now = ui.input(|i| i.time);
        let copied = self
            .plans
            .copied
            .as_ref()
            .is_some_and(|(id, at)| *id == plan.id && now - at < 2.0);
        let workspace = self.workspace_cwd();
        let mut save = Item::new("save", "Save to workspace");
        save.disabled = workspace.is_none() || self.plans.save.as_ref().is_some_and(|s| s.saving);
        let items = [
            Item::new(
                "copy",
                if copied {
                    "Copied!"
                } else {
                    "Copy to clipboard"
                },
            ),
            Item::new("download", "Download as markdown"),
            save,
        ];
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.plans.menu = Some((plan.clone(), state));
        }
        let contents = export_contents(&plan.plan_markdown);
        match clicked.as_deref() {
            Some("copy") => {
                ui.ctx().copy_text(contents);
                self.plans.copied = Some((plan.id.clone(), now));
            }
            Some("download") => {
                let filename = markdown_filename(&plan.plan_markdown);
                crate::ui::media::save_text_file(filename, contents, ui.ctx());
            }
            Some("save") => {
                if workspace.is_none() {
                    self.toasts.add(
                        Toast::new(Kind::Error, "Workspace path is unavailable").description(
                            "This thread does not have a workspace path to save into.",
                        ),
                    );
                    return;
                }
                let path = markdown_filename(&plan.plan_markdown);
                let path = match &self.plans.save {
                    Some(dialog) if !dialog.path.is_empty() => dialog.path.clone(),
                    _ => path,
                };
                self.plans.save = Some(SaveDialog {
                    plan,
                    path,
                    saving: false,
                    focus: true,
                });
            }
            _ => {}
        }
    }

    /// The "Save plan to workspace" dialog (`DialogPopup max-w-xl`).
    pub(crate) fn plan_save_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(mut dialog) = self.plans.save.take() else {
            return;
        };
        let workspace = self.workspace_cwd();
        let mut decision = None;
        let width = 576.0;
        let left_pad = 25.0;
        let inner = width - 2.0 * left_pad;
        let mut job = egui::text::LayoutJob::default();
        let muted = theme::muted_foreground();
        job.append(
            "Enter a path relative to ",
            0.0,
            widgets::format(font(14.0, Weight::Regular), muted, 20.0),
        );
        job.append(
            workspace.as_deref().unwrap_or("the workspace"),
            0.0,
            widgets::format(theme::code_font(), muted, 20.0),
        );
        job.append(
            ".",
            0.0,
            widgets::format(font(14.0, Weight::Regular), muted, 20.0),
        );
        job.wrap.max_width = inner;
        let description = ui.fonts_mut(|f| f.layout_job(job));
        let panel_top = 53.0 + description.size().y + 12.0;
        let footer_top = panel_top + 4.0 + 16.0 + 6.0 + 32.0 + 24.0;
        let d = Rect::from_center_size(screen.center(), Vec2::new(width, footer_top + 61.0));
        egui::Area::new(ui.id().with("plan-save-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, backdrop_response) =
                    ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if backdrop_response.clicked() && !ui.rect_contains_pointer(d) && !dialog.saving {
                    decision = Some(false);
                }
                ui.painter().rect(
                    d,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let close = Rect::from_min_size(
                    Pos2::new(d.right() - 41.0, d.top() + 9.0),
                    Vec2::splat(32.0),
                );
                if widgets::icon_button(ui, close, "plan-save-close", "x", 16.0, muted, "")
                    .clicked()
                    && !dialog.saving
                {
                    decision = Some(false);
                }
                let left = d.left() + left_pad;
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    "Save plan to workspace",
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                ui.painter()
                    .galley(Pos2::new(left, d.top() + 53.0), description.clone(), muted);
                let mut y = d.top() + panel_top + 4.0;
                widgets::text(
                    ui,
                    Pos2::new(left, y),
                    "Workspace path",
                    font(12.0, Weight::Medium),
                    theme::foreground(),
                    16.0,
                );
                y += 16.0 + 6.0;
                let field = Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner, 32.0));
                let placeholder = markdown_filename(&dialog.plan.plan_markdown);
                let edit = ui
                    .add_enabled_ui(!dialog.saving, |ui| {
                        crate::ui::action_editor::input(
                            ui,
                            field,
                            "plan-save-path",
                            &mut dialog.path,
                            &placeholder,
                            false,
                        )
                    })
                    .inner;
                if std::mem::take(&mut dialog.focus) {
                    edit.request_focus();
                }
                let band = Rect::from_min_max(
                    Pos2::new(d.left() + 1.0, d.top() + footer_top),
                    d.max - Vec2::splat(1.0),
                );
                ui.painter().rect_filled(
                    band,
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 17,
                        se: 17,
                    },
                    theme::overlay(0.016),
                );
                ui.painter().rect_filled(
                    Rect::from_min_size(band.min, Vec2::new(band.width(), 1.0)),
                    0.0,
                    theme::overlay(0.07),
                );
                use crate::ui::action_editor::{Kind, footer_button};
                // `size="sm"` buttons.
                let button = |right: f32, label: &str| {
                    let width =
                        (widgets::text_width(ui, label, font(14.0, Weight::Medium)) + 18.0).ceil();
                    Rect::from_min_size(
                        Pos2::new(right - width, band.top() + 16.0),
                        Vec2::new(width, 28.0),
                    )
                };
                let save_label = if dialog.saving { "Saving..." } else { "Save" };
                let save = button(d.right() - left_pad, save_label);
                let cancel = button(save.left() - 8.0, "Cancel");
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if !dialog.saving {
                    if footer_button(ui, save, save_label, Kind::Primary) || enter {
                        decision = Some(true);
                    }
                    if footer_button(ui, cancel, "Cancel", Kind::Outline)
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        decision = Some(false);
                    }
                } else {
                    ui.add_enabled_ui(false, |ui| {
                        footer_button(ui, save, save_label, Kind::Primary);
                        footer_button(ui, cancel, "Cancel", Kind::Outline);
                    });
                }
            });
        match decision {
            Some(false) => {}
            Some(true) => {
                let relative = dialog.path.trim().to_owned();
                let Some(cwd) = workspace else {
                    return;
                };
                if relative.is_empty() {
                    self.toasts
                        .add(Toast::new(Kind::Plain, "Enter a workspace path"));
                    self.plans.save = Some(dialog);
                    return;
                }
                dialog.saving = true;
                self.send_request(
                    SAVE_REQUEST.into(),
                    "projects.writeFile",
                    json!({
                        "cwd": cwd,
                        "relativePath": relative,
                        "contents": export_contents(&dialog.plan.plan_markdown),
                    }),
                );
                self.plans.save = Some(dialog);
            }
            None => self.plans.save = Some(dialog),
        }
    }

    /// The save's answer as a toast; `false` for other keys.
    pub(crate) fn plan_result(
        &mut self,
        key: &str,
        result: &Result<serde_json::Value, String>,
    ) -> bool {
        if key != SAVE_REQUEST {
            return false;
        }
        match result {
            Ok(value) => {
                self.plans.save = None;
                self.toasts.add(
                    Toast::new(Kind::Success, "Plan saved to workspace")
                        .description(value["relativePath"].as_str().unwrap_or_default()),
                );
            }
            Err(error) => {
                if let Some(dialog) = &mut self.plans.save {
                    dialog.saving = false;
                }
                self.toasts
                    .add(Toast::new(Kind::Error, "Could not save plan").description(error.clone()));
            }
        }
        true
    }
}

/// The composer in plan mode (`ChatComposer.tsx`, `ChatView.tsx`,
/// `ComposerPlanFollowUpBanner.tsx`, `ComposerPrimaryActions.tsx`): the
/// Plan/Build toggle while plan mode is enabled in the settings and the
/// provider offers it (`resolveComposerInteractionMode`); after a settled
/// plan-mode turn proposed a plan not yet implemented, the "Plan ready"
/// banner, Refine (with feedback, staying in plan mode) or Implement (the
/// plan as the prompt, back in build mode), and "Implement in a new thread".
impl App {
    /// `resolveComposerInteractionMode`'s `enabled`.
    pub(crate) fn plan_mode_enabled(&self, draft: bool) -> bool {
        if self.client_setting("planModeEnabled") != true {
            return false;
        }
        let instance = self.composer_model_selection(draft).instance_id;
        self.providers()
            .iter()
            .find(|p| p["instanceId"] == instance.as_str())
            .is_some_and(|p| p["showInteractionModeToggle"] != false)
    }

    /// The composer's interaction mode: its own choice, else the thread's,
    /// and always "default" when plan mode is off.
    pub(crate) fn composer_interaction_mode(&self, draft: bool) -> String {
        if !self.plan_mode_enabled(draft) {
            return "default".into();
        }
        if let Some(mode) = self.composer_interaction.get(&self.composer_key(draft)) {
            return mode.clone();
        }
        match self.current_thread() {
            Some(thread) if !draft && !thread.interaction_mode.is_empty() => {
                thread.interaction_mode.clone()
            }
            _ => "default".into(),
        }
    }

    /// `shouldShowPlanFollowUpPrompt` with `findLatestProposedPlan`: an
    /// active plan of the settled latest run, in plan mode.
    pub(crate) fn plan_follow_up(
        &self,
        thread: &crate::model::Thread,
        detail: &crate::model::Detail,
    ) -> Option<ProposedPlan> {
        if !thread.latest_run_settled()
            || !self.composer_images.is_empty()
            || !self.composer_files.is_empty()
            || self.composer_interaction_mode(false) != "plan"
        {
            return None;
        }
        let latest = thread.latest_run.as_ref().map(|r| r.run_id.as_str());
        let plan = crate::timeline::latest_proposed_plan(detail, latest)?;
        (plan.status == "active").then_some(plan)
    }

    /// The Plan/Build toggle after the runtime picker, its icon alone when
    /// `icon_only`; returns its right edge.
    pub(crate) fn interaction_toggle(
        &mut self,
        ui: &mut Ui,
        x: f32,
        top: f32,
        draft: bool,
        icon_only: bool,
    ) -> f32 {
        let plan = self.composer_interaction_mode(draft) == "plan";
        let (icon, label, tip) = if plan {
            (
                "pencil-ruler",
                "Plan",
                "Plan mode — click to return to normal build mode",
            )
        } else {
            ("bot", "Build", "Default mode — click to enter plan mode")
        };
        let label_font = font(14.0, Weight::Medium);
        let width = if icon_only {
            10.0 + 16.0 + 10.0
        } else {
            crate::ui::composer::toggle_width(widgets::text_width(ui, label, label_font.clone()))
        };
        let rect = Rect::from_min_size(Pos2::new(x, top), Vec2::new(width, 28.0));
        let response = ui
            .interact(rect, ui.id().with("interaction-toggle"), Sense::click())
            .on_hover_text(tip);
        if plan {
            let fill = if response.hovered() {
                theme::alpha(theme::accent(), 0.8)
            } else {
                theme::accent()
            };
            ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), theme::accent());
        }
        let color = if plan || response.hovered() {
            theme::foreground()
        } else {
            theme::secondary_label()
        };
        let icon_color = if plan {
            theme::foreground()
        } else {
            theme::muted_foreground()
        };
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(Pos2::new(x + 10.0, top + 6.0), Vec2::splat(16.0)),
            16.0,
            icon_color,
        );
        if !icon_only {
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x + 32.0, top), rect.max),
                label,
                label_font,
                color,
            );
        }
        if response.clicked() {
            let next = if plan { "default" } else { "plan" };
            let key = self.composer_key(draft);
            self.composer_interaction.insert(key, next.into());
        }
        rect.right()
    }

    /// The "Plan ready" row attached above the composer `surface`.
    pub(crate) fn plan_banner(&self, ui: &Ui, surface: Rect, plan: &ProposedPlan) {
        use crate::ui::requests_panel::{OVERLAP, plan_banner_surface};
        let rect = Rect::from_min_max(
            Pos2::new(surface.left() + 22.0, surface.top() + OVERLAP - PLAN_BANNER),
            Pos2::new(surface.right() - 22.0, surface.top() + OVERLAP),
        );
        plan_banner_surface(ui, rect);
        let row = Rect::from_min_size(
            Pos2::new(rect.left() + 4.0 + 24.0 + 4.0, rect.top() + 4.0),
            Vec2::new(rect.width() - 8.0 - 28.0, 24.0),
        );
        let small = font(12.0, Weight::Medium);
        let lead = widgets::text_in(ui, row, "Plan ready", small, theme::muted_foreground());
        if let Some(title) = proposed_plan_title(&plan.plan_markdown) {
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(row.left() + lead + 4.0, row.top()), row.max),
                &title,
                font(12.0, Weight::Regular),
                theme::alpha(theme::foreground(), 0.85),
            );
        }
    }

    /// Refine, or Implement with its menu; returns the left edge.
    pub(crate) fn plan_actions(
        &mut self,
        ui: &mut Ui,
        right: f32,
        bar_top: f32,
        plan: &ProposedPlan,
        submit: bool,
    ) -> f32 {
        let enabled = self.connected();
        let label_font = font(14.0, Weight::Medium);
        let top = bar_top - 2.0;
        let pill = |ui: &mut Ui, rect: Rect, id: &str, corners: CornerRadius| {
            let response = ui.interact(
                rect,
                ui.id().with(id),
                if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            let opacity = if enabled { 1.0 } else { 0.64 };
            let hovered = response.hovered() && enabled;
            let text = crate::ui::backdrop::fill_action(ui, rect, corners, hovered, opacity);
            (response, text)
        };
        if !self.composer.trim().is_empty() {
            let width = (widgets::text_width(ui, "Refine", label_font.clone()) + 32.0).ceil();
            let rect = Rect::from_min_size(Pos2::new(right - width, top), Vec2::new(width, 32.0));
            let (response, text) = pill(ui, rect, "plan-refine", CornerRadius::same(16));
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(rect.left() + 16.0, rect.top()), rect.max),
                "Refine",
                label_font,
                text,
            );
            if (response.clicked() || submit) && enabled {
                self.submit_plan_follow_up(plan);
            }
            return rect.left();
        }
        let chevron = Rect::from_min_size(Pos2::new(right - 30.0, top), Vec2::new(30.0, 32.0));
        let width = (widgets::text_width(ui, "Implement", label_font.clone()) + 32.0).ceil();
        let main = Rect::from_min_size(
            Pos2::new(chevron.left() - width, top),
            Vec2::new(width, 32.0),
        );
        let (response, text) = pill(
            ui,
            main,
            "plan-implement",
            CornerRadius {
                nw: 16,
                sw: 16,
                ne: 0,
                se: 0,
            },
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(main.left() + 16.0, main.top()), main.max),
            "Implement",
            label_font,
            text,
        );
        if (response.clicked() || submit) && enabled {
            self.submit_plan_follow_up(plan);
        }
        let (menu_response, text) = pill(
            ui,
            chevron,
            "plan-implement-menu",
            CornerRadius {
                nw: 0,
                sw: 0,
                ne: 16,
                se: 16,
            },
        );
        ui.painter().rect_filled(
            Rect::from_min_size(chevron.min, Vec2::new(1.0, chevron.height())),
            0.0,
            theme::alpha(Color32::WHITE, 0.2),
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_center_size(chevron.center(), Vec2::splat(14.0)),
            14.0,
            text,
        );
        if menu_response
            .on_hover_text("Implementation actions")
            .clicked()
            && enabled
        {
            // `MenuPopup align="end" side="top"`: opened above the button.
            self.plans.implement_menu = match self.plans.implement_menu.take() {
                Some(_) => None,
                None => Some(
                    context_menu::State::dropdown(Pos2::new(
                        chevron.right(),
                        chevron.top() - 4.0 - 38.0,
                    ))
                    .aligned_end(),
                ),
            };
        }
        if self.plans.implement_menu.is_some() {
            let items = [Item::new("new-thread", "Implement in a new thread")];
            let clicked = context_menu::show(ui, &mut self.plans.implement_menu, &items);
            if clicked.as_deref() == Some("new-thread") {
                self.implement_plan_in_new_thread(plan);
            }
        }
        main.left()
    }

    /// `resolvePlanFollowUpSubmission`: feedback refines the plan in plan
    /// mode; an empty composer implements it in build mode.
    fn submit_plan_follow_up(&mut self, plan: &ProposedPlan) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        let draft_text = self.composer.trim().to_owned();
        let (text, mode) = if draft_text.is_empty() {
            (implementation_prompt(&plan.plan_markdown), "default")
        } else {
            (draft_text, "plan")
        };
        let model = self.composer_model_json(false);
        let runtime = self.composer_runtime_mode(false);
        let mut command = crate::ui::composer::message_dispatch(&thread.id, &text, &model, "auto");
        command["titleSeed"] = json!(thread.title);
        if mode == "default" {
            command["sourcePlanRef"] = json!({"threadId": thread.id, "planId": plan.id});
        }
        crate::ui::composer_context::apply(
            &mut command,
            &text,
            &self.composer_contexts,
            self.inline_message_context(),
        );
        let before = self.mode_commands(&thread, &runtime, mode);
        self.start_turn(crate::ui::composer::TurnSend {
            thread_id: thread.id.clone(),
            before,
            message: command,
            launch: false,
            uploads: Vec::new(),
        });
        self.composer.clear();
        self.composer_contexts.clear();
        let key = self.composer_key(false);
        self.composer_interaction.insert(key, mode.into());
    }

    /// `onImplementPlanInNewThread`: a thread in the same project and
    /// checkout, named after the plan, implementing it in build mode.
    fn implement_plan_in_new_thread(&mut self, plan: &ProposedPlan) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        let title: String = {
            let title = implementation_thread_title(&plan.plan_markdown);
            let chars: Vec<char> = title.chars().collect();
            if chars.len() <= 50 {
                title
            } else {
                format!("{}...", chars[..50].iter().collect::<String>())
            }
        };
        let model = self.composer_model_json(false);
        let runtime = self.state.config["settings"]["defaultRuntimeMode"]
            .as_str()
            .unwrap_or("full-access")
            .to_owned();
        let thread_id = uuid::Uuid::new_v4().to_string();
        let prompt = implementation_prompt(&plan.plan_markdown);
        // `createThread`, then `startThreadTurn` with the plan as its source.
        let mut create = crate::model::command("thread.create", &thread_id);
        create["createdBy"] = json!("user");
        create["creationSource"] = json!("web");
        create["projectId"] = json!(thread.project_id);
        create["title"] = json!(title);
        create["modelSelection"] = model.clone();
        create["runtimeMode"] = json!(runtime);
        create["interactionMode"] = json!("default");
        create["branch"] = json!(thread.branch);
        create["worktreePath"] = json!(thread.worktree_path);
        let mut command =
            crate::ui::composer::message_dispatch(&thread_id, &prompt, &model, "auto");
        command["titleSeed"] = json!(title);
        command["sourcePlanRef"] = json!({"threadId": thread.id, "planId": plan.id});
        self.start_turn(crate::ui::composer::TurnSend {
            thread_id: thread_id.clone(),
            before: vec![create],
            message: command,
            launch: false,
            uploads: Vec::new(),
        });
        self.select_thread(Some(thread_id));
    }
}

/// The "Plan ready" banner's height, its overlap with the composer included.
pub(crate) const PLAN_BANNER: f32 = 4.0 + 24.0 + 4.0 + crate::ui::requests_panel::OVERLAP;

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &str =
        "# Migrer le rendu\n\n## Summary\n\nRemplacer le moteur.\n\n## Étapes\n\n1. Un\n2. Deux\n";

    #[test]
    fn plans_show_without_their_title_and_summary_heading() {
        assert_eq!(
            proposed_plan_title(PLAN).as_deref(),
            Some("Migrer le rendu")
        );
        assert_eq!(proposed_plan_title("    # code"), None);
        assert_eq!(proposed_plan_title("#nospace"), None);
        assert_eq!(
            strip_displayed_plan_markdown(PLAN),
            "Remplacer le moteur.\n\n## Étapes\n\n1. Un\n2. Deux"
        );
    }

    #[test]
    fn collapsed_previews_keep_the_first_lines() {
        assert_eq!(
            collapsed_preview(PLAN, 2),
            "Remplacer le moteur.\n\n## Étapes\n\n..."
        );
        assert_eq!(collapsed_preview("# Seul titre", 10), "Seul titre");
        assert_eq!(collapsed_preview("", 10), "Plan preview unavailable.");
    }

    #[test]
    fn exports_and_follow_ups_follow_upstream() {
        assert_eq!(markdown_filename(PLAN), "migrer-le-rendu.md");
        assert_eq!(
            markdown_filename("# Fix: the `app`'s (big) bug!"),
            "fix-the-apps-big-bug.md"
        );
        assert_eq!(markdown_filename("no title"), "plan.md");
        assert_eq!(
            implementation_thread_title(PLAN),
            "Implement Migrer le rendu"
        );
        assert_eq!(implementation_thread_title("steps"), "Implement plan");
        assert_eq!(
            implementation_prompt("  # P\n1. a  \n"),
            "PLEASE IMPLEMENT THIS PLAN:\n# P\n1. a"
        );
        assert_eq!(export_contents("a\n\n"), "a\n");
    }
}
