//! The thread's error banner (`ThreadErrorBanner.tsx`, placed by
//! `ChatView.tsx`'s banner lane): the runtime's `lastError` at the top of
//! the chat, an error, or a warning for a usage limit, clamped to three
//! lines with the full text in a tooltip, and its Dismiss button. A
//! dismissal lasts the run, for that thread and that message: another
//! error on the same thread shows again.
//!
//! Not ported: the ChatGPT usage-limit variant with its usage button.

use eframe::egui::{CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// `max-w-[min(48rem,calc(100%-2rem))]`.
const MAX_WIDTH: f32 = 768.0;

impl App {
    /// The error shown for the open thread, unless dismissed.
    fn visible_thread_error(&self) -> Option<(String, bool)> {
        let thread = self.current_thread()?;
        let runtime = thread.runtime.as_ref()?;
        let error = runtime.last_error.clone()?;
        let key = format!("{}\0{error}", thread.id);
        if self.dismissed_thread_errors.contains(&key) {
            return None;
        }
        let usage_limit = runtime.last_error_class.as_deref() == Some("usage_limit");
        Some((error, usage_limit))
    }

    /// Draws the banner centered at the top of `body`.
    pub(crate) fn thread_error_banner(&mut self, ui: &mut Ui, body: Rect) {
        let Some((error, usage_limit)) = self.visible_thread_error() else {
            return;
        };
        // `w-fit`: as wide as its text, up to the max.
        let chrome = 16.0 + 16.0 + 8.0 + 28.0 + 12.0;
        let natural = widgets::text_width(ui, &error, font(14.0, Weight::Regular)) + chrome;
        let width = MAX_WIDTH.min(body.width() - 32.0).min(natural.ceil());
        if width < 120.0 {
            return;
        }
        let text_width = width - chrome;
        let full = widgets::galley(
            ui,
            &error,
            font(14.0, Weight::Regular),
            theme::foreground(),
            20.0,
            text_width,
        );
        // `line-clamp-3`.
        let clamped = full.rows.len() > 3;
        let galley = if clamped {
            let mut job = eframe::egui::text::LayoutJob::default();
            job.append(
                &error,
                0.0,
                widgets::format(font(14.0, Weight::Regular), theme::foreground(), 20.0),
            );
            job.wrap.max_width = text_width;
            job.wrap.max_rows = 3;
            job.wrap.overflow_character = Some('…');
            widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job)))
        } else {
            full
        };
        let height = galley.size().y.max(20.0) + 24.0;
        let rect = Rect::from_min_size(
            Pos2::new((body.center().x - width / 2.0).round(), body.top() + 12.0),
            Vec2::new(width, height),
        );
        let tone = if usage_limit {
            theme::warning()
        } else {
            theme::error_foreground()
        };
        // `surface="glass"` over the timeline, tinted by its variant.
        let painter = ui.painter();
        painter.rect(
            rect,
            CornerRadius::same(12),
            theme::popover(),
            Stroke::NONE,
            StrokeKind::Inside,
        );
        painter.rect(
            rect,
            CornerRadius::same(12),
            theme::alpha(tone, 0.08),
            Stroke::new(1.0, theme::alpha(tone, 0.32)),
            StrokeKind::Inside,
        );
        theme::paint_icon(
            ui,
            "circle-alert",
            Rect::from_min_size(rect.min + Vec2::new(16.0, 14.0), Vec2::splat(16.0)),
            16.0,
            tone,
        );
        let text_rect =
            Rect::from_min_size(rect.min + Vec2::new(16.0 + 16.0 + 8.0, 12.0), galley.size());
        let hover = ui.interact(text_rect, ui.id().with("thread-error"), Sense::hover());
        ui.painter()
            .galley(text_rect.min, galley, theme::foreground());
        if clamped {
            hover.on_hover_text(&error);
        }
        let close = Rect::from_min_size(
            Pos2::new(rect.right() - 12.0 - 24.0, rect.top() + 10.0),
            Vec2::splat(24.0),
        );
        if widgets::icon_button(
            ui,
            close,
            "thread-error-dismiss",
            "x",
            14.0,
            theme::muted_foreground(),
            "Dismiss error",
        )
        .clicked()
            && let Some(thread) = self.current_thread()
        {
            self.dismissed_thread_errors
                .insert(format!("{}\0{error}", thread.id));
        }
    }
}
