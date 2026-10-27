//! Pages agents publish with T3's `html_render` tool
//! (`packages/shared/src/htmlRender.ts`, `toolOutput.ts`'s
//! `htmlRenderFromToolItem`, `t3McpToolPresentation.ts`'s tool name
//! resolution, and the web client's `useHtmlRenderTheme.ts`): the reference a
//! completed call carries, the frame height at a width, and the theme handed
//! to the page in its URL fragment.

use serde_json::{Map, Value, json};

use crate::model::TurnItem;

pub const MIN_HEIGHT: f32 = 80.0;
pub const MAX_HEIGHT: f32 = 2000.0;
const MAX_TITLE_LENGTH: usize = 200;
const MAX_MEASURED_HEIGHTS: usize = 24;
/// `HTML_RENDER_COLUMN_WIDTH`.
pub const COLUMN_WIDTH: f32 = 728.0;

/// `HtmlRenderReference`.
#[derive(Clone, Debug, PartialEq)]
pub struct HtmlRender {
    pub attachment_id: String,
    pub title: String,
    /// The agent's frame height, and the cap on any measured height.
    pub height: f32,
    /// `[width, contentHeight]` the server measured at publish, by width.
    pub heights: Vec<(f32, f32)>,
}

/// `clampHtmlRenderHeight`.
pub fn clamp_height(height: f32) -> f32 {
    height.round().clamp(MIN_HEIGHT, MAX_HEIGHT)
}

fn is_html_render_tool(name: &str) -> bool {
    is_t3_tool(name, "html_render")
}

/// `resolveT3McpToolName`: whether `name` is T3's `tool` under any of the
/// prefixes providers give T3's server (`mcp__t3-code__`, `t3-code.`, …).
pub fn is_t3_tool(name: &str, tool: &str) -> bool {
    let label = name.trim();
    let label = label
        .strip_suffix(" completed")
        .or_else(|| label.strip_suffix(" complete"))
        .unwrap_or(label)
        .trim();
    if let Some(rest) = label.strip_prefix("mcp__") {
        return rest.split_once("__").is_some_and(|(server, rest_tool)| {
            matches!(
                server.to_lowercase().as_str(),
                "t3-code" | "t3_code" | "t3code"
            ) && rest_tool == tool
        });
    }
    let lower = label.to_lowercase();
    for server in ["t3-code", "t3_code", "t3code"] {
        if let Some(rest) = lower.strip_prefix(server) {
            let rest_tool = rest
                .trim_start_matches(|c: char| ".:/ -_·".contains(c))
                .trim();
            if rest_tool == tool {
                return true;
            }
        }
    }
    // OpenCode 2 names one server per thread: `t3-code-<thread>_html_render`.
    label == tool || (lower.starts_with("t3-code-") && lower.ends_with(&format!("_{tool}")))
}

/// `readHtmlRenderReference`.
fn read_reference(value: &Value) -> Option<HtmlRender> {
    let attachment_id = value["attachmentId"].as_str()?;
    if attachment_id.is_empty() || attachment_id.len() > 256 {
        return None;
    }
    let title = value["title"].as_str()?;
    let height = value["height"].as_f64().filter(|h| h.is_finite())? as f32;
    let title: String = title.trim().chars().take(MAX_TITLE_LENGTH).collect();
    let mut heights: Vec<(f32, f32)> = value["heights"]
        .as_array()
        .filter(|list| !list.is_empty() && list.len() <= MAX_MEASURED_HEIGHTS)
        .and_then(|list| {
            list.iter()
                .map(|entry| {
                    let pair = entry.as_array().filter(|p| p.len() == 2)?;
                    let width = pair[0].as_u64().filter(|w| (1..=10_000).contains(w))?;
                    let height = pair[1].as_f64().filter(|h| h.is_finite())?;
                    Some((width as f32, clamp_height(height as f32)))
                })
                .collect::<Option<Vec<_>>>()
        })
        .unwrap_or_default();
    heights.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some(HtmlRender {
        attachment_id: attachment_id.to_owned(),
        title: if title.is_empty() {
            "HTML".into()
        } else {
            title
        },
        height: clamp_height(height),
        heights,
    })
}

/// `htmlRenderFromToolItem` on a completed `dynamic_tool` item, whose wire
/// output the server already compacted (`compactDynamicToolOutput`).
pub fn from_tool_item(item: &TurnItem) -> Option<HtmlRender> {
    if item.kind != "dynamic_tool" || item.status != "completed" {
        return None;
    }
    if !item.str("toolName").is_some_and(is_html_render_tool) {
        return None;
    }
    let output = item.get("output");
    if output["isError"] == true {
        return None;
    }
    read_reference(&output["htmlRender"])
}

/// `measuredHeight`: the taller of the heights at the nearest widths on
/// each side.
fn measured_height(heights: &[(f32, f32)], width: f32) -> f32 {
    let high = heights
        .iter()
        .position(|(w, _)| *w >= width)
        .unwrap_or(heights.len() - 1);
    let low = if heights[high].0 == width {
        high
    } else {
        high.saturating_sub(1)
    };
    heights[low].1.max(heights[high].1)
}

impl HtmlRender {
    /// `htmlRenderFrameHeight`: the page's reported height when known, else
    /// the server's measurement at this width; the agent's height caps it
    /// only when it asked for a scrolling frame or nothing was measured.
    pub fn frame_height(&self, width: f32, content: Option<f32>) -> f32 {
        if self.heights.is_empty() {
            return clamp_height(self.height.min(content.unwrap_or(self.height)));
        }
        let cap = if measured_height(&self.heights, COLUMN_WIDTH) > self.height {
            self.height
        } else {
            MAX_HEIGHT
        };
        clamp_height(cap.min(content.unwrap_or_else(|| measured_height(&self.heights, width))))
    }
}

/// `HTML_RENDER_DEFAULT_FONTS`.
const DEFAULT_SANS: &str =
    r#"-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif"#;
const DEFAULT_MONO: &str =
    r#""SF Mono", "SFMono-Regular", Menlo, Consolas, "Liberation Mono", monospace"#;

/// `appearanceFontStack`: the chosen families before the default stack.
fn font_stack(custom: &str, default: &str) -> String {
    let families: Vec<String> = custom
        .split(',')
        .map(|f| f.trim().trim_matches(|c| c == '"' || c == '\''))
        .filter(|f| !f.is_empty())
        .map(|f| format!("\"{}\"", f.replace('"', "")))
        .collect();
    if families.is_empty() {
        default.to_owned()
    } else {
        format!("{}, {default}", families.join(", "))
    }
}

/// `htmlRenderTheme` for a theme's colors (`ThemeColors`), with the
/// interface and code fonts chosen in Settings.
pub fn theme(colors: &Map<String, Value>, dark: bool, sans: &str, mono: &str) -> Value {
    // `FIXED_COLORS`.
    let (success, success_fg, info, info_fg, chart) = if dark {
        (
            "#10b981",
            "#34d399",
            "#3b82f6",
            "#60a5fa",
            ["#2dd4bf", "#fbbf24", "#c084fc", "#fb7185", "#a3e635"],
        )
    } else {
        (
            "#10b981",
            "#047857",
            "#3b82f6",
            "#1d4ed8",
            ["#0d9488", "#d97706", "#9333ea", "#e11d48", "#65a30d"],
        )
    };
    let mut variables = Map::new();
    let mut role = |name: &str, key: &str| {
        if let Some(color) = colors.get(key).and_then(Value::as_str) {
            variables.insert(name.to_owned(), json!(color));
        }
    };
    for (name, key) in [
        ("--background", "canvas"),
        ("--foreground", "text"),
        ("--muted", "muted"),
        ("--muted-foreground", "mutedForeground"),
        ("--card", "surface"),
        ("--card-foreground", "text"),
        ("--popover", "surfaceOverlay"),
        ("--popover-foreground", "text"),
        ("--secondary", "secondary"),
        ("--secondary-foreground", "secondaryForeground"),
        ("--border", "border"),
        ("--input", "input"),
        ("--ring", "focus"),
        ("--primary", "messageAction"),
        ("--primary-foreground", "messageActionForeground"),
        ("--accent", "accent"),
        ("--accent-foreground", "accentForeground"),
        ("--accent-surface", "accentSurface"),
        ("--accent-surface-foreground", "accentSurfaceForeground"),
        ("--destructive", "error"),
        ("--destructive-foreground", "errorForeground"),
        ("--destructive-surface", "errorSurface"),
        ("--warning", "warning"),
        ("--warning-foreground", "warningForeground"),
        ("--warning-surface", "warningSurface"),
        ("--code-background", "codeBackground"),
        ("--code-foreground", "codeForeground"),
        ("--chart-1", "accent"),
    ] {
        role(name, key);
    }
    for (name, value) in [
        ("--success", success),
        ("--success-foreground", success_fg),
        ("--info", info),
        ("--info-foreground", info_fg),
    ] {
        variables.insert(name.into(), json!(value));
    }
    for (index, color) in chart.iter().enumerate() {
        variables.insert(format!("--chart-{}", index + 2), json!(color));
    }
    variables.insert("--radius".into(), json!("0.625rem"));
    variables.insert("--font-sans".into(), json!(font_stack(sans, DEFAULT_SANS)));
    variables.insert("--font-mono".into(), json!(font_stack(mono, DEFAULT_MONO)));
    json!({"appearance": if dark { "dark" } else { "light" }, "variables": variables})
}

/// `htmlRenderThemeFragment`: the page reads it before its first paint.
pub fn theme_fragment(theme: &Value) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(theme.to_string().as_bytes())
        .collect::<String>()
        // `encodeURIComponent` keeps spaces as %20.
        .replace('+', "%20");
    format!("#t3-theme={encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(tool: &str, status: &str, output: Value) -> TurnItem {
        serde_json::from_value(json!({
            "id": "i", "threadId": "t", "ordinal": 1, "status": status,
            "updatedAt": "2026-10-27T00:00:00Z", "type": "dynamic_tool",
            "toolName": tool, "input": {}, "output": output,
        }))
        .unwrap()
    }

    #[test]
    fn reads_the_page_from_any_t3_prefix() {
        let output = json!({"htmlRender": {"attachmentId": "a-html", "title": " Chart ",
            "height": 300, "heights": [[728, 420], [320, 900]]}});
        for tool in [
            "mcp__t3-code__html_render",
            "t3-code.html_render",
            "t3_code:html_render",
            "t3-code-thread_1_html_render",
        ] {
            let render = from_tool_item(&item(tool, "completed", output.clone())).unwrap();
            assert_eq!(render.title, "Chart");
            assert_eq!(render.heights, vec![(320.0, 900.0), (728.0, 420.0)]);
        }
        assert!(
            from_tool_item(&item(
                "mcp__other__html_render",
                "completed",
                output.clone()
            ))
            .is_none()
        );
        assert!(from_tool_item(&item("mcp__t3-code__html_render", "inProgress", output)).is_none());
        let failed = json!({"isError": true, "htmlRender": {"attachmentId": "a", "title": "x", "height": 100}});
        assert!(from_tool_item(&item("mcp__t3-code__html_render", "completed", failed)).is_none());
    }

    #[test]
    fn frames_fit_the_page_like_upstream() {
        let render = HtmlRender {
            attachment_id: "a".into(),
            title: "t".into(),
            height: 2000.0,
            heights: vec![(320.0, 900.0), (728.0, 420.0), (1000.0, 380.0)],
        };
        // Between two measured widths: the taller of the two.
        assert_eq!(render.frame_height(500.0, None), 900.0);
        assert_eq!(render.frame_height(728.0, None), 420.0);
        assert_eq!(render.frame_height(728.0, Some(450.0)), 450.0);
        // The agent asked for a scrolling frame: its height caps the page.
        let capped = HtmlRender {
            height: 300.0,
            ..render.clone()
        };
        assert_eq!(capped.frame_height(728.0, Some(450.0)), 300.0);
        // Never measured: the agent's height, or less once the page reports.
        let unmeasured = HtmlRender {
            heights: Vec::new(),
            ..capped
        };
        assert_eq!(unmeasured.frame_height(728.0, Some(10.0)), MIN_HEIGHT);
    }

    #[test]
    fn theme_travels_in_the_fragment() {
        let colors = json!({"canvas": "#161616", "text": "#f2f4f8", "accent": "#ffffff"});
        let theme = theme(colors.as_object().unwrap(), true, "Geist Mono", "");
        assert_eq!(theme["variables"]["--background"], "#161616");
        assert_eq!(theme["variables"]["--chart-1"], "#ffffff");
        assert_eq!(theme["variables"]["--chart-2"], "#2dd4bf");
        assert!(
            theme["variables"]["--font-sans"]
                .as_str()
                .unwrap()
                .starts_with("\"Geist Mono\", -apple-system")
        );
        let fragment = theme_fragment(&theme);
        assert!(fragment.starts_with("#t3-theme=%7B"));
        assert!(!fragment.contains('+'));
    }
}
