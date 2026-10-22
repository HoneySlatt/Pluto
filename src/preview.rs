//! The preview browser behind the right panel's Browser tabs. Upstream's
//! desktop app shows previews in an Electron webview; egui cannot embed a
//! web view (and on Wayland no native one can sit inside its window), so a
//! headless Chromium-family browser renders each tab off screen and the
//! DevTools protocol carries its frames (`Page.startScreencast`) to the
//! panel and the panel's pointer and keys back to the page.
//!
//! The browser runs with a throwaway profile in the temporary directory,
//! never the user's own. It talks over `--remote-debugging-pipe` (file
//! descriptors 3 and 4, NUL-separated JSON), so it exits on its own once
//! the client goes away, crash included, even behind launcher scripts.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use eframe::egui;
use serde_json::{Value, json};

/// Chromium-family browsers that can run headless, by command name;
/// `T3_PREVIEW_BROWSER` names another.
const CANDIDATES: [&str; 9] = [
    "chromium",
    "chromium-browser",
    "google-chrome-stable",
    "google-chrome",
    "brave",
    "brave-browser",
    "brave-origin",
    "microsoft-edge",
    "vivaldi",
];

/// The preview browser's executable, if one is installed.
pub fn browser_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("T3_PREVIEW_BROWSER") {
        return Some(PathBuf::from(path));
    }
    let paths = std::env::var_os("PATH")?;
    CANDIDATES.iter().find_map(|name| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
    })
}

/// A pointer event on a page, in CSS pixels.
#[derive(Clone, Debug)]
pub enum Pointer {
    Move {
        x: f32,
        y: f32,
    },
    Press {
        x: f32,
        y: f32,
        button: &'static str,
        clicks: u32,
    },
    Release {
        x: f32,
        y: f32,
        button: &'static str,
        clicks: u32,
    },
    Wheel {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
}

/// A key on a page: its DOM `key` and `code`, Windows virtual key code and
/// the text it types, if any.
#[derive(Clone, Debug)]
pub struct KeyStroke {
    pub key: String,
    pub code: String,
    pub virtual_key: u32,
    pub text: Option<String>,
    pub pressed: bool,
    /// CDP modifier bits: Alt 1, Ctrl 2, Meta 4, Shift 8.
    pub modifiers: u32,
}

enum Request {
    Open {
        tab: u64,
        url: Option<String>,
    },
    Navigate {
        tab: u64,
        url: String,
    },
    History {
        tab: u64,
        offset: i64,
    },
    Reload {
        tab: u64,
    },
    Stop {
        tab: u64,
    },
    Resize {
        tab: u64,
        width: u32,
        height: u32,
        scale: f32,
    },
    Visible {
        tab: u64,
        visible: bool,
    },
    Pointer {
        tab: u64,
        event: Pointer,
        modifiers: u32,
    },
    Key {
        tab: u64,
        stroke: KeyStroke,
    },
    Text {
        tab: u64,
        text: String,
    },
    Close {
        tab: u64,
    },
    Shutdown,
}

/// What the engine thread waits on: the panel's requests and the
/// browser's messages.
enum Input {
    Panel(Request),
    Browser(String),
    Closed,
}

/// What the panel shows of a tab.
#[derive(Clone, Default)]
pub struct TabState {
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    /// The latest frame and a counter that moves with each new one.
    pub frame: Option<Arc<egui::ColorImage>>,
    pub frame_seq: u64,
    pub error: Option<String>,
}

/// The running preview browser.
pub struct Preview {
    commands: Sender<Input>,
    tabs: Arc<Mutex<HashMap<u64, TabState>>>,
    pub failure: Arc<Mutex<Option<String>>>,
    /// Joined on drop, so the browser and its profile are gone before the
    /// client exits.
    engine: Option<std::thread::JoinHandle<()>>,
}

impl Preview {
    pub fn start(ctx: egui::Context) -> Self {
        let (commands, inbox) = mpsc::channel();
        let tabs = Arc::new(Mutex::new(HashMap::new()));
        let failure = Arc::new(Mutex::new(None));
        let (shared, failed, browser) = (tabs.clone(), failure.clone(), commands.clone());
        let engine = std::thread::spawn(move || {
            if let Err(error) = run(inbox, browser, &shared, &ctx) {
                if let Ok(mut failure) = failed.lock() {
                    *failure = Some(error);
                }
                ctx.request_repaint();
            }
        });
        Self {
            commands,
            tabs,
            failure,
            engine: Some(engine),
        }
    }

    pub fn tab(&self, tab: u64) -> Option<TabState> {
        self.tabs.lock().ok()?.get(&tab).cloned()
    }

    fn send(&self, command: Request) {
        let _ = self.commands.send(Input::Panel(command));
    }

    pub fn open(&self, tab: u64, url: Option<String>) {
        self.send(Request::Open { tab, url });
    }
    pub fn navigate(&self, tab: u64, url: String) {
        self.send(Request::Navigate { tab, url });
    }
    pub fn back(&self, tab: u64) {
        self.send(Request::History { tab, offset: -1 });
    }
    pub fn forward(&self, tab: u64) {
        self.send(Request::History { tab, offset: 1 });
    }
    pub fn reload(&self, tab: u64) {
        self.send(Request::Reload { tab });
    }
    pub fn stop(&self, tab: u64) {
        self.send(Request::Stop { tab });
    }
    pub fn resize(&self, tab: u64, width: u32, height: u32, scale: f32) {
        self.send(Request::Resize {
            tab,
            width,
            height,
            scale,
        });
    }
    pub fn set_visible(&self, tab: u64, visible: bool) {
        self.send(Request::Visible { tab, visible });
    }
    pub fn pointer(&self, tab: u64, event: Pointer, modifiers: u32) {
        self.send(Request::Pointer {
            tab,
            event,
            modifiers,
        });
    }
    pub fn key(&self, tab: u64, stroke: KeyStroke) {
        self.send(Request::Key { tab, stroke });
    }
    pub fn text(&self, tab: u64, text: String) {
        self.send(Request::Text { tab, text });
    }
    pub fn close(&self, tab: u64) {
        self.send(Request::Close { tab });
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.send(Request::Shutdown);
        if let Some(engine) = self.engine.take() {
            let _ = engine.join();
        }
    }
}

/// A tab's page: its DevTools target and session, and its viewport.
struct Page {
    target: String,
    session: String,
    viewport: Option<(u32, u32, f32)>,
    visible: bool,
    casting: bool,
}

/// The headless browser process and its temporary profile.
struct Process {
    child: Child,
    profile: PathBuf,
}

impl Drop for Process {
    fn drop(&mut self) {
        // The launcher script may have forked the browser: its group goes.
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

fn pipe() -> Result<(RawFd, RawFd), String> {
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err("Could not open the preview pipe".into());
    }
    Ok((fds[0], fds[1]))
}

/// Starts the browser; returns it, the pipe it reads commands from and
/// the pipe it answers on.
fn launch() -> Result<(Process, File, File), String> {
    let binary = browser_binary().ok_or("No Chromium-based browser was found")?;
    let profile = std::env::temp_dir().join(format!("t3-code-rust-preview-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&profile);
    std::fs::create_dir_all(&profile).map_err(|_| "Could not create the preview profile")?;
    let (commands_read, commands_write) = pipe()?;
    let (answers_read, answers_write) = pipe()?;
    let mut command = Command::new(&binary);
    command
        .arg("--headless=new")
        .arg("--remote-debugging-pipe")
        .arg(format!("--user-data-dir={}", profile.display()))
        .args([
            "--no-first-run",
            "--no-default-browser-check",
            "--mute-audio",
            "--disable-background-networking",
            "--disable-sync",
            "about:blank",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(move || {
            // Its own process group, so the whole tree can be stopped.
            libc::setpgid(0, 0);
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            // The DevTools pipe: commands on fd 3, answers on fd 4. The ends
            // move above both first: one may already be fd 3 or 4, where
            // dup2 would keep its close-on-exec flag (or clobber the other).
            let commands = libc::fcntl(commands_read, libc::F_DUPFD, 10);
            let answers = libc::fcntl(answers_write, libc::F_DUPFD, 10);
            if commands < 0
                || answers < 0
                || libc::dup2(commands, 3) < 0
                || libc::dup2(answers, 4) < 0
            {
                return Err(std::io::Error::last_os_error());
            }
            libc::close(commands);
            libc::close(answers);
            Ok(())
        });
    }
    let child = command.spawn();
    unsafe {
        libc::close(commands_read);
        libc::close(answers_write);
    }
    let child = child.map_err(|_| format!("Could not start {}", binary.display()))?;
    let (commands, answers) = unsafe {
        (
            File::from_raw_fd(commands_write),
            File::from_raw_fd(answers_read),
        )
    };
    Ok((Process { child, profile }, commands, answers))
}

/// A DevTools connection: requests by id and their purpose.
struct Devtools {
    pipe: File,
    next_id: u64,
    pending: HashMap<u64, Pending>,
}

enum Pending {
    Created {
        tab: u64,
        url: Option<String>,
    },
    Attached {
        tab: u64,
        target: String,
        url: Option<String>,
    },
    History {
        tab: u64,
        offset: i64,
    },
    /// A still of a page whose screencast just started: Chromium only casts
    /// frames it paints from then on, so a page already painted sends none.
    Still {
        tab: u64,
        seq: u64,
    },
    Ignore,
}

impl Devtools {
    fn send(
        &mut self,
        session: Option<&str>,
        method: &str,
        params: Value,
        pending: Pending,
    ) -> Result<(), String> {
        self.next_id += 1;
        let mut message = json!({"id": self.next_id, "method": method, "params": params});
        if let Some(session) = session {
            message["sessionId"] = json!(session);
        }
        self.pending.insert(self.next_id, pending);
        let mut bytes = message.to_string().into_bytes();
        bytes.push(0);
        self.pipe
            .write_all(&bytes)
            .map_err(|_| "The preview browser closed".to_owned())
    }
}

fn run(
    inbox: Receiver<Input>,
    browser: Sender<Input>,
    tabs: &Mutex<HashMap<u64, TabState>>,
    ctx: &egui::Context,
) -> Result<(), String> {
    let (_process, pipe, answers) = launch()?;
    // The browser's messages, one per NUL-terminated chunk.
    std::thread::spawn(move || {
        let mut reader = BufReader::new(answers);
        let mut chunk = Vec::new();
        loop {
            chunk.clear();
            match reader.read_until(0, &mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    chunk.pop_if(|b| *b == 0);
                    let text = String::from_utf8_lossy(&chunk).into_owned();
                    if browser.send(Input::Browser(text)).is_err() {
                        return;
                    }
                }
            }
        }
        let _ = browser.send(Input::Closed);
    });
    let mut devtools = Devtools {
        pipe,
        next_id: 0,
        pending: HashMap::new(),
    };
    devtools.send(
        None,
        "Target.setDiscoverTargets",
        json!({"discover": true}),
        Pending::Ignore,
    )?;
    let mut pages: HashMap<u64, Page> = HashMap::new();
    let update = |tab: u64, f: &mut dyn FnMut(&mut TabState)| {
        if let Ok(mut tabs) = tabs.lock() {
            f(tabs.entry(tab).or_default());
        }
        ctx.request_repaint();
    };
    loop {
        let message = match inbox.recv() {
            Ok(Input::Browser(text)) => text,
            Ok(Input::Closed) => return Err("The preview browser closed".into()),
            Ok(Input::Panel(Request::Shutdown)) | Err(_) => return Ok(()),
            // Requests from the panel.
            Ok(Input::Panel(command)) => {
                let session_of = |pages: &HashMap<u64, Page>, tab: u64| {
                    pages.get(&tab).map(|p| p.session.clone())
                };
                match command {
                    Request::Open { tab, url } => {
                        update(tab, &mut |state| state.loading = url.is_some());
                        devtools.send(
                            None,
                            "Target.createTarget",
                            json!({"url": "about:blank"}),
                            Pending::Created { tab, url },
                        )?;
                    }
                    Request::Navigate { tab, url } => {
                        if let Some(session) = session_of(&pages, tab) {
                            update(tab, &mut |state| {
                                state.loading = true;
                                state.error = None;
                            });
                            devtools.send(
                                Some(&session),
                                "Page.navigate",
                                json!({"url": url}),
                                Pending::Ignore,
                            )?;
                        }
                    }
                    Request::History { tab, offset } => {
                        if let Some(session) = session_of(&pages, tab) {
                            devtools.send(
                                Some(&session),
                                "Page.getNavigationHistory",
                                json!({}),
                                Pending::History { tab, offset },
                            )?;
                        }
                    }
                    Request::Reload { tab } => {
                        if let Some(session) = session_of(&pages, tab) {
                            devtools.send(
                                Some(&session),
                                "Page.reload",
                                json!({}),
                                Pending::Ignore,
                            )?;
                        }
                    }
                    Request::Stop { tab } => {
                        if let Some(session) = session_of(&pages, tab) {
                            devtools.send(
                                Some(&session),
                                "Page.stopLoading",
                                json!({}),
                                Pending::Ignore,
                            )?;
                        }
                    }
                    Request::Resize {
                        tab,
                        width,
                        height,
                        scale,
                    } => {
                        if let Some(page) = pages.get_mut(&tab) {
                            if page.viewport == Some((width, height, scale)) {
                                continue;
                            }
                            page.viewport = Some((width, height, scale));
                            let session = page.session.clone();
                            devtools.send(
                            Some(&session),
                            "Emulation.setDeviceMetricsOverride",
                            json!({"width": width, "height": height, "deviceScaleFactor": scale, "mobile": false}),
                            Pending::Ignore,
                        )?;
                            cast(&mut devtools, page, true)?;
                            still(&mut devtools, page, tab, tabs)?;
                        }
                    }
                    Request::Visible { tab, visible } => {
                        if let Some(page) = pages.get_mut(&tab)
                            && page.visible != visible
                        {
                            page.visible = visible;
                            cast(&mut devtools, page, visible)?;
                            if visible {
                                still(&mut devtools, page, tab, tabs)?;
                            }
                        }
                    }
                    Request::Pointer {
                        tab,
                        event,
                        modifiers,
                    } => {
                        if let Some(session) = session_of(&pages, tab) {
                            let params = match event {
                                Pointer::Move { x, y } => {
                                    json!({"type": "mouseMoved", "x": x, "y": y, "modifiers": modifiers})
                                }
                                Pointer::Press {
                                    x,
                                    y,
                                    button,
                                    clicks,
                                } => {
                                    json!({"type": "mousePressed", "x": x, "y": y, "button": button, "clickCount": clicks, "modifiers": modifiers})
                                }
                                Pointer::Release {
                                    x,
                                    y,
                                    button,
                                    clicks,
                                } => {
                                    json!({"type": "mouseReleased", "x": x, "y": y, "button": button, "clickCount": clicks, "modifiers": modifiers})
                                }
                                Pointer::Wheel { x, y, dx, dy } => {
                                    json!({"type": "mouseWheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy, "modifiers": modifiers})
                                }
                            };
                            devtools.send(
                                Some(&session),
                                "Input.dispatchMouseEvent",
                                params,
                                Pending::Ignore,
                            )?;
                        }
                    }
                    Request::Key { tab, stroke } => {
                        if let Some(session) = session_of(&pages, tab) {
                            let kind = match (stroke.pressed, stroke.text.is_some()) {
                                (true, true) => "keyDown",
                                (true, false) => "rawKeyDown",
                                (false, _) => "keyUp",
                            };
                            let mut params = json!({
                                "type": kind,
                                "key": stroke.key,
                                "code": stroke.code,
                                "windowsVirtualKeyCode": stroke.virtual_key,
                                "nativeVirtualKeyCode": stroke.virtual_key,
                                "modifiers": stroke.modifiers,
                            });
                            if let (true, Some(text)) = (stroke.pressed, &stroke.text) {
                                params["text"] = json!(text);
                                params["unmodifiedText"] = json!(text);
                            }
                            devtools.send(
                                Some(&session),
                                "Input.dispatchKeyEvent",
                                params,
                                Pending::Ignore,
                            )?;
                        }
                    }
                    Request::Text { tab, text } => {
                        if let Some(session) = session_of(&pages, tab) {
                            devtools.send(
                                Some(&session),
                                "Input.insertText",
                                json!({"text": text}),
                                Pending::Ignore,
                            )?;
                        }
                    }
                    Request::Close { tab } => {
                        if let Some(page) = pages.remove(&tab) {
                            devtools.send(
                                None,
                                "Target.closeTarget",
                                json!({"targetId": page.target}),
                                Pending::Ignore,
                            )?;
                        }
                        if let Ok(mut tabs) = tabs.lock() {
                            tabs.remove(&tab);
                        }
                    }
                    Request::Shutdown => {}
                }
                continue;
            }
        };
        let Ok(message) = serde_json::from_str::<Value>(&message) else {
            continue;
        };
        if let Some(id) = message["id"].as_u64() {
            let result = &message["result"];
            match devtools.pending.remove(&id) {
                Some(Pending::Still { tab, seq }) => {
                    let Some(image) = result["data"].as_str().and_then(decode_frame) else {
                        continue;
                    };
                    let mut image = Some(Arc::new(image));
                    update(tab, &mut |state| {
                        // A cast frame that came first is newer.
                        if state.frame_seq == seq {
                            state.frame = image.take();
                            state.frame_seq += 1;
                        }
                    });
                }
                Some(Pending::Created { tab, url }) => {
                    let Some(target) = result["targetId"].as_str() else {
                        update(tab, &mut |state| {
                            state.error = Some("Could not open the page".into())
                        });
                        continue;
                    };
                    devtools.send(
                        None,
                        "Target.attachToTarget",
                        json!({"targetId": target, "flatten": true}),
                        Pending::Attached {
                            tab,
                            target: target.to_owned(),
                            url,
                        },
                    )?;
                }
                Some(Pending::Attached { tab, target, url }) => {
                    let Some(session) = result["sessionId"].as_str().map(str::to_owned) else {
                        continue;
                    };
                    devtools.send(Some(&session), "Page.enable", json!({}), Pending::Ignore)?;
                    if let Some(url) = url {
                        devtools.send(
                            Some(&session),
                            "Page.navigate",
                            json!({"url": url}),
                            Pending::Ignore,
                        )?;
                    }
                    pages.insert(
                        tab,
                        Page {
                            target,
                            session,
                            viewport: None,
                            visible: true,
                            casting: false,
                        },
                    );
                }
                Some(Pending::History { tab, offset }) => {
                    let current = result["currentIndex"].as_i64().unwrap_or(0);
                    let entries = result["entries"].as_array().cloned().unwrap_or_default();
                    if let (Some(entry), Some(page)) = (
                        usize::try_from(current + offset)
                            .ok()
                            .and_then(|i| entries.get(i)),
                        pages.get(&tab),
                    ) {
                        if offset == 0 {
                            // The blank page a tab starts on is not history.
                            let back = current > 0
                                && entries[current as usize - 1]["url"] != "about:blank";
                            let forward = (current as usize) + 1 < entries.len();
                            let url = entry["url"].as_str().unwrap_or_default().to_owned();
                            // A page without a title reports its URL.
                            let title = entry["title"]
                                .as_str()
                                .filter(|t| *t != url)
                                .unwrap_or_default()
                                .to_owned();
                            update(tab, &mut |state| {
                                state.can_go_back = back;
                                state.can_go_forward = forward;
                                state.title = title.clone();
                            });
                        } else {
                            let session = page.session.clone();
                            devtools.send(
                                Some(&session),
                                "Page.navigateToHistoryEntry",
                                json!({"entryId": entry["id"]}),
                                Pending::Ignore,
                            )?;
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let method = message["method"].as_str().unwrap_or_default();
        let params = &message["params"];
        let session = message["sessionId"].as_str();
        let tab_of = |session: Option<&str>| {
            pages
                .iter()
                .find(|(_, page)| Some(page.session.as_str()) == session)
                .map(|(tab, _)| *tab)
        };
        match method {
            "Page.screencastFrame" => {
                let Some(tab) = tab_of(session) else {
                    continue;
                };
                let ack = params["sessionId"].clone();
                devtools.send(
                    session,
                    "Page.screencastFrameAck",
                    json!({"sessionId": ack}),
                    Pending::Ignore,
                )?;
                if let Some(image) = params["data"].as_str().and_then(decode_frame) {
                    let mut image = Some(Arc::new(image));
                    update(tab, &mut |state| {
                        state.frame = image.take();
                        state.frame_seq += 1;
                    });
                }
            }
            "Page.frameNavigated" if params["frame"]["parentId"].is_null() => {
                let Some(tab) = tab_of(session) else {
                    continue;
                };
                let url = params["frame"]["url"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                update(tab, &mut |state| state.url = url.clone());
                if let Some(session) = session {
                    devtools.send(
                        Some(session),
                        "Page.getNavigationHistory",
                        json!({}),
                        Pending::History { tab, offset: 0 },
                    )?;
                }
            }
            "Page.frameStartedLoading" | "Page.frameStoppedLoading" => {
                let Some((tab, page)) = pages
                    .iter()
                    .find(|(_, p)| Some(p.session.as_str()) == session)
                else {
                    continue;
                };
                if params["frameId"].as_str() == Some(page.target.as_str()) {
                    let loading = method == "Page.frameStartedLoading";
                    update(*tab, &mut |state| state.loading = loading);
                    // A page that painted before its cast began shows too.
                    if !loading {
                        still(&mut devtools, page, *tab, tabs)?;
                    }
                }
            }
            "Page.loadEventFired" | "Page.domContentEventFired" => {
                if let Some(tab) = tab_of(session) {
                    devtools.send(
                        session,
                        "Page.getNavigationHistory",
                        json!({}),
                        Pending::History { tab, offset: 0 },
                    )?;
                }
            }
            "Page.navigatedWithinDocument" => {
                let Some(tab) = tab_of(session) else {
                    continue;
                };
                let url = params["url"].as_str().unwrap_or_default().to_owned();
                update(tab, &mut |state| state.url = url.clone());
            }
            "Target.targetInfoChanged" => {
                let info = &params["targetInfo"];
                if let Some((tab, _)) = pages
                    .iter()
                    .find(|(_, p)| info["targetId"].as_str() == Some(p.target.as_str()))
                {
                    let title = info["title"].as_str().unwrap_or_default().to_owned();
                    let url = info["url"].as_str().unwrap_or_default().to_owned();
                    update(*tab, &mut |state| {
                        // A page without a title reports its URL.
                        state.title = if title == url {
                            String::new()
                        } else {
                            title.clone()
                        };
                    });
                }
            }
            "Inspector.targetCrashed" => {
                if let Some(tab) = tab_of(session) {
                    update(tab, &mut |state| {
                        state.error = Some("The page crashed".into())
                    });
                }
            }
            _ => {}
        }
    }
}

/// Starts or stops a page's screencast at its viewport size.
/// A JPEG frame from the browser, base64-encoded, as an image.
fn decode_frame(data: &str) -> Option<egui::ColorImage> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    let rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

/// Asks for a still of a page that started casting (`Pending::Still`).
fn still(
    devtools: &mut Devtools,
    page: &Page,
    tab: u64,
    tabs: &Mutex<HashMap<u64, TabState>>,
) -> Result<(), String> {
    if !page.casting {
        return Ok(());
    }
    let seq = tabs
        .lock()
        .ok()
        .and_then(|tabs| tabs.get(&tab).map(|state| state.frame_seq))
        .unwrap_or(0);
    devtools.send(
        Some(&page.session),
        "Page.captureScreenshot",
        json!({"format": "jpeg", "quality": 90}),
        Pending::Still { tab, seq },
    )
}

fn cast(devtools: &mut Devtools, page: &mut Page, on: bool) -> Result<(), String> {
    let session = page.session.clone();
    if page.casting {
        devtools.send(
            Some(&session),
            "Page.stopScreencast",
            json!({}),
            Pending::Ignore,
        )?;
        page.casting = false;
    }
    if on
        && page.visible
        && let Some((width, height, scale)) = page.viewport
    {
        devtools.send(
            Some(&session),
            "Page.startScreencast",
            json!({
                "format": "jpeg",
                "quality": 90,
                "maxWidth": (width as f32 * scale).round() as u32,
                "maxHeight": (height as f32 * scale).round() as u32,
                "everyNthFrame": 1,
            }),
            Pending::Ignore,
        )?;
        page.casting = true;
    }
    Ok(())
}
