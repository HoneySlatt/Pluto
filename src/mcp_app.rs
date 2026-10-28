//! MCP Apps (`packages/shared/src/mcpApp.ts`, `toolOutput.ts`'s
//! `mcpAppFromToolItem`): an MCP tool can name a `ui://` HTML resource that
//! hosts show as an interactive view of its result. The server snapshots the
//! resource as a thread attachment and records an `McpAppReference` in the
//! tool item's output (`t3McpApp`); `crate::ui::mcp_apps` hosts it.

use serde_json::Value;

use crate::model::TurnItem;

/// `MCP_APP_PROTOCOL_VERSION`.
pub const PROTOCOL_VERSION: &str = "2026-01-26";
/// `MCP_APP_DEFAULT_HEIGHT` and its bounds.
pub const DEFAULT_HEIGHT: f32 = 320.0;
const MIN_HEIGHT: f32 = 80.0;
const MAX_HEIGHT: f32 = 2000.0;
const MAX_NAME_LENGTH: usize = 256;
const MAX_RESOURCE_URI_LENGTH: usize = 4096;

/// `McpAppReference`.
#[derive(Clone, Debug, PartialEq)]
pub struct McpApp {
    pub attachment_id: String,
    pub server: String,
    pub tool: String,
    pub resource_uri: String,
    /// The `_meta.ui.permissions` features the app asked for (`camera`,
    /// `microphone`, `geolocation`, `clipboardWrite`).
    pub permissions: Vec<String>,
    /// The well-formed `_meta.ui.csp`, kept as sent for the host's answer.
    pub csp: Value,
    pub prefers_border: bool,
}

fn bounded(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= MAX_NAME_LENGTH)
        .map(str::to_owned)
}

/// `readMcpAppReference`.
pub fn read_reference(value: &Value) -> Option<McpApp> {
    let resource_uri = value["resourceUri"]
        .as_str()
        .filter(|uri| uri.len() <= MAX_RESOURCE_URI_LENGTH && uri.starts_with("ui://"))?
        .to_owned();
    let permissions = ["camera", "microphone", "geolocation", "clipboardWrite"]
        .iter()
        .filter(|key| value["permissions"][**key].is_object())
        .map(|key| (*key).to_owned())
        .collect();
    Some(McpApp {
        attachment_id: bounded(&value["attachmentId"])?,
        server: bounded(&value["server"])?,
        tool: bounded(&value["tool"])?,
        resource_uri,
        permissions,
        csp: value["csp"].clone(),
        prefers_border: value["prefersBorder"] == true,
    })
}

/// `mcpAppFromToolItem`: a completed call whose output names the app of the
/// tool it is.
pub fn from_tool_item(item: &TurnItem) -> Option<McpApp> {
    if item.kind != "dynamic_tool" || item.status != "completed" {
        return None;
    }
    let app = read_reference(&item.get("output")["t3McpApp"])?;
    (item.str("toolName") == Some(format!("{}.{}", app.server, app.tool).as_str())).then_some(app)
}

/// `clampMcpAppHeight`.
pub fn clamp_height(height: f32) -> f32 {
    if height.is_finite() {
        height.round().clamp(MIN_HEIGHT, MAX_HEIGHT)
    } else {
        DEFAULT_HEIGHT
    }
}

/// `mcpAppFileName`: the tool's name, runs of other characters as one dash.
pub fn file_name(app: &McpApp) -> String {
    let mut name = String::new();
    let mut dash = false;
    for c in app.tool.chars() {
        if c.is_alphanumeric() || c == '_' || c == '.' || c == '-' {
            name.push(c);
            dash = false;
        } else if !dash {
            name.push('-');
            dash = true;
        }
    }
    let name: String = name.chars().take(80).collect();
    format!("{}.html", if name.is_empty() { "app" } else { &name })
}

/// `mcpAppAllowAttribute`.
pub fn allow_attribute(app: &McpApp) -> String {
    app.permissions
        .iter()
        .map(|p| {
            if p == "clipboardWrite" {
                "clipboard-write"
            } else {
                p.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn references_need_a_ui_resource_and_their_own_tool() {
        let reference = json!({"attachmentId": "a", "server": "weather", "tool": "forecast",
            "resourceUri": "ui://weather/forecast", "permissions": {"geolocation": {}},
            "prefersBorder": true});
        let app = read_reference(&reference).unwrap();
        assert_eq!(app.permissions, ["geolocation"]);
        assert_eq!(file_name(&app), "forecast.html");
        assert_eq!(allow_attribute(&app), "geolocation");
        let mut bad = reference.clone();
        bad["resourceUri"] = json!("https://example.com");
        assert!(read_reference(&bad).is_none());
        let item = |tool: &str| -> TurnItem {
            serde_json::from_value(json!({"id": "i", "threadId": "t", "ordinal": 1,
                "status": "completed", "updatedAt": "1", "type": "dynamic_tool",
                "toolName": tool, "output": {"t3McpApp": reference}}))
            .unwrap()
        };
        assert!(from_tool_item(&item("weather.forecast")).is_some());
        assert!(from_tool_item(&item("other.forecast")).is_none());
        assert_eq!(clamp_height(10.0), 80.0);
        assert_eq!(clamp_height(f32::NAN), DEFAULT_HEIGHT);
    }
}
