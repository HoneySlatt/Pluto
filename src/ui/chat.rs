//! Main workspace: header (`ChatHeader.tsx`), the messages timeline
//! (`MessagesTimeline.tsx`) and the empty/draft hero (`DraftHeroHeadline.tsx`).
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::model::Project;
use crate::theme::{self, Weight, font};
use crate::timeline::Message;
use crate::timeline::{self, Row, WorkEntry};
use crate::ui::markdown::Markdown;
use crate::ui::{attachments, project_icon, timeline_view, widgets};

const MESSAGE_LINE: f32 = 22.75;

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
            self.composer(
                ui,
                Pos2::new(column_left, composer_top),
                column,
                false,
                &mode,
            );
        } else if let Some(project) = draft_project {
            // Hero + composer, vertically centered as one group.
            // Centered as one group; without the checkout strip it is 32px
            // shorter.
            let strip = if self.workspace_is_repo() { 0.0 } else { 16.0 };
            let composer_top = (body.center().y - 98.0 + strip).round();
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
            crate::generated_images::images(&detail, ui.ctx())
        } else {
            Vec::new()
        };
        let mut rows =
            self.timeline_view
                .rows(&thread, &detail, &self.expansion, revision, &images);
        // A revealed citation's message: its folded turn opens, and the
        // timeline stops following its end until it is in view.
        let reveal = self.citation_scroll_target(&thread.id);
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
        // Toggling a fold keeps the clicked row in place instead of following the end.
        let keep_scroll = std::mem::take(&mut self.keep_scroll);
        let (select_all, selecting) = {
            let plugin = ui.ctx().plugin::<widgets::CopyCleanup>();
            let plugin = plugin.lock();
            (plugin.select_all, plugin.has_range || plugin.select_all)
        };
        let mut view = std::mem::take(&mut self.timeline_view);
        view.open(&thread.id);
        if !selecting {
            view.pinned.clear();
        }
        let viewport_height = if view.viewport > 0.0 {
            view.viewport
        } else {
            rect.height()
        };
        let keys: Vec<egui::Id> = rows.iter().map(timeline_view::row_key).collect();
        let guesses: Vec<f32> = rows
            .iter()
            .zip(&keys)
            .map(|(row, key)| view.height(*key, row, column))
            .collect();
        let content_height = 24.0 + guesses.iter().sum::<f32>() + view.tail + bottom_padding;
        // The wheel moves the view in this pass; egui would apply it after
        // drawing, a frame late.
        let mut offset = view.offset;
        let mut scrolled = false;
        let wheel = if ui.rect_contains_pointer(rect) && ui.ctx().dragged_id().is_none() {
            ui.input(|i| i.smooth_scroll_delta.y)
        } else {
            0.0
        };
        if wheel != 0.0 {
            ui.input_mut(|i| i.smooth_scroll_delta.y = 0.0);
            let max = (content_height - viewport_height).max(0.0);
            offset = (offset - wheel).clamp(0.0, max);
            scrolled = true;
        } else if view.follow && !keep_scroll {
            offset = (content_height - viewport_height).max(0.0);
        }
        let mut drawn = Vec::new();
        let mut used = offset;
        // Height changes of rows above the viewport, and whether a row cut
        // by its top edge changed (what is on screen moved).
        let mut shift = 0.0;
        let mut cut_changed = false;
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
                    for ((row, key), guess) in rows.iter().zip(&keys).zip(guesses.iter().copied()) {
                        let top = ui.cursor().top() - origin;
                        let near = top + guess >= viewport.top() - overscan
                            && top <= viewport.bottom() + overscan;
                        let revealing = matches!(
                            (row, &reveal),
                            (Row::Assistant { message, .. }, Some(id)) if &message.id == id
                        );
                        if !(near || select_all || revealing || view.pinned.contains(key)) {
                            ui.add_space(guess);
                            continue;
                        }
                        let before = ui.cursor().top();
                        self.row(ui, row, column);
                        let height = ui.cursor().top() - before;
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
                    // Queued follow-ups close the timeline.
                    let before = ui.cursor().top();
                    self.queued_rows(ui, &detail, column);
                    view.tail = ui.cursor().top() - before;
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
        let misaligned = view.follow && ((content - view.viewport).max(0.0) - used).abs() > 0.5;
        if misaligned || cut_changed {
            ui.ctx().request_discard("timeline row heights");
        }
        view.offset = next;
        self.timeline_view = view;
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
                run_id,
                label,
                expanded,
                ..
            } => {
                if self.fold_row(ui, label, *expanded, width).clicked() {
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
                None => self.status = Some("No installed editors found".into()),
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
        // Image attachments sit above the text (`mb-2`).
        let images = attachments::images(&message.attachments);
        let (grid_height, cell) = attachments::grid_size(images.len(), max_width - 24.0);
        let grid = if images.is_empty() {
            Vec2::ZERO
        } else {
            Vec2::new(2.0 * cell.x + 8.0, grid_height + 8.0)
        };
        let bubble_size = Vec2::new(text_size.x.max(grid.x) + 24.0, grid.y + text_size.y + 24.0);
        let (row_rect, response) =
            ui.allocate_exact_size(Vec2::new(width, bubble_size.y + 4.0 + 24.0), Sense::hover());
        let bubble = Rect::from_min_size(
            Pos2::new(row_rect.right() - bubble_size.x, row_rect.top()),
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
        let text_pos = bubble.min + Vec2::new(12.0, 12.0 + grid.y);
        if !text.is_empty() {
            let body = Rect::from_min_size(text_pos, Vec2::new(measured.x.max(1.0), body_height));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
                text_pos,
                Vec2::new(measured.x.max(1.0) + 1.0, f32::INFINITY),
            )));
            child.set_clip_rect(body.expand2(Vec2::new(2.0, 0.0)).intersect(ui.clip_rect()));
            let workspace = self.workspace_cwd();
            let mut markdown = Markdown::new(
                &mut self.highlighter,
                ui.id().with(("user-text", &message.id)),
            );
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
                    .filter(|r| r["kind"] == "terminal")
                    .cloned();
                if let (Some(rect), Some(record)) = (rect, record) {
                    self.bubble_popover = Some((rect, record));
                }
            }
        }
        if response.hovered() || ui.rect_contains_pointer(row_rect) {
            let meta_top = bubble.bottom() + 4.0;
            let copy = Rect::from_min_size(
                Pos2::new(row_rect.right() - 4.0 - 24.0, meta_top),
                Vec2::splat(24.0),
            );
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
            let highlight = self.citation_highlight(ui.ctx(), &message.id);
            let workspace = self.workspace_cwd();
            let plugin = ui.ctx().plugin::<widgets::CopyCleanup>();
            // The message's text can be cited (`data-assistant-citation-source`).
            plugin.lock().source = Some(message.id.clone());
            let mut markdown = Markdown::new(
                &mut self.highlighter,
                ui.id().with(("markdown", &message.id)),
            );
            markdown.highlight = highlight;
            markdown.media_root = workspace;
            markdown.media_thread = self.selected.clone();
            markdown.show(ui, text, width - 8.0);
            plugin.lock().source = None;
            let opened_file = markdown.opened_file.take();
            let opened_citation = markdown.opened_citation.take();
            let asset_requests = std::mem::take(&mut markdown.asset_requests);
            let opened_media = markdown.opened_media.take();
            let media_menu = markdown.media_menu.take();
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
                let copy = Rect::from_min_size(
                    Pos2::new(meta.left() + 4.0, meta.top()),
                    Vec2::splat(24.0),
                );
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
