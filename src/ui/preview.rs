//! The right panel's Browser surface (`PreviewView.tsx`,
//! `PreviewChromeRow.tsx`, `PreviewEmptyState.tsx`,
//! `PreviewLocalServerCard.tsx`, the preview tab of `RightPanelTabs.tsx`):
//! the navigation row, the local servers to open, and the page, drawn from
//! the headless preview browser's frames (`crate::preview`) with the
//! pointer and keys sent back to it.
//!
//! Not ported: browser profiles, the more menu (hard reload, devtools,
//! zoom, clearing data), annotations, screenshots and recording, the
//! floating mini player, recent URLs, favicons captured from pages, and the
//! system clipboard inside pages (the headless browser has its own).

use std::collections::HashMap;

use eframe::egui::{
    self, Color32, CornerRadius, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::app::{App, PanelTab};
use crate::preview::{KeyStroke, Pointer, Preview};
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

#[derive(Default)]
pub(crate) struct PreviewUi {
    engine: Option<Preview>,
    next_tab: u64,
    /// Each tab's uploaded frame and the frame counter it shows.
    textures: HashMap<u64, (u64, egui::TextureHandle)>,
    /// The address being typed, per tab.
    drafts: HashMap<u64, String>,
    /// The tab drawn this frame and the last one, for pausing hidden pages.
    drawn: Option<u64>,
    shown: Option<u64>,
    /// The page pointer: its last position, the pressed button and the
    /// last press (time, position, count) for click counts.
    pointer: Option<Pos2>,
    pressed: Option<&'static str>,
    last_press: Option<(f64, Pos2, u32)>,
    /// When each tab started loading (the progress bar).
    loading_since: HashMap<u64, f64>,
}

/// `PreviewChromeRow` in the inline panel: `h-7` with `mb-3` below.
const CHROME: f32 = 28.0;
const CHROME_GAP: f32 = 12.0;

/// `normalizePreviewUrl` (packages/shared/src/preview.ts): bare loopback
/// hosts get `http://`, other bare hosts `https://`; only http(s) URLs
/// open.
fn normalize_preview_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_lowercase();
    // `LOOPBACK_PREFIX_PATTERN`.
    let loopback = ["localhost", "127.0.0.1", "0.0.0.0", "[::1]", "[::]"]
        .iter()
        .any(|host| {
            lower.strip_prefix(host).is_some_and(|rest| {
                rest.is_empty() || rest.starts_with(':') || rest.starts_with('/')
            })
        });
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("{}://{trimmed}", if loopback { "http" } else { "https" })
    };
    let parsed = url::Url::parse(&candidate).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| parsed.to_string())
}

/// The DOM key, code and Windows virtual key of a key the page receives
/// as a key event (characters arrive as text).
fn dom_key(key: Key) -> Option<(&'static str, &'static str, u32)> {
    Some(match key {
        Key::Enter => ("Enter", "Enter", 13),
        Key::Tab => ("Tab", "Tab", 9),
        Key::Backspace => ("Backspace", "Backspace", 8),
        Key::Delete => ("Delete", "Delete", 46),
        Key::Escape => ("Escape", "Escape", 27),
        Key::ArrowLeft => ("ArrowLeft", "ArrowLeft", 37),
        Key::ArrowUp => ("ArrowUp", "ArrowUp", 38),
        Key::ArrowRight => ("ArrowRight", "ArrowRight", 39),
        Key::ArrowDown => ("ArrowDown", "ArrowDown", 40),
        Key::Home => ("Home", "Home", 36),
        Key::End => ("End", "End", 35),
        Key::PageUp => ("PageUp", "PageUp", 33),
        Key::PageDown => ("PageDown", "PageDown", 34),
        Key::Space => (" ", "Space", 32),
        Key::A => ("a", "KeyA", 65),
        Key::C => ("c", "KeyC", 67),
        Key::V => ("v", "KeyV", 86),
        Key::X => ("x", "KeyX", 88),
        Key::Z => ("z", "KeyZ", 90),
        Key::Y => ("y", "KeyY", 89),
        Key::F5 => ("F5", "F5", 116),
        _ => return None,
    })
}

fn modifier_bits(modifiers: egui::Modifiers) -> u32 {
    u32::from(modifiers.alt)
        | (u32::from(modifiers.ctrl) << 1)
        | (u32::from(modifiers.mac_cmd) << 2)
        | (u32::from(modifiers.shift) << 3)
}

impl App {
    fn preview_engine(&mut self, ctx: &egui::Context) -> &Preview {
        self.preview
            .engine
            .get_or_insert_with(|| Preview::start(ctx.clone()))
    }

    /// `addBrowserSurface` / `openDiscoveredPort`: a new Browser tab in the
    /// thread's panel, on `url` or on its empty state.
    pub(crate) fn open_browser_tab(&mut self, ctx: &egui::Context, url: Option<String>) {
        self.preview.next_tab += 1;
        let tab = self.preview.next_tab;
        self.preview_engine(ctx).open(tab, url);
        if let Some(panel) = self.panel_mut() {
            panel.open = true;
            panel.show(PanelTab::Browser { id: tab }, |_| false);
        }
    }

    /// A closed Browser tab's page goes away.
    pub(crate) fn close_browser_tab(&mut self, tab: u64) {
        if let Some(engine) = &self.preview.engine {
            engine.close(tab);
        }
        self.preview.textures.remove(&tab);
        self.preview.drafts.remove(&tab);
    }

    /// A Browser tab's label and whether it has a page (`surfaceTitle`):
    /// the page title, else its host, else "Browser".
    pub(crate) fn browser_tab_title(&self, tab: u64) -> String {
        let Some(state) = self.preview.engine.as_ref().and_then(|e| e.tab(tab)) else {
            return "Browser".into();
        };
        if !state.title.trim().is_empty() {
            return state.title;
        }
        url::Url::parse(&state.url)
            .ok()
            .and_then(|u| {
                u.host_str().map(|h| match u.port() {
                    Some(port) => format!("{h}:{port}"),
                    None => h.to_owned(),
                })
            })
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "Browser".into())
    }

    /// Pauses the page of a Browser tab that was not drawn this frame.
    pub(crate) fn preview_end_frame(&mut self) {
        let drawn = self.preview.drawn.take();
        if drawn != self.preview.shown {
            if let (Some(engine), Some(old)) = (&self.preview.engine, self.preview.shown) {
                engine.set_visible(old, false);
            }
            if let (Some(engine), Some(new)) = (&self.preview.engine, drawn) {
                engine.set_visible(new, true);
            }
            self.preview.shown = drawn;
        }
    }

    /// The Browser surface below the panel's tab bar.
    pub(crate) fn browser_surface(&mut self, ui: &mut Ui, body: Rect, tab: u64) {
        let ctx = ui.ctx().clone();
        self.preview_engine(&ctx);
        self.preview.drawn = Some(tab);
        let failure = self
            .preview
            .engine
            .as_ref()
            .and_then(|e| e.failure.lock().ok().and_then(|f| f.clone()));
        let state = self
            .preview
            .engine
            .as_ref()
            .and_then(|e| e.tab(tab))
            .unwrap_or_default();
        let idle = state.url.is_empty() || state.url == "about:blank";
        let chrome = Rect::from_min_size(
            Pos2::new(body.left() + 8.0, body.top()),
            Vec2::new(body.width() - 16.0, CHROME),
        );
        self.preview_chrome(ui, chrome, tab, &state);
        let page = Rect::from_min_max(
            Pos2::new(body.left(), chrome.bottom() + CHROME_GAP),
            body.max,
        );
        if let Some(error) = failure.or(state.error.clone()) {
            centered_note(ui, page, &error);
            return;
        }
        if idle && !state.loading {
            self.preview_empty_state(ui, page, tab);
            return;
        }
        self.preview_page(ui, page, tab, &state);
    }

    /// Back, forward, refresh, the address field with its hover "Open in
    /// system browser" button, and the loading bar.
    fn preview_chrome(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        tab: u64,
        state: &crate::preview::TabState,
    ) {
        let button = |ui: &mut Ui, x: f32, icon: &str, id: &str, enabled: bool, tooltip: &str| {
            let rect = Rect::from_min_size(Pos2::new(x, rect.center().y - 12.0), Vec2::splat(24.0));
            if enabled {
                widgets::icon_button(
                    ui,
                    rect,
                    (id, tab),
                    icon,
                    14.0,
                    theme::foreground(),
                    tooltip,
                )
                .clicked()
            } else {
                // `disabled:opacity-64`.
                theme::paint_icon(
                    ui,
                    icon,
                    rect,
                    14.0,
                    theme::alpha(theme::foreground(), 0.64 * 0.6),
                );
                false
            }
        };
        let mut x = rect.left();
        let back = button(
            ui,
            x,
            "arrow-left",
            "preview-back",
            state.can_go_back,
            "Back",
        );
        x += 26.0;
        let forward = button(
            ui,
            x,
            "arrow-right",
            "preview-forward",
            state.can_go_forward,
            "Forward",
        );
        x += 26.0;
        let idle = state.url.is_empty() || state.url == "about:blank";
        let refresh = button(
            ui,
            x,
            "refresh-cw",
            "preview-refresh",
            !idle,
            if state.loading {
                "Loading…"
            } else {
                "Refresh"
            },
        );
        x += 24.0 + 4.0;
        if let Some(engine) = &self.preview.engine {
            if back {
                engine.back(tab);
            }
            if forward {
                engine.forward(tab);
            }
            if refresh {
                if state.loading {
                    engine.stop(tab);
                } else {
                    engine.reload(tab);
                }
            }
        }

        // `InputGroup variant="ghost" className="h-7 flex-1"`.
        let field = Rect::from_min_max(Pos2::new(x, rect.top()), rect.max);
        let id = ui.id().with(("preview-address", tab));
        let focused = ui.memory(|m| m.has_focus(id));
        let hovered = ui.rect_contains_pointer(field);
        if focused || hovered {
            ui.painter().rect_filled(
                field,
                CornerRadius::same(8),
                theme::overlay(if focused { 0.06 } else { 0.04 }),
            );
        }
        let shown = if idle {
            String::new()
        } else {
            state.url.clone()
        };
        let mut text = if focused {
            self.preview
                .drafts
                .entry(tab)
                .or_insert_with(|| shown.clone())
                .clone()
        } else {
            self.preview.drafts.remove(&tab);
            shown.clone()
        };
        let submit = focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
        let escape = focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        let external = !focused && !idle;
        let text_rect = Rect::from_min_max(
            Pos2::new(field.left() + 10.0, field.top()),
            Pos2::new(
                field.right() - if external { 32.0 } else { 10.0 },
                field.bottom(),
            ),
        );
        let response = ui.put(
            text_rect,
            egui::TextEdit::singleline(&mut text)
                .id(id)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font(14.0, Weight::Regular))
                .text_color(theme::foreground())
                .hint_text(
                    egui::RichText::new("Search or enter URL")
                        .color(theme::muted_foreground())
                        .font(font(14.0, Weight::Regular)),
                )
                .vertical_align(egui::Align::Center),
        );
        // Focusing selects the whole address.
        if response.gained_focus()
            && let Some(mut edit) = egui::TextEdit::load_state(ui.ctx(), id)
        {
            edit.cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(text.chars().count()),
                )));
            edit.store(ui.ctx(), id);
        }
        if focused || response.has_focus() {
            self.preview.drafts.insert(tab, text.clone());
        }
        if escape {
            self.preview.drafts.remove(&tab);
            ui.memory_mut(|m| m.surrender_focus(id));
        }
        if submit && !text.trim().is_empty() {
            self.preview.drafts.remove(&tab);
            ui.memory_mut(|m| m.surrender_focus(id));
            if let (Some(url), Some(engine)) = (normalize_preview_url(&text), &self.preview.engine)
            {
                engine.navigate(tab, url);
            }
        }
        if external && hovered {
            let open = Rect::from_min_size(
                Pos2::new(field.right() - 26.0, field.center().y - 12.0),
                Vec2::splat(24.0),
            );
            if widgets::icon_button(
                ui,
                open,
                ("preview-external", tab),
                "external-link",
                14.0,
                theme::foreground(),
                "Open in system browser",
            )
            .clicked()
            {
                widgets::open_external(&state.url);
            }
        }

        // `preview-loading-progress`: a 2px primary bar under the row.
        let now = ui.input(|i| i.time);
        if state.loading {
            let since = *self.preview.loading_since.entry(tab).or_insert(now);
            let progress = 0.9 * (1.0 - (-(now - since) / 1.2).exp()) as f32;
            let bar = Rect::from_min_size(
                Pos2::new(rect.left() - 8.0, rect.bottom() - 2.0),
                Vec2::new((rect.width() + 16.0) * progress, 2.0),
            );
            ui.painter()
                .rect_filled(bar, CornerRadius::same(1), theme::primary());
            ui.ctx().request_repaint();
        } else {
            self.preview.loading_since.remove(&tab);
        }
    }

    /// `PreviewEmptyState`: the live local servers to open, or "No preview yet".
    fn preview_empty_state(&mut self, ui: &mut Ui, body: Rect, tab: u64) {
        self.watch_discovered_servers();
        let mut servers: Vec<(String, String, String)> = self
            .discovered_servers
            .iter()
            .filter_map(|server| {
                let url = server["url"].as_str()?.to_owned();
                let host = server["host"].as_str().unwrap_or("localhost");
                let port = server["port"].as_u64()?;
                let title = server["processName"]
                    .as_str()
                    .or(server["terminal"]["processName"].as_str())
                    .filter(|n| !n.is_empty())
                    .unwrap_or("Listening")
                    .to_owned();
                Some((url, title, format!("{host}:{port}")))
            })
            .collect();
        // `mergeServers`: by port.
        servers.sort_by_key(|(url, _, _)| {
            url::Url::parse(url)
                .ok()
                .and_then(|u| u.port_or_known_default())
        });
        servers.dedup_by(|a, b| a.2 == b.2);
        if servers.is_empty() {
            no_preview(ui, body);
            return;
        }
        // `px-5 py-8`, `max-w-xl` centered.
        let width = (body.width() - 40.0).min(576.0);
        let left = body.center().x - width / 2.0;
        let mut y = body.top() + 32.0;
        let header = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 20.0));
        theme::paint_icon(
            ui,
            "radio-tower",
            Rect::from_min_size(Pos2::new(left, y + 2.0), Vec2::splat(16.0)),
            16.0,
            theme::muted_foreground(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(left + 24.0, header.top()), header.max),
            "Local servers",
            font(14.0, Weight::Medium),
            theme::muted_foreground(),
        );
        y += 20.0 + 12.0;
        // `DiscoveryList`: rounded-xl, bordered, rows divided.
        let row_height = 12.0 + 36.0 + 12.0;
        let list = Rect::from_min_size(
            Pos2::new(left, y),
            Vec2::new(width, row_height * servers.len() as f32 + 2.0),
        );
        ui.painter().rect(
            list,
            CornerRadius::same(12),
            theme::background(),
            Stroke::new(1.0, theme::alpha(theme::border(), 0.7)),
            StrokeKind::Inside,
        );
        let mut open = None;
        for (index, (url, title, address)) in servers.iter().enumerate() {
            let row = Rect::from_min_size(
                Pos2::new(left + 1.0, y + 1.0 + index as f32 * row_height),
                Vec2::new(width - 2.0, row_height),
            );
            if index > 0 {
                ui.painter().rect_filled(
                    Rect::from_min_size(row.min, Vec2::new(row.width(), 1.0)),
                    0.0,
                    theme::alpha(theme::border(), 0.6),
                );
            }
            let response = ui.interact(
                row,
                ui.id().with(("preview-server", tab, index)),
                Sense::click(),
            );
            if response.hovered() {
                let rounding = CornerRadius {
                    nw: if index == 0 { 11 } else { 0 },
                    ne: if index == 0 { 11 } else { 0 },
                    sw: if index + 1 == servers.len() { 11 } else { 0 },
                    se: if index + 1 == servers.len() { 11 } else { 0 },
                };
                ui.painter()
                    .rect_filled(row, rounding, theme::alpha(theme::accent(), 0.4));
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            // `BrowserMockup` stands in for the page's favicon.
            let icon = Rect::from_min_size(
                Pos2::new(row.left() + 12.0, row.center().y - 14.0),
                Vec2::splat(28.0),
            );
            browser_mockup(ui, icon);
            let text_left = icon.right() + 12.0;
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(text_left, row.top() + 12.0),
                    Pos2::new(row.right() - 12.0, row.top() + 32.0),
                ),
                title,
                font(14.0, Weight::Medium),
                theme::foreground(),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(text_left, row.top() + 32.0),
                    Pos2::new(row.right() - 12.0, row.top() + 48.0),
                ),
                address,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            );
            if response.clicked() {
                open = Some(url.clone());
            }
        }
        y = list.bottom() + 12.0;
        widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(left + 4.0, y), Vec2::new(width - 8.0, 16.0)),
            "Select a live local server to open it in this browser tab.",
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
        );
        if let (Some(url), Some(engine)) = (open, &self.preview.engine) {
            engine.navigate(tab, url);
        }
    }

    /// The page: its latest frame, sized to the area (the viewport follows
    /// it), with the pointer, wheel and keys going to the page.
    fn preview_page(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        tab: u64,
        state: &crate::preview::TabState,
    ) {
        let ppp = ui.ctx().pixels_per_point();
        let width = area.width().floor().max(1.0) as u32;
        let height = area.height().floor().max(1.0) as u32;
        let Some(engine) = &self.preview.engine else {
            return;
        };
        engine.resize(tab, width, height, ppp);
        ui.painter().rect_filled(area, 0.0, Color32::WHITE);
        if let Some(frame) = &state.frame {
            let entry = self.preview.textures.get(&tab).map(|(seq, _)| *seq);
            if entry != Some(state.frame_seq) {
                let image = (**frame).clone();
                match self.preview.textures.get_mut(&tab) {
                    Some((seq, texture)) => {
                        texture.set(image, egui::TextureOptions::LINEAR);
                        *seq = state.frame_seq;
                    }
                    None => {
                        let texture = ui.ctx().load_texture(
                            format!("preview-{tab}"),
                            image,
                            egui::TextureOptions::LINEAR,
                        );
                        self.preview
                            .textures
                            .insert(tab, (state.frame_seq, texture));
                    }
                }
            }
            if let Some((_, texture)) = self.preview.textures.get(&tab) {
                let size = Vec2::new(frame.size[0] as f32, frame.size[1] as f32) / ppp;
                ui.painter().image(
                    texture.id(),
                    Rect::from_min_size(area.min, size),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }

        // Input.
        let id = ui.id().with(("preview-page", tab));
        let response = ui.interact(area, id, Sense::click_and_drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        let focused = ui.memory(|m| m.has_focus(id));
        if focused {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
        }
        let now = ui.input(|i| i.time);
        let events = ui.input(|i| i.events.clone());
        let local = |pos: Pos2| (pos.x - area.left(), pos.y - area.top());
        let engine = self.preview.engine.as_ref().expect("preview engine");
        for event in &events {
            match event {
                egui::Event::PointerMoved(pos)
                    if area.contains(*pos) || self.preview.pressed.is_some() =>
                {
                    if self.preview.pointer != Some(*pos) {
                        self.preview.pointer = Some(*pos);
                        let (x, y) = local(*pos);
                        engine.pointer(tab, Pointer::Move { x, y }, 0);
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } => {
                    let name = match button {
                        egui::PointerButton::Primary => "left",
                        egui::PointerButton::Secondary => "right",
                        egui::PointerButton::Middle => "middle",
                        _ => continue,
                    };
                    let (x, y) = local(*pos);
                    let bits = modifier_bits(*modifiers);
                    if *pressed && area.contains(*pos) {
                        let clicks = match self.preview.last_press {
                            Some((at, last, count))
                                if now - at < 0.5 && last.distance(*pos) < 5.0 =>
                            {
                                count + 1
                            }
                            _ => 1,
                        };
                        self.preview.last_press = Some((now, *pos, clicks));
                        self.preview.pressed = Some(name);
                        engine.pointer(
                            tab,
                            Pointer::Press {
                                x,
                                y,
                                button: name,
                                clicks,
                            },
                            bits,
                        );
                    } else if !*pressed && self.preview.pressed == Some(name) {
                        let clicks = self.preview.last_press.map_or(1, |(_, _, c)| c);
                        self.preview.pressed = None;
                        engine.pointer(
                            tab,
                            Pointer::Release {
                                x,
                                y,
                                button: name,
                                clicks,
                            },
                            bits,
                        );
                    }
                }
                egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } if response.hovered() => {
                    let scale = match unit {
                        egui::MouseWheelUnit::Point => 1.0,
                        egui::MouseWheelUnit::Line => 40.0,
                        egui::MouseWheelUnit::Page => area.height(),
                    };
                    let (x, y) = local(self.preview.pointer.unwrap_or(area.center()));
                    engine.pointer(
                        tab,
                        Pointer::Wheel {
                            x,
                            y,
                            dx: -delta.x * scale,
                            dy: -delta.y * scale,
                        },
                        modifier_bits(*modifiers),
                    );
                }
                egui::Event::Text(text) if focused => engine.text(tab, text.clone()),
                egui::Event::Paste(text) if focused => engine.text(tab, text.clone()),
                egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    ..
                } if focused => {
                    // Characters arrive as text; shortcuts and special keys
                    // as key events.
                    let Some((dom, code, virtual_key)) = dom_key(*key) else {
                        continue;
                    };
                    let character = dom.chars().count() == 1;
                    if character && !(modifiers.ctrl || modifiers.alt || modifiers.mac_cmd) {
                        continue;
                    }
                    engine.key(
                        tab,
                        KeyStroke {
                            key: dom.to_owned(),
                            code: code.to_owned(),
                            virtual_key,
                            text: (*key == Key::Enter).then(|| "\r".to_owned()),
                            pressed: *pressed,
                            modifiers: modifier_bits(*modifiers),
                        },
                    );
                }
                _ => {}
            }
        }
        // The page scrolls, not the panel.
        if response.hovered() {
            ui.input_mut(|i| {
                i.smooth_scroll_delta = Vec2::ZERO;
            });
        }
    }
}

/// A small browser window glyph (`BrowserMockup`): a rounded frame with
/// its title bar dots.
fn browser_mockup(ui: &Ui, rect: Rect) {
    let frame = rect.shrink2(Vec2::new(1.0, 4.0));
    ui.painter().rect(
        frame,
        CornerRadius::same(4),
        theme::alpha(theme::muted(), 0.6),
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );
    ui.painter().rect_filled(
        Rect::from_min_size(
            frame.min + Vec2::new(0.0, 5.0),
            Vec2::new(frame.width(), 1.0),
        ),
        0.0,
        theme::border(),
    );
    for i in 0..3 {
        ui.painter().circle_filled(
            Pos2::new(frame.left() + 3.5 + i as f32 * 3.0, frame.top() + 2.75),
            0.9,
            theme::muted_foreground(),
        );
    }
}

/// `Empty` with the globe, "No preview yet" and its description.
fn no_preview(ui: &Ui, body: Rect) {
    let center = body.center();
    let media = Rect::from_center_size(Pos2::new(center.x, center.y - 56.0), Vec2::splat(36.0));
    ui.painter().rect(
        media,
        CornerRadius::same(8),
        theme::card(),
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );
    theme::paint_icon(ui, "globe", media, 18.0, theme::muted_foreground());
    let title = "No preview yet";
    let title_width = widgets::text_width(ui, title, font(20.0, Weight::SemiBold));
    widgets::text_in(
        ui,
        Rect::from_min_size(
            Pos2::new(center.x - title_width / 2.0, media.bottom() + 24.0),
            Vec2::new(title_width + 1.0, 28.0),
        ),
        title,
        font(20.0, Weight::SemiBold),
        theme::foreground(),
    );
    let mut job = egui::text::LayoutJob::single_section(
        "Type a URL above, or run a dev script. Browser-ready localhost servers will show up here automatically.".to_owned(),
        widgets::format(font(14.0, Weight::Regular), theme::muted_foreground(), 20.0),
    );
    job.wrap.max_width = 384.0f32.min(body.width() - 48.0);
    job.halign = egui::Align::Center;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(
        Pos2::new(center.x, media.bottom() + 24.0 + 28.0 + 4.0),
        galley,
        theme::muted_foreground(),
    );
}

fn centered_note(ui: &Ui, area: Rect, text: &str) {
    let width = widgets::text_width(ui, text, font(14.0, Weight::Regular));
    widgets::text_in(
        ui,
        Rect::from_center_size(area.center(), Vec2::new(width + 1.0, 20.0)),
        text,
        font(14.0, Weight::Regular),
        theme::muted_foreground(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_normalize_like_upstream() {
        let n = |s: &str| normalize_preview_url(s);
        assert_eq!(
            n("localhost:5173").as_deref(),
            Some("http://localhost:5173/")
        );
        assert_eq!(
            n("example.com/docs").as_deref(),
            Some("https://example.com/docs")
        );
        assert_eq!(n("https://a.b").as_deref(), Some("https://a.b/"));
        assert_eq!(n("file:///etc/passwd"), None);
        assert_eq!(n("  "), None);
    }
}
