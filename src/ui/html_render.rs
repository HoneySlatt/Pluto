//! Pages agents publish with `html_render`, inline in the chat
//! (`components/chat/HtmlRenderFrame.tsx`, `HtmlRenderTimelineRow` in
//! `MessagesTimeline.tsx`, `HtmlRenderDocument` in
//! `files/BrowserDocumentFrame.tsx`).
//!
//! Upstream frames the page in a sandboxed iframe. egui has no web view,
//! so the preview browser (`crate::preview`) renders each page off screen
//! and the row draws its frames, the pointer and keys going back to the
//! page like in the Browser panel. The page loads from its signed asset URL
//! with the app theme in its fragment, fits its reported height, and pauses
//! while scrolled out of view; a page unseen for a while gives its tab back.
//!
//! Differences from upstream: the hover buttons open the page in the Design
//! surface (`crate::ui::design`, instead of a full-screen dialog) or in the
//! system browser, links the page opens go to the system browser (like chat
//! links here), app shortcuts are not passed out of a focused page, and the
//! loading skeleton is the generic one (the server's outline is not drawn).

use std::collections::HashMap;

use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::html_render::HtmlRender;
use crate::theme;
use crate::ui::widgets;

/// Seconds a page may stay out of view before its tab closes.
const IDLE_CLOSE: f64 = 120.0;

/// `SKELETON_SECTION_HEIGHT`.
const SKELETON_SECTION: f32 = 264.0;

#[derive(Default)]
pub(crate) struct HtmlRenders {
    pages: HashMap<String, Page>,
}

#[derive(Default)]
struct Page {
    url: Url,
    tab: Option<u64>,
    /// Drawn this frame, and whether its tab is casting.
    drawn: bool,
    shown: bool,
    last_drawn: f64,
}

#[derive(Default, PartialEq)]
enum Url {
    #[default]
    Unrequested,
    Pending,
    /// The signed URL, without the theme fragment.
    Ready(String),
    Failed,
}

/// `htmlRenderFileName`: the title without characters file systems reject.
fn file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if "\\/:*?\"<>|".contains(c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let name: String = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect();
    let name = name.trim();
    format!("{}.html", if name.is_empty() { "Page" } else { name })
}

impl App {
    /// Asks for a page's signed URL the first time it is needed.
    pub(crate) fn request_html_render_url(&mut self, render: &HtmlRender) {
        let id = render.attachment_id.clone();
        let page = self.html_renders.pages.entry(id.clone()).or_default();
        if page.url == Url::Unrequested {
            page.url = Url::Pending;
            self.send_request(
                format!("html-render:{id}"),
                "assets.createUrl",
                json!({"resource": {
                    "_tag": "attachment",
                    "attachmentId": id,
                    "fileName": file_name(&render.title),
                    "mimeType": "text/html",
                    "disposition": "inline",
                }}),
            );
        }
    }

    /// A page's URL with the app theme in its fragment: `Some(Ok)` once
    /// signed, `Some(Err)` when it could not be, `None` while waiting.
    pub(crate) fn html_render_url(&self, id: &str) -> Option<Result<String, ()>> {
        match &self.html_renders.pages.get(id)?.url {
            Url::Ready(url) => Some(Ok(format!("{url}{}", self.html_render_fragment()))),
            Url::Failed => Some(Err(())),
            Url::Unrequested | Url::Pending => None,
        }
    }

    /// `HtmlRenderTimelineRow`: the page `px-1` inside the column.
    pub(crate) fn html_render_row(&mut self, ui: &mut Ui, render: &HtmlRender, width: f32) {
        let frame_width = (width - 8.0).max(1.0);
        let id = render.attachment_id.clone();
        let now = ui.input(|i| i.time);
        self.request_html_render_url(render);
        let state = self.html_render_tab(ui.ctx(), &id);
        let content = state.as_ref().and_then(|s| s.content_height);
        let height = render.frame_height(frame_width, content);
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let area = Rect::from_min_size(
            Pos2::new(row.left() + 4.0, row.top()),
            Vec2::new(frame_width, height),
        );
        if !ui.is_rect_visible(area) {
            return;
        }
        let page = self.html_renders.pages.entry(id.clone()).or_default();
        page.drawn = true;
        page.last_drawn = now;
        let failed = page.url == Url::Failed
            || state.as_ref().is_some_and(|s| s.error.is_some())
            || self
                .preview
                .engine
                .as_ref()
                .is_some_and(|e| e.failure.lock().is_ok_and(|f| f.is_some()));
        if failed {
            let text = format!("Unable to load {}", render.title);
            let text_width =
                widgets::text_width(ui, &text, theme::font(12.0, theme::Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_center_size(area.center(), Vec2::new(text_width + 1.0, 16.0)),
                &text,
                theme::font(12.0, theme::Weight::Regular),
                theme::muted_foreground(),
            );
            return;
        }
        let mut shown = false;
        if let (Some(tab), Some(state)) = (self.html_renders.pages[&id].tab, &state) {
            let ppp = ui.ctx().pixels_per_point();
            if let Some(engine) = &self.preview.engine {
                engine.resize(
                    tab,
                    area.width().floor().max(1.0) as u32,
                    area.height().floor().max(1.0) as u32,
                    ppp,
                );
            }
            // A page taller than its frame scrolls inside it; else the
            // wheel scrolls the chat.
            let scrolls = content.is_some_and(|c| c > height + 1.0);
            // The page keeps the frame's area clipped to the chat.
            let mut page_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(area)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            page_ui.set_clip_rect(area.intersect(ui.clip_rect()));
            // Shown once the page reported its height, like upstream's
            // `ready` on the frame's first size message.
            shown = self.page_view(&mut page_ui, area, tab, state, scrolls)
                && state.content_height.is_some();
            if let Some(engine) = &self.preview.engine {
                for link in engine.take_links(tab) {
                    widgets::open_external(&link);
                }
            }
        }
        if !shown {
            // The frame stays clear until the page is ready.
            ui.painter().rect_filled(area, 0.0, theme::background());
            skeleton(ui, area, now);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
            return;
        }
        // Open in the Design surface or the system browser, shown on hover
        // (`variant="glass"` buttons at `end-2 top-2`).
        if ui.rect_contains_pointer(area) {
            let mut right = area.right() - 8.0;
            for (icon, tip) in [
                ("external-link", "Open in browser"),
                ("maximize-2", "Open in Design"),
            ] {
                let button = Rect::from_min_size(
                    Pos2::new(right - 24.0, area.top() + 8.0),
                    Vec2::splat(24.0),
                );
                right -= 24.0 + 4.0;
                ui.painter().rect(
                    button,
                    CornerRadius::same(6),
                    theme::alpha(theme::popover(), 0.9),
                    Stroke::new(1.0, theme::border()),
                    StrokeKind::Inside,
                );
                let response = widgets::icon_button(
                    ui,
                    button,
                    ("html-render-open", icon, &id),
                    icon,
                    14.0,
                    theme::muted_foreground(),
                    tip,
                );
                if !response.clicked() {
                    continue;
                }
                if icon == "maximize-2" {
                    self.open_design(Some(id.clone()));
                } else if let Some(Ok(url)) = self.html_render_url(&id) {
                    widgets::open_external(&url);
                }
            }
        }
    }

    /// The page's tab state, opening its tab once its URL is known.
    fn html_render_tab(
        &mut self,
        ctx: &egui::Context,
        id: &str,
    ) -> Option<crate::preview::TabState> {
        let page = self.html_renders.pages.get(id)?;
        if let Some(tab) = page.tab {
            return self.preview.engine.as_ref().and_then(|e| e.tab(tab));
        }
        let Url::Ready(url) = &page.url else {
            return None;
        };
        let url = format!("{url}{}", self.html_render_fragment());
        self.preview.next_tab += 1;
        let tab = self.preview.next_tab;
        self.preview_engine(ctx).open_render(tab, url);
        if let Some(page) = self.html_renders.pages.get_mut(id) {
            page.tab = Some(tab);
            page.shown = true;
        }
        None
    }

    /// The app theme for a page (`useHtmlRenderTheme`), as its URL
    /// fragment; empty on the stock theme, which the page already wears.
    fn html_render_fragment(&self) -> String {
        let Some(theme) = self.active_theme() else {
            return String::new();
        };
        let Some(colors) = theme["colors"].as_object() else {
            return String::new();
        };
        let text = |key: &str| self.client_setting(key).as_str().unwrap_or("").to_owned();
        let theme = crate::html_render::theme(
            colors,
            theme["appearance"] != "light",
            &text("fontFamilySans"),
            &text("fontFamilyCode"),
        );
        crate::html_render::theme_fragment(&theme)
    }

    /// A page's signed URL.
    pub(crate) fn html_render_url_result(
        &mut self,
        key: &str,
        result: &Result<Value, String>,
    ) -> bool {
        let Some(id) = key.strip_prefix("html-render:") else {
            return false;
        };
        let base = url::Url::parse(self.connect.server.trim()).ok();
        let url = result.as_ref().ok().and_then(|r| {
            let relative = r["relativeUrl"].as_str()?;
            base?.join(relative).ok()
        });
        if let Some(page) = self.html_renders.pages.get_mut(id) {
            page.url = match url {
                // The fragment is the theme's; the page drops it on load.
                Some(mut url) => {
                    url.set_fragment(None);
                    Url::Ready(url.to_string())
                }
                None => Url::Failed,
            };
        }
        true
    }

    /// Pauses pages scrolled out of view and closes those unseen for a
    /// while; called once a frame after the chat drew.
    pub(crate) fn html_renders_end_frame(&mut self, now: f64) {
        let Some(engine) = &self.preview.engine else {
            for page in self.html_renders.pages.values_mut() {
                page.drawn = false;
            }
            return;
        };
        let mut closed = Vec::new();
        for page in self.html_renders.pages.values_mut() {
            let Some(tab) = page.tab else {
                page.drawn = false;
                continue;
            };
            if page.drawn != page.shown {
                engine.set_visible(tab, page.drawn);
                page.shown = page.drawn;
            }
            if !page.drawn && now - page.last_drawn > IDLE_CLOSE {
                engine.close(tab);
                closed.push(tab);
                page.tab = None;
            }
            page.drawn = false;
        }
        for tab in closed {
            self.preview.textures.remove(&tab);
        }
    }
}

/// `HtmlRenderSkeleton` without an outline: a heading, a line of text and a
/// chart-sized block, repeated to fill the frame, pulsing.
fn skeleton(ui: &Ui, area: Rect, now: f64) {
    // `animate-skeleton`: a 2s pulse.
    let pulse = 0.5 + 0.5 * ((now * std::f64::consts::PI).cos() as f32).abs();
    let color = theme::alpha(theme::foreground(), 0.04 + 0.04 * pulse);
    let painter = ui.painter_at(area);
    let mut y = area.top() + 4.0;
    while y < area.bottom() {
        let bar = |y: f32, fraction: f32, height: f32| {
            Rect::from_min_size(
                Pos2::new(area.left(), y),
                Vec2::new(area.width() * fraction, height),
            )
        };
        painter.rect_filled(bar(y, 0.4, 16.0), CornerRadius::same(4), color);
        painter.rect_filled(bar(y + 24.0, 0.6, 12.0), CornerRadius::same(4), color);
        painter.rect_filled(bar(y + 44.0, 1.0, 160.0), CornerRadius::same(12), color);
        y += SKELETON_SECTION;
    }
}
