//! Deep research in chats (Pluto, not upstream), after the research modes
//! of ChatGPT, Claude, Le Chat and Kimi: the chat composer's Research
//! toggle sends the next message with a `research` context record, shown as
//! a chip in the bubble, whose payload asks the agent for an in-depth,
//! sourced report. The server hands record kinds it does not know to the
//! agent as they are (`projectComposerContextForProvider`), so every
//! provider gets the request with its own web search.

use eframe::egui::{CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const LABEL: &str = "Research";

/// What the agent is asked to do with a research message.
const RESEARCH_REQUEST: &str = "Deep research requested: treat the user's message as a research question and investigate it in depth before answering.
1. If the request is ambiguous in a way that would change the research, ask up to three short clarifying questions and stop there. Otherwise state your understanding in one line and start.
2. Plan the sub-questions. Search the web broadly, with many queries from several angles, favouring primary and recent sources, and read the most relevant pages in full rather than relying on snippets.
3. Cross-check important facts across independent sources, and note disagreements and how recent each source is.
4. Keep going until the key questions are answered or new searches stop adding anything.
5. Write a structured report in the user's language: a short summary of the findings first, then sections with headings, tables where they help compare, and a last section on open questions and limits. Cite every factual claim inline with Markdown links to the pages you used, and end with the list of sources. When a chart makes the findings clearer and html_render is available, publish one.";

/// The text and context records of a research message: `text` led by the
/// research chip, and `records` with its record.
pub(crate) fn research_message(text: &str, records: &[Value]) -> (String, Vec<Value>) {
    let id = format!("research_{}", uuid::Uuid::new_v4().simple());
    let record = json!({
        "version": 1,
        "contextId": id,
        "kind": "research",
        "label": LABEL,
        "payload": {"request": RESEARCH_REQUEST},
    });
    let chip = crate::ui::composer_context::format_reference("research", &id, LABEL);
    let mut records = records.to_vec();
    records.push(record);
    (format!("{chip} {text}"), records)
}

impl App {
    /// Whether the next chat message is a research request.
    pub(crate) fn research_on(&self, draft: bool) -> bool {
        self.chat_mode.research && self.chat_context(draft) && !self.temporary_context()
    }

    /// The chat composer's Research toggle at `x`.
    /// Returns its right edge.
    pub(crate) fn research_toggle(
        &mut self,
        ui: &mut Ui,
        x: f32,
        top: f32,
        icon_only: bool,
    ) -> f32 {
        let on = self.chat_mode.research;
        let label_font = font(14.0, Weight::Medium);
        let width = if icon_only {
            10.0 + 16.0 + 10.0
        } else {
            crate::ui::composer::toggle_width(widgets::text_width(ui, LABEL, label_font.clone()))
        };
        let rect = Rect::from_min_size(Pos2::new(x, top), Vec2::new(width, 28.0));
        let tip = if on {
            "Deep research is on for the next message — click to turn it off"
        } else {
            "Deep research: an in-depth, sourced report on the next message"
        };
        let response = ui
            .interact(rect, ui.id().with("research-toggle"), Sense::click())
            .on_hover_text(tip);
        if on {
            crate::ui::backdrop::paint_toggle_on(ui, rect, response.hovered());
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), theme::accent());
        }
        let color = if on || response.hovered() {
            theme::foreground()
        } else {
            theme::secondary_label()
        };
        theme::paint_icon(
            ui,
            "file-search",
            Rect::from_min_size(Pos2::new(x + 10.0, top + 6.0), Vec2::splat(16.0)),
            16.0,
            if on {
                theme::primary()
            } else {
                theme::muted_foreground()
            },
        );
        if !icon_only {
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x + 32.0, top), rect.max),
                LABEL,
                label_font,
                color,
            );
        }
        if response.clicked() {
            self.chat_mode.research = !on;
            // One tool per message.
            if self.chat_mode.research {
                self.chat_mode.image = false;
            }
        }
        rect.right()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_messages_lead_with_their_chip() {
        let (text, records) = research_message("Compare les mutuelles", &[]);
        let found = crate::ui::composer_context::collect(&text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, "research");
        assert!(text.ends_with(" Compare les mutuelles"));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["contextId"], found[0].context_id.as_str());
        assert!(
            records[0]["payload"]["request"]
                .as_str()
                .unwrap()
                .contains("Cite every factual claim")
        );
    }
}
