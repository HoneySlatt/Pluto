//! Image generation in chats (Pluto, not upstream), after the image tools of
//! ChatGPT and Le Chat. Of the subscriptions Pluto drives, only Codex (a
//! ChatGPT login) generates images, so the chat composer's Image toggle sends
//! the next message to Codex's default model with an `image-request` context
//! record (a chip in the bubble), whatever model the chat uses: the server
//! hands the thread over with its context. The next message goes back to the
//! chat's model unless the user picked one. Codex saves the images under its
//! own folder, which `generated_images` reads (local servers only).

use eframe::egui::{CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

use pluto_core::chat_requests::IMAGE_LABEL as LABEL;
pub(crate) use pluto_core::chat_requests::image_message;

impl App {
    /// Codex's default model, when a Codex provider is ready.
    pub(crate) fn image_model(&self) -> Option<Value> {
        let provider = self
            .providers()
            .iter()
            .find(|p| p["driver"] == "codex" && p["enabled"] != false && p["status"] == "ready")?;
        let models = provider["models"].as_array()?;
        let model = models
            .iter()
            .find(|m| m["isDefault"] == true)
            .or(models.first())?;
        Some(json!({
            "instanceId": provider["instanceId"],
            "model": model["slug"],
        }))
    }

    /// Whether the next chat message asks for an image.
    pub(crate) fn image_on(&self, draft: bool) -> bool {
        self.chat_mode.image && self.chat_context(draft)
    }

    /// The model the next message in `thread_id` goes to after an image:
    /// the chat's own, unless the user picked one since.
    pub(crate) fn model_after_image(&mut self, thread_id: &str, draft: bool) -> Option<Value> {
        let returning = self.chat_mode.image_return.remove(thread_id)?;
        let picked = self
            .composer_selection
            .contains_key(&self.composer_key(draft));
        (!picked).then_some(returning)
    }

    /// The chat composer's Image toggle at `x`.
    pub(crate) fn image_toggle(&mut self, ui: &mut Ui, x: f32, top: f32, icon_only: bool) {
        let on = self.chat_mode.image;
        let label_font = font(14.0, Weight::Medium);
        let width = if icon_only {
            10.0 + 16.0 + 10.0
        } else {
            crate::ui::composer::toggle_width(widgets::text_width(ui, LABEL, label_font.clone()))
        };
        let rect = Rect::from_min_size(Pos2::new(x, top), Vec2::new(width, 28.0));
        let available = self.image_model().is_some();
        let tip = if !available {
            "Image generation needs Codex (a ChatGPT login) to be ready"
        } else if on {
            "Image is on for the next message — click to turn it off"
        } else {
            "Image: the next message makes an image, with Codex"
        };
        let response = ui
            .interact(rect, ui.id().with("image-toggle"), Sense::click())
            .on_hover_text(tip);
        if on {
            crate::ui::backdrop::paint_toggle_on(ui, rect, response.hovered());
        } else if response.hovered() && available {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), theme::accent());
        }
        let alpha = if available { 1.0 } else { 0.5 };
        let color = if on || response.hovered() {
            theme::foreground()
        } else {
            theme::secondary_label()
        };
        theme::paint_icon(
            ui,
            "image",
            Rect::from_min_size(Pos2::new(x + 10.0, top + 6.0), Vec2::splat(16.0)),
            16.0,
            theme::alpha(
                if on {
                    theme::primary()
                } else {
                    theme::muted_foreground()
                },
                alpha,
            ),
        );
        if !icon_only {
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x + 32.0, top), rect.max),
                LABEL,
                label_font,
                theme::alpha(color, alpha),
            );
        }
        if response.clicked() && (available || on) {
            self.chat_mode.image = !on;
            // One tool per message.
            if self.chat_mode.image {
                self.chat_mode.research = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_messages_lead_with_their_chip() {
        let (text, records) = image_message("un chat roux sur un toit, style aquarelle", &[]);
        let found = crate::ui::composer_context::collect(&text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, "image-request");
        assert_eq!(records[0]["kind"], "image-request");
        assert!(text.ends_with(" un chat roux sur un toit, style aquarelle"));
    }
}
