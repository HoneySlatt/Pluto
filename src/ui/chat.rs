//! Main workspace: header (`ChatHeader.tsx`), the messages timeline
//! (`MessagesTimeline.tsx`) and the empty/draft hero (`DraftHeroHeadline.tsx`).
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::model::{History, Project};
use crate::theme::{self, Weight, font};
use crate::timeline::Message;
use crate::timeline::{self, Row, WorkEntry};
use crate::ui::markdown::Markdown;
use crate::ui::thread_find::{self, FindPaint};
use crate::ui::{attachments, project_icon, timeline_view, widgets};

const MESSAGE_LINE: f32 = 22.75;
/// The load-earlier button: 12px type with `py-1.5`.
const HISTORY_BUTTON: f32 = 28.0;

/// A searchable row's id (`data-timeline-row-id`): a message's or a plan's.
fn row_entry(row: &Row) -> Option<&str> {
    match row {
        Row::User(message) | Row::Assistant { message, .. } => Some(&message.id),
        Row::Plan(plan) => Some(&plan.id),
        _ => None,
    }
}

/// The row id of a find entry: plans are found by their turn item.
fn find_row_id(detail: &crate::model::Detail, entry_id: &str) -> String {
    detail
        .turn_items
        .iter()
        .find(|item| item.id == entry_id && item.kind == "proposed_plan")
        .and_then(|item| item.str("planId"))
        .unwrap_or(entry_id)
        .to_owned()
}

/// The find entry of a row (`row_id`): a plan's newest turn item.
fn find_entry_id(detail: &crate::model::Detail, row: &Row, row_id: &str) -> String {
    if !matches!(row, Row::Plan(_)) {
        return row_id.to_owned();
    }
    detail
        .turn_items
        .iter()
        .rev()
        .find(|item| item.kind == "proposed_plan" && item.str("planId") == Some(row_id))
        .map_or_else(|| row_id.to_owned(), |item| item.id.clone())
}

/// The history control's height (`pb-2` below, `gap-1.5` before an error
/// line); none when there is nothing to load or report.
fn history_control_height(history: &History) -> f32 {
    let button = if history.has_more {
        HISTORY_BUTTON
    } else {
        0.0
    };
    let error = if history.error.is_some() { 16.0 } else { 0.0 };
    let gap = if button > 0.0 && error > 0.0 {
        6.0
    } else {
        0.0
    };
    if button + error > 0.0 {
        button + gap + error + 8.0
    } else {
        0.0
    }
}

/// A header split button: where it starts, and which half was clicked.
struct Split {
    left: f32,
    main: bool,
    menu: bool,
    chevron: Rect,
}

impl App {
    pub(crate) fn main_area(&mut self, ui: &mut Ui, rect: Rect) {
        ui.painter().rect_filled(rect, 0.0, theme::background());
        let header = Rect::from_min_size(rect.min, Vec2::new(rect.width(), theme::TOPBAR_HEIGHT));
        let body = Rect::from_min_max(Pos2::new(rect.left(), header.bottom()), rect.max);
        let thread = self.current_thread();
        let draft_project = self.draft_project();
        // Pluto stock's glow, dimmer under a thread.
        let glow = if thread.is_some() { 0.4 } else { 1.0 };
        crate::ui::backdrop::paint_glow(ui.painter(), rect, glow);
        self.header(
            ui,
            header,
            thread.as_ref().map(|t| t.title.as_str()),
            draft_project.as_ref(),
        );

        // `--chat-max-width` from the Chat width setting.
        let max_width = match self.client_setting("chatWidth").as_str() {
            Some("wide") => 1152.0, // 72rem
            Some("full") => f32::INFINITY,
            _ => theme::CHAT_WIDTH,
        };
        let column = max_width.min(body.width() - 2.0 * theme::WORKSPACE_GUTTER);
        let column_left = (body.center().x - column / 2.0).round();
        if thread.is_some() {
            let mode = self.composer_mode(false);
            let composer_top = self.composer_layout_top(ui, body, column, &mode);
            // The timeline scrolls under the composer, padded so its end clears it.
            let overlay = self.composer_overlay(ui, body, column, &mode);
            // The timeline keeps 26px gutters (the composer 20px) on narrow columns.
            let timeline_width = max_width.min(body.width() - 52.0);
            let timeline_left = (body.center().x - timeline_width / 2.0).round();
            self.timeline(ui, body, timeline_left, timeline_width, overlay);
            self.cite_selection(ui, body);
            ui.painter().rect_filled(
                Rect::from_min_max(
                    Pos2::new(column_left, composer_top + 22.0),
                    Pos2::new(column_left + column, body.bottom()),
                ),
                0.0,
                theme::background(),
            );
            // The provider owns a native subagent's conversation: a bar
            // takes the composer's place (`ProviderSubagentBar`).
            match self
                .current_thread()
                .filter(|t| t.is_provider_native_subagent())
            {
                Some(subagent) => {
                    // At the composer's foot: 20 + 32 below it, like the card.
                    let top = body.bottom() - 20.0 - 32.0 - 48.0;
                    self.provider_subagent_bar(ui, Pos2::new(column_left, top), column, &subagent)
                }
                None => self.composer(
                    ui,
                    Pos2::new(column_left, composer_top),
                    column,
                    false,
                    &mode,
                ),
            }
            self.thread_find_bar(ui, body);
            self.thread_error_banner(ui, body);
        } else if let Some(project) = draft_project.clone().filter(|p| self.is_chat_project(p)) {
            // Pluto: a chat project's page (`chat_projects`).
            self.project_page(ui, body, &project, column_left, column);
        } else if let Some(project) = draft_project {
            // Hero + composer, vertically centered as one group.
            // Centered as one group; without the checkout strip it is 32px
            // shorter.
            let strip = if self.workspace_is_repo() { 0.0 } else { 16.0 };
            let composer_top = (body.center().y - 98.0 + strip).round();
            crate::ui::backdrop::paint_sky(
                ui.painter(),
                body,
                Pos2::new(column_left + column / 2.0, composer_top - 40.0),
            );
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

    /// Where a workspace titlebar's content starts: past the floating
    /// sidebar trigger while the sidebar is hidden
    /// (`--workspace-titlebar-content-left`, 13 + 28 + 12px).
    pub(crate) fn titlebar_left(&self, rect: Rect) -> f32 {
        if self.sidebar_docked() {
            rect.left() + theme::WORKSPACE_GUTTER
        } else {
            rect.left() + 53.0
        }
    }

    fn header(&mut self, ui: &mut Ui, rect: Rect, title: Option<&str>, draft: Option<&Project>) {
        // Pluto: a chat's header is its title alone (`crate::ui::chat_mode`).
        if self.chat_context(self.current_thread().is_none()) {
            self.chat_header(ui, rect, title.unwrap_or("New chat"));
            return;
        }
        let project = self
            .current_thread()
            .and_then(|t| self.state.project(&t.project_id).cloned())
            .or_else(|| draft.cloned());
        let mut x = self.titlebar_left(rect);
        let title = title.unwrap_or("New thread");
        // Right-side controls, laid out from the right edge. With the right
        // panel open, its header owns the layout toggles. These buttons
        // collapse to icons then, and below `@3xl` (48rem) of header (their
        // labels become screen-reader only upstream).
        let panel_header = self.panel_open() && self.current_thread().is_some();
        let compact = panel_header || rect.width() < 768.0;
        let mut right = if panel_header {
            rect.right() - theme::WORKSPACE_GUTTER
        } else {
            self.panel_controls(ui, rect.right());
            let details = if self.thread_details_available() {
                32.0
            } else {
                0.0
            };
            rect.right() - 13.0 - 64.0 + 4.0 - 12.0 - details
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
            } else {
                // `GitActionsControl`: the suggested action and the menu.
                let (action, icon) = self.git_quick_action();
                let split =
                    self.split_button(ui, right, &action.label, Some(icon), "git-action", compact);
                if split.main {
                    self.run_quick_git_action();
                }
                if split.menu {
                    self.toggle_git_menu(split.chevron.right_bottom() + Vec2::new(0.0, 4.0));
                }
                right = split.left - gap;
            }
            // `OpenInPicker`: the workspace in the preferred editor, or another.
            let preferred = self.preferred_editor();
            let icon = preferred.and_then(crate::ui::open_in::editor_icon);
            let split = self.split_button(
                ui,
                right,
                "Open",
                icon.map(|(icon, _)| icon),
                "open-in",
                compact,
            );
            let cwd = self.workspace_cwd();
            if split.main
                && let (Some(cwd), Some(editor)) = (&cwd, preferred)
            {
                self.open_in_editor(cwd, editor);
            }
            if split.menu {
                self.toggle_open_in_menu(cwd, split.chevron.right_bottom() + Vec2::new(0.0, 4.0));
            }
            right = split.left - gap;
            // `ProjectScriptsControl`: the actions, or "Add action".
            let target = project
                .as_ref()
                .map(|p| p.id.clone())
                .or_else(|| self.draft_project().map(|p| p.id));
            if let Some(id) = target {
                right = self.project_actions_control(ui, right, &id, compact);
            }
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
        icon: Option<&str>,
        id: &str,
        compact: bool,
    ) -> Split {
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
        // Brand marks take the text color, generic icons the muted one.
        let tint = match icon {
            Some("zed" | "provider-cursor" | "provider-antigravity") => theme::foreground(),
            _ => theme::muted_foreground(),
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
            icon.map(|icon| (icon, tint)),
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
        Split {
            left: main.left(),
            main: clicked,
            menu: menu.clicked(),
            chevron,
        }
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
        // `NoProjectsHero`: without a Scratch folder there is no starting
        // without a project.
        let scratch = self.scratch_root().is_some();
        let sub = if scratch {
            "Add a project, or start without one."
        } else {
            "Add a project to start your first thread."
        };
        let width = widgets::text_width(ui, sub, font(14.0, Weight::Regular));
        widgets::text(
            ui,
            Pos2::new(center.x - width / 2.0, center.y - 42.0),
            sub,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            20.0,
        );
        // `Button size="sm"`: Add project, and Start without a project.
        let label_font = font(14.0, Weight::Medium);
        let button_width = |label: &str| {
            (12.0 + 16.0 + 6.0 + widgets::text_width(ui, label, label_font.clone()) + 12.0).ceil()
        };
        let add = "Add project";
        let start = "Start without a project";
        let widths = [
            button_width(add),
            if scratch { button_width(start) } else { 0.0 },
        ];
        let total = widths[0] + if scratch { 8.0 + widths[1] } else { 0.0 };
        let mut x = (center.x - total / 2.0).round();
        let top = center.y - 42.0 + 20.0 + 24.0;
        let mut clicked = [false; 2];
        for (index, (label, icon)) in [(add, "plus"), (start, "message-square-dashed")]
            .into_iter()
            .enumerate()
            .take(if scratch { 2 } else { 1 })
        {
            let rect = Rect::from_min_size(Pos2::new(x, top), Vec2::new(widths[index], 32.0));
            let response = ui
                .interact(rect, ui.id().with(("empty-state", index)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let primary = index == 0;
            let (fill, color) = if primary {
                let fill = if response.hovered() {
                    theme::primary_hover()
                } else {
                    theme::primary()
                };
                (fill, theme::primary_foreground())
            } else {
                let fill = if response.hovered() {
                    theme::overlay(0.06)
                } else {
                    theme::control_surface()
                };
                (fill, theme::foreground())
            };
            ui.painter().rect(
                rect,
                CornerRadius::same(8),
                fill,
                if primary {
                    egui::Stroke::NONE
                } else {
                    egui::Stroke::new(1.0, theme::input())
                },
                egui::StrokeKind::Inside,
            );
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(
                    Pos2::new(rect.left() + 12.0, rect.top() + 8.0),
                    Vec2::splat(16.0),
                ),
                16.0,
                color,
            );
            widgets::text(
                ui,
                Pos2::new(rect.left() + 12.0 + 16.0 + 6.0, rect.top() + 6.0),
                label,
                label_font.clone(),
                color,
                20.0,
            );
            clicked[index] = response.clicked();
            x = rect.right() + 8.0;
        }
        if clicked[0] {
            self.toggle_palette(crate::ui::palette::View::AddProject);
        }
        if clicked[1] {
            self.start_scratch_thread();
        }
    }

    /// Pluto: the pixel logo 22px above the hero's headline at `top`, its
    /// moon stepping.
    fn hero_logo(&self, ui: &Ui, left: f32, width: f32, top: f32) {
        let size = Vec2::new(72.0, 60.0);
        let rect = Rect::from_min_size(
            Pos2::new(left + (width - size.x) / 2.0, top - 22.0 - size.y),
            size,
        );
        let moon = crate::ui::pixel_logo::moon_phase(ui);
        crate::ui::pixel_logo::paint(ui, rect, moon);
    }

    fn draft_hero(
        &mut self,
        ui: &mut Ui,
        project: &Project,
        left: f32,
        width: f32,
        composer_top: f32,
    ) {
        // Pluto: a new chat asks how it can help, with no project picker; a
        // temporary one says what it keeps.
        if self.is_chats(project) {
            let temporary = self.is_temporary_chats(project);
            let text = if temporary {
                "Temporary chat"
            } else {
                "How can I help?"
            };
            let hero = font(30.0, Weight::Regular);
            let text_width = widgets::text_width(ui, text, hero.clone());
            let top = composer_top - if temporary { 100.0 } else { 72.0 };
            self.hero_logo(ui, left, width, top);
            widgets::text_in(
                ui,
                Rect::from_min_size(
                    Pos2::new(left + (width - text_width) / 2.0, top),
                    Vec2::new(text_width + 1.0, 40.0),
                ),
                text,
                hero,
                theme::foreground(),
            );
            if temporary {
                let note = "This chat won't appear in your history or use your memory, \
                            and it is deleted when you leave it.";
                let small = font(14.0, Weight::Regular);
                let note_width = widgets::text_width(ui, note, small.clone());
                widgets::text_in(
                    ui,
                    Rect::from_min_size(
                        Pos2::new(left + (width - note_width) / 2.0, top + 44.0),
                        Vec2::new(note_width + 1.0, 20.0),
                    ),
                    note,
                    small,
                    theme::muted_foreground(),
                );
            }
            return;
        }
        let offers_scratch = self.scratch_root().is_some();
        let scratch_draft = self.is_scratch(project);
        let regular = font(30.0, Weight::Regular);
        let medium = font(30.0, Weight::Medium);
        let mut job = egui::text::LayoutJob::default();
        let mut format = widgets::format(regular, theme::foreground(), 36.0);
        format.extra_letter_spacing = -0.75;
        // Without a project the question loses its picker, which moves to the
        // line below.
        if scratch_draft {
            job.append("What should we work on?", 0.0, format);
        } else {
            job.append("What should we build in ", 0.0, format.clone());
            let mut name = widgets::format(medium, theme::foreground(), 36.0);
            name.extra_letter_spacing = -0.75;
            job.append(&project.title, 0.0, name);
            job.append("?", 0.0, format);
        }
        let galley = widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job)));
        let top = composer_top - 100.0;
        self.hero_logo(ui, left, width, top);
        let pos = Pos2::new(left + (width - galley.size().x) / 2.0, top);
        ui.painter().galley(pos, galley, theme::foreground());
        // The project picker (`InlineButton tone="picker"`): in the question,
        // or alone on the second line for a draft without a project.
        let trigger = if scratch_draft {
            let label_width = widgets::text_width(ui, "No project", font(14.0, Weight::Medium));
            let rect = Rect::from_min_size(
                Pos2::new(left + (width - label_width) / 2.0, top + 46.0),
                Vec2::new(label_width, 20.0),
            );
            widgets::text_in(
                ui,
                rect,
                "No project",
                font(14.0, Weight::Medium),
                theme::foreground(),
            );
            (rect, rect.bottom() - 1.0)
        } else {
            let prefix =
                widgets::text_width(ui, "What should we build in ", font(30.0, Weight::Regular))
                    - 0.75 * 24.0;
            let name_width = widgets::text_width(ui, &project.title, font(30.0, Weight::Medium))
                - 0.75 * project.title.chars().count() as f32;
            (
                Rect::from_min_max(
                    Pos2::new(pos.x + prefix, top),
                    Pos2::new(pos.x + prefix + name_width, top + 36.0),
                ),
                top + 33.0,
            )
        };
        let (trigger, underline_y) = trigger;
        let picker = ui.interact(
            trigger,
            ui.id().with("draft-project-trigger"),
            Sense::click(),
        );
        if picker.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let open = self.hero_project_menu.is_some();
        // Dotted at rest; solid in the text color while hovered or open.
        if picker.hovered() || open {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    Pos2::new(trigger.left(), underline_y),
                    Vec2::new(trigger.width(), 1.5),
                ),
                0.0,
                theme::foreground(),
            );
        } else {
            let mut dot_x = trigger.left();
            while dot_x < trigger.right() {
                ui.painter().rect_filled(
                    Rect::from_min_size(Pos2::new(dot_x, underline_y), Vec2::new(1.5, 1.5)),
                    0.0,
                    theme::alpha(theme::foreground(), 0.4),
                );
                dot_x += 4.0;
            }
        }
        if picker.clicked() {
            self.hero_project_menu = if open {
                None
            } else {
                Some(
                    crate::ui::context_menu::State::dropdown(Pos2::new(
                        trigger.center().x,
                        trigger.bottom() + 4.0,
                    ))
                    .centered(),
                )
            };
        }
        // "or start without a project", where the environment offers it.
        if offers_scratch && !scratch_draft {
            let sub = "or start without a project";
            let sub_width = widgets::text_width(ui, sub, font(14.0, Weight::Medium));
            let sub_rect = Rect::from_min_size(
                Pos2::new(left + (width - sub_width) / 2.0, top + 46.0),
                Vec2::new(sub_width, 20.0),
            );
            let bindings = self.state.config["keybindings"].clone();
            let shortcut = crate::ui::shortcuts::effective_shortcut_label(
                &bindings,
                "chat.newWithoutProject",
                &[("isDesktop", true)],
            );
            let mut response = ui.interact(
                sub_rect,
                ui.id().with("start-without-project"),
                Sense::click(),
            );
            if let Some(shortcut) = shortcut {
                response = response.on_hover_text(shortcut);
            }
            let color = if response.hovered() {
                theme::foreground()
            } else {
                theme::muted_foreground()
            };
            widgets::text_in(ui, sub_rect, sub, font(14.0, Weight::Medium), color);
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                // `hover:underline underline-offset-2`.
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        Pos2::new(sub_rect.left(), sub_rect.bottom() - 2.0),
                        Vec2::new(sub_width, 1.0),
                    ),
                    0.0,
                    color,
                );
            }
            if response.clicked() {
                self.open_scratch(crate::ui::scratch::Then::Retarget);
            }
        }
        self.draft_project_menu(ui, project);
    }

    /// The draft hero's project picker (`DraftHeroHeadline`): "No project"
    /// where the environment offers it, the logical projects (the draft's
    /// first), then Add project. A choice retargets the open draft.
    fn draft_project_menu(&mut self, ui: &mut Ui, project: &Project) {
        use crate::ui::context_menu::{self, Item};
        if self.hero_project_menu.is_none() {
            return;
        }
        // `buildSidebarProjectPickerEntries`: each group's target project,
        // the draft's own when it is a member, and that group first. The
        // Scratch project shows once, as "No project".
        let mut entries: Vec<(String, String, Project, bool)> = self
            .sorted_project_groups()
            .into_iter()
            .map(|group| {
                let own = group.members.iter().find(|(_, p)| p.id == project.id);
                let target = own
                    .map(|(_, p)| p.clone())
                    .unwrap_or_else(|| group.representative.clone());
                (group.key, group.label, target, own.is_some())
            })
            .filter(|(_, _, target, _)| !self.is_scratch(target))
            .collect();
        if let Some(index) = entries.iter().position(|e| e.3)
            && index > 0
        {
            let preferred = entries.remove(index);
            entries.insert(0, preferred);
        }
        let mut items = Vec::new();
        if self.scratch_root().is_some() {
            let mut item = Item::new("no-project", "No project").icon("message-square-dashed");
            item.selected = self.is_scratch(project);
            items.push(item);
        }
        items.extend(entries.iter().map(|(key, label, target, own)| {
            let mut item = Item::new(key.clone(), label.clone()).project(target.clone());
            item.selected = *own;
            item
        }));
        items.push(
            Item::new("add-project", "Add project")
                .icon("folder-plus")
                .separated(),
        );
        let clicked = context_menu::show(ui, &mut self.hero_project_menu, &items);
        match clicked.as_deref() {
            Some("add-project") => {
                self.toggle_palette(crate::ui::palette::View::AddProject);
            }
            Some("no-project") => {
                if !self.is_scratch(project) {
                    self.open_scratch(crate::ui::scratch::Then::Retarget);
                }
            }
            Some(key) => {
                if let Some((_, _, target, own)) = entries.iter().find(|e| e.0 == key)
                    && !own
                {
                    self.draft_project_id = Some(target.id.clone());
                }
            }
            None => {}
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
        // Until its projection arrives the thread shows no rows.
        let detail = self.current_detail().unwrap_or_default();
        let revision = self.state.revision;
        let images = if self.server_is_local() {
            crate::generated_images::images(&detail, thread.is_running(), &crate::repaint(ui.ctx()))
        } else {
            Vec::new()
        };
        let mut rows =
            self.timeline_view
                .rows(&thread, &detail, &self.expansion, revision, &images);
        rows = self.with_worktree_setup(rows, &thread, &detail);
        // A revealed citation's message: its folded turn opens, and the
        // timeline stops following its end until it is in view.
        let mut reveal = self.citation_scroll_target(&thread.id);
        if reveal.is_some() && !self.locate_citation_source(&detail) {
            reveal = None;
            // Waiting for the source: the view stays where it is.
            self.keep_scroll = self.cite_target_pending();
        }
        if let Some(message_id) = &reveal {
            let shown = rows
                .iter()
                .any(|r| matches!(r, Row::Assistant { message, .. } if &message.id == message_id));
            let fold = timeline::fold_containing(&thread, &detail, message_id);
            if !shown && let Some(fold) = fold {
                self.expansion.turns.insert(fold);
                rows =
                    self.timeline_view
                        .rows(&thread, &detail, &self.expansion, revision, &images);
            }
            self.keep_scroll = true;
        }
        // Find in thread: the active match's turn opens, or the older turns
        // holding it are paged in (`useThreadFindNavigation`).
        let find = self.find.selection();
        let mut find_key = None;
        if let Some(selection) = &find {
            let row_id = find_row_id(&detail, &selection.matched.entry_id);
            let shown = |rows: &[Row]| rows.iter().any(|r| row_entry(r) == Some(row_id.as_str()));
            if !shown(&rows) {
                if let Some(fold) = timeline::fold_containing(&thread, &detail, &row_id) {
                    self.expansion.turns.insert(fold);
                    rows = self.timeline_view.rows(
                        &thread,
                        &detail,
                        &self.expansion,
                        revision,
                        &images,
                    );
                } else if timeline::message_role(&detail, &row_id).is_none()
                    && row_id == selection.matched.entry_id
                    && detail.history.has_more
                    && !detail.history.loading
                    && detail.history.error.is_none()
                {
                    let cursor = detail.history.cursor.clone().unwrap_or("first".into());
                    if self.find.page_for(selection.navigation_id, &cursor) {
                        self.load_earlier_history_through(Some(&selection.matched.entry_id));
                    }
                }
            }
            find_key = rows
                .iter()
                .find(|r| row_entry(r) == Some(row_id.as_str()))
                .map(timeline_view::row_key);
            self.find_paint = Some(FindPaint {
                query: selection.query.clone(),
                row_id,
                occurrence: selection.matched.occurrence,
                active_rect: None,
            });
        } else {
            self.find_paint = None;
        }
        // Pluto: a chat's versions are forks, which it does not announce.
        let fork_card = |row: &Row| match row {
            Row::Work { entry, .. } => entry.item_type.as_deref() == Some("fork"),
            Row::Event { entries, .. } => {
                entries.first().and_then(|e| e.item_type.as_deref()) == Some("fork")
            }
            _ => false,
        };
        if self.is_chat_thread(&thread) && rows.iter().any(fork_card) {
            rows =
                std::sync::Arc::new(rows.iter().filter(|row| !fork_card(row)).cloned().collect());
        }
        // Toggling a fold keeps the clicked row in place instead of following the end.
        let keep_scroll = std::mem::take(&mut self.keep_scroll);
        let (select_all, selecting) = {
            let plugin = ui.ctx().plugin::<widgets::CopyCleanup>();
            let plugin = plugin.lock();
            (plugin.select_all, plugin.has_range || plugin.select_all)
        };
        let mut view = std::mem::take(&mut self.timeline_view);
        view.open(&thread.id, ui.input(|i| i.time));
        // The open search does not follow the live end.
        if self.find.is_open() {
            view.follow = false;
        }
        if !selecting {
            view.pinned.clear();
        }
        let viewport_height = if view.viewport > 0.0 {
            view.viewport
        } else {
            rect.height()
        };
        let keys: Vec<egui::Id> = rows.iter().map(timeline_view::row_key).collect();
        // Pluto: the first reply of each turn opens with the logo and the
        // thread's model.
        let reply_header = self.model_label(&thread.model_selection).0;
        let mut turn_answered = false;
        let turn_openers: std::collections::HashSet<&str> = rows
            .iter()
            .filter_map(|row| match row {
                Row::User(_) => {
                    turn_answered = false;
                    None
                }
                Row::Assistant { message, .. } if !turn_answered => {
                    turn_answered = true;
                    Some(message.id.as_str())
                }
                _ => None,
            })
            .collect();
        let guesses: Vec<f32> = rows
            .iter()
            .zip(&keys)
            .map(|(row, key)| view.height(*key, row, column))
            .collect();
        // A subagent's thread opens on "Subagent of" its parent.
        let subagent_header = if thread.is_subagent() {
            8.0 + 42.0
        } else {
            0.0
        };
        let history_height = history_control_height(&detail.history) + subagent_header;
        let status_line = self.thread_status_line(&thread);
        let footer = if status_line.is_some() { 24.0 } else { 0.0 };
        let content_height =
            24.0 + history_height + guesses.iter().sum::<f32>() + footer + bottom_padding;
        // Older turns loaded above keep what is on screen in place
        // (`maintainVisibleContentPosition`).
        if !view.follow {
            let first = view
                .first
                .and_then(|first| keys.iter().position(|key| *key == first));
            if let Some(index) = first {
                view.offset += guesses[..index].iter().sum::<f32>();
            }
            view.offset += history_height - view.header;
        }
        view.first = keys.first().copied();
        view.header = history_height;
        // A view that follows starts from the end as it is now: the rows
        // may have grown, or arrived, since the last pass.
        let mut offset = if view.follow && !keep_scroll {
            (content_height - viewport_height).max(0.0)
        } else {
            view.offset
        };
        let mut scrolled = false;
        // The wheel moves the view in this pass; egui would apply it after
        // drawing, a frame late.
        let mut wheel = if ui.rect_contains_pointer(rect) && ui.ctx().dragged_id().is_none() {
            ui.input(|i| i.smooth_scroll_delta.y)
        } else {
            0.0
        };
        // egui spreads a wheel step over the next frames. What is left of a
        // step taken before this thread opened belongs to the previous
        // thread: here it would stop the view following the end. Both
        // durations are rounded to f32 alike, so a step taken in the frame
        // the thread opened counts as this thread's.
        let before_open =
            ui.input(|i| i.time_since_last_scroll() > (i.time - view.opened_at) as f32);
        if wheel != 0.0 && before_open {
            ui.input_mut(|i| i.smooth_scroll_delta.y = 0.0);
            wheel = 0.0;
        }
        // `pageScrollController`: PageUp and PageDown page the timeline (from
        // the composer too), a page less 36px; Home and End go to its ends
        // when nothing editable has the focus.
        let composer_focused = self
            .cite
            .editor_id
            .is_some_and(|id| ui.memory(|m| m.has_focus(id)));
        let free = ui.memory(|m| m.focused().is_none());
        let page = (viewport_height - 36.0).max(40.0);
        let key = |key| ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
        let paging = if (free || composer_focused) && !self.palette.open {
            if key(egui::Key::PageUp) {
                Some(-page)
            } else if key(egui::Key::PageDown) {
                Some(page)
            } else if free && key(egui::Key::Home) {
                Some(f32::NEG_INFINITY)
            } else if free && key(egui::Key::End) {
                Some(f32::INFINITY)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(delta) = paging {
            let max = (content_height - viewport_height).max(0.0);
            offset = (offset + delta).clamp(0.0, max);
            scrolled = true;
        } else if wheel != 0.0 {
            ui.input_mut(|i| i.smooth_scroll_delta.y = 0.0);
            let max = (content_height - viewport_height).max(0.0);
            offset = (offset - wheel).clamp(0.0, max);
            scrolled = true;
        }
        let mut drawn = Vec::new();
        let mut used = offset;
        // Height changes of rows above the viewport, and whether a row cut
        // by its top edge changed (what is on screen moved).
        let mut shift = 0.0;
        let mut cut_changed = false;
        let mut load_earlier = false;
        let mut first_entry = None;
        let mut find_row_rect = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        let output = egui::ScrollArea::vertical()
            .id_salt(("timeline", &thread.id))
            .auto_shrink([false, false])
            .vertical_scroll_offset(offset)
            .show_viewport(&mut child, |ui, viewport| {
                used = viewport.top();
                let origin = ui.max_rect().top();
                let overscan = (viewport.height() * 0.5).max(240.0);
                ui.set_width(rect.width());
                ui.add_space(24.0);
                let inner = Rect::from_min_size(
                    Pos2::new(column_left, ui.cursor().top()),
                    Vec2::new(column, f32::INFINITY),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    if history_control_height(&detail.history) > 0.0
                        && self.history_control(ui, &detail.history, column)
                    {
                        load_earlier = true;
                    }
                    if subagent_header > 0.0 {
                        ui.add_space(8.0);
                        self.subagent_header(ui, &thread, column);
                    }
                    for ((row, key), guess) in rows.iter().zip(&keys).zip(guesses.iter().copied()) {
                        let top = ui.cursor().top() - origin;
                        let near = top + guess >= viewport.top() - overscan
                            && top <= viewport.bottom() + overscan;
                        let revealing = matches!(
                            (row, &reveal),
                            (Row::Assistant { message, .. }, Some(id)) if &message.id == id
                        ) || find_key == Some(*key);
                        if first_entry.is_none()
                            && top + guess > viewport.top()
                            && let Some(entry) = row_entry(row)
                        {
                            first_entry = Some(find_entry_id(&detail, row, entry));
                        }
                        if !(near || select_all || revealing || view.pinned.contains(key)) {
                            ui.add_space(guess);
                            continue;
                        }
                        let before = ui.cursor().top();
                        let header = match row {
                            Row::Assistant { message, .. }
                                if turn_openers.contains(message.id.as_str()) =>
                            {
                                Some(reply_header.as_str())
                            }
                            _ => None,
                        };
                        self.row(ui, row, column, header);
                        let height = ui.cursor().top() - before;
                        if find_key == Some(*key) {
                            find_row_rect = Some(Rect::from_min_max(
                                Pos2::new(column_left, before),
                                Pos2::new(column_left + column, before + height),
                            ));
                        }
                        view.measured(*key, height);
                        drawn.push(*key);
                        if (height - guess).abs() > 0.5 && top < viewport.top() {
                            // Rows below stay where this pass expected them;
                            // the next pass moves the offset instead.
                            ui.add_space(guess - height);
                            shift += height - guess;
                            cut_changed |= top + guess > viewport.top();
                        }
                    }
                    if let Some(line) = &status_line {
                        self.paint_thread_status_line(ui, &thread.id, line, column);
                    }
                });
                ui.add_space(bottom_padding);
            });
        if selecting {
            view.pinned.extend(drawn);
        }
        view.viewport = output.inner_rect.height();
        let content = output.content_size.y;
        let max = (content + shift - view.viewport).max(0.0);
        let mut next = (output.state.offset.y + shift).clamp(0.0, max);
        // A drag on the bar, a reveal or the wheel decide whether the view
        // still follows the end.
        view.scrolled =
            scrolled || (used - offset).abs() > 0.5 || (output.state.offset.y - used).abs() > 0.5;
        let moved = view.scrolled || keep_scroll;
        if moved {
            view.follow = next >= max - 1.0;
        } else if view.follow {
            next = max;
        }
        // Show the corrected layout instead of this pass when what is on
        // screen moved: the end left the bottom edge, or a row cut by the
        // top edge changed height.
        view.first_entry = first_entry;
        // The active match scrolls just enough into view, above the
        // composer, once per navigation (a few passes while rows measure).
        if let Some(selection) = &find
            && self.find.revealed != Some(selection.navigation_id)
        {
            let target = self
                .find_paint
                .as_ref()
                .and_then(|p| p.active_rect)
                .or(find_row_rect);
            if let Some(target) = target {
                let viewport = output.inner_rect;
                let margin = thread_find::VIEW_MARGIN
                    .min(((viewport.height() - bottom_padding - target.height()) / 2.0).max(0.0));
                let top = viewport.top() + margin;
                let bottom = viewport.bottom() - margin - bottom_padding;
                let delta = if target.top() < top {
                    target.top() - top
                } else if target.bottom() > bottom {
                    target.bottom() - bottom
                } else {
                    0.0
                };
                view.follow = false;
                self.find_attempts += 1;
                if delta.abs() < 1.0 || self.find_attempts > 6 {
                    self.find.revealed = Some(selection.navigation_id);
                    self.find_attempts = 0;
                } else {
                    next = (next + delta).clamp(0.0, max);
                    ui.ctx().request_repaint();
                }
            }
        }
        // `chat-scroll-to-bottom`: away from the end for 150ms, a "Scroll to
        // end" pill above the composer brings the view back and follows.
        let now = ui.input(|i| i.time);
        if view.follow || max - next < 1.0 {
            view.away_since = None;
        } else if view.away_since.is_none() {
            view.away_since = Some(now);
        }
        let shown =
            view.away_since.is_some_and(|since| now - since >= 0.15) && !self.find.is_open();
        if view.away_since.is_some() && !shown {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(160));
        }
        if shown && scroll_to_end_pill(ui, rect, bottom_padding) {
            view.follow = true;
            view.away_since = None;
            next = max;
            ui.ctx().request_repaint();
        }
        // The minimap in the left gutter: each prompt's row, and a click
        // scrolls to it.
        {
            let mut items = Vec::new();
            let mut top = 24.0 + history_height;
            for (row, key) in rows.iter().zip(&keys) {
                let height = view.height(*key, row, column);
                match row {
                    Row::User(message) => items.push(crate::ui::minimap::Item {
                        top,
                        height,
                        user: crate::ui::minimap::preview_text(&message.text),
                        assistant: None,
                    }),
                    Row::Assistant { message, .. } => {
                        if let Some(last) = items.last_mut() {
                            last.assistant = crate::ui::minimap::preview_text(&message.text);
                        }
                    }
                    _ => {}
                }
                top += height;
            }
            let gutter = column_left - rect.left();
            if let Some(target) = crate::ui::minimap::show(ui, rect, gutter, &items, next) {
                view.follow = false;
                next = target.min(max);
                ui.ctx().request_repaint();
            }
        }
        let misaligned = view.follow && ((content - view.viewport).max(0.0) - used).abs() > 0.5;
        if misaligned || cut_changed {
            ui.ctx().request_discard("timeline row heights");
        }
        view.offset = next;
        self.timeline_view = view;
        if load_earlier {
            self.load_earlier_history();
        }
    }

    /// `TimelineHistoryControl`: loads the turns before the bounded window,
    /// and says why the last load failed. True when clicked.
    fn history_control(&mut self, ui: &mut Ui, history: &History, width: f32) -> bool {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(width, history_control_height(history)),
            Sense::hover(),
        );
        let label = font(12.0, Weight::Regular);
        let mut clicked = false;
        let mut top = rect.top();
        if history.has_more {
            let button = Rect::from_min_size(rect.min, Vec2::new(width, HISTORY_BUTTON));
            let sense = if history.loading {
                Sense::hover()
            } else {
                Sense::click()
            };
            let response = ui.interact(button, ui.id().with("load-earlier-turns"), sense);
            let color = if response.hovered() && !history.loading {
                theme::foreground()
            } else {
                theme::alpha(theme::muted_foreground(), 0.6)
            };
            let text = if history.loading {
                "Loading earlier turns…"
            } else {
                "Load earlier turns"
            };
            let text_width = widgets::text_width(ui, text, label.clone());
            let at =
                Rect::from_center_size(button.center(), Vec2::new(text_width, button.height()));
            widgets::text_in(ui, at, text, label.clone(), color);
            clicked = response.clicked();
            top = button.bottom() + 6.0;
        }
        if let Some(error) = &history.error {
            let line = Rect::from_min_size(Pos2::new(rect.left(), top), Vec2::new(width, 16.0));
            let text_width = widgets::text_width(ui, error, label.clone()).min(width);
            let at = Rect::from_center_size(line.center(), Vec2::new(text_width, 16.0));
            widgets::text_in(ui, at, error, label, theme::muted_foreground());
        }
        clicked
    }

    fn row(&mut self, ui: &mut Ui, row: &Row, width: f32, header: Option<&str>) {
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
                self.assistant_row(ui, message, header, *show_meta, changes.as_ref(), width);
                ui.add_space(if *show_meta { 16.0 } else { 8.0 });
            }
            Row::TurnFold {
                run_id,
                label,
                expanded,
                created_at,
            } => {
                if self
                    .fold_row(ui, label, *expanded, width, created_at)
                    .clicked()
                {
                    if !self.expansion.turns.remove(run_id) {
                        self.expansion.turns.insert(run_id.clone());
                    }
                    self.keep_scroll = true;
                }
                ui.add_space(6.0);
            }
            Row::Work { label, entry, .. } => {
                self.work_entry_row(ui, entry, label, width, false);
                ui.add_space(8.0);
            }
            Row::Event { id, entries } => self.event_row(ui, id, entries, width),
            Row::LiveWork {
                group_id,
                entry,
                expanded,
                active,
                thought,
                ..
            } => {
                let thought = thought.as_deref().filter(|_| !*expanded);
                if self
                    .live_work_row(ui, entry, thought, *active, width)
                    .clicked()
                {
                    self.toggle_work_group(group_id);
                }
                ui.add_space(if *expanded { 0.0 } else { 8.0 });
            }
            Row::AttemptFold {
                attempt_id,
                expanded,
            } => {
                if self.attempt_fold_row(ui, *expanded, width).clicked() {
                    self.toggle_work_group(&format!("attempt-fold:{attempt_id}"));
                }
                ui.add_space(8.0);
            }
            Row::Compaction { label, active, .. } => {
                crate::ui::live_activity::compaction_row(ui, label, *active, width);
                ui.add_space(8.0);
            }
            Row::Thinking { group_id, expanded } => {
                if self.thinking_row(ui, group_id.is_some(), width).clicked()
                    && let Some(group_id) = group_id
                {
                    self.toggle_work_group(group_id);
                }
                ui.add_space(if *expanded { 0.0 } else { 8.0 });
            }
            Row::WorkToggle {
                group_id,
                summary,
                expanded,
                kind,
                ..
            } => {
                if self
                    .work_row(ui, summary_icon(kind), summary, width, Some(group_id))
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
                ui.add_space(4.0);
            }
            Row::WorktreeSetup { snapshot, embedded } => {
                self.worktree_setup_row(ui, snapshot, *embedded, width);
            }
            Row::GeneratedImage { image } => {
                if self.generated_image_row(ui, image, width) {
                    ui.add_space(16.0);
                }
            }
            Row::Plan(plan) => {
                self.plan_row(ui, plan, width);
                ui.add_space(16.0);
            }
            Row::HtmlRender(page) => {
                self.html_render_row(ui, &page.render, width);
                ui.add_space(8.0);
            }
            Row::McpApp(app) => {
                self.mcp_app_row(ui, app, width);
                ui.add_space(8.0);
            }
        }
    }

    /// A generated image, at most 384px wide, a click opening it in the
    /// expanded preview; nothing when the server is remote or Codex saved
    /// no file for the native thread. Upstream shows nothing for these items.
    fn generated_image_row(
        &mut self,
        ui: &mut Ui,
        generated: &crate::generated_images::GeneratedImage,
        width: f32,
    ) -> bool {
        if !self.server_is_local() {
            return false;
        }
        let image = attachments::generated_image(generated, ui.ctx());
        let frame_width = width.min(384.0);
        let Some(image) = image else {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(width, frame_width * 0.75), Sense::hover());
            let failed = matches!(
                attachments::media_state(&generated.key()),
                Some(attachments::MediaState::Failed)
            );
            widgets::text_in(
                ui,
                rect,
                if failed {
                    "Image unavailable"
                } else {
                    "Loading image…"
                },
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            );
            return true;
        };
        let height = (frame_width * image.height() as f32 / image.width().max(1) as f32)
            .min(frame_width * 1.25);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let frame = Rect::from_min_size(rect.min, Vec2::new(frame_width, height));
        let radius = CornerRadius::same(8);
        let key = generated.key();
        attachments::paint_cover(ui, frame.shrink(1.0), &key, &image, CornerRadius::same(7));
        ui.painter().rect_stroke(
            frame,
            radius,
            egui::Stroke::new(1.0, theme::alpha(theme::border(), 0.8)),
            egui::StrokeKind::Inside,
        );
        // The menu copies Codex's saved file and reads its bytes from it.
        let local_file = attachments::local_file(&key);
        let source = crate::ui::media::Source {
            name: "Generated image".into(),
            reference: local_file.as_ref().map(|path| {
                crate::ui::media::file_reference(
                    &path.to_string_lossy(),
                    self.workspace_cwd().as_deref(),
                )
            }),
            key: key.clone(),
            local_file,
        };
        let response = ui
            .interact(
                frame,
                ui.id().with(("generated-image", &generated.path)),
                Sense::click(),
            )
            .on_hover_text(source.tooltip().to_owned());
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomIn);
        }
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.open_media_menu(source.clone(), at);
        }
        if response.clicked() {
            self.open_lightbox(
                vec![crate::ui::lightbox::Item {
                    name: "Generated image".into(),
                    key,
                    image,
                    source: Some(source),
                }],
                0,
            );
        }
        true
    }

    /// One image cell (`aspect-[4/3] overflow-hidden rounded-lg border
    /// border-border/80 bg-background/70`): the image cropped to fill it, or
    /// its name while it loads.
    /// `openFileInPanel`: a markdown file reference opens in the right
    /// panel when it is inside the workspace.
    pub(crate) fn open_markdown_file(&mut self, path: &str) {
        let root = self.workspace_cwd().unwrap_or_default();
        let relative =
            if let Some(rest) = path.strip_prefix(&format!("{}/", root.trim_end_matches('/'))) {
                Some(rest.to_owned())
            } else if path.starts_with('/') || path.starts_with('~') {
                None
            } else {
                Some(path.trim_start_matches("./").to_owned())
            };
        match relative {
            Some(relative) => self.open_file(relative),
            // Media outside the workspace opens in the expanded preview;
            // other host files in the preferred editor.
            None if crate::ui::media::is_image_path(path) && path.starts_with('/') => {
                self.open_media_file(path);
            }
            None => match self.preferred_editor() {
                Some(editor) => self.open_in_editor(path, editor),
                None => self.notify(
                    crate::ui::toasts::Kind::Warning,
                    "No installed editors found",
                ),
            },
        }
    }

    /// Returns whether the loaded image was clicked (`cursor-zoom-in`).
    fn attachment_cell(&mut self, ui: &mut Ui, rect: Rect, image: &serde_json::Value) -> bool {
        let radius = CornerRadius::same(8);
        let mut clicked = false;
        ui.painter()
            .rect_filled(rect, radius, theme::alpha(theme::background(), 0.7));
        let id = image["id"].as_str().unwrap_or_default();
        if let Some(decoded) = self.attachment_image(image) {
            attachments::paint_cover(ui, rect.shrink(1.0), id, &decoded, CornerRadius::same(7));
            let name = image["name"].as_str().unwrap_or_default();
            let response = ui
                .interact(
                    rect,
                    ui.id().with(("attachment-preview", id)),
                    Sense::click(),
                )
                .on_hover_text(format!("Preview {name}"));
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomIn);
            }
            clicked = response.clicked();
        } else {
            let name = image["name"].as_str().unwrap_or_default();
            let text_font = font(10.0, Weight::Regular);
            let text_width =
                widgets::text_width(ui, name, text_font.clone()).min(rect.width() - 16.0);
            widgets::text_in(
                ui,
                Rect::from_center_size(rect.center(), Vec2::new(text_width + 1.0, 16.0)),
                name,
                text_font,
                theme::secondary_label(),
            );
        }
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(1.0, theme::alpha(theme::border(), 0.8)),
            egui::StrokeKind::Inside,
        );
        clicked
    }

    /// A user message's text size at most `width` wide, measured in an
    /// invisible pass (the markdown drawn, nothing shown) and kept.
    fn user_text_size(&mut self, ui: &mut Ui, message: &Message, text: &str, width: f32) -> Vec2 {
        if text.is_empty() {
            return Vec2::ZERO;
        }
        let key = (message.id.clone(), text.len(), width.to_bits());
        if let Some(size) = self.user_text_sizes.get(&key) {
            return *size;
        }
        let origin = ui.cursor().min;
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(origin, Vec2::new(width, f32::INFINITY)))
                .invisible(),
        );
        let mut markdown = Markdown::new(
            &mut self.highlighter,
            ui.id().with(("user-text-measure", &message.id)),
        );
        markdown.line_breaks = true;
        markdown.skill_chips = true;
        let height = markdown.show(&mut child, text, width);
        let size = Vec2::new(markdown.used_width.ceil().min(width), height);
        if self.user_text_sizes.len() > 2000 {
            self.user_text_sizes.clear();
        }
        self.user_text_sizes.insert(key, size);
        size
    }

    fn user_row(&mut self, ui: &mut Ui, message: &Message, width: f32) {
        // Pluto: a chat message being edited (`chat_versions`).
        if self
            .chat_versions
            .editing
            .as_ref()
            .is_some_and(|(id, _)| *id == message.id)
        {
            self.edit_user_row(ui, message, width);
            return;
        }
        let max_width = (width * 0.8).floor();
        let text = message.text.trim_end();
        // `UserMessageBody`: the text as markdown with its line breaks and
        // chips, measured once so the bubble fits it (`max-w-[80%]`).
        let inner_max = max_width - 24.0;
        let measured = self.user_text_size(ui, message, text, inner_max);
        // `shouldCollapseUserMessage`: past 600 characters or 8 lines, the
        // body folds to 11rem behind a fade.
        let can_collapse = !text.trim().is_empty()
            && (text.encode_utf16().count() > 600 || text.split('\n').count() > 8);
        let collapsed = can_collapse && !self.expanded_user_messages.contains(&message.id);
        let body_height = if collapsed {
            measured.y.min(176.0)
        } else {
            measured.y
        };
        let footer = if can_collapse { 6.0 + 24.0 } else { 0.0 };
        let text_size = Vec2::new(measured.x, body_height + footer);
        // Image attachments sit above the text (`mb-2`); videos share the
        // grid (`UserVideoAttachment`).
        let images = attachments::images(&message.attachments);
        let all_files = attachments::files(&message.attachments);
        let is_video = |file: &&serde_json::Value| {
            crate::ui::video::is_video(
                file["name"].as_str().unwrap_or_default(),
                file["mimeType"].as_str(),
            )
        };
        let videos: Vec<&serde_json::Value> = all_files.iter().copied().filter(is_video).collect();
        let (grid_height, cell) =
            attachments::grid_size(images.len() + videos.len(), max_width - 24.0);
        let grid = if images.is_empty() && videos.is_empty() {
            Vec2::ZERO
        } else {
            Vec2::new(2.0 * cell.x + 8.0, grid_height + 8.0)
        };
        // Pluto: file attachments as chips under the images.
        let files: Vec<&serde_json::Value> =
            all_files.iter().copied().filter(|f| !is_video(f)).collect();
        let (files_per_row, files_block) = attachments::file_grid(files.len(), max_width - 24.0);
        let files_size = if files.is_empty() {
            Vec2::ZERO
        } else {
            files_block + Vec2::new(0.0, 8.0)
        };
        let bubble_size = Vec2::new(
            text_size.x.max(grid.x).max(files_size.x) + 24.0,
            grid.y + files_size.y + text_size.y + 24.0,
        );
        // Over the bubble (`gap-1`): who sent it when the user did not, and a
        // queued or steering message's marker.
        let markers = user_markers(message);
        let markers_height = markers.len() as f32 * (16.0 + 4.0);
        // A message that did not complete says how it ended, in red.
        let status_pill = !matches!(
            message.status.as_str(),
            "" | "completed" | "pending" | "waiting"
        );
        let pill_height = if status_pill { 4.0 + 20.0 } else { 0.0 };
        let (row_rect, response) = ui.allocate_exact_size(
            Vec2::new(
                width,
                markers_height + bubble_size.y + pill_height + 4.0 + 24.0,
            ),
            Sense::hover(),
        );
        if let Some(target) = self.paint_user_markers(ui, &message.id, &markers, row_rect) {
            match target {
                MarkerTarget::Task => {
                    self.open_settings_page(crate::ui::scheduled_tasks::SCHEDULED_TASKS_PAGE)
                }
                MarkerTarget::Thread(id) => self.select_thread(Some(id)),
            }
        }
        let bubble = Rect::from_min_size(
            Pos2::new(
                row_rect.right() - bubble_size.x,
                row_rect.top() + markers_height,
            ),
            bubble_size,
        );
        ui.painter()
            .rect_filled(bubble, CornerRadius::same(18), theme::message_surface());
        for (index, image) in images.iter().enumerate() {
            let cell_rect = Rect::from_min_size(
                bubble.min
                    + Vec2::splat(12.0)
                    + Vec2::new(
                        (index % 2) as f32 * (cell.x + 8.0),
                        (index / 2) as f32 * (cell.y + 8.0),
                    ),
                cell,
            );
            if self.attachment_cell(ui, cell_rect, image) {
                // `buildExpandedImagePreview`: the message's loaded images.
                let mut items = Vec::new();
                let mut start = 0;
                for (other_index, other) in images.iter().enumerate() {
                    if let Some(decoded) = self.attachment_image(other) {
                        if other_index == index {
                            start = items.len();
                        }
                        items.push(crate::ui::lightbox::Item {
                            name: other["name"].as_str().unwrap_or_default().to_owned(),
                            key: other["id"].as_str().unwrap_or_default().to_owned(),
                            image: decoded,
                            source: None,
                        });
                    }
                }
                self.open_lightbox(items, start);
            }
        }
        for (offset, video) in videos.iter().enumerate() {
            let index = images.len() + offset;
            let cell_rect = Rect::from_min_size(
                bubble.min
                    + Vec2::splat(12.0)
                    + Vec2::new(
                        (index % 2) as f32 * (cell.x + 8.0),
                        (index / 2) as f32 * (cell.y + 8.0),
                    ),
                cell,
            );
            if let Some(key) = self.attachment_video(video) {
                let name = video["name"].as_str().unwrap_or("video").to_owned();
                if crate::ui::video::player(ui, &key, cell_rect, &name, true) {
                    self.video_overlay = Some((key, name));
                }
            }
        }
        for (index, file) in files.iter().enumerate() {
            let chip = Rect::from_min_size(
                bubble.min
                    + Vec2::new(12.0, 12.0 + grid.y)
                    + Vec2::new(
                        (index % files_per_row) as f32 * (attachments::FILE_CHIP.x + 8.0),
                        (index / files_per_row) as f32 * (attachments::FILE_CHIP.y + 8.0),
                    ),
                attachments::FILE_CHIP,
            );
            let name = file["name"].as_str().unwrap_or("File");
            let detail = format!(
                "{} · {}",
                attachments::file_kind(name),
                attachments::format_size(file["sizeBytes"].as_u64().unwrap_or_default())
            );
            attachments::paint_file_chip(ui, chip, name, &detail, theme::muted_foreground());
        }
        let text_pos = bubble.min + Vec2::new(12.0, 12.0 + grid.y + files_size.y);
        if !text.is_empty() {
            let body = Rect::from_min_size(text_pos, Vec2::new(measured.x.max(1.0), body_height));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
                text_pos,
                Vec2::new(measured.x.max(1.0) + 1.0, f32::INFINITY),
            )));
            child.set_clip_rect(body.expand2(Vec2::new(2.0, 0.0)).intersect(ui.clip_rect()));
            let workspace = self.workspace_cwd();
            let marks = self.find_marks(&message.id);
            let mut markdown = Markdown::new(
                &mut self.highlighter,
                ui.id().with(("user-text", &message.id)),
            );
            markdown.find = marks;
            markdown.line_breaks = true;
            markdown.skill_chips = true;
            markdown.text_color = Some(theme::foreground());
            markdown.media_root = workspace;
            markdown.context_records = message.context["records"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            markdown.show(&mut child, text, measured.x.max(1.0) + 1.0);
            let opened_file = markdown.opened_file.take();
            let opened_citation = markdown.opened_citation.take();
            let opened_context = markdown.opened_context.take();
            let marks = markdown.find.take();
            // The active match past the fold opens the message (`beforematch`).
            if let Some(found) = self.found_mark(marks)
                && collapsed
                && found.bottom() > body.bottom()
            {
                self.expanded_user_messages.insert(message.id.clone());
                ui.ctx().request_repaint();
            }
            if collapsed {
                // `mask-image: linear-gradient(black calc(100% - 1.75rem), transparent)`.
                let fade = Rect::from_min_max(
                    Pos2::new(bubble.left(), body.bottom() - 28.0),
                    Pos2::new(bubble.right(), body.bottom()),
                );
                let surface = theme::message_surface();
                let mut mesh = egui::Mesh::default();
                for (y, alpha) in [(fade.top(), 0.0), (fade.bottom(), 1.0)] {
                    mesh.colored_vertex(Pos2::new(fade.left(), y), theme::alpha(surface, alpha));
                    mesh.colored_vertex(Pos2::new(fade.right(), y), theme::alpha(surface, alpha));
                }
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(1, 2, 3);
                ui.painter().add(egui::Shape::mesh(mesh));
            }
            if can_collapse {
                // `Button size="xs" variant="ghost-muted"`, `-ml-1`.
                let label = if collapsed {
                    "Show full message"
                } else {
                    "Show less"
                };
                let label_font = font(12.0, Weight::Medium);
                let button_width = widgets::text_width(ui, label, label_font.clone()).ceil() + 16.0;
                let button = Rect::from_min_size(
                    Pos2::new(text_pos.x - 4.0, body.bottom() + 6.0),
                    Vec2::new(button_width, 24.0),
                );
                let response = ui.interact(
                    button,
                    ui.id().with(("user-collapse", &message.id)),
                    Sense::click(),
                );
                if response.hovered() {
                    ui.painter()
                        .rect_filled(button, CornerRadius::same(8), theme::accent());
                }
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(button.left() + 8.0, button.top()), button.max),
                    label,
                    label_font,
                    if response.hovered() {
                        theme::foreground()
                    } else {
                        theme::muted_foreground()
                    },
                );
                if response.clicked() {
                    if !self.expanded_user_messages.remove(&message.id) {
                        self.expanded_user_messages.insert(message.id.clone());
                    }
                    self.keep_scroll = true;
                }
            }
            if let Some(citation) = opened_citation {
                self.open_citation_source(&citation);
            }
            if let Some(file) = opened_file {
                self.open_markdown_file(&file.path);
            }
            if let Some(id) = opened_context {
                let rect = ui
                    .ctx()
                    .data(|d| d.get_temp::<Rect>(egui::Id::new(crate::ui::markdown::CLICKED_CHIP)));
                let record = message.context["records"]
                    .as_array()
                    .and_then(|records| records.iter().find(|r| r["contextId"] == id.as_str()))
                    .cloned();
                // A thread chip opens that thread (`ThreadContextChip`).
                let thread = record
                    .as_ref()
                    .filter(|r| r["kind"] == "thread")
                    .and_then(|r| r["threadId"].as_str())
                    .filter(|id| self.state.threads.iter().any(|t| t.id == *id))
                    .map(str::to_owned);
                if let Some(thread) = thread {
                    self.select_thread(Some(thread));
                } else if let (Some(rect), Some(record)) =
                    (rect, record.filter(|r| r["kind"] == "terminal"))
                {
                    self.bubble_popover = Some((rect, record));
                }
            }
        }
        if status_pill {
            status_pill_at(
                ui,
                Pos2::new(row_rect.right() - 4.0, bubble.bottom() + 4.0),
                &message.status,
                true,
            );
        }
        let meta_top = bubble.bottom() + pill_height + 4.0;
        // Pluto: a chat message's versions, and its Edit button.
        let mut meta_right = row_rect.right() - 4.0;
        if let Some((versions, at, original)) = self.message_versions(&message.id) {
            meta_right = self.version_switcher(
                ui,
                meta_right,
                meta_top,
                &message.id,
                (&versions, at),
                original.as_deref(),
            ) - 4.0;
        }
        if response.hovered() || ui.rect_contains_pointer(row_rect) {
            if self.can_branch(&message.id) {
                let edit =
                    Rect::from_min_size(Pos2::new(meta_right - 24.0, meta_top), Vec2::splat(24.0));
                if widgets::icon_button(
                    ui,
                    edit,
                    ("edit-user", &message.id),
                    "pencil",
                    14.0,
                    theme::muted_foreground(),
                    "Edit message",
                )
                .clicked()
                {
                    self.chat_versions.editing = Some((message.id.clone(), message.text.clone()));
                    self.chat_versions.focus_editor = true;
                }
                meta_right = edit.left() - 4.0;
            }
            let copy =
                Rect::from_min_size(Pos2::new(meta_right - 24.0, meta_top), Vec2::splat(24.0));
            widgets::copy_button(
                ui,
                copy,
                ("copy-user", &message.id),
                "Copy message",
                "Copy message",
                theme::primary(),
                1.0,
                true,
                || message.text.clone(),
            );
            // `MessageCopyButton` "Copy prompt link", for a prompt a link
            // can carry.
            let mut time_right = copy.left();
            let link = crate::ui::prompt_links::prompt_link_for(
                &crate::ui::prompt_links::plain_prompt(&message.text),
            );
            if let Some(link) = link {
                let button = Rect::from_min_size(
                    Pos2::new(time_right - 2.0 - 24.0, meta_top),
                    Vec2::splat(24.0),
                );
                if widgets::icon_button(
                    ui,
                    button,
                    ("prompt-link", &message.id),
                    "link",
                    12.0,
                    theme::muted_foreground(),
                    "Copy prompt link",
                )
                .clicked()
                {
                    self.copy_link(Some(link));
                }
                time_right = button.left();
            }
            // `RevertUserMessageButton`: Edit from here.
            if self.rewind_checkpoint(message).is_some() {
                let revert = Rect::from_min_size(
                    Pos2::new(time_right - 2.0 - 24.0, meta_top),
                    Vec2::splat(24.0),
                );
                let busy =
                    self.rewinding() || self.current_thread().is_some_and(|t| t.is_running());
                let tint = if busy {
                    theme::alpha(theme::muted_foreground(), 0.5)
                } else {
                    theme::muted_foreground()
                };
                if widgets::icon_button(
                    ui,
                    revert,
                    ("revert-user", &message.id),
                    "undo-2",
                    12.0,
                    tint,
                    "Edit from here",
                )
                .clicked()
                    && !busy
                {
                    self.request_rewind(message);
                }
                time_right = revert.left();
            }
            let time = widgets::clock_time(&message.created_at, self.now);
            let time_width = widgets::text_width(ui, &time, font(12.0, Weight::Regular));
            let time_rect = Rect::from_min_size(
                Pos2::new(time_right - 8.0 - time_width, meta_top),
                Vec2::new(time_width + 1.0, 24.0),
            );
            widgets::text_in(
                ui,
                time_rect,
                &time,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            );
            ui.interact(
                time_rect,
                ui.id().with(("user-time", &message.id)),
                Sense::hover(),
            )
            .on_hover_text(widgets::timestamp_tooltip(&message.created_at));
        }
    }

    fn assistant_row(
        &mut self,
        ui: &mut Ui,
        message: &Message,
        header: Option<&str>,
        show_meta: bool,
        changes: Option<&(String, Vec<crate::model::CheckpointFile>)>,
        width: f32,
    ) {
        let top = ui.cursor().top();
        let left = ui.cursor().left();
        // Pluto: the logo and the model over a turn's first reply.
        if let Some(label) = header {
            ui.add_space(4.0);
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());
            crate::ui::pixel_logo::paint(
                ui,
                Rect::from_min_size(
                    Pos2::new(row.left() + 4.0, row.top() + 1.5),
                    Vec2::new(18.0, 15.0),
                ),
                0.0,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(row.left() + 30.0, row.top()), row.max),
                label,
                font(12.0, Weight::Medium),
                theme::muted_foreground(),
            );
            ui.add_space(8.0);
        }
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
            let highlight = self.citation_highlight(ui.ctx(), &message.id);
            let workspace = self.workspace_cwd();
            let plugin = ui.ctx().plugin::<widgets::CopyCleanup>();
            // The message's text can be cited (`data-assistant-citation-source`).
            plugin.lock().source = Some(message.id.clone());
            let marks = self.find_marks(&message.id);
            let word_wrap = self.client_setting("wordWrap") != false;
            let project = self.current_thread().map(|t| t.project_id.clone());
            let sources = self.answer_sources(message.run_id.as_deref());
            let mut markdown = Markdown::new(
                &mut self.highlighter,
                ui.id().with(("markdown", &message.id)),
            );
            markdown.sources = sources;
            markdown.highlight = highlight;
            markdown.find = marks;
            markdown.media_root = workspace;
            markdown.media_thread = self.selected.clone();
            markdown.word_wrap = word_wrap;
            markdown.can_run_shell = !message.streaming && project.is_some();
            markdown.streaming = message.streaming;
            markdown.show(ui, text, width - 8.0);
            let run_command = markdown.run_command.take();
            let opened_thread = markdown.opened_context.take();
            let marks = markdown.find.take();
            plugin.lock().source = None;
            let opened_file = markdown.opened_file.take();
            let opened_citation = markdown.opened_citation.take();
            let asset_requests = std::mem::take(&mut markdown.asset_requests);
            let opened_media = markdown.opened_media.take();
            let media_menu = markdown.media_menu.take();
            self.found_mark(marks);
            // Images in the answer: their signed URLs, then a click expands
            // one and a right click opens its menu.
            self.markdown_media(asset_requests, opened_media, media_menu);
            // `openFileInPanel`: a file chip opens in the right panel.
            if let Some(file) = opened_file {
                self.open_markdown_file(&file.path);
            }
            if let Some(citation) = opened_citation {
                self.open_citation_source(&citation);
            }
            // A thread link's chip opens that thread.
            if let Some(id) =
                opened_thread.filter(|id| self.state.threads.iter().any(|t| &t.id == id))
            {
                self.select_thread(Some(id));
            }
            // `runShellCommand`: the code block's command in the terminal.
            if let (Some(command), Some(project)) = (run_command, project) {
                let script = serde_json::json!({
                    "id": "chat-code-block",
                    "name": "Chat code block",
                    "command": command,
                });
                self.run_project_script(&project, &script);
            }
            if let Some((turn_id, files)) = changes {
                ui.add_space(16.0);
                self.changed_files_card(ui, turn_id, files, width - 8.0);
            }
        });
        ui.add_space(2.0);
        // A revealed citation's message scrolls into view, then pulses.
        if self
            .cite
            .target
            .as_ref()
            .is_some_and(|t| t.message_id == message.id)
            && self.current_thread().is_some_and(|t| {
                self.citation_scroll_target(&t.id).as_deref() == Some(message.id.as_str())
            })
        {
            let rect = Rect::from_min_max(
                Pos2::new(left, top),
                Pos2::new(left + width, ui.cursor().top()),
            );
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
            self.citation_revealed(ui.ctx());
        }
        if show_meta {
            ui.add_space(6.0);
            let (meta, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
            let row_rect = Rect::from_min_max(Pos2::new(left, top), meta.max);
            if ui.rect_contains_pointer(row_rect) {
                // An answer that did not complete says how it ended first.
                let mut x = meta.left() + 4.0;
                if !matches!(message.status.as_str(), "" | "completed") {
                    let width = status_pill_width(ui, &message.status, false);
                    status_pill_at(
                        ui,
                        Pos2::new(x + width, meta.top() + 2.0),
                        &message.status,
                        false,
                    );
                    x += width + 8.0;
                }
                let copy = Rect::from_min_size(Pos2::new(x, meta.top()), Vec2::splat(24.0));
                widgets::copy_button(
                    ui,
                    copy,
                    ("copy-assistant", &message.id),
                    "Copy message",
                    "Copy message",
                    theme::primary(),
                    1.0,
                    true,
                    || message.text.clone(),
                );
                // Pluto: a chat's last answer can be regenerated (`chat_versions`).
                let mut after = copy.right();
                if !message.streaming
                    && let Some((prompt, text)) = self.regenerable_prompt(&message.id)
                    && self.can_branch(&prompt)
                {
                    let regenerate =
                        Rect::from_min_size(Pos2::new(after + 4.0, meta.top()), Vec2::splat(24.0));
                    if widgets::icon_button(
                        ui,
                        regenerate,
                        ("regenerate", &message.id),
                        "refresh-cw",
                        14.0,
                        theme::muted_foreground(),
                        "Regenerate",
                    )
                    .clicked()
                    {
                        self.branch_message(&prompt, &text);
                    }
                    after = regenerate.right();
                }
                // `AssistantForkButton`: a new thread from this answer.
                if let Some(run) = self.forkable_run(&message.id) {
                    let fork =
                        Rect::from_min_size(Pos2::new(after + 4.0, meta.top()), Vec2::splat(24.0));
                    if widgets::icon_button(
                        ui,
                        fork,
                        ("fork", &message.id),
                        "git-fork",
                        12.0,
                        theme::muted_foreground(),
                        "Fork from this response",
                    )
                    .clicked()
                    {
                        self.fork_from_run(&run);
                    }
                    after = fork.right();
                }
                if !message.streaming {
                    let time = widgets::clock_time(&message.updated_at, self.now);
                    let width = widgets::text_in(
                        ui,
                        Rect::from_min_max(Pos2::new(after + 8.0, meta.top()), meta.max),
                        &time,
                        font(12.0, Weight::Regular),
                        theme::muted_foreground(),
                    );
                    let time_rect = Rect::from_min_size(
                        Pos2::new(after + 8.0, meta.top()),
                        Vec2::new(width, 24.0),
                    );
                    ui.interact(
                        time_rect,
                        ui.id().with(("answer-time", &message.id)),
                        Sense::hover(),
                    )
                    .on_hover_text(widgets::timestamp_tooltip(&message.updated_at));
                }
            }
        }
    }

    fn fold_row(
        &mut self,
        ui: &mut Ui,
        label: &str,
        expanded: bool,
        width: f32,
        created_at: &str,
    ) -> egui::Response {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(width, 4.0 + MESSAGE_LINE + 8.0 + 1.0),
            Sense::hover(),
        );
        // `TimelineRowTimestamp`: the time at the right while hovered.
        if ui.rect_contains_pointer(rect) {
            let time = widgets::clock_time(created_at, self.now);
            let time_width = widgets::text_width(ui, &time, font(12.0, Weight::Regular));
            let time_rect = Rect::from_min_size(
                Pos2::new(rect.right() - 4.0 - time_width, rect.top() + 4.0),
                Vec2::new(time_width, MESSAGE_LINE),
            );
            widgets::text_in(
                ui,
                time_rect,
                &time,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            );
            ui.interact(
                time_rect,
                ui.id().with(("fold-time", rect.top() as i32)),
                Sense::hover(),
            )
            .on_hover_text(widgets::timestamp_tooltip(created_at));
        }
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
        paint_work_icon(
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
        if self.notification_card(ui, entry, width) {
            return;
        }
        // `formatWorkspaceRelativePath`: a read inside the workspace names
        // its path from there.
        let relative;
        let label = match self.workspace_cwd() {
            Some(root) if label.starts_with("Read ") => {
                let root = format!("{}/", root.trim_end_matches('/'));
                relative = label.replacen(&format!("Read {root}"), "Read ", 1);
                relative.as_str()
            }
            _ => label,
        };
        let key = format!("entry:{}", entry.id);
        let expanded = self.expansion.groups.contains(&key);
        let failed = timeline::indicates_failure(entry);
        let tool_like = timeline::is_tool_like(entry);
        let destructive = failed
            && (entry.item_type.as_deref() == Some("error")
                || entry.tone == "error" && entry.item_type.is_none()
                || !tool_like);
        // Command runs and tool calls open as the inspector's body.
        let call = entry
            .item
            .clone()
            .filter(|item| crate::ui::inspector::is_tool_call(item));
        let body = if call.is_some() {
            None
        } else {
            expanded_body(entry, label)
        };
        // A call whose output the timeline withheld opens to fetch it.
        let withheld = call.as_deref().is_some_and(crate::ui::inspector::has_body);
        let can_expand = entry.question.is_some()
            || (failed && !label.trim().is_empty())
            || body.is_some()
            || withheld;
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
        } else if failed {
            // `text-tool-error-icon/40`: a failed tool keeps its icon, tinted.
            (work_icon(entry), theme::alpha(theme::error(), 0.4))
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
        let icon_rect = Rect::from_min_size(line.min, Vec2::new(24.0, MESSAGE_LINE));
        let image_icon = !destructive
            && entry
                .item
                .as_deref()
                .is_some_and(|item| self.tool_image_icon(ui, icon_rect, item, true));
        if !image_icon {
            paint_work_icon(ui, icon, icon_rect, 16.0, icon_color);
        }
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
        // Trailing: a created thread's "Open chat", a subagent's "Open
        // subagent", and the time while hovered, before the chevron.
        let item = entry.item.as_deref();
        let open_target = item.and_then(|item| match item.kind.as_str() {
            "thread_created" => item
                .str("targetThreadId")
                .map(|t| ("Open chat", t.to_owned(), true)),
            "notification"
                if matches!(
                    item.get("source")["kind"].as_str(),
                    Some("subagent" | "delegated_task")
                ) =>
            {
                item.get("source")["childThreadId"]
                    .as_str()
                    .map(|t| ("Open subagent", t.to_owned(), false))
            }
            _ => None,
        });
        let mut trailing = if can_expand {
            line.right() - 22.0
        } else {
            line.right()
        };
        if let Some((action, target, primary)) = &open_target {
            let action_font = if *primary {
                font(14.0, Weight::Regular)
            } else {
                font(14.0, Weight::Medium)
            };
            let w = widgets::text_width(ui, action, action_font.clone());
            let action_rect = Rect::from_min_size(
                Pos2::new(trailing - w, line.top()),
                Vec2::new(w, MESSAGE_LINE),
            );
            let action_response = ui
                .interact(
                    action_rect,
                    ui.id().with(("work-open", &entry.id)),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let color = if *primary {
                theme::primary()
            } else {
                theme::foreground()
            };
            widgets::text_in(ui, action_rect, action, action_font, color);
            if action_response.hovered() {
                ui.painter().line_segment(
                    [
                        Pos2::new(action_rect.left(), action_rect.bottom() - 3.0),
                        Pos2::new(action_rect.right(), action_rect.bottom() - 3.0),
                    ],
                    egui::Stroke::new(1.0, color),
                );
            }
            if action_response.clicked() && self.state.threads.iter().any(|t| &t.id == target) {
                self.select_thread(Some(target.clone()));
            }
            trailing = action_rect.left() - 8.0;
        }
        if ui.rect_contains_pointer(rect) && !entry.created_at.is_empty() {
            let time = widgets::clock_time(&entry.created_at, self.now);
            let w = widgets::text_width(ui, &time, font(12.0, Weight::Regular));
            let time_rect = Rect::from_min_size(
                Pos2::new(trailing - w, line.top()),
                Vec2::new(w, MESSAGE_LINE),
            );
            widgets::text_in(
                ui,
                time_rect,
                &time,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            );
            trailing = time_rect.left() - 8.0;
        }
        let text_right = trailing;
        let label_rect = Rect::from_min_max(
            Pos2::new(line.left() + 30.0, line.top()),
            Pos2::new(text_right, line.bottom()),
        );
        let label_width = match self.thread_read_label(entry, label) {
            // The read thread's title links to it, the quotes outside.
            Some((prefix, title, target)) => {
                let lead = format!("{prefix} \u{201c}");
                let x = label_rect.left()
                    + widgets::text_in(ui, label_rect, &lead, label_font.clone(), label_color);
                let quote = widgets::text_width(ui, "\u{201d}", label_font.clone());
                let title_rect = Rect::from_min_max(
                    Pos2::new(x, line.top()),
                    Pos2::new(text_right - quote, line.bottom()),
                );
                let title_width = widgets::text_in(
                    ui,
                    title_rect,
                    &title,
                    label_font.clone(),
                    theme::foreground(),
                );
                let link =
                    Rect::from_min_size(title_rect.min, Vec2::new(title_width, MESSAGE_LINE));
                let response = ui
                    .interact(
                        link,
                        ui.id().with(("thread-read", &entry.id)),
                        Sense::click(),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.hovered() {
                    ui.painter().line_segment(
                        [
                            Pos2::new(link.left(), link.bottom() - 3.0),
                            Pos2::new(link.right(), link.bottom() - 3.0),
                        ],
                        egui::Stroke::new(1.0, theme::foreground()),
                    );
                }
                if response.clicked() {
                    self.select_thread(Some(target));
                }
                let end = Rect::from_min_max(Pos2::new(link.right(), line.top()), line.max);
                let close = widgets::text_in(ui, end, "\u{201d}", label_font, label_color);
                link.right() + close - label_rect.left()
            }
            None => widgets::text_in(ui, label_rect, label, label_font, label_color),
        };
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
        // `workspacePreparationRetryRunIds`: a failed preparation retries.
        if let Some(run_id) = entry
            .item
            .as_deref()
            .and_then(|item| self.retryable_preparation(item))
        {
            let label = "Retry";
            let button_width = widgets::outline_button_width(ui, true, label);
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, 24.0 + 4.0), Sense::hover());
            let button = Rect::from_min_size(
                Pos2::new(row.left() + 2.0 + 28.0, row.top()),
                Vec2::new(button_width, 24.0),
            );
            if widgets::outline_button(
                ui,
                button,
                ("retry-preparation", &run_id),
                Some(("rotate-ccw", theme::foreground())),
                label,
                CornerRadius::same(6),
            )
            .clicked()
            {
                let mut command = crate::model::command(
                    "prepared-run.retry",
                    &entry
                        .item
                        .as_ref()
                        .map_or_else(String::new, |i| i.thread_id.clone()),
                );
                command["runId"] = serde_json::json!(run_id);
                self.dispatch(command);
            }
        }
        if expanded && let Some(item) = &call {
            self.inspector_body(ui, item, rect.left() + 2.0 + 28.0, width - 30.0);
        }
        // `V2ItemInspector`'s checkpoint: its state and files, and Roll back
        // while it is ready.
        if expanded && let Some(item) = entry.item.as_deref().filter(|i| i.kind == "checkpoint") {
            self.checkpoint_rollback_row(ui, item, rect.left() + 2.0 + 28.0, width);
        }
        if expanded && let Some(item) = entry.item.clone() {
            let left = rect.left() + 2.0 + 28.0;
            if self.tool_output_images(ui, &item, left, width - 30.0) {
                ui.add_space(4.0);
            }
        }
    }

    fn checkpoint_rollback_row(
        &mut self,
        ui: &mut Ui,
        item: &crate::model::TurnItem,
        left: f32,
        width: f32,
    ) {
        let (Some(checkpoint), Some(scope)) = (item.str("checkpointId"), item.str("scopeId"))
        else {
            return;
        };
        let status = self
            .current_detail()
            .and_then(|d| {
                d.checkpoints
                    .iter()
                    .find(|c| c.id == checkpoint && c.scope_id == scope)
                    .map(|c| c.status.clone())
            })
            .unwrap_or_else(|| item.status.clone());
        let files = item.get("files").as_array().map_or(0, Vec::len);
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, 24.0 + 6.0), Sense::hover());
        let summary = format!("{status} · {files} files");
        let summary_width = widgets::text_width(ui, &summary, font(12.0, Weight::Regular));
        widgets::text_in(
            ui,
            Rect::from_min_size(
                Pos2::new(left, row.top() + 2.0),
                Vec2::new(summary_width, 24.0),
            ),
            &summary,
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
        );
        if status != "ready" {
            return;
        }
        let label = "Roll back";
        let button = Rect::from_min_size(
            Pos2::new(left + summary_width + 8.0, row.top() + 2.0),
            Vec2::new(widgets::outline_button_width(ui, true, label), 24.0),
        );
        let clicked = widgets::outline_button(
            ui,
            button,
            ("checkpoint-rollback", checkpoint),
            Some(("rotate-ccw", theme::foreground())),
            label,
            CornerRadius::same(6),
        )
        .clicked();
        if !clicked {
            return;
        }
        // `onRollbackCheckpoint`.
        if !self.connected() {
            self.notify(
                crate::ui::toasts::Kind::Warning,
                "Reconnect before reverting checkpoints.",
            );
        } else if self.running() {
            self.notify(
                crate::ui::toasts::Kind::Warning,
                "Interrupt the current turn before reverting checkpoints.",
            );
        } else {
            self.confirm = Some(crate::ui::thread_menu::Confirm {
                title: "Roll back this thread to the selected checkpoint?".into(),
                message: "This action cannot be undone.".into(),
                confirm_label: "Roll back",
                command: serde_json::json!({
                    "type": "checkpoint.rollback",
                    "commandId": uuid::Uuid::new_v4().to_string(),
                    "threadId": item.thread_id,
                    "scopeId": scope,
                    "checkpointId": checkpoint,
                }),
                more: Vec::new(),
            });
        }
    }

    /// `AttemptFoldTimelineRow`: the output a steer cut off, behind a bordered
    /// disclosure.
    fn attempt_fold_row(&mut self, ui: &mut Ui, expanded: bool, width: f32) -> egui::Response {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::hover());
        let response = ui
            .interact(
                rect,
                ui.id().with(("attempt-fold", rect.top() as i32)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let fill = theme::alpha(theme::muted(), if response.hovered() { 0.35 } else { 0.2 });
        ui.painter().rect(
            rect,
            CornerRadius::same(6),
            fill,
            egui::Stroke::new(1.0, theme::alpha(theme::border(), 0.6)),
            egui::StrokeKind::Inside,
        );
        let muted = theme::muted_foreground();
        theme::paint_icon(
            ui,
            if expanded {
                "chevron-down"
            } else {
                "chevron-right"
            },
            Rect::from_center_size(
                Pos2::new(rect.left() + 11.0 + 7.0, rect.center().y),
                Vec2::splat(14.0),
            ),
            14.0,
            muted,
        );
        let x = rect.left() + 11.0 + 14.0 + 8.0;
        let label = "Superseded attempt";
        let label_font = font(12.0, Weight::Medium);
        let label_width = widgets::text_width(ui, label, label_font.clone());
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(x, rect.top()), rect.max),
            label,
            label_font,
            theme::alpha(theme::foreground(), 0.8),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(x + label_width + 8.0, rect.top() + 1.0), rect.max),
            "Cut off by a steer",
            font(11.0, Weight::Regular),
            muted,
        );
        response
    }

    /// `canForkProjectedAssistantItem`: a completed answer's run, when its
    /// provider can fork from it (natively, or by handing the whole thread
    /// over); history without a session record may always fork. Pluto's
    /// chats branch through their versions instead.
    fn forkable_run(&self, message_id: &str) -> Option<(String, String)> {
        let thread = self.current_thread()?;
        if self.is_chat_thread(&thread) {
            return None;
        }
        let detail = self.current_detail()?;
        let row = detail.visible.iter().find(|row| {
            row.item.kind == "assistant_message" && row.item.str("messageId") == Some(message_id)
        })?;
        let item = &row.item;
        if item.status != "completed" {
            return None;
        }
        // An inherited answer forks from the thread that holds it.
        let run = (row.source_thread_id.clone(), item.run_id.clone()?);
        let session = item
            .provider_thread_id
            .as_deref()
            .and_then(|id| detail.provider_threads.iter().find(|t| t["id"] == id))
            .and_then(|t| t["providerSessionId"].as_str())
            .and_then(|id| detail.provider_sessions.iter().find(|s| s["id"] == id));
        let Some(capabilities) = session.map(|s| &s["capabilities"]) else {
            return Some(run);
        };
        let threads = &capabilities["threads"];
        let native = threads["canForkThread"] == true
            && threads["canForkFromTurn"] == true
            && capabilities["identity"]["nativeThreadIds"] == "strong";
        (native || capabilities["context"]["supportsFullThreadHandoff"] == true).then_some(run)
    }

    /// `forkThreadFromRun`: the fork opens as "<title> fork".
    fn fork_from_run(&mut self, (source, run): &(String, String)) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        let target = uuid::Uuid::new_v4().to_string();
        self.dispatch(serde_json::json!({
            "type": "thread.fork",
            "commandId": uuid::Uuid::new_v4().to_string(),
            "createdBy": "user",
            "creationSource": "web",
            "sourceThreadId": source,
            "targetThreadId": target,
            "sourcePoint": {"type": "run", "runId": run},
            "title": format!("{} fork", thread.title),
        }));
        self.select_thread(Some(target));
    }

    /// The failed run a workspace preparation error can retry.
    fn retryable_preparation(&self, item: &crate::model::TurnItem) -> Option<String> {
        if item.kind != "error"
            || item.status != "failed"
            || item.get("failure")["code"] != "workspace_preparation_failed"
        {
            return None;
        }
        let run_id = item.run_id.as_deref()?;
        let detail = self.current_detail()?;
        detail
            .runs
            .iter()
            .any(|run| {
                run.id == run_id && run.status == "failed" && run.workspace_preparation.is_some()
            })
            .then(|| run_id.to_owned())
    }

    /// Opens or closes a work group, keeping the clicked row in place.
    fn toggle_work_group(&mut self, group_id: &str) {
        if !self.expansion.groups.remove(group_id) {
            self.expansion.groups.insert(group_id.to_owned());
        }
        self.keep_scroll = true;
    }

    fn working_row(&mut self, ui: &mut Ui, started_at: Option<&str>, width: f32) {
        // `pt-1`, an `h-6` line, `pb-2` and the bottom border.
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(width, 4.0 + 24.0 + 8.0 + 1.0), Sense::hover());
        ui.painter().rect_filled(
            Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 1.0), rect.max),
            0.0,
            theme::alpha(theme::border(), 0.6),
        );
        // While the worktree is made, the row says so, shimmering.
        if self.preparing_worktree() {
            crate::ui::live_activity::sweep_label(
                ui,
                Rect::from_min_size(
                    Pos2::new(rect.left() + 4.0, rect.top() + 4.0),
                    Vec2::new(rect.width() - 8.0, 24.0),
                ),
                "Setting up worktree…",
                font(14.0, Weight::Regular),
                MESSAGE_LINE,
                theme::muted_foreground(),
                theme::foreground(),
                true,
            );
            ui.ctx().request_repaint();
            return;
        }
        let elapsed = started_at
            .and_then(timeline::parse_time)
            .map(|start| (self.now - start).num_milliseconds().max(0))
            .map(|ms| timeline::format_duration((ms / 1000) * 1000))
            .map(|d| format!("Working for {d}"))
            .unwrap_or_else(|| "Working".into());
        widgets::text(
            ui,
            Pos2::new(
                rect.left() + 4.0,
                rect.top() + 4.0 + (24.0 - MESSAGE_LINE) / 2.0,
            ),
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
/// A line over a user bubble: its text, icon, tooltip and what a click
/// opens.
struct Marker {
    text: &'static str,
    icon: Option<&'static str>,
    tooltip: Option<&'static str>,
    small: bool,
    target: Option<MarkerTarget>,
}

#[derive(Clone)]
enum MarkerTarget {
    Task,
    Thread(String),
}

/// `UserTimelineRow`'s attribution line and `UserMessageIntentMarker`.
fn user_markers(message: &Message) -> Vec<Marker> {
    let mut markers = Vec::new();
    match message.attribution.as_deref() {
        Some("automation") => markers.push(Marker {
            text: "Sent by automation",
            icon: None,
            tooltip: None,
            small: true,
            target: message
                .scheduled_task_id
                .as_ref()
                .map(|_| MarkerTarget::Task),
        }),
        // Pluto: the server is Pluto's.
        Some("t3code") => markers.push(Marker {
            text: "Sent by Pluto",
            icon: None,
            tooltip: None,
            small: true,
            target: None,
        }),
        Some("agent") => markers.push(Marker {
            text: "Sent by another agent",
            icon: None,
            tooltip: None,
            small: true,
            target: message.sender_thread_id.clone().map(MarkerTarget::Thread),
        }),
        _ => {}
    }
    match message.input_intent.as_deref() {
        None | Some("turn_start") => {}
        Some("queued_turn") => markers.push(Marker {
            text: "From queue",
            icon: None,
            tooltip: Some("Queued while the agent was busy"),
            small: false,
            target: None,
        }),
        Some(intent) => markers.push(Marker {
            text: "Steer",
            icon: Some("redo-2"),
            tooltip: Some(if intent == "promoted_queued_to_steer" {
                "Originally queued, then promoted to steer the active turn"
            } else {
                "Steered the active turn"
            }),
            small: false,
            target: None,
        }),
    }
    markers
}

impl App {
    /// Draws the markers right-aligned at the top of `row`; returns what a
    /// clicked one opens.
    fn paint_user_markers(
        &mut self,
        ui: &mut Ui,
        message_id: &str,
        markers: &[Marker],
        row: Rect,
    ) -> Option<MarkerTarget> {
        let mut clicked = None;
        for (index, marker) in markers.iter().enumerate() {
            // `text-2xs text-muted-foreground/70`, or `text-xs`.
            let (size, color) = if marker.small {
                (10.0, theme::alpha(theme::muted_foreground(), 0.7))
            } else {
                (12.0, theme::muted_foreground())
            };
            let label_font = font(size, Weight::Regular);
            let text_width = widgets::text_width(ui, marker.text, label_font.clone());
            let icon_width = if marker.icon.is_some() {
                12.0 + 4.0
            } else {
                0.0
            };
            let top = row.top() + index as f32 * 20.0;
            let right = row.right() - 4.0;
            let rect = Rect::from_min_max(
                Pos2::new(right - text_width - icon_width, top),
                Pos2::new(right, top + 16.0),
            );
            let mut response = ui.interact(
                rect,
                ui.id().with(("user-marker", message_id, index)),
                if marker.target.is_some() {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            if let Some(tooltip) = marker.tooltip {
                response = response.on_hover_text(tooltip);
            }
            let hovered = marker.target.is_some() && response.hovered();
            let color = if hovered {
                theme::muted_foreground()
            } else {
                color
            };
            if let Some(icon) = marker.icon {
                theme::paint_icon(
                    ui,
                    icon,
                    Rect::from_min_size(Pos2::new(rect.left(), top + 2.0), Vec2::splat(12.0)),
                    12.0,
                    color,
                );
            }
            widgets::text(
                ui,
                Pos2::new(rect.left() + icon_width, top + (16.0 - size * 1.3) / 2.0),
                marker.text,
                label_font,
                color,
                (size * 1.3).round(),
            );
            if hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                ui.painter().line_segment(
                    [
                        Pos2::new(rect.left() + icon_width, rect.bottom() - 2.0),
                        Pos2::new(rect.right(), rect.bottom() - 2.0),
                    ],
                    egui::Stroke::new(1.0, color),
                );
            }
            if response.clicked() {
                clicked = marker.target.clone();
            }
        }
        clicked
    }
}

/// `ThreadStatusLine`: what the thread's state is, under its last message,
/// with the one action that leaves it.
pub(crate) struct StatusLine {
    icon: &'static str,
    label: String,
    action: &'static str,
    kind: StatusAction,
}

enum StatusAction {
    Wake,
    Unsettle,
    /// Dismisses the Woke notice (the thread's wake time, read).
    Dismiss(String),
}

impl App {
    /// Snoozed, settled, or just woke (`ChatView`'s `threadStatusLine`).
    fn thread_status_line(&self, thread: &crate::model::Thread) -> Option<StatusLine> {
        if crate::snooze::effective_snoozed(thread, self.now) {
            let left = thread
                .snoozed_until
                .as_deref()
                .map(|until| until_label(until, self.now))
                .filter(|l| !l.is_empty());
            return Some(StatusLine {
                icon: "alarm-clock",
                label: left.map_or("Snoozed".into(), |l| format!("Snoozed, {l}")),
                action: "Wake now",
                kind: StatusAction::Wake,
            });
        }
        if thread.settled_override.as_deref() == Some("settled") {
            let ago = thread
                .settled_at
                .as_deref()
                .map(|at| ago_label(at, self.now))
                .filter(|l| !l.is_empty());
            return Some(StatusLine {
                icon: "circle-check",
                label: ago.map_or("Settled".into(), |a| format!("Settled {a}")),
                action: "Un-settle",
                kind: StatusAction::Unsettle,
            });
        }
        // Only with snoozing; a completion after the wake reads as seen, so a
        // completion's own wake does not flash (`ChatView`'s visit floor).
        if !self.capability("threadSnooze") {
            return None;
        }
        let woke = self.woke_at(thread)?;
        let completed = thread
            .latest_run
            .as_ref()
            .and_then(|r| r.completed_at.as_deref())
            .and_then(crate::timeline::parse_time);
        if completed
            .zip(crate::timeline::parse_time(&woke))
            .is_some_and(|(done, woke)| done >= woke)
        {
            return None;
        }
        Some(StatusLine {
            icon: "alarm-clock",
            label: "Woke from snooze".into(),
            action: "Dismiss",
            kind: StatusAction::Dismiss(woke),
        })
    }

    fn paint_thread_status_line(
        &mut self,
        ui: &mut Ui,
        thread_id: &str,
        line: &StatusLine,
        column: f32,
    ) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(column, 24.0), Sense::hover());
        let small = font(12.0, Weight::Regular);
        let muted = theme::muted_foreground();
        let mut x = rect.left() + 4.0;
        theme::paint_icon(
            ui,
            line.icon,
            Rect::from_min_size(Pos2::new(x, rect.top() + 1.0), Vec2::splat(14.0)),
            14.0,
            muted,
        );
        x += 14.0 + 6.0;
        for (text, gap) in [(line.label.as_str(), 6.0), ("·", 6.0)] {
            widgets::text(
                ui,
                Pos2::new(x, rect.top()),
                text,
                small.clone(),
                muted,
                16.0,
            );
            x += widgets::text_width(ui, text, small.clone()) + gap;
        }
        let width = widgets::text_width(ui, line.action, small.clone());
        let button = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(width, 16.0));
        let response = ui
            .interact(
                button,
                ui.id().with(("status-line", thread_id)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let color = if response.hovered() {
            theme::foreground()
        } else {
            muted
        };
        widgets::text(ui, button.min, line.action, small, color, 16.0);
        if response.hovered() {
            ui.painter().line_segment(
                [
                    Pos2::new(button.left(), button.bottom() - 1.0),
                    Pos2::new(button.right(), button.bottom() - 1.0),
                ],
                egui::Stroke::new(1.0, color),
            );
        }
        if !response.clicked() {
            return;
        }
        match &line.kind {
            StatusAction::Wake | StatusAction::Unsettle => {
                let kind = if matches!(line.kind, StatusAction::Wake) {
                    "thread.unsnooze"
                } else {
                    "thread.unsettle"
                };
                let mut command = crate::model::command(kind, thread_id);
                command["reason"] = serde_json::json!("user");
                let failure = if kind == "thread.unsnooze" {
                    "Failed to wake thread"
                } else {
                    "Failed to un-settle thread"
                };
                self.dispatch_titled(command, failure);
            }
            StatusAction::Dismiss(at) => self.acknowledge_woke(thread_id, at),
        }
    }
}

/// `formatRelativeTimeUntilLabel`: "3h left", "Soon", "Expired".
fn until_label(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(at) = crate::timeline::parse_time(iso) else {
        return String::new();
    };
    let seconds = (at - now).num_seconds();
    match seconds {
        ..=0 => "Expired".into(),
        1..5 => "Soon".into(),
        5..60 => format!("{seconds}s left"),
        60..3600 => format!("{}m left", seconds / 60),
        3600..86_400 => format!("{}h left", seconds / 3600),
        _ => format!("{}d left", seconds / 86_400),
    }
}

/// `formatRelativeTimeLabel`: "just now", "5m ago", "2d ago".
fn ago_label(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(at) = crate::timeline::parse_time(iso) else {
        return String::new();
    };
    let seconds = (now - at).num_seconds();
    match seconds {
        ..60 => "just now".into(),
        60..3600 => format!("{}m ago", seconds / 60),
        3600..86_400 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86_400),
    }
}

/// The "Scroll to end" pill (`Button size="xs" variant="glass"`), centered
/// 4px above the composer; true when clicked.
fn scroll_to_end_pill(ui: &mut Ui, rect: Rect, bottom_padding: f32) -> bool {
    let label_font = font(12.0, Weight::Medium);
    let width =
        8.0 + 14.0 + 4.0 + widgets::text_width(ui, "Scroll to end", label_font.clone()) + 8.0;
    let button = Rect::from_center_size(
        Pos2::new(
            rect.center().x,
            rect.bottom() - bottom_padding - 4.0 - 6.0 - 12.0,
        ),
        Vec2::new(width.round(), 24.0),
    );
    let response = ui
        .interact(button, ui.id().with("scroll-to-end"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    painter.rect(
        button,
        CornerRadius::same(6),
        if response.hovered() {
            theme::accent()
        } else {
            theme::alpha(theme::popover(), 0.85)
        },
        egui::Stroke::new(1.0, theme::border()),
        egui::StrokeKind::Inside,
    );
    theme::paint_icon(
        ui,
        "chevron-down",
        Rect::from_min_size(
            Pos2::new(button.left() + 8.0, button.center().y - 7.0),
            Vec2::splat(14.0),
        ),
        14.0,
        theme::foreground(),
    );
    widgets::text(
        ui,
        Pos2::new(button.left() + 8.0 + 14.0 + 4.0, button.top() + 4.0),
        "Scroll to end",
        label_font,
        theme::foreground(),
        16.0,
    );
    response.clicked()
}

pub(crate) use pluto_core::timeline::{summary_icon, work_icon};

impl App {
    /// `threadReadTargetTitle`: a `t3_thread_read` row names the thread it
    /// read, unless that thread is archived, deleted or untitled. Returns
    /// the label's prefix, the title and the thread.
    pub(crate) fn thread_read_label(
        &self,
        entry: &WorkEntry,
        label: &str,
    ) -> Option<(String, String, String)> {
        let target = timeline::thread_read_target(entry)?;
        let prefix = timeline::thread_read_prefix(label)?;
        let thread = self.state.threads.iter().find(|t| t.id == target)?;
        let title = thread.title.trim();
        (thread.archived_at.is_none() && thread.deleted_at.is_none() && !title.is_empty())
            .then(|| (prefix, title.to_owned(), target))
    }
}

/// A work icon: Pluto's mark for `t3-code`, else the lucide icon.
pub(crate) fn paint_work_icon(ui: &Ui, icon: &str, rect: Rect, size: f32, color: egui::Color32) {
    if icon == "t3-code" {
        crate::ui::pixel_logo::paint(
            ui,
            Rect::from_center_size(rect.center(), Vec2::splat(size)),
            0.0,
        );
    } else {
        theme::paint_icon(ui, icon, rect, size, color);
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

fn pill_font(user: bool) -> egui::FontId {
    if user {
        font(10.0, Weight::Medium)
    } else {
        theme::mono(10.0)
    }
}

/// The status pill's width (`px-1.5` and its border).
pub(crate) fn status_pill_width(ui: &Ui, status: &str, user: bool) -> f32 {
    widgets::text_width(ui, status, pill_font(user)) + 12.0 + 2.0
}

/// A message's status pill, its right edge at `right_top`: red for a user
/// message (`text-destructive`), neutral and mono on an answer's meta row.
/// Returns its width.
pub(crate) fn status_pill_at(ui: &Ui, right_top: Pos2, status: &str, user: bool) -> f32 {
    let text_font = pill_font(user);
    let width = status_pill_width(ui, status, user);
    let rect = Rect::from_min_size(
        Pos2::new(right_top.x - width, right_top.y),
        Vec2::new(width, 20.0),
    );
    let (fill, stroke, color) = if user {
        let red = theme::error();
        (theme::alpha(red, 0.08), theme::alpha(red, 0.25), red)
    } else {
        (
            egui::Color32::TRANSPARENT,
            theme::alpha(theme::border(), 0.7),
            theme::muted_foreground(),
        )
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        fill,
        egui::Stroke::new(1.0, stroke),
        egui::StrokeKind::Inside,
    );
    widgets::text(
        ui,
        Pos2::new(rect.left() + 7.0, rect.top() + 3.0),
        status,
        text_font,
        color,
        14.0,
    );
    width
}
