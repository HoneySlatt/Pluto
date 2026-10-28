//! "New design" (Pluto, not upstream): the entry screen of a design session,
//! after OpenDesign's Home (nexu-io/open-design): a brief, the kind of
//! deliverable, the agent, the project folder and example briefs. Designs
//! share one T3 project, Designs (`~/Designs`, created by the first one with
//! `project.create`): each works in its own folder there, its thread's
//! directory (`existing_worktree`), with its design profile
//! (`.pluto/design.json`, which gives its sessions the server's design
//! charter). A folder chosen elsewhere becomes a project of its own, as one
//! that already is a project stays. The thread starts with the brief and
//! opens in the studio layout (`crate::ui::design`). The folder the screen
//! proposed also gets a Git repository (`vcs.init`), so T3 checkpoints each
//! turn and the Design surface can show earlier versions; a folder the user
//! chose is left as it is. References (images and files,
//! pasted, dropped or picked) go with the brief, through the composer's
//! attachments and launch (`start_turn`).

use std::sync::mpsc::Receiver;

use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::ModelSelection;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const WIDTH: f32 = 720.0;
const MODEL_MENU: &str = "design-home-model";
const TRAITS_MENU: &str = "design-home-traits";
const KIND_MENU: &str = "design-home-kind";
const PROFILE_PATH: &str = ".pluto/design.json";
/// The Designs project's folder, which holds every design's folder.
const DESIGNS_ROOT: &str = "~/Designs";

/// The kinds of deliverable, as the server's `DESIGN_KIND_RULES` keys.
const KINDS: [(&str, &str, &str); 6] = [
    ("prototype", "Prototype", "monitor"),
    ("landing", "Landing page", "panels-top-left"),
    ("mobile", "Mobile app", "smartphone"),
    ("dashboard", "Dashboard", "chart-no-axes-column"),
    ("deck", "Slides", "layers"),
    ("document", "Document", "file-text"),
];

/// Example briefs: title, brief, kind index.
const EXAMPLES: [(&str, &str, usize); 4] = [
    (
        "Product pricing page",
        "Design a pricing page for a note-taking app with three plans, a monthly and yearly switch, a plan comparison table and a short FAQ.",
        1,
    ),
    (
        "Habit tracker app",
        "Design the main screens of a habit tracker app: today's habits with streaks, a habit's detail with its history, and adding a new habit.",
        2,
    ),
    (
        "Order operations dashboard",
        "Design a dashboard for tracking e-commerce orders: today's key numbers, an orders table with filters and statuses, and a chart of orders per hour.",
        3,
    ),
    (
        "Quarterly review deck",
        "Design an 8-slide quarterly review for a small product team: highlights, metrics, what shipped, what slipped, and next quarter's goals.",
        4,
    ),
];

#[derive(Default)]
pub(crate) struct DesignHome {
    pub(crate) open: bool,
    brief: String,
    kind: usize,
    folder: String,
    /// The folder was typed or picked, so the brief no longer names it.
    folder_edited: bool,
    model: Option<ModelSelection>,
    picking: Option<Receiver<Option<String>>>,
    creating: Option<Creating>,
    error: Option<String>,
}

/// A design being created, waiting on `stage`.
struct Creating {
    project_id: String,
    /// The design's folder: the thread's directory.
    cwd: String,
    /// The folder within the project's (the Designs project's) root, for a
    /// design that does not work at the root.
    within: Option<(String, String)>,
    title: String,
    brief: String,
    model: Value,
    stage: Stage,
    /// Initialize Git once the profile is written.
    init_git: bool,
    /// The references: images to persist, and uploaded files.
    uploads: Vec<Value>,
    files: Vec<Value>,
}

#[derive(PartialEq)]
enum Stage {
    /// `project.create`, by command id.
    Project(String),
    /// `projects.writeFile` of the profile.
    Profile,
    /// `vcs.init` of a new folder.
    Git,
}

/// A folder name from the brief's first words.
fn slug(brief: &str) -> String {
    let words: Vec<String> = brief
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(5)
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        "untitled-design".into()
    } else {
        words.join("-")
    }
}

/// `~/` expanded against this machine's home; the server runs here.
fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{}/{rest}", home.trim_end_matches('/')),
        _ => path.to_owned(),
    }
}

impl App {
    pub(crate) fn open_design_home(&mut self) {
        self.design_home.open = true;
        self.design_home.error = None;
        self.usage_open = false;
        self.pull_requests_open = false;
    }

    /// The New design screen in the main area.
    pub(crate) fn design_home_page(&mut self, ui: &mut Ui, area: Rect) {
        ui.painter().rect_filled(area, 0.0, theme::background());
        self.take_incoming_images();
        if let Some(picked) = &self.design_home.picking
            && let Ok(picked) = picked.try_recv()
        {
            self.design_home.picking = None;
            if let Some(folder) = picked {
                self.design_home.folder = folder;
                self.design_home.folder_edited = true;
            }
        }
        if !self.design_home.folder_edited {
            self.design_home.folder = format!("~/Designs/{}", slug(&self.design_home.brief));
        }
        let width = WIDTH.min(area.width() - 48.0);
        let left = (area.center().x - width / 2.0).round();
        let mut y = (area.top() + area.height() * 0.14).max(area.top() + 48.0);
        let kind = KINDS[self.design_home.kind];

        // "Let's create" and the kind.
        let title = format!("Let's create a {}", kind.1.to_lowercase());
        let title_font = font(28.0, Weight::SemiBold);
        let title_width = widgets::text_width(ui, &title, title_font.clone());
        widgets::text_in(
            ui,
            Rect::from_min_size(
                Pos2::new(area.center().x - title_width / 2.0, y),
                Vec2::new(title_width + 1.0, 36.0),
            ),
            &title,
            title_font,
            theme::foreground(),
        );
        y += 40.0;
        let subtitle =
            "Describe it. The agent designs it in a folder of HTML files you watch live.";
        let sub_width = widgets::text_width(ui, subtitle, font(14.0, Weight::Regular));
        widgets::text_in(
            ui,
            Rect::from_min_size(
                Pos2::new(area.center().x - sub_width / 2.0, y),
                Vec2::new(sub_width + 1.0, 20.0),
            ),
            subtitle,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
        );
        y += 36.0;

        // The brief card: the text, the references, the kinds, the agent and Create.
        let strip = self.image_strip_height(width - 32.0 + 34.0);
        let card = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 176.0 + strip));
        ui.painter().rect(
            card,
            CornerRadius::same(16),
            theme::card(),
            Stroke::new(1.0, theme::border()),
            StrokeKind::Inside,
        );
        let editor = Rect::from_min_max(
            card.min + Vec2::new(16.0, 14.0),
            Pos2::new(card.right() - 16.0, card.bottom() - 56.0 - strip),
        );
        let placeholder = EXAMPLES
            .iter()
            .find(|e| e.2 == self.design_home.kind)
            .map_or("Describe what to design…", |e| e.1);
        let response = ui.put(
            editor,
            egui::TextEdit::multiline(&mut self.design_home.brief)
                .id_salt("design-home-brief")
                .frame(egui::Frame::NONE)
                .font(font(15.0, Weight::Regular))
                .text_color(theme::foreground())
                .hint_text(
                    egui::RichText::new(placeholder)
                        .font(font(15.0, Weight::Regular))
                        .color(theme::muted_foreground()),
                )
                .desired_width(editor.width()),
        );
        if self.design_home.brief.is_empty() && !response.has_focus() {
            response.request_focus();
        }
        // The references below the brief, as in the composer.
        if strip > 0.0 {
            self.composer_image_strip(
                ui,
                Pos2::new(editor.left(), editor.bottom() + 2.0),
                editor.width(),
            );
        }
        // An image paste: egui only pastes text, but reports the V key's release.
        let pasted = response.has_focus()
            && ui.input(|i| {
                i.events.iter().any(|event| {
                    matches!(event, egui::Event::Key { key: egui::Key::V, pressed: false, modifiers, .. }
                        if modifiers.command)
                })
            });
        if pasted {
            crate::ui::attachments::paste_clipboard_image(&self.incoming_images, ui.ctx());
        }
        // Files dropped anywhere on the screen.
        let dropped = ui.input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() {
            crate::ui::attachments::add_dropped_files(&self.incoming_images, ui.ctx(), dropped);
        }
        let row_y = card.bottom() - 44.0;
        // The agent's button comes first: the kinds take the room it leaves.
        let selection = self
            .design_home
            .model
            .clone()
            .unwrap_or_else(|| self.composer_model_selection(true));
        let (model_label, model_icon) = self.model_label(&selection);
        let label_width = widgets::text_width(ui, &model_label, font(13.0, Weight::Medium));
        // The kind of deliverable: one chip and its menu (OpenDesign's mode chip).
        let (_, kind_label, kind_icon) = KINDS[self.design_home.kind];
        let kind_width = widgets::text_width(ui, kind_label, font(13.0, Weight::Medium));
        let chip = Rect::from_min_size(
            Pos2::new(card.left() + 12.0, row_y),
            Vec2::new(
                (10.0 + 16.0 + 6.0 + kind_width + 6.0 + 12.0 + 10.0).round(),
                30.0,
            ),
        );
        let chip_response = ui.interact(chip, ui.id().with("design-kind"), Sense::click());
        ui.painter().rect(
            chip,
            CornerRadius::same(15),
            if chip_response.hovered() || self.open_menu.as_deref() == Some(KIND_MENU) {
                theme::accent()
            } else {
                egui::Color32::TRANSPARENT
            },
            Stroke::new(1.0, theme::border()),
            StrokeKind::Inside,
        );
        theme::paint_icon(
            ui,
            kind_icon,
            Rect::from_min_size(
                Pos2::new(chip.left() + 10.0, chip.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            16.0,
            theme::foreground(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(chip.left() + 32.0, chip.top()), chip.max),
            kind_label,
            font(13.0, Weight::Medium),
            theme::foreground(),
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(chip.right() - 22.0, chip.center().y - 6.0),
                Vec2::splat(12.0),
            ),
            12.0,
            theme::muted_foreground(),
        );
        if chip_response.clicked() {
            self.open_menu = if self.open_menu.as_deref() == Some(KIND_MENU) {
                None
            } else {
                Some(KIND_MENU.into())
            };
        }
        let entries: Vec<widgets::MenuEntry> = KINDS
            .iter()
            .map(|(_, label, icon)| {
                widgets::MenuEntry::Item(widgets::MenuItem {
                    icon: Some(icon),
                    icon_size: 16.0,
                    ..widgets::MenuItem::new(label)
                })
            })
            .collect();
        if let Some(index) =
            widgets::menu(ui, &mut self.open_menu, KIND_MENU, chip, false, &entries)
        {
            self.design_home.kind = index;
        }
        // Attach references (the composer's paperclip).
        let attach = Rect::from_min_size(Pos2::new(chip.right() + 6.0, row_y), Vec2::splat(30.0));
        let attach_response = ui
            .interact(attach, ui.id().with("design-attach"), Sense::click())
            .on_hover_text("Attach images or files as references");
        if attach_response.hovered() {
            ui.painter()
                .rect_filled(attach, CornerRadius::same(8), theme::accent());
        }
        theme::paint_icon(
            ui,
            "paperclip",
            Rect::from_center_size(attach.center(), Vec2::splat(16.0)),
            16.0,
            if attach_response.hovered() {
                theme::foreground()
            } else {
                theme::muted_foreground()
            },
        );
        if attach_response.clicked() {
            crate::ui::attachments::pick_files(&self.incoming_images, ui.ctx());
        }
        // Create.
        let create = Rect::from_min_size(
            Pos2::new(card.right() - 12.0 - 32.0, row_y - 1.0),
            Vec2::splat(32.0),
        );
        let ready = !self.design_home.brief.trim().is_empty()
            && !self.design_home.folder.trim().is_empty()
            && self.design_home.creating.is_none();
        let create_response = ui.interact(
            create,
            ui.id().with("design-create"),
            if ready {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        ui.painter().circle_filled(
            create.center(),
            16.0,
            if ready {
                theme::primary()
            } else {
                theme::alpha(theme::primary(), 0.4)
            },
        );
        theme::paint_icon(
            ui,
            if self.design_home.creating.is_some() {
                "loader-circle"
            } else {
                "arrow-up"
            },
            create,
            16.0,
            theme::primary_foreground(),
        );
        // The model's thinking level (`TraitsPicker`), left of the button.
        let traits_label = self.traits_label(&selection);
        let traits_button = traits_label.as_ref().map(|label| {
            let width = widgets::text_width(ui, label, font(13.0, Weight::Medium)) + 34.0;
            Rect::from_min_size(
                Pos2::new(create.left() - 8.0 - width, row_y),
                Vec2::new(width, 30.0),
            )
        });
        let agent_right = traits_button.map_or(create.left() - 8.0, |rect| rect.left() - 4.0);
        // The agent: the model picker, below the button.
        let model_button = Rect::from_min_size(
            Pos2::new(agent_right - (label_width + 46.0), row_y),
            Vec2::new(label_width + 46.0, 30.0),
        );
        let model_response =
            ui.interact(model_button, ui.id().with("design-model"), Sense::click());
        if model_response.hovered() {
            ui.painter()
                .rect_filled(model_button, CornerRadius::same(8), theme::accent());
        }
        if let Some(icon) = model_icon {
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(
                    Pos2::new(model_button.left() + 8.0, model_button.center().y - 8.0),
                    Vec2::splat(16.0),
                ),
                16.0,
                theme::foreground(),
            );
        }
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(model_button.left() + 28.0, model_button.top()),
                model_button.max,
            ),
            &model_label,
            font(13.0, Weight::Medium),
            theme::foreground(),
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(model_button.right() - 16.0, model_button.center().y - 6.0),
                Vec2::splat(12.0),
            ),
            12.0,
            theme::muted_foreground(),
        );
        if model_response.clicked() {
            self.open_menu = if self.open_menu.as_deref() == Some(MODEL_MENU) {
                None
            } else {
                Some(MODEL_MENU.into())
            };
        }
        if let Some(chosen) =
            self.model_picker_popup(ui, model_button, MODEL_MENU, selection.clone(), true, false)
        {
            self.design_home.model = Some(chosen);
        }
        if let (Some(rect), Some(label)) = (traits_button, &traits_label) {
            let response = ui.interact(rect, ui.id().with("design-traits"), Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), theme::accent());
            }
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(rect.left() + 10.0, rect.top()), rect.max),
                label,
                font(13.0, Weight::Medium),
                theme::foreground(),
            );
            theme::paint_icon(
                ui,
                "chevron-down",
                Rect::from_min_size(
                    Pos2::new(rect.right() - 18.0, rect.center().y - 6.0),
                    Vec2::splat(12.0),
                ),
                12.0,
                theme::muted_foreground(),
            );
            if response.clicked() {
                self.open_menu = if self.open_menu.as_deref() == Some(TRAITS_MENU) {
                    None
                } else {
                    Some(TRAITS_MENU.into())
                };
            }
            if let Some(next) = self.traits_menu_popup(ui, rect, TRAITS_MENU, &selection, false) {
                self.design_home.model = Some(next);
            }
        }
        // How the agent may act (`RuntimeModePicker`), left of the model: the
        // draft composer's mode, which the launch takes.
        let runtime = self.composer_runtime_mode(true);
        let (_, runtime_label, runtime_icon, _) = crate::ui::pickers::RUNTIME_MODES
            .iter()
            .find(|(mode, ..)| *mode == runtime)
            .copied()
            .unwrap_or(crate::ui::pickers::RUNTIME_MODES[0]);
        let runtime_width =
            widgets::text_width(ui, runtime_label, font(13.0, Weight::Medium)) + 54.0;
        let runtime_button = Rect::from_min_size(
            Pos2::new(model_button.left() - 4.0 - runtime_width, row_y),
            Vec2::new(runtime_width, 30.0),
        );
        let runtime_response = ui.interact(
            runtime_button,
            ui.id().with("design-runtime"),
            Sense::click(),
        );
        if runtime_response.hovered() {
            ui.painter()
                .rect_filled(runtime_button, CornerRadius::same(8), theme::accent());
        }
        theme::paint_icon(
            ui,
            runtime_icon,
            Rect::from_min_size(
                Pos2::new(runtime_button.left() + 8.0, runtime_button.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            16.0,
            theme::foreground(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(runtime_button.left() + 30.0, runtime_button.top()),
                runtime_button.max,
            ),
            runtime_label,
            font(13.0, Weight::Medium),
            theme::foreground(),
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(
                    runtime_button.right() - 18.0,
                    runtime_button.center().y - 6.0,
                ),
                Vec2::splat(12.0),
            ),
            12.0,
            theme::muted_foreground(),
        );
        if runtime_response.clicked() {
            self.open_menu = if self.open_menu.as_deref() == Some("runtime-picker") {
                None
            } else {
                Some("runtime-picker".into())
            };
        }
        self.runtime_menu(ui, runtime_button, true);
        let submit = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        if ready && (create_response.clicked() || submit) {
            self.create_design(&selection);
        }
        y = card.bottom() + 12.0;

        // The folder.
        let folder_row =
            Rect::from_min_size(Pos2::new(left + 12.0, y), Vec2::new(width - 24.0, 28.0));
        theme::paint_icon(
            ui,
            "folder",
            Rect::from_min_size(
                Pos2::new(folder_row.left(), folder_row.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            16.0,
            theme::muted_foreground(),
        );
        let browse_width = 88.0;
        let field = Rect::from_min_max(
            Pos2::new(folder_row.left() + 24.0, folder_row.top() + 4.0),
            Pos2::new(folder_row.right() - browse_width - 8.0, folder_row.bottom()),
        );
        let before = self.design_home.folder.clone();
        ui.put(
            field,
            egui::TextEdit::singleline(&mut self.design_home.folder)
                .id_salt("design-home-folder")
                .frame(egui::Frame::NONE)
                .font(font(13.0, Weight::Regular))
                .text_color(theme::muted_foreground())
                .desired_width(field.width()),
        );
        if self.design_home.folder != before {
            self.design_home.folder_edited = true;
        }
        if self.server_is_local() {
            let browse = Rect::from_min_size(
                Pos2::new(folder_row.right() - browse_width, folder_row.top()),
                Vec2::new(browse_width, 28.0),
            );
            let response = ui.interact(browse, ui.id().with("design-browse"), Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(browse, CornerRadius::same(8), theme::accent());
            }
            widgets::text_in(
                ui,
                browse.shrink2(Vec2::new(10.0, 0.0)),
                "Browse…",
                font(13.0, Weight::Medium),
                theme::muted_foreground(),
            );
            if response.clicked() && self.design_home.picking.is_none() {
                self.design_home.picking = Some(pick_folder(ui.ctx()));
            }
        }
        y = folder_row.bottom() + 8.0;
        if let Some(error) = &self.design_home.error {
            widgets::text_in(
                ui,
                Rect::from_min_size(Pos2::new(left + 12.0, y), Vec2::new(width - 24.0, 20.0)),
                error,
                font(13.0, Weight::Regular),
                theme::error_foreground(),
            );
        }
        y += 32.0;

        // Example briefs.
        for (index, (title, brief, kind)) in EXAMPLES.iter().enumerate() {
            let row = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 56.0));
            y += 60.0;
            if row.bottom() > area.bottom() - 8.0 {
                break;
            }
            let response =
                ui.interact(row, ui.id().with(("design-example", index)), Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(row, CornerRadius::same(10), theme::overlay(0.03));
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            theme::paint_icon(
                ui,
                KINDS[*kind].2,
                Rect::from_min_size(
                    Pos2::new(row.left() + 12.0, row.top() + 10.0),
                    Vec2::splat(16.0),
                ),
                16.0,
                theme::muted_foreground(),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(row.left() + 40.0, row.top() + 6.0),
                    Pos2::new(row.right() - 12.0, row.top() + 26.0),
                ),
                title,
                font(14.0, Weight::Medium),
                theme::foreground(),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(row.left() + 40.0, row.top() + 28.0),
                    Pos2::new(row.right() - 12.0, row.top() + 48.0),
                ),
                brief,
                font(13.0, Weight::Regular),
                theme::muted_foreground(),
            );
            if response.clicked() {
                self.design_home.brief = (*brief).to_owned();
                self.design_home.kind = *kind;
            }
        }
    }

    /// Starts creating the design: the project first, unless the folder
    /// already is one.
    fn create_design(&mut self, selection: &ModelSelection) {
        // The references go with the brief once their files are uploaded.
        let files = match self.file_attachments() {
            Ok(files) => files,
            Err(reason) => {
                self.design_home.error = Some(reason);
                return;
            }
        };
        let uploads = crate::ui::attachments::upload_payload(&self.composer_images);
        self.composer_images.clear();
        self.composer_files.clear();
        let home = &mut self.design_home;
        let cwd = expand_home(home.folder.trim().trim_end_matches('/'));
        let brief = home.brief.trim().to_owned();
        let title: String = brief
            .lines()
            .next()
            .unwrap_or("Design")
            .chars()
            .take(60)
            .collect();
        let mut model = json!({"instanceId": selection.instance_id, "model": selection.model});
        if let Some(options) = &selection.options {
            model["options"] = options.clone();
        }
        home.error = None;
        let proposed = !home.folder_edited;
        let project_at = |root: &str| {
            self.state
                .projects
                .iter()
                .find(|p| p.workspace_root.trim_end_matches('/') == root)
                .map(|p| p.id.clone())
        };
        let mut cwd = cwd;
        let mut existing = project_at(&cwd);
        // A folder under Designs works there, in the Designs project; the
        // proposed one skips the folders other designs already use.
        let designs = expand_home(DESIGNS_ROOT);
        let within = if existing.is_none()
            && cwd
                .strip_prefix(&format!("{designs}/"))
                .is_some_and(|rest| !rest.is_empty())
        {
            if proposed {
                let used = |path: &str| {
                    self.state
                        .threads
                        .iter()
                        .any(|t| t.worktree_path.as_deref() == Some(path))
                };
                let base = cwd.clone();
                let mut n = 2;
                while used(&cwd) {
                    cwd = format!("{base}-{n}");
                    n += 1;
                }
            }
            existing = project_at(&designs);
            let rest = cwd[designs.len() + 1..].to_owned();
            Some((designs.clone(), rest))
        } else {
            None
        };
        let creating = |project_id: String, stage, init_git| Creating {
            project_id,
            cwd: cwd.clone(),
            within: within.clone(),
            title: title.clone(),
            brief: brief.clone(),
            model: model.clone(),
            stage,
            init_git,
            uploads: uploads.clone(),
            files: files.clone(),
        };
        match existing {
            Some(project_id) => {
                // A new folder in Designs is versioned; a folder that already
                // is a project is left as it is.
                let init_git = proposed && within.is_some();
                self.design_home.creating = Some(creating(project_id, Stage::Profile, init_git));
                self.write_design_profile();
            }
            None => {
                let project_id = uuid::Uuid::new_v4().to_string();
                let command_id = uuid::Uuid::new_v4().to_string();
                let root = within
                    .as_ref()
                    .map_or(cwd.clone(), |(root, _)| root.clone());
                let name = root
                    .rsplit('/')
                    .find(|s| !s.is_empty())
                    .unwrap_or(&root)
                    .to_owned();
                self.design_home.creating = Some(creating(
                    project_id.clone(),
                    Stage::Project(command_id.clone()),
                    proposed,
                ));
                self.dispatch(json!({
                    "type": "project.create",
                    "commandId": command_id,
                    "projectId": project_id,
                    "title": name,
                    "workspaceRoot": root,
                    "createWorkspaceRootIfMissing": true,
                    "defaultModelSelection": null,
                }));
            }
        }
    }

    fn write_design_profile(&mut self) {
        let Some(creating) = &self.design_home.creating else {
            return;
        };
        let kind = KINDS[self.design_home.kind].0;
        let contents = format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({"version": 1, "kind": kind})).unwrap_or_default()
        );
        // A folder within the project is written from its root, which
        // creates it.
        let (root, path) = match &creating.within {
            Some((root, rest)) => (root.clone(), format!("{rest}/{PROFILE_PATH}")),
            None => (creating.cwd.clone(), PROFILE_PATH.to_owned()),
        };
        self.send_request(
            format!("design-profile-write:{}", creating.cwd),
            "projects.writeFile",
            json!({"cwd": root, "relativePath": path, "contents": contents}),
        );
    }

    /// The creation steps' answers; true when `key` was one of them.
    pub(crate) fn design_home_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        let Some(creating) = &self.design_home.creating else {
            return false;
        };
        let ours = match &creating.stage {
            Stage::Project(command) => key == format!("project-mutation:{command}"),
            Stage::Profile => key == format!("design-profile-write:{}", creating.cwd),
            Stage::Git => key == format!("design-git-init:{}", creating.cwd),
        };
        if !ours {
            return false;
        }
        // Without Git the design still works, only without versions.
        if creating.stage != Stage::Git
            && let Err(error) = result
        {
            self.design_home.creating = None;
            self.design_home.error = Some(format!("Could not create the design: {error}"));
            return true;
        }
        if matches!(creating.stage, Stage::Project(_)) {
            if let Some(creating) = &mut self.design_home.creating {
                creating.stage = Stage::Profile;
            }
            self.write_design_profile();
            return true;
        }
        if creating.stage == Stage::Profile && creating.init_git {
            let cwd = creating.cwd.clone();
            if let Some(creating) = &mut self.design_home.creating {
                creating.stage = Stage::Git;
            }
            self.send_request(
                format!("design-git-init:{cwd}"),
                "vcs.init",
                json!({"cwd": cwd}),
            );
            return true;
        }
        // The project is ready: the thread starts with the brief.
        let Some(creating) = self.design_home.creating.take() else {
            return true;
        };
        let thread_id = uuid::Uuid::new_v4().to_string();
        let runtime = self.composer_runtime_mode(true);
        let mut input = crate::ui::composer::launch_input(
            &thread_id,
            &creating.project_id,
            &creating.title,
            &creating.brief,
            &creating.model,
            &runtime,
            "default",
        );
        if creating.within.is_some() {
            input["workspaceStrategy"] =
                json!({"type": "existing_worktree", "worktreePath": creating.cwd});
        }
        // The composer's launch: the reference images are persisted first.
        let mut send = crate::ui::composer::TurnSend {
            thread_id: thread_id.clone(),
            before: Vec::new(),
            message: input,
            launch: true,
            uploads: creating.uploads,
        };
        send.set_attachments(Value::Array(creating.files));
        self.start_turn(send);
        self.mark_design_project(&creating.cwd);
        self.design_home = DesignHome::default();
        self.select_thread(Some(thread_id));
        true
    }
}

/// The portal's folder chooser, off the UI thread.
fn pick_folder(ctx: &egui::Context) -> Receiver<Option<String>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let mut dialog = rfd::FileDialog::new().set_title("Choose the design's folder");
        if let Ok(home) = std::env::var("HOME") {
            dialog = dialog.set_directory(home);
        }
        let picked = dialog
            .pick_folder()
            .and_then(|path| path.to_str().map(str::to_owned));
        let _ = sender.send(picked);
        ctx.request_repaint();
    });
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_follow_the_brief() {
        assert_eq!(
            slug("Design a pricing page, for notes!"),
            "design-a-pricing-page-for"
        );
        assert_eq!(slug("   "), "untitled-design");
    }
}
