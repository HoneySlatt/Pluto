//! The server's browser (`packages/client-runtime/src/preview/serverBrowserStream.ts`,
//! `apps/web/src/browser/ServerBrowserSurface.tsx`): tabs the server runs
//! (`runtime: "server"`), which agents see and drive too, shown here from
//! the `/api/preview-stream/ws` WebSocket: JPEG frames (each acknowledged),
//! the page's viewport, who has control, dialogs, popups and the agent's
//! pointer; input goes back as JSON while this client has control.
//!
//! Each tab gets a local id from the Browser surface's counter so the
//! panel and the page view treat it like a local tab.
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use serde_json::{Value, json};
use tungstenite::client::IntoClientRequest;
use zeroize::Zeroizing;

use crate::preview::{KeyStroke, Pointer, TabState};

/// What the stream told about its tab.
#[derive(Default)]
pub struct StreamState {
    pub frame: Option<Arc<egui::ColorImage>>,
    pub frame_seq: u64,
    /// The page's layout size in CSS pixels: the input's coordinates.
    pub viewport: Option<(f32, f32)>,
    /// `control`: whether this client may operate, who controls, the dialog.
    pub control: Option<Value>,
    /// The agent's pointer and when it moved.
    pub pointer: Option<(f32, f32, Instant)>,
    /// A tab the page opened while this client had control.
    pub popup: Option<String>,
    /// Why the stream stopped for good (gone, host setup, unauthorized).
    pub failure: Option<String>,
    pub connected: bool,
}

/// One server tab: its thread, server id, latest snapshot and stream.
pub struct ServerTab {
    pub thread: String,
    pub tab_id: String,
    pub snapshot: Value,
    state: Arc<Mutex<StreamState>>,
    outgoing: Option<mpsc::Sender<String>>,
    /// The size last asked for.
    size: Option<(u32, u32)>,
}

/// The server tabs this client knows, by local id.
#[derive(Default)]
pub struct ServerBrowser {
    pub tabs: HashMap<u64, ServerTab>,
    /// `serverEpoch` and `revision` of what was applied (`applyPreviewServerEvent`).
    pub epoch: Option<String>,
    pub revision: u64,
    /// Reveal requests already handled.
    pub revealed: std::collections::HashSet<String>,
}

fn modifiers_of(bits: u32) -> u32 {
    // CDP and the stream share the bits: Alt 1, Ctrl 2, Meta 4, Shift 8.
    bits
}

impl ServerBrowser {
    pub fn id_of(&self, thread: &str, tab_id: &str) -> Option<u64> {
        self.tabs
            .iter()
            .find(|(_, t)| t.thread == thread && t.tab_id == tab_id)
            .map(|(id, _)| *id)
    }

    pub fn owns(&self, id: u64) -> bool {
        self.tabs.contains_key(&id)
    }

    /// Adds or updates a tab from its snapshot; returns its local id.
    pub fn upsert(&mut self, snapshot: &Value, next_id: &mut u64) -> Option<u64> {
        let thread = snapshot["threadId"].as_str()?.to_owned();
        let tab_id = snapshot["tabId"].as_str()?.to_owned();
        if let Some(id) = self.id_of(&thread, &tab_id) {
            if let Some(tab) = self.tabs.get_mut(&id) {
                tab.snapshot = snapshot.clone();
            }
            return Some(id);
        }
        *next_id += 1;
        let id = *next_id;
        self.tabs.insert(
            id,
            ServerTab {
                thread,
                tab_id,
                snapshot: snapshot.clone(),
                state: Default::default(),
                outgoing: None,
                size: None,
            },
        );
        Some(id)
    }

    /// Forgets a tab and stops its stream.
    pub fn remove(&mut self, id: u64) -> Option<ServerTab> {
        self.tabs.remove(&id)
    }

    /// Opens the tab's stream if it has none.
    pub fn attach(&mut self, id: u64, server: &str, token: &str, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(&id) else {
            return;
        };
        if tab.outgoing.is_some() {
            return;
        }
        let Ok(mut url) = url::Url::parse(server.trim()) else {
            return;
        };
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        if url.set_scheme(scheme).is_err() {
            return;
        }
        url.set_path("/api/preview-stream/ws");
        url.query_pairs_mut()
            .clear()
            .append_pair("threadId", &tab.thread)
            .append_pair("tabId", &tab.tab_id)
            .append_pair("maxWidth", "3840")
            .append_pair("maxHeight", "2160");
        let (sender, receiver) = mpsc::channel();
        tab.outgoing = Some(sender);
        let state = tab.state.clone();
        let token = Zeroizing::new(token.to_owned());
        let ctx = ctx.clone();
        std::thread::spawn(move || stream(url, token, state, receiver, ctx));
    }

    fn send(&self, id: u64, message: Value) {
        if let Some(sender) = self.tabs.get(&id).and_then(|t| t.outgoing.as_ref()) {
            let _ = sender.send(message.to_string());
        }
    }

    /// Whether this client controls the tab (`controller === "you"`).
    pub fn controlling(&self, id: u64) -> bool {
        self.tabs.get(&id).is_some_and(|tab| {
            tab.state
                .lock()
                .is_ok_and(|s| s.control.as_ref().is_some_and(|c| c["controller"] == "you"))
        })
    }

    pub fn control(&self, id: u64) -> Option<Value> {
        self.tabs.get(&id)?.state.lock().ok()?.control.clone()
    }

    pub fn agent_pointer(&self, id: u64) -> Option<(f32, f32)> {
        let state = self.tabs.get(&id)?.state.lock().ok()?;
        state
            .pointer
            .filter(|(_, _, at)| at.elapsed() < Duration::from_secs(3))
            .map(|(x, y, _)| (x, y))
    }

    pub fn take_popup(&self, id: u64) -> Option<String> {
        self.tabs.get(&id)?.state.lock().ok()?.popup.take()
    }

    /// The tab as the page view and chrome read it.
    pub fn tab(&self, id: u64) -> Option<TabState> {
        let tab = self.tabs.get(&id)?;
        let state = tab.state.lock().ok()?;
        let status = &tab.snapshot["navStatus"];
        let failed = status["_tag"] == "LoadFailed";
        Some(TabState {
            url: status["url"].as_str().unwrap_or_default().to_owned(),
            title: status["title"].as_str().unwrap_or_default().to_owned(),
            loading: status["_tag"] == "Loading",
            can_go_back: tab.snapshot["canGoBack"] == true,
            can_go_forward: tab.snapshot["canGoForward"] == true,
            frame: state.frame.clone(),
            frame_seq: state.frame_seq,
            error: state.failure.clone().or_else(|| {
                failed.then(|| {
                    status["description"]
                        .as_str()
                        .unwrap_or("Could not open the page")
                        .to_owned()
                })
            }),
            display_size: state.viewport,
            ..Default::default()
        })
    }

    pub fn take_control(&self, id: u64, take: bool) {
        self.send(
            id,
            json!({"type": if take { "takeControl" } else { "releaseControl" }}),
        );
    }

    pub fn answer_dialog(&self, id: u64, accept: bool, prompt: Option<String>) {
        let mut message = json!({"type": "dialog", "accept": accept});
        if let Some(prompt) = prompt {
            message["promptText"] = json!(prompt);
        }
        self.send(id, message);
    }

    pub fn back(&self, id: u64) {
        self.send(id, json!({"type": "history", "delta": -1}));
    }
    pub fn forward(&self, id: u64) {
        self.send(id, json!({"type": "history", "delta": 1}));
    }
    pub fn reload(&self, id: u64) {
        self.send(id, json!({"type": "reload"}));
    }
    pub fn navigate(&self, id: u64, url: String) {
        self.send(id, json!({"type": "navigate", "url": url}));
    }

    /// The page follows the panel's size while this client controls it.
    pub fn resize(&mut self, id: u64, width: u32, height: u32) {
        let controlling = self.controlling(id);
        let Some(tab) = self.tabs.get_mut(&id) else {
            return;
        };
        let size = (width.clamp(100, 3840), height.clamp(100, 2160));
        if !controlling || tab.size == Some(size) {
            return;
        }
        tab.size = Some(size);
        self.send(
            id,
            json!({"type": "resize", "width": size.0, "height": size.1}),
        );
    }

    pub fn pointer(&self, id: u64, event: Pointer, bits: u32) {
        if !self.controlling(id) {
            return;
        }
        let modifiers = modifiers_of(bits);
        let message = match event {
            Pointer::Move { x, y } => {
                json!({"type": "mouse", "action": "move", "x": x, "y": y, "button": "none", "buttons": 0, "clickCount": 0, "modifiers": modifiers})
            }
            Pointer::Press {
                x,
                y,
                button,
                clicks,
            } => json!({"type": "mouse", "action": "down", "x": x, "y": y, "button": button,
                "buttons": buttons(button), "clickCount": clicks, "modifiers": modifiers}),
            Pointer::Release {
                x,
                y,
                button,
                clicks,
            } => json!({"type": "mouse", "action": "up", "x": x, "y": y, "button": button,
                "buttons": 0, "clickCount": clicks, "modifiers": modifiers}),
            Pointer::Wheel { x, y, dx, dy } => {
                json!({"type": "wheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy, "modifiers": modifiers})
            }
        };
        self.send(id, message);
    }

    pub fn key(&self, id: u64, stroke: KeyStroke) {
        if !self.controlling(id) {
            return;
        }
        let mut message = json!({
            "type": "key",
            "action": if stroke.pressed { "down" } else { "up" },
            "key": stroke.key,
            "code": stroke.code,
            "keyCode": stroke.virtual_key,
            "modifiers": modifiers_of(stroke.modifiers),
        });
        if let Some(text) = stroke.text {
            message["text"] = json!(text);
        }
        self.send(id, message);
    }

    pub fn text(&self, id: u64, text: String) {
        if !self.controlling(id) {
            return;
        }
        self.send(id, json!({"type": "text", "text": text}));
    }
}

fn buttons(button: &str) -> u32 {
    match button {
        "left" => 1,
        "right" => 2,
        "middle" => 4,
        _ => 0,
    }
}

/// The stream's connection loop: reconnects with backoff
/// (`min(500·2^n, 10000)` ms) until the tab is gone, the host lacks what
/// it needs, or the client drops the tab.
fn stream(
    url: url::Url,
    token: Zeroizing<String>,
    state: Arc<Mutex<StreamState>>,
    outgoing: mpsc::Receiver<String>,
    ctx: egui::Context,
) {
    let mut attempt = 0u32;
    loop {
        let result = session(&url, &token, &state, &outgoing, &ctx);
        if let Ok(mut state) = state.lock() {
            state.connected = false;
        }
        match result {
            Ok(()) => return,
            Err(Some(failure)) => {
                if let Ok(mut state) = state.lock() {
                    state.failure = Some(failure);
                }
                ctx.request_repaint();
                return;
            }
            Err(None) => {}
        }
        // A dropped tab closes the channel: stop.
        if matches!(outgoing.try_recv(), Err(mpsc::TryRecvError::Disconnected)) {
            return;
        }
        let delay = (500u64 << attempt.min(5)).min(10_000);
        attempt += 1;
        std::thread::sleep(Duration::from_millis(delay));
    }
}

type Socket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

/// One connection. `Ok`: the client dropped the tab; `Err(Some)`: a final
/// failure; `Err(None)`: try again.
fn session(
    url: &url::Url,
    token: &str,
    state: &Arc<Mutex<StreamState>>,
    outgoing: &mpsc::Receiver<String>,
    ctx: &egui::Context,
) -> Result<(), Option<String>> {
    let host = url
        .host_str()
        .ok_or(None)?
        .trim_matches(['[', ']'])
        .to_owned();
    let port = url.port_or_known_default().ok_or(None)?;
    let tcp = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|_| None)?
        .find_map(|a| TcpStream::connect_timeout(&a, Duration::from_secs(5)).ok())
        .ok_or(None)?;
    let mut request = url.as_str().into_client_request().map_err(|_| None)?;
    if !token.trim().is_empty() {
        let mut value = format!("Bearer {}", token.trim())
            .parse::<tungstenite::http::HeaderValue>()
            .map_err(|_| None)?;
        value.set_sensitive(true);
        request.headers_mut().insert("authorization", value);
    }
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_frame_size(Some(64 << 20))
        .max_message_size(Some(64 << 20));
    let (mut socket, _) = tungstenite::client_tls_with_config(request, tcp, Some(config), None)
        .map_err(|error| match error {
            tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response))
                if matches!(response.status().as_u16(), 401 | 403) =>
            {
                Some("This connection cannot watch the server's browser.".to_owned())
            }
            tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response))
                if response.status().as_u16() == 404 =>
            {
                Some("The browser tab is gone.".to_owned())
            }
            _ => None,
        })?;
    set_timeout(&socket, Some(Duration::from_millis(30)));
    if let Ok(mut state) = state.lock() {
        state.connected = true;
        state.failure = None;
    }
    loop {
        // What the panel asked for.
        loop {
            match outgoing.try_recv() {
                Ok(message) => socket
                    .send(tungstenite::Message::text(message))
                    .map_err(|_| None)?,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = socket.close(None);
                    return Ok(());
                }
            }
        }
        match socket.read() {
            Ok(tungstenite::Message::Binary(data)) => {
                socket
                    .send(tungstenite::Message::text(r#"{"type":"ack"}"#))
                    .map_err(|_| None)?;
                if let Ok(image) =
                    image::load_from_memory_with_format(&data, image::ImageFormat::Jpeg)
                {
                    let rgba = image.to_rgba8();
                    let frame = egui::ColorImage::from_rgba_unmultiplied(
                        [rgba.width() as usize, rgba.height() as usize],
                        rgba.as_raw(),
                    );
                    if let Ok(mut state) = state.lock() {
                        state.frame = Some(Arc::new(frame));
                        state.frame_seq += 1;
                    }
                    ctx.request_repaint();
                }
            }
            Ok(tungstenite::Message::Text(text)) => {
                if let Ok(message) = serde_json::from_str::<Value>(text.as_str()) {
                    apply(state, &message, ctx);
                }
            }
            Ok(tungstenite::Message::Close(frame)) => {
                let code = frame.as_ref().map_or(1005, |f| u16::from(f.code));
                let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                return Err(match code {
                    4404 => Some("The browser tab is gone.".to_owned()),
                    4503 => Some(host_setup(&reason)),
                    4401 | 1008 => {
                        Some("This connection cannot watch the server's browser.".to_owned())
                    }
                    _ => None,
                });
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return Err(None),
        }
    }
}

/// `previewStreamHostSetupMessage`: what the server's machine lacks.
fn host_setup(reason: &str) -> String {
    let parsed: Value = serde_json::from_str(reason).unwrap_or_default();
    let command = parsed["command"].as_str().unwrap_or_default();
    let what = match parsed["need"].as_str() {
        Some("sandbox") => "The server's browser needs its sandbox set up.",
        _ => "The server's browser is missing system libraries.",
    };
    if command.is_empty() {
        what.to_owned()
    } else {
        format!("{what} Run on the server: {command}")
    }
}

fn set_timeout(socket: &Socket, timeout: Option<Duration>) {
    let stream = match socket.get_ref() {
        tungstenite::stream::MaybeTlsStream::Plain(stream) => stream,
        tungstenite::stream::MaybeTlsStream::Rustls(stream) => stream.get_ref(),
        _ => return,
    };
    let _ = stream.set_read_timeout(timeout);
}

/// A text message from the stream.
fn apply(state: &Arc<Mutex<StreamState>>, message: &Value, ctx: &egui::Context) {
    let Ok(mut state) = state.lock() else {
        return;
    };
    match message["type"].as_str() {
        Some("viewport") => {
            if let (Some(w), Some(h)) = (message["width"].as_f64(), message["height"].as_f64()) {
                state.viewport = Some((w as f32, h as f32));
            }
        }
        Some("control") => state.control = Some(message.clone()),
        Some("pointer") => {
            if let (Some(x), Some(y)) = (message["x"].as_f64(), message["y"].as_f64()) {
                state.pointer = Some((x as f32, y as f32, Instant::now()));
            }
        }
        Some("popup") => state.popup = message["tabId"].as_str().map(str::to_owned),
        Some("clipboard") => {
            if let Some(text) = message["text"].as_str() {
                ctx.copy_text(text.to_owned());
            }
        }
        _ => {}
    }
    ctx.request_repaint();
}
