//! The instance's environment variables (`ProviderEnvironmentSection` in
//! `components/settings/ProviderInstanceCard.tsx`).
//!
//! The server never returns sensitive values: it sends an empty value with
//! `valueRedacted`, shown as "Stored secret, enter a new value to replace".
//! A value typed here only lives in the draft until the server echoes the
//! redacted entry back, which resets the draft. Values are never logged.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Map, Value, json};

use super::{InstanceRow, paint_card};
use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::settings::{Control, Row, RowAction};
use crate::ui::widgets;

/// `EnvironmentDraftRow`, with the text being typed in each field
/// (`DraftInput` commits on Enter or blur).
pub(crate) struct EnvRow {
    key: u64,
    name: String,
    value: String,
    sensitive: bool,
    value_redacted: Option<bool>,
    name_edit: String,
    value_edit: String,
}

/// The draft rows of one instance and what they were derived from.
#[derive(Default)]
pub(crate) struct EnvDraft {
    rows: Vec<EnvRow>,
    /// The environment the rows were last reset from.
    source: Value,
    /// The environment last sent (`lastPublishedEnvironmentRef`).
    published: Option<Value>,
    next_key: u64,
}

impl EnvDraft {
    /// `makeEnvironmentDraftRow` for each stored variable.
    fn reset(&mut self, environment: &Value) {
        self.rows = environment
            .as_array()
            .into_iter()
            .flatten()
            .map(|variable| {
                self.next_key += 1;
                let value = variable["value"].as_str().unwrap_or_default().to_owned();
                let value_redacted = variable["valueRedacted"].as_bool();
                EnvRow {
                    key: self.next_key,
                    name: variable["name"].as_str().unwrap_or_default().to_owned(),
                    value_edit: if value_redacted == Some(true) {
                        String::new()
                    } else {
                        value.clone()
                    },
                    value,
                    sensitive: variable["sensitive"].as_bool().unwrap_or(false),
                    value_redacted,
                    name_edit: variable["name"].as_str().unwrap_or_default().to_owned(),
                }
            })
            .collect();
        self.source = environment.clone();
    }
}

/// `ENVIRONMENT_VARIABLE_NAME_PATTERN`: `^[a-zA-Z_][a-zA-Z0-9_]*$`.
fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `publishRows`: None while a row is half filled with an invalid name;
/// blank new rows are skipped.
fn published(rows: &[EnvRow]) -> Option<Vec<Value>> {
    let mut out = Vec::new();
    for row in rows {
        let name = row.name.trim();
        if !valid_name(name) {
            if !name.is_empty()
                || !row.value.is_empty()
                || !row.sensitive
                || row.value_redacted.is_some()
            {
                return None;
            }
            continue;
        }
        let mut variable = Map::new();
        variable.insert("name".into(), json!(name));
        variable.insert("value".into(), json!(row.value));
        variable.insert("sensitive".into(), json!(row.sensitive));
        if let Some(redacted) = row.value_redacted {
            variable.insert("valueRedacted".into(), json!(redacted));
        }
        out.push(Value::Object(variable));
    }
    Some(out)
}

const ROW_STEP: f32 = 36.0;

impl App {
    /// The Variables card; returns its height.
    pub(super) fn provider_environment(
        &mut self,
        ui: &mut Ui,
        row: &InstanceRow,
        left: f32,
        top: f32,
        width: f32,
    ) -> f32 {
        let environment = if row.instance["environment"].is_array() {
            row.instance["environment"].clone()
        } else {
            json!([])
        };
        let draft = self.env_drafts.entry(row.id.clone()).or_default();
        if draft.rows.is_empty() && draft.source.is_null() {
            draft.reset(&environment);
        } else if environment != draft.source {
            // A change from elsewhere (or the server's redacted echo of what
            // was sent) replaces the draft; our own echo does not.
            if draft.published.as_ref() != Some(&environment) {
                draft.reset(&environment);
            }
            draft.source = environment.clone();
            draft.published = None;
        }
        let count = draft.rows.len();

        let header = Row {
            id: format!("provider:{}:variables", row.id),
            title: "Variables".into(),
            description: "API keys, base URLs, and other per-instance CLI settings.".into(),
            status: None,
            layers: false,
            reset: false,
            control: Control::OutlineIcon {
                label: "Add variable",
                icon: "plus",
                icon_size: 12.0,
            },
        };
        let header_height = self.rows_height(ui, std::slice::from_ref(&header), width);
        let height = if count == 0 {
            header_height
        } else {
            header_height + count as f32 * ROW_STEP + 28.0
        };
        let card = Rect::from_min_size(Pos2::new(left, top), Vec2::new(width, height));
        paint_card(ui, card);
        let header_rect = Rect::from_min_size(
            Pos2::new(card.left() + 1.0, card.top() + 1.0),
            Vec2::new(width - 2.0, header_height - 1.0),
        );
        let add = matches!(
            self.settings_row(ui, header_rect, &header, false),
            Some(RowAction::Click)
        );

        let draft = self.env_drafts.get_mut(&row.id).expect("inserted above");
        let mut changed = false;
        let mut remove: Option<usize> = None;
        let inner_left = card.left() + 17.0;
        let inner_right = card.right() - 17.0;
        let mut y = card.top() + header_height - 1.0;
        for (index, variable) in draft.rows.iter_mut().enumerate() {
            let name_rect = Rect::from_min_size(Pos2::new(inner_left, y), Vec2::new(176.0, 28.0));
            if let Some(name) = field(
                ui,
                name_rect,
                (&row.id, variable.key, "name"),
                &mut variable.name_edit,
                "VARIABLE_NAME",
                false,
            ) {
                variable.name = name.trim().to_owned();
                variable.name_edit = variable.name.clone();
                changed = true;
            }
            let equals = widgets::text_width(ui, "=", font(12.0, Weight::Regular));
            widgets::text(
                ui,
                Pos2::new(name_rect.right() + 6.0, y + 6.0),
                "=",
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
                16.0,
            );
            let close =
                Rect::from_min_size(Pos2::new(inner_right - 20.0, y + 4.0), Vec2::splat(20.0));
            let lock = Rect::from_min_size(
                Pos2::new(close.left() - 6.0 - 20.0, y + 4.0),
                Vec2::splat(20.0),
            );
            let value_rect = Rect::from_min_max(
                Pos2::new(name_rect.right() + 6.0 + equals + 6.0, y),
                Pos2::new(lock.left() - 6.0, y + 28.0),
            );
            let placeholder = if variable.value_redacted == Some(true) {
                "Stored secret, enter a new value to replace"
            } else {
                "value"
            };
            if let Some(value) = field(
                ui,
                value_rect,
                (&row.id, variable.key, "value"),
                &mut variable.value_edit,
                placeholder,
                variable.sensitive,
            ) {
                // A typed value replaces the stored one (`valueRedacted: false`).
                variable.value = value;
                variable.value_redacted = Some(false);
                changed = true;
            }
            let tooltip = if variable.sensitive {
                "Sensitive, stored separately"
            } else {
                "Plain text"
            };
            let icon = if variable.sensitive {
                "lock"
            } else {
                "lock-open"
            };
            if widgets::icon_button(
                ui,
                lock,
                ("env-lock", &row.id, variable.key),
                icon,
                12.0,
                theme::muted_foreground(),
                tooltip,
            )
            .clicked()
            {
                let sensitive = !variable.sensitive;
                variable.sensitive = sensitive;
                if !(sensitive && variable.value_redacted.is_none()) {
                    variable.value_redacted = Some(if sensitive {
                        variable.value_redacted.unwrap_or(false)
                    } else {
                        false
                    });
                }
                changed = true;
            }
            if remove_button(ui, close, (&row.id, variable.key)) {
                remove = Some(index);
            }
            y += ROW_STEP;
        }
        if count > 0 {
            widgets::text(
                ui,
                Pos2::new(inner_left, y),
                "Sensitive values are stored separately and never returned to the app.",
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
                16.0,
            );
        }
        if let Some(index) = remove {
            draft.rows.remove(index);
            changed = true;
        }
        if add {
            // `addVariable`: a sensitive blank row, published once named.
            draft.next_key += 1;
            draft.rows.push(EnvRow {
                key: draft.next_key,
                name: String::new(),
                value: String::new(),
                sensitive: true,
                value_redacted: None,
                name_edit: String::new(),
                value_edit: String::new(),
            });
        }
        if changed && let Some(variables) = published(&draft.rows) {
            // `updateEnvironment`: named variables, or no key at all.
            let cleaned: Vec<Value> = variables
                .into_iter()
                .filter(|v| !v["name"].as_str().unwrap_or_default().trim().is_empty())
                .collect();
            draft.published = Some(Value::Array(cleaned.clone()));
            let mut instance = row.instance.clone();
            if let Some(object) = instance.as_object_mut() {
                if cleaned.is_empty() {
                    object.remove("environment");
                } else {
                    object.insert("environment".into(), Value::Array(cleaned));
                }
            }
            self.save_instance(row, instance);
        }
        height
    }
}

/// A mono `Input size="sm"` that commits on Enter or blur; masked when
/// `password`. Returns the committed text.
fn field(
    ui: &mut Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    placeholder: &str,
    password: bool,
) -> Option<String> {
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        theme::control_surface(),
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let inner = Rect::from_min_max(
        Pos2::new(rect.left() + 10.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 10.0, rect.bottom() - 1.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    let response = child.add(
        egui::TextEdit::singleline(text)
            .id(egui::Id::new(("provider-env", id)))
            .password(password)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(theme::mono(14.0))
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
            theme::mono(14.0),
            theme::muted_foreground(),
        );
    }
    response.lost_focus().then(|| text.clone())
}

/// `Button size="icon-micro" variant="ghost-destructive"` with an X.
fn remove_button(ui: &Ui, rect: Rect, id: impl std::hash::Hash + std::fmt::Debug) -> bool {
    let response = ui.interact(rect, ui.id().with(("env-remove", id)), Sense::click());
    let color = if response.hovered() {
        let destructive = Color32::from_rgb(251, 65, 74);
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), theme::alpha(destructive, 0.1));
        destructive
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, value: &str, sensitive: bool, redacted: Option<bool>) -> EnvRow {
        EnvRow {
            key: 0,
            name: name.into(),
            value: value.into(),
            sensitive,
            value_redacted: redacted,
            name_edit: String::new(),
            value_edit: String::new(),
        }
    }

    #[test]
    fn publishing_follows_upstream() {
        assert!(
            valid_name("_FOO1") && !valid_name("1FOO") && !valid_name("A-B") && !valid_name("")
        );
        // A blank new row is skipped; a half-filled invalid one holds everything back.
        let rows = [row("FOO", "bar", false, None), row("", "", true, None)];
        assert_eq!(
            published(&rows),
            Some(vec![
                json!({"name": "FOO", "value": "bar", "sensitive": false})
            ])
        );
        assert_eq!(published(&[row("", "x", true, None)]), None);
        assert_eq!(
            published(&[row("KEY", "", true, Some(true))]),
            Some(vec![
                json!({"name": "KEY", "value": "", "sensitive": true, "valueRedacted": true})
            ])
        );
    }
}
