//! The thread's error banner (`ThreadErrorBanner.tsx`, placed by
//! `ChatView.tsx`'s banner lane): the runtime's `lastError` at the top of
//! the chat, an error, or a warning for a usage limit, clamped to three
//! lines with the full text in a tooltip, and its Dismiss button. A
//! dismissal lasts the run, for that thread and that message: another
//! error on the same thread shows again.
//!
//! Above it, the selected provider's status (`ProviderStatusBanner.tsx`):
//! unauthenticated, a version known to be broken or unsupported, or limited,
//! with "Open provider setup" when setup can fix it; dismissed until the
//! status recovers and fails again.
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

    /// Draws the banner lane at the top of `body`: the selected provider's
    /// status, then the thread's error.
    pub(crate) fn thread_error_banner(&mut self, ui: &mut Ui, body: Rect) {
        let mut top = body.top() + 12.0;
        if let Some((key, banner)) = self.provider_status_banner() {
            let (height, dismissed, link) = alert(ui, body, top, &banner, "provider-status");
            if dismissed {
                self.dismissed_provider_status = Some(key);
            }
            if link {
                self.open_settings_page(crate::ui::settings::PROVIDERS_PAGE);
            }
            top += height + 8.0;
        }
        let Some((error, usage_limit)) = self.visible_thread_error() else {
            return;
        };
        let banner = Banner {
            warning: usage_limit,
            title: None,
            message: error.clone(),
            link: None,
            dismiss_label: "Dismiss error".into(),
        };
        let (_, dismissed, _) = alert(ui, body, top, &banner, "thread-error");
        if dismissed && let Some(thread) = self.current_thread() {
            self.dismissed_thread_errors
                .insert(format!("{}\0{error}", thread.id));
        }
    }

    /// `ProviderStatusBanner`: the selected provider is unauthenticated,
    /// broken or limited; its key, which a dismissal remembers until the
    /// status recovers.
    fn provider_status_banner(&mut self) -> Option<(String, Banner)> {
        let provider = self
            .current_thread()
            .and(self.composer_provider(false))?
            .clone();
        let key = crate::ui::pickers::provider_status_key(&provider);
        let Some(key) = key else {
            // Recovered: a new failure shows again.
            self.dismissed_provider_status = None;
            return None;
        };
        if self.dismissed_provider_status.as_deref() == Some(key.as_str()) {
            return None;
        }
        let (title, message, warning) = crate::ui::pickers::provider_status_banner(&provider);
        let setup = crate::ui::pickers::has_provider_setup(&provider);
        Some((
            key,
            Banner {
                warning,
                title: Some(title.clone()),
                message,
                link: setup.then(|| "Open provider setup".into()),
                dismiss_label: format!("Dismiss {title}"),
            },
        ))
    }
}

/// An alert in the banner lane (`ui/alert.tsx`, `surface="glass"`).
struct Banner {
    warning: bool,
    title: Option<String>,
    message: String,
    /// An inline link under the message.
    link: Option<String>,
    dismiss_label: String,
}

/// Draws `banner` centered in `body` from `top`; its height, and whether
/// it was dismissed or its link clicked.
fn alert(ui: &mut Ui, body: Rect, top: f32, banner: &Banner, id: &str) -> (f32, bool, bool) {
    // `w-fit`: as wide as its text, up to the max.
    let chrome = 16.0 + 16.0 + 8.0 + 28.0 + 12.0;
    let widest = banner
        .title
        .iter()
        .map(|t| widgets::text_width(ui, t, font(14.0, Weight::Medium)))
        .chain(std::iter::once(widgets::text_width(
            ui,
            &banner.message,
            font(14.0, Weight::Regular),
        )))
        .fold(0.0, f32::max);
    let width = MAX_WIDTH
        .min(body.width() - 32.0)
        .min((widest + chrome).ceil());
    if width < 120.0 {
        return (0.0, false, false);
    }
    let text_width = width - chrome;
    let full = widgets::galley(
        ui,
        &banner.message,
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
            &banner.message,
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
    let title = banner.title.as_ref().map(|t| {
        widgets::galley(
            ui,
            t,
            font(14.0, Weight::Medium),
            theme::foreground(),
            20.0,
            text_width,
        )
    });
    let title_height = title.as_ref().map_or(0.0, |g| g.size().y + 2.0);
    let link_height = if banner.link.is_some() { 30.0 } else { 0.0 };
    let height = title_height + galley.size().y.max(20.0) + link_height + 24.0;
    let rect = Rect::from_min_size(
        Pos2::new((body.center().x - width / 2.0).round(), top),
        Vec2::new(width, height),
    );
    let tone = if banner.warning {
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
        if banner.title.is_some() {
            "info"
        } else {
            "circle-alert"
        },
        Rect::from_min_size(rect.min + Vec2::new(16.0, 14.0), Vec2::splat(16.0)),
        16.0,
        tone,
    );
    let mut y = rect.top() + 12.0;
    let text_left = rect.left() + 16.0 + 16.0 + 8.0;
    if let Some(title) = title {
        let h = title.size().y;
        ui.painter()
            .galley(Pos2::new(text_left, y), title, theme::foreground());
        y += h + 2.0;
    }
    let color = if banner.title.is_some() {
        theme::muted_foreground()
    } else {
        theme::foreground()
    };
    let text_rect = Rect::from_min_size(Pos2::new(text_left, y), galley.size());
    let hover = ui.interact(text_rect, ui.id().with((id, "text")), Sense::hover());
    ui.painter().galley(text_rect.min, galley, color);
    if clamped {
        hover.on_hover_text(&banner.message);
    }
    let mut link = false;
    if let Some(label) = &banner.link {
        let w = widgets::text_width(ui, label, font(14.0, Weight::Medium));
        let link_rect = Rect::from_min_size(
            Pos2::new(text_left, text_rect.bottom() + 10.0),
            Vec2::new(w, 20.0),
        );
        let response = ui
            .interact(link_rect, ui.id().with((id, "link")), Sense::click())
            .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
        widgets::text_in(
            ui,
            link_rect,
            label,
            font(14.0, Weight::Medium),
            if response.hovered() {
                theme::foreground()
            } else {
                theme::muted_foreground()
            },
        );
        link = response.clicked();
    }
    let close = Rect::from_min_size(
        Pos2::new(rect.right() - 12.0 - 24.0, rect.top() + 10.0),
        Vec2::splat(24.0),
    );
    let dismissed = widgets::icon_button(
        ui,
        close,
        (id, "dismiss"),
        "x",
        14.0,
        theme::muted_foreground(),
        &banner.dismiss_label,
    )
    .clicked();
    (height, dismissed, link)
}
