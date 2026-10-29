//! The sidebar's unsent drafts (`SidebarDraftBlock`, `SidebarDraftRow`,
//! `buildDraftActionMenuItems`): new-thread drafts left with content, above
//! the pinned threads, newest first, then a divider. A row opens its draft
//! where it started; its menu copies the path or branch, or discards it.

use eframe::egui::{self, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::drafts::DraftRow;
use crate::ui::{project_icon, widgets};

/// A row box (as a thread card's) and the divider under the block.
pub(crate) const DRAFT_SLOT: f32 = 82.0;
pub(crate) const DRAFT_DIVIDER: f32 = 13.0;

impl App {
    /// The block's height for `rows`.
    pub(crate) fn draft_block_height(rows: &[DraftRow]) -> f32 {
        if rows.is_empty() {
            0.0
        } else {
            rows.len() as f32 * DRAFT_SLOT + DRAFT_DIVIDER
        }
    }

    /// Draws the rows from `top` in `area`; returns the block's bottom.
    pub(crate) fn draft_block(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        top: f32,
        rows: &[DraftRow],
    ) -> f32 {
        if rows.is_empty() {
            return top;
        }
        let mut y = top;
        for row in rows {
            let rect = Rect::from_min_size(
                Pos2::new(area.left() + 8.0, y + 2.0),
                Vec2::new(area.width() - 16.0, DRAFT_SLOT - 4.0),
            );
            if ui.is_rect_visible(rect) {
                self.draft_row(ui, rect, row);
            }
            y += DRAFT_SLOT;
        }
        // `mx-2.5 my-1.5 h-px bg-sidebar-border/60`.
        let line = Rect::from_min_size(
            Pos2::new(area.left() + 10.0, y + 6.0),
            Vec2::new(area.width() - 20.0, 1.0),
        );
        ui.painter()
            .rect_filled(line, 0.0, theme::alpha(theme::sidebar_border(), 0.6));
        y + DRAFT_DIVIDER
    }

    fn draft_row(&mut self, ui: &mut Ui, rect: Rect, row: &DraftRow) {
        let response = ui.interact(rect, ui.id().with(("draft-row", &row.key)), Sense::click());
        let project = row
            .session
            .project_id
            .as_ref()
            .and_then(|id| self.state.project(id).cloned());
        let project_name = project
            .as_ref()
            .map(|p| p.title.clone())
            .unwrap_or_default();
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("{}, Unsent draft, {project_name}", row.preview),
            )
        });
        let open = self.selected.is_none() && self.draft_key == row.key;
        let hovered = response.contains_pointer();
        let fill = if open {
            theme::row_active()
        } else {
            theme::alpha(theme::warning(), if hovered { 0.08 } else { 0.04 })
        };
        ui.painter().rect_filled(rect, widgets::radius(8.0), fill);
        let x = rect.left() + 10.0;
        let top = rect.top() + 8.0;
        theme::paint_icon(
            ui,
            "square-pen",
            Rect::from_min_size(Pos2::new(x, top + 4.0), Vec2::splat(12.0)),
            12.0,
            theme::warning_foreground(),
        );
        let mut left = x + 12.0 + 6.0;
        if let Some(project) = &project {
            project_icon::paint(
                ui,
                Rect::from_min_size(Pos2::new(left, top + 2.0), Vec2::splat(16.0)),
                project,
            );
            left += 16.0 + 6.0;
        }
        let right = rect.right() - 10.0;
        // The discard button shows on hover.
        let discard = Rect::from_min_size(Pos2::new(right - 20.0, top), Vec2::splat(20.0));
        let mut discarded = false;
        if hovered {
            discarded = widgets::icon_button(
                ui,
                discard,
                ("draft-discard", &row.key),
                "x",
                12.0,
                theme::muted_foreground(),
                "Discard draft",
            )
            .clicked();
        }
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(left, top),
                Pos2::new(discard.left() - 6.0, top + 20.0),
            ),
            &project_name,
            font(12.0, Weight::Medium),
            theme::secondary_label(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(x, top + 22.0),
                Pos2::new(right, top + 22.0 + 20.0),
            ),
            &row.preview,
            font(14.0, Weight::Medium),
            theme::alpha(theme::foreground(), 0.9),
        );
        if discarded {
            self.discard_draft_session(&row.key);
        } else if response.secondary_clicked() {
            let at = ui
                .input(|i| i.pointer.interact_pos())
                .unwrap_or(rect.left_bottom());
            self.draft_menu = Some((row.key.clone(), crate::ui::context_menu::State::new(at)));
        } else if response.clicked() {
            self.open_draft_session(&row.key);
        }
    }

    /// The draft row's menu: Copy (Path, Branch), Discard draft.
    pub(crate) fn draft_row_menu(&mut self, ui: &mut Ui) {
        use crate::ui::context_menu::{self, Item};
        let Some((key, state)) = self.draft_menu.take() else {
            return;
        };
        let Some(row) = self.draft_rows(None).into_iter().find(|r| r.key == key) else {
            return;
        };
        let path = row.session.worktree.clone().or_else(|| {
            row.session
                .project_id
                .as_ref()
                .and_then(|id| self.state.project(id))
                .map(|p| p.workspace_root.clone())
        });
        let mut copy = Vec::new();
        if path.is_some() {
            copy.push(Item::new("copy-path", "Path").icon("folder"));
        }
        if row.session.branch.is_some() {
            copy.push(Item::new("copy-branch", "Branch").icon("git-branch"));
        }
        let items = vec![
            Item {
                disabled: copy.is_empty(),
                children: copy,
                ..Item::new("copy", "Copy").icon("copy")
            },
            Item {
                destructive: true,
                ..Item::new("discard", "Discard draft")
                    .icon("trash")
                    .separated()
            },
        ];
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.draft_menu = Some((key.clone(), state));
        }
        match clicked.as_deref() {
            Some("copy-path") => ui.ctx().copy_text(path.unwrap_or_default()),
            Some("copy-branch") => ui.ctx().copy_text(row.session.branch.unwrap_or_default()),
            Some("discard") => {
                self.draft_menu = None;
                self.discard_draft_session(&key);
            }
            _ => {}
        }
    }
}
