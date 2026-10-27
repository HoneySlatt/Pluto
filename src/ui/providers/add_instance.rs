//! "Add provider instance": the three-step wizard (Driver, Identity, Config)
//! of `components/settings/AddProviderInstanceDialog.tsx`, its navigation
//! rules (`AddProviderInstanceDialog.logic.ts`) and the step strip of
//! `components/ui/wizard.tsx`.
//!
//! "Continue with ChatGPT" (the managed Codex account sign-in) is not ported.

use std::collections::HashMap;

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Map, Value, json};

use super::{DRIVERS, driver_fields, driver_meta};
use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::accent_picker::{self, AccentPicker, PickerEvent};
use crate::ui::widgets;

const WIDTH: f32 = 768.0;
const STEPS: [&str; 3] = ["Driver", "Identity", "Config"];
const IDENTITY_STEP: usize = 1;
/// `PROVIDER_ACCENT_SWATCHES`.
const SWATCHES: [&str; 6] = [
    "#2563eb", "#16a34a", "#ea580c", "#dc2626", "#7c3aed", "#0891b2",
];
/// `COMING_SOON_DRIVER_OPTIONS`: label, icon, whether the icon keeps its
/// colors, and its height in a 16px-wide box (the marks keep their ratio).
const COMING_SOON: [(&str, &str, bool, f32); 4] = [
    (
        "Github Copilot",
        "provider-copilot",
        false,
        16.0 * 208.0 / 256.0,
    ),
    ("Gemini", "provider-gemini", true, 16.0),
    ("ACP Registry", "provider-acp", false, 16.0 * 220.0 / 576.0),
    ("Pi Agent", "provider-pi", true, 16.0),
];

pub(crate) struct AddInstance {
    step: usize,
    driver: &'static str,
    label: String,
    accent: String,
    id_override: Option<String>,
    /// Config drafts per driver, so switching drivers keeps what was typed.
    config: HashMap<&'static str, Map<String, Value>>,
    /// Errors stay hidden until the first blocked step or save.
    attempted: bool,
    picker: Option<AccentPicker>,
}

impl Default for AddInstance {
    fn default() -> Self {
        Self {
            step: 0,
            driver: "codex",
            label: String::new(),
            accent: String::new(),
            id_override: None,
            config: HashMap::new(),
            attempted: false,
            picker: None,
        }
    }
}

/// `slugifyLabel`: lowercase, runs of other characters to `_`, trimmed, 48 max.
fn slugify(label: &str) -> String {
    let mut out = String::new();
    for c in label.trim().to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').chars().take(48).collect()
}

/// `deriveInstanceId`: `<driver>_<slug>`, or empty without a label.
fn derive_id(driver: &str, label: &str) -> String {
    let slug = slugify(label);
    if slug.is_empty() {
        String::new()
    } else {
        format!("{driver}_{slug}")
    }
}

/// `validateInstanceId`.
fn validate_id(id: &str, existing: &[String]) -> Option<String> {
    if id.is_empty() {
        return Some("Instance ID is required.".into());
    }
    if id.chars().count() > 64 {
        return Some("Instance ID must be 64 characters or fewer.".into());
    }
    let mut chars = id.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        return Some(
            "Instance ID must start with a letter and use only letters, digits, '-', or '_'."
                .into(),
        );
    }
    if existing.iter().any(|e| e == id) {
        return Some(format!("An instance named '{id}' already exists."));
    }
    None
}

/// `resolveWizardNavigation`: moving past Identity needs a valid id; a
/// blocked move lands on Identity. Returns the step and whether it was blocked.
fn navigate(current: usize, requested: usize, id_error: bool) -> (usize, bool) {
    let target = requested.min(STEPS.len() - 1);
    if current <= IDENTITY_STEP && target > IDENTITY_STEP && id_error {
        return (IDENTITY_STEP, true);
    }
    (target, false)
}

impl App {
    pub(crate) fn open_add_instance(&mut self) {
        self.add_instance = Some(AddInstance::default());
    }

    /// The wizard over `screen`.
    pub(crate) fn add_instance_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(mut dialog) = self.add_instance.take() else {
            return;
        };
        let existing: Vec<String> = self.provider_instances_map().keys().cloned().collect();
        let instance_id = dialog
            .id_override
            .clone()
            .unwrap_or_else(|| derive_id(dialog.driver, &dialog.label));
        let id_error = validate_id(&instance_id, &existing);
        let driver_label = driver_meta(dialog.driver).map_or(dialog.driver, |d| d.1);
        let fields = driver_fields(dialog.driver);
        let environment = self.environment_label();

        let panel_height = match dialog.step {
            // The driver grid: two columns of 44px cells, 12px apart.
            0 => {
                let rows = (DRIVERS.len() + COMING_SOON.len()).div_ceil(2) as f32;
                20.0 + 8.0 + rows * 44.0 + (rows - 1.0) * 12.0
            }
            // Label and Instance ID (80 each), then the accent row (76).
            1 => 80.0 + 80.0 + 76.0,
            _ if fields.is_empty() => 20.0,
            _ => fields.len() as f32 * 80.0 + (fields.len() as f32 - 1.0) * 16.0,
        };
        let height = 137.0 + 20.0 + panel_height + 20.0 + 16.0 + 32.0 + 17.0;
        let d = Rect::from_center_size(screen.center(), Vec2::new(WIDTH, height));
        let mut close = false;
        let mut save = false;
        let mut chatgpt = false;
        let mut step_request: Option<usize> = None;

        egui::Area::new(ui.id().with("add-instance-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if response.clicked() && !ui.rect_contains_pointer(d) && dialog.picker.is_none() {
                    close = true;
                }
                ui.painter().rect(
                    d,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let close_rect = Rect::from_min_size(
                    Pos2::new(d.right() - 41.0, d.top() + 9.0),
                    Vec2::splat(32.0),
                );
                if widgets::icon_button(
                    ui,
                    close_rect,
                    "add-instance-close",
                    "x",
                    16.0,
                    theme::muted_foreground(),
                    "",
                )
                .clicked()
                {
                    close = true;
                }
                let left = d.left() + 25.0;
                let right = d.right() - 25.0;
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    "Add provider instance",
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 53.0),
                    &format!("Add an account or configure a provider on {environment}."),
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                );
                if let Some(step) = step_strip(
                    ui,
                    Rect::from_min_size(
                        Pos2::new(left, d.top() + 81.0),
                        Vec2::new(right - left, 44.0),
                    ),
                    dialog.step,
                ) {
                    step_request = Some(step);
                }

                // The panel: `bg-white/2` between hairlines.
                let panel = Rect::from_min_size(
                    Pos2::new(d.left() + 1.0, d.top() + 137.0),
                    Vec2::new(WIDTH - 2.0, 20.0 + panel_height + 20.0),
                );
                ui.painter().rect_filled(panel, 0.0, theme::overlay(0.02));
                for y in [panel.top(), panel.bottom() - 1.0] {
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(panel.left(), y),
                            Vec2::new(panel.width(), 1.0),
                        ),
                        0.0,
                        theme::overlay(0.05),
                    );
                }
                let top = panel.top() + 20.0;
                match dialog.step {
                    0 => driver_step(self, ui, &mut dialog, left, right, top),
                    1 => identity_step(
                        ui,
                        &mut dialog,
                        left,
                        right,
                        top,
                        &instance_id,
                        id_error.as_deref(),
                        driver_label,
                    ),
                    _ => config_step(ui, &mut dialog, left, right, top, fields),
                }

                // Footer, right to left.
                let footer = panel.bottom() + 16.0;
                let mut x = right;
                let mut button = |ui: &mut Ui, label: &str, icon: Option<&str>, kind: Kind| {
                    let width = (widgets::text_width(ui, label, font(14.0, Weight::Medium))
                        + 24.0
                        + if icon.is_some() { 20.0 } else { 0.0 })
                    .ceil();
                    let rect =
                        Rect::from_min_size(Pos2::new(x - width, footer), Vec2::new(width, 32.0));
                    x = rect.left() - 8.0;
                    dialog_button(ui, rect, label, icon, kind)
                };
                match dialog.step {
                    0 => {
                        if dialog.driver == "codex" {
                            chatgpt =
                                button(ui, "Continue with ChatGPT", Some("openai"), Kind::Primary);
                            if button(ui, "Configure manually", None, Kind::Outline) {
                                step_request = Some(1);
                            }
                        } else if button(ui, "Next", None, Kind::Primary) {
                            step_request = Some(1);
                        }
                        close |= button(ui, "Cancel", None, Kind::GhostMuted);
                    }
                    step => {
                        if step < STEPS.len() - 1 {
                            if button(ui, "Next", None, Kind::Primary) {
                                step_request = Some(step + 1);
                            }
                        } else {
                            save = button(ui, "Add instance", None, Kind::Primary);
                        }
                        if button(ui, "Back", None, Kind::Outline) {
                            dialog.step -= 1;
                        }
                    }
                }
            });

        if let Some(requested) = step_request {
            let (step, blocked) = navigate(dialog.step, requested, id_error.is_some());
            dialog.attempted |= blocked;
            dialog.step = step;
        }
        if dialog.picker.is_none() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if chatgpt {
            self.status = Some("Signing in a ChatGPT account is not implemented yet".into());
        }
        if save {
            dialog.attempted = true;
            if id_error.is_none() {
                // `handleSave`.
                let mut config = dialog
                    .config
                    .get(dialog.driver)
                    .cloned()
                    .unwrap_or_default();
                if dialog.driver == "codex" {
                    config.insert("setupMode".into(), json!("existing"));
                }
                let mut instance = Map::new();
                instance.insert("driver".into(), json!(dialog.driver));
                instance.insert("enabled".into(), json!(true));
                if !dialog.label.trim().is_empty() {
                    instance.insert("displayName".into(), json!(dialog.label.trim()));
                }
                if let Some(color) = accent_picker::normalize(&dialog.accent) {
                    instance.insert("accentColor".into(), json!(color));
                }
                if !config.is_empty() {
                    instance.insert("config".into(), Value::Object(config));
                }
                let mut instances = self.provider_instances_map();
                instances.insert(instance_id.clone(), Value::Object(instance));
                self.replace_provider_instances(instances);
                self.status = Some("Provider instance added".into());
                return;
            }
        }
        if !close {
            self.add_instance = Some(dialog);
        }
    }
}

/// `WizardSteps`: numbered tabs; done steps get a check, the current one a card.
pub(crate) fn step_strip(ui: &Ui, rect: Rect, current: usize) -> Option<usize> {
    step_strip_with(ui, rect, &STEPS, &[], current, |_| false)
}

/// `WizardSteps` for `steps`, a done step showing its summary
/// (`showSummaries`) and disabled steps left inert (`isStepDisabled`).
pub(crate) fn step_strip_with(
    ui: &Ui,
    rect: Rect,
    steps: &[&str],
    summaries: &[Option<String>],
    current: usize,
    disabled: impl Fn(usize) -> bool,
) -> Option<usize> {
    ui.painter().rect(
        rect,
        CornerRadius::same(14),
        theme::overlay(0.04),
        Stroke::new(1.0, theme::overlay(0.05)),
        StrokeKind::Inside,
    );
    let count = steps.len() as f32;
    let width = (rect.width() - 8.0 - 4.0 * (count - 1.0)) / count;
    let mut clicked = None;
    for (index, step) in steps.iter().enumerate() {
        let item = Rect::from_min_size(
            Pos2::new(
                rect.left() + 4.0 + index as f32 * (width + 4.0),
                rect.top() + 4.0,
            ),
            Vec2::new(width, 36.0),
        );
        let inert = disabled(index);
        let response = ui.interact(
            item,
            ui.id().with(("wizard-step", index)),
            if inert {
                Sense::hover()
            } else {
                Sense::click()
            },
        );
        if index == current {
            ui.painter().rect(
                item,
                CornerRadius::same(10),
                theme::card(),
                Stroke::new(1.0, theme::overlay(0.05)),
                StrokeKind::Inside,
            );
        } else if response.hovered() && !inert {
            ui.painter()
                .rect_filled(item, CornerRadius::same(10), theme::card());
        }
        let circle = Rect::from_min_size(
            Pos2::new(item.left() + 10.0, item.top() + 8.0),
            Vec2::splat(20.0),
        );
        let primary = theme::primary();
        if index < current {
            ui.painter().circle_filled(circle.center(), 10.0, primary);
            theme::paint_icon(ui, "check", circle, 16.0, Color32::WHITE);
        } else {
            let (fill, ring, text) = if index == current {
                (
                    theme::alpha(primary, 0.1),
                    theme::alpha(primary, 0.3),
                    primary,
                )
            } else {
                (
                    theme::overlay(0.05),
                    theme::overlay(0.1),
                    theme::muted_foreground(),
                )
            };
            ui.painter().circle_filled(circle.center(), 10.0, fill);
            ui.painter()
                .circle_stroke(circle.center(), 9.5, Stroke::new(1.0, ring));
            let number = (index + 1).to_string();
            let w = widgets::text_width(ui, &number, font(14.0, Weight::Medium));
            widgets::text(
                ui,
                Pos2::new(circle.center().x - w / 2.0, circle.top()),
                &number,
                font(14.0, Weight::Medium),
                text,
                20.0,
            );
        }
        let label = match summaries.get(index).and_then(Option::as_ref) {
            Some(summary) if index < current => format!("{step}: {summary}"),
            _ => (*step).to_owned(),
        };
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(item.left() + 38.0, item.top() + 8.0),
                Pos2::new(item.right() - 10.0, item.top() + 28.0),
            ),
            &label,
            font(14.0, Weight::Medium),
            if index == current {
                theme::foreground()
            } else {
                theme::muted_foreground()
            },
        );
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

/// Step 1: the driver radio grid, then the drivers to come.
fn driver_step(app: &App, ui: &mut Ui, dialog: &mut AddInstance, left: f32, right: f32, top: f32) {
    widgets::text(
        ui,
        Pos2::new(left, top),
        "Driver",
        font(14.0, Weight::Medium),
        theme::foreground(),
        20.0,
    );
    let column = (right - left - 12.0) / 2.0;
    let cell = |index: usize| {
        Rect::from_min_size(
            Pos2::new(
                left + (index % 2) as f32 * (column + 12.0),
                top + 28.0 + (index / 2) as f32 * 56.0,
            ),
            Vec2::new(column, 44.0),
        )
    };
    for (index, (driver, label, _, badge)) in DRIVERS.iter().enumerate() {
        let rect = cell(index);
        let checked = dialog.driver == *driver;
        let response = ui.interact(rect, ui.id().with(("add-driver", driver)), Sense::click());
        let fill = if checked {
            theme::alpha(theme::primary(), 0.15)
        } else if response.hovered() {
            theme::overlay(0.05)
        } else {
            theme::overlay(0.03)
        };
        if checked {
            ui.painter().rect(
                rect,
                CornerRadius::same(10),
                fill,
                Stroke::new(2.0, theme::primary()),
                StrokeKind::Inside,
            );
        } else {
            ui.painter().rect(
                rect,
                CornerRadius::same(10),
                fill,
                Stroke::new(1.0, theme::overlay(0.05)),
                StrokeKind::Inside,
            );
        }
        app.paint_provider_mark(
            ui,
            driver,
            Pos2::new(rect.left() + 10.0, rect.center().y - 10.0),
            1.0,
        );
        widgets::text(
            ui,
            Pos2::new(rect.left() + 40.0, rect.center().y - 10.0),
            label,
            font(14.0, Weight::Medium),
            theme::foreground(),
            20.0,
        );
        let mut x = rect.right() - 12.0;
        if checked {
            let indicator = Rect::from_min_size(
                Pos2::new(x - 20.0, rect.center().y - 10.0),
                Vec2::splat(20.0),
            );
            ui.painter()
                .circle_filled(indicator.center(), 10.0, theme::primary());
            theme::paint_icon(ui, "check", indicator, 14.0, Color32::WHITE);
            x = indicator.left() - 12.0;
        }
        if let Some(badge) = badge {
            warning_badge(ui, x, rect.center().y, badge, 1.0);
        }
        if response.clicked() {
            dialog.driver = driver;
        }
    }
    for (offset, (label, icon, colored, icon_height)) in COMING_SOON.iter().enumerate() {
        let rect = cell(DRIVERS.len() + offset);
        let fade = 0.64;
        ui.painter().rect(
            rect,
            CornerRadius::same(10),
            theme::overlay(0.02 * fade),
            Stroke::new(1.0, theme::overlay(0.05 * fade)),
            StrokeKind::Inside,
        );
        let tint = if *colored {
            Color32::WHITE
        } else {
            theme::muted_foreground()
        };
        theme::tinted_icon(icon, 16.0, theme::alpha(tint, fade)).paint_at(
            ui,
            Rect::from_center_size(
                Pos2::new(rect.left() + 20.0, rect.center().y),
                Vec2::new(16.0, *icon_height),
            ),
        );
        widgets::text(
            ui,
            Pos2::new(rect.left() + 40.0, rect.center().y - 10.0),
            label,
            font(14.0, Weight::Medium),
            theme::alpha(theme::foreground(), fade),
            20.0,
        );
        warning_badge(
            ui,
            rect.right() - 12.0,
            rect.center().y,
            "Coming Soon",
            fade,
        );
    }
}

/// `Badge variant="warning" size="sm"`, right-aligned at `right`.
fn warning_badge(ui: &Ui, right: f32, center_y: f32, label: &str, fade: f32) {
    let badge_font = font(10.0, Weight::Medium);
    let w = widgets::text_width(ui, label, badge_font.clone()) + 8.0;
    let rect = Rect::from_min_size(Pos2::new(right - w, center_y - 8.0), Vec2::new(w, 16.0));
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(4),
        theme::alpha(theme::warning(), 0.16 * fade),
    );
    widgets::text(
        ui,
        Pos2::new(rect.left() + 4.0, rect.top() + 3.0),
        label,
        badge_font,
        theme::alpha(theme::warning_foreground(), fade),
        10.0,
    );
}

/// Step 2: label, instance id and accent color.
#[allow(clippy::too_many_arguments)]
fn identity_step(
    ui: &mut Ui,
    dialog: &mut AddInstance,
    left: f32,
    right: f32,
    top: f32,
    instance_id: &str,
    id_error: Option<&str>,
    driver_label: &str,
) {
    let mut y = top;
    let label_font = font(12.0, Weight::Medium);
    let help_font = font(11.0, Weight::Regular);

    widgets::text(
        ui,
        Pos2::new(left, y),
        "Label",
        label_font.clone(),
        theme::foreground(),
        16.0,
    );
    y += 24.0;
    text_input(
        ui,
        Rect::from_min_size(Pos2::new(left, y), Vec2::new(right - left, 32.0)),
        "add-instance-label",
        &mut dialog.label,
        "e.g. Work",
    );
    y += 40.0;
    widgets::text(
        ui,
        Pos2::new(left, y),
        "Shown in the provider list. Optional.",
        help_font.clone(),
        theme::muted_foreground(),
        16.0,
    );
    y += 16.0;

    widgets::text(
        ui,
        Pos2::new(left, y),
        "Instance ID",
        label_font.clone(),
        theme::foreground(),
        16.0,
    );
    y += 24.0;
    let mut id = instance_id.to_owned();
    let placeholder = format!("{}_work", dialog.driver);
    let show_error = dialog.attempted && id_error.is_some();
    if text_input(
        ui,
        Rect::from_min_size(Pos2::new(left, y), Vec2::new(right - left, 32.0)),
        "add-instance-id",
        &mut id,
        &placeholder,
    ) {
        dialog.id_override = Some(id);
    }
    if show_error {
        let destructive = Color32::from_rgb(251, 65, 74);
        ui.painter().rect_stroke(
            Rect::from_min_size(Pos2::new(left, y), Vec2::new(right - left, 32.0)),
            CornerRadius::same(10),
            Stroke::new(1.0, theme::alpha(destructive, 0.36)),
            StrokeKind::Inside,
        );
    }
    y += 40.0;
    match id_error.filter(|_| show_error) {
        Some(error) => widgets::text(
            ui,
            Pos2::new(left, y),
            error,
            help_font.clone(),
            Color32::from_rgb(251, 65, 74),
            16.0,
        ),
        None => widgets::text(
            ui,
            Pos2::new(left, y),
            "Routing key used by threads and sessions. Letters, digits, '-', or '_'.",
            help_font.clone(),
            theme::muted_foreground(),
            16.0,
        ),
    };
    y += 16.0;

    widgets::text(
        ui,
        Pos2::new(left, y),
        "Accent color",
        label_font,
        theme::foreground(),
        16.0,
    );
    y += 24.0;
    // The custom color swatch (`ProviderAccentColorPicker layout="inline"`).
    let swatch = Rect::from_min_size(Pos2::new(left, y), Vec2::splat(28.0));
    let shown = accent_picker::parse(&dialog.accent).unwrap_or(Color32::from_rgb(37, 99, 235));
    let response = ui.interact(swatch, ui.id().with("add-instance-accent"), Sense::click());
    ui.painter().rect(
        swatch,
        CornerRadius::same(8),
        shown,
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    theme::paint_icon(
        ui,
        "pipette",
        swatch,
        12.0,
        theme::alpha(Color32::WHITE, 0.7),
    );
    if response.clicked() && dialog.picker.is_none() {
        let current = (!dialog.accent.is_empty()).then_some(dialog.accent.as_str());
        dialog.picker = Some(AccentPicker::new(format!("add:{driver_label}"), current));
    }
    let mut x = swatch.right() + 8.0;
    for color in SWATCHES {
        let rect = Rect::from_min_size(Pos2::new(x, y + 2.0), Vec2::splat(24.0));
        let selected = dialog.accent.to_lowercase() == color;
        let response = ui.interact(
            rect,
            ui.id().with(("add-instance-swatch", color)),
            Sense::click(),
        );
        let fill = accent_picker::parse(color).unwrap_or_default();
        let radius = if selected || response.hovered() {
            13.2
        } else {
            12.0
        };
        if selected {
            ui.painter().circle_stroke(
                rect.center(),
                radius + 2.5,
                Stroke::new(2.0, theme::muted_foreground()),
            );
        }
        ui.painter().circle_filled(rect.center(), radius, fill);
        ui.painter().circle_stroke(
            rect.center(),
            radius - 0.5,
            Stroke::new(
                1.0,
                if selected {
                    theme::foreground()
                } else {
                    theme::alpha(Color32::WHITE, 0.2)
                },
            ),
        );
        if response.clicked() {
            dialog.accent = color.to_owned();
        }
        x = rect.right() + 6.0;
    }
    if !dialog.accent.is_empty() {
        // `Button size="xs" variant="ghost-muted"`.
        let w = widgets::text_width(ui, "Clear", font(12.0, Weight::Medium)) + 16.0;
        let rect = Rect::from_min_size(Pos2::new(x + 2.0, y + 2.0), Vec2::new(w.ceil(), 24.0));
        let response = ui.interact(rect, ui.id().with("add-instance-clear"), Sense::click());
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), theme::accent());
        }
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(rect.left() + 8.0, rect.top()), rect.max),
            "Clear",
            font(12.0, Weight::Medium),
            theme::muted_foreground(),
        );
        if response.clicked() {
            dialog.accent.clear();
        }
    }
    if let Some(picker) = dialog.picker.as_mut() {
        match picker.show(ui, swatch) {
            PickerEvent::None => {}
            PickerEvent::Commit(value) => dialog.accent = value,
            PickerEvent::Close(pending) => {
                if let Some(value) = pending {
                    dialog.accent = value;
                }
                dialog.picker = None;
            }
        }
    }
    y += 36.0;
    widgets::text(
        ui,
        Pos2::new(left, y),
        "Optional marker shown in the picker.",
        help_font,
        theme::muted_foreground(),
        16.0,
    );
}

/// Step 3: the driver's settings form (`ProviderSettingsForm variant="dialog"`).
fn config_step(
    ui: &mut Ui,
    dialog: &mut AddInstance,
    left: f32,
    right: f32,
    top: f32,
    fields: &[super::Field],
) {
    if fields.is_empty() {
        widgets::text(
            ui,
            Pos2::new(left, top),
            "This driver has no required configuration. You can add the instance now.",
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            20.0,
        );
        return;
    }
    let config = dialog.config.entry(dialog.driver).or_default();
    let mut y = top;
    for (key, title, description, placeholder) in fields {
        widgets::text(
            ui,
            Pos2::new(left, y + 5.0),
            title,
            font(12.0, Weight::Medium),
            theme::foreground(),
            16.0,
        );
        let mut value = config
            .get(*key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if text_input(
            ui,
            Rect::from_min_size(Pos2::new(left, y + 24.0), Vec2::new(right - left, 32.0)),
            &format!("add-instance-{key}"),
            &mut value,
            placeholder,
        ) {
            if value.is_empty() {
                config.remove(*key);
            } else {
                config.insert((*key).to_owned(), json!(value));
            }
        }
        widgets::text(
            ui,
            Pos2::new(left, y + 62.0),
            description,
            font(11.0, Weight::Regular),
            theme::muted_foreground(),
            14.0,
        );
        y += 96.0;
    }
}

/// The dialog `Input` (32px, radius 10, 12px insets); returns whether it changed.
pub(crate) fn text_input(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    text: &mut String,
    placeholder: &str,
) -> bool {
    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        theme::control_surface(),
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let inner = Rect::from_min_max(
        Pos2::new(rect.left() + 12.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 12.0, rect.bottom() - 1.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    let response = child.add(
        egui::TextEdit::singleline(text)
            .id(egui::Id::new(id))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(font(14.0, Weight::Regular))
            .text_color(theme::foreground())
            .vertical_align(egui::Align::Center)
            .desired_width(inner.width())
            .min_size(Vec2::new(0.0, inner.height())),
    );
    // Painted like the settings fields: egui's hint text renders lighter.
    if text.is_empty() {
        widgets::text_in(
            ui,
            inner,
            placeholder,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
        );
    }
    response.changed()
}

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Primary,
    Outline,
    GhostMuted,
}

/// A dialog footer `Button` (32px).
pub(crate) fn dialog_button(
    ui: &mut Ui,
    rect: Rect,
    label: &str,
    icon: Option<&str>,
    kind: Kind,
) -> bool {
    dialog_button_enabled(ui, rect, label, icon, kind, true)
}

/// A footer `Button`, at 64% and inert when disabled.
pub(crate) fn dialog_button_enabled(
    ui: &mut Ui,
    rect: Rect,
    label: &str,
    icon: Option<&str>,
    kind: Kind,
    enabled: bool,
) -> bool {
    let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    if !enabled {
        ui.multiply_opacity(0.64);
    }
    let ui = &ui;
    let response = ui.interact(
        rect,
        ui.id().with(("add-instance-button", label)),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = response.hovered();
    let text = match kind {
        Kind::Primary => {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(8),
                if hovered {
                    theme::primary_hover()
                } else {
                    theme::primary()
                },
            );
            Color32::WHITE
        }
        Kind::Outline => {
            ui.painter().rect(
                rect,
                CornerRadius::same(8),
                if hovered {
                    theme::overlay(0.06)
                } else {
                    theme::control_surface()
                },
                Stroke::new(1.0, theme::input()),
                StrokeKind::Inside,
            );
            theme::foreground()
        }
        Kind::GhostMuted => {
            if hovered {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), theme::accent());
                theme::foreground()
            } else {
                theme::muted_foreground()
            }
        }
    };
    // `px-3`; a leading icon sits 10px in (`-ms-0.5`) with a 6px gap.
    let mut x = rect.left() + 12.0;
    if let Some(icon) = icon {
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(
                Pos2::new(rect.left() + 10.0, rect.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            16.0,
            text,
        );
        x = rect.left() + 32.0;
    }
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(x, rect.top()), rect.max),
        label,
        font(14.0, Weight::Medium),
        text,
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_navigation_follow_upstream() {
        assert_eq!(derive_id("codex", "  Work Account! "), "codex_work_account");
        assert_eq!(derive_id("codex", "  "), "");
        let existing = vec!["codex".to_owned()];
        assert_eq!(
            validate_id("", &existing).as_deref(),
            Some("Instance ID is required.")
        );
        assert_eq!(
            validate_id("codex", &existing).as_deref(),
            Some("An instance named 'codex' already exists.")
        );
        assert!(validate_id("1abc", &existing).is_some());
        assert_eq!(validate_id("codex_work", &existing), None);
        assert_eq!(navigate(0, 2, true), (1, true));
        assert_eq!(navigate(1, 2, false), (2, false));
        assert_eq!(navigate(2, 0, true), (0, false));
    }
}
