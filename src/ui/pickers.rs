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

/// Provider glyph for a driver, with its dark-theme brand tint
/// (`PROVIDER_ICON_BY_PROVIDER` in `providerIconUtils.ts`).
pub fn provider_icon(driver: &str) -> Option<(&'static str, Color32)> {
    match driver {
        "codex" => Some(("openai", theme::foreground())),
        "claudeAgent" => Some(("claude", Color32::from_rgb(0xd9, 0x77, 0x57))),
        "cursor" => Some(("provider-cursor", Color32::from_rgb(0xed, 0xec, 0xec))),
        "grok" => Some(("provider-grok", Color32::from_rgb(0xf5, 0xf5, 0xf5))),
        "opencode" => Some(("opencode", Color32::WHITE)),
        "antigravity" => Some(("provider-antigravity", Color32::WHITE)),
        // Pluto's drivers: full-color marks, Kimi's moon following the theme.
        "kimiCode" => Some((
            if theme::palette().dark {
                "provider-kimi-dark"
            } else {
                "provider-kimi-light"
            },
            Color32::WHITE,
        )),
        "mistralVibe" => Some(("provider-mistral-vibe", Color32::WHITE)),
        // Z Code's monochrome Z follows the text color, like Codex's mark.
        "zcode" => Some(("provider-zcode", theme::foreground())),
        // Ollama's monochrome llama too.
        "ollama" => Some(("provider-ollama", theme::foreground())),
        _ => None,
    }
}

/// Pluto: drivers whose models run only in chats (the Ollama driver's server
/// refuses other projects; their code runs through its OpenCode companion).
const CHAT_ONLY_DRIVERS: [&str; 1] = ["ollama"];

/// Pluto: whether `provider` is the OpenCode instance the server runs beside
/// an Ollama instance for code projects (`<ollama id>_code`, provider-ollama's
/// `codeModels.ts`). It shows as Ollama, in code projects only.
pub(crate) fn is_ollama_companion(providers: &[Value], provider: &Value) -> bool {
    provider["driver"] == "opencode"
        && provider["instanceId"]
            .as_str()
            .and_then(|id| id.strip_suffix("_code"))
            .is_some_and(|base| {
                providers
                    .iter()
                    .any(|p| p["instanceId"] == base && p["driver"] == "ollama")
            })
}

/// A space's remembered models: one per provider, and the provider last picked.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct StickyModel {
    pub by_provider: std::collections::HashMap<String, ModelSelection>,
    pub active: Option<String>,
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
            _ => self.draft_key.clone(),
        }
    }

    fn set_selection(&mut self, draft: bool, selection: ModelSelection) {
        self.remember_model(draft, &selection);
        let key = self.composer_key(draft);
        self.composer_selection.insert(key, selection);
    }

    /// `setStickyModelSelection`: the pick becomes the model new drafts
    /// start on; a model-only pick keeps the provider's last options.
    fn remember_model(&mut self, draft: bool, selection: &ModelSelection) {
        if selection.instance_id.is_empty() || selection.model.is_empty() {
            return;
        }
        let space = if self.chat_context(draft) {
            "chat"
        } else {
            "code"
        };
        let sticky = self.sticky_models.entry(space.to_owned()).or_default();
        let mut next = selection.clone();
        if next.options.is_none() {
            next.options = sticky
                .by_provider
                .get(&selection.instance_id)
                .and_then(|s| s.options.clone());
        }
        let changed = sticky.by_provider.get(&next.instance_id) != Some(&next)
            || sticky.active.as_deref() != Some(next.instance_id.as_str());
        sticky.active = Some(next.instance_id.clone());
        sticky.by_provider.insert(next.instance_id.clone(), next);
        if changed {
            crate::app::save_ui_state(self);
        }
    }

    /// The last model picked in the draft's space, while its provider is
    /// still offered.
    pub(crate) fn sticky_model(&self, draft: bool) -> Option<ModelSelection> {
        let space = if self.chat_context(draft) {
            "chat"
        } else {
            "code"
        };
        let sticky = self.sticky_models.get(space)?;
        let selection = sticky.by_provider.get(sticky.active.as_deref()?)?;
        self.providers()
            .iter()
            .any(|p| p["instanceId"] == selection.instance_id.as_str() && p["enabled"] != false)
            .then(|| selection.clone())
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
            option_of(id).unwrap_or_else(|| descriptor_default(descriptor))
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
        let chat = self.chat_context(draft);
        let locked = if draft { None } else { self.locked_driver() };
        if let Some(chosen) = self.model_picker_popup(
            ui,
            anchor,
            "model-picker",
            selection,
            false,
            chat,
            locked.as_deref(),
        ) {
            self.set_selection(draft, chosen);
        }
    }

    /// `deriveLockedProvider`: a started thread keeps its provider's driver
    /// unless it can hand its context over (`threadSupportsProviderHandoff`).
    fn locked_driver(&self) -> Option<String> {
        let thread = self.current_thread()?;
        let detail = self.current_detail()?;
        let started =
            thread.latest_run.is_some() || thread.runtime.is_some() || !detail.visible.is_empty();
        if !started || crate::ui::queue::supports_handoff(&detail) {
            return None;
        }
        let instance = thread
            .runtime
            .as_ref()
            .map(|r| r.provider_instance_id.clone())
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| thread.model_selection.instance_id.clone());
        self.providers()
            .iter()
            .find(|p| p["instanceId"] == instance.as_str())
            .and_then(|p| p["driver"].as_str().map(str::to_owned))
            .or(Some(instance))
    }

    /// The model picker (`ProviderModelPicker`) open as `menu`, above
    /// `anchor` or below it; returns the model chosen. `chats` offers the
    /// providers that run only in chats (Pluto's `CHAT_ONLY_DRIVERS`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn model_picker_popup(
        &mut self,
        ui: &mut Ui,
        anchor: Rect,
        menu: &str,
        selection: ModelSelection,
        below: bool,
        chats: bool,
        locked: Option<&str>,
    ) -> Option<ModelSelection> {
        if self.open_menu.as_deref() != Some(menu) {
            return None;
        }
        // A locked thread offers its own driver's instances only; the others
        // stay on the rail, inert.
        let unlocked = |p: &Value| locked.is_none_or(|driver| p["driver"] == driver);
        // The rail shows every enabled instance; those not ready are dimmed
        // and list no models (`isProviderInstancePickerReady`).
        let providers = sort_instances(
            self.providers()
                .iter()
                .filter(|p| p["enabled"] != false)
                .filter(|p| {
                    chats || !CHAT_ONLY_DRIVERS.contains(&p["driver"].as_str().unwrap_or(""))
                })
                .filter(|p| !chats || !is_ollama_companion(self.providers(), p))
                .cloned()
                .collect(),
        );
        let ready =
            |p: &Value| p["status"] == "ready" && p["availability"] != "unavailable" && unlocked(p);
        // Each instance's models after its Settings → Providers preferences.
        let model_lists: std::collections::HashMap<String, Vec<Value>> = providers
            .iter()
            .map(|p| {
                let id = p["instanceId"].as_str().unwrap_or_default().to_owned();
                let models = self.picker_models(&id, p);
                (id, models)
            })
            .collect();
        // `shouldOfferModelPickerSetup`: instances that can be installed or
        // signed in stay selectable and show their setup notice.
        let setup: std::collections::HashMap<String, (String, String)> = providers
            .iter()
            .filter(|p| {
                let id = p["instanceId"].as_str().unwrap_or_default();
                let models = model_lists.get(id).map(Vec::as_slice).unwrap_or_default();
                p["enabled"] != false
                    && p["status"] != "disabled"
                    && unlocked(p)
                    && has_provider_setup(p)
                    && (!ready(p)
                        || p["installed"] == false
                        || p["auth"]["status"] == "unauthenticated"
                        || models.iter().all(|m| m["isUnavailable"] == true))
            })
            .map(|p| {
                let id = p["instanceId"].as_str().unwrap_or_default().to_owned();
                let name = p["displayName"].as_str().unwrap_or(&id).to_owned();
                (id, (name, provider_status_message(p)))
            })
            .collect();
        let mut open_setup: Option<String> = None;
        let bindings = self.state.config["keybindings"].clone();
        let jump = self.picker_jump.take();
        // `adjacentModelPickerProvider`: Favorites, then the ready instances,
        // wrapping; the search clears.
        if let Some(direction) = self.picker_cycle.take() {
            let order: Vec<String> = std::iter::once(FAVORITES.to_owned())
                .chain(
                    providers
                        .iter()
                        .filter(|p| ready(p))
                        .filter_map(|p| p["instanceId"].as_str().map(str::to_owned)),
                )
                .collect();
            let current = self.picker_provider.clone().unwrap_or_default();
            let next = match order.iter().position(|id| *id == current) {
                Some(index) => (index as i32 + direction).rem_euclid(order.len() as i32) as usize,
                None if direction > 0 => 0,
                None => order.len() - 1,
            };
            self.picker_provider = Some(order[next].clone());
            self.picker_query.clear();
        }
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
                ("star-filled", theme::foreground()),
                active == FAVORITES,
                true,
                rail.right(),
            )
            .on_hover_text("Favorites")
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
                let driver = if is_ollama_companion(self.providers(), provider) {
                    "ollama"
                } else {
                    provider["driver"].as_str().unwrap_or_default()
                };
                let (icon, tint) =
                    provider_icon(driver).unwrap_or(("circle-alert", theme::muted_foreground()));
                let enabled = ready(provider) || setup.contains_key(&id);
                let slot = Rect::from_min_size(Pos2::new(rail.left() + 12.0, y), Vec2::splat(20.0));
                let response = rail_button(
                    ui,
                    slot,
                    &format!("rail-{id}"),
                    (
                        icon,
                        if enabled {
                            tint
                        } else {
                            theme::alpha(tint, 0.5)
                        },
                    ),
                    active == id,
                    enabled,
                    rail.right(),
                );
                // `describeUnavailableInstance`.
                let label = provider["displayName"].as_str().unwrap_or(&id).to_owned();
                let tooltip = if provider["enabled"] == false || provider["status"] == "disabled" {
                    format!("{label} — Disabled in settings.")
                } else if ready(provider) {
                    label
                } else {
                    let kind = match provider["status"].as_str() {
                        Some("error") => "Unavailable",
                        Some("warning") => "Limited",
                        _ => "Not ready",
                    };
                    match provider["message"]
                        .as_str()
                        .map(str::trim)
                        .filter(|m| !m.is_empty())
                    {
                        Some(message) => format!("{label} — {kind}. {message}"),
                        None => format!("{label} — {kind}."),
                    }
                };
                let response = response.on_hover_text(tooltip);
                if enabled && response.clicked() {
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
            for provider in providers.iter().filter(|p| ready(p)) {
                let id = provider["instanceId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                if active != FAVORITES && active != id && needle.is_empty() {
                    continue;
                }
                let name = provider["displayName"].as_str().unwrap_or(&id).to_owned();
                for model in model_lists.get(&id).into_iter().flatten() {
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
            // `sortProviderModelItems`: within a provider, favorites lead.
            if active != FAVORITES && needle.is_empty() {
                let favorite = |(id, _, m): &(String, String, Value)| {
                    favorites.contains(&format!("{id}/{}", m["slug"].as_str().unwrap_or_default()))
                };
                current.sort_by_key(|entry| !favorite(entry));
                legacy.sort_by_key(|entry| !favorite(entry));
            }
            // The setup notices of the selected instance (or, on an empty
            // Favorites tab, every one), under the list.
            let notices: Vec<(&String, &(String, String))> = if needle.is_empty() {
                setup
                    .iter()
                    .filter(|(id, _)| {
                        **id == active
                            || (active == FAVORITES && current.is_empty() && legacy.is_empty())
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let notice_font = font(12.0, Weight::Regular);
            let notice_heights: Vec<f32> = notices
                .iter()
                .map(|(_, (_, message))| {
                    let lines = widgets::galley(
                        ui,
                        message,
                        notice_font.clone(),
                        theme::muted_foreground(),
                        16.0,
                        content.width() - 1.0 - 16.0 - 8.0,
                    )
                    .rows
                    .len()
                    .min(3) as f32;
                    6.0 + lines * 16.0 + 4.0 + 16.0 + 6.0
                })
                .collect();
            let notices_height = if notices.is_empty() {
                0.0
            } else {
                (notice_heights.iter().sum::<f32>() + 16.0 + 1.0).min(176.0)
            };
            if !notices.is_empty() {
                // `border-t border-border/70 p-2`, then each notice.
                let panel = Rect::from_min_max(
                    Pos2::new(content.left() + 1.0, content.bottom() - notices_height),
                    content.max,
                );
                ui.painter().rect_filled(
                    Rect::from_min_size(panel.min, Vec2::new(panel.width(), 1.0)),
                    0.0,
                    theme::alpha(theme::border(), 0.7),
                );
                let mut y = panel.top() + 1.0 + 8.0;
                for ((id, (name, message)), height) in notices.iter().zip(&notice_heights) {
                    let galley = widgets::galley(
                        ui,
                        message,
                        notice_font.clone(),
                        theme::muted_foreground(),
                        16.0,
                        panel.width() - 16.0 - 8.0,
                    );
                    let text_height = galley.size().y.min(48.0);
                    ui.painter()
                        .with_clip_rect(Rect::from_min_size(
                            Pos2::new(panel.left() + 12.0, y + 6.0),
                            Vec2::new(panel.width() - 24.0, text_height),
                        ))
                        .galley(
                            Pos2::new(panel.left() + 12.0, y + 6.0),
                            galley,
                            theme::muted_foreground(),
                        );
                    let label = if notices.len() > 1 {
                        format!("Set up {name}")
                    } else {
                        "Open provider setup".to_owned()
                    };
                    if crate::ui::setting_inheritance::inline_button(
                        ui,
                        Pos2::new(panel.left() + 12.0, y + 6.0 + text_height + 4.0),
                        ("picker-setup", *id),
                        &label,
                        12.0,
                    ) {
                        open_setup = Some((*id).clone());
                    }
                    y += height;
                }
            }
            let list = Rect::from_min_max(
                Pos2::new(content.left() + 13.0, search.bottom() + 11.0),
                Pos2::new(
                    content.right() - 12.0,
                    content.bottom() - 8.0 - notices_height,
                ),
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
                    // The first nine rows answer `modelPicker.jump.N`.
                    if let Some((provider_id, _, model)) =
                        jump.and_then(|n| rows.get(n.wrapping_sub(1)))
                    {
                        chosen = Some(ModelSelection {
                            instance_id: provider_id.clone(),
                            model: model["slug"].as_str().unwrap_or_default().to_owned(),
                            options: None,
                        });
                    }
                    if rows.is_empty() && legacy.is_empty() && notices.is_empty() {
                        widgets::text(
                            ui,
                            ui.cursor().min + Vec2::new(8.0, 8.0),
                            "No models found",
                            font(12.0, Weight::Regular),
                            theme::muted_foreground(),
                            16.0,
                        );
                    }
                    for (index, (provider_id, provider_name, model)) in rows.into_iter().enumerate()
                    {
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
                        let label_width = widgets::text(
                            ui,
                            row.min + Vec2::new(8.0, 4.0),
                            label,
                            font(12.0, Weight::Medium),
                            theme::foreground(),
                            16.5,
                        );
                        if model["badge"] == "new" {
                            paint_new_badge(ui, row.min + Vec2::new(16.0 + label_width.x, 6.5));
                        }
                        let driver = providers
                            .iter()
                            .find(|p| p["instanceId"] == provider_id.as_str())
                            .and_then(|p| p["driver"].as_str())
                            .unwrap_or_default();
                        if let Some((icon, tint)) = provider_icon(driver) {
                            theme::paint_icon(
                                ui,
                                icon,
                                Rect::from_min_size(
                                    row.min + Vec2::new(8.0, 23.0),
                                    Vec2::new(12.0, 16.5),
                                ),
                                12.0,
                                if driver == "codex" {
                                    theme::muted_foreground()
                                } else {
                                    tint
                                },
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
                        let shortcut = (index < 9)
                            .then(|| {
                                crate::ui::shortcuts::effective_shortcut_label(
                                    &bindings,
                                    &format!("modelPicker.jump.{}", index + 1),
                                    &[("isDesktop", true), ("modelPickerOpen", true)],
                                )
                            })
                            .flatten();
                        if let Some(shortcut) = shortcut {
                            paint_kbd(ui, &shortcut, star_rect.left() + 2.0, row.center().y);
                        }
                        if widgets::icon_button(
                            ui,
                            star_rect,
                            ("favorite", &key),
                            if favorite { "star-filled" } else { "star" },
                            12.0,
                            if favorite {
                                theme::warning()
                            } else {
                                theme::muted_foreground()
                            },
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
        // `openProviderSetup`: Settings → Providers on that instance.
        if let Some(id) = open_setup {
            self.open_menu = None;
            self.open_settings();
            self.open_settings_page(crate::ui::settings::PROVIDERS_PAGE);
            self.providers_selected = Some(id);
        }
        if self.open_menu.is_none() {
            self.picker_provider = None;
            self.picker_query.clear();
        }
        chosen
    }
}

impl App {
    /// `getAppModelOptionsForInstance`: the instance's models without the
    /// ones hidden in Settings → Providers (custom models always show), in
    /// the saved order, then the server's (`sortModelsForProviderInstance`).
    fn picker_models(&self, instance_id: &str, provider: &Value) -> Vec<Value> {
        let prefs = &self.client_settings["providerModelPreferences"][instance_id];
        let list = |key: &str| -> Vec<String> {
            prefs[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        };
        let hidden = list("hiddenModels");
        let order = list("modelOrder");
        let mut models: Vec<Value> = provider["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| {
                m["isCustom"] == true
                    || !hidden
                        .iter()
                        .any(|h| m["slug"].as_str() == Some(h.as_str()))
            })
            .cloned()
            .collect();
        // A stable sort keeps the server order among unranked models.
        models.sort_by_key(|m| {
            m["slug"]
                .as_str()
                .and_then(|slug| order.iter().position(|o| o == slug))
                .unwrap_or(usize::MAX)
        });
        models
    }
}

/// A trait's value when the selection does not set it
/// (`getProviderOptionCurrentValue`): the descriptor's `currentValue`, else,
/// for a select, the option marked `isDefault`.
pub(crate) fn descriptor_default(descriptor: &Value) -> Value {
    let current = &descriptor["currentValue"];
    if !current.is_null() || descriptor["type"] == "boolean" {
        return current.clone();
    }
    descriptor["options"]
        .as_array()
        .and_then(|options| options.iter().find(|o| o["isDefault"] == true))
        .map(|o| o["id"].clone())
        .unwrap_or(Value::Null)
}

/// `formatProviderDriverKindLabel`: "claudeAgent" reads "Claude Agent".
fn driver_kind_label(driver: &str) -> String {
    let mut spaced = String::new();
    let mut previous_lower = false;
    for c in driver.chars() {
        if c.is_ascii_uppercase() && previous_lower {
            spaced.push(' ');
        }
        previous_lower = c.is_ascii_lowercase();
        spaced.push(if c == '_' || c == '-' { ' ' } else { c });
    }
    spaced
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `hasProviderSetup`.
pub(crate) fn has_provider_setup(provider: &Value) -> bool {
    provider["driver"] == "antigravity"
        || provider["setup"]["canAuthenticate"] == true
        || provider["setup"]["canInstall"] == true
}

/// `getProviderStatusBannerKey`: what identifies the provider's status
/// worth a banner; `None` when it is ready (or disabled).
pub(crate) fn provider_status_key(provider: &Value) -> Option<String> {
    let status = provider["status"].as_str()?;
    if status == "disabled" {
        return None;
    }
    let unauthenticated = provider["auth"]["status"] == "unauthenticated";
    let advisory = &provider["compatibilityAdvisory"];
    let incompatible = !(status == "error" && unauthenticated)
        && (advisory["status"] == "broken"
            || (advisory["status"] == "unsupported" && status == "ready"));
    let instance = provider["instanceId"].as_str().unwrap_or_default();
    if incompatible {
        return Some(
            [
                instance,
                advisory["status"].as_str().unwrap_or_default(),
                provider["version"].as_str().unwrap_or_default(),
                advisory["message"].as_str().unwrap_or_default(),
            ]
            .join("\u{0}"),
        );
    }
    if status == "ready" {
        return None;
    }
    if provider["driver"] == "antigravity"
        && provider["installed"] == true
        && status == "warning"
        && provider["auth"]["status"] == "unknown"
    {
        return None;
    }
    Some(
        [
            instance,
            status,
            provider["auth"]["status"].as_str().unwrap_or_default(),
            provider["message"].as_str().unwrap_or_default(),
        ]
        .join("\u{0}"),
    )
}

/// `ProviderStatusBanner`'s title, message and whether it is a warning
/// (rather than an error).
pub(crate) fn provider_status_banner(provider: &Value) -> (String, String, bool) {
    let driver = provider["driver"].as_str().unwrap_or_default();
    let name = provider["displayName"]
        .as_str()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| driver_kind_label(driver));
    let status = provider["status"].as_str().unwrap_or_default();
    let unauthenticated = provider["auth"]["status"] == "unauthenticated";
    let advisory = &provider["compatibilityAdvisory"];
    let broken = advisory["status"] == "broken";
    let incompatible = !(status == "error" && unauthenticated)
        && (broken || (advisory["status"] == "unsupported" && status == "ready"));
    let title = if status == "error" && unauthenticated {
        format!("{name} is unauthenticated")
    } else if incompatible {
        format!(
            "{name} {} is known to be {}",
            provider["version"].as_str().unwrap_or_default(),
            if broken { "broken" } else { "unsupported" }
        )
    } else {
        format!("{name} provider status")
    };
    let message = advisory["message"]
        .as_str()
        .filter(|m| !m.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| provider_status_message(provider));
    let warning = !broken && (status == "warning" || incompatible);
    (title, message, warning)
}

/// `getProviderStatusMessage`: broken-version guidance first, then the
/// server's message, then what setup or sign-in would fix.
fn provider_status_message(provider: &Value) -> String {
    let unauthenticated = provider["auth"]["status"] == "unauthenticated";
    let advisory = &provider["compatibilityAdvisory"];
    if !unauthenticated
        && advisory["status"] == "broken"
        && let Some(message) = advisory["message"].as_str().filter(|m| !m.is_empty())
    {
        return message.to_owned();
    }
    if let Some(message) = provider["message"].as_str().filter(|m| !m.is_empty()) {
        return message.to_owned();
    }
    let driver = provider["driver"].as_str().unwrap_or_default();
    let kind = driver_kind_label(driver);
    let name = provider["displayName"]
        .as_str()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(&kind)
        .to_owned();
    if provider["installed"] == false && has_provider_setup(provider) {
        return format!("Open provider setup to install {kind} on this environment.");
    }
    if unauthenticated {
        if has_provider_setup(provider) {
            return if driver == "antigravity" {
                "Open provider setup to sign in with Google.".into()
            } else {
                "Open provider setup to sign in.".into()
            };
        }
        return "Sign in via the CLI to authenticate again.".into();
    }
    match provider["status"].as_str() {
        Some("ready") => "No models are available for this provider.".into(),
        Some("error") => format!("{name} provider is unavailable."),
        _ => format!("{name} provider has limited availability."),
    }
}

/// Provider rail button with the active indicator pill on the rail edge.
fn rail_button(
    ui: &mut Ui,
    slot: Rect,
    id: &str,
    (icon, tint): (&str, Color32),
    active: bool,
    enabled: bool,
    rail_right: f32,
) -> eframe::egui::Response {
    let hit = slot.expand(8.0);
    let response = ui.interact(hit, ui.id().with(id), Sense::click());
    if response.hovered() && !active && enabled {
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

/// `sortProviderInstanceEntries`: the server's order grouped by driver kind
/// (first appearance), each kind's default instance before its custom ones.
fn sort_instances(providers: Vec<Value>) -> Vec<Value> {
    let mut kinds: Vec<String> = Vec::new();
    for provider in &providers {
        let driver = provider["driver"].as_str().unwrap_or_default().to_owned();
        if !kinds.contains(&driver) {
            kinds.push(driver);
        }
    }
    let mut sorted = Vec::with_capacity(providers.len());
    for kind in &kinds {
        let bucket = providers.iter().filter(|p| p["driver"] == kind.as_str());
        let (defaults, customs): (Vec<&Value>, Vec<&Value>) =
            bucket.partition(|p| p["instanceId"] == kind.as_str());
        sorted.extend(defaults.into_iter().chain(customs).cloned());
    }
    sorted
}

/// The row's "New" tag (`ModelListRow`): 10px bold caps in `update-foreground`
/// on a 15% `update` fill with a 35% border.
fn paint_new_badge(ui: &Ui, min: Pos2) {
    let label_font = font(10.0, Weight::Bold);
    let width = widgets::text_width(ui, "NEW", label_font.clone());
    let rect = Rect::from_min_size(min, Vec2::new(width + 4.0, 12.0));
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::alpha(theme::primary(), 0.15),
        Stroke::new(1.0, theme::alpha(theme::primary(), 0.35)),
        StrokeKind::Inside,
    );
    widgets::text_in(
        ui,
        rect.shrink2(Vec2::new(2.0, 0.0)),
        "NEW",
        label_font,
        theme::BLUE_400,
    );
}

/// A `Kbd` chip (h-5, min-w-5, px-1, `bg-muted`, 12px medium) ending at `right`.
fn paint_kbd(ui: &Ui, label: &str, right: f32, center_y: f32) {
    let label_font = font(12.0, Weight::Medium);
    let text_width = widgets::text_width(ui, label, label_font.clone());
    let width = (text_width + 8.0).max(20.0);
    let rect = Rect::from_min_size(
        Pos2::new(right - 6.0 - width, center_y - 10.0),
        Vec2::new(width, 20.0),
    );
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), theme::muted());
    let text = Rect::from_min_max(
        Pos2::new(rect.center().x - text_width / 2.0, rect.top()),
        rect.max,
    );
    widgets::text_in(ui, text, label, label_font, theme::muted_foreground());
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_ollama_instance_s_opencode_companion_is_recognized() {
        use serde_json::json;
        let providers = vec![
            json!({"instanceId": "ollama_local", "driver": "ollama"}),
            json!({"instanceId": "ollama_local_code", "driver": "opencode"}),
            json!({"instanceId": "work_code", "driver": "opencode"}),
            json!({"instanceId": "opencode", "driver": "opencode"}),
        ];
        let companion = |index: usize| super::is_ollama_companion(&providers, &providers[index]);
        assert!(!companion(0));
        assert!(companion(1));
        // A `_code` OpenCode instance without an Ollama instance of that name is the user's own.
        assert!(!companion(2));
        assert!(!companion(3));
    }

    #[test]
    fn descriptor_default_falls_back_to_the_default_option() {
        use serde_json::json;
        let select = json!({"type": "select", "options": [
            {"id": "low"}, {"id": "medium", "isDefault": true}]});
        assert_eq!(super::descriptor_default(&select), json!("medium"));
        let mut chosen = select.clone();
        chosen["currentValue"] = json!("low");
        assert_eq!(super::descriptor_default(&chosen), json!("low"));
        assert!(super::descriptor_default(&json!({"type": "boolean"})).is_null());
    }

    use super::*;

    #[test]
    fn driver_kinds_read_as_words() {
        assert_eq!(driver_kind_label("claudeAgent"), "Claude Agent");
        assert_eq!(driver_kind_label("opencode"), "Opencode");
    }

    #[test]
    fn status_messages_explain_the_fix() {
        let provider = serde_json::json!({
            "driver": "opencode", "installed": false, "status": "error",
            "auth": {"status": "unknown"}, "setup": {"canInstall": true},
        });
        assert_eq!(
            provider_status_message(&provider),
            "Open provider setup to install Opencode on this environment."
        );
    }

    #[test]
    fn new_drafts_start_on_the_last_model_picked_in_their_space() {
        use crate::app::{App, Options};
        let mut harness = egui_kittest::Harness::builder().build_eframe(|cc| {
            App::new(
                cc,
                Options {
                    server: None,
                    token: None,
                    client_settings_path: None,
                    block_turn_starts: true,
                },
            )
        });
        let model = |slug: &str| json!({"instanceId": "codex", "model": slug});
        let config = json!({"type": "snapshot", "config": {
            "providers": [{"instanceId": "codex", "driver": "codex", "enabled": true,
                "status": "ready", "models": [{"slug": "first", "isDefault": true}]},
                {"instanceId": "grok", "driver": "grok", "enabled": true, "status": "ready"}],
            "settings": {"projectSettingsOverrides":
                {"overridden": {"defaultModelSelection": model("override")}}}}});
        let shell = json!({"kind": "snapshot", "snapshot": {"snapshotSequence": 1,
            "projects": [
                {"id": "plain", "title": "Plain", "workspaceRoot": "/fixture/plain"},
                {"id": "overridden", "title": "Overridden", "workspaceRoot": "/fixture/overridden"}],
            "threads": []}});
        let app = harness.state_mut();
        app.load_offline(&config, &shell).unwrap();
        assert_eq!(app.draft_model_in("plain"), "codex/first");
        // A pick in a chat does not move code drafts.
        app.sticky_models.insert(
            "chat".into(),
            StickyModel {
                by_provider: [(
                    "grok".to_owned(),
                    ModelSelection {
                        instance_id: "grok".into(),
                        model: "grok-4.7".into(),
                        options: None,
                    },
                )]
                .into(),
                active: Some("grok".into()),
            },
        );
        assert_eq!(app.draft_model_in("plain"), "codex/first");
        app.set_selection(
            true,
            ModelSelection {
                instance_id: "codex".into(),
                model: "picked".into(),
                options: Some(json!([{"id": "effort", "value": "high"}])),
            },
        );
        assert_eq!(app.draft_model_in("plain"), "codex/picked");
        // A project's default still wins.
        assert_eq!(app.draft_model_in("overridden"), "codex/override");
        // A model-only pick keeps the provider's options.
        app.set_selection(
            true,
            ModelSelection {
                instance_id: "codex".into(),
                model: "other".into(),
                options: None,
            },
        );
        let sticky = app.sticky_model(true).unwrap();
        assert_eq!(sticky.model, "other");
        assert!(sticky.options.is_some());
    }
}
