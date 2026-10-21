//! Composer pickers: model (`ProviderModelPicker.tsx`, `ModelPickerContent.tsx`),
//! traits (`TraitsPicker.tsx`) and runtime mode (`runtimeModeConfig.ts`).
//! Choices are composer draft state, sent with the next `thread.turn.start`.
use eframe::egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::ModelSelection;
use crate::theme::{self, Weight, font};
use crate::ui::widgets::{self, MenuEntry, MenuItem};

pub const RUNTIME_MODES: [(&str, &str, &str, &str); 4] = [
    (
        "approval-required",
        "Supervised",
        "lock",
        "Ask before commands and file changes.",
    ),
    (
        "auto-accept-edits",
        "Auto-accept edits",
        "pen-line",
        "Auto-approve edits, ask before other actions.",
    ),
    (
        "auto",
        "Auto",
        "sparkles",
        "Supported providers approve routine actions; others still ask.",
    ),
    (
        "full-access",
        "Full access",
        "lock-open",
        "Allow commands and edits without prompts.",
    ),
];

/// Provider glyph for a driver, with its brand tint.
pub fn provider_icon(driver: &str) -> Option<(&'static str, Color32)> {
    match driver {
        "codex" => Some(("openai", theme::foreground())),
        "claudeAgent" => Some(("claude", Color32::from_rgb(0xd9, 0x77, 0x57))),
        _ => None,
    }
}

const PICKER_WIDTH: f32 = 362.0;
const PICKER_HEIGHT: f32 = 348.0;
const SIDEBAR_WIDTH: f32 = 44.0;
const FAVORITES: &str = "__favorites";

impl App {
    /// Composer draft key: the open thread, or the new-thread draft.
    pub(crate) fn composer_key(&self, draft: bool) -> String {
        match (&self.selected, draft) {
            (Some(id), false) => id.clone(),
            _ => "draft".into(),
        }
    }

    fn set_selection(&mut self, draft: bool, selection: ModelSelection) {
        let key = self.composer_key(draft);
        self.composer_selection.insert(key, selection);
    }

    pub(crate) fn traits_menu(&mut self, ui: &mut Ui, anchor: Rect, draft: bool) {
        let selection = self.composer_model_selection(draft);
        if let Some(next) = self.traits_menu_popup(ui, anchor, "traits-picker", &selection, true) {
            self.set_selection(draft, next);
        }
    }

    /// The model's traits menu (reasoning, fast mode…) open as `menu`;
    /// returns the selection with the chosen option. Options may be a map
    /// (composer drafts) or `[{id, value}]` (server settings); the result
    /// keeps the input's shape.
    pub(crate) fn traits_menu_popup(
        &mut self,
        ui: &mut Ui,
        anchor: Rect,
        menu: &str,
        selection: &ModelSelection,
        above: bool,
    ) -> Option<ModelSelection> {
        if self.open_menu.as_deref() != Some(menu) {
            return None;
        }
        let model = self.model_entry(selection)?;
        let option_of = |id: &str| -> Option<Value> {
            let options = selection.options.as_ref()?;
            match options {
                Value::Array(list) => list
                    .iter()
                    .find(|o| o["id"] == id)
                    .map(|o| o["value"].clone()),
                _ => options.get(id).cloned(),
            }
        };
        let descriptors = model["capabilities"]["optionDescriptors"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let current = |descriptor: &Value| -> Value {
            let id = descriptor["id"].as_str().unwrap_or_default();
            option_of(id).unwrap_or_else(|| descriptor["currentValue"].clone())
        };
        // Owned strings first; menu entries borrow them.
        struct Row {
            header: Option<String>,
            separator: bool,
            label: String,
            detail: Option<String>,
            default: bool,
            selected: bool,
            descriptor: String,
            value: Value,
        }
        let mut rows: Vec<Row> = Vec::new();
        for (index, descriptor) in descriptors.iter().enumerate() {
            let id = descriptor["id"].as_str().unwrap_or_default().to_owned();
            let label = descriptor["label"].as_str().unwrap_or(&id).to_owned();
            let value = current(descriptor);
            if index > 0 {
                rows.push(Row {
                    header: None,
                    separator: true,
                    label: String::new(),
                    detail: None,
                    default: false,
                    selected: false,
                    descriptor: String::new(),
                    value: Value::Null,
                });
            }
            rows.push(Row {
                header: Some(label.clone()),
                separator: false,
                label: String::new(),
                detail: None,
                default: false,
                selected: false,
                descriptor: String::new(),
                value: Value::Null,
            });
            match descriptor["type"].as_str() {
                Some("select") => {
                    for option in descriptor["options"].as_array().into_iter().flatten() {
                        let option_id = option["id"].clone();
                        rows.push(Row {
                            header: None,
                            separator: false,
                            label: option["label"].as_str().unwrap_or_default().to_owned(),
                            detail: option["description"].as_str().map(str::to_owned),
                            default: option["isDefault"] == true,
                            selected: option_id == value,
                            descriptor: id.clone(),
                            value: option_id,
                        });
                    }
                }
                Some("boolean") => {
                    for (text, flag) in [("On", true), ("Off", false)] {
                        rows.push(Row {
                            header: None,
                            separator: false,
                            label: text.into(),
                            detail: None,
                            default: false,
                            selected: value == json!(flag),
                            descriptor: id.clone(),
                            value: json!(flag),
                        });
                    }
                }
                _ => {}
            }
        }
        let entries: Vec<MenuEntry> = rows
            .iter()
            .map(|row| {
                if row.separator {
                    MenuEntry::Separator
                } else if let Some(header) = &row.header {
                    MenuEntry::Header(header)
                } else {
                    MenuEntry::Item(MenuItem {
                        badge: row.default.then_some("Default"),
                        detail: row.detail.as_deref(),
                        selected: row.selected,
                        ..MenuItem::new(&row.label)
                    })
                }
            })
            .collect();
        let index = widgets::menu(ui, &mut self.open_menu, menu, anchor, above, &entries)?;
        let row = &rows[index];
        let mut next = selection.clone();
        let options = match next.options.take() {
            Some(Value::Array(mut list)) => {
                match list.iter_mut().find(|o| o["id"] == row.descriptor.as_str()) {
                    Some(entry) => entry["value"] = row.value.clone(),
                    None => list.push(json!({"id": row.descriptor, "value": row.value})),
                }
                Value::Array(list)
            }
            Some(mut map) if map.is_object() => {
                map[&row.descriptor] = row.value.clone();
                map
            }
            _ => json!({ row.descriptor.clone(): row.value.clone() }),
        };
        next.options = Some(options);
        Some(next)
    }

    pub(crate) fn runtime_menu(&mut self, ui: &mut Ui, anchor: Rect, draft: bool) {
        let current = self.composer_runtime_mode(draft);
        let entries: Vec<MenuEntry> = RUNTIME_MODES
            .iter()
            .map(|(mode, label, icon, detail)| {
                MenuEntry::Item(MenuItem {
                    icon: Some(icon),
                    detail: Some(detail),
                    strong: true,
                    selected: *mode == current,
                    ..MenuItem::new(label)
                })
            })
            .collect();
        if let Some(index) = widgets::menu(
            ui,
            &mut self.open_menu,
            "runtime-picker",
            anchor,
            true,
            &entries,
        ) {
            let key = self.composer_key(draft);
            self.composer_runtime
                .insert(key, RUNTIME_MODES[index].0.to_owned());
        }
    }

    pub(crate) fn model_picker(&mut self, ui: &mut Ui, anchor: Rect, draft: bool) {
        let selection = self.composer_model_selection(draft);
        if let Some(chosen) = self.model_picker_popup(ui, anchor, "model-picker", selection, false)
        {
            self.set_selection(draft, chosen);
        }
    }

    /// The model picker (`ProviderModelPicker`) open as `menu`, above
    /// `anchor` or below it; returns the model chosen.
    pub(crate) fn model_picker_popup(
        &mut self,
        ui: &mut Ui,
        anchor: Rect,
        menu: &str,
        selection: ModelSelection,
        below: bool,
    ) -> Option<ModelSelection> {
        if self.open_menu.as_deref() != Some(menu) {
            return None;
        }
        let providers: Vec<Value> = self
            .providers()
            .into_iter()
            .filter(|p| p["status"] == "ready" && p["enabled"] != false)
            .collect();
        if self.picker_provider.is_none() {
            self.picker_provider = Some(selection.instance_id.clone());
        }
        let active = self.picker_provider.clone().unwrap_or_default();
        let rect = Rect::from_min_size(
            if below {
                Pos2::new(anchor.left(), anchor.bottom() + 4.0)
            } else {
                Pos2::new(anchor.left(), anchor.top() - 4.0 - PICKER_HEIGHT)
            },
            Vec2::new(PICKER_WIDTH, PICKER_HEIGHT),
        );
        let mut chosen: Option<ModelSelection> = None;
        let mut switch_provider: Option<String> = None;
        let mut toggle_favorite: Option<String> = None;
        let mut open = self.open_menu.take();
        let query = self.picker_query.clone();
        let mut new_query = query.clone();
        let favorites = self.favorite_models.clone();
        let legacy_open = self.picker_legacy_open;
        let mut toggle_legacy = false;
        widgets::popup(ui, &mut open, menu, rect, anchor, |ui, rect| {
            let inner = rect.shrink(1.0);
            // Provider rail.
            let rail = Rect::from_min_size(inner.min, Vec2::new(SIDEBAR_WIDTH, inner.height()));
            ui.painter().rect_filled(
                rail,
                CornerRadius {
                    nw: 9,
                    sw: 9,
                    ne: 0,
                    se: 0,
                },
                theme::overlay(0.009),
            );
            let mut y = rail.top() + 12.0;
            let star = Rect::from_min_size(Pos2::new(rail.left() + 12.0, y), Vec2::splat(20.0));
            if rail_button(
                ui,
                star,
                "rail-favorites",
                "star-filled",
                theme::foreground(),
                active == FAVORITES,
                rail.right(),
            )
            .clicked()
            {
                switch_provider = Some(FAVORITES.into());
            }
            ui.painter().rect_filled(
                Rect::from_min_size(
                    Pos2::new(rail.left() + 4.0, rail.top() + 44.0),
                    Vec2::new(36.0, 1.0),
                ),
                0.0,
                theme::overlay(0.042),
            );
            y = rail.top() + 57.0;
            for provider in &providers {
                let id = provider["instanceId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let driver = provider["driver"].as_str().unwrap_or_default();
                let (icon, tint) =
                    provider_icon(driver).unwrap_or(("circle-alert", theme::muted_foreground()));
                let slot = Rect::from_min_size(Pos2::new(rail.left() + 12.0, y), Vec2::splat(20.0));
                let response = rail_button(
                    ui,
                    slot,
                    &format!("rail-{id}"),
                    icon,
                    tint,
                    active == id,
                    rail.right(),
                );
                if let Some(badge) = self.instance_badge(&id) {
                    // `indicatorBackground`: the hover, the selection or the rail.
                    let ring = if response.hovered() {
                        theme::muted()
                    } else if active == id {
                        theme::background()
                    } else {
                        theme::alpha(theme::muted(), 0.3)
                    };
                    crate::ui::instance_badge::paint(ui, slot, 2.0, &badge, ring, 1.0);
                }
                if response.clicked() {
                    switch_provider = Some(id);
                }
                y += 40.0;
            }

            // Search + list.
            let content = Rect::from_min_max(Pos2::new(rail.right(), inner.top()), inner.max);
            ui.painter().rect_filled(
                content,
                CornerRadius {
                    nw: 0,
                    sw: 0,
                    ne: 9,
                    se: 9,
                },
                theme::overlay(0.012),
            );
            ui.painter().rect_filled(
                Rect::from_min_size(content.min, Vec2::new(1.0, content.height())),
                0.0,
                theme::overlay(0.042),
            );
            let search = Rect::from_min_size(
                Pos2::new(content.left() + 13.0, content.top() + 9.0),
                Vec2::new(content.width() - 25.0, 33.0),
            );
            widgets::combobox_search(ui, search, &mut new_query, "Search models...");

            // Models of the active provider (or favorites), legacy ones folded.
            let needle = new_query.trim().to_lowercase();
            let mut current: Vec<(String, String, Value)> = Vec::new();
            let mut legacy: Vec<(String, String, Value)> = Vec::new();
            for provider in &providers {
                let id = provider["instanceId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                if active != FAVORITES && active != id && needle.is_empty() {
                    continue;
                }
                let name = provider["displayName"].as_str().unwrap_or(&id).to_owned();
                for model in provider["models"].as_array().into_iter().flatten() {
                    let slug = model["slug"].as_str().unwrap_or_default();
                    let key = format!("{id}/{slug}");
                    if active == FAVORITES && needle.is_empty() && !favorites.contains(&key) {
                        continue;
                    }
                    let label = model["displayName"]
                        .as_str()
                        .or(model["name"].as_str())
                        .unwrap_or(slug);
                    if !needle.is_empty()
                        && !label.to_lowercase().contains(&needle)
                        && !slug.contains(&needle)
                    {
                        continue;
                    }
                    let entry = (id.clone(), name.clone(), model.clone());
                    if model["isLegacy"] == true && needle.is_empty() {
                        legacy.push(entry);
                    } else {
                        current.push(entry);
                    }
                }
            }
            let list = Rect::from_min_max(
                Pos2::new(content.left() + 13.0, search.bottom() + 11.0),
                Pos2::new(content.right() - 12.0, content.bottom() - 8.0),
            );
            let mut list_ui = ui.new_child(eframe::egui::UiBuilder::new().max_rect(list));
            eframe::egui::ScrollArea::vertical()
                .id_salt("model-list")
                .show(&mut list_ui, |ui| {
                    let rows: Vec<&(String, String, Value)> = current
                        .iter()
                        .chain(if legacy_open {
                            legacy.iter()
                        } else {
                            [].iter()
                        })
                        .collect();
                    if rows.is_empty() && legacy.is_empty() {
                        widgets::text(
                            ui,
                            ui.cursor().min + Vec2::new(8.0, 8.0),
                            "No models",
                            font(12.0, Weight::Regular),
                            theme::muted_foreground(),
                            16.0,
                        );
                    }
                    for (provider_id, provider_name, model) in rows {
                        let slug = model["slug"].as_str().unwrap_or_default();
                        let label = model["displayName"]
                            .as_str()
                            .or(model["name"].as_str())
                            .unwrap_or(slug);
                        let (row, _) =
                            ui.allocate_exact_size(Vec2::new(list.width(), 45.0), Sense::hover());
                        let response = ui.interact(
                            row,
                            ui.id().with(("model-row", provider_id, slug)),
                            Sense::click(),
                        );
                        let is_selected =
                            selection.instance_id == *provider_id && selection.model == slug;
                        if is_selected {
                            ui.painter().rect_filled(
                                row,
                                CornerRadius::same(6),
                                theme::overlay(0.078),
                            );
                        } else if response.hovered() {
                            ui.painter()
                                .rect_filled(row, CornerRadius::same(6), theme::accent());
                        }
                        widgets::text(
                            ui,
                            row.min + Vec2::new(8.0, 4.0),
                            label,
                            font(12.0, Weight::Medium),
                            theme::foreground(),
                            16.5,
                        );
                        let driver = providers
                            .iter()
                            .find(|p| p["instanceId"] == provider_id.as_str())
                            .and_then(|p| p["driver"].as_str())
                            .unwrap_or_default();
                        if let Some((icon, _)) = provider_icon(driver) {
                            theme::paint_icon(
                                ui,
                                icon,
                                Rect::from_min_size(
                                    row.min + Vec2::new(8.0, 23.0),
                                    Vec2::new(12.0, 16.5),
                                ),
                                12.0,
                                theme::muted_foreground(),
                            );
                        }
                        widgets::text(
                            ui,
                            row.min + Vec2::new(26.0, 25.0),
                            provider_name,
                            font(12.0, Weight::Regular),
                            theme::alpha(theme::muted_foreground(), 0.7),
                            16.5,
                        );
                        let key = format!("{provider_id}/{slug}");
                        let star_rect = Rect::from_min_size(
                            Pos2::new(row.right() - 28.0, row.center().y - 12.0),
                            Vec2::splat(24.0),
                        );
                        let favorite = favorites.contains(&key);
                        if widgets::icon_button(
                            ui,
                            star_rect,
                            ("favorite", &key),
                            if favorite { "star-filled" } else { "star" },
                            12.0,
                            theme::muted_foreground(),
                            if favorite {
                                "Remove from favorites"
                            } else {
                                "Add to favorites"
                            },
                        )
                        .clicked()
                        {
                            toggle_favorite = Some(key);
                        } else if response.clicked() {
                            chosen = Some(ModelSelection {
                                instance_id: provider_id.clone(),
                                model: slug.to_owned(),
                                options: None,
                            });
                        }
                        ui.add_space(2.0);
                    }
                    if !legacy.is_empty() {
                        let (row, _) =
                            ui.allocate_exact_size(Vec2::new(list.width(), 45.0), Sense::hover());
                        let response =
                            ui.interact(row, ui.id().with("legacy-models"), Sense::click());
                        if response.hovered() {
                            ui.painter()
                                .rect_filled(row, CornerRadius::same(6), theme::accent());
                        }
                        widgets::text(
                            ui,
                            row.min + Vec2::new(8.0, 4.0),
                            "Legacy models",
                            font(12.0, Weight::Medium),
                            theme::foreground(),
                            16.5,
                        );
                        let count = format!("{} models", legacy.len());
                        widgets::text(
                            ui,
                            row.min + Vec2::new(8.0, 25.0),
                            &count,
                            font(12.0, Weight::Regular),
                            theme::alpha(theme::muted_foreground(), 0.7),
                            16.5,
                        );
                        theme::paint_icon(
                            ui,
                            if legacy_open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            },
                            Rect::from_min_size(
                                Pos2::new(row.right() - 24.0, row.top()),
                                Vec2::new(16.0, row.height()),
                            ),
                            16.0,
                            theme::muted_foreground(),
                        );
                        if response.clicked() {
                            toggle_legacy = true;
                        }
                    }
                });
        });
        self.open_menu = open;
        self.picker_query = new_query;
        if toggle_legacy {
            self.picker_legacy_open = !self.picker_legacy_open;
        }
        if let Some(provider) = switch_provider {
            self.picker_provider = Some(provider);
        }
        if let Some(key) = toggle_favorite {
            if !self.favorite_models.remove(&key) {
                self.favorite_models.insert(key);
            }
            self.save_favorites();
        }
        if chosen.is_some() {
            self.open_menu = None;
        }
        if self.open_menu.is_none() {
            self.picker_provider = None;
            self.picker_query.clear();
        }
        chosen
    }
}

/// Provider rail button with the active indicator pill on the rail edge.
fn rail_button(
    ui: &mut Ui,
    slot: Rect,
    id: &str,
    icon: &str,
    tint: Color32,
    active: bool,
    rail_right: f32,
) -> eframe::egui::Response {
    let hit = slot.expand(8.0);
    let response = ui.interact(hit, ui.id().with(id), Sense::click());
    if response.hovered() && !active {
        ui.painter()
            .rect_filled(hit, CornerRadius::same(8), theme::accent());
    }
    theme::paint_icon(ui, icon, slot, 20.0, tint);
    if active {
        ui.painter().rect(
            Rect::from_min_size(
                Pos2::new(rail_right - 3.0, slot.top()),
                Vec2::new(3.0, 20.0),
            ),
            CornerRadius {
                nw: 2,
                sw: 2,
                ne: 0,
                se: 0,
            },
            theme::primary(),
            Stroke::NONE,
            StrokeKind::Inside,
        );
    }
    response
}
