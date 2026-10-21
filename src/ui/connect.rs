//! Connection screen, styled after the upstream pairing card (`pair.tsx`).
//! A native client cannot use browser pairing cookies, so it asks for the
//! server address and a bearer token (`t3 auth session issue`).
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

impl App {
    pub(crate) fn connect_screen(&mut self, ui: &mut Ui, rect: Rect) {
        ui.painter().rect_filled(rect, 0.0, theme::background());
        let card = Rect::from_center_size(rect.center(), Vec2::new(576.0, 456.0));
        ui.painter().rect(
            card,
            CornerRadius::same(16),
            theme::card(),
            Stroke::new(1.0, theme::border()),
            StrokeKind::Inside,
        );
        let x = card.left() + 33.0;
        let inner = card.width() - 66.0;
        let mut eyebrow = widgets::format(
            font(11.0, Weight::SemiBold),
            theme::muted_foreground(),
            16.0,
        );
        eyebrow.extra_letter_spacing = 2.2;
        let mut job = egui::text::LayoutJob::default();
        job.append("T3 CODE (ALPHA)", 0.0, eyebrow);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(
            Pos2::new(x, card.top() + 32.0),
            galley,
            theme::muted_foreground(),
        );
        widgets::text(
            ui,
            Pos2::new(x, card.top() + 60.0),
            "Connect to a T3 Code server",
            font(30.0, Weight::SemiBold),
            theme::foreground(),
            36.0,
        );
        widgets::text(
            ui,
            Pos2::new(x, card.top() + 104.0),
            "Enter the server address and an access token to start a session.",
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            20.0,
        );

        let mut y = card.top() + 152.0;
        for (label, secret) in [("Server", false), ("Access token", true)] {
            widgets::text(
                ui,
                Pos2::new(x, y),
                label,
                font(14.0, Weight::Medium),
                theme::foreground(),
                20.0,
            );
            let field = Rect::from_min_size(Pos2::new(x, y + 24.0), Vec2::new(inner, 32.0));
            ui.painter().rect(
                field,
                CornerRadius::same(8),
                theme::muted(),
                Stroke::new(1.0, theme::input()),
                StrokeKind::Inside,
            );
            let edit_rect = field.shrink2(Vec2::new(12.0, 6.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(edit_rect));
            let edit = if secret {
                egui::TextEdit::singleline(&mut *self.connect.token)
                    .password(true)
                    .hint_text(
                        egui::RichText::new("Paste a bearer token")
                            .color(theme::muted_foreground()),
                    )
            } else {
                egui::TextEdit::singleline(&mut self.connect.server).hint_text(
                    egui::RichText::new("http://127.0.0.1:3773").color(theme::muted_foreground()),
                )
            };
            child.add(
                edit.frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(font(14.0, Weight::Regular))
                    .text_color(theme::foreground())
                    .desired_width(edit_rect.width()),
            );
            y += 76.0;
        }

        let button_font = font(14.0, Weight::Medium);
        let label = if self.connect.busy {
            "Connecting…"
        } else {
            "Continue"
        };
        let width = widgets::text_width(ui, label, button_font.clone()) + 20.0;
        let button = Rect::from_min_size(Pos2::new(x, y + 8.0), Vec2::new(width, 28.0));
        let response = ui.interact(
            button,
            ui.id().with("connect"),
            if self.connect.busy {
                Sense::hover()
            } else {
                Sense::click()
            },
        );
        let fill = if response.hovered() {
            theme::primary_hover()
        } else {
            theme::primary()
        };
        ui.painter()
            .rect_filled(button, CornerRadius::same(8), fill);
        widgets::text_in(
            ui,
            Rect::from_min_max(button.min + Vec2::new(10.0, 0.0), button.max),
            label,
            button_font,
            Color32::WHITE,
        );
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (response.clicked() || enter) && !self.connect.busy {
            self.connect_now(ui.ctx());
        }

        let note = Rect::from_min_size(Pos2::new(x, y + 60.0), Vec2::new(inner, 64.0));
        ui.painter().rect(
            note,
            CornerRadius::same(10),
            Color32::TRANSPARENT,
            Stroke::new(1.0, theme::border()),
            StrokeKind::Inside,
        );
        let message = self.connect.error.clone().unwrap_or_else(|| {
            "Issue a token on the server with `t3 auth session issue`. It is kept in memory only.".into()
        });
        let color = if self.connect.error.is_some() {
            theme::error_foreground()
        } else {
            theme::muted_foreground()
        };
        let galley = widgets::galley(
            ui,
            &message,
            font(12.0, Weight::Regular),
            color,
            20.0,
            inner - 26.0,
        );
        ui.painter()
            .galley(note.min + Vec2::new(13.0, 12.0), galley, color);
    }
}
