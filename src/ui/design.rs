//! Pluto's Design surface: a right-panel tab for the pages the thread's
//! agents publish with `html_render` (`crate::ui::html_render`), in the
//! spirit of claude.ai's artifacts. Upstream has no such surface; it opens a
//! page full screen (`HtmlRenderDialog`). The panel's maximize control
//! (`RightPanelMaximizeControl`) gives the surface the chat's width too.
//!
//! Pages with the same title are versions of one design. The surface
//! follows the newest page until one is pinned (from a chat page's "Open in
//! Design" button, the version arrows or the design menu). The page fills
//! the surface at full width or at a tablet or phone width, scrolls inside
//! itself and takes the pointer and keys, through one preview-browser tab
//! that navigates as the shown page changes.

use std::collections::HashMap;

use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::app::{App, PanelTab};
use crate::html_render::HtmlRender;
use crate::theme::{self, Weight, font};
use crate::ui::widgets::{self, MenuEntry, MenuItem};

/// `PreviewChromeRow`'s height and the gap below it, as in the Browser.
const CHROME: f32 = 28.0;
const CHROME_GAP: f32 = 12.0;
const MENU: &str = "design-pages";

/// The width the page is laid out at.
#[derive(Clone, Copy, Default, PartialEq)]
enum Width {
    #[default]
    Full,
    Tablet,
    Phone,
}

impl Width {
    const ALL: [(Width, &'static str, &'static str); 3] = [
        (Width::Full, "monitor", "Full width"),
        (Width::Tablet, "tablet", "Tablet (768px)"),
        (Width::Phone, "smartphone", "Phone (390px)"),
    ];

    fn css(self) -> Option<f32> {
        match self {
            Width::Full => None,
            Width::Tablet => Some(768.0),
            Width::Phone => Some(390.0),
        }
    }
}

#[derive(Default)]
pub(crate) struct DesignState {
    /// The page each thread pins, by attachment id; none follows the newest.
    pinned: HashMap<String, String>,
    width: Width,
    /// The surface's preview-browser tab and the page it loaded.
    tab: Option<u64>,
    loaded: Option<String>,
    /// Drawn this frame, and whether the tab is casting.
    drawn: bool,
    shown: bool,
}

impl App {
    /// The thread's published pages, oldest first.
    pub(crate) fn thread_pages(&self) -> Vec<HtmlRender> {
        let Some(detail) = self.current_detail() else {
            return Vec::new();
        };
        detail
            .visible
            .iter()
            .filter_map(|row| crate::html_render::from_tool_item(&row.item))
            .collect()
    }

    /// Opens the Design surface, pinned to `page` or following the newest.
    pub(crate) fn open_design(&mut self, page: Option<String>) {
        if let Some(thread) = self.selected.clone() {
            match page {
                Some(page) => self.design.pinned.insert(thread, page),
                None => self.design.pinned.remove(&thread),
            };
        }
        if let Some(panel) = self.panel_mut() {
            panel.open = true;
            panel.show(PanelTab::Design, |t| *t == PanelTab::Design);
        }
    }

    /// The Design surface below the panel's tab bar.
    pub(crate) fn design_surface(&mut self, ui: &mut Ui, body: Rect) {
        let pages = self.thread_pages();
        let thread = self.selected.clone().unwrap_or_default();
        let pinned = self.design.pinned.get(&thread).cloned();
        let current = pinned
            .as_ref()
            .and_then(|id| pages.iter().position(|p| &p.attachment_id == id))
            .or_else(|| pages.len().checked_sub(1));
        let Some(current) = current else {
            empty_state(ui, body);
            return;
        };
        let page = pages[current].clone();
        // The design's versions: its pages with the same title.
        let versions: Vec<usize> = (0..pages.len())
            .filter(|&i| pages[i].title == page.title)
            .collect();
        let version = versions.iter().position(|&i| i == current).unwrap_or(0);

        let chrome = Rect::from_min_size(
            Pos2::new(body.left() + 8.0, body.top()),
            Vec2::new(body.width() - 16.0, CHROME),
        );
        let mut select: Option<Option<String>> = None;
        let mut x = chrome.left();
        for (icon, tip, target) in [
            (
                "chevron-left",
                "Previous version",
                version.checked_sub(1).map(|v| versions[v]),
            ),
            (
                "chevron-right",
                "Next version",
                versions.get(version + 1).copied(),
            ),
        ] {
            let button = Rect::from_min_size(Pos2::new(x, chrome.top()), Vec2::splat(CHROME));
            x += CHROME;
            match target {
                Some(target) => {
                    if widgets::icon_button(
                        ui,
                        button,
                        ("design-version", icon),
                        icon,
                        16.0,
                        theme::muted_foreground(),
                        tip,
                    )
                    .clicked()
                    {
                        select = Some(Some(pages[target].attachment_id.clone()));
                    }
                }
                None => theme::paint_icon(
                    ui,
                    icon,
                    button,
                    16.0,
                    theme::alpha(theme::muted_foreground(), 0.4),
                ),
            }
        }

        // Right side: widths, pin, reload, open in the browser.
        let mut right = chrome.right();
        // The next button leftward, 2px apart.
        let button = |right: &mut f32| {
            *right -= CHROME;
            let rect = Rect::from_min_size(Pos2::new(*right, chrome.top()), Vec2::splat(CHROME));
            *right -= 2.0;
            rect
        };
        let open = button(&mut right);
        let reload = button(&mut right);
        let pin = button(&mut right);
        right -= 6.0;
        let width_buttons: Vec<(Width, &str, &str, Rect)> = Width::ALL
            .iter()
            .rev()
            .map(|(width, icon, tip)| (*width, *icon, *tip, button(&mut right)))
            .collect();
        let url = self.html_render_url(&page.attachment_id);
        if widgets::icon_button(
            ui,
            open,
            "design-open",
            "external-link",
            16.0,
            theme::muted_foreground(),
            "Open in browser",
        )
        .clicked()
            && let Some(Ok(url)) = &url
        {
            widgets::open_external(url);
        }
        if widgets::icon_button(
            ui,
            reload,
            "design-reload",
            "refresh-cw",
            16.0,
            theme::muted_foreground(),
            "Reload",
        )
        .clicked()
            && let (Some(engine), Some(tab)) = (&self.preview.engine, self.design.tab)
        {
            engine.reload(tab);
        }
        if pinned.is_some() {
            ui.painter()
                .rect_filled(pin, CornerRadius::same(8), theme::accent());
        }
        let pin_tip = if pinned.is_some() {
            "Pinned: follow the newest page"
        } else {
            "Pin this page"
        };
        if widgets::icon_button(
            ui,
            pin,
            "design-pin",
            "pin",
            16.0,
            if pinned.is_some() {
                theme::foreground()
            } else {
                theme::muted_foreground()
            },
            pin_tip,
        )
        .clicked()
        {
            select = Some(match pinned {
                Some(_) => None,
                None => Some(page.attachment_id.clone()),
            });
        }
        for (width, icon, tip, rect) in width_buttons {
            let active = self.design.width == width;
            if active {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), theme::accent());
            }
            if widgets::icon_button(
                ui,
                rect,
                ("design-width", icon),
                icon,
                16.0,
                if active {
                    theme::foreground()
                } else {
                    theme::muted_foreground()
                },
                tip,
            )
            .clicked()
            {
                self.design.width = width;
            }
        }

        // The title opens the menu of the thread's designs.
        let title_rect = Rect::from_min_max(
            Pos2::new(x + 6.0, chrome.top()),
            Pos2::new(right - 6.0, chrome.bottom()),
        );
        let designs: Vec<usize> = (0..pages.len())
            .rev()
            .filter(|&i| !pages[i + 1..].iter().any(|p| p.title == pages[i].title))
            .collect();
        let title_response = ui.interact(title_rect, ui.id().with("design-title"), Sense::click());
        let title_width = widgets::text_in(
            ui,
            title_rect,
            &page.title,
            font(13.0, Weight::Medium),
            theme::foreground(),
        );
        let mut after = title_rect.left() + title_width + 6.0;
        if versions.len() > 1 {
            let label = format!("v{} of {}", version + 1, versions.len());
            after += widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(after, title_rect.top()), title_rect.max),
                &label,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            ) + 4.0;
        }
        if designs.len() > 1 {
            theme::paint_icon(
                ui,
                "chevron-down",
                Rect::from_min_size(
                    Pos2::new(after, title_rect.center().y - 7.0),
                    Vec2::splat(14.0),
                ),
                14.0,
                theme::muted_foreground(),
            );
            if title_response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if title_response.clicked() {
                self.open_menu = Some(MENU.into());
            }
            let entries: Vec<MenuEntry> = designs
                .iter()
                .map(|&i| {
                    MenuEntry::Item(MenuItem {
                        icon: Some(if pages[i].title == page.title {
                            "check"
                        } else {
                            "palette"
                        }),
                        icon_size: 16.0,
                        ..MenuItem::new(&pages[i].title)
                    })
                })
                .collect();
            if let Some(index) =
                widgets::menu(ui, &mut self.open_menu, MENU, title_rect, false, &entries)
            {
                select = Some(Some(pages[designs[index]].attachment_id.clone()));
            }
        }

        if let Some(choice) = select {
            match choice {
                Some(id) => self.design.pinned.insert(thread.clone(), id),
                None => self.design.pinned.remove(&thread),
            };
        }

        // The page.
        let area = Rect::from_min_max(
            Pos2::new(body.left(), chrome.bottom() + CHROME_GAP),
            body.max,
        );
        let width = self
            .design
            .width
            .css()
            .map_or(area.width(), |w| w.min(area.width()));
        let frame = Rect::from_min_size(
            Pos2::new((area.center().x - width / 2.0).round(), area.top()),
            Vec2::new(width.round(), area.height()),
        );
        if self.design.width != Width::Full {
            ui.painter().rect_filled(area, 0.0, theme::overlay(0.02));
            ui.painter().rect_stroke(
                frame.expand(1.0),
                0.0,
                Stroke::new(1.0, theme::border()),
                StrokeKind::Inside,
            );
        }
        ui.painter().rect_filled(frame, 0.0, theme::background());
        self.request_html_render_url(&page);
        let note = |ui: &Ui, text: &str| {
            let text_width = widgets::text_width(ui, text, font(13.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_center_size(frame.center(), Vec2::new(text_width + 1.0, 18.0)),
                text,
                font(13.0, Weight::Regular),
                theme::muted_foreground(),
            );
        };
        let url = match self.html_render_url(&page.attachment_id) {
            Some(Ok(url)) => url,
            Some(Err(())) => {
                note(ui, &format!("Unable to load {}", page.title));
                return;
            }
            None => {
                note(ui, "Loading…");
                return;
            }
        };
        let ctx = ui.ctx().clone();
        let failure = self
            .preview_engine(&ctx)
            .failure
            .lock()
            .ok()
            .and_then(|f| f.clone());
        if let Some(failure) = failure {
            note(ui, &failure);
            return;
        }
        let (tab, fresh) = match self.design.tab {
            Some(tab) => (tab, false),
            None => {
                self.preview.next_tab += 1;
                let tab = self.preview.next_tab;
                self.design.tab = Some(tab);
                self.design.shown = true;
                (tab, true)
            }
        };
        let navigate = self.design.loaded.as_deref() != Some(page.attachment_id.as_str());
        self.design.loaded = Some(page.attachment_id.clone());
        self.design.drawn = true;
        let Some(engine) = &self.preview.engine else {
            return;
        };
        if fresh {
            engine.open_render(tab, url);
        } else if navigate {
            engine.navigate(tab, url);
        }
        engine.resize(
            tab,
            frame.width().floor().max(1.0) as u32,
            frame.height().floor().max(1.0) as u32,
            ui.ctx().pixels_per_point(),
        );
        for link in engine.take_links(tab) {
            widgets::open_external(&link);
        }
        let state = engine.tab(tab).unwrap_or_default();
        if let Some(error) = &state.error {
            note(ui, error);
            return;
        }
        let mut page_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(frame)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        page_ui.set_clip_rect(frame.intersect(ui.clip_rect()));
        if !self.page_view(&mut page_ui, frame, tab, &state, true) || state.loading {
            if state.frame.is_none() {
                note(ui, "Loading…");
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    /// Pauses the surface's page while it is not shown.
    pub(crate) fn design_end_frame(&mut self) {
        let drawn = std::mem::take(&mut self.design.drawn);
        if let (Some(engine), Some(tab)) = (&self.preview.engine, self.design.tab)
            && drawn != self.design.shown
        {
            engine.set_visible(tab, drawn);
            self.design.shown = drawn;
        }
    }
}

/// No page yet: the palette, a title and what shows here.
fn empty_state(ui: &Ui, body: Rect) {
    let center = body.center();
    let media = Rect::from_center_size(Pos2::new(center.x, center.y - 48.0), Vec2::splat(36.0));
    ui.painter().rect(
        media,
        CornerRadius::same(8),
        theme::card(),
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );
    theme::paint_icon(ui, "palette", media, 18.0, theme::muted_foreground());
    let title = "No designs yet";
    let title_width = widgets::text_width(ui, title, font(16.0, Weight::SemiBold));
    widgets::text_in(
        ui,
        Rect::from_min_size(
            Pos2::new(center.x - title_width / 2.0, media.bottom() + 20.0),
            Vec2::new(title_width + 1.0, 24.0),
        ),
        title,
        font(16.0, Weight::SemiBold),
        theme::foreground(),
    );
    let mut job = egui::text::LayoutJob::single_section(
        "Pages the agent publishes with html_render show here, newest first. Ask it for a mockup, a chart or a page to iterate on.".to_owned(),
        widgets::format(font(13.0, Weight::Regular), theme::muted_foreground(), 20.0),
    );
    job.wrap.max_width = 340.0f32.min(body.width() - 48.0);
    job.halign = egui::Align::Center;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(
        Pos2::new(center.x, media.bottom() + 20.0 + 24.0 + 4.0),
        galley,
        theme::muted_foreground(),
    );
}
