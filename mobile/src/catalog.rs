//! The models the server offers (`config.providers`), as the model sheet
//! lists them, after the desktop's `pickers.rs`: the ready, enabled
//! instances with their models; Ollama in chats only, its OpenCode companion
//! shown as Ollama in code. The sheet's Effort is the model's first select
//! trait (`TraitsPicker`), written like the desktop's traits menu.

use dioxus::prelude::*;
use pluto_core::model::ModelSelection;
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct Provider {
    pub instance_id: String,
    /// The driver whose mark shows (`ollama` for the OpenCode companion).
    pub driver: String,
    pub name: String,
    pub models: Vec<(String, String)>,
}

/// `is_ollama_companion`: the OpenCode instance beside an Ollama one.
fn ollama_companion(providers: &[Value], provider: &Value) -> bool {
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

/// The instances a chat (or a code thread) can pick, their models by label.
pub fn providers(all: &[Value], chat: bool) -> Vec<Provider> {
    let mut out = Vec::new();
    for provider in all {
        if provider["enabled"] == false || provider["status"] != "ready" {
            continue;
        }
        let companion = ollama_companion(all, provider);
        let driver = provider["driver"].as_str().unwrap_or_default();
        if (chat && companion) || (!chat && driver == "ollama") {
            continue;
        }
        let instance_id = provider["instanceId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let models: Vec<(String, String)> = provider["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| m["isLegacy"] != true)
            .filter_map(|m| {
                let slug = m["slug"].as_str()?.to_owned();
                let label = m["displayName"]
                    .as_str()
                    .or(m["name"].as_str())
                    .unwrap_or(&slug)
                    .to_owned();
                Some((slug, label))
            })
            .collect();
        if models.is_empty() {
            continue;
        }
        let name = if companion {
            "Ollama".to_owned()
        } else {
            provider["displayName"]
                .as_str()
                .unwrap_or(&instance_id)
                .to_owned()
        };
        out.push(Provider {
            instance_id,
            driver: if companion {
                "ollama".into()
            } else {
                driver.to_owned()
            },
            name,
            models,
        });
    }
    out
}

fn model_entry<'a>(all: &'a [Value], selection: &ModelSelection) -> Option<&'a Value> {
    all.iter()
        .find(|p| p["instanceId"] == selection.instance_id.as_str())?["models"]
        .as_array()?
        .iter()
        .find(|m| m["slug"] == selection.model.as_str())
}

/// The model's label and its instance's mark.
pub fn label(all: &[Value], selection: &ModelSelection) -> (String, String) {
    let label = model_entry(all, selection)
        .and_then(|m| m["displayName"].as_str().or(m["name"].as_str()))
        .map(str::to_owned)
        .unwrap_or_else(|| selection.model.clone());
    let driver = all
        .iter()
        .find(|p| p["instanceId"] == selection.instance_id.as_str())
        .map(|p| {
            if ollama_companion(all, p) {
                "ollama".to_owned()
            } else {
                p["driver"].as_str().unwrap_or_default().to_owned()
            }
        })
        .unwrap_or_default();
    (label, driver)
}

/// The first ready model, when nothing else picks one.
pub fn first_model(all: &[Value], chat: bool) -> ModelSelection {
    providers(all, chat)
        .into_iter()
        .next()
        .and_then(|p| {
            p.models.first().map(|(slug, _)| ModelSelection {
                instance_id: p.instance_id.clone(),
                model: slug.clone(),
                options: None,
            })
        })
        .unwrap_or_default()
}

/// A selection from a `defaultModelSelection`-like value, when it names a
/// model.
pub fn selection(value: &Value) -> Option<ModelSelection> {
    serde_json::from_value::<ModelSelection>(value.clone())
        .ok()
        .filter(|s| !s.model.is_empty())
}

/// A select trait: its id, options (id, label) and current option id.
pub type Trait = (String, Vec<(String, String)>, String);

/// The model's effort trait.
pub fn effort(all: &[Value], selection: &ModelSelection) -> Option<Trait> {
    let descriptor = model_entry(all, selection)?["capabilities"]["optionDescriptors"]
        .as_array()?
        .iter()
        .find(|d| d["type"] == "select")?;
    let id = descriptor["id"].as_str()?.to_owned();
    let options: Vec<(String, String)> = descriptor["options"]
        .as_array()?
        .iter()
        .filter_map(|o| {
            Some((
                o["id"].as_str()?.to_owned(),
                o["label"].as_str().unwrap_or_default().to_owned(),
            ))
        })
        .collect();
    let chosen = selection
        .options
        .as_ref()
        .and_then(|o| match o {
            Value::Array(list) => list
                .iter()
                .find(|e| e["id"] == id.as_str())
                .map(|e| &e["value"]),
            _ => o.get(&id),
        })
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            descriptor["currentValue"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| {
                    options
                        .iter()
                        .zip(descriptor["options"].as_array()?.iter())
                        .find(|(_, o)| o["isDefault"] == true)
                        .map(|((id, _), _)| id.clone())
                })
        })
        .unwrap_or_default();
    Some((id, options, chosen))
}

/// The selection with trait `id` set to `value`.
pub fn with_option(selection: &ModelSelection, id: &str, value: &str) -> ModelSelection {
    let mut next = selection.clone();
    next.options = Some(match next.options.take() {
        Some(Value::Array(mut list)) => {
            match list.iter_mut().find(|o| o["id"] == id) {
                Some(entry) => entry["value"] = json!(value),
                None => list.push(json!({"id": id, "value": value})),
            }
            Value::Array(list)
        }
        Some(mut map) if map.is_object() => {
            map[id] = json!(value);
            map
        }
        _ => json!({ id: value }),
    });
    next
}

/// The selection as sent on the wire.
pub fn wire(selection: &ModelSelection) -> Value {
    let mut model = json!({"instanceId": selection.instance_id, "model": selection.model});
    if let Some(options) = &selection.options {
        model["options"] = options.clone();
    }
    model
}

/// `RUNTIME_MODES` (`pickers.rs`): value, label, icon, description.
pub const ACCESS: [(&str, &str, &str, &str); 4] = [
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

pub fn access(mode: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    ACCESS
        .iter()
        .find(|a| a.0 == mode)
        .copied()
        .unwrap_or(ACCESS[3])
}

static CLAUDE: Asset = asset!("/assets/icons/claude.svg");
static OPENAI: Asset = asset!("/assets/icons/openai.svg");
static KIMI: Asset = asset!("/assets/icons/provider-kimi-dark.svg");
static VIBE: Asset = asset!("/assets/icons/provider-mistral-vibe.svg");
static OLLAMA: Asset = asset!("/assets/icons/provider-ollama.svg");
static ZCODE: Asset = asset!("/assets/icons/provider-zcode.svg");
static CURSOR: Asset = asset!("/assets/icons/provider-cursor.svg");
static GROK: Asset = asset!("/assets/icons/provider-grok.svg");
static OPENCODE: Asset = asset!("/assets/icons/opencode.svg");
static GEMINI: Asset = asset!("/assets/icons/provider-gemini.svg");
static PI: Asset = asset!("/assets/icons/provider-pi.svg");
static COPILOT: Asset = asset!("/assets/icons/provider-copilot.svg");

/// A driver's mark (`provider_icon`), or a usage source's (`claude`,
/// `codex`, ...).
pub fn mark(driver: &str) -> Option<Asset> {
    Some(match driver {
        "claudeAgent" | "claude" => CLAUDE,
        "codex" => OPENAI,
        "kimiCode" | "kimi" => KIMI,
        "mistralVibe" | "vibe" | "mistral" => VIBE,
        "ollama" => OLLAMA,
        "zcode" => ZCODE,
        "cursor" => CURSOR,
        "grok" => GROK,
        "opencode" => OPENCODE,
        "gemini" => GEMINI,
        "pi" => PI,
        "copilot" => COPILOT,
        _ => return None,
    })
}

/// `driver_label` (usage.rs) for usage sources and drivers.
pub fn driver_label(driver: &str) -> String {
    match driver {
        "claudeAgent" | "claude" => "Claude".into(),
        "codex" => "Codex".into(),
        "kimiCode" | "kimi" => "Kimi Code".into(),
        "mistralVibe" | "vibe" => "Mistral Vibe".into(),
        "zcode" => "Z Code".into(),
        "ollama" => "Ollama".into(),
        "opencode" => "OpenCode".into(),
        "cursor" => "Cursor".into(),
        "grok" => "Grok".into(),
        "gemini" => "Gemini".into(),
        other => other.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Vec<Value> {
        vec![
            json!({"instanceId": "claude", "driver": "claudeAgent", "status": "ready", "displayName": "Claude",
                "models": [{"slug": "opus", "name": "Opus 5.5", "capabilities": {"optionDescriptors": [
                    {"id": "effort", "type": "select", "options": [
                        {"id": "low", "label": "Low"}, {"id": "high", "label": "High", "isDefault": true}]}]}}]}),
            json!({"instanceId": "ollama", "driver": "ollama", "status": "ready", "models": [{"slug": "q"}]}),
            json!({"instanceId": "ollama_code", "driver": "opencode", "status": "ready", "models": [{"slug": "q"}]}),
            json!({"instanceId": "off", "driver": "codex", "status": "ready", "enabled": false, "models": [{"slug": "g"}]}),
        ]
    }

    #[test]
    fn chats_take_ollama_and_code_its_companion() {
        let all = config();
        let chat: Vec<String> = providers(&all, true)
            .into_iter()
            .map(|p| p.instance_id)
            .collect();
        let code: Vec<(String, String)> = providers(&all, false)
            .into_iter()
            .map(|p| (p.instance_id, p.driver))
            .collect();
        assert_eq!(chat, ["claude", "ollama"]);
        assert_eq!(
            code,
            [
                ("claude".into(), "claudeAgent".into()),
                ("ollama_code".into(), "ollama".into())
            ]
        );
    }

    #[test]
    fn effort_reads_and_writes_the_first_select() {
        let all = config();
        let opus = ModelSelection {
            instance_id: "claude".into(),
            model: "opus".into(),
            options: None,
        };
        let (id, options, chosen) = effort(&all, &opus).unwrap();
        assert_eq!(
            (id.as_str(), options.len(), chosen.as_str()),
            ("effort", 2, "high")
        );
        let low = with_option(&opus, "effort", "low");
        assert_eq!(effort(&all, &low).unwrap().2, "low");
        assert_eq!(wire(&low)["options"]["effort"], "low");
    }
}
