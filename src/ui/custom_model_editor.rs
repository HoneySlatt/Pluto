//! The inline editor for one custom model (the pencil on its row): display
//! name plus the option descriptors the composer offers for it. Ported from
//! `components/settings/CustomModelEditor.tsx` and
//! `components/settings/customModelEditor.logic.ts`; the stored shape follows
//! `toCustomModelSetting` in `packages/shared/src/model.ts`.
//!
//! The draft stays local until Save; Escape or Cancel drops it.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Map, Value, json};

use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// `ProviderOptionChoice` being edited; `key` keeps text fields stable.
struct Choice {
    key: u64,
    id: String,
    label: String,
    is_default: bool,
    description: Option<String>,
}

/// `ProviderOptionDescriptor` being edited.
struct Descriptor {
    key: u64,
    boolean: bool,
    id: String,
    label: String,
    choices: Vec<Choice>,
    current_boolean: Option<bool>,
    description: Option<String>,
}

/// `DescriptorPreset`: an option id a driver reads, with its usual choices.
struct Preset {
    id: &'static str,
    label: &'static str,
    boolean: bool,
    choices: &'static [(&'static str, &'static str, bool)],
}

const EFFORT: &[(&str, &str, bool)] = &[
    ("low", "Low", false),
    ("medium", "Medium", true),
    ("high", "High", false),
    ("xhigh", "Extra High", false),
];

const fn select(
    id: &'static str,
    label: &'static str,
    choices: &'static [(&'static str, &'static str, bool)],
) -> Preset {
    Preset {
        id,
        label,
        boolean: false,
        choices,
    }
}

const fn toggle(id: &'static str, label: &'static str) -> Preset {
    Preset {
        id,
        label,
        boolean: true,
        choices: &[],
    }
}

/// `DESCRIPTOR_PRESETS_BY_KIND`.
fn presets(driver: &str) -> &'static [Preset] {
    const CODEX: &[Preset] = &[
        select("reasoningEffort", "Reasoning", EFFORT),
        select(
            "serviceTier",
            "Speed",
            &[("default", "Standard", true), ("fast", "Fast", false)],
        ),
    ];
    const CLAUDE: &[Preset] = &[
        select(
            "effort",
            "Reasoning",
            &[
                ("low", "Low", false),
                ("medium", "Medium", false),
                ("high", "High", true),
                ("xhigh", "Extra High", false),
                ("max", "Max", false),
            ],
        ),
        toggle("fastMode", "Fast Mode"),
        toggle("thinking", "Thinking"),
    ];
    const CURSOR: &[Preset] = &[
        select("reasoning", "Reasoning", EFFORT),
        toggle("fastMode", "Fast Mode"),
        toggle("thinking", "Thinking"),
    ];
    const GROK: &[Preset] = &[select("reasoningEffort", "Reasoning", EFFORT)];
    const OPENCODE: &[Preset] = &[
        select("variant", "Reasoning", EFFORT),
        select(
            "agent",
            "Agent",
            &[("build", "Build", true), ("plan", "Plan", false)],
        ),
    ];
    match driver {
        "codex" => CODEX,
        "claudeAgent" => CLAUDE,
        "cursor" => CURSOR,
        "grok" => GROK,
        "opencode" => OPENCODE,
        _ => &[],
    }
}

pub(crate) struct CustomModelEditor {
    /// The provider instance and the model being edited.
    pub(crate) row: String,
    pub(crate) slug: String,
    driver: String,
    name: String,
    descriptors: Vec<Descriptor>,
    error: Option<String>,
    /// The open select: "id:<key>", "type:<key>" or "copy".
    select: Option<String>,
    next_key: u64,
    focused: bool,
}

pub(crate) enum EditorEvent {
    None,
    /// The stored `customModels` entry (`toCustomModelSetting`).
    Save(Value),
    Cancel,
}

fn text_of(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

impl CustomModelEditor {
    /// `draftFromDefinition` for a stored entry (a slug or `{slug, name, capabilities}`).
    pub(crate) fn new(row: &str, driver: &str, entry: &Value) -> Self {
        let slug = entry
            .as_str()
            .or(entry["slug"].as_str())
            .unwrap_or_default()
            .trim()
            .to_owned();
        let name = entry["name"].as_str().map(str::trim).unwrap_or_default();
        let mut editor = Self {
            row: row.to_owned(),
            name: if name == slug {
                String::new()
            } else {
                name.to_owned()
            },
            slug,
            driver: driver.to_owned(),
            descriptors: Vec::new(),
            error: None,
            select: None,
            next_key: 0,
            focused: false,
        };
        let descriptors: Vec<Value> = entry["capabilities"]["optionDescriptors"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        editor.descriptors = descriptors
            .iter()
            .map(|d| editor.descriptor_to_editor(d))
            .collect();
        editor
    }

    fn key(&mut self) -> u64 {
        self.next_key += 1;
        self.next_key
    }

    /// `descriptorToEditor`: prompt-injected choices are dropped, and the
    /// current value (else the flagged default) becomes the default choice.
    fn descriptor_to_editor(&mut self, descriptor: &Value) -> Descriptor {
        let boolean = descriptor["type"] == "boolean";
        let injected: Vec<&str> = descriptor["promptInjectedValues"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let options: Vec<&Value> = if boolean {
            Vec::new()
        } else {
            descriptor["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|o| !injected.contains(&o["id"].as_str().unwrap_or_default()))
                .collect()
        };
        let default = options
            .iter()
            .position(|o| {
                descriptor["currentValue"].is_string() && o["id"] == descriptor["currentValue"]
            })
            .or_else(|| options.iter().position(|o| o["isDefault"] == true));
        let key = self.key();
        let choices = options
            .iter()
            .enumerate()
            .map(|(index, option)| Choice {
                key: self.key(),
                id: option["id"].as_str().unwrap_or_default().to_owned(),
                label: option["label"].as_str().unwrap_or_default().to_owned(),
                is_default: Some(index) == default,
                description: text_of(&option["description"]),
            })
            .collect();
        Descriptor {
            key,
            boolean,
            id: descriptor["id"].as_str().unwrap_or_default().to_owned(),
            label: descriptor["label"].as_str().unwrap_or_default().to_owned(),
            choices,
            current_boolean: if boolean {
                descriptor["currentValue"].as_bool()
            } else {
                None
            },
            description: text_of(&descriptor["description"]),
        }
    }

    fn preset_choices(&mut self, preset: &Preset) -> Vec<Choice> {
        preset
            .choices
            .iter()
            .map(|(id, label, is_default)| Choice {
                key: self.key(),
                id: (*id).to_owned(),
                label: (*label).to_owned(),
                is_default: *is_default,
                description: None,
            })
            .collect()
    }

    /// `descriptorFromPreset`.
    fn descriptor_from_preset(&mut self, preset: &Preset) -> Descriptor {
        Descriptor {
            key: self.key(),
            boolean: preset.boolean,
            id: preset.id.to_owned(),
            label: preset.label.to_owned(),
            choices: self.preset_choices(preset),
            current_boolean: None,
            description: None,
        }
    }

    /// `validateDraft`: the first problem in reading order.
    fn validate(&self) -> Option<String> {
        let mut seen: Vec<&str> = Vec::new();
        for (index, descriptor) in self.descriptors.iter().enumerate() {
            let position = format!("Option {}", index + 1);
            let id = descriptor.id.trim();
            if id.is_empty() {
                return Some(format!("{position} needs an id."));
            }
            if seen.contains(&id) {
                return Some(format!("{position}: id \"{id}\" is used twice."));
            }
            seen.push(id);
            if descriptor.label.trim().is_empty() {
                return Some(format!("{position} needs a label."));
            }
            if descriptor.boolean {
                continue;
            }
            if descriptor.choices.is_empty() {
                return Some(format!("{position} needs at least one choice."));
            }
            let mut choices: Vec<&str> = Vec::new();
            for choice in &descriptor.choices {
                let choice_id = choice.id.trim();
                if choice_id.is_empty() {
                    return Some(format!("{position} has a choice without a value."));
                }
                if choices.contains(&choice_id) {
                    return Some(format!("{position}: choice \"{choice_id}\" is used twice."));
                }
                choices.push(choice_id);
            }
        }
        None
    }

    /// `definitionFromDraft` then `toCustomModelSetting`: a bare slug unless
    /// it has a name of its own or options.
    fn setting(&self) -> Value {
        let descriptors: Vec<Value> = self
            .descriptors
            .iter()
            .map(|descriptor| {
                let mut out = Map::new();
                out.insert("id".into(), json!(descriptor.id.trim()));
                out.insert("label".into(), json!(descriptor.label.trim()));
                out.insert(
                    "type".into(),
                    json!(if descriptor.boolean {
                        "boolean"
                    } else {
                        "select"
                    }),
                );
                if let Some(description) = &descriptor.description {
                    out.insert("description".into(), json!(description));
                }
                if descriptor.boolean {
                    if let Some(current) = descriptor.current_boolean {
                        out.insert("currentValue".into(), json!(current));
                    }
                } else {
                    let options: Vec<Value> = descriptor
                        .choices
                        .iter()
                        .map(|choice| {
                            let id = choice.id.trim();
                            let label = choice.label.trim();
                            let mut option = Map::new();
                            option.insert("id".into(), json!(id));
                            option.insert(
                                "label".into(),
                                json!(if label.is_empty() { id } else { label }),
                            );
                            if let Some(description) = &choice.description {
                                option.insert("description".into(), json!(description));
                            }
                            if choice.is_default {
                                option.insert("isDefault".into(), json!(true));
                            }
                            Value::Object(option)
                        })
                        .collect();
                    if let Some(current) = descriptor
                        .choices
                        .iter()
                        .find(|c| c.is_default)
                        .map(|c| c.id.trim())
                    {
                        out.insert("currentValue".into(), json!(current));
                    }
                    out.insert("options".into(), Value::Array(options));
                }
                Value::Object(out)
            })
            .collect();
        let name = self.name.trim();
        if (name.is_empty() || name == self.slug) && descriptors.is_empty() {
            return json!(self.slug);
        }
        let mut setting = Map::new();
        setting.insert("slug".into(), json!(self.slug));
        if !name.is_empty() && name != self.slug {
            setting.insert("name".into(), json!(name));
        }
        if !descriptors.is_empty() {
            setting.insert(
                "capabilities".into(),
                json!({"optionDescriptors": descriptors}),
            );
        }
        Value::Object(setting)
    }

    fn descriptor_height(descriptor: &Descriptor) -> f32 {
        let choices = if descriptor.boolean {
            0.0
        } else {
            let n = descriptor.choices.len() as f32;
            8.0 + if n > 0.0 {
                n * 30.0 + (n - 1.0) * 6.0 + 6.0
            } else {
                0.0
            } + 24.0
        };
        1.0 + 10.0 + 30.0 + choices + 10.0 + 1.0
    }

    /// The editor's height, including its `mt-1 mb-2` margins.
    pub(crate) fn height(&self, has_candidates: bool) -> f32 {
        let mut options = if has_candidates { 28.0 } else { 16.0 } + 8.0;
        if self.descriptors.is_empty() {
            options += 16.0 + 8.0;
        }
        for descriptor in &self.descriptors {
            options += Self::descriptor_height(descriptor) + 8.0;
        }
        options += 24.0;
        let error = if self.error.is_some() {
            16.0 + 12.0
        } else {
            0.0
        };
        4.0 + 13.0 + 48.0 + 12.0 + options + 12.0 + error + 28.0 + 13.0 + 8.0
    }

    /// Paints the editor at `rect` (its border box) and handles its input.
    /// `candidates` are the built-in models offered by "Copy from…".
    pub(crate) fn show(&mut self, ui: &mut Ui, rect: Rect, candidates: &[Value]) -> EditorEvent {
        let presets = presets(&self.driver);
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            // `bg-muted/20`.
            theme::overlay(0.008),
            Stroke::new(1.0, theme::border()),
            StrokeKind::Inside,
        );
        let left = rect.left() + 13.0;
        let right = rect.right() - 13.0;
        let mut y = rect.top() + 13.0;
        let muted = theme::muted_foreground();

        widgets::text(
            ui,
            Pos2::new(left, y),
            "Display name",
            font(12.0, Weight::Regular),
            muted,
            16.0,
        );
        y += 20.0;
        let name_rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(288.0, 28.0));
        let slug = self.slug.clone();
        let response = input(
            ui,
            name_rect,
            (&self.row, &self.slug, "name"),
            &mut self.name,
            &slug,
            font(14.0, Weight::Regular),
        );
        if !self.focused {
            response.request_focus();
            self.focused = true;
        }
        y += 28.0 + 12.0;

        // "Options shown in the composer" and "Copy from…".
        let mut copy_from: Option<usize> = None;
        if candidates.is_empty() {
            widgets::text(
                ui,
                Pos2::new(left, y),
                "Options shown in the composer",
                font(12.0, Weight::Regular),
                muted,
                16.0,
            );
            y += 16.0 + 8.0;
        } else {
            widgets::text(
                ui,
                Pos2::new(left, y + 6.0),
                "Options shown in the composer",
                font(12.0, Weight::Regular),
                muted,
                16.0,
            );
            let trigger = Rect::from_min_size(Pos2::new(right - 176.0, y), Vec2::new(176.0, 28.0));
            if select_trigger(ui, trigger, (&self.row, "copy"), "Copy from…", false) {
                toggle_select(&mut self.select, "copy");
            }
            let items: Vec<(Option<String>, String)> = candidates
                .iter()
                .map(|m| {
                    (
                        None,
                        m["name"]
                            .as_str()
                            .or(m["slug"].as_str())
                            .unwrap_or_default()
                            .to_owned(),
                    )
                })
                .collect();
            copy_from = select_popup(ui, &mut self.select, "copy", trigger, &items, None, true);
            y += 28.0 + 8.0;
        }
        if self.descriptors.is_empty() {
            widgets::text(
                ui,
                Pos2::new(left, y),
                "No custom options. The composer uses the provider's default options.",
                font(12.0, Weight::Regular),
                theme::alpha(muted, 0.7),
                16.0,
            );
            y += 16.0 + 8.0;
        }

        let mut remove: Option<usize> = None;
        let mut set_preset: Option<(usize, Option<usize>)> = None;
        let mut set_type: Option<(usize, bool)> = None;
        let mut add_choice: Option<usize> = None;
        let mut remove_choice: Option<(usize, usize)> = None;
        let mut make_default: Option<(usize, usize, bool)> = None;
        let mut edited = false;
        let row_id = self.row.clone();
        for index in 0..self.descriptors.len() {
            let height = Self::descriptor_height(&self.descriptors[index]);
            let boxed = Rect::from_min_size(Pos2::new(left, y), Vec2::new(right - left, height));
            ui.painter().rect(
                boxed,
                CornerRadius::same(8),
                theme::alpha(theme::background(), 0.4),
                Stroke::new(1.0, theme::alpha(theme::border(), 0.6)),
                StrokeKind::Inside,
            );
            let descriptor = &mut self.descriptors[index];
            let key = descriptor.key;
            let inner_left = boxed.left() + 11.0;
            let inner_right = boxed.right() - 11.0;
            let header = boxed.top() + 11.0;
            let center = header + 15.0;
            widgets::text(
                ui,
                Pos2::new(inner_left, center - 8.0),
                &format!("Option {}", index + 1),
                font(11.0, Weight::Regular),
                muted,
                16.0,
            );
            // Remove, type select, label, then the id select and custom id.
            let close = Rect::from_min_size(
                Pos2::new(inner_right - 20.0, center - 10.0),
                Vec2::splat(20.0),
            );
            if micro_x(ui, close, (&row_id, key, "remove")) {
                remove = Some(index);
            }
            let type_trigger = Rect::from_min_size(
                Pos2::new(close.left() - 8.0 - 144.0, center - 14.0),
                Vec2::new(144.0, 28.0),
            );
            let type_label = if descriptor.boolean {
                "Toggle"
            } else {
                "Choices"
            };
            let type_id = format!("type:{key}");
            if select_trigger(ui, type_trigger, (&row_id, key, "type"), type_label, false) {
                toggle_select(&mut self.select, &type_id);
            }
            let mut x = inner_left + 56.0 + 8.0;
            let preset_index = presets.iter().position(|p| p.id == descriptor.id);
            let id_id = format!("id:{key}");
            let mut id_trigger = None;
            if !presets.is_empty() {
                let trigger =
                    Rect::from_min_size(Pos2::new(x, center - 14.0), Vec2::new(160.0, 28.0));
                let shown = preset_index.map_or("Custom…", |i| presets[i].id);
                if select_trigger(ui, trigger, (&row_id, key, "id"), shown, false) {
                    toggle_select(&mut self.select, &id_id);
                }
                id_trigger = Some(trigger);
                x = trigger.right() + 8.0;
            }
            if preset_index.is_none() {
                let field =
                    Rect::from_min_size(Pos2::new(x, center - 15.0), Vec2::new(144.0, 30.0));
                edited |= input(
                    ui,
                    field,
                    (&row_id, key, "id"),
                    &mut descriptor.id,
                    "optionId",
                    theme::mono(12.0),
                )
                .changed();
                x = field.right() + 8.0;
            }
            let label_field = Rect::from_min_max(
                Pos2::new(x, center - 15.0),
                Pos2::new((type_trigger.left() - 8.0).max(x), center + 15.0),
            );
            edited |= input(
                ui,
                label_field,
                (&row_id, key, "label"),
                &mut descriptor.label,
                "Label",
                font(12.0, Weight::Regular),
            )
            .changed();

            if !descriptor.boolean {
                let mut cy = header + 30.0 + 8.0;
                let choice_left = inner_left + 64.0;
                for (choice_index, choice) in descriptor.choices.iter_mut().enumerate() {
                    let ckey = choice.key;
                    let value =
                        Rect::from_min_size(Pos2::new(choice_left, cy), Vec2::new(112.0, 30.0));
                    edited |= input(
                        ui,
                        value,
                        (&row_id, ckey, "value"),
                        &mut choice.id,
                        "value",
                        theme::mono(12.0),
                    )
                    .changed();
                    let remove_rect = Rect::from_min_size(
                        Pos2::new(inner_right - 20.0, cy + 5.0),
                        Vec2::splat(20.0),
                    );
                    if micro_x(ui, remove_rect, (&row_id, ckey, "remove")) {
                        remove_choice = Some((index, choice_index));
                    }
                    let default_width = 26.0
                        + 6.0
                        + widgets::text_width(ui, "Default", font(11.0, Weight::Regular));
                    let default_left = remove_rect.left() - 8.0 - default_width;
                    let track = Rect::from_min_size(
                        Pos2::new(default_left, cy + 7.0),
                        Vec2::new(26.0, 16.0),
                    );
                    if small_switch(ui, track, (&row_id, ckey, "default"), choice.is_default) {
                        make_default = Some((index, choice_index, !choice.is_default));
                    }
                    widgets::text(
                        ui,
                        Pos2::new(track.right() + 6.0, cy + 7.0),
                        "Default",
                        font(11.0, Weight::Regular),
                        muted,
                        16.0,
                    );
                    let label = Rect::from_min_max(
                        Pos2::new(value.right() + 8.0, cy),
                        Pos2::new(default_left - 8.0, cy + 30.0),
                    );
                    edited |= input(
                        ui,
                        label,
                        (&row_id, ckey, "label"),
                        &mut choice.label,
                        "Label",
                        font(12.0, Weight::Regular),
                    )
                    .changed();
                    cy += 36.0;
                }
                let add =
                    Rect::from_min_size(Pos2::new(choice_left - 8.0, cy), Vec2::new(0.0, 24.0));
                if ghost_xs(ui, add, (&row_id, key, "add-choice"), "Add choice").1 {
                    add_choice = Some(index);
                }
            }
            if let Some(trigger) = id_trigger {
                let mut items: Vec<(Option<String>, String)> = presets
                    .iter()
                    .map(|p| (Some(p.id.to_owned()), p.label.to_owned()))
                    .collect();
                items.push((None, "Custom…".to_owned()));
                let current = Some(preset_index.unwrap_or(presets.len()));
                if let Some(chosen) = select_popup(
                    ui,
                    &mut self.select,
                    &id_id,
                    trigger,
                    &items,
                    current,
                    false,
                ) {
                    set_preset = Some((index, (chosen < presets.len()).then_some(chosen)));
                }
            }
            let items = vec![(None, "Choices".to_owned()), (None, "Toggle".to_owned())];
            let current = Some(usize::from(self.descriptors[index].boolean));
            if let Some(chosen) = select_popup(
                ui,
                &mut self.select,
                &type_id,
                type_trigger,
                &items,
                current,
                true,
            ) {
                set_type = Some((index, chosen == 1));
            }
            y += height + 8.0;
        }

        // Presets not used yet, then "Custom option".
        let mut add_preset: Option<usize> = None;
        let mut add_custom = false;
        let mut x = left;
        let mut first = true;
        for (index, preset) in presets.iter().enumerate() {
            if self.descriptors.iter().any(|d| d.id == preset.id) {
                continue;
            }
            if !first {
                x -= 8.0;
            }
            first = false;
            let (rect, clicked) = ghost_xs(
                ui,
                Rect::from_min_size(Pos2::new(x, y), Vec2::new(0.0, 24.0)),
                (&row_id, "preset", index),
                preset.label,
            );
            if clicked {
                add_preset = Some(index);
            }
            x = rect.right() + 4.0;
        }
        if ghost_xs(
            ui,
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(0.0, 24.0)),
            (&row_id, "custom-option"),
            "Custom option",
        )
        .1
        {
            add_custom = true;
        }
        y += 24.0 + 12.0;

        if let Some(error) = &self.error {
            widgets::text(
                ui,
                Pos2::new(left, y),
                error,
                font(12.0, Weight::Regular),
                Color32::from_rgb(251, 65, 74),
                16.0,
            );
            y += 16.0 + 12.0;
        }
        let save_w = (widgets::text_width(ui, "Save", font(14.0, Weight::Medium)) + 20.0).round();
        let save = Rect::from_min_size(Pos2::new(left, y), Vec2::new(save_w, 28.0));
        let cancel_w =
            (widgets::text_width(ui, "Cancel", font(14.0, Weight::Medium)) + 20.0).round();
        let cancel =
            Rect::from_min_size(Pos2::new(save.right() + 8.0, y), Vec2::new(cancel_w, 28.0));
        let save_clicked = small_button(ui, save, (&row_id, "save"), "Save", true);
        let cancel_clicked = small_button(ui, cancel, (&row_id, "cancel"), "Cancel", false);

        // Apply this frame's edits.
        if edited
            || remove.is_some()
            || set_preset.is_some()
            || set_type.is_some()
            || add_choice.is_some()
            || remove_choice.is_some()
            || make_default.is_some()
            || add_preset.is_some()
            || add_custom
            || copy_from.is_some()
        {
            self.error = None;
        }
        if let Some((index, preset)) = set_preset {
            match preset {
                // `applyPresetId`: the preset's label, type and choices.
                Some(p) => {
                    let choices = self.preset_choices(&presets[p]);
                    let descriptor = &mut self.descriptors[index];
                    descriptor.id = presets[p].id.to_owned();
                    descriptor.label = presets[p].label.to_owned();
                    descriptor.boolean = presets[p].boolean;
                    descriptor.choices = choices;
                    descriptor.current_boolean = None;
                    descriptor.description = None;
                }
                None => self.descriptors[index].id.clear(),
            }
        }
        if let Some((index, boolean)) = set_type {
            self.descriptors[index].boolean = boolean;
        }
        if let Some((index, choice, on)) = make_default {
            for (i, c) in self.descriptors[index].choices.iter_mut().enumerate() {
                if i == choice {
                    c.is_default = on;
                } else if on {
                    // Only one choice can be the default.
                    c.is_default = false;
                }
            }
        }
        if let Some((index, choice)) = remove_choice {
            self.descriptors[index].choices.remove(choice);
        }
        if let Some(index) = add_choice {
            let key = self.key();
            self.descriptors[index].choices.push(Choice {
                key,
                id: String::new(),
                label: String::new(),
                is_default: false,
                description: None,
            });
        }
        if let Some(index) = remove {
            self.descriptors.remove(index);
        }
        if let Some(index) = add_preset {
            let descriptor = self.descriptor_from_preset(&presets[index]);
            self.descriptors.push(descriptor);
        }
        if add_custom {
            let key = self.key();
            self.descriptors.push(Descriptor {
                key,
                boolean: false,
                id: String::new(),
                label: String::new(),
                choices: Vec::new(),
                current_boolean: None,
                description: None,
            });
        }
        if let Some(index) = copy_from {
            // `descriptorsFromCapabilities`: Claude's context window needs
            // runtime mappings a custom entry does not carry.
            let claude = self.driver == "claudeAgent";
            let descriptors: Vec<Value> = candidates[index]["capabilities"]["optionDescriptors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| !(claude && d["id"] == "contextWindow"))
                .cloned()
                .collect();
            self.descriptors = descriptors
                .iter()
                .map(|d| self.descriptor_to_editor(d))
                .collect();
        }
        if cancel_clicked
            || (self.select.is_none() && ui.input(|i| i.key_pressed(egui::Key::Escape)))
        {
            return EditorEvent::Cancel;
        }
        if save_clicked {
            match self.validate() {
                Some(problem) => self.error = Some(problem),
                None => return EditorEvent::Save(self.setting()),
            }
        }
        EditorEvent::None
    }
}

fn toggle_select(open: &mut Option<String>, id: &str) {
    *open = if open.as_deref() == Some(id) {
        None
    } else {
        Some(id.to_owned())
    };
}

/// An `Input`: `size="sm"` (28px) or `size="compact"` (30px) with 10px insets.
fn input(
    ui: &mut Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    placeholder: &str,
    text_font: egui::FontId,
) -> egui::Response {
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        theme::control_surface(),
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let field = Rect::from_min_max(
        Pos2::new(rect.left() + 10.0, rect.top() + 1.0),
        Pos2::new(
            (rect.right() - 10.0).max(rect.left() + 10.0),
            rect.bottom() - 1.0,
        ),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
    child.set_clip_rect(field.intersect(ui.clip_rect()));
    let response = child.add(
        egui::TextEdit::singleline(text)
            .id(egui::Id::new(("custom-model-editor", id)))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(text_font.clone())
            .text_color(theme::foreground())
            .vertical_align(egui::Align::Center)
            .desired_width(field.width())
            .min_size(Vec2::new(0.0, field.height())),
    );
    // Painted like the settings fields: egui's hint text renders lighter.
    if text.is_empty() {
        let clipped = ui.painter().with_clip_rect(field.intersect(ui.clip_rect()));
        let galley = ui.painter().layout_no_wrap(
            placeholder.to_owned(),
            text_font,
            theme::muted_foreground(),
        );
        let pos = Pos2::new(field.left(), field.center().y - galley.size().y / 2.0);
        clipped.galley(pos, galley, theme::muted_foreground());
    }
    response
}

/// `SelectTrigger size="compact"`: the value at 8px and a chevron.
fn select_trigger(
    ui: &Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    _open: bool,
) -> bool {
    let response = ui.interact(rect, ui.id().with(("cme-select", id)), Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        if response.hovered() {
            theme::overlay(0.05)
        } else {
            theme::control_surface()
        },
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.left() + 8.0, rect.top()),
            Pos2::new(rect.right() - 24.0, rect.bottom()),
        ),
        label,
        font(14.0, Weight::Regular),
        theme::foreground(),
    );
    theme::paint_icon(
        ui,
        "chevron-down",
        Rect::from_center_size(
            Pos2::new(rect.right() - 14.0, rect.center().y),
            Vec2::splat(12.0),
        ),
        12.0,
        theme::alpha(theme::muted_foreground(), 0.8),
    );
    response.clicked()
}

/// `SelectPopup`: 28px items (a mono id, then the muted label), under the
/// trigger, at least as wide as it; returns the chosen index.
fn select_popup(
    ui: &mut Ui,
    open: &mut Option<String>,
    id: &str,
    anchor: Rect,
    items: &[(Option<String>, String)],
    current: Option<usize>,
    end: bool,
) -> Option<usize> {
    if open.as_deref() != Some(id) {
        return None;
    }
    let widths: Vec<f32> = items
        .iter()
        .map(|(code, label)| {
            code.as_ref()
                .map_or(0.0, |c| widgets::text_width(ui, c, theme::mono(12.0)) + 8.0)
                + widgets::text_width(ui, label, font(14.0, Weight::Regular))
        })
        .collect();
    let width = (widths.iter().cloned().fold(0.0, f32::max) + 16.0 + 8.0 + 2.0)
        .max(anchor.width())
        .ceil();
    let height = items.len() as f32 * 28.0 + 8.0 + 2.0;
    let x = if end {
        anchor.right() - 1.0 - width
    } else {
        anchor.left() + 1.0
    };
    let rect = Rect::from_min_size(
        Pos2::new(x, anchor.bottom() + 5.5),
        Vec2::new(width, height),
    );
    let mut chosen = None;
    widgets::popup(ui, open, id, rect, anchor, |ui, inner| {
        let hovered = items.iter().enumerate().position(|(index, _)| {
            ui.rect_contains_pointer(Rect::from_min_size(
                Pos2::new(inner.left() + 5.0, inner.top() + 5.0 + index as f32 * 28.0),
                Vec2::new(inner.width() - 10.0, 28.0),
            ))
        });
        for (index, (code, label)) in items.iter().enumerate() {
            let row = Rect::from_min_size(
                Pos2::new(inner.left() + 5.0, inner.top() + 5.0 + index as f32 * 28.0),
                Vec2::new(inner.width() - 10.0, 28.0),
            );
            let response =
                ui.interact(row, ui.id().with(("cme-option", id, index)), Sense::click());
            if hovered.or(current) == Some(index) {
                ui.painter()
                    .rect_filled(row, CornerRadius::same(6), theme::overlay(0.078));
            }
            let mut tx = row.left() + 8.0;
            if let Some(code) = code {
                tx += widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(tx, row.top()), row.max),
                    code,
                    theme::mono(12.0),
                    theme::foreground(),
                ) + 8.0;
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(tx, row.top()), row.max),
                    label,
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                );
            } else {
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(tx, row.top()), row.max),
                    label,
                    font(14.0, Weight::Regular),
                    theme::foreground(),
                );
            }
            if response.clicked() {
                chosen = Some(index);
            }
        }
    });
    if chosen.is_some() {
        *open = None;
    }
    chosen
}

/// `Button size="xs" variant="ghost-muted"` with a plus: 24px tall, sized to
/// its label; returns its rect and whether it was clicked.
fn ghost_xs(
    ui: &Ui,
    at: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
) -> (Rect, bool) {
    let width = widgets::text_width(ui, label, font(12.0, Weight::Regular)) + 30.0;
    let rect = Rect::from_min_size(at.min, Vec2::new(width.round(), 24.0));
    let response = ui.interact(rect, ui.id().with(("cme-xs", id)), Sense::click());
    let color = if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), theme::accent());
        theme::foreground()
    } else {
        theme::muted_foreground()
    };
    theme::paint_icon(
        ui,
        "plus",
        Rect::from_min_size(
            Pos2::new(rect.left() + 6.0, rect.center().y - 6.0),
            Vec2::splat(12.0),
        ),
        12.0,
        color,
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(rect.left() + 20.0, rect.top()), rect.max),
        label,
        font(12.0, Weight::Regular),
        color,
    );
    (rect, response.clicked())
}

/// `Button size="icon-micro" variant="ghost-muted"` with an X.
fn micro_x(ui: &Ui, rect: Rect, id: impl std::hash::Hash + std::fmt::Debug) -> bool {
    let response = ui.interact(rect, ui.id().with(("cme-x", id)), Sense::click());
    let color = if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), theme::accent());
        theme::foreground()
    } else {
        theme::muted_foreground()
    };
    theme::paint_icon(
        ui,
        "x",
        Rect::from_center_size(rect.center(), Vec2::splat(12.0)),
        12.0,
        color,
    );
    response.clicked()
}

/// `Switch size="sm"`: a 26×16 track with a 12px thumb.
fn small_switch(
    ui: &Ui,
    track: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    on: bool,
) -> bool {
    let response = ui.interact(track, ui.id().with(("cme-switch", id)), Sense::click());
    ui.painter().rect_filled(
        track,
        CornerRadius::same(8),
        if on {
            theme::primary()
        } else {
            theme::overlay(0.08)
        },
    );
    let thumb_x = track.left() + if on { 12.0 } else { 2.0 };
    ui.painter().rect_filled(
        Rect::from_min_size(Pos2::new(thumb_x, track.top() + 2.0), Vec2::splat(12.0)),
        CornerRadius::same(6),
        theme::background(),
    );
    response.clicked()
}

/// `Button size="sm"`: outline (Save) or ghost (Cancel).
fn small_button(
    ui: &Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    outline: bool,
) -> bool {
    let response = ui.interact(rect, ui.id().with(("cme-button", id)), Sense::click());
    if outline {
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            if response.hovered() {
                theme::overlay(0.06)
            } else {
                theme::control_surface()
            },
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
    } else if response.hovered() {
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
        theme::foreground(),
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drafts_validate_and_store_like_upstream() {
        let bare = CustomModelEditor::new("codex", "codex", &json!("my-model"));
        assert_eq!(bare.setting(), json!("my-model"));
        let mut editor = CustomModelEditor::new(
            "codex",
            "codex",
            &json!({"slug": "m", "name": "M", "capabilities": {"optionDescriptors": [
                {"id": "effort", "label": "Effort", "type": "select", "currentValue": "high",
                 "promptInjectedValues": ["ultrathink"],
                 "options": [{"id": "low", "label": "Low", "isDefault": true}, {"id": "high", "label": ""}, {"id": "ultrathink", "label": "U"}]}
            ]}}),
        );
        assert_eq!(editor.validate(), None);
        assert_eq!(
            editor.setting(),
            json!({"slug": "m", "name": "M", "capabilities": {"optionDescriptors": [
                {"id": "effort", "label": "Effort", "type": "select", "currentValue": "high",
                 "options": [{"id": "low", "label": "Low"}, {"id": "high", "label": "high", "isDefault": true}]}
            ]}})
        );
        let preset = editor.descriptor_from_preset(&presets("codex")[1]);
        editor.descriptors.push(preset);
        editor.descriptors[1].id = "effort".into();
        assert_eq!(
            editor.validate().as_deref(),
            Some("Option 2: id \"effort\" is used twice.")
        );
        editor.descriptors[1].id = "speed".into();
        editor.descriptors[1].choices.clear();
        assert_eq!(
            editor.validate().as_deref(),
            Some("Option 2 needs at least one choice.")
        );
        // Two choices (156) and none (84) under "Copy from…".
        let options = 28.0 + 8.0 + 156.0 + 8.0 + 84.0 + 8.0 + 24.0;
        assert_eq!(
            editor.height(true),
            4.0 + 13.0 + 48.0 + 12.0 + options + 12.0 + 28.0 + 13.0 + 8.0
        );
    }
}
