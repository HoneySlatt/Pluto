//! MCP apps inline in the chat (`components/chat/McpAppFrame.tsx`,
//! `McpAppTimelineRow` in `MessagesTimeline.tsx`, and the host side of the
//! bridge in `client-runtime/mcpApps/host.ts`, spec 2026-01-26).
//!
//! Upstream frames the captured app document in an opaque-origin iframe and
//! speaks the MCP Apps JSON-RPC bridge with it over `postMessage`. egui has no
//! web view, so the preview browser (`crate::preview`) loads a small host page
//! holding that same sandboxed iframe (`allow-scripts allow-forms`, never
//! same-origin); the host page relays the app's messages to the client through
//! a binding and posts the client's into the frame. The browser serves the
//! host page itself at a path of the server's origin: an opaque origin (a
//! `data:` page) may not reach a loopback server. The row draws the page's
//! frames and passes the pointer and keys back, like a published page.
//!
//! The bridge answers `ui/initialize` with the host's capabilities and
//! context, then sends the original call's arguments and result
//! (`tool-input`, `tool-result`). The app's tool calls and resource reads
//! reach its own MCP server through the environment (`mcpApps.callTool`,
//! `mcpApps.readResource`); a call to a tool its server does not mark
//! read-only, and a chat message the app sends, ask first, in a bar above the
//! app. Model context updates (`mcpApps.updateModelContext`) inform the
//! thread's next turn; the app sizes its row (`size-changed`) and may close
//! itself (`request-teardown`), leaving "Show app".
//!
//! Not ported: full screen (the app stays inline), downloads (refused with
//! `isError`), the theme's style variables (the app gets the dark theme), and
//! stopping an app whose document navigates away.

use std::collections::HashMap;

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::mcp_app;
use crate::theme::{self, Weight, font};
use crate::timeline::McpAppEntry;
use crate::ui::action_editor::{self, Kind};
use crate::ui::tool_images::ToolOutput;
use crate::ui::widgets;

/// `MAX_MESSAGE_BYTES`.
const MAX_MESSAGE_BYTES: usize = 256 * 1024;
/// `MAX_PENDING_REQUESTS`.
const MAX_PENDING: usize = 16;
/// Seconds an app may stay out of view before its tab closes.
const IDLE_CLOSE: f64 = 120.0;

#[derive(Default)]
pub(crate) struct McpApps {
    apps: HashMap<String, AppState>,
    /// Requests on their way to the environment, by RPC key: the app, its
    /// JSON-RPC id, and what the answer is for.
    pending: HashMap<String, (String, Value, Op)>,
    next: u64,
}

#[derive(Clone)]
enum Op {
    /// The tool definition the host hands the app at initialize.
    Definition,
    /// `toolInfo` before a call: whether to ask, then the call.
    CallInfo {
        name: String,
        arguments: Value,
    },
    Call,
    Read,
    ModelContext,
}

#[derive(Default, PartialEq)]
enum Url {
    #[default]
    Unrequested,
    Pending,
    Ready(String),
    Failed,
}

/// What the user is asked before the app goes on.
struct Approval {
    id: Value,
    text: String,
    action: Approve,
}

enum Approve {
    Call { name: String, arguments: Value },
    Message(String),
}

struct AppState {
    url: Url,
    tab: Option<u64>,
    drawn: bool,
    shown: bool,
    last_drawn: f64,
    height: f32,
    closed: bool,
    initialized: bool,
    tool_call_sent: bool,
    /// Requests the app has in flight.
    in_flight: usize,
    definition: Option<Value>,
    definition_asked: bool,
    approval: Option<Approval>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            url: Url::Unrequested,
            tab: None,
            drawn: false,
            shown: false,
            last_drawn: 0.0,
            height: mcp_app::DEFAULT_HEIGHT,
            closed: false,
            initialized: false,
            tool_call_sent: false,
            in_flight: 0,
            definition: None,
            definition_asked: false,
            approval: None,
        }
    }
}

/// The page that frames the app and relays its messages.
fn host_page(url: &str, allow: &str) -> String {
    let attribute = |value: &str| {
        value
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
    };
    let html = format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>html,body{{margin:0;height:100%;background:transparent;overflow:hidden}}iframe{{border:0;width:100%;height:100%;display:block;color-scheme:dark}}</style></head><body><iframe id="app" sandbox="allow-scripts allow-forms" allow="{allow}" src="{url}"></iframe><script>var f=document.getElementById("app");window.addEventListener("message",function(e){{if(e.source===f.contentWindow){{try{{{binding}(JSON.stringify(e.data))}}catch(x){{}}}}}});window.__plutoMcpIn=function(m){{if(f.contentWindow)f.contentWindow.postMessage(m,"*")}};</script></body></html>"#,
        url = attribute(url),
        allow = attribute(allow),
        binding = crate::preview::APP_BINDING,
    );
    html
}

/// `normalizeMcpAppToolResult`: absent fields left out, never null.
fn normalize_result(result: &Value) -> Value {
    let mut out = json!({"content": result["content"].as_array().cloned().unwrap_or_default()});
    if result["structuredContent"].is_object() {
        out["structuredContent"] = result["structuredContent"].clone();
    }
    if result["isError"] == true {
        out["isError"] = json!(true);
    }
    if result["_meta"].is_object() {
        out["_meta"] = result["_meta"].clone();
    }
    out
}

/// `ui/message`'s text: user-role text blocks only.
fn message_text(params: &Value) -> Option<String> {
    let blocks: Vec<&Value> = match &params["content"] {
        Value::Array(blocks) => blocks.iter().collect(),
        other => vec![other],
    };
    if params["role"] != "user" || blocks.is_empty() {
        return None;
    }
    let texts: Option<Vec<&str>> = blocks
        .iter()
        .map(|b| (b["type"] == "text").then(|| b["text"].as_str()).flatten())
        .collect();
    let text = texts?.join("\n").trim().to_owned();
    (!text.is_empty()).then_some(text)
}

impl App {
    /// `McpAppTimelineRow`: the app `px-1` inside the column.
    pub(crate) fn mcp_app_row(&mut self, ui: &mut Ui, entry: &McpAppEntry, width: f32) {
        let id = entry.id.clone();
        let app = entry.app.clone();
        let now = ui.input(|i| i.time);
        self.request_mcp_app(entry);
        self.pump_mcp_app(ui.ctx(), entry);
        let state = self.mcp_apps.apps.entry(id.clone()).or_default();
        if state.closed {
            // `The <server> app was closed`, with "Show app".
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, 40.0), Sense::hover());
            let rect = row.shrink2(Vec2::new(4.0, 0.0));
            ui.painter().rect(
                rect,
                CornerRadius::same(8),
                Color32::TRANSPARENT,
                Stroke::new(1.0, theme::border()),
                StrokeKind::Inside,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(rect.left() + 12.0, rect.top()), rect.max),
                &format!("The {} app was closed", app.server),
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            );
            let button =
                action_editor::footer_rect(ui, rect.right() - 6.0, rect.top() + 6.0, "Show app");
            let button = Rect::from_min_size(button.min, Vec2::new(button.width(), 28.0));
            if action_editor::footer_button(ui, button, "Show app", Kind::Outline) {
                // A new document: the app initializes again.
                let state = self.mcp_apps.apps.entry(id.clone()).or_default();
                *state = AppState::default();
            }
            ui.add_space(8.0);
            return;
        }
        // The approval the app waits on.
        let approval = state.approval.as_ref().map(|a| a.text.clone());
        if let Some(text) = approval {
            let galley = widgets::galley(
                ui,
                &text,
                font(13.0, Weight::Regular),
                theme::foreground(),
                18.0,
                width - 8.0 - 24.0,
            );
            let height = 12.0 + galley.size().y + 10.0 + 28.0 + 12.0;
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, height + 8.0), Sense::hover());
            let rect = Rect::from_min_size(
                Pos2::new(row.left() + 4.0, row.top()),
                Vec2::new(width - 8.0, height),
            );
            ui.painter().rect(
                rect,
                CornerRadius::same(10),
                theme::alpha(theme::warning(), 0.08),
                Stroke::new(1.0, theme::alpha(theme::warning(), 0.35)),
                StrokeKind::Inside,
            );
            ui.painter().galley(
                Pos2::new(rect.left() + 12.0, rect.top() + 12.0),
                galley.clone(),
                theme::foreground(),
            );
            let top = rect.top() + 12.0 + galley.size().y + 10.0;
            let allow = action_editor::footer_rect(ui, rect.right() - 12.0, top, "Allow");
            let allow = Rect::from_min_size(allow.min, Vec2::new(allow.width(), 28.0));
            let decline = action_editor::footer_rect(ui, allow.left() - 8.0, top, "Decline");
            let decline = Rect::from_min_size(decline.min, Vec2::new(decline.width(), 28.0));
            if action_editor::footer_button(ui, allow, "Allow", Kind::Primary) {
                self.answer_mcp_approval(entry, true);
            } else if action_editor::footer_button(ui, decline, "Decline", Kind::Outline) {
                self.answer_mcp_approval(entry, false);
            }
        }
        let state = self.mcp_apps.apps.entry(id.clone()).or_default();
        let height = state.height;
        let failed = state.url == Url::Failed;
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let area = Rect::from_min_size(
            Pos2::new(row.left() + 4.0, row.top()),
            Vec2::new((width - 8.0).max(1.0), height),
        );
        if !ui.is_rect_visible(area) {
            return;
        }
        state.drawn = true;
        state.last_drawn = now;
        let tab = state.tab;
        let tab_state = tab.and_then(|t| self.preview.engine.as_ref().and_then(|e| e.tab(t)));
        let engine_failed = self
            .preview
            .engine
            .as_ref()
            .is_some_and(|e| e.failure.lock().is_ok_and(|f| f.is_some()));
        if failed || engine_failed || tab_state.as_ref().is_some_and(|s| s.error.is_some()) {
            let text = format!("Unable to load the {} app", app.server);
            let text_width = widgets::text_width(ui, &text, font(12.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_center_size(area.center(), Vec2::new(text_width + 1.0, 16.0)),
                &text,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            );
            return;
        }
        let mut shown = false;
        if let (Some(tab), Some(tab_state)) = (tab, &tab_state) {
            let ppp = ui.ctx().pixels_per_point();
            if let Some(engine) = &self.preview.engine {
                engine.resize(
                    tab,
                    area.width().floor().max(1.0) as u32,
                    area.height().floor().max(1.0) as u32,
                    ppp,
                );
            }
            let mut page_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(area)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            page_ui.set_clip_rect(area.intersect(ui.clip_rect()));
            shown = self.page_view(&mut page_ui, area, tab, tab_state, true, true);
            if let Some(engine) = &self.preview.engine {
                for link in engine.take_links(tab) {
                    widgets::open_external(&link);
                }
            }
        }
        if app.prefers_border {
            ui.painter().rect_stroke(
                area,
                CornerRadius::same(8),
                Stroke::new(1.0, theme::border()),
                StrokeKind::Inside,
            );
        }
        if !shown {
            ui.painter().rect_filled(area, 0.0, theme::background());
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    /// The app's document URL, its tool definition, and its tab once both
    /// the URL and the engine are there.
    fn request_mcp_app(&mut self, entry: &McpAppEntry) {
        let id = entry.id.clone();
        let state = self.mcp_apps.apps.entry(id.clone()).or_default();
        if state.url == Url::Unrequested {
            state.url = Url::Pending;
            let resource = json!({
                "_tag": "attachment",
                "attachmentId": entry.app.attachment_id,
                "fileName": mcp_app::file_name(&entry.app),
                "mimeType": "text/html",
                "disposition": "inline",
                "renderIntent": "mcp-app",
            });
            self.send_request(
                format!("mcp-app-url:{id}"),
                "assets.createUrl",
                json!({"resource": resource}),
            );
        }
        let state = self.mcp_apps.apps.entry(id.clone()).or_default();
        if !state.definition_asked {
            state.definition_asked = true;
            self.mcp_app_rpc(
                entry,
                Value::Null,
                Op::Definition,
                "mcpApps.toolInfo",
                json!({"name": entry.app.tool}),
            );
        }
    }

    /// Opens the tab, takes the app's messages and sends the original call.
    fn pump_mcp_app(&mut self, ctx: &egui::Context, entry: &McpAppEntry) {
        let id = entry.id.clone();
        let Some(state) = self.mcp_apps.apps.get(&id) else {
            return;
        };
        if state.closed {
            return;
        }
        if state.tab.is_none()
            && let Url::Ready(url) = &state.url
        {
            let page = host_page(url, &mcp_app::allow_attribute(&entry.app));
            // The host page takes the app server's origin.
            let host = url::Url::parse(url)
                .ok()
                .and_then(|u| u.join(&format!("/__pluto/mcp-app-host/{}", entry.id)).ok())
                .map(|u| u.to_string());
            let Some(host) = host else {
                return;
            };
            self.preview.next_tab += 1;
            let tab = self.preview.next_tab;
            self.preview_engine(ctx).open_app(tab, host, page);
            if let Some(state) = self.mcp_apps.apps.get_mut(&id) {
                state.tab = Some(tab);
                state.shown = true;
            }
        }
        let Some(tab) = self.mcp_apps.apps.get(&id).and_then(|s| s.tab) else {
            return;
        };
        let messages = self
            .preview
            .engine
            .as_ref()
            .map(|e| e.take_app_messages(tab))
            .unwrap_or_default();
        for message in messages {
            self.receive_mcp_message(entry, message);
        }
        self.send_mcp_tool_call(entry);
    }

    fn post_mcp(&self, entry: &McpAppEntry, message: Value) {
        let Some(tab) = self.mcp_apps.apps.get(&entry.id).and_then(|s| s.tab) else {
            return;
        };
        if let (Some(engine), Ok(text)) = (&self.preview.engine, serde_json::to_string(&message)) {
            engine.evaluate(tab, format!("window.__plutoMcpIn({text})"));
        }
    }

    fn respond_mcp(&mut self, entry: &McpAppEntry, id: Value, result: Value) {
        self.post_mcp(entry, json!({"jsonrpc": "2.0", "id": id, "result": result}));
    }

    fn fail_mcp(&mut self, entry: &McpAppEntry, id: Value, code: i64, message: &str) {
        self.post_mcp(
            entry,
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
        );
    }

    /// `hostContext`.
    fn mcp_host_context(&self, entry: &McpAppEntry) -> Value {
        let state = self.mcp_apps.apps.get(&entry.id);
        let mut context = json!({
            "theme": "dark",
            "styles": {"variables": {}},
            "displayMode": "inline",
            "availableDisplayModes": ["inline"],
            "containerDimensions": {"width": 720, "maxHeight": 2000},
            "platform": "desktop",
            "userAgent": format!("pluto/{}", env!("CARGO_PKG_VERSION")),
            "deviceCapabilities": {"touch": false, "hover": true},
            "safeAreaInsets": {"top": 0, "right": 0, "bottom": 0, "left": 0},
        });
        if let Some(definition) = state.and_then(|s| s.definition.clone()) {
            context["toolInfo"] = json!({"tool": definition});
        }
        context
    }

    /// `receive`: one message the app posted.
    fn receive_mcp_message(&mut self, entry: &McpAppEntry, data: Value) {
        if data["jsonrpc"] != "2.0" {
            return;
        }
        let id = data
            .get("id")
            .filter(|id| id.is_string() || id.is_number())
            .cloned();
        if serde_json::to_string(&data).map_or(usize::MAX, |s| s.len()) > MAX_MESSAGE_BYTES {
            if let (Some(id), true) = (id, data["method"].is_string()) {
                self.fail_mcp(entry, id, -32600, "Message too large.");
            }
            return;
        }
        let Some(method) = data["method"].as_str().map(str::to_owned) else {
            // A response to a request the host sent; none are awaited.
            return;
        };
        let params = if data["params"].is_object() {
            data["params"].clone()
        } else {
            json!({})
        };
        match id {
            Some(id) => self.mcp_request(entry, id, &method, &params),
            None => self.mcp_notification(entry, &method, &params),
        }
    }

    fn mcp_request(&mut self, entry: &McpAppEntry, id: Value, method: &str, params: &Value) {
        match method {
            "ui/initialize" => {
                let requested = params["protocolVersion"].as_str();
                let mut sandbox = json!({});
                if !entry.app.csp.is_null() {
                    sandbox["csp"] = entry.app.csp.clone();
                }
                let result = json!({
                    "protocolVersion": requested
                        .filter(|v| *v == mcp_app::PROTOCOL_VERSION)
                        .unwrap_or(mcp_app::PROTOCOL_VERSION),
                    "hostInfo": {"name": "pluto", "version": env!("CARGO_PKG_VERSION")},
                    "hostCapabilities": {
                        "openLinks": {},
                        "serverTools": {},
                        "serverResources": {},
                        "logging": {},
                        "message": {"text": {}},
                        "updateModelContext": {"text": {}, "structuredContent": {}},
                        "downloadFile": {},
                        "sandbox": sandbox,
                    },
                    "hostContext": self.mcp_host_context(entry),
                });
                self.respond_mcp(entry, id, result);
            }
            "ping" => self.respond_mcp(entry, id, json!({})),
            "tools/call" => {
                let name = params["name"].as_str().filter(|n| !n.trim().is_empty());
                let arguments = if params["arguments"].is_null() {
                    json!({})
                } else {
                    params["arguments"].clone()
                };
                let (Some(name), true) = (name, arguments.is_object()) else {
                    self.fail_mcp(
                        entry,
                        id,
                        -32602,
                        "tools/call needs a tool name and object arguments.",
                    );
                    return;
                };
                let name = name.to_owned();
                self.mcp_app_rpc(
                    entry,
                    id,
                    Op::CallInfo {
                        name: name.clone(),
                        arguments,
                    },
                    "mcpApps.toolInfo",
                    json!({"name": name}),
                );
            }
            "resources/read" => match params["uri"].as_str().filter(|u| !u.trim().is_empty()) {
                Some(uri) => {
                    let uri = uri.to_owned();
                    self.mcp_app_rpc(
                        entry,
                        id,
                        Op::Read,
                        "mcpApps.readResource",
                        json!({"uri": uri}),
                    );
                }
                None => self.fail_mcp(entry, id, -32602, "resources/read needs a uri."),
            },
            "ui/open-link" => match params["url"].as_str() {
                Some(url) if url.starts_with("http://") || url.starts_with("https://") => {
                    widgets::open_external(url);
                    self.respond_mcp(entry, id, json!({}));
                }
                _ => self.fail_mcp(entry, id, -32602, "Only http(s) links can be opened."),
            },
            "ui/message" => match message_text(params) {
                Some(text) => {
                    let ask = format!("Send this message from {}?\n{text}", entry.app.server);
                    self.ask_mcp(entry, id, ask, Approve::Message(text));
                }
                None => self.fail_mcp(
                    entry,
                    id,
                    -32602,
                    "Only text messages from the user role are supported.",
                ),
            },
            // Only inline here: the answer is the current mode.
            "ui/request-display-mode" => self.respond_mcp(entry, id, json!({"mode": "inline"})),
            "ui/update-model-context" => {
                let content = &params["content"];
                let structured = &params["structuredContent"];
                if (!content.is_null() && !content.is_array())
                    || (!structured.is_null() && !structured.is_object())
                {
                    self.fail_mcp(
                        entry,
                        id,
                        -32602,
                        "Model context needs content blocks or an object of structured content.",
                    );
                    return;
                }
                let mut input = json!({
                    "conversationThreadId": self.selected.clone().unwrap_or_else(|| entry.source_thread_id.clone()),
                });
                if !content.is_null() {
                    input["content"] = content.clone();
                }
                if !structured.is_null() {
                    input["structuredContent"] = structured.clone();
                }
                self.mcp_app_rpc(
                    entry,
                    id,
                    Op::ModelContext,
                    "mcpApps.updateModelContext",
                    input,
                );
            }
            // Downloads are refused; the draft reports that in the result.
            "ui/download-file" => self.respond_mcp(entry, id, json!({"isError": true})),
            _ => self.fail_mcp(entry, id, -32601, &format!("Method not found: {method}")),
        }
    }

    fn mcp_notification(&mut self, entry: &McpAppEntry, method: &str, params: &Value) {
        let Some(state) = self.mcp_apps.apps.get_mut(&entry.id) else {
            return;
        };
        match method {
            "ui/notifications/initialized" => {
                state.initialized = true;
                self.send_mcp_tool_call(entry);
            }
            "ui/notifications/size-changed" => {
                if let Some(height) = params["height"].as_f64() {
                    state.height = mcp_app::clamp_height(height as f32);
                }
            }
            "ui/notifications/request-teardown" => {
                state.closed = true;
                if let Some(tab) = state.tab.take()
                    && let Some(engine) = &self.preview.engine
                {
                    engine.close(tab);
                    self.preview.textures.remove(&tab);
                }
            }
            _ => {}
        }
    }

    /// `sendToolCall`: the call's arguments and result, once the app has
    /// initialized and the stored call is fetched.
    fn send_mcp_tool_call(&mut self, entry: &McpAppEntry) {
        let ready = self
            .mcp_apps
            .apps
            .get(&entry.id)
            .is_some_and(|s| s.initialized && !s.tool_call_sent);
        if !ready {
            return;
        }
        // The wire timeline withholds the call's input and output.
        let (arguments, result) = match self.fetched_tool_item(&entry.item) {
            Some(ToolOutput::Item(item)) if item["type"] == "dynamic_tool" => {
                (item["input"].clone(), item["output"]["result"].clone())
            }
            Some(ToolOutput::Loading) => return,
            _ => (
                entry.item.get("input").clone(),
                entry.item.get("output")["result"].clone(),
            ),
        };
        if let Some(state) = self.mcp_apps.apps.get_mut(&entry.id) {
            state.tool_call_sent = true;
        }
        let arguments = if arguments.is_object() {
            arguments
        } else {
            json!({})
        };
        self.post_mcp(
            entry,
            json!({"jsonrpc": "2.0", "method": "ui/notifications/tool-input", "params": {"arguments": arguments}}),
        );
        if result.is_object() {
            self.post_mcp(
                entry,
                json!({"jsonrpc": "2.0", "method": "ui/notifications/tool-result", "params": normalize_result(&result)}),
            );
        }
    }

    fn ask_mcp(&mut self, entry: &McpAppEntry, id: Value, text: String, action: Approve) {
        let Some(state) = self.mcp_apps.apps.get_mut(&entry.id) else {
            return;
        };
        // One question at a time; a second one is refused.
        if state.approval.is_some() {
            self.fail_mcp(
                entry,
                id,
                -32000,
                "Another request is waiting for the user.",
            );
            return;
        }
        state.approval = Some(Approval { id, text, action });
    }

    fn answer_mcp_approval(&mut self, entry: &McpAppEntry, allowed: bool) {
        let Some(approval) = self
            .mcp_apps
            .apps
            .get_mut(&entry.id)
            .and_then(|s| s.approval.take())
        else {
            return;
        };
        if !allowed {
            self.fail_mcp(entry, approval.id, -32000, "Declined by the user.");
            return;
        }
        match approval.action {
            Approve::Call { name, arguments } => self.mcp_app_rpc(
                entry,
                approval.id,
                Op::Call,
                "mcpApps.callTool",
                json!({"name": name, "arguments": arguments}),
            ),
            Approve::Message(text) => {
                // `sendAppMessage`: queued like a typed message.
                let Some(thread) = self.current_thread() else {
                    self.fail_mcp(
                        entry,
                        approval.id,
                        -32000,
                        "Messages are not available here.",
                    );
                    return;
                };
                let model = self.composer_model_json(false);
                let command =
                    crate::ui::composer::message_dispatch(&thread.id, &text, &model, "queue");
                self.dispatch(command);
                self.respond_mcp(entry, approval.id, json!({}));
            }
        }
    }

    /// Sends one environment request for the app (`McpAppTarget` is the
    /// item's thread and id).
    fn mcp_app_rpc(
        &mut self,
        entry: &McpAppEntry,
        id: Value,
        op: Op,
        method: &'static str,
        extra: Value,
    ) {
        // The requests that answer the app count toward its limit.
        let counts = matches!(op, Op::Call | Op::Read | Op::ModelContext);
        if counts && let Some(state) = self.mcp_apps.apps.get_mut(&entry.id) {
            if state.in_flight >= MAX_PENDING {
                self.fail_mcp(entry, id, -32000, "Too many requests in flight.");
                return;
            }
            state.in_flight += 1;
        }
        let mut input = json!({"threadId": entry.source_thread_id, "itemId": entry.item.id});
        if let (Some(input), Some(extra)) = (input.as_object_mut(), extra.as_object()) {
            input.extend(extra.clone());
        }
        self.mcp_apps.next += 1;
        let key = format!("mcp-app-rpc:{}", self.mcp_apps.next);
        self.mcp_apps
            .pending
            .insert(key.clone(), (entry.id.clone(), id, op));
        self.send_request(key, method, input);
    }

    /// The URL and environment answers; true when `key` was one.
    pub(crate) fn mcp_app_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        if let Some(id) = key.strip_prefix("mcp-app-url:") {
            let base = url::Url::parse(self.connect.server.trim()).ok();
            let url = result
                .as_ref()
                .ok()
                .and_then(|r| base?.join(r["relativeUrl"].as_str()?).ok());
            if let Some(state) = self.mcp_apps.apps.get_mut(id) {
                state.url = url.map_or(Url::Failed, |u| Url::Ready(u.to_string()));
            }
            return true;
        }
        if !key.starts_with("mcp-app-rpc:") {
            return false;
        }
        let Some((app_id, id, op)) = self.mcp_apps.pending.remove(key) else {
            return true;
        };
        // The entry the row draws, rebuilt from the stored ids for posting.
        let Some(entry) = self.mcp_app_entry(&app_id) else {
            return true;
        };
        if matches!(op, Op::Call | Op::Read | Op::ModelContext)
            && let Some(state) = self.mcp_apps.apps.get_mut(&app_id)
        {
            state.in_flight = state.in_flight.saturating_sub(1);
        }
        let error = |e: &String| {
            if e.trim().is_empty() {
                "Request failed.".to_owned()
            } else {
                e.clone()
            }
        };
        match (op, result) {
            (Op::Definition, Ok(info)) => {
                if let Some(state) = self.mcp_apps.apps.get_mut(&app_id)
                    && !info["tool"].is_null()
                {
                    state.definition = Some(info["tool"].clone());
                }
            }
            (Op::Definition, Err(_)) => {}
            (Op::CallInfo { name, arguments }, Ok(info)) => {
                if info["callable"] != true {
                    self.fail_mcp(&entry, id, -32000, "This app cannot call that tool.");
                } else if info["readOnly"] == true {
                    self.mcp_app_rpc(
                        &entry,
                        id,
                        Op::Call,
                        "mcpApps.callTool",
                        json!({"name": name, "arguments": arguments}),
                    );
                } else {
                    let title = info["title"].as_str().unwrap_or(&name).to_owned();
                    let pretty = serde_json::to_string_pretty(&arguments).unwrap_or_default();
                    let ask = format!("Allow {} to run {title}?\n{pretty}", entry.app.server);
                    self.ask_mcp(&entry, id, ask, Approve::Call { name, arguments });
                }
            }
            (Op::Call, Ok(value)) => self.respond_mcp(&entry, id, normalize_result(value)),
            (Op::Read, Ok(value)) => self.respond_mcp(&entry, id, value.clone()),
            (Op::ModelContext, Ok(_)) => self.respond_mcp(&entry, id, json!({})),
            (_, Err(e)) => self.fail_mcp(&entry, id, -32000, &error(e)),
        }
        true
    }

    /// The timeline entry of an app, from the open thread's rows.
    fn mcp_app_entry(&self, id: &str) -> Option<McpAppEntry> {
        let detail = self.current_detail()?;
        let row = detail.visible.iter().find(|row| row.item.id == id)?;
        Some(McpAppEntry {
            id: row.item.id.clone(),
            run_id: row.item.run_id.clone(),
            created_at: String::new(),
            source_thread_id: row.source_thread_id.clone(),
            item: row.item.clone(),
            app: mcp_app::from_tool_item(&row.item)?,
        })
    }

    /// Pauses apps scrolled out of view and closes those unseen for a
    /// while; called once a frame after the chat drew.
    pub(crate) fn mcp_apps_end_frame(&mut self, now: f64) {
        let Some(engine) = &self.preview.engine else {
            for state in self.mcp_apps.apps.values_mut() {
                state.drawn = false;
            }
            return;
        };
        let mut closed = Vec::new();
        for state in self.mcp_apps.apps.values_mut() {
            let Some(tab) = state.tab else {
                state.drawn = false;
                continue;
            };
            if state.drawn != state.shown {
                engine.set_visible(tab, state.drawn);
                state.shown = state.drawn;
            }
            if !state.drawn && now - state.last_drawn > IDLE_CLOSE {
                // A new document starts over: the app initializes again.
                engine.close(tab);
                closed.push(tab);
                state.tab = None;
                state.initialized = false;
                state.tool_call_sent = false;
            }
            state.drawn = false;
        }
        for tab in closed {
            self.preview.textures.remove(&tab);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_and_messages_follow_the_spec() {
        assert_eq!(
            normalize_result(
                &json!({"content": [{"type": "text", "text": "ok"}], "_meta": null, "isError": false})
            ),
            json!({"content": [{"type": "text", "text": "ok"}]})
        );
        assert_eq!(
            message_text(&json!({"role": "user", "content": [{"type": "text", "text": " hi "}]}))
                .as_deref(),
            Some("hi")
        );
        assert_eq!(
            message_text(&json!({"role": "assistant", "content": {"type": "text", "text": "x"}})),
            None
        );
        let html = host_page("http://127.0.0.1/a?x=1&y=\"2\"", "camera");
        assert!(html.contains(r#"src="http://127.0.0.1/a?x=1&amp;y=&quot;2&quot;""#));
        assert!(html.contains(r#"sandbox="allow-scripts allow-forms""#));
        assert!(!html.contains("allow-same-origin"));
    }
}
