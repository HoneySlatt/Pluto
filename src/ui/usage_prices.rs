//! Custom model prices (`usage/UsagePriceOverrides.tsx`, `usagePriceForm.ts`,
//! `usagePriceTable.ts`, `usagePriceTargets.ts`): the environment's
//! `usagePriceOverrides`, edited cell by cell and saved as a settings patch.
//!
//! This client knows one environment, so "Apply to" selects it or nothing.
//! Not ported: the model suggestions list.
use std::collections::BTreeMap;

use eframe::egui::{
    self, Color32, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::{Map, Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// `USAGE_PRICE_FIELDS`: key, label, optional.
const FIELDS: [(&str, &str, bool); 4] = [
    ("inputCostPerMillionTokens", "Input", false),
    ("outputCostPerMillionTokens", "Output", false),
    ("cacheReadCostPerMillionTokens", "Cache read", true),
    ("cacheWriteCostPerMillionTokens", "Cache write", true),
];
const WIDTH: f32 = 768.0;

/// `UsagePriceDraft`.
#[derive(Clone)]
struct Draft {
    id: String,
    model: String,
    is_new: bool,
    values: BTreeMap<&'static str, String>,
    removed: bool,
}

/// The open dialog.
#[derive(Default)]
pub struct PricesDialog {
    drafts: Vec<Draft>,
    next_row: usize,
    /// Whether the environment is selected (`selectedIds === null`).
    deselected: bool,
    apply_menu: bool,
    saving: bool,
}

/// `usagePriceForm`: a stored price as the editable strings.
fn form_value(price: Option<&Value>, field: &str) -> String {
    match price.and_then(|p| p[field].as_f64()) {
        Some(value) => format_number(value),
        None => String::new(),
    }
}

/// JavaScript's `Number.prototype.toString` for prices.
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// `parseUsagePriceForm`: blank cache rates use the input rate.
fn parse_price(values: &BTreeMap<&'static str, String>) -> Option<Value> {
    let mut price = Map::new();
    for (key, _, optional) in FIELDS {
        let raw = values.get(key).map(|v| v.trim()).unwrap_or_default();
        if raw.is_empty() {
            if optional {
                continue;
            }
            return None;
        }
        let valid = raw
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
            && raw
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '.');
        let value: f64 = raw
            .parse()
            .ok()
            .filter(|v: &f64| valid && v.is_finite() && *v >= 0.0)?;
        price.insert(key.to_owned(), json!(value));
    }
    Some(Value::Object(price))
}

impl Draft {
    fn is_empty(&self) -> bool {
        self.is_new
            && self.model.trim().is_empty()
            && self.values.values().all(|v| v.trim().is_empty())
    }
}

impl App {
    pub(crate) fn open_price_dialog(&mut self) {
        self.prices_dialog = Some(PricesDialog::default());
    }

    fn price_overrides(&self) -> Option<Map<String, Value>> {
        self.state.config["settings"]["usagePriceOverrides"]
            .as_object()
            .cloned()
            .or_else(|| self.state.config["settings"].is_object().then(Map::new))
    }

    /// `usagePriceTableChanges` for the environment: the patch, or the first
    /// row error.
    fn price_changes(&self, drafts: &[Draft]) -> (Map<String, Value>, BTreeMap<String, String>) {
        let prices = self.price_overrides().unwrap_or_default();
        let mut changes = Map::new();
        let mut errors = BTreeMap::new();
        for draft in drafts.iter().filter(|d| !d.is_empty()) {
            let model = draft.model.trim().to_owned();
            if draft.removed {
                if prices.contains_key(&model) {
                    changes.insert(model, Value::Null);
                }
                continue;
            }
            let original: BTreeMap<&'static str, String> = FIELDS
                .iter()
                .map(|(key, _, _)| (*key, form_value(prices.get(&model), key)))
                .collect();
            let mut form = original.clone();
            for (key, value) in &draft.values {
                form.insert(key, value.clone());
            }
            let parsed = (!model.is_empty()).then(|| parse_price(&form)).flatten();
            let Some(price) = parsed else {
                let missing = FIELDS
                    .iter()
                    .find(|(key, _, optional)| !optional && form[key].trim().is_empty());
                errors.insert(
                    draft.id.clone(),
                    if model.is_empty() {
                        "Enter a model ID.".to_owned()
                    } else if let Some((_, label, _)) = missing {
                        format!("{label} is required on {}.", self.environment_label())
                    } else {
                        "Use non-negative numbers for prices.".to_owned()
                    },
                );
                continue;
            };
            let next: BTreeMap<&'static str, String> = FIELDS
                .iter()
                .map(|(key, _, _)| (*key, form_value(Some(&price), key)))
                .collect();
            if next != original {
                changes.insert(model, price);
            }
        }
        (changes, errors)
    }

    pub(crate) fn environment_label(&self) -> String {
        self.state.config["environment"]["label"]
            .as_str()
            .unwrap_or("this environment")
            .to_owned()
    }

    pub(crate) fn prices_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(mut dialog) = self.prices_dialog.take() else {
            return;
        };
        let prices = self.price_overrides();
        let unavailable =
            self.state.config["environment"]["capabilities"]["usagePriceOverrides"] != true;
        let selected = !dialog.deselected;
        let custom: Vec<String> = if selected {
            prices
                .as_ref()
                .map(|p| p.keys().cloned().collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut rows: Vec<Draft> = custom
            .iter()
            .map(|model| {
                dialog
                    .drafts
                    .iter()
                    .find(|d| d.id == format!("model:{model}"))
                    .cloned()
                    .unwrap_or_else(|| Draft {
                        id: format!("model:{model}"),
                        model: model.clone(),
                        is_new: false,
                        values: BTreeMap::new(),
                        removed: false,
                    })
            })
            .collect();
        rows.extend(dialog.drafts.iter().filter(|d| d.is_new).cloned());
        let staged: Vec<Draft> = dialog
            .drafts
            .iter()
            .filter(|d| !d.is_empty())
            .cloned()
            .collect();
        let (changes, mut errors) = if selected && !unavailable {
            self.price_changes(&staged)
        } else {
            (Map::new(), BTreeMap::new())
        };
        for draft in staged.iter().filter(|d| d.is_new) {
            let model = draft.model.trim();
            if model.is_empty() {
                errors.insert(draft.id.clone(), "Enter a model ID.".into());
            } else if custom.iter().any(|m| m == model)
                || dialog
                    .drafts
                    .iter()
                    .any(|o| o.id != draft.id && o.model.trim() == model)
            {
                errors.insert(
                    draft.id.clone(),
                    "This model already has a row. Edit its prices there.".into(),
                );
            }
        }
        let has_changes = !staged.is_empty();
        let locked = dialog.saving;
        let env_label = self.environment_label();

        // Height: header, apply row, the table (or the notice), footnote, footer.
        let row_heights: Vec<f32> = rows
            .iter()
            .map(|row| {
                48.0 + if errors.contains_key(&row.id) {
                    20.0
                } else {
                    0.0
                }
            })
            .collect();
        let table_body = if rows.is_empty() {
            82.0
        } else {
            row_heights.iter().sum::<f32>()
        };
        let content = if selected {
            40.0 + table_body + 2.0 + 15.0 + 16.0
        } else {
            20.0
        };
        let height = 25.0 + 20.0 + 8.0 + 20.0 + 16.0 + 28.0 + 16.0 + content + 18.0 + 32.0 + 25.0;
        let d = Rect::from_center_size(screen.center(), Vec2::new(WIDTH, height));
        let mut close = false;
        let mut save = false;
        let mut discard = false;
        let mut add_row = false;
        let mut edits: Vec<Draft> = Vec::new();
        let mut drop_row: Option<String> = None;
        egui::Area::new(ui.id().with("prices-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if response.clicked()
                    && !ui.rect_contains_pointer(d)
                    && !locked
                    && !dialog.apply_menu
                {
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
                    "prices-close",
                    "x",
                    16.0,
                    theme::muted_foreground(),
                    "",
                )
                .clicked()
                    && !locked
                {
                    close = true;
                }
                let left = d.left() + 25.0;
                let right = d.right() - 25.0;
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    "Custom model prices",
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 53.0),
                    "Prices apply to all past and future usage on the environments you select.",
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                );
                // "Apply to" and the environment menu.
                let row_top = d.top() + 89.0;
                widgets::text(
                    ui,
                    Pos2::new(left, row_top + 6.0),
                    "Apply to",
                    font(14.0, Weight::Medium),
                    theme::foreground(),
                    16.0,
                );
                let label = if selected {
                    "All environments".to_owned()
                } else {
                    "0 environments".to_owned()
                };
                let label_width = widgets::text_width(ui, &label, font(14.0, Weight::Medium));
                let apply = Rect::from_min_size(
                    Pos2::new(left + 63.0, row_top),
                    Vec2::new((10.0 + label_width + 3.0 + 14.0 + 9.0).ceil(), 28.0),
                );
                let disabled = locked || has_changes;
                let response = ui.interact(
                    apply,
                    ui.id().with("prices-apply"),
                    if disabled {
                        Sense::hover()
                    } else {
                        Sense::click()
                    },
                );
                ui.painter().rect(
                    apply,
                    CornerRadius::same(8),
                    if response.hovered() && !disabled {
                        theme::overlay(0.06)
                    } else {
                        theme::control_surface()
                    },
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let alpha = if disabled { 0.64 } else { 1.0 };
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(apply.left() + 10.0, apply.top()), apply.max),
                    &label,
                    font(14.0, Weight::Medium),
                    theme::alpha(theme::foreground(), alpha),
                );
                theme::paint_icon(
                    ui,
                    "chevron-down",
                    Rect::from_min_size(
                        Pos2::new(apply.right() - 23.0, apply.top()),
                        Vec2::new(14.0, 28.0),
                    ),
                    14.0,
                    theme::alpha(theme::muted_foreground(), alpha),
                );
                if response.clicked() {
                    dialog.apply_menu = !dialog.apply_menu;
                }
                let unit = "USD / million tokens";
                let unit_width = widgets::text_width(ui, unit, font(12.0, Weight::Regular));
                widgets::text(
                    ui,
                    Pos2::new(right - unit_width, row_top + 6.0),
                    unit,
                    font(12.0, Weight::Regular),
                    theme::muted_foreground(),
                    16.0,
                );

                let table_top = row_top + 28.0 + 16.0;
                if !selected {
                    widgets::text(
                        ui,
                        Pos2::new(left, table_top),
                        "Select an environment to see and change its model prices.",
                        font(14.0, Weight::Regular),
                        theme::muted_foreground(),
                        20.0,
                    );
                } else {
                    let table = Rect::from_min_size(
                        Pos2::new(left, table_top),
                        Vec2::new(right - left, 40.0 + table_body + 2.0),
                    );
                    ui.painter().rect(
                        table,
                        CornerRadius::same(10),
                        Color32::TRANSPARENT,
                        Stroke::new(1.0, theme::border()),
                        StrokeKind::Inside,
                    );
                    let inner = table.shrink(1.0);
                    let model_width = (inner.width() * 0.34).round();
                    let price_width = ((inner.width() - model_width - 40.0) / 4.0).floor();
                    let column = |index: usize| -> f32 {
                        if index == 0 {
                            inner.left()
                        } else {
                            inner.left() + model_width + (index - 1) as f32 * price_width
                        }
                    };
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(inner.left(), inner.top() + 40.0),
                            Vec2::new(inner.width(), 1.0),
                        ),
                        0.0,
                        theme::border(),
                    );
                    for (index, heading) in std::iter::once("Model ID")
                        .chain(FIELDS.iter().map(|f| f.1))
                        .enumerate()
                    {
                        widgets::text_in(
                            ui,
                            Rect::from_min_size(
                                Pos2::new(column(index) + 8.0, inner.top()),
                                Vec2::new(price_width, 40.0),
                            ),
                            heading,
                            font(12.0, Weight::Medium),
                            theme::foreground(),
                        );
                    }
                    let add = Rect::from_min_size(
                        Pos2::new(inner.right() - 32.0, inner.top() + 8.0),
                        Vec2::splat(24.0),
                    );
                    if widgets::icon_button(
                        ui,
                        add,
                        "prices-add",
                        "plus",
                        14.0,
                        theme::muted_foreground(),
                        "Add model price",
                    )
                    .clicked()
                        && !locked
                    {
                        add_row = true;
                    }
                    let mut y = inner.top() + 41.0;
                    if rows.is_empty() {
                        let message = "No custom prices. Add a row to override automatic pricing.";
                        let width = widgets::text_width(ui, message, font(12.0, Weight::Regular));
                        widgets::text(
                            ui,
                            Pos2::new(inner.center().x - width / 2.0, y + 8.0 + 24.0),
                            message,
                            font(12.0, Weight::Regular),
                            theme::muted_foreground(),
                            16.0,
                        );
                    }
                    for (row, row_height) in rows.iter().zip(&row_heights) {
                        let top = y + 8.0;
                        let mut edited = row.clone();
                        let mut changed = false;
                        // The model: an input on a new row, the id otherwise.
                        let model_cell = Rect::from_min_size(
                            Pos2::new(column(0) + 8.0, top),
                            Vec2::new(model_width - 16.0, 30.0),
                        );
                        if row.is_new {
                            changed |= text_cell(
                                ui,
                                model_cell,
                                &format!("price-model:{}", row.id),
                                &mut edited.model,
                                "Model ID",
                                false,
                            );
                        } else {
                            let color = if row.removed {
                                theme::muted_foreground()
                            } else {
                                theme::foreground()
                            };
                            let width = widgets::text_in(
                                ui,
                                Rect::from_min_max(
                                    Pos2::new(model_cell.left(), top),
                                    Pos2::new(model_cell.right(), top + 30.0),
                                ),
                                &row.model,
                                font(14.0, Weight::Regular),
                                color,
                            );
                            if row.removed {
                                ui.painter().hline(
                                    model_cell.left()..=model_cell.left() + width,
                                    top + 15.0,
                                    Stroke::new(1.0, color),
                                );
                            }
                        }
                        if let Some(error) = errors.get(&row.id).filter(|_| {
                            !row.model.trim().is_empty()
                                || row.values.values().any(|v| !v.is_empty())
                        }) {
                            widgets::text(
                                ui,
                                Pos2::new(model_cell.left(), top + 34.0),
                                error,
                                font(12.0, Weight::Regular),
                                theme::error_foreground(),
                                16.0,
                            );
                        }
                        if row.removed {
                            widgets::text_in(
                                ui,
                                Rect::from_min_size(
                                    Pos2::new(column(1) + 8.0, top),
                                    Vec2::new(price_width * 4.0, 30.0),
                                ),
                                "Automatic pricing after saving",
                                font(14.0, Weight::Regular),
                                theme::muted_foreground(),
                            );
                        } else {
                            for (index, (key, _, optional)) in FIELDS.iter().enumerate() {
                                let stored = prices.as_ref().and_then(|p| p.get(&row.model));
                                let original = form_value(stored, key);
                                let placeholder = if row.is_new {
                                    if *optional { "Input rate" } else { "0.00" }
                                } else if prices.is_none() {
                                    "Unavailable"
                                } else if stored.is_none() {
                                    "Automatic"
                                } else if *optional {
                                    "Input rate"
                                } else {
                                    "0.00"
                                };
                                let mut value = row.values.get(key).cloned().unwrap_or_else(|| {
                                    if row.is_new {
                                        String::new()
                                    } else {
                                        original.clone()
                                    }
                                });
                                let cell = Rect::from_min_size(
                                    Pos2::new(column(index + 1) + 8.0, top),
                                    Vec2::new(price_width - 16.0, 30.0),
                                );
                                if text_cell(
                                    ui,
                                    cell,
                                    &format!("price:{}:{key}", row.id),
                                    &mut value,
                                    placeholder,
                                    true,
                                ) {
                                    changed = true;
                                    // `editCell`: a value back at the stored rate is no edit.
                                    if !row.is_new && value == original {
                                        edited.values.remove(key);
                                    } else {
                                        edited.values.insert(key, value);
                                    }
                                }
                            }
                        }
                        let action = Rect::from_min_size(
                            Pos2::new(inner.right() - 32.0, top + 3.0),
                            Vec2::splat(24.0),
                        );
                        let (icon, tip) = if row.removed {
                            ("rotate-ccw", "Undo reset")
                        } else if row.is_new {
                            ("x", "Remove row")
                        } else {
                            ("rotate-ccw", "Reset to automatic")
                        };
                        if widgets::icon_button(
                            ui,
                            action,
                            ("price-row", &row.id),
                            icon,
                            14.0,
                            theme::muted_foreground(),
                            tip,
                        )
                        .clicked()
                            && !locked
                        {
                            if row.is_new || (row.removed && row.values.is_empty()) {
                                drop_row = Some(row.id.clone());
                            } else {
                                edited.removed = !row.removed;
                                changed = true;
                            }
                        }
                        if changed {
                            edits.push(edited);
                        }
                        y += row_height;
                    }
                    widgets::text(
                        ui,
                        Pos2::new(left, table.bottom() + 15.0),
                        "Blank cache rates use the input price. Enter 0 for free tokens.",
                        font(12.0, Weight::Regular),
                        theme::muted_foreground(),
                        16.0,
                    );
                }

                // Footer: what the changes apply to, discard and save.
                let save_label = if locked { "Saving…" } else { "Save changes" };
                let save_width =
                    (widgets::text_width(ui, save_label, font(14.0, Weight::Medium)) + 24.0).ceil();
                let save_rect = Rect::from_min_size(
                    Pos2::new(right - save_width, d.bottom() - 25.0 - 32.0),
                    Vec2::new(save_width, 32.0),
                );
                let can_save =
                    !locked && has_changes && errors.is_empty() && selected && !unavailable;
                if footer_button(ui, save_rect, "prices-save", save_label, true, can_save) {
                    save = true;
                }
                if has_changes {
                    let label = "Discard changes";
                    let width =
                        (widgets::text_width(ui, label, font(14.0, Weight::Medium)) + 24.0).ceil();
                    let rect = Rect::from_min_size(
                        Pos2::new(save_rect.left() - 8.0 - width, save_rect.top()),
                        Vec2::new(width, 32.0),
                    );
                    if footer_button(ui, rect, "prices-discard", label, false, !locked) {
                        discard = true;
                    }
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(left, save_rect.top()),
                            Pos2::new(rect.left() - 8.0, save_rect.bottom()),
                        ),
                        &format!("Changes apply to {env_label}"),
                        font(12.0, Weight::Regular),
                        theme::muted_foreground(),
                    );
                } else if unavailable {
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(left, save_rect.top()),
                            Pos2::new(save_rect.left() - 8.0, save_rect.bottom()),
                        ),
                        "Update server to edit prices",
                        font(12.0, Weight::Regular),
                        theme::muted_foreground(),
                    );
                }

                if dialog.apply_menu {
                    let mut open = Some("prices-apply".to_owned());
                    let entries = [
                        widgets::MenuEntry::Item(widgets::MenuItem {
                            checked: Some(selected),
                            ..widgets::MenuItem::new("All environments")
                        }),
                        widgets::MenuEntry::Separator,
                        widgets::MenuEntry::Item(widgets::MenuItem {
                            checked: Some(selected),
                            ..widgets::MenuItem::new(&env_label)
                        }),
                    ];
                    if widgets::menu_aligned(
                        ui,
                        &mut open,
                        "prices-apply",
                        apply,
                        false,
                        false,
                        &entries,
                    )
                    .is_some()
                    {
                        dialog.deselected = !dialog.deselected;
                    }
                    if open.is_none() {
                        dialog.apply_menu = false;
                    }
                }
            });
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) && !locked {
            close = true;
        }
        for edit in edits {
            let empty_existing = !edit.is_new && !edit.removed && edit.values.is_empty();
            dialog.drafts.retain(|d| d.id != edit.id);
            if !empty_existing {
                dialog.drafts.push(edit);
            }
        }
        if let Some(id) = drop_row {
            dialog.drafts.retain(|d| d.id != id);
        }
        if add_row {
            dialog.drafts.push(Draft {
                id: format!("new:{}", dialog.next_row),
                model: String::new(),
                is_new: true,
                values: BTreeMap::new(),
                removed: false,
            });
            dialog.next_row += 1;
        }
        if discard {
            dialog.drafts.clear();
        }
        if save {
            if !changes.is_empty() {
                self.patch_server_settings(json!({"usagePriceOverrides": changes}));
            }
            dialog.drafts.clear();
        }
        if !close {
            self.prices_dialog = Some(dialog);
        }
    }
}

/// A compact input (`Input size="compact"`), mono for prices; returns
/// whether the text changed.
fn text_cell(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    value: &mut String,
    placeholder: &str,
    mono: bool,
) -> bool {
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        theme::control_surface(),
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let text_font = if mono {
        theme::mono(12.0)
    } else {
        font(13.0, Weight::Regular)
    };
    let field = Rect::from_min_max(
        rect.min + Vec2::new(10.0, 6.0),
        rect.max - Vec2::new(10.0, 6.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
    let response = child.add(
        egui::TextEdit::singleline(value)
            .id(egui::Id::new(id))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(text_font.clone())
            .text_color(theme::foreground())
            .vertical_align(egui::Align::Center)
            .hint_text(
                egui::RichText::new(placeholder)
                    .font(text_font)
                    .color(theme::muted_foreground()),
            )
            .desired_width(field.width()),
    );
    response.changed()
}

/// A dialog footer button (`size="default"`): primary or ghost.
fn footer_button(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    label: &str,
    primary: bool,
    enabled: bool,
) -> bool {
    let response = ui.interact(
        rect,
        ui.id().with(id),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let alpha = if enabled { 1.0 } else { 0.64 };
    if primary {
        let fill = if response.hovered() && enabled {
            theme::primary_hover()
        } else {
            theme::primary()
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::alpha(fill, alpha));
    } else if response.hovered() && enabled {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::accent());
    }
    let width = widgets::text_width(ui, label, font(14.0, Weight::Medium));
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.center().x - width / 2.0, rect.top()),
            rect.max,
        ),
        label,
        font(14.0, Weight::Medium),
        theme::alpha(
            if primary {
                Color32::WHITE
            } else {
                theme::foreground()
            },
            alpha,
        ),
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_forms_parse_like_upstream() {
        let mut values = BTreeMap::new();
        values.insert("inputCostPerMillionTokens", "3".to_owned());
        values.insert("outputCostPerMillionTokens", "15.5".to_owned());
        let price = parse_price(&values).unwrap();
        assert_eq!(
            price,
            json!({"inputCostPerMillionTokens": 3.0, "outputCostPerMillionTokens": 15.5})
        );
        values.insert("cacheReadCostPerMillionTokens", "-1".to_owned());
        assert!(parse_price(&values).is_none());
        values.remove("cacheReadCostPerMillionTokens");
        values.insert("outputCostPerMillionTokens", String::new());
        assert!(parse_price(&values).is_none());
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(0.25), "0.25");
    }
}
