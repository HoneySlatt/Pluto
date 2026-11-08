//! The Providers settings page (`ProviderSettingsPanel.tsx`,
//! `ProviderInstanceCard.tsx`, `ProviderSettingsForm.tsx`,
//! `ProviderModelsSection.tsx`, `providerStatus.ts`, `providerDriverMeta.ts`,
//! `UsageProviderSettings.tsx`).
//!
//! Ported: the provider instance list (status, version, update marker,
//! enable switch), the selected instance's editor (display name with the
//! redacted account, the driver's runtime fields, models with favorites,
//! visibility and ordering) and the health check interval. Edits are saved
//! as `server.updateSettings` patches like upstream's
//! `buildProviderInstanceUpdatePatch` (without resetting the legacy
//! `providers` mirror); model preferences are client settings.
//!
//! Not ported (a notice shows instead): adding or deleting instances, the
//! accent color picker, environment variable editing, custom models, usage
//! limit hubs, update installs and the Codex/Antigravity setup flows.
//! Credential fields (`control: "password"`) are write-only where
//! `driver_secrets` lists them (Z Code's API key): the server sends only a
//! redaction, and a typed value is saved, then dropped. The others (the
//! Antigravity API key, the OpenCode server password) and environment
//! variable values are not editable here. None is ever read or shown.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::accent_picker::{self, AccentPicker, PickerEvent};
use crate::ui::custom_model_editor::{CustomModelEditor, EditorEvent};
use crate::ui::settings::{
    Control, Row, RowAction, Store, card_border, card_fill, number_or_text_field, row_divider,
    row_height, switch,
};
use crate::ui::widgets;

pub(crate) mod add_instance;
mod environment;
pub(crate) use add_instance::AddInstance;
pub(crate) use environment::EnvDraft;

const WIDTH: f32 = 976.0;
/// `providerCardHeightClassName`: the list and editor share a fixed height.
const CARD_HEIGHT: f32 = 704.0;
/// `grid-cols-[17rem_…]`.
const LIST_WIDTH: f32 = 272.0;
const FIELD_WIDTH: f32 = 224.0;
const ACCENT_DEFAULT: Color32 = Color32::from_rgb(37, 99, 235); // blue-600
const HEALTH_STEP_SECONDS: i64 = 30;

/// `DEFAULT_DISABLED_PROVIDER_DRIVERS` (contracts `settings.ts`): built-in
/// drivers that stay off until the user turns them on.
const DEFAULT_DISABLED_DRIVERS: [&str; 11] = [
    "cursor",
    "grok",
    "muse",
    "kimiCode",
    "deepseekHarness",
    "mistralVibe",
    "zcode",
    "ollama",
    "pi",
    "opencode",
    "antigravity",
];

/// A select field of a driver's settings schema (`control: "select"`): key,
/// title, description, options as (value, label), and its decoding default.
type SelectField = (
    &'static str,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static str,
);

fn driver_selects(driver: &str) -> &'static [SelectField] {
    match driver {
        "zcode" => &[(
            "platform",
            "Platform",
            "Where your GLM Coding Plan is: Z.ai, or BigModel in mainland China.",
            &[("zai", "Z.ai"), ("bigmodel", "BigModel")],
            "zai",
        )],
        "ollama" => &[
            (
                "contextLength",
                "Context length",
                "Tokens of conversation each model keeps, capped at the model's own limit. Longer contexts take more GPU memory.",
                &[
                    ("8192", "8K"),
                    ("16384", "16K"),
                    ("32768", "32K"),
                    ("65536", "64K"),
                    ("131072", "128K"),
                ],
                "32768",
            ),
            (
                "webSearch",
                "Web search",
                "Where local models search the web: a SearXNG instance, or Brave's Search API with a key.",
                &[
                    ("searxng", "SearXNG"),
                    ("brave", "Brave Search API"),
                    ("off", "Off"),
                ],
                "searxng",
            ),
            (
                "codeAgent",
                "Code projects",
                "Run local models in code projects through OpenCode, which reads and edits files and runs commands with approvals.",
                &[("off", "Off"), ("opencode", "OpenCode")],
                "off",
            ),
        ],
        _ => &[],
    }
}

/// A password field of a driver's settings schema (`control: "password"`):
/// key, title, description, placeholder. The server stores it apart and
/// sends only a redaction, so the client writes it and never reads it.
fn driver_secrets(driver: &str) -> &'static [Field] {
    match driver {
        "zcode" => &[(
            "apiKey",
            "API key",
            "GLM Coding Plan API key. Stored separately on the host.",
            "Required",
        )],
        "ollama" => &[(
            "braveApiKey",
            "Brave Search API key",
            "Key from api-dashboard.search.brave.com. Stored separately on the host.",
            "For Brave Search",
        )],
        _ => &[],
    }
}

/// `PROVIDER_CLIENT_DEFINITIONS`: driver, label, mark, early-access badge.
const DRIVERS: [(&str, &str, &str, Option<&str>); 11] = [
    ("codex", "Codex", "openai", None),
    ("claudeAgent", "Claude", "claude", None),
    ("cursor", "Cursor", "provider-cursor", Some("Early Access")),
    ("grok", "Grok", "provider-grok", Some("Early Access")),
    ("opencode", "OpenCode", "opencode", None),
    ("antigravity", "Antigravity", "provider-antigravity", None),
    // Pluto's own drivers (packages/provider-kimi-code, provider-mistral-vibe,
    // provider-zcode).
    ("kimiCode", "Kimi Code", "provider-kimi-dark", Some("Beta")),
    (
        "deepseekHarness",
        "DeepSeek Harness",
        "provider-deepseek",
        Some("Preview"),
    ),
    (
        "mistralVibe",
        "Mistral Vibe",
        "provider-mistral-vibe",
        Some("Beta"),
    ),
    ("zcode", "Z Code", "provider-zcode", Some("Beta")),
    // packages/provider-ollama: local models, chats only for now.
    ("ollama", "Ollama", "provider-ollama", Some("Beta")),
];

/// One runtime field of a driver's settings schema: key, title,
/// description, placeholder. Password fields are `driver_secrets`.
type Field = (&'static str, &'static str, &'static str, &'static str);

fn driver_fields(driver: &str) -> &'static [Field] {
    match driver {
        "codex" => &[
            (
                "binaryPath",
                "Binary path",
                "Path to the Codex binary used by this instance.",
                "codex",
            ),
            (
                "homePath",
                "CODEX_HOME path",
                "Custom Codex home and config directory.",
                "~/.codex",
            ),
            (
                "shadowHomePath",
                "Shadow home path",
                "Account-specific Codex home. Keeps auth.json separate while sharing state from CODEX_HOME.",
                "~/.codex-t3/personal",
            ),
            (
                "launchArgs",
                "Launch arguments",
                "Additional CLI arguments passed to codex app-server on session start.",
                "",
            ),
        ],
        "claudeAgent" => &[
            (
                "binaryPath",
                "Binary path",
                "Path to the Claude binary used by this instance.",
                "claude",
            ),
            (
                "homePath",
                "CLAUDE_CONFIG_DIR path",
                "Custom Claude home and config directory. Keeps .claude.json and .claude separate.",
                "~/.claude",
            ),
            (
                "autoCompactWindow",
                "Auto-compact after",
                "Compact after 100,000 to 1,000,000 tokens. Leave empty to use Claude's default.",
                "e.g. 300000",
            ),
            (
                "launchArgs",
                "Launch arguments",
                "Additional CLI arguments passed on session start.",
                "e.g. --chrome",
            ),
        ],
        "cursor" => &[
            (
                "binaryPath",
                "Binary path",
                "Path to the Cursor agent binary.",
                "cursor-agent",
            ),
            (
                "apiEndpoint",
                "API endpoint",
                "Override the Cursor API endpoint for this instance.",
                "https://...",
            ),
        ],
        "grok" => &[(
            "binaryPath",
            "Binary path",
            "Path to the Grok CLI binary.",
            "grok",
        )],
        "opencode" => &[
            (
                "binaryPath",
                "Binary path",
                "Path to the OpenCode binary.",
                "opencode",
            ),
            (
                "serverUrl",
                "Server URL",
                "Leave blank to let T3 Code spawn the server when needed.",
                "http://127.0.0.1:4096",
            ),
        ],
        "antigravity" => &[
            (
                "gcpProject",
                "GCP project",
                "Required for Gemini Enterprise. Agent Platform uses it when no API key is set.",
                "my-project-id",
            ),
            (
                "gcpLocation",
                "GCP location",
                "Region for Gemini Enterprise or Agent Platform.",
                "us-central1",
            ),
            (
                "binaryPath",
                "Binary path",
                "Custom ACP executable. Leave empty to select automatically.",
                "Automatic",
            ),
        ],
        "kimiCode" => &[(
            "binaryPath",
            "Binary path",
            "Path to the Kimi Code binary on this environment. Sign in by running kimi and /login on that host.",
            "kimi",
        )],
        "deepseekHarness" => &[(
            "binaryPath",
            "Binary path",
            "Path to the DeepSeek Harness binary on this environment. Sign in once in that instance's Harness home before the first turn.",
            "dsh",
        )],
        "mistralVibe" => &[(
            "binaryPath",
            "Binary path",
            "Path to Mistral Vibe's vibe-app-server on this environment. It ships with the vibe CLI.",
            "vibe-app-server",
        )],
        "zcode" => &[(
            "binaryPath",
            "Binary path",
            "Path to the Z Code desktop app on this environment. Pluto runs the agent it bundles.",
            "zcode",
        )],
        "ollama" => &[
            (
                "baseUrl",
                "Server URL",
                "Address of the Ollama server, as seen from this environment.",
                "http://127.0.0.1:11434",
            ),
            (
                "searxngUrl",
                "SearXNG URL",
                "Address of the SearXNG instance, with its JSON format turned on.",
                "http://127.0.0.1:8888",
            ),
        ],
        _ => &[],
    }
}

fn driver_meta(
    driver: &str,
) -> Option<&'static (
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
)> {
    DRIVERS.iter().find(|d| d.0 == driver)
}

/// A provider instance as the page shows it (`InstanceRow`).
#[derive(Clone)]
struct InstanceRow {
    id: String,
    driver: String,
    /// `ProviderInstanceConfig`: driver, enabled, displayName, accentColor, config.
    instance: Value,
}

impl InstanceRow {
    /// `resolveProviderInstanceEnabled`: an explicit false on the envelope
    /// or the config wins, then the envelope, the config, and the driver's
    /// default.
    fn enabled(&self) -> bool {
        let envelope = self.instance["enabled"].as_bool();
        let config = self.instance["config"]["enabled"].as_bool();
        if envelope == Some(false) || config == Some(false) {
            return false;
        }
        envelope
            .or(config)
            .unwrap_or(!DEFAULT_DISABLED_DRIVERS.contains(&self.driver.as_str()))
    }

    fn display_name(&self) -> String {
        self.instance["displayName"]
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                driver_meta(&self.driver)
                    .map(|d| d.1.to_owned())
                    .unwrap_or_else(|| self.driver.clone())
            })
    }
}

/// `getProviderVersionLabel`.
fn version_label(version: Option<&str>) -> Option<String> {
    let version = version.filter(|v| !v.is_empty())?;
    if let Some(rest) = version.strip_prefix("agy_acp_server_") {
        let date = rest.get(..8)?;
        let candidate = rest.split('_').nth(2);
        return Some(format!(
            "{}-{}-{}{}",
            &date[..4],
            &date[4..6],
            &date[6..8],
            candidate.map(|c| format!(" {c}")).unwrap_or_default()
        ));
    }
    Some(if version.starts_with(|c: char| c.is_ascii_digit()) {
        format!("v{version}")
    } else {
        version.to_owned()
    })
}

/// `getProviderSummary`: the status headline (and its detail).
fn summary(provider: Option<&Value>, enabled: bool) -> (String, Option<String>) {
    let message = |p: &Value| p["message"].as_str().map(str::to_owned);
    if !enabled {
        return ("Disabled".into(), None);
    }
    let Some(p) = provider else {
        return (
            "Checking provider status".into(),
            Some(
                "Waiting for the server to report installation and authentication details.".into(),
            ),
        );
    };
    if p["enabled"] == false || p["status"] == "disabled" {
        return ("Disabled".into(), message(p));
    }
    if p["installed"] == false {
        return (
            "Not found".into(),
            message(p).or(Some("CLI not detected on PATH.".into())),
        );
    }
    if p["auth"]["status"] == "unauthenticated" {
        return ("Not authenticated".into(), message(p));
    }
    if p["status"] == "warning" {
        return ("Needs attention".into(), message(p));
    }
    if p["status"] == "error" {
        return ("Unavailable".into(), message(p));
    }
    if p["auth"]["status"] == "authenticated" {
        let label = p["auth"]["label"].as_str().or(p["auth"]["type"].as_str());
        return (
            label.map_or("Authenticated".into(), |l| format!("Authenticated · {l}")),
            message(p),
        );
    }
    ("Available".into(), message(p))
}

/// Whether the version advisory offers an update (`Update available`).
fn update_available(provider: Option<&Value>) -> bool {
    let Some(p) = provider else {
        return false;
    };
    let latest_broken = matches!(
        p["compatibilityAdvisory"]["latestVersionStatus"].as_str(),
        Some("broken" | "unsupported")
    );
    let status = p["versionAdvisory"]["status"].as_str().unwrap_or("unknown");
    !latest_broken && !matches!(status, "current" | "unknown")
}

/// `describeModelCapabilities`.
fn capabilities(model: &Value) -> String {
    let descriptors: Vec<&Value> = model["capabilities"]["optionDescriptors"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    let mut labels = Vec::new();
    let fast = descriptors.iter().any(|d| {
        d["id"] == "fastMode"
            || (d["id"] == "serviceTier"
                && d["type"] == "select"
                && d["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|o| o["id"] == "fast" || o["label"] == "Fast"))
    });
    if fast {
        labels.push("Fast mode");
    }
    if descriptors.iter().any(|d| d["id"] == "thinking") {
        labels.push("Thinking");
    }
    if descriptors.iter().any(|d| {
        d["type"] == "select"
            && matches!(
                d["id"].as_str(),
                Some("reasoningEffort" | "effort" | "reasoning" | "variant")
            )
    }) {
        labels.push("Reasoning");
    }
    labels.join(" · ")
}

/// A model row's display group: favorites, then visible, then hidden.
#[derive(Clone, Copy, PartialEq)]
enum Group {
    Favorite,
    Visible,
    Hidden,
}

/// `groupModelsForDisplay` over `sortModelsForProviderInstance`.
fn ordered_models(
    models: &[Value],
    favorites: &[String],
    hidden: &[String],
    order: &[String],
) -> Vec<(Value, Group)> {
    let slug = |m: &Value| m["slug"].as_str().unwrap_or_default().to_owned();
    let custom = |m: &Value| m["isCustom"] == true;
    let mut indexed: Vec<(usize, &Value)> = models.iter().enumerate().collect();
    indexed.sort_by_key(|(index, m)| {
        let s = slug(m);
        (
            !favorites.contains(&s),
            order.iter().position(|o| *o == s).unwrap_or(usize::MAX),
            *index,
        )
    });
    let group = |m: &Value| {
        let s = slug(m);
        if favorites.contains(&s) {
            Group::Favorite
        } else if !custom(m) && hidden.contains(&s) {
            Group::Hidden
        } else {
            Group::Visible
        }
    };
    let mut out = Vec::new();
    for wanted in [Group::Favorite, Group::Visible, Group::Hidden] {
        for (_, m) in &indexed {
            if group(m) == wanted {
                out.push(((*m).clone(), wanted));
            }
        }
    }
    out
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// Section title (`SettingsSection`): 14px at 70%, 28px tall.
fn section_title(ui: &Ui, pos: Pos2, text: &str) {
    widgets::text_in(
        ui,
        Rect::from_min_size(pos, Vec2::new(600.0, 28.0)),
        text,
        font(14.0, Weight::Regular),
        theme::alpha(theme::foreground(), 0.7),
    );
}

fn paint_card(ui: &Ui, rect: Rect) {
    ui.painter().rect(
        rect,
        CornerRadius::same(14),
        card_fill(),
        Stroke::new(1.0, card_border()),
        StrokeKind::Inside,
    );
}

/// `readCustomModelEntries`: slugs (strings or `{slug, name}`), first
/// occurrence wins; returns (slug, name).
fn custom_model_entries(value: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for raw in value.as_array().into_iter().flatten() {
        let slug = raw
            .as_str()
            .or(raw["slug"].as_str())
            .map(str::trim)
            .unwrap_or_default();
        if slug.is_empty() || out.iter().any(|(s, _)| s == slug) {
            continue;
        }
        let name = raw["name"]
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(slug);
        out.push((slug.to_owned(), name.to_owned()));
    }
    out
}

/// `Button size="sm"`: outline (Add) or ghost (Cancel); returns whether clicked.
fn ghost_outline_button(ui: &Ui, rect: Rect, id: &str, label: &str, outline: bool) -> bool {
    let response = ui.interact(rect, ui.id().with(("small-button", id)), Sense::click());
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

fn ghost_button(ui: &Ui, rect: Rect, id: &str, icon: Option<&str>, label: &str) -> bool {
    let response = ui.interact(rect, ui.id().with(("ghost", id)), Sense::click());
    let color = if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::accent());
        theme::foreground()
    } else {
        theme::muted_foreground()
    };
    // `px-[7px]` inside a 1px border.
    let mut x = rect.left() + 8.0;
    if let Some(icon) = icon {
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(
                Pos2::new(rect.left() + 6.0, rect.center().y - 6.0),
                Vec2::splat(12.0),
            ),
            12.0,
            color,
        );
        x = rect.left() + 20.0;
    }
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(x, rect.top()), rect.max),
        label,
        font(12.0, Weight::Medium),
        color,
    );
    response.clicked()
}

impl App {
    /// The provider instances (`EnvironmentProviderSettings`): each known
    /// driver's default slot (an explicit `providerInstances` entry, one
    /// synthesized from the legacy `providers` blob, or, for Codex and
    /// Claude, which run unconfigured, the driver's default config), its
    /// custom instances, then instances of drivers this build does not know.
    /// Cursor is listed only when the server reports its default instance.
    fn provider_rows(&self) -> Vec<InstanceRow> {
        let settings = &self.state.config["settings"];
        let instances = settings["providerInstances"].as_object();
        let mut rows = Vec::new();
        for (driver, ..) in DRIVERS {
            if driver == "cursor" && self.live_provider(driver).is_none() {
                continue;
            }
            let explicit = instances.and_then(|i| i.get(driver)).cloned();
            let synthesized = settings["providers"][driver].as_object().map(|legacy| {
                let mut config = legacy.clone();
                let enabled = config.remove("enabled").unwrap_or(json!(true));
                json!({"driver": driver, "enabled": enabled, "config": config})
            });
            let unconfigured =
                matches!(driver, "codex" | "claudeAgent").then(|| json!({"driver": driver}));
            if let Some(instance) = explicit.or(synthesized).or(unconfigured) {
                rows.push(InstanceRow {
                    id: driver.into(),
                    driver: driver.into(),
                    instance,
                });
            }
            for (id, instance) in instances.into_iter().flatten() {
                if id != driver && instance["driver"] == driver {
                    rows.push(InstanceRow {
                        id: id.clone(),
                        driver: driver.into(),
                        instance: instance.clone(),
                    });
                }
            }
        }
        for (id, instance) in instances.into_iter().flatten() {
            let driver = instance["driver"].as_str().unwrap_or_default();
            if driver_meta(driver).is_none() {
                rows.push(InstanceRow {
                    id: id.clone(),
                    driver: driver.into(),
                    instance: instance.clone(),
                });
            }
        }
        rows
    }

    fn live_provider(&self, id: &str) -> Option<Value> {
        self.providers()
            .iter()
            .find(|p| p["instanceId"] == id)
            .cloned()
    }

    /// Saves one instance (`usePersistEnvironmentProviderInstanceMutation`):
    /// an `upsert` the server applies to its latest settings, so edits made
    /// elsewhere meanwhile stay.
    fn save_instance(&mut self, row: &InstanceRow, instance: Value) {
        self.mutate_provider_instance(json!({
            "operation": "upsert",
            "instanceId": row.id,
            "instance": instance,
        }));
    }

    /// `deleteProviderInstance`: the instance's `remove`.
    fn delete_instance(&mut self, id: &str) {
        self.mutate_provider_instance(json!({"operation": "remove", "instanceId": id}));
    }

    fn provider_instances_map(&self) -> serde_json::Map<String, Value> {
        self.state.config["settings"]["providerInstances"]
            .as_object()
            .cloned()
            .unwrap_or_default()
    }

    /// One instance's `create`, `upsert` or `remove`, applied here at once
    /// and sent as `server.updateSettings`' `providerInstanceMutation`.
    pub(crate) fn mutate_provider_instance(&mut self, mutation: Value) {
        let mut instances = self.provider_instances_map();
        let id = mutation["instanceId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if mutation["operation"] == "remove" {
            instances.remove(&id);
        } else {
            instances.insert(id.clone(), mutation["instance"].clone());
        }
        // Replace locally first: the settings merge would keep removed
        // instances or keys until the server's echo.
        if !self.state.config["settings"].is_object() {
            self.state.config["settings"] = json!({});
        }
        self.state.config["settings"]["providerInstances"] = Value::Object(instances);
        self.send_request(
            format!("settings:providerInstances:{id}"),
            "server.updateSettings",
            json!({"patch": {}, "providerInstanceMutation": mutation}),
        );
    }

    /// `updateAccentColor`: set when valid, otherwise dropped from the instance.
    fn save_accent_color(&mut self, row: &InstanceRow, value: &str) {
        let mut instance = row.instance.clone();
        if let Some(object) = instance.as_object_mut() {
            match accent_picker::normalize(value) {
                Some(color) => {
                    object.insert("accentColor".into(), json!(color));
                }
                None => {
                    object.remove("accentColor");
                }
            }
        }
        self.save_instance(row, instance);
    }

    /// `updateCustomModels`: the instance config's `customModels`, kept
    /// explicit even when empty.
    fn save_custom_models(&mut self, row: &InstanceRow, models: Vec<Value>) {
        let mut instance = row.instance.clone();
        if !instance["config"].is_object() {
            instance["config"] = json!({});
        }
        instance["config"]["customModels"] = Value::Array(models);
        self.save_instance(row, instance);
    }

    /// The Providers page from `top` (the scope sentence's top); returns the
    /// bottom of its last card.
    pub(crate) fn providers_page(&mut self, ui: &mut Ui, left: f32, top: f32) -> f32 {
        let mut y = self.providers_scope_sentence(ui, left, top);
        let rows = self.provider_rows();
        let selected = self
            .providers_selected
            .clone()
            .filter(|id| rows.iter().any(|r| r.id == *id))
            .or_else(|| rows.first().map(|r| r.id.clone()));

        // Header: "Providers", the last check (refresh) and add.
        section_title(ui, Pos2::new(left + 16.0, y), "Providers");
        let right = left + WIDTH - 16.0;
        let add = Rect::from_min_size(Pos2::new(right - 24.0, y + 2.0), Vec2::splat(24.0));
        if widgets::icon_button(
            ui,
            add,
            "add-provider",
            "plus",
            14.0,
            theme::muted_foreground(),
            "Add provider",
        )
        .clicked()
        {
            self.open_add_instance();
        }
        let checked = self
            .providers()
            .iter()
            .filter_map(|p| p["checkedAt"].as_str().map(str::to_owned))
            .max();
        let label = if self.providers_refreshing {
            "Refreshing providers".to_owned()
        } else {
            checked
                .as_deref()
                .map(|at| format!("Checked {} ago", widgets::relative_time(at, self.now)))
                .unwrap_or_else(|| "Not checked yet".into())
        };
        let label_width = widgets::text_width(ui, &label, font(12.0, Weight::Medium));
        let refresh = Rect::from_min_size(
            Pos2::new(add.left() - 8.0 - (label_width + 30.0).round(), y + 2.0),
            Vec2::new((label_width + 30.0).round(), 24.0),
        );
        if ghost_button(ui, refresh, "refresh-providers", None, "") && !self.providers_refreshing {
            self.providers_refreshing = true;
            self.send_request(
                "providers-refresh".into(),
                "server.refreshProviders",
                json!({}),
            );
        }
        theme::paint_icon(
            ui,
            "refresh-cw",
            Rect::from_min_size(
                Pos2::new(refresh.left() + 6.0, refresh.center().y - 7.0),
                Vec2::splat(14.0),
            ),
            14.0,
            theme::muted_foreground(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(refresh.left() + 22.0, refresh.top()), refresh.max),
            &label,
            font(12.0, Weight::Medium),
            theme::muted_foreground(),
        );

        // The two-pane card.
        let card = Rect::from_min_size(Pos2::new(left, y + 38.0), Vec2::new(WIDTH, CARD_HEIGHT));
        paint_card(ui, card);
        let list = Rect::from_min_size(
            card.min + Vec2::splat(1.0),
            Vec2::new(LIST_WIDTH, CARD_HEIGHT - 2.0),
        );
        ui.painter().rect_filled(
            list,
            CornerRadius {
                nw: 13,
                sw: 13,
                ne: 0,
                se: 0,
            },
            theme::alpha(theme::muted(), 0.1),
        );
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(list.right() - 1.0, list.top()),
                Vec2::new(1.0, list.height()),
            ),
            0.0,
            card_border(),
        );
        let mut row_top = list.top();
        for row in &rows {
            let height = self.provider_list_row(
                ui,
                row,
                row_top,
                list.left(),
                selected.as_deref() == Some(row.id.as_str()),
            );
            row_top += height;
        }
        let editor = Rect::from_min_max(
            Pos2::new(list.right(), card.top() + 1.0),
            card.max - Vec2::splat(1.0),
        );
        if let Some(row) = rows
            .iter()
            .find(|r| Some(r.id.as_str()) == selected.as_deref())
            .cloned()
        {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(editor));
            child.set_clip_rect(editor.intersect(ui.clip_rect()));
            egui::ScrollArea::vertical()
                .id_salt(("provider-editor", &row.id))
                .auto_shrink([false, false])
                .show(&mut child, |ui| {
                    let inner_left = editor.left() + 16.0;
                    let content_top = ui.max_rect().top() + 16.0;
                    let bottom = self.provider_editor(ui, &row, inner_left, content_top);
                    ui.allocate_rect(
                        Rect::from_min_max(
                            Pos2::new(inner_left, ui.max_rect().top()),
                            Pos2::new(editor.right() - 16.0, bottom + 16.0),
                        ),
                        Sense::hover(),
                    );
                });
        } else {
            widgets::text(
                ui,
                editor.min + Vec2::splat(24.0),
                "No providers configured.",
                font(14.0, Weight::Regular),
                theme::muted_foreground(),
                20.0,
            );
        }
        y = card.bottom() + 32.0;

        // Usage providers (limit hubs).
        section_title(ui, Pos2::new(left + 16.0, y), "Usage providers");
        let hub_label_width = widgets::outline_button_width(ui, true, "Add hub") - 2.0;
        let hub = Rect::from_min_size(
            Pos2::new(right - hub_label_width, y + 2.0),
            Vec2::new(hub_label_width, 24.0),
        );
        let response = ui.interact(hub, ui.id().with("add-hub"), Sense::click());
        ui.painter().rect(
            hub,
            CornerRadius::same(8),
            if response.hovered() {
                theme::overlay(0.05)
            } else {
                theme::control_surface()
            },
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
        theme::paint_icon(
            ui,
            "plus",
            Rect::from_min_size(
                Pos2::new(hub.left() + 6.0, hub.center().y - 6.0),
                Vec2::splat(12.0),
            ),
            12.0,
            theme::muted_foreground(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(hub.left() + 22.0, hub.top()), hub.max),
            "Add hub",
            font(12.0, Weight::Medium),
            theme::foreground(),
        );
        if response.clicked() {
            self.notify(
                crate::ui::toasts::Kind::Info,
                "Usage limit hubs are not implemented yet",
            );
        }
        let hubs = Rect::from_min_size(Pos2::new(left, y + 38.0), Vec2::new(WIDTH, 46.0));
        paint_card(ui, hubs);
        let sources = self.state.config["settings"]["usageLimitSources"]
            .as_object()
            .map_or(0, |s| s.len());
        let hub_text = if sources == 0 {
            "No hubs configured.".to_owned()
        } else {
            format!("{sources} hubs configured.")
        };
        widgets::text(
            ui,
            Pos2::new(hubs.left() + 17.0, hubs.top() + 13.0),
            &hub_text,
            font(14.0, Weight::Medium),
            theme::foreground(),
            20.0,
        );
        y = hubs.bottom() + 32.0;

        // Advanced: the health check interval.
        section_title(ui, Pos2::new(left + 16.0, y), "Advanced");
        let advanced = Rect::from_min_size(Pos2::new(left, y + 38.0), Vec2::new(WIDTH, 68.0));
        paint_card(ui, advanced);
        let title_font = font(14.0, Weight::Medium);
        let title = "Health check interval";
        widgets::text(
            ui,
            Pos2::new(advanced.left() + 17.0, advanced.top() + 13.0),
            title,
            title_font.clone(),
            theme::foreground(),
            20.0,
        );
        let info = Rect::from_min_size(
            Pos2::new(
                advanced.left() + 17.0 + widgets::text_width(ui, title, title_font) + 6.0,
                advanced.top() + 13.0,
            ),
            Vec2::splat(20.0),
        );
        theme::paint_icon(ui, "info", info, 14.0, theme::muted_foreground());
        ui.interact(info, ui.id().with("health-info"), Sense::hover()).on_hover_text(
            "This interval is configured here, then the shared Background activity policy decides whether provider probes may run when the timer fires. Custom intervals appear as Advanced in General settings.",
        );
        widgets::text(
            ui,
            Pos2::new(advanced.left() + 17.0, advanced.top() + 37.0),
            "Refresh provider status, versions, and models in the background. Set to 0 to disable.",
            font(12.0, Weight::Regular),
            theme::alpha(theme::muted_foreground(), 0.8),
            18.0,
        );
        let seconds = self.state.config["settings"]["providerHealthRefreshInterval"]
            .as_i64()
            .map_or(300, |ms| ms / 1000);
        let seconds_width = widgets::text_width(ui, "seconds", font(12.0, Weight::Regular));
        let field = Rect::from_min_size(
            Pos2::new(
                advanced.right() - 17.0 - seconds_width - 8.0 - 128.0,
                advanced.center().y - 14.0,
            ),
            Vec2::new(128.0, 28.0),
        );
        if let Some(next) = self.interval_field(
            ui,
            field,
            seconds,
            "provider-health-interval",
            HEALTH_STEP_SECONDS,
        ) {
            self.patch_server_settings(
                json!({"providerHealthRefreshInterval": next.max(0) * 1000}),
            );
        }
        widgets::text(
            ui,
            Pos2::new(field.right() + 8.0, advanced.center().y - 8.0),
            "seconds",
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
            16.0,
        );
        advanced.bottom()
    }

    /// `SettingsScopeSentence` with one environment: "on" and its machine.
    fn providers_scope_sentence(&self, ui: &Ui, left: f32, top: f32) -> f32 {
        let y = top + 23.0;
        let mut x = left + 16.0;
        let label = self.state.config["environment"]["label"]
            .as_str()
            .unwrap_or("This machine")
            .to_owned();
        let parts: [(&str, Weight, Color32, Option<&str>, bool); 4] = [
            (
                "Applying settings for",
                Weight::Regular,
                theme::muted_foreground(),
                None,
                false,
            ),
            (
                "All projects",
                Weight::Medium,
                theme::foreground(),
                None,
                true,
            ),
            (
                "on",
                Weight::Regular,
                theme::muted_foreground(),
                None,
                false,
            ),
            (
                &label,
                Weight::Medium,
                theme::foreground(),
                Some("monitor"),
                true,
            ),
        ];
        for (text, weight, color, icon, chevron) in parts {
            if let Some(icon) = icon {
                theme::paint_icon(
                    ui,
                    icon,
                    Rect::from_min_size(Pos2::new(x, y + 5.0), Vec2::splat(14.0)),
                    14.0,
                    theme::foreground(),
                );
                x += 20.0;
            }
            let width = widgets::text_width(ui, text, font(16.0, weight));
            widgets::text(ui, Pos2::new(x, y), text, font(16.0, weight), color, 24.0);
            x += width;
            if chevron {
                theme::paint_icon(
                    ui,
                    "chevron-down",
                    Rect::from_min_size(Pos2::new(x + 6.0, y + 5.0), Vec2::splat(14.0)),
                    14.0,
                    theme::muted_foreground(),
                );
                x += 26.0;
            } else {
                x += 6.0;
            }
        }
        y + 25.0 + 32.0
    }

    /// A provider in the list (`mode === "list"`); returns its height.
    fn provider_list_row(
        &mut self,
        ui: &mut Ui,
        row: &InstanceRow,
        top: f32,
        left: f32,
        selected: bool,
    ) -> f32 {
        let live = self.live_provider(&row.id);
        let enabled = row.enabled();
        let (headline, _) = summary(live.as_ref(), enabled);
        let text_left = left + 16.0 + 20.0 + 12.0;
        let switch_left = left + LIST_WIDTH - 1.0 - 16.0 - 30.0;
        let summary_width = switch_left - 12.0 - text_left;
        let galley = widgets::galley(
            ui,
            &headline,
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
            18.0,
            summary_width,
        );
        let lines = (galley.size().y / 18.0).round().clamp(1.0, 2.0);
        let natural = 12.0 + 20.0 + 2.0 + lines * 18.0 + 12.0 + 1.0;
        let height = natural.max(72.0);
        let rect = Rect::from_min_size(Pos2::new(left, top), Vec2::new(LIST_WIDTH - 1.0, height));
        let response = ui.interact(
            rect,
            ui.id().with(("provider-row", &row.id)),
            Sense::click(),
        );
        if selected {
            ui.painter()
                .rect_filled(rect, 0.0, theme::alpha(theme::muted(), 0.45));
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, 0.0, theme::alpha(theme::muted(), 0.25));
        }
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left(), rect.bottom() - 1.0),
                Vec2::new(rect.width(), 1.0),
            ),
            0.0,
            row_divider(),
        );
        // Disabled rows fade to 60% until hovered or selected.
        let fade = if !enabled && !selected && !response.hovered() {
            0.6
        } else {
            1.0
        };
        // `min-h-18 items-center`: short rows center their content.
        let line = top + 12.0 + ((height - natural) / 2.0).ceil();
        self.paint_provider_mark(ui, &row.driver, Pos2::new(left + 16.0, line), fade);
        let name = row.display_name();
        let name_font = font(14.0, Weight::Medium);
        widgets::text(
            ui,
            Pos2::new(text_left, line),
            &name,
            name_font.clone(),
            theme::alpha(theme::foreground(), fade),
            20.0,
        );
        let mut x = text_left + widgets::text_width(ui, &name, name_font) + 8.0;
        if let Some(version) = version_label(live.as_ref().and_then(|p| p["version"].as_str())) {
            widgets::text(
                ui,
                Pos2::new(x, line + 2.0),
                &version,
                theme::mono(12.0),
                theme::alpha(theme::muted_foreground(), fade),
                16.0,
            );
            x += widgets::text_width(ui, &version, theme::mono(12.0)) + 8.0;
        }
        if enabled && update_available(live.as_ref()) {
            theme::paint_icon(
                ui,
                "circle-arrow-up",
                Rect::from_min_size(Pos2::new(x, line + 3.0), Vec2::splat(14.0)),
                14.0,
                theme::alpha(theme::muted_foreground(), fade),
            );
        }
        ui.painter().galley(
            Pos2::new(text_left, line + 22.0),
            widgets::galley(
                ui,
                &headline,
                font(12.0, Weight::Regular),
                theme::alpha(theme::muted_foreground(), 0.8 * fade),
                18.0,
                summary_width,
            ),
            theme::muted_foreground(),
        );
        if switch(
            ui,
            switch_left + 30.0,
            rect.center().y,
            enabled,
            &format!("provider-enable-{}", row.id),
        ) {
            let mut instance = row.instance.clone();
            instance["enabled"] = json!(!enabled);
            self.save_instance(row, instance);
        } else if response.clicked() {
            self.providers_selected = Some(row.id.clone());
        }
        height
    }

    fn paint_provider_mark(&self, ui: &Ui, driver: &str, min: Pos2, fade: f32) {
        let Some((icon, tint)) = crate::ui::pickers::provider_icon(driver) else {
            return;
        };
        let rect = Rect::from_min_size(min + Vec2::splat(2.0), Vec2::splat(16.0));
        let tint = if driver == "codex" {
            theme::alpha(theme::foreground(), 0.8)
        } else {
            tint
        };
        theme::paint_icon(ui, icon, rect, 16.0, theme::alpha(tint, fade));
    }

    /// The selected instance's editor (`mode === "editor"`); returns the
    /// bottom of its content.
    fn provider_editor(&mut self, ui: &mut Ui, row: &InstanceRow, left: f32, top: f32) -> f32 {
        let live = self.live_provider(&row.id);
        let width = ui.max_rect().right() - 16.0 - left;
        let right = left + width;
        let name = row.display_name();
        let enabled = row.enabled();

        // Title: mark, name, version and the update marker.
        self.paint_provider_mark(ui, &row.driver, Pos2::new(left + 16.0, top + 4.0), 1.0);
        section_title(ui, Pos2::new(left + 44.0, top), &name);
        let mut x = right - 16.0;
        // Custom instances can be deleted (`Trash2Icon`, ghost-destructive).
        if row.id != row.driver {
            let button =
                Rect::from_min_size(Pos2::new(x + 6.0 - 24.0, top + 2.0), Vec2::splat(24.0));
            let response = ui.interact(
                button,
                ui.id().with(("delete-instance", &row.id)),
                Sense::click(),
            );
            let color = if response.hovered() {
                let destructive = Color32::from_rgb(251, 65, 74);
                ui.painter().rect_filled(
                    button,
                    CornerRadius::same(6),
                    theme::alpha(destructive, 0.1),
                );
                destructive
            } else {
                theme::muted_foreground()
            };
            theme::paint_icon(ui, "trash-2", button, 14.0, color);
            if response.clicked() {
                self.delete_instance(&row.id);
            }
            x = button.left() - 2.0;
        }
        if enabled && update_available(live.as_ref()) {
            let button = Rect::from_min_size(Pos2::new(x - 24.0, top + 2.0), Vec2::splat(24.0));
            if widgets::icon_button(
                ui,
                button,
                ("provider-update", &row.id),
                "circle-arrow-up",
                14.0,
                theme::muted_foreground(),
                "Update available",
            )
            .clicked()
            {
                self.notify(
                    crate::ui::toasts::Kind::Info,
                    "Provider updates are not implemented yet",
                );
            }
            x = button.left();
        }
        x -= 6.0;
        if let Some(version) = version_label(live.as_ref().and_then(|p| p["version"].as_str())) {
            let w = widgets::text_width(ui, &version, theme::mono(12.0));
            widgets::text(
                ui,
                Pos2::new(x - w, top + 6.0),
                &version,
                theme::mono(12.0),
                theme::muted_foreground(),
                16.0,
            );
            x -= w + 6.0;
        }
        if let Some(badge) = driver_meta(&row.driver).and_then(|d| d.3) {
            let badge_font = font(10.0, Weight::Medium);
            let w = widgets::text_width(ui, badge, badge_font.clone()) + 8.0;
            let rect = Rect::from_min_size(Pos2::new(x - w, top + 6.0), Vec2::new(w, 16.0));
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(4),
                theme::alpha(theme::warning(), 0.16),
            );
            widgets::text(
                ui,
                Pos2::new(rect.left() + 4.0, rect.top() + 3.0),
                badge,
                badge_font,
                theme::warning_foreground(),
                10.0,
            );
        }

        // Display name: the status line, the accent color and the name input.
        let card = Rect::from_min_size(Pos2::new(left, top + 38.0), Vec2::new(width, 85.0));
        paint_card(ui, card);
        let inner = card.left() + 17.0;
        widgets::text(
            ui,
            Pos2::new(inner, card.top() + 13.0),
            "Display name",
            font(14.0, Weight::Medium),
            theme::foreground(),
            20.0,
        );
        let input = Rect::from_min_size(
            Pos2::new(
                card.right() - 17.0 - FIELD_WIDTH,
                (card.center().y - 14.0).floor(),
            ),
            Vec2::new(FIELD_WIDTH, 28.0),
        );
        let pipette = Rect::from_min_size(
            Pos2::new(input.left() - 8.0 - 28.0, input.top()),
            Vec2::splat(28.0),
        );
        let accent = self
            .accent_picker
            .as_ref()
            .filter(|p| p.row == row.id)
            .map(|p| p.value.clone())
            .or_else(|| row.instance["accentColor"].as_str().map(str::to_owned))
            .and_then(|hex| accent_picker::parse(&hex))
            .unwrap_or(ACCENT_DEFAULT);
        let pick = ui.interact(pipette, ui.id().with(("accent", &row.id)), Sense::click());
        let scale = if pick.is_pointer_button_down_on() {
            0.95
        } else if pick.hovered() {
            1.05
        } else {
            1.0
        };
        let pipette = Rect::from_center_size(pipette.center(), pipette.size() * scale);
        ui.painter().rect(
            pipette,
            CornerRadius::same(8),
            accent,
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
        theme::paint_icon(
            ui,
            "pipette",
            pipette,
            12.0,
            theme::alpha(Color32::WHITE, 0.7),
        );
        let picker_open = self.accent_picker.as_ref().is_some_and(|p| p.row == row.id);
        if pick.clicked() && !picker_open {
            self.accent_picker = Some(AccentPicker::new(
                row.id.clone(),
                row.instance["accentColor"].as_str(),
            ));
        }
        if let Some(picker) = self.accent_picker.as_mut().filter(|p| p.row == row.id) {
            match picker.show(ui, pipette) {
                PickerEvent::None => {}
                PickerEvent::Commit(value) => self.save_accent_color(row, &value),
                PickerEvent::Close(pending) => {
                    self.accent_picker = None;
                    if let Some(value) = pending {
                        self.save_accent_color(row, &value);
                    }
                }
            }
        }
        let edit_key = format!("provider:{}:displayName", row.id);
        let current = row.instance["displayName"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let placeholder = driver_meta(&row.driver).map_or("Instance label", |d| d.1);
        let text = self
            .settings_edits
            .entry(edit_key.clone())
            .or_insert_with(|| current.clone());
        let (_, done, buffer) = number_or_text_field(ui, input, &edit_key, text, placeholder);
        *self
            .settings_edits
            .get_mut(&edit_key)
            .expect("inserted above") = buffer.clone();
        if done {
            self.settings_edits.remove(&edit_key);
            if buffer.trim() != current {
                let mut instance = row.instance.clone();
                match instance.as_object_mut() {
                    Some(object) if buffer.trim().is_empty() => {
                        object.remove("displayName");
                    }
                    Some(object) => {
                        object.insert("displayName".into(), json!(buffer.trim()));
                    }
                    None => {}
                }
                self.save_instance(row, instance);
            }
        }
        self.provider_status_line(
            ui,
            row,
            live.as_ref(),
            Pos2::new(inner, card.top() + 39.0),
            pipette.left() - 32.0,
        );

        // Runtime: the driver's settings fields.
        let mut y = card.bottom() + 24.0;
        section_title(ui, Pos2::new(left + 16.0, y), "Runtime");
        let config = row.instance["config"].clone();
        let mut field_rows: Vec<Row> = driver_fields(&row.driver)
            .iter()
            .map(|(key, title, description, placeholder)| Row {
                id: format!("provider:{}:{key}", row.id),
                title: (*title).into(),
                description: (*description).into(),
                status: None,
                layers: false,
                reset: false,
                control: Control::Field {
                    value: config[*key].as_str().map(str::to_owned).unwrap_or_else(|| {
                        config[*key]
                            .to_string()
                            .trim_matches('"')
                            .replace("null", "")
                    }),
                    placeholder: (*placeholder).into(),
                    width: FIELD_WIDTH,
                },
            })
            .collect();
        for (key, title, description, choices, default) in driver_selects(&row.driver) {
            field_rows.push(Row {
                id: format!("provider:{}:{key}", row.id),
                title: (*title).into(),
                description: (*description).into(),
                status: None,
                layers: false,
                reset: false,
                control: Control::Select {
                    value: config[*key].as_str().unwrap_or(default).to_owned(),
                    options: choices
                        .iter()
                        .map(|(value, label)| ((*value).to_owned(), (*label).to_owned()))
                        .collect(),
                    width: FIELD_WIDTH,
                    icon: None,
                },
            });
        }
        for (key, title, description, placeholder) in driver_secrets(&row.driver) {
            field_rows.push(Row {
                id: format!("provider:{}:{key}", row.id),
                title: (*title).into(),
                description: (*description).into(),
                status: None,
                layers: false,
                reset: false,
                control: Control::Secret {
                    saved: config[*key].as_str().is_some_and(|value| !value.is_empty()),
                    placeholder: (*placeholder).into(),
                    width: FIELD_WIDTH,
                },
            });
        }
        let actions = self.provider_card_rows(ui, left, y + 38.0, width, &field_rows);
        y += 38.0 + self.rows_height(ui, &field_rows, width) + 24.0;
        for (id, action) in actions {
            if let (Some(key), RowAction::Text(value) | RowAction::Select(value)) =
                (id.rsplit(':').next(), action)
            {
                let mut instance = row.instance.clone();
                if !instance["config"].is_object() {
                    instance["config"] = json!({});
                }
                if value.is_empty() {
                    if let Some(object) = instance["config"].as_object_mut() {
                        object.remove(key);
                    }
                } else {
                    instance["config"][key] = json!(value);
                }
                self.save_instance(row, instance);
            }
        }

        // Environment variables (sensitive values never reach the client).
        section_title(ui, Pos2::new(left + 16.0, y), "Environment");
        y += 38.0 + self.provider_environment(ui, row, left, y + 38.0, width) + 24.0;

        // Models.
        section_title(ui, Pos2::new(left + 16.0, y), "Models");
        self.provider_models(ui, row, live.as_ref(), left, y + 38.0, width)
    }

    /// `editorStatusNode`: "Authenticated as <account> · <plan>", or the
    /// status headline, wrapped like `flex-wrap gap-x-1.5`.
    fn provider_status_line(
        &mut self,
        ui: &mut Ui,
        row: &InstanceRow,
        live: Option<&Value>,
        pos: Pos2,
        max_right: f32,
    ) {
        let small = font(12.0, Weight::Regular);
        let color = theme::muted_foreground();
        let enabled = row.enabled();
        let email = live
            .and_then(|p| p["auth"]["email"].as_str())
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(str::to_owned);
        let authenticated = enabled && live.is_some_and(|p| p["auth"]["status"] == "authenticated");
        let mut x = pos.x;
        let mut y = pos.y;
        let place = |width: f32, x: &mut f32, y: &mut f32| {
            if *x > pos.x && *x + width > max_right {
                *x = pos.x;
                *y += 17.0;
            }
            let at = Pos2::new(*x, *y);
            *x += width + 6.0;
            at
        };
        if let (true, Some(email)) = (authenticated, email) {
            let text = "Authenticated as";
            let w = widgets::text_width(ui, text, small.clone());
            let at = place(w, &mut x, &mut y);
            widgets::text(ui, at, text, small.clone(), color, 16.0);
            let redacted_width = widgets::text_width(
                ui,
                &crate::ui::source_control::redacted_placeholder(&email),
                theme::mono(11.0),
            );
            let at = place(redacted_width, &mut x, &mut y);
            self.redacted_text(
                ui,
                at,
                &email,
                &format!("provider-email-{}", row.id),
                ("Click to reveal email", "Click to hide email"),
            );
            if let Some(plan) =
                live.and_then(|p| p["auth"]["label"].as_str().or(p["auth"]["type"].as_str()))
            {
                let text = format!("· {plan}");
                let w = widgets::text_width(ui, &text, small.clone());
                let at = place(w, &mut x, &mut y);
                widgets::text(ui, at, &text, small.clone(), color, 16.0);
            }
        } else {
            let (headline, _) = summary(live, enabled);
            let w = widgets::text_width(ui, &headline, small.clone());
            let at = place(w, &mut x, &mut y);
            widgets::text(ui, at, &headline, small, color, 16.0);
        }
    }

    fn rows_height(&self, ui: &Ui, rows: &[Row], width: f32) -> f32 {
        rows.iter()
            .enumerate()
            .map(|(i, row)| row_height(ui, row, width) - if i == 0 { 1.0 } else { 0.0 })
            .sum::<f32>()
            + 2.0
    }

    /// A card of settings rows; returns their actions.
    fn provider_card_rows(
        &mut self,
        ui: &mut Ui,
        left: f32,
        top: f32,
        width: f32,
        rows: &[Row],
    ) -> Vec<(String, RowAction)> {
        let card = Rect::from_min_size(
            Pos2::new(left, top),
            Vec2::new(width, self.rows_height(ui, rows, width)),
        );
        paint_card(ui, card);
        let mut actions = Vec::new();
        let mut row_top = card.top() + 1.0;
        for (index, row) in rows.iter().enumerate() {
            let height = row_height(ui, row, width) - if index == 0 { 1.0 } else { 0.0 };
            let rect = Rect::from_min_size(
                Pos2::new(card.left() + 1.0, row_top),
                Vec2::new(width - 2.0, height),
            );
            if index > 0 {
                ui.painter().rect_filled(
                    Rect::from_min_size(rect.min, Vec2::new(rect.width(), 1.0)),
                    0.0,
                    row_divider(),
                );
            }
            if let Some(action) = self.settings_row(ui, rect, row, index > 0) {
                actions.push((row.id.clone(), action));
            }
            row_top += height;
        }
        actions
    }

    /// `ProviderModelsSection` in its card; returns the card's bottom.
    fn provider_models(
        &mut self,
        ui: &mut Ui,
        row: &InstanceRow,
        live: Option<&Value>,
        left: f32,
        top: f32,
        width: f32,
    ) -> f32 {
        // `deriveProviderModelsForDisplay`: built-ins from the probe, custom
        // rows from the instance config so edits show at once.
        let live_models: Vec<Value> = live
            .and_then(|p| p["models"].as_array())
            .cloned()
            .unwrap_or_default();
        let custom_models = custom_model_entries(&row.instance["config"]["customModels"]);
        let mut models: Vec<Value> = live_models
            .iter()
            .filter(|m| m["isCustom"] != true)
            .cloned()
            .collect();
        if row.driver != "antigravity" {
            for (slug, name) in &custom_models {
                let capabilities = live_models
                    .iter()
                    .find(|m| m["isCustom"] == true && m["slug"] == slug.as_str())
                    .map(|m| m["capabilities"].clone())
                    .unwrap_or(Value::Null);
                models.push(json!({"slug": slug, "name": name, "isCustom": true, "capabilities": capabilities}));
            }
        }
        let adding = self
            .custom_model_input
            .as_ref()
            .filter(|(id, _, _)| *id == row.id)
            .map(|(_, text, error)| (text.clone(), error.clone()));
        let prefs = self.client_settings["providerModelPreferences"][&row.id].clone();
        let hidden = strings(&prefs["hiddenModels"]);
        let order = strings(&prefs["modelOrder"]);
        let favorites: Vec<String> = self.client_settings["favorites"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| f["provider"] == row.id.as_str())
            .filter_map(|f| f["model"].as_str().map(str::to_owned))
            .collect();
        let display = ordered_models(&models, &favorites, &hidden, &order);
        let favorite_count = display
            .iter()
            .filter(|(_, g)| *g == Group::Favorite)
            .count();
        let hidden_count = display
            .iter()
            .filter(|(m, _)| {
                m["isCustom"] != true
                    && hidden.contains(&m["slug"].as_str().unwrap_or_default().to_owned())
            })
            .count();
        // Group labels: "Favorites" and "All" when there are favorites, and
        // "Hidden from picker" before the hidden rows.
        let mut labels = 0.0;
        let mut previous: Option<Group> = None;
        for (index, (_, group)) in display.iter().enumerate() {
            if previous != Some(*group) && (favorite_count > 0 || *group == Group::Hidden) {
                labels += if index == 0 { 4.0 } else { 20.0 } + 16.0 + 6.0;
            }
            previous = Some(*group);
        }
        // The custom model editor opens under its row (`editingSlug`).
        let mut editor = self.custom_model_editor.take().filter(|e| {
            e.row == row.id
                && display
                    .iter()
                    .any(|(m, _)| m["isCustom"] == true && m["slug"] == e.slug.as_str())
        });
        let candidates: Vec<Value> = models
            .iter()
            .filter(|m| m["isCustom"] != true)
            .filter(|m| {
                m["capabilities"]["optionDescriptors"]
                    .as_array()
                    .is_some_and(|d| !d.is_empty())
            })
            .cloned()
            .collect();
        let editor_height = editor
            .as_ref()
            .map_or(0.0, |e| e.height(!candidates.is_empty()));
        let list_height = if display.is_empty() {
            32.0
        } else {
            display.len() as f32 * 28.0 + labels + editor_height
        };
        let card = Rect::from_min_size(
            Pos2::new(left, top),
            Vec2::new(
                width,
                1.0 + 12.0
                    + 16.0
                    + 12.0
                    + 24.0
                    + 8.0
                    + list_height
                    + 12.0
                    + 1.0
                    + adding.as_ref().map_or(0.0, |(_, error)| {
                        12.0 + 28.0 + if error.is_some() { 24.0 } else { 0.0 }
                    }),
            ),
        );
        paint_card(ui, card);
        let inner_left = card.left() + 17.0;
        let inner_right = card.right() - 17.0;
        widgets::text(
            ui,
            Pos2::new(inner_left, card.top() + 13.0),
            "Favorites, visibility, and ordering are saved on this device. Custom models are saved on the selected environment.",
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
            16.0,
        );
        let toolbar = card.top() + 41.0;
        let built_in: Vec<String> = models
            .iter()
            .filter(|m| m["isCustom"] != true)
            .filter_map(|m| m["slug"].as_str().map(str::to_owned))
            .collect();
        let mut x = inner_left;
        let mut new_hidden: Option<Vec<String>> = None;
        let mut new_order: Option<Vec<String>> = None;
        let mut toggle_favorite: Option<String> = None;
        if !built_in.is_empty() {
            let all_hidden = built_in.iter().all(|s| hidden.contains(s));
            let label = if all_hidden {
                "Enable all"
            } else {
                "Disable all"
            };
            let w = (widgets::text_width(ui, label, font(12.0, Weight::Medium)) + 16.0).round();
            if ghost_button(
                ui,
                Rect::from_min_size(Pos2::new(x, toolbar), Vec2::new(w, 24.0)),
                "models-bulk",
                None,
                label,
            ) {
                // `nextHiddenModelsForBulkToggle`.
                new_hidden = Some(if all_hidden {
                    hidden
                        .iter()
                        .filter(|s| !built_in.contains(s))
                        .cloned()
                        .collect()
                } else {
                    let mut next = hidden.clone();
                    for slug in &built_in {
                        if !next.contains(slug) {
                            next.push(slug.clone());
                        }
                    }
                    next
                });
            }
            x += w + 8.0;
        }
        let mut count = format!(
            "{} model{}",
            models.len(),
            if models.len() == 1 { "" } else { "s" }
        );
        if favorite_count > 0 {
            count.push_str(&format!(
                " · {favorite_count} favorite{}",
                if favorite_count == 1 { "" } else { "s" }
            ));
        }
        if hidden_count > 0 {
            count.push_str(&format!(" · {hidden_count} hidden"));
        }
        widgets::text(
            ui,
            Pos2::new(x, toolbar + 4.0),
            &count,
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
            16.0,
        );
        if row.driver != "antigravity" && adding.is_none() {
            let label = "Add custom model";
            let w = (widgets::text_width(ui, label, font(12.0, Weight::Medium)) + 34.0).round();
            if ghost_button(
                ui,
                Rect::from_min_size(Pos2::new(inner_right - w, toolbar), Vec2::new(w, 24.0)),
                "add-custom-model",
                Some("plus"),
                label,
            ) {
                self.custom_model_input = Some((row.id.clone(), String::new(), None));
            }
        }
        let mut remove_custom: Option<String> = None;
        let mut save_custom: Option<Value> = None;

        let mut y = toolbar + 32.0;
        if display.is_empty() {
            widgets::text(
                ui,
                Pos2::new(inner_left, y + 8.0),
                "No models reported for this provider yet.",
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
                16.0,
            );
        }
        let slugs: Vec<String> = display
            .iter()
            .map(|(m, _)| m["slug"].as_str().unwrap_or_default().to_owned())
            .collect();
        let mut previous: Option<Group> = None;
        for (index, (model, group)) in display.iter().enumerate() {
            if previous != Some(*group) && (favorite_count > 0 || *group == Group::Hidden) {
                y += if index == 0 { 4.0 } else { 20.0 };
                let label = match group {
                    Group::Favorite => "Favorites",
                    Group::Visible => "All",
                    Group::Hidden => "Hidden from picker",
                };
                widgets::text(
                    ui,
                    Pos2::new(inner_left, y),
                    label,
                    font(11.0, Weight::Regular),
                    theme::muted_foreground(),
                    16.0,
                );
                y += 16.0 + 6.0;
            }
            previous = Some(*group);
            let slug = &slugs[index];
            let custom = model["isCustom"] == true;
            let is_hidden = !custom && hidden.contains(slug);
            let fade = if is_hidden { 0.5 } else { 1.0 };
            let rect = Rect::from_min_max(
                Pos2::new(card.left() + 9.0, y),
                Pos2::new(card.right() - 9.0, y + 28.0),
            );
            if ui.rect_contains_pointer(rect) {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(6),
                    theme::alpha(theme::muted(), 0.3),
                );
            }
            let center = y + 14.0;
            // Star (favorite).
            let star = Rect::from_min_size(Pos2::new(inner_left, center - 10.0), Vec2::splat(20.0));
            let favorite = *group == Group::Favorite;
            let response = ui.interact(
                star,
                ui.id().with(("model-star", &row.id, slug)),
                Sense::click(),
            );
            theme::paint_icon(
                ui,
                if favorite { "star-filled" } else { "star" },
                star,
                12.0,
                theme::alpha(
                    if favorite {
                        theme::warning()
                    } else {
                        theme::muted_foreground()
                    },
                    fade,
                ),
            );
            if response
                .on_hover_text(if favorite {
                    "Remove from favorites"
                } else {
                    "Add to favorites"
                })
                .clicked()
            {
                toggle_favorite = Some(slug.clone());
            }
            // Name and slug.
            let name = model["name"].as_str().unwrap_or(slug);
            let name_color = if is_hidden {
                theme::muted_foreground()
            } else {
                theme::alpha(theme::foreground(), 0.9)
            };
            // `grid-cols-[1.5rem_…] gap-2`.
            let name_x = inner_left + 32.0;
            widgets::text(
                ui,
                Pos2::new(name_x, center - 8.0),
                name,
                font(12.0, Weight::Regular),
                theme::alpha(name_color, fade),
                16.0,
            );
            let mut text_x =
                name_x + widgets::text_width(ui, name, font(12.0, Weight::Regular)) + 8.0;
            if name != slug {
                widgets::text(
                    ui,
                    Pos2::new(text_x, center - 8.0),
                    slug,
                    theme::mono(11.0),
                    theme::alpha(theme::muted_foreground(), 0.7 * fade),
                    16.0,
                );
                text_x += widgets::text_width(ui, slug, theme::mono(11.0)) + 8.0;
            }
            if custom {
                widgets::text(
                    ui,
                    Pos2::new(text_x, center - 8.0),
                    "custom",
                    font(11.0, Weight::Regular),
                    theme::alpha(theme::muted_foreground(), 0.7 * fade),
                    16.0,
                );
            }
            // Switch (shown in picker), arrows, capabilities, right to left.
            let switch_rect = Rect::from_min_size(
                Pos2::new(inner_right - 26.0, center - 8.0),
                Vec2::new(26.0, 16.0),
            );
            let shown = !is_hidden;
            let switch_response = ui.interact(
                switch_rect,
                ui.id().with(("model-shown", &row.id, slug)),
                if custom {
                    Sense::hover()
                } else {
                    Sense::click()
                },
            );
            let track = if shown {
                theme::primary()
            } else {
                theme::overlay(0.08)
            };
            ui.painter().rect_filled(
                switch_rect,
                CornerRadius::same(8),
                theme::alpha(track, if custom { 0.64 } else { 1.0 } * fade),
            );
            let thumb_x = if shown {
                switch_rect.right() - 14.0
            } else {
                switch_rect.left() + 2.0
            };
            ui.painter().rect_filled(
                Rect::from_min_size(
                    Pos2::new(thumb_x, switch_rect.top() + 2.0),
                    Vec2::splat(12.0),
                ),
                CornerRadius::same(6),
                theme::background(),
            );
            let tip = if custom {
                "Custom models are always shown in the picker"
            } else if is_hidden {
                "Hidden from picker"
            } else {
                "Shown in picker"
            };
            if switch_response.on_hover_text(tip).clicked() && !custom {
                let mut next = hidden.clone();
                if shown {
                    next.push(slug.clone());
                } else {
                    next.retain(|s| s != slug);
                }
                new_hidden = Some(next);
            }
            let mut actions_right = switch_rect.left() - 8.0;
            if custom {
                // Remove (`XIcon`), then edit (`PencilIcon`) to its left.
                let button = Rect::from_min_size(
                    Pos2::new(actions_right - 20.0, center - 10.0),
                    Vec2::splat(20.0),
                );
                actions_right -= 22.0;
                if widgets::icon_button(
                    ui,
                    button,
                    ("model-remove", &row.id, slug),
                    "x",
                    12.0,
                    theme::muted_foreground(),
                    "Remove custom model",
                )
                .clicked()
                {
                    remove_custom = Some(slug.clone());
                }
                let button = Rect::from_min_size(
                    Pos2::new(actions_right - 20.0, center - 10.0),
                    Vec2::splat(20.0),
                );
                actions_right -= 22.0;
                if widgets::icon_button(
                    ui,
                    button,
                    ("model-edit", &row.id, slug),
                    "pencil",
                    12.0,
                    theme::muted_foreground(),
                    "Edit name and options",
                )
                .clicked()
                {
                    editor = if editor.as_ref().is_some_and(|e| e.slug == *slug) {
                        None
                    } else {
                        let entry = row.instance["config"]["customModels"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|e| {
                                e.as_str().or(e["slug"].as_str()).map(str::trim)
                                    == Some(slug.as_str())
                            })
                            .cloned()
                            .unwrap_or_else(|| json!(slug));
                        Some(CustomModelEditor::new(&row.id, &row.driver, &entry))
                    };
                }
            }
            if !is_hidden {
                let same_group = |other: usize| display.get(other).is_some_and(|(_, g)| g == group);
                for (direction, icon) in [(1i32, "arrow-down"), (-1, "arrow-up")] {
                    let button = Rect::from_min_size(
                        Pos2::new(actions_right - 20.0, center - 10.0),
                        Vec2::splat(20.0),
                    );
                    actions_right -= 22.0;
                    let target = index as i32 + direction;
                    let enabled = target >= 0 && same_group(target as usize);
                    let response = ui.interact(
                        button,
                        ui.id().with(("model-move", &row.id, slug, direction)),
                        if enabled {
                            Sense::click()
                        } else {
                            Sense::hover()
                        },
                    );
                    if enabled && response.hovered() {
                        ui.painter()
                            .rect_filled(button, CornerRadius::same(6), theme::accent());
                    }
                    theme::paint_icon(
                        ui,
                        icon,
                        button,
                        12.0,
                        theme::alpha(theme::muted_foreground(), if enabled { 1.0 } else { 0.5 }),
                    );
                    if enabled && response.clicked() {
                        let mut next = slugs.clone();
                        next.swap(index, target as usize);
                        new_order = Some(next);
                    }
                }
            }
            let caps = capabilities(model);
            if !caps.is_empty() {
                // The actions column is at least 5.5rem wide.
                let caps_right = switch_rect.left() - 8.0 - 88.0 - 8.0;
                let w = widgets::text_width(ui, &caps, font(11.0, Weight::Regular));
                widgets::text(
                    ui,
                    Pos2::new(caps_right - w, center - 7.0),
                    &caps,
                    font(11.0, Weight::Regular),
                    theme::alpha(theme::muted_foreground(), 0.7 * fade),
                    14.0,
                );
            }
            y += 28.0;
            if let Some(open) = editor.as_mut().filter(|e| e.slug == *slug) {
                let height = open.height(!candidates.is_empty());
                let rect = Rect::from_min_max(
                    Pos2::new(card.left() + 17.0, y + 4.0),
                    Pos2::new(card.right() - 17.0, y + height - 8.0),
                );
                match open.show(ui, rect, &candidates) {
                    EditorEvent::None => {}
                    EditorEvent::Cancel => editor = None,
                    EditorEvent::Save(setting) => {
                        save_custom = Some(setting);
                        editor = None;
                    }
                }
                y += height;
            }
        }

        // The add row: slug input, Add and Cancel, and the error below.
        let mut add_custom: Option<String> = None;
        if let Some((mut text, error)) = adding {
            let top = y + 12.0;
            let add_label = "Add";
            let cancel_label = "Cancel";
            let add_w =
                (widgets::text_width(ui, add_label, font(14.0, Weight::Medium)) + 20.0).round();
            let cancel_w =
                (widgets::text_width(ui, cancel_label, font(14.0, Weight::Medium)) + 20.0).round();
            let cancel = Rect::from_min_size(
                Pos2::new(inner_right - cancel_w, top),
                Vec2::new(cancel_w, 28.0),
            );
            let add = Rect::from_min_size(
                Pos2::new(cancel.left() - 8.0 - add_w, top),
                Vec2::new(add_w, 28.0),
            );
            let input = Rect::from_min_max(
                Pos2::new(inner_left, top),
                Pos2::new(add.left() - 8.0, top + 28.0),
            );
            ui.painter().rect(
                input,
                CornerRadius::same(8),
                theme::control_surface(),
                Stroke::new(1.0, theme::input()),
                StrokeKind::Inside,
            );
            let field = Rect::from_min_max(
                input.min + Vec2::new(10.0, 5.0),
                input.max - Vec2::new(10.0, 5.0),
            );
            let placeholder = match row.driver.as_str() {
                "codex" => "gpt-6.7-codex-ultra-preview",
                "claudeAgent" => "claude-sonnet-5",
                "cursor" => "claude-sonnet-4-6",
                "opencode" => "openai/gpt-5",
                _ => "model-slug",
            };
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
            let response = child.add(
                egui::TextEdit::singleline(&mut text)
                    .id(egui::Id::new(("custom-model-input", &row.id)))
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(font(14.0, Weight::Regular))
                    .text_color(theme::foreground())
                    .vertical_align(egui::Align::Center)
                    .hint_text(
                        egui::RichText::new(placeholder)
                            .font(font(14.0, Weight::Regular))
                            .color(theme::muted_foreground()),
                    )
                    .desired_width(field.width()),
            );
            if !response.has_focus() && !response.lost_focus() && text.is_empty() && error.is_none()
            {
                response.request_focus();
            }
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let escape = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
            let add_clicked = ghost_outline_button(ui, add, "custom-model-add", add_label, true);
            let cancel_clicked =
                ghost_outline_button(ui, cancel, "custom-model-cancel", cancel_label, false);
            if let Some(error) = &error {
                widgets::text(
                    ui,
                    Pos2::new(inner_left, top + 28.0 + 8.0),
                    error,
                    font(12.0, Weight::Regular),
                    Color32::from_rgb(251, 65, 74), // --destructive
                    16.0,
                );
            }
            let edited = text
                != self
                    .custom_model_input
                    .as_ref()
                    .map(|(_, t, _)| t.clone())
                    .unwrap_or_default();
            if escape || cancel_clicked {
                self.custom_model_input = None;
            } else if enter || add_clicked {
                add_custom = Some(text);
            } else if let Some((_, current, current_error)) = &mut self.custom_model_input {
                if edited {
                    *current_error = None;
                }
                *current = text;
            }
        }
        if let Some(text) = add_custom {
            // `handleAdd`.
            let slug = text.trim().to_owned();
            let error = if slug.is_empty() {
                Some("Enter a model slug.".to_owned())
            } else if models
                .iter()
                .any(|m| m["isCustom"] != true && m["slug"] == slug.as_str())
            {
                Some("That model is already built in.".to_owned())
            } else if slug.chars().count() > 256 {
                Some("Model slugs must be 256 characters or less.".to_owned())
            } else if custom_models.iter().any(|(s, _)| *s == slug) {
                Some("That custom model is already saved.".to_owned())
            } else {
                None
            };
            match error {
                Some(error) => {
                    self.custom_model_input = Some((row.id.clone(), text, Some(error)));
                }
                None => {
                    let mut next: Vec<Value> = row.instance["config"]["customModels"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    next.push(json!(slug));
                    self.save_custom_models(row, next);
                    self.custom_model_input = None;
                }
            }
        }
        if let Some(setting) = save_custom {
            // `handleSaveEdit`: the edited entry replaces its slug's.
            let slug = setting
                .as_str()
                .or(setting["slug"].as_str())
                .unwrap_or_default()
                .to_owned();
            let next: Vec<Value> = row.instance["config"]["customModels"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|entry| {
                    if entry.as_str().or(entry["slug"].as_str()).map(str::trim)
                        == Some(slug.as_str())
                    {
                        setting.clone()
                    } else {
                        entry.clone()
                    }
                })
                .collect();
            self.save_custom_models(row, next);
        }
        if remove_custom.is_some()
            && editor
                .as_ref()
                .is_some_and(|e| Some(&e.slug) == remove_custom.as_ref())
        {
            editor = None;
        }
        if editor.is_some() {
            self.custom_model_editor = editor;
        }
        if let Some(slug) = remove_custom {
            // `handleRemove`: also drop it from the order and favorites.
            let next: Vec<Value> = row.instance["config"]["customModels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|entry| entry.as_str().or(entry["slug"].as_str()) != Some(slug.as_str()))
                .cloned()
                .collect();
            self.save_custom_models(row, next);
            if order.contains(&slug) {
                new_order = Some(order.iter().filter(|s| **s != slug).cloned().collect());
            }
            if self.favorite_models.remove(&format!("{}/{slug}", row.id)) {
                self.save_favorites();
            }
        }

        // Model preferences are client settings (`providerModelPreferences`, `favorites`).
        if new_hidden.is_some() || new_order.is_some() {
            let hidden = new_hidden.unwrap_or(hidden);
            let order = new_order.unwrap_or(order);
            let mut all = self.client_settings["providerModelPreferences"].clone();
            if !all.is_object() {
                all = json!({});
            }
            if hidden.is_empty() && order.is_empty() {
                if let Some(object) = all.as_object_mut() {
                    object.remove(&row.id);
                }
            } else {
                all[&row.id] = json!({"hiddenModels": hidden, "modelOrder": order});
            }
            self.update_setting(Store::Client, "providerModelPreferences", all);
        }
        if let Some(slug) = toggle_favorite {
            let key = format!("{}/{slug}", row.id);
            if !self.favorite_models.remove(&key) {
                self.favorite_models.insert(key);
            }
            self.save_favorites();
        }
        card.bottom()
    }

    /// A `NumberField size="sm" w-32` with a 30-second step; returns the
    /// committed value.
    /// A `NumberField size="sm"` of seconds (decrement, digits, increment);
    /// returns the new value.
    pub(crate) fn interval_field(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        value: i64,
        key: &str,
        step: i64,
    ) -> Option<i64> {
        ui.painter().rect(
            rect,
            CornerRadius::same(10),
            theme::control_surface(),
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
        let mut result = None;
        for (index, icon) in ["minus", "plus"].into_iter().enumerate() {
            let button = if index == 0 {
                Rect::from_min_size(rect.min + Vec2::splat(1.0), Vec2::new(34.0, 26.0))
            } else {
                Rect::from_min_size(
                    Pos2::new(rect.right() - 35.0, rect.top() + 1.0),
                    Vec2::new(34.0, 26.0),
                )
            };
            let response = ui.interact(button, ui.id().with((key, "step", index)), Sense::click());
            if response.hovered() {
                let radius = if index == 0 {
                    CornerRadius {
                        nw: 9,
                        sw: 9,
                        ne: 0,
                        se: 0,
                    }
                } else {
                    CornerRadius {
                        nw: 0,
                        sw: 0,
                        ne: 9,
                        se: 9,
                    }
                };
                ui.painter().rect_filled(button, radius, theme::accent());
            }
            theme::paint_icon(ui, icon, button, 16.0, theme::foreground());
            if response.clicked() {
                let delta = if index == 0 { -step } else { step };
                result = Some((value + delta).max(0));
            }
        }
        let key = key.to_owned();
        let digits = Rect::from_min_max(
            Pos2::new(rect.left() + 35.0, rect.top() + 4.0),
            Pos2::new(rect.right() - 35.0, rect.bottom() - 4.0),
        );
        let mut buffer = self
            .settings_edits
            .get(&key)
            .cloned()
            .unwrap_or_else(|| value.to_string());
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(digits));
        let response = child.add(
            egui::TextEdit::singleline(&mut buffer)
                .id_salt((&key, "field"))
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font(14.0, Weight::Regular))
                .text_color(theme::foreground())
                .horizontal_align(egui::Align::Center)
                .vertical_align(egui::Align::Center)
                .desired_width(digits.width()),
        );
        if response.changed() {
            buffer.retain(|c| c.is_ascii_digit());
            self.settings_edits.insert(key.clone(), buffer.clone());
        }
        if response.lost_focus() {
            self.settings_edits.remove(&key);
            if let Ok(next) = buffer.trim().parse::<i64>()
                && next != value
            {
                result = Some(next);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_model_entries_dedupe_and_name() {
        let config =
            json!(["a", {"slug": " b ", "name": "Bee"}, "a", "", {"slug": "c", "name": " "}]);
        assert_eq!(
            custom_model_entries(&config),
            vec![
                ("a".to_owned(), "a".to_owned()),
                ("b".to_owned(), "Bee".to_owned()),
                ("c".to_owned(), "c".to_owned()),
            ]
        );
    }

    #[test]
    fn enabled_follows_resolve_provider_instance_enabled() {
        let row = |instance: Value| InstanceRow {
            id: "x".into(),
            driver: instance["driver"].as_str().unwrap().into(),
            instance,
        };
        assert!(row(json!({"driver": "codex"})).enabled());
        assert!(!row(json!({"driver": "kimiCode"})).enabled());
        assert!(!row(json!({"driver": "deepseekHarness"})).enabled());
        assert!(row(json!({"driver": "deepseekHarness", "enabled": true})).enabled());
        assert!(row(json!({"driver": "kimiCode", "config": {"enabled": true}})).enabled());
        assert!(
            !row(json!({"driver": "codex", "enabled": true, "config": {"enabled": false}}))
                .enabled()
        );
    }

    #[test]
    fn versions_and_summaries_follow_upstream() {
        assert_eq!(version_label(Some("0.163.0")).as_deref(), Some("v0.163.0"));
        assert_eq!(
            version_label(Some("agy_acp_server_20260818_01_RC01")).as_deref(),
            Some("2026-08-18 RC01")
        );
        let provider = json!({"enabled": true, "installed": true, "status": "ready",
            "auth": {"status": "authenticated", "label": "Pro"}});
        assert_eq!(summary(Some(&provider), true).0, "Authenticated · Pro");
        assert_eq!(summary(Some(&provider), false).0, "Disabled");
        assert_eq!(summary(None, true).0, "Checking provider status");
    }

    #[test]
    fn models_group_favorites_then_visible_then_hidden() {
        let models: Vec<Value> = ["a", "b", "c", "d"]
            .iter()
            .map(|s| json!({"slug": s, "name": s}))
            .collect();
        let favorites = vec!["c".to_owned()];
        let hidden = vec!["a".to_owned()];
        let order = vec!["d".to_owned()];
        let display: Vec<(String, Group)> = ordered_models(&models, &favorites, &hidden, &order)
            .into_iter()
            .map(|(m, g)| (m["slug"].as_str().unwrap().to_owned(), g))
            .collect();
        let slugs: Vec<&str> = display.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(slugs, ["c", "d", "b", "a"]);
        assert!(display[3].1 == Group::Hidden);
    }

    #[test]
    fn capability_words_match_the_composer() {
        let model = json!({"capabilities": {"optionDescriptors": [
            {"id": "serviceTier", "type": "select", "options": [{"id": "fast", "label": "Fast"}]},
            {"id": "reasoningEffort", "type": "select", "options": []}
        ]}});
        assert_eq!(capabilities(&model), "Fast mode · Reasoning");
    }
}
