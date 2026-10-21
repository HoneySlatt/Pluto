//! Main workspace: header (`ChatHeader.tsx`), the messages timeline
//! (`MessagesTimeline.tsx`) and the empty/draft hero (`DraftHeroHeadline.tsx`).
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::model::{Message, Project};
use crate::theme::{self, Weight, font};
use crate::timeline::{self, Row, WorkEntry};
use crate::ui::markdown::Markdown;
use crate::ui::{project_icon, widgets};

const MESSAGE_LINE: f32 = 22.75;

impl App {
    pub(crate) fn main_area(&mut self, ui: &mut Ui, rect: Rect) {
        ui.painter().rect_filled(rect, 0.0, theme::background());
        let header = Rect::from_min_size(rect.min, Vec2::new(rect.width(), theme::TOPBAR_HEIGHT));
        let body = Rect::from_min_max(Pos2::new(rect.left(), header.bottom()), rect.max);
        let thread = self.current_thread();
        let draft_project = self.draft_project();
        self.header(
            ui,
            header,
            thread.as_ref().map(|t| t.title.as_str()),
            draft_project.as_ref(),
        );

        let column = theme::CHAT_WIDTH.min(body.width() - 2.0 * theme::WORKSPACE_GUTTER);
        let column_left = (body.center().x - column / 2.0).round();
        if thread.is_some() {
            let mode = self.composer_mode(false);
            let composer_top = self.composer_layout_top(ui, body, column, &mode);
            // The timeline scrolls under the composer, padded so its end clears it.
            let overlay = self.composer_overlay(ui, body, column, &mode);
            // The timeline keeps 26px gutters (the composer 20px) on narrow columns.
            let timeline_width = theme::CHAT_WIDTH.min(body.width() - 52.0);
            let timeline_left = (body.center().x - timeline_width / 2.0).round();
            self.timeline(ui, body, timeline_left, timeline_width, overlay);
            ui.painter().rect_filled(
                Rect::from_min_max(
                    Pos2::new(column_left, composer_top + 22.0),
                    Pos2::new(column_left + column, body.bottom()),
                ),
                0.0,
                theme::background(),
            );
            self.composer(
                ui,
                Pos2::new(column_left, composer_top),
                column,
                false,
                &mode,
            );
        } else if let Some(project) = draft_project {
            // Hero + composer, vertically centered as one group.
            let composer_top = (body.center().y - 98.0).round();
            self.draft_hero(ui, &project, column_left, column, composer_top);
            self.composer(
                ui,
                Pos2::new(column_left, composer_top),
                column,
                true,
                &crate::ui::composer::Mode::Normal,
            );
        } else {
            self.empty_state(ui, body);
        }
    }

    fn header(&mut self, ui: &mut Ui, rect: Rect, title: Option<&str>, draft: Option<&Project>) {
        let project = self
            .current_thread()
            .and_then(|t| self.state.project(&t.project_id).cloned())
            .or_else(|| draft.cloned());
        let mut x = rect.left() + theme::WORKSPACE_GUTTER;
        let title = title.unwrap_or("New thread");
        // Right-side controls, laid out from the right edge. With the right
        // panel open, its header owns the layout toggles and these buttons
        // collapse to icons (their labels become screen-reader only upstream).
        let compact = self.panel_open() && self.current_thread().is_some();
        let mut right = if compact {
            rect.right() - theme::WORKSPACE_GUTTER
        } else {
            self.panel_controls(ui, rect.right());
            rect.right() - 13.0 - 64.0 + 4.0 - 12.0
        };
        let gap = if compact { 8.0 } else { 12.0 };
        if project.is_some() {
            if !self.workspace_is_repo() {
                // Not a repository: a lone "Initialize Git" button.
                let label = if self.vcs_init.is_some() {
                    "Initializing..."
                } else {
                    "Initialize Git"
                };
                // The label's `ml-0.5` span measures 1px wider than the
                // shared outline-button width (98px in the reference).
                let width = widgets::outline_button_width(ui, true, label) + 1.0;
                let rect_button =
                    Rect::from_min_size(Pos2::new(right - width, 14.0), Vec2::new(width, 24.0));
                if widgets::outline_button(
                    ui,
                    rect_button,
                    "initialize-git",
                    Some(("git-branch-plus", theme::muted_foreground())),
                    label,
                    CornerRadius::same(8),
                )
                .clicked()
                {
                    self.initialize_git();
                }
                right = rect_button.left() - gap;
            } else if let Some(action) = self.git_action_label() {
                right = self.split_button(ui, right, &action.0, action.1, "git-action", compact);
                right -= gap;
            }
            right = self.split_button(ui, right, "Open", "zed", "open-in", compact);
            right -= gap;
            let label = if compact { "" } else { "Add action" };
            let width = if compact {
                24.0
            } else {
                widgets::outline_button_width(ui, true, label)
            };
            let rect_button =
                Rect::from_min_size(Pos2::new(right - width, 14.0), Vec2::new(width, 24.0));
            if widgets::outline_button(
                ui,
                rect_button,
                "add-action",
                Some(("plus", theme::muted_foreground())),
                label,
                CornerRadius::same(8),
            )
            .clicked()
            {
                // `ProjectScriptsControl` with no actions: the editor.
                let target = project
                    .as_ref()
                    .map(|p| p.id.clone())
                    .or_else(|| self.draft_project().map(|p| p.id));
                if let Some(id) = target {
                    self.open_action_editor(&id, None);
                }
            }
            right = rect_button.left();
        }

        // Breadcrumb: project / thread title.
        let crumb = Rect::from_min_max(Pos2::new(x, 15.0), Pos2::new(right - 12.0, 37.0));
        if let Some(project) = &project {
            project_icon::paint(
                ui,
                Rect::from_min_size(Pos2::new(x, 19.0), Vec2::splat(14.0)),
                project,
            );
            x += 20.0;
            let width = widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x, crumb.top()), crumb.max),
                &project.title,
                font(14.0, Weight::Medium),
                theme::muted_foreground(),
            );
            x += width + 12.0;
            let slash = widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x, crumb.top()), crumb.max),
                "/",
                font(14.0, Weight::Regular),
                theme::muted_foreground(),
            );
            x += slash + 12.0;
        }
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(x, crumb.top()), crumb.max),
            title,
            font(14.0, Weight::Medium),
            theme::foreground(),
        );
    }

    /// Split button (label + chevron) ending at `right`; returns its left edge.
    fn split_button(
        &mut self,
        ui: &mut Ui,
        right: f32,
        label: &str,
        icon: &str,
        id: &str,
        compact: bool,
    ) -> f32 {
        let chevron = Rect::from_min_size(Pos2::new(right - 24.0, 14.0), Vec2::new(24.0, 24.0));
        let width = if compact {
            25.0
        } else {
            widgets::outline_button_width(ui, true, label)
        };
        let shown = if compact { "" } else { label };
        let main = Rect::from_min_size(
            Pos2::new(chevron.left() - 1.0 - width, 14.0),
            Vec2::new(width, 24.0),
        );
        let tint = if icon == "zed" {
            theme::foreground()
        } else {
            theme::muted_foreground()
        };
        let left_corners = CornerRadius {
            nw: 8,
            sw: 8,
            ne: 0,
            se: 0,
        };
        let right_corners = CornerRadius {
            nw: 0,
            sw: 0,
            ne: 8,
            se: 8,
        };
        let clicked = widgets::outline_button(
            ui,
            main,
            (id, "main"),
            Some((icon, tint)),
            shown,
            left_corners,
        )
        .clicked();
        let menu = widgets::outline_button(ui, chevron, (id, "menu"), None, "", right_corners);
        theme::paint_icon(ui, "chevron-down", chevron, 16.0, theme::muted_foreground());
        // Shared 1px divider between the halves.
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(main.right(), 14.0), Vec2::new(1.0, 24.0)),
            0.0,
            theme::input(),
        );
        if clicked || menu.clicked() {
            self.status = Some(format!("{label} is not implemented yet"));
        }
        main.left()
    }

    fn empty_state(&mut self, ui: &mut Ui, body: Rect) {
        let center = body.center();
        let headline = "What should we work on?";
        let width = widgets::text_width(ui, headline, font(30.0, Weight::SemiBold));
        widgets::text(
            ui,
            Pos2::new(center.x - width / 2.0, center.y - 82.0),
            headline,
            font(30.0, Weight::SemiBold),
            theme::foreground(),
            36.0,
        );
        let sub = "Add a project, or start without one.";
        let width = widgets::text_width(ui, sub, font(14.0, Weight::Regular));
        widgets::text(
            ui,
            Pos2::new(center.x - width / 2.0, center.y - 42.0),
            sub,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            20.0,
        );
    }

    fn draft_hero(
        &mut self,
        ui: &mut Ui,
        project: &Project,
        left: f32,
        width: f32,
        composer_top: f32,
    ) {
        let regular = font(30.0, Weight::Regular);
        let medium = font(30.0, Weight::Medium);
        let mut job = egui::text::LayoutJob::default();
        let mut format = widgets::format(regular, theme::foreground(), 36.0);
        format.extra_letter_spacing = -0.75;
        job.append("What should we build in ", 0.0, format.clone());
        let mut name = widgets::format(medium, theme::foreground(), 36.0);
        name.extra_letter_spacing = -0.75;
        job.append(&project.title, 0.0, name);
        job.append("?", 0.0, format);
        let galley = widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job)));
        let top = composer_top - 100.0;
        let pos = Pos2::new(left + (width - galley.size().x) / 2.0, top);
        // Dotted underline under the project name.
        let prefix =
            widgets::text_width(ui, "What should we build in ", font(30.0, Weight::Regular))
                - 0.75 * 24.0;
        let name_width = widgets::text_width(ui, &project.title, font(30.0, Weight::Medium))
            - 0.75 * project.title.chars().count() as f32;
        let underline_y = top + 33.0;
        let mut dot_x = pos.x + prefix;
        while dot_x < pos.x + prefix + name_width {
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(dot_x, underline_y), Vec2::new(1.5, 1.5)),
                0.0,
                theme::alpha(theme::foreground(), 0.4),
            );
            dot_x += 4.0;
        }
        ui.painter().galley(pos, galley, theme::foreground());
        let sub = "or start without a project";
        let sub_width = widgets::text_width(ui, sub, font(14.0, Weight::Medium));
        let sub_rect = Rect::from_min_size(
            Pos2::new(left + (width - sub_width) / 2.0, top + 46.0),
            Vec2::new(sub_width, 20.0),
        );
        let response = ui.interact(
            sub_rect,
            ui.id().with("start-without-project"),
            Sense::click(),
        );
        widgets::text_in(
            ui,
            sub_rect,
            sub,
            font(14.0, Weight::Medium),
            if response.hovered() {
                theme::foreground()
            } else {
                theme::muted_foreground()
            },
        );
        if response.clicked() {
            self.status = Some("Scratch threads are not implemented yet".into());
        }
    }

    fn timeline(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        column_left: f32,
        column: f32,
        bottom_padding: f32,
    ) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        let rows = timeline::rows(&thread, &self.expansion);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        egui::ScrollArea::vertical()
            .id_salt(("timeline", &thread.id))
            .auto_shrink([false, false])
            // Toggling a fold keeps the clicked row in place instead of following the end.
            .stick_to_bottom(!std::mem::take(&mut self.keep_scroll))
            .show(&mut child, |ui| {
                ui.set_width(rect.width());
                ui.add_space(22.0);
                let inner = Rect::from_min_size(
                    Pos2::new(column_left, ui.cursor().top()),
                    Vec2::new(column, f32::INFINITY),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    for row in &rows {
                        self.row(ui, row, column);
                    }
                });
                ui.add_space(bottom_padding);
            });
    }

    fn row(&mut self, ui: &mut Ui, row: &Row, width: f32) {
        match row {
            Row::User(message) => {
                self.user_row(ui, message, width);
                ui.add_space(16.0);
            }
            Row::Assistant {
                message,
                show_meta,
                changes,
            } => {
                self.assistant_row(ui, message, *show_meta, changes.as_ref(), width);
                ui.add_space(if *show_meta { 16.0 } else { 8.0 });
            }
            Row::TurnFold {
                turn_id,
                label,
                expanded,
                ..
            } => {
                if self.fold_row(ui, label, *expanded, width).clicked() {
                    if !self.expansion.turns.remove(turn_id) {
                        self.expansion.turns.insert(turn_id.clone());
                    }
                    self.keep_scroll = true;
                }
                ui.add_space(6.0);
            }
            Row::Work { label, entry, .. } => {
                self.work_entry_row(ui, entry, label, width, false);
                ui.add_space(8.0);
            }
            Row::WorkToggle {
                group_id,
                summary,
                expanded,
                ..
            } => {
                if self
                    .work_row(ui, "hammer", summary, width, Some(group_id))
                    .clicked()
                {
                    if !self.expansion.groups.remove(group_id) {
                        self.expansion.groups.insert(group_id.clone());
                    }
                    self.keep_scroll = true;
                }
                ui.add_space(if *expanded { 0.0 } else { 8.0 });
            }
            Row::WorkDetails { entries, .. } => {
                for entry in entries {
                    let label = timeline::display_label(entry);
                    self.work_entry_row(ui, entry, &label, width, true);
                }
                ui.add_space(4.0);
            }
            Row::Working { started_at } => {
                self.working_row(ui, started_at.as_deref(), width);
                ui.add_space(6.0);
            }
        }
    }

    fn user_row(&mut self, ui: &mut Ui, message: &Message, width: f32) {
        let max_width = (width * 0.8).floor();
        let galley = widgets::galley(
            ui,
            message.text.trim_end(),
            font(14.0, Weight::Regular),
            theme::foreground(),
            MESSAGE_LINE,
            max_width - 24.0,
        );
        let bubble_size = Vec2::new(galley.size().x + 24.0, galley.size().y + 24.0);
        let (row_rect, response) =
            ui.allocate_exact_size(Vec2::new(width, bubble_size.y + 4.0 + 24.0), Sense::hover());
        let bubble = Rect::from_min_size(
            Pos2::new(row_rect.right() - bubble_size.x, row_rect.top()),
            bubble_size,
        );
        ui.painter()
            .rect_filled(bubble, CornerRadius::same(18), theme::message_surface());
        ui.painter()
            .galley(bubble.min + Vec2::splat(12.0), galley, theme::foreground());
        if response.hovered() || ui.rect_contains_pointer(row_rect) {
            let meta_top = bubble.bottom() + 4.0;
            let copy = Rect::from_min_size(
                Pos2::new(row_rect.right() - 4.0 - 24.0, meta_top),
                Vec2::splat(24.0),
            );
            if widgets::icon_button(
                ui,
                copy,
                ("copy-user", &message.id),
                "copy",
                12.0,
                theme::muted_foreground(),
                "Copy message",
            )
            .clicked()
            {
                ui.ctx().copy_text(message.text.clone());
            }
            let time = widgets::clock_time(&message.created_at, self.now);
            let time_width = widgets::text_width(ui, &time, font(12.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_min_size(
                    Pos2::new(copy.left() - 8.0 - time_width, meta_top),
                    Vec2::new(time_width + 1.0, 24.0),
                ),
                &time,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            );
        }
    }

    fn assistant_row(
        &mut self,
        ui: &mut Ui,
        message: &Message,
        show_meta: bool,
        changes: Option<&(String, Vec<crate::model::CheckpointFile>)>,
        width: f32,
    ) {
        let top = ui.cursor().top();
        let left = ui.cursor().left();
        let text = if message.text.is_empty() && !message.streaming {
            "(empty response)"
        } else {
            message.text.as_str()
        };
        ui.add_space(2.0);
        let inner = Rect::from_min_size(
            Pos2::new(left + 4.0, ui.cursor().top()),
            Vec2::new(width - 8.0, f32::INFINITY),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            Markdown {
                highlighter: &mut self.highlighter,
                id: ui.id().with(("markdown", &message.id)),
            }
            .show(ui, text, width - 8.0);
            if let Some((turn_id, files)) = changes {
                ui.add_space(16.0);
                self.changed_files_card(ui, turn_id, files, width - 8.0);
            }
        });
        ui.add_space(2.0);
        if show_meta {
            ui.add_space(6.0);
            let (meta, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
            let row_rect = Rect::from_min_max(Pos2::new(left, top), meta.max);
            if ui.rect_contains_pointer(row_rect) {
                let copy = Rect::from_min_size(
                    Pos2::new(meta.left() + 4.0, meta.top()),
                    Vec2::splat(24.0),
                );
                if widgets::icon_button(
                    ui,
                    copy,
                    ("copy-assistant", &message.id),
                    "copy",
                    12.0,
                    theme::muted_foreground(),
                    "Copy message",
                )
                .clicked()
                {
                    ui.ctx().copy_text(message.text.clone());
                }
                if !message.streaming {
                    let time = widgets::clock_time(&message.updated_at, self.now);
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(Pos2::new(copy.right() + 8.0, meta.top()), meta.max),
                        &time,
                        font(12.0, Weight::Regular),
                        theme::muted_foreground(),
                    );
                }
            }
        }
    }

    fn fold_row(&mut self, ui: &mut Ui, label: &str, expanded: bool, width: f32) -> egui::Response {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(width, 4.0 + MESSAGE_LINE + 8.0 + 1.0),
            Sense::hover(),
        );
        let label_font = font(14.0, Weight::Regular);
        let text_width = widgets::text_width(ui, label, label_font.clone());
        let button = Rect::from_min_size(
            Pos2::new(rect.left(), rect.top() + 4.0),
            Vec2::new(text_width + 4.0 + 14.0 + 8.0, MESSAGE_LINE),
        );
        let response = ui.interact(
            button,
            ui.id().with(("fold", label, rect.top() as i32)),
            Sense::click(),
        );
        let color = if response.hovered() {
            theme::foreground()
        } else {
            theme::muted_foreground()
        };
        widgets::text(
            ui,
            Pos2::new(button.left() + 4.0, button.top()),
            label,
            label_font,
            color,
            MESSAGE_LINE,
        );
        theme::paint_icon(
            ui,
            if expanded {
                "chevron-down"
            } else {
                "chevron-right"
            },
            Rect::from_min_size(
                Pos2::new(button.left() + 4.0 + text_width + 4.0, button.top() + 4.0),
                Vec2::splat(14.0),
            ),
            14.0,
            color,
        );
        ui.painter().rect_filled(
            Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 1.0), rect.max),
            0.0,
            theme::overlay(0.036),
        );
        response
    }

    fn work_row(
        &mut self,
        ui: &mut Ui,
        icon: &str,
        label: &str,
        width: f32,
        toggle: Option<&str>,
    ) -> egui::Response {
        let height = 2.0 + MESSAGE_LINE + 4.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let response = ui.interact(
            rect,
            ui.id().with(("work", toggle, label, rect.top() as i32)),
            if toggle.is_some() {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        if toggle.is_some() && response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), theme::overlay(0.008));
        }
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(
                Pos2::new(rect.left() + 2.0, rect.top()),
                Vec2::new(24.0, height),
            ),
            16.0,
            theme::muted_foreground(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(rect.left() + 32.0, rect.top()), rect.max),
            label,
            font(14.0, Weight::Regular),
            theme::secondary_label(),
        );
        response
    }

    /// One work-log entry (`SimpleWorkEntryRow`): icon, label, optional answer
    /// preview, and a disclosure for its command/detail body.
    fn work_entry_row(
        &mut self,
        ui: &mut Ui,
        entry: &WorkEntry,
        label: &str,
        width: f32,
        in_group: bool,
    ) {
        let key = format!("entry:{}", entry.id);
        let expanded = self.expansion.groups.contains(&key);
        let failed = timeline::indicates_failure(entry);
        let tool_like = timeline::is_tool_like(entry);
        let destructive =
            failed && (entry.tone == "error" && entry.item_type.is_none() || !tool_like);
        let body = expanded_body(entry, label);
        let can_expand =
            entry.question.is_some() || (failed && !label.trim().is_empty()) || body.is_some();
        // Measured row boxes: 32px standalone, 28px inside an expanded group.
        let pad = if in_group { 2.625 } else { 4.625 };
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(width, pad * 2.0 + MESSAGE_LINE), Sense::hover());
        let response = ui.interact(
            rect,
            ui.id().with(("work-entry", &entry.id)),
            if can_expand {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        if can_expand && response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), theme::overlay(0.008));
        }
        let (icon, icon_color) = if destructive {
            ("circle-alert", DESTRUCTIVE)
        } else if entry.question.is_some() {
            ("message-circle", theme::muted_foreground())
        } else {
            let icon = work_icon(entry);
            let color = if tool_like || matches!(entry.tone.as_str(), "thinking" | "info") {
                theme::muted_foreground()
            } else {
                theme::foreground()
            };
            (icon, color)
        };
        let line = Rect::from_min_size(
            Pos2::new(rect.left() + 2.0, rect.top() + pad),
            Vec2::new(width - 4.0, MESSAGE_LINE),
        );
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(line.min, Vec2::new(24.0, MESSAGE_LINE)),
            16.0,
            icon_color,
        );
        let (label_font, label_color) = if destructive {
            (font(14.0, Weight::Medium), DESTRUCTIVE)
        } else if tool_like {
            (font(14.0, Weight::Regular), theme::secondary_label())
        } else {
            (
                font(14.0, Weight::Regular),
                theme::alpha(theme::foreground(), 0.8),
            )
        };
        let text_right = line.right() - 22.0;
        let label_width = widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(line.left() + 30.0, line.top()),
                Pos2::new(text_right, line.bottom()),
            ),
            label,
            label_font,
            label_color,
        );
        if let Some(answer) = entry.question.as_ref().filter(|q| !q.answers.is_empty()) {
            let x = line.left() + 30.0 + label_width + 6.0;
            if x < text_right {
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(x, line.top()),
                        Pos2::new(text_right, line.bottom()),
                    ),
                    &answer.answers.join(" · "),
                    font(14.0, Weight::Regular),
                    theme::foreground(),
                );
            }
        }
        if can_expand {
            theme::paint_icon(
                ui,
                if expanded {
                    "chevron-down"
                } else {
                    "chevron-right"
                },
                Rect::from_min_size(
                    Pos2::new(line.right() - 16.0, line.top()),
                    Vec2::new(16.0, MESSAGE_LINE),
                ),
                12.0,
                theme::alpha(theme::muted_foreground(), 0.7),
            );
            if response.clicked() {
                if !self.expansion.groups.remove(&key) {
                    self.expansion.groups.insert(key);
                }
                self.keep_scroll = true;
            }
        }
        if expanded && let Some(body) = body {
            ui.add_space(4.0);
            let left = rect.left() + 2.0 + 28.0;
            let inner_width = width - 30.0 - 24.0;
            let galley = widgets::galley(
                ui,
                &body,
                theme::mono(11.0),
                theme::secondary_label(),
                17.875,
                inner_width,
            );
            let height = galley.size().y.min(256.0) + 16.0;
            let (_, _) = ui.allocate_exact_size(Vec2::new(width, height + 4.0), Sense::hover());
            let frame = Rect::from_min_size(
                Pos2::new(left, rect.bottom() + 4.0),
                Vec2::new(width - 30.0, height),
            );
            ui.painter()
                .rect_filled(frame, CornerRadius::same(6), theme::overlay(0.012));
            ui.painter().with_clip_rect(frame.shrink(1.0)).galley(
                frame.min + Vec2::new(12.0, 8.0),
                galley,
                theme::secondary_label(),
            );
        }
    }

    fn working_row(&mut self, ui: &mut Ui, started_at: Option<&str>, width: f32) {
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(width, MESSAGE_LINE + 4.0), Sense::hover());
        let elapsed = started_at
            .and_then(timeline::parse_time)
            .map(|start| (self.now - start).num_milliseconds().max(0))
            .map(|ms| timeline::format_duration((ms / 1000) * 1000))
            .map(|d| format!("Working for {d}"))
            .unwrap_or_else(|| "Working".into());
        widgets::text(
            ui,
            Pos2::new(rect.left() + 4.0, rect.top() + 2.0),
            &elapsed,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            MESSAGE_LINE,
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
    }
}

const DESTRUCTIVE: eframe::egui::Color32 = eframe::egui::Color32::from_rgb(251, 65, 74); // --destructive

/// `workEntryIconName`: tool action icons, then the tone icon.
fn work_icon(entry: &WorkEntry) -> &'static str {
    match entry.item_type.as_deref() {
        Some("command_execution") => return "terminal",
        Some("file_change") => return "square-pen",
        Some("web_search") => return "globe",
        Some("mcp_tool_call") => return "wrench",
        Some("dynamic_tool_call") => return "hammer",
        _ => {}
    }
    if entry.label.eq_ignore_ascii_case("read file") {
        return "eye";
    }
    if entry.command.is_some() || !entry.changed_files.is_empty() {
        return if entry.command.is_some() {
            "terminal"
        } else {
            "square-pen"
        };
    }
    match entry.tone.as_str() {
        "error" => "circle-alert",
        "thinking" => "brain",
        "info" => "check",
        "tool" => "wrench",
        _ => "zap",
    }
}

/// `buildToolCallExpandedBody`: blocks not already shown in the label.
fn expanded_body(entry: &WorkEntry, label: &str) -> Option<String> {
    let mut seen = vec![label.trim().to_owned()];
    let mut blocks: Vec<String> = Vec::new();
    if let Some(question) = &entry.question {
        for (index, text) in question.questions.iter().enumerate() {
            blocks.push(text.clone());
            if let Some(answer) = question.answers.get(index) {
                blocks.push(format!("→ {answer}"));
            }
        }
        return (!blocks.is_empty()).then(|| blocks.join("\n"));
    }
    let mut add = |value: Option<&str>| {
        let Some(text) = value.map(str::trim).filter(|t| !t.is_empty()) else {
            return;
        };
        if !seen.iter().any(|s| s == text) {
            seen.push(text.to_owned());
            blocks.push(text.to_owned());
        }
    };
    add(entry.command.as_deref());
    add(entry.detail.as_deref());
    let files: Vec<&str> = entry.changed_files.iter().map(String::as_str).collect();
    if !files.is_empty() {
        add(Some(&files.join("\n")));
    }
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}
