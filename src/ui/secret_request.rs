//! A secret an agent asks for (`SecretRequestCard.tsx`,
//! `client-runtime/secretRequest.ts`): while pending in this thread, a card
//! with a masked field, "Save securely" and "Decline"; otherwise one row
//! saying how it ended. The value stays in memory, zeroed when dropped, is
//! sent once with `secrets.answerRequest`, and never goes to the drafts,
//! toasts or logs.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, CornerRadius, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::json;
use zeroize::Zeroizing;

use crate::app::App;
use crate::model::TurnItem;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// The cards' fields, failures and answers on their way, by item id.
#[derive(Default)]
pub(crate) struct Secrets {
    values: HashMap<String, Zeroizing<String>>,
    errors: HashMap<String, String>,
    busy: HashSet<String>,
}

const KEY: &str = "secret-answer:";

impl App {
    /// The card (pending here) or its outcome row, in `width`.
    pub(crate) fn secret_request(&mut self, ui: &mut Ui, item: &TurnItem, width: f32) {
        let label = item.str("label").unwrap_or("Secret").to_owned();
        let status = item.str("secretStatus").unwrap_or("pending");
        let local = self.selected.as_deref() == Some(item.thread_id.as_str());
        if status != "pending" || !local {
            self.secrets.values.remove(&item.id);
            let (icon, outcome) = match status {
                "pending" => ("lock", "Waiting for an answer in the original thread"),
                "saved" => ("check", "Saved securely and kept private"),
                "declined" => ("minus", "Declined"),
                _ => ("minus", "Request ended"),
            };
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::hover());
            theme::paint_icon(
                ui,
                icon,
                Rect::from_center_size(
                    Pos2::new(rect.left() + 8.0, rect.center().y),
                    Vec2::splat(14.0),
                ),
                14.0,
                theme::muted_foreground(),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(rect.left() + 24.0, rect.top()), rect.max),
                &format!("{label} · {outcome}"),
                font(14.0, Weight::Regular),
                theme::muted_foreground(),
            );
            return;
        }
        let reason = item.str("reason").map(str::trim).filter(|r| !r.is_empty());
        let error = self.secrets.errors.get(&item.id).cloned();
        let height = 16.0
            + 20.0
            + reason.map_or(0.0, |_| 4.0 + 20.0)
            + 12.0
            + 32.0
            + error.as_ref().map_or(0.0, |_| 12.0 + 16.0)
            + 12.0
            + 20.0
            + 16.0;
        let (card, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        ui.painter().rect(
            card,
            CornerRadius::same(14),
            theme::card(),
            Stroke::new(1.0, theme::alpha(theme::border(), 0.6)),
            StrokeKind::Inside,
        );
        let left = card.left() + 16.0;
        let inner = card.width() - 32.0;
        let mut y = card.top() + 16.0;
        widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner, 20.0)),
            &label,
            font(14.0, Weight::Medium),
            theme::foreground(),
        );
        y += 20.0;
        if let Some(reason) = reason {
            y += 4.0;
            widgets::text_in(
                ui,
                Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner, 20.0)),
                reason,
                font(14.0, Weight::Regular),
                theme::muted_foreground(),
            );
            y += 20.0;
        }
        y += 12.0;
        // The masked field and "Save securely".
        let busy = self.secrets.busy.contains(&item.id);
        let button_label = "Save securely";
        let button_width = widgets::text_width(ui, button_label, font(14.0, Weight::Medium)) + 22.0;
        let field = Rect::from_min_size(
            Pos2::new(left, y + 1.0),
            Vec2::new(inner - 8.0 - button_width, 30.0),
        );
        ui.painter().rect(
            field,
            CornerRadius::same(8),
            theme::background(),
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
        let placeholder = item
            .str("placeholder")
            .unwrap_or("Paste the secret")
            .to_owned();
        let value = self.secrets.values.entry(item.id.clone()).or_default();
        let mut text = std::mem::take(&mut **value);
        // In its own child: the card's height alone moves the timeline on.
        let mut field_ui =
            ui.new_child(egui::UiBuilder::new().max_rect(field.shrink2(Vec2::new(11.0, 5.0))));
        let edit = field_ui.add(
            egui::TextEdit::singleline(&mut text)
                .password(true)
                .frame(egui::Frame::NONE)
                .hint_text(placeholder)
                .interactive(!busy)
                .font(font(14.0, Weight::Regular)),
        );
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        let ready = !text.trim().is_empty() && !busy;
        **value = text;
        let button = Rect::from_min_size(
            Pos2::new(field.right() + 8.0, y),
            Vec2::new(button_width, 32.0),
        );
        let save =
            widgets::primary_button(ui, button, ("secret-save", &item.id), button_label, ready)
                .clicked();
        y += 32.0;
        if let Some(error) = &error {
            y += 12.0;
            widgets::text_in(
                ui,
                Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner, 16.0)),
                error,
                font(12.0, Weight::Regular),
                theme::error(),
            );
            y += 16.0;
        }
        y += 12.0;
        theme::paint_icon(
            ui,
            "shield-check",
            Rect::from_center_size(Pos2::new(left + 7.0, y + 10.0), Vec2::splat(14.0)),
            14.0,
            theme::muted_foreground(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_size(
                Pos2::new(left + 20.0, y + 2.0),
                Vec2::new(inner - 100.0, 16.0),
            ),
            "Stored securely, never shown to the agent",
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
        );
        let decline_width = widgets::text_width(ui, "Decline", font(12.0, Weight::Medium)) + 14.0;
        let decline_rect = Rect::from_min_size(
            Pos2::new(card.right() - 16.0 - decline_width, y - 2.0),
            Vec2::new(decline_width, 24.0),
        );
        let decline = ui.interact(
            decline_rect,
            ui.id().with(("secret-decline", &item.id)),
            if busy { Sense::hover() } else { Sense::click() },
        );
        if decline.hovered() && !busy {
            ui.painter()
                .rect_filled(decline_rect, CornerRadius::same(6), theme::accent());
        }
        widgets::text_in(
            ui,
            decline_rect.shrink2(Vec2::new(7.0, 0.0)),
            "Decline",
            font(12.0, Weight::Medium),
            theme::muted_foreground(),
        );
        if (save || enter) && ready {
            let secret = self
                .secrets
                .values
                .get(&item.id)
                .map(|v| v.trim().to_owned())
                .unwrap_or_default();
            self.answer_secret(item, json!({"type": "save", "secret": secret}));
        } else if decline.clicked() && !busy {
            self.answer_secret(item, json!({"type": "decline"}));
        }
    }

    fn answer_secret(&mut self, item: &TurnItem, answer: serde_json::Value) {
        if !self.secrets.busy.insert(item.id.clone()) {
            return;
        }
        self.secrets.errors.remove(&item.id);
        self.send_request(
            format!("{KEY}{}", item.id),
            "secrets.answerRequest",
            json!({"threadId": item.thread_id, "turnItemId": item.id, "answer": answer}),
        );
    }

    /// The answer's receipt: the field empties, or the failure shows.
    pub(crate) fn secret_result(
        &mut self,
        key: &str,
        result: &Result<serde_json::Value, String>,
    ) -> bool {
        let Some(id) = key.strip_prefix(KEY) else {
            return false;
        };
        self.secrets.busy.remove(id);
        match result {
            Ok(_) => {
                self.secrets.values.remove(id);
            }
            Err(error) => {
                self.secrets.errors.insert(id.to_owned(), error.clone());
            }
        }
        true
    }
}
