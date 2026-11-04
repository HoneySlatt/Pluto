//! The server's browser tabs in the Browser surface (`usePreviewSession.ts`,
//! `previewStateStore.ts`, `openPreviewSession.ts`, `closePreviewSession.ts`,
//! `ServerBrowserSurface.tsx`, the reveal handling of `ChatView.tsx`): with
//! the `serverBrowser` capability, Browser tabs run on the server
//! (`preview.open` with `runtime: "server"`) so agents see and drive them,
//! and tabs an agent opens show up when it reveals them. `preview.list` and
//! `subscribePreviewEvents` keep them in step (a new `serverEpoch` lists
//! again, older revisions are dropped).
//!
//! Not ported: file choosers, downloads, the floating mini player (a
//! revealed tab opens in the panel), profiles.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::{App, PanelTab};
use crate::preview::TabState;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const EVENTS: &str = "preview-events";
const LIST: &str = "preview-list:";
const OPEN: &str = "preview-open:";

impl App {
    /// Browser tabs run on the server.
    pub(crate) fn server_browser_enabled(&self) -> bool {
        self.connected() && self.capability("serverBrowser")
    }

    /// A Browser tab's state, local or on the server.
    pub(crate) fn browser_state(&self, tab: u64) -> Option<TabState> {
        if self.server_browser.owns(tab) {
            return self.server_browser.tab(tab);
        }
        self.preview.engine.as_ref().and_then(|e| e.tab(tab))
    }

    pub(crate) fn browser_navigate(&self, tab: u64, url: String) {
        if self.server_browser.owns(tab) {
            self.server_browser.navigate(tab, url);
        } else if let Some(engine) = &self.preview.engine {
            engine.navigate(tab, url);
        }
    }

    pub(crate) fn attach_server_tab(&mut self, tab: u64, ctx: &egui::Context) {
        let server = self.connect.server.clone();
        self.server_browser
            .attach(tab, &server, &self.connect.token, ctx);
    }

    /// The events stream, and the open thread's tabs once.
    pub(crate) fn sync_server_previews(&mut self) {
        if !self.server_browser_enabled() {
            return;
        }
        if !self.streams.iter().any(|(key, ..)| key == EVENTS) {
            self.send(crate::protocol::Action::Stream {
                key: EVENTS.into(),
                method: "subscribePreviewEvents",
                payload: json!({}),
            });
        }
        if let Some(thread) = self.selected.clone()
            && self.server_previews_listed.insert(thread.clone())
        {
            self.send_request(
                format!("{LIST}{thread}"),
                "preview.list",
                json!({"threadId": thread}),
            );
        }
    }

    /// `openPreviewSession`: a server tab on `url`, shown once it exists.
    /// False without a thread to open it in.
    pub(crate) fn open_server_browser_tab(&mut self, url: Option<String>) -> bool {
        let Some(thread) = self.current_thread().map(|t| t.id.clone()) else {
            return false;
        };
        let mut input = json!({"threadId": thread, "runtime": "server"});
        if let Some(url) = url {
            input["url"] = json!(url);
        }
        self.send_request(format!("{OPEN}{thread}"), "preview.open", input);
        true
    }

    /// Server tabs saved with the panels: placeholders until the thread's
    /// list says what they show (or that they are gone).
    pub(crate) fn restore_server_tabs(&mut self, saved: &Value) {
        for (thread, panel) in saved.as_object().into_iter().flatten() {
            for tab in panel["tabs"].as_array().into_iter().flatten() {
                if tab["kind"] != "server-browser" {
                    continue;
                }
                let snapshot = json!({"threadId": thread, "tabId": tab["tabId"], "navStatus": {"_tag": "Idle"}});
                if let Some(id) = self
                    .server_browser
                    .upsert(&snapshot, &mut self.preview.next_tab)
                {
                    let panel = self.panels.entry(thread.clone()).or_default();
                    panel.tabs.push(PanelTab::Browser { id });
                }
            }
        }
    }

    /// `closePreviewSession`.
    pub(crate) fn close_server_browser_tab(&mut self, tab: u64) {
        if let Some(closed) = self.server_browser.remove(tab) {
            self.send_request(
                format!("preview-close:{}", closed.tab_id),
                "preview.close",
                json!({"threadId": closed.thread, "tabId": closed.tab_id}),
            );
        }
        self.preview.textures.remove(&tab);
    }

    /// Shows a server tab in its thread's panel.
    fn show_server_tab(&mut self, thread: &str, id: u64) {
        let panel = self.panels.entry(thread.to_owned()).or_default();
        panel.show(PanelTab::Browser { id }, |t| *t == PanelTab::Browser { id });
        panel.open = true;
    }

    /// A reveal the agent asked for and not handled yet.
    fn reveal(&mut self, snapshot: &Value, id: u64) {
        let Some(request) = snapshot["revealRequest"]["id"].as_str() else {
            return;
        };
        if snapshot["reveal"] != true || !self.server_browser.revealed.insert(request.to_owned()) {
            return;
        }
        if let Some(thread) = snapshot["threadId"].as_str() {
            self.show_server_tab(thread, id);
        }
    }

    /// Drops a tab the server no longer has, and its panel tab.
    fn forget_server_tab(&mut self, id: u64) {
        if let Some(tab) = self.server_browser.remove(id)
            && let Some(panel) = self.panels.get_mut(&tab.thread)
            && let Some(index) = panel
                .tabs
                .iter()
                .position(|t| *t == PanelTab::Browser { id })
        {
            panel.close(index);
        }
        self.preview.textures.remove(&id);
    }

    /// `preview.list`, `preview.open` and the events; false for other keys.
    pub(crate) fn server_preview_result(
        &mut self,
        key: &str,
        result: &Result<Value, String>,
    ) -> bool {
        if let Some(thread) = key.strip_prefix(LIST) {
            let Ok(value) = result else {
                self.server_previews_listed.remove(thread);
                return true;
            };
            self.server_browser.epoch = value["serverEpoch"].as_str().map(str::to_owned);
            self.server_browser.revision = self
                .server_browser
                .revision
                .max(value["revision"].as_u64().unwrap_or(0));
            let sessions: Vec<Value> = value["sessions"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|s| s["runtime"] == "server")
                .collect();
            let mut kept = Vec::new();
            for session in &sessions {
                // Reveals from before this client looked are history.
                if let Some(request) = session["revealRequest"]["id"].as_str() {
                    self.server_browser.revealed.insert(request.to_owned());
                }
                if let Some(id) = self
                    .server_browser
                    .upsert(session, &mut self.preview.next_tab)
                {
                    kept.push(id);
                }
            }
            let gone: Vec<u64> = self
                .server_browser
                .tabs
                .iter()
                .filter(|(id, t)| t.thread == thread && !kept.contains(id))
                .map(|(id, _)| *id)
                .collect();
            for id in gone {
                self.forget_server_tab(id);
            }
            return true;
        }
        if let Some(thread) = key.strip_prefix(OPEN) {
            match result {
                Ok(snapshot) => {
                    let thread = thread.to_owned();
                    if let Some(id) = self
                        .server_browser
                        .upsert(snapshot, &mut self.preview.next_tab)
                    {
                        self.show_server_tab(&thread, id);
                    }
                }
                Err(error) => self.notify(crate::ui::toasts::Kind::Error, error.clone()),
            }
            return true;
        }
        if key.starts_with("preview-close:") {
            return true;
        }
        if key != EVENTS {
            return false;
        }
        let Ok(event) = result else {
            return true;
        };
        // `applyPreviewServerEvent`.
        let epoch = event["serverEpoch"].as_str();
        if epoch.is_some() && epoch != self.server_browser.epoch.as_deref() {
            // A restarted server: its tabs are new, list again.
            self.server_browser.epoch = epoch.map(str::to_owned);
            self.server_previews_listed.clear();
            let all: Vec<u64> = self.server_browser.tabs.keys().copied().collect();
            for id in all {
                self.forget_server_tab(id);
            }
            return true;
        }
        let revision = event["revision"].as_u64().unwrap_or(0);
        if revision <= self.server_browser.revision {
            return true;
        }
        self.server_browser.revision = revision;
        let (Some(thread), Some(tab_id)) = (event["threadId"].as_str(), event["tabId"].as_str())
        else {
            return true;
        };
        match event["type"].as_str() {
            Some("opened" | "navigated" | "resized") => {
                let snapshot = &event["snapshot"];
                if snapshot["runtime"] != "server" {
                    return true;
                }
                if let Some(id) = self
                    .server_browser
                    .upsert(snapshot, &mut self.preview.next_tab)
                {
                    self.reveal(snapshot, id);
                }
            }
            Some("failed") => {
                if let Some(id) = self.server_browser.id_of(thread, tab_id)
                    && let Some(tab) = self.server_browser.tabs.get_mut(&id)
                {
                    tab.snapshot["navStatus"] = json!({
                        "_tag": "LoadFailed", "url": event["url"], "title": event["title"],
                        "code": event["code"], "description": event["description"],
                    });
                }
            }
            Some("closed") => {
                if let Some(id) = self.server_browser.id_of(thread, tab_id) {
                    self.forget_server_tab(id);
                }
            }
            _ => {}
        }
        true
    }

    /// Over a server page: who has control with Take or Release control,
    /// the agent's pointer, a page dialog, and a popup to switch to.
    pub(crate) fn server_tab_overlay(&mut self, ui: &mut Ui, page: Rect, tab: u64) {
        if let Some(popup) = self.server_browser.take_popup(tab)
            && let Some(thread) = self.server_browser.tabs.get(&tab).map(|t| t.thread.clone())
        {
            let snapshot =
                json!({"threadId": thread, "tabId": popup, "navStatus": {"_tag": "Idle"}});
            if let Some(id) = self
                .server_browser
                .upsert(&snapshot, &mut self.preview.next_tab)
            {
                self.show_server_tab(&thread, id);
            }
        }
        if let Some((x, y)) = self.server_browser.agent_pointer(tab) {
            let at = page.min + Vec2::new(x, y);
            ui.painter().circle(
                at,
                5.0,
                theme::alpha(theme::primary(), 0.8),
                Stroke::new(1.5, Color32::WHITE),
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
        let control = self.server_browser.control(tab);
        // `previewStreamControlLabel`.
        let (label, can_take, mine) = match &control {
            None => ("Connecting...", false, false),
            Some(c) if c["canOperate"] != true => ("Read-only", false, false),
            Some(c) => match c["controller"].as_str() {
                Some("you") => ("You have control", false, true),
                Some("agent") => ("Agent has control", true, false),
                Some("another-viewer") => ("Another viewer has control", false, false),
                _ => ("Watching", true, false),
            },
        };
        let small = font(12.0, Weight::Medium);
        let button_label = if mine {
            "Release control"
        } else {
            "Take control"
        };
        let show_button = mine || can_take;
        let label_width = widgets::text_width(ui, label, small.clone());
        let button_width = widgets::text_width(ui, button_label, small.clone()) + 16.0;
        let width = 12.0 + label_width + if show_button { 8.0 + button_width } else { 0.0 } + 4.0;
        let bar = Rect::from_min_size(
            Pos2::new(page.right() - 8.0 - width, page.bottom() - 8.0 - 28.0),
            Vec2::new(width, 28.0),
        );
        ui.painter().rect(
            bar,
            CornerRadius::same(8),
            theme::popover(),
            Stroke::new(1.0, theme::border()),
            StrokeKind::Inside,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(bar.min + Vec2::new(12.0, 0.0), bar.max),
            label,
            small.clone(),
            theme::muted_foreground(),
        );
        if show_button {
            let button = Rect::from_min_size(
                Pos2::new(bar.right() - 4.0 - button_width, bar.top() + 3.0),
                Vec2::new(button_width, 22.0),
            );
            if widgets::outline_button(
                ui,
                button,
                ("server-control", tab),
                None,
                button_label,
                CornerRadius::same(6),
            )
            .clicked()
            {
                self.server_browser.take_control(tab, !mine);
            }
        }
        // The page's dialog (`alert`, `confirm`, `prompt`).
        let Some(dialog) = control
            .as_ref()
            .map(|c| c["dialog"].clone())
            .filter(|d| d.is_object())
        else {
            return;
        };
        let card = Rect::from_center_size(
            page.center(),
            Vec2::new(360.0_f32.min(page.width() - 32.0), 140.0),
        );
        ui.painter().rect(
            card,
            CornerRadius::same(12),
            theme::popover(),
            Stroke::new(1.0, theme::border()),
            StrokeKind::Inside,
        );
        ui.interact(card, ui.id().with(("server-dialog", tab)), Sense::click());
        widgets::text(
            ui,
            card.min + Vec2::new(16.0, 14.0),
            "Browser dialog",
            font(14.0, Weight::Medium),
            theme::foreground(),
            20.0,
        );
        widgets::text_in(
            ui,
            Rect::from_min_size(
                card.min + Vec2::new(16.0, 40.0),
                Vec2::new(card.width() - 32.0, 40.0),
            ),
            dialog["message"].as_str().unwrap_or_default(),
            font(13.0, Weight::Regular),
            theme::muted_foreground(),
        );
        if !mine {
            widgets::text(
                ui,
                Pos2::new(card.left() + 16.0, card.bottom() - 30.0),
                "Take control to respond.",
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
                16.0,
            );
            return;
        }
        let accept = Rect::from_min_size(
            Pos2::new(card.right() - 16.0 - 72.0, card.bottom() - 16.0 - 28.0),
            Vec2::new(72.0, 28.0),
        );
        let dismiss = Rect::from_min_size(
            Pos2::new(accept.left() - 8.0 - 72.0, accept.top()),
            Vec2::new(72.0, 28.0),
        );
        let prompt = (dialog["type"] == "prompt").then(|| {
            dialog["defaultValue"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        });
        if widgets::primary_button(ui, accept, ("server-dialog-accept", tab), "Accept", true)
            .clicked()
        {
            self.server_browser.answer_dialog(tab, true, prompt);
        } else if widgets::outline_button(
            ui,
            dismiss,
            ("server-dialog-dismiss", tab),
            None,
            "Dismiss",
            CornerRadius::same(8),
        )
        .clicked()
        {
            self.server_browser.answer_dialog(tab, false, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;
    use serde_json::json;

    use crate::app::{App, Options, PanelTab};

    fn snapshot(tab: &str, reveal: Option<(&str, bool)>) -> serde_json::Value {
        let mut snapshot = json!({"threadId": "a", "tabId": tab, "runtime": "server",
            "navStatus": {"_tag": "Success", "url": "http://localhost:5173/", "title": "Vite"},
            "canGoBack": false, "canGoForward": false, "updatedAt": "2026-01-01T00:00:00Z"});
        if let Some((id, force)) = reveal {
            snapshot["reveal"] = json!(true);
            snapshot["revealRequest"] = json!({"id": id, "force": force});
        }
        snapshot
    }

    #[test]
    fn agent_tabs_show_when_revealed_and_leave_when_closed() {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(800.0, 600.0))
            .build_eframe(|cc| {
                App::new(
                    cc,
                    Options {
                        server: None,
                        token: None,
                        client_settings_path: None,
                        block_turn_starts: true,
                    },
                )
            });
        let app = harness.state_mut();
        // A reveal already in the list is history.
        let list = json!({"serverEpoch": "e1", "revision": 3,
            "sessions": [snapshot("old", Some(("r0", false))), {"threadId": "a", "tabId": "desk", "runtime": "desktop"}]});
        assert!(app.server_preview_result("preview-list:a", &Ok(list)));
        assert_eq!(app.server_browser.tabs.len(), 1);
        assert!(app.panels.get("a").is_none_or(|p| p.tabs.is_empty()));
        // An agent reveals a new tab: it opens in the thread's panel.
        let event = json!({"type": "opened", "threadId": "a", "tabId": "new", "serverEpoch": "e1",
            "revision": 4, "createdAt": "2026-01-01T00:00:01Z", "snapshot": snapshot("new", Some(("r1", true)))});
        app.server_preview_result("preview-events", &Ok(event.clone()));
        let id = app.server_browser.id_of("a", "new").unwrap();
        assert!(app.panels["a"].tabs.contains(&PanelTab::Browser { id }));
        assert_eq!(app.server_browser.tab(id).unwrap().title, "Vite");
        // A replayed (older) event changes nothing; a close removes the tab.
        app.server_preview_result("preview-events", &Ok(event));
        let closed = json!({"type": "closed", "threadId": "a", "tabId": "new", "serverEpoch": "e1",
            "revision": 5, "createdAt": "2026-01-01T00:00:02Z"});
        app.server_preview_result("preview-events", &Ok(closed));
        assert!(app.server_browser.id_of("a", "new").is_none());
        assert!(!app.panels["a"].tabs.contains(&PanelTab::Browser { id }));
    }
}
