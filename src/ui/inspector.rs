//! An opened tool call's body (`V2ItemInspector.tsx`'s `ToolCallBody`,
//! `work-log/itemDetail.ts`'s `toolCallLines` and `turnItemOutputText`): the
//! call in the foreground (the full command, or the arguments as `key value`
//! lines, or their formatted JSON), then its result muted below, fetched
//! when the timeline withheld it ("Loading output…", "Couldn't load output:
//! …", "No output."), and a non-zero exit code. Command runs and dynamic tool
//! calls open this way; other items keep their plain body.
//!
//! Not ported: the shell command's highlighting (`ShellCommandBlock`) and the
//! JSON and TypeScript token colors; the result's images draw after its text
//! rather than before.

use eframe::egui::text::LayoutJob;
use eframe::egui::{Color32, CornerRadius, Pos2, Rect, ScrollArea, Sense, Ui, UiBuilder, Vec2};
use serde_json::Value;

use crate::app::App;
use crate::model::TurnItem;
use crate::theme;
use crate::ui::tool_images::{self, ToolOutput};
use crate::ui::widgets;

/// `MAX_TEXT_BLOCK_DEPTH`.
const MAX_DEPTH: usize = 4;
/// `max-h-80`.
const MAX_HEIGHT: f32 = 320.0;
/// The body's 11px mono type, `leading-relaxed`.
const LINE: f32 = 17.875;

/// The items that open as a tool call.
pub(crate) fn is_tool_call(item: &TurnItem) -> bool {
    matches!(item.kind.as_str(), "command_execution" | "dynamic_tool")
}

/// `textFromBlocks`: text blocks as text; `None` when something is not text.
fn text_from_blocks(value: &Value, depth: usize) -> Option<String> {
    if depth > MAX_DEPTH {
        return None;
    }
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => {
            let parts: Option<Vec<String>> = blocks
                .iter()
                .map(|block| text_from_blocks(block, depth + 1))
                .collect();
            Some(
                parts?
                    .into_iter()
                    .filter(|p| !p.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("text")
                && let Some(text) = map.get("text").and_then(Value::as_str)
            {
                return Some(text.to_owned());
            }
            // Images the client shows render on their own.
            if map.get("type").and_then(Value::as_str) == Some("image") {
                return Some(if tool_images::image_count(value) == 1 {
                    String::new()
                } else {
                    "[image]".into()
                });
            }
            if map.get("type").and_then(Value::as_str) == Some("resource_link")
                && let Some(uri) = map.get("uri").and_then(Value::as_str)
            {
                return Some(uri.to_owned());
            }
            if map.get("type").and_then(Value::as_str) == Some("resource")
                && let Some(resource) = map.get("resource").filter(|r| r.is_object())
            {
                if let Some(text) = resource["text"].as_str() {
                    return Some(text.to_owned());
                }
                if let Some(uri) = resource["uri"].as_str() {
                    return Some(uri.to_owned());
                }
            }
            let keys: Vec<&str> = map
                .keys()
                .map(String::as_str)
                .filter(|k| *k != "isError" && *k != "is_error")
                .collect();
            // MCP results wrap their text in `content`; `structuredContent`
            // repeats it as data.
            match keys.as_slice() {
                ["content"] => text_from_blocks(&map["content"], depth + 1),
                [_, _] if keys.contains(&"content") && keys.contains(&"structuredContent") => {
                    text_from_blocks(&map["content"], depth + 1).filter(|t| !t.trim().is_empty())
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// `prettyJsonText`: minified JSON documents, one per line, indented.
fn pretty_json_text(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with(['[', '{']) {
        return text.to_owned();
    }
    if let Ok(whole) = serde_json::from_str::<Value>(trimmed) {
        return serde_json::to_string_pretty(&whole).unwrap_or_else(|_| text.to_owned());
    }
    let documents: Option<Vec<Value>> = trimmed
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line.trim()).ok())
        .collect();
    match documents {
        Some(documents) => documents
            .iter()
            .map(|d| serde_json::to_string_pretty(d).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n\n"),
        None => text.to_owned(),
    }
}

/// `formatToolValue`: text blocks as text, the rest as JSON.
fn format_tool_value(value: &Value, image_preview: bool) -> Option<String> {
    if value.is_null() {
        return None;
    }
    if image_preview && value["type"] == "image" {
        return None;
    }
    if let Some(text) = text_from_blocks(value, 0) {
        return (!text.trim().is_empty()).then(|| pretty_json_text(&text));
    }
    let json = serde_json::to_string_pretty(value).ok()?;
    (json != "{}" && json != "[]").then_some(json)
}

/// `commandOutputText`: older Claude bash rows stored the raw result object.
fn command_output_text(output: &str) -> String {
    if !output.trim_start().starts_with("{\"stdout\"") {
        return output.to_owned();
    }
    let Ok(parsed) = serde_json::from_str::<Value>(output.trim()) else {
        return output.to_owned();
    };
    let (Some(stdout), Some(stderr)) = (parsed["stdout"].as_str(), parsed["stderr"].as_str())
    else {
        return output.to_owned();
    };
    if !parsed["interrupted"].is_boolean() {
        return output.to_owned();
    }
    [stdout, stderr]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `turnItemOutputText` for a call (the item's fields as sent).
fn output_text(kind: &str, item: &Value) -> Option<String> {
    match kind {
        "command_execution" => item["output"]
            .as_str()
            .filter(|o| !o.trim().is_empty())
            .map(command_output_text)
            .filter(|o| !o.is_empty()),
        "dynamic_tool" if item["outputOmitted"] == true => None,
        "dynamic_tool" => format_tool_value(&item["output"], !item["viewedImagePath"].is_null()),
        _ => None,
    }
}

/// `toolCallLines`.
enum CallLines {
    Command(String),
    Args(Vec<(String, String)>),
    Text(String),
    None,
}

fn call_lines(kind: &str, item: &Value) -> CallLines {
    if kind == "command_execution" {
        // The row's title truncates, so the body has the full command.
        return match item["input"].as_str().map(str::trim) {
            Some(command) if !command.is_empty() => CallLines::Command(command.to_owned()),
            _ => CallLines::None,
        };
    }
    let args = &item["input"];
    if let Some(map) = args.as_object().filter(|_| !tool_images::summarized(args)) {
        let mut lines: Vec<(String, String)> = map
            .iter()
            .map(|(key, value)| {
                let text = match value.as_str() {
                    Some(text) if !text.is_empty() => text.to_owned(),
                    _ => value.to_string(),
                };
                (key.clone(), text)
            })
            .collect();
        // Keep the inspector's alphabetical order even when another crate
        // enables serde_json's preserve_order feature.
        lines.sort_by(|a, b| a.0.cmp(&b.0));
        return if lines.is_empty() {
            CallLines::None
        } else {
            CallLines::Args(lines)
        };
    }
    match format_tool_value(args, false) {
        Some(text) => CallLines::Text(text),
        None => CallLines::None,
    }
}

/// Whether an unopened call has a body to open.
pub(crate) fn has_body(item: &TurnItem) -> bool {
    let wire = Value::Object(item.data.clone());
    tool_images::has_withheld_output(item)
        || !matches!(call_lines(&item.kind, &wire), CallLines::None)
        || output_text(&item.kind, &wire).is_some()
}

impl App {
    /// The opened call's body under its row, at `left`.
    pub(crate) fn inspector_body(&mut self, ui: &mut Ui, item: &TurnItem, left: f32, width: f32) {
        let wire = Value::Object(item.data.clone());
        let fetch = self.fetched_tool_item(item);
        let fetched = match &fetch {
            Some(ToolOutput::Item(value)) if value["type"] == item.kind.as_str() => {
                Some(value.clone())
            }
            _ => None,
        };
        let shown = fetched.as_deref().unwrap_or(&wire);
        let mono = theme::mono(11.0);
        let muted = theme::muted_foreground();
        let call_color = theme::alpha(theme::foreground(), 0.85);
        let mut job = LayoutJob::default();
        let push = |job: &mut LayoutJob, text: &str, color: Color32| {
            job.append(text, 0.0, widgets::format(mono.clone(), color, LINE));
        };
        let mut blocks = 0;
        let gap = |job: &mut LayoutJob, blocks: &mut usize| {
            if *blocks > 0 {
                job.append("\n\n", 0.0, widgets::format(theme::mono(4.0), muted, 6.0));
            }
            *blocks += 1;
        };
        match call_lines(&item.kind, shown) {
            CallLines::Command(command) => {
                gap(&mut job, &mut blocks);
                push(&mut job, &command, call_color);
            }
            CallLines::Args(lines) => {
                gap(&mut job, &mut blocks);
                for (index, (key, value)) in lines.iter().enumerate() {
                    if index > 0 {
                        push(&mut job, "\n", call_color);
                    }
                    push(&mut job, &format!("{key} "), muted);
                    push(&mut job, value, call_color);
                }
            }
            CallLines::Text(text) => {
                gap(&mut job, &mut blocks);
                push(&mut job, &text, muted);
            }
            CallLines::None => {}
        }
        let images = fetched
            .as_ref()
            .map_or(0, |f| tool_images::image_count(&f["output"]));
        let output = output_text(&item.kind, shown);
        let status = match (&output, &fetch) {
            (Some(_), _) => None,
            (None, Some(ToolOutput::Loading)) => Some(("Loading output…".to_owned(), muted)),
            (None, Some(ToolOutput::Failed(error))) => Some((
                format!("Couldn't load output: {error}"),
                Color32::from_rgb(251, 65, 74),
            )),
            (None, Some(ToolOutput::Item(_))) if images == 0 => {
                Some(("No output.".to_owned(), muted))
            }
            _ => None,
        };
        if let Some(output) = &output {
            gap(&mut job, &mut blocks);
            push(&mut job, output, muted);
        }
        if let Some((text, color)) = status {
            gap(&mut job, &mut blocks);
            push(&mut job, &text, color);
        }
        if let Some(code) = shown["exitCode"].as_i64().filter(|c| *c != 0) {
            gap(&mut job, &mut blocks);
            push(
                &mut job,
                &format!("exit {code}"),
                Color32::from_rgb(251, 65, 74),
            );
        }
        if blocks == 0 {
            return;
        }
        job.wrap.max_width = (width - 24.0).max(40.0);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let content = galley.size().y;
        let height = content.min(MAX_HEIGHT) + 16.0;
        ui.add_space(4.0);
        let (row, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        let frame = Rect::from_min_size(Pos2::new(left, row.top()), Vec2::new(width, height));
        ui.painter()
            .rect_filled(frame, CornerRadius::same(6), theme::overlay(0.012));
        let inner = frame.shrink2(Vec2::new(12.0, 8.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(inner));
        child.set_clip_rect(inner.intersect(ui.clip_rect()));
        ScrollArea::vertical()
            .id_salt(("inspector", &item.id))
            .max_height(MAX_HEIGHT)
            .auto_shrink([false, true])
            .show(&mut child, |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(inner.width(), content), Sense::hover());
                widgets::selectable_galley(
                    ui,
                    ui.id().with(("inspector-text", &item.id)),
                    rect.min,
                    galley,
                    muted,
                );
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn outputs_read_as_text_blocks_or_indented_json() {
        let mcp = json!({"content": [{"type": "text", "text": "{\"ok\":true}"}],
            "structuredContent": {"ok": true}});
        assert_eq!(
            format_tool_value(&mcp, false).as_deref(),
            Some("{\n  \"ok\": true\n}")
        );
        assert_eq!(
            format_tool_value(&json!({"rows": 2}), false).as_deref(),
            Some("{\n  \"rows\": 2\n}")
        );
        assert_eq!(format_tool_value(&json!({}), false), None);
        let claude = r#"{"stdout":"built","stderr":"","interrupted":false}"#;
        assert_eq!(command_output_text(claude), "built");
    }

    #[test]
    fn calls_list_their_arguments() {
        let CallLines::Args(lines) = call_lines(
            "dynamic_tool",
            &json!({"input": {"path": "src/app.rs", "limit": 20, "query": ""}}),
        ) else {
            panic!("arguments");
        };
        assert_eq!(
            lines,
            [
                ("limit".to_owned(), "20".to_owned()),
                ("path".to_owned(), "src/app.rs".to_owned()),
                ("query".to_owned(), "\"\"".to_owned()),
            ]
        );
        assert!(matches!(
            call_lines(
                "dynamic_tool",
                &json!({"input": {"summary": "big", "truncated": true}})
            ),
            CallLines::Text(_)
        ));
    }
}
