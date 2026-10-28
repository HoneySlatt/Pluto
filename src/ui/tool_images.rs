//! Images a tool returned (`V2ItemInspector.tsx`'s `ToolOutput`,
//! `work-log/itemDetail.ts`): a screenshot, an image file the agent read.
//! The timeline withholds a tool call's output (`outputOmitted`), so an
//! opened call fetches its item (`orchestration.getTurnItem`), counts the
//! images of its output like `toolOutputImages`, and loads each through a
//! `tool-output-image` asset, shown within 16rem of height under the call;
//! a click expands one.
//!
//! Not ported: the rest of the inspector (the fetched input and output text
//! of the call).
use eframe::egui::{CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::TurnItem;
use crate::theme;
use crate::ui::attachments::{self, MediaState};
use crate::ui::widgets;

/// `maxHeightRem={16}`.
const MAX_HEIGHT: f32 = 256.0;
/// `MAX_TOOL_OUTPUT_IMAGES`.
const MAX_IMAGES: usize = 8;
/// `LIVE_TURN_ITEM_STATUSES`: their detail is not cached by time.
const LIVE_STATUSES: [&str; 4] = ["idle", "pending", "running", "inProgress"];

/// A fetched call's images, by the revision fetched.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ToolOutput {
    Loading,
    Images(usize),
    Failed(String),
}

/// `turnItemDetailRevision`.
fn revision(item: &TurnItem) -> String {
    if LIVE_STATUSES.contains(&item.status.as_str()) {
        "live".into()
    } else {
        item.updated_at.clone()
    }
}

/// Whether the timeline withheld output the call's item holds.
pub(crate) fn has_withheld_output(item: &TurnItem) -> bool {
    item.kind == "dynamic_tool" && item.get("outputOmitted") == &Value::Bool(true)
}

/// `readToolOutputImage`'s raster types (the image types a turn can send).
fn raster(mime: &str) -> bool {
    matches!(
        mime.to_ascii_lowercase().as_str(),
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp"
    )
}

/// `toolOutputImages(...).length`: the image blocks of a block, a list of
/// blocks or an MCP result's `content`, in either the MCP `{data, mimeType}`
/// or the Anthropic `{source: {media_type}}` shape (or a detail read's
/// `{mimeType}` marker).
pub(crate) fn image_count(output: &Value) -> usize {
    let blocks: Vec<&Value> = match output {
        Value::Array(blocks) => blocks.iter().collect(),
        Value::Object(map) if map.get("content").is_some_and(Value::is_array) => {
            map["content"].as_array().into_iter().flatten().collect()
        }
        other => vec![other],
    };
    blocks
        .into_iter()
        .filter(|block| {
            if block["type"] != "image" {
                return false;
            }
            let source = &block["source"];
            if source.is_object() && source["type"] != "base64" {
                return false;
            }
            let mime = if source.is_object() {
                &source["media_type"]
            } else {
                &block["mimeType"]
            };
            mime.as_str().is_some_and(raster)
        })
        .take(MAX_IMAGES)
        .count()
}

fn image_key(item: &TurnItem, index: usize) -> String {
    format!("tool-image:{}:{}:{index}", item.thread_id, item.id)
}

impl App {
    /// Draws an opened call's images under it, fetching them the first time;
    /// returns whether anything was drawn.
    pub(crate) fn tool_output_images(
        &mut self,
        ui: &mut Ui,
        item: &TurnItem,
        left: f32,
        width: f32,
    ) -> bool {
        if !has_withheld_output(item) {
            return false;
        }
        let key = format!("{}@{}", item.id, revision(item));
        let state = self.tool_outputs.get(&key).cloned();
        let count = match state {
            None => {
                self.tool_outputs.insert(key.clone(), ToolOutput::Loading);
                self.send_request(
                    format!("turn-item:{key}"),
                    "orchestration.getTurnItem",
                    json!({"threadId": item.thread_id, "itemId": item.id,
                        "revision": revision(item)}),
                );
                return false;
            }
            Some(ToolOutput::Images(count)) if count > 0 => count,
            Some(ToolOutput::Images(_)) => return false,
            // `ToolOutput`'s pending and error lines.
            Some(ToolOutput::Loading) => {
                self.tool_output_note(ui, left, width, "Loading output…".into());
                return true;
            }
            Some(ToolOutput::Failed(error)) => {
                self.tool_output_note(ui, left, width, format!("Couldn't load output: {error}"));
                return true;
            }
        };
        for index in 0..count {
            let image_key = image_key(item, index);
            if attachments::media_state(&image_key).is_none() {
                attachments::mark_loading(&image_key);
                self.send_request(
                    format!("attachment:{image_key}"),
                    "assets.createUrl",
                    json!({"resource": {"_tag": "tool-output-image", "threadId": item.thread_id,
                        "itemId": item.id, "index": index}}),
                );
            }
            ui.add_space(6.0);
            let state = attachments::media_state(&image_key);
            let natural = match &state {
                Some(MediaState::Ready(image)) => Some((image.width(), image.height())),
                _ => attachments::image_size(&image_key),
            };
            let size = natural.map_or(Vec2::new(width.min(320.0), 180.0), |(w, h)| {
                let (w, h) = (w.max(1) as f32, h.max(1) as f32);
                let scale = 1.0_f32.min(width / w).min(MAX_HEIGHT / h);
                Vec2::new((w * scale).round(), (h * scale).round())
            });
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, size.y), Sense::hover());
            let rect = Rect::from_min_size(Pos2::new(left, row.top()), size);
            let radius = CornerRadius::same(8);
            let border = eframe::egui::Stroke::new(1.0, theme::alpha(theme::border(), 0.4));
            match state {
                Some(MediaState::Ready(image)) => {
                    attachments::paint_cover(
                        ui,
                        rect.shrink(1.0),
                        &image_key,
                        &image,
                        CornerRadius::same(7),
                    );
                    ui.painter().rect_stroke(
                        rect,
                        radius,
                        border,
                        eframe::egui::StrokeKind::Inside,
                    );
                    let response = ui.interact(
                        rect,
                        ui.id().with(("tool-image", &image_key)),
                        Sense::click(),
                    );
                    if response.clicked() {
                        self.open_lightbox(
                            vec![crate::ui::lightbox::Item {
                                name: "Tool output image".into(),
                                key: image_key.clone(),
                                image,
                                source: None,
                            }],
                            0,
                        );
                    }
                }
                state => {
                    ui.painter()
                        .rect_filled(rect, radius, theme::alpha(theme::muted(), 0.6));
                    ui.painter().rect_stroke(
                        rect,
                        radius,
                        border,
                        eframe::egui::StrokeKind::Inside,
                    );
                    if matches!(state, Some(MediaState::Failed)) {
                        widgets::text_in(
                            ui,
                            rect.shrink2(Vec2::new(12.0, 0.0)),
                            "Image unavailable",
                            theme::font(12.0, theme::Weight::Regular),
                            theme::muted_foreground(),
                        );
                    }
                }
            }
        }
        true
    }

    fn tool_output_note(&self, ui: &mut Ui, left: f32, width: f32, text: String) {
        ui.add_space(6.0);
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, 17.875), Sense::hover());
        widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(left, row.top()), Vec2::new(width, 17.875)),
            &text,
            theme::mono(11.0),
            theme::muted_foreground(),
        );
    }

    /// A fetched call (`orchestration.getTurnItem`); true when `key` was one.
    pub(crate) fn turn_item_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        let Some(key) = key.strip_prefix("turn-item:") else {
            return false;
        };
        let state = match result {
            Ok(value) if value["item"].is_null() => {
                ToolOutput::Failed("Output is no longer available.".into())
            }
            Ok(value) => ToolOutput::Images(image_count(&value["item"]["output"])),
            Err(error) => ToolOutput::Failed(error.clone()),
        };
        if self.tool_outputs.len() > 500 {
            self.tool_outputs.clear();
        }
        self.tool_outputs.insert(key.to_owned(), state);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_the_raster_images_of_every_output_shape() {
        let png = |data: bool| {
            let mut block = json!({"type": "image", "mimeType": "image/png"});
            if data {
                block["data"] = json!("iVBOR");
            }
            block
        };
        // A detail read's markers, an MCP result, Claude's stored blocks.
        assert_eq!(
            image_count(&json!([png(false), {"type": "text", "text": "x"}])),
            1
        );
        assert_eq!(
            image_count(&json!({"content": [png(true), png(true)], "structuredContent": {}})),
            2
        );
        assert_eq!(
            image_count(&json!([{"type": "image",
                "source": {"type": "base64", "media_type": "image/jpeg", "data": "x"}}])),
            1
        );
        // Not raster, not inline, or not an image.
        assert_eq!(
            image_count(&json!({"type": "image", "mimeType": "image/svg+xml"})),
            0
        );
        assert_eq!(
            image_count(&json!({"type": "image", "file": {"type": "image/png"}})),
            0
        );
        assert_eq!(image_count(&json!("text")), 0);
        let many: Vec<Value> = (0..12).map(|_| png(false)).collect();
        assert_eq!(image_count(&Value::Array(many)), MAX_IMAGES);
    }
}
