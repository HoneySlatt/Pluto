//! Quitting with Ctrl+Q (`confirmQuit`, `QuitHoldOverlay.tsx`): `direct`
//! quits at once; `hold` quits once the keys are held or pressed twice,
//! saying "Hold Ctrl+Q or press twice to quit" meanwhile; `double-click`
//! quits on a second press ("Press Ctrl+Q again to quit"). The hold time and
//! the double-press window live in upstream's Electron process, which the
//! fork does not ship: 1.2 s and 1 s are chosen here.

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, CornerRadius, Key, Modifiers, Pos2, Rect, Ui, Vec2};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const HOLD: Duration = Duration::from_millis(1200);
const SECOND_PRESS: Duration = Duration::from_millis(1000);

#[derive(Default)]
pub(crate) struct QuitHold {
    /// When the keys went down, and whether they still are.
    pressed: Option<Instant>,
    held: bool,
    /// The hint, until it goes.
    hint_until: Option<Instant>,
}

impl App {
    /// Every frame: Ctrl+Q per `confirmQuit`, and its hint.
    pub(crate) fn quit_hold(&mut self, ui: &mut Ui, area: Rect) {
        let mode = match &self.client_settings["confirmQuit"] {
            serde_json::Value::Bool(false) => "direct".to_owned(),
            serde_json::Value::String(mode) => mode.clone(),
            _ => "hold".to_owned(),
        };
        let now = Instant::now();
        let down = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Q));
        let still = ui.input(|i| i.key_down(Key::Q) && i.modifiers.command);
        let quit = |ui: &Ui| ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        if down {
            let second = self
                .quit_hold
                .pressed
                .is_some_and(|at| now.duration_since(at) < SECOND_PRESS);
            if mode == "direct" || second {
                quit(ui);
                return;
            }
            self.quit_hold.pressed = Some(now);
            self.quit_hold.held = true;
            self.quit_hold.hint_until = None;
        } else if self.quit_hold.held && !still {
            // Let go: a hold hint lingers, a double-press one goes.
            self.quit_hold.held = false;
            self.quit_hold.hint_until = Some(if mode == "hold" { now + HOLD } else { now });
        }
        if self.quit_hold.held
            && mode == "hold"
            && self
                .quit_hold
                .pressed
                .is_some_and(|at| now.duration_since(at) >= HOLD)
        {
            quit(ui);
            return;
        }
        let showing = self.quit_hold.held || self.quit_hold.hint_until.is_some_and(|at| now < at);
        if self
            .quit_hold
            .pressed
            .is_some_and(|at| now.duration_since(at) >= SECOND_PRESS)
            && !showing
        {
            self.quit_hold = QuitHold::default();
        }
        if !showing {
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_millis(50));
        let text = if mode == "hold" {
            "Hold Ctrl+Q or press twice to quit"
        } else {
            "Press Ctrl+Q again to quit"
        };
        let big = font(24.0, Weight::Bold);
        let width = widgets::text_width(ui, text, big.clone()) + 64.0;
        let rect = Rect::from_center_size(
            Pos2::new(area.center().x, area.top() + area.height() * 0.22 + 32.0),
            Vec2::new(width, 64.0),
        );
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("quit-hold"),
        ));
        painter.add(
            egui::epaint::Shadow {
                offset: [0, 10],
                blur: 25,
                spread: 0,
                color: Color32::from_black_alpha(80),
            }
            .as_shape(rect, CornerRadius::same(32)),
        );
        painter.rect_filled(
            rect,
            CornerRadius::same(32),
            theme::alpha(theme::foreground(), 0.95),
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            big,
            theme::background(),
        );
    }
}
