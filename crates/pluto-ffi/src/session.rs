//! The phone's session with a Pluto server: the core connection, the state
//! its events fill, one-shot replies by key, the scheduled tasks stream, and
//! the connection's health. A session lost after it was established
//! reconnects on its own, three times, before it shows "Can't reach the
//! server" with Retry; a remembered server that does not answer at launch
//! shows "Can't reach your server". Commands only go out once the server
//! synchronized; a reconnected session follows the open thread and the
//! scheduled tasks again. A message the server refused (or whose fate the
//! connection lost) comes back to its composer. Temporary chats the phone
//! opened are deleted once it leaves them (`chat_mode.rs`'s sweep).
//!
//! The connection's events arrive on a pump thread, which applies them under
//! the hub's lock and tells the interface something changed.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak, mpsc};
use std::time::Duration;

use pluto_core::model::{Project, State, Thread};
use pluto_core::protocol::{Action, Connection, Event, Route};
use pluto_core::spaces::{Space, space_of};
use pluto_core::timeline::Expansion;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::Hub;
use crate::attachments::Attached;
use crate::prefs::Prefs;

/// Reconnection attempts before the session gives up (`#lost`).
pub const ATTEMPTS: u8 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(1500);
pub const NOT_CONNECTED: &str = "Not connected to the server: nothing was sent.";

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Net {
    /// No session yet: the connection screen, with the last failure.
    Disconnected {
        error: Option<String>,
    },
    /// The first connection of this server is on its way.
    Connecting,
    Connected,
    /// Lost after it was established: this attempt is on its way.
    Reconnecting {
        attempt: u8,
    },
    /// Every attempt failed; Retry starts over.
    Offline,
    /// Just reconnected; the bar says so for a moment.
    Back,
    /// A remembered server did not answer at launch.
    Unreachable,
}

impl Net {
    /// Whether the session's data is on screen (as opposed to the
    /// connection screens).
    pub fn in_session(&self) -> bool {
        matches!(
            self,
            Net::Connected | Net::Reconnecting { .. } | Net::Offline | Net::Back
        )
    }

    /// Whether commands can go out: Send, Stop and the request cards wait
    /// for the server otherwise.
    pub fn online(&self) -> bool {
        matches!(self, Net::Connected | Net::Back)
    }
}

/// An image the server serves, by key (`favicon:{project}`,
/// `attachment:{id}`).
#[derive(Clone)]
pub enum Asset {
    /// Asked for, or found missing for this session.
    Pending,
    Missing,
    /// Raster bytes (an SVG arrives rendered as PNG).
    Ready(Arc<Vec<u8>>),
    /// The last try failed then: the next one waits a little (the screens
    /// ask on every frame they show it).
    Failed(std::time::Instant),
}

/// How long a failed image waits before it is asked for again.
const ASSET_RETRY: Duration = Duration::from_secs(20);

pub struct Session {
    pub state: State,
    pub net: Net,
    /// The server's address and the token, kept in memory for reconnecting.
    pub server: String,
    token: Option<Zeroizing<String>>,
    connection: Option<Connection>,
    /// The connection synchronized (its config arrived): commands may go.
    synced: bool,
    /// The connection that reached the server at least once.
    established: bool,
    /// A launch from the remembered server: `Some(true)` once it was
    /// unreachable, so a retry stays on that page.
    pub launch: Option<bool>,
    /// The connection the events come from; older ones are ignored.
    generation: u64,
    /// The Chat space (else Code).
    pub chat: bool,
    pub selected: Option<String>,
    pub expansion: Expansion,
    /// The shell's first snapshot arrived.
    pub listed: bool,
    /// One-shot replies by request key.
    pub replies: HashMap<String, Result<Value, String>>,
    /// Requests and tracked commands (`command:{id}`) still waiting for
    /// their reply.
    pending: HashSet<String>,
    /// Messages on their way, by request key or command id: their composer
    /// and text, given back if the server refuses them.
    sends: HashMap<String, (String, String, Vec<Attached>)>,
    /// Messages waiting for their images to be stored
    /// (`persist:{message id}`), with their key in `sends`.
    persisting: HashMap<String, (Held, String)>,
    /// Settings changed here, before the change, until the server answers.
    setting_backups: HashMap<String, Value>,
    /// Each composer's attachments (`attachments.rs`).
    pub attachments: HashMap<String, Vec<Attached>>,
    /// Texts given back to their composer (by composer key).
    pub unsent: HashMap<String, Vec<String>>,
    /// The scheduled tasks (`scheduledTasks.subscribe`), once listed.
    pub tasks: Option<Result<Vec<Value>, String>>,
    tasks_streaming: bool,
    pub prefs: Prefs,
    /// A short message at the top of the screen, with its serial.
    pub toast: Option<(String, u64)>,
    serial: u64,
    /// The open thread to follow again once the shell lists it.
    resubscribe: Option<String>,
    /// The last visit sent (`thread.visit`), by thread and time.
    pub visited: Option<(String, String)>,
    /// Why the last connection failed, for the unreachable screen.
    pub last_error: Option<String>,
    /// Why the open thread could not be followed, by thread id.
    pub thread_error: Option<(String, String)>,
    pub assets: HashMap<String, Asset>,
    /// The images Codex generated for a thread (`generatedImages.list`),
    /// placed in its timeline like the desktop's local scan.
    pub generated: HashMap<String, Vec<pluto_core::generated_images::GeneratedImage>>,
    /// Temporary chats opened here, and those already sent `thread.delete`
    /// on this connection.
    temporary: HashSet<String>,
    deleted: HashSet<String>,
}

impl Session {
    pub fn new(prefs: Prefs) -> Self {
        // The temporary chats this phone left before it closed, still to
        // delete.
        let temporary = prefs
            .get("temporaryChats")
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|id| id.as_str().map(str::to_owned))
            .collect();
        Self {
            state: State::default(),
            net: Net::Disconnected { error: None },
            server: String::new(),
            token: None,
            connection: None,
            synced: false,
            established: false,
            launch: None,
            generation: 0,
            chat: true,
            selected: None,
            expansion: Expansion::default(),
            listed: false,
            replies: HashMap::new(),
            pending: HashSet::new(),
            sends: HashMap::new(),
            persisting: HashMap::new(),
            attachments: HashMap::new(),
            setting_backups: HashMap::new(),
            unsent: HashMap::new(),
            tasks: None,
            tasks_streaming: false,
            prefs,
            toast: None,
            serial: 0,
            resubscribe: None,
            last_error: None,
            visited: None,
            thread_error: None,
            assets: HashMap::new(),
            generated: HashMap::new(),
            temporary,
            deleted: HashSet::new(),
        }
    }

    pub fn scratch_root(&self) -> Option<&str> {
        self.state.config["scratchWorkspaceRoot"].as_str()
    }

    pub fn project_space(&self, project: &Project) -> Space {
        space_of(self.scratch_root(), &project.workspace_root)
    }

    pub fn space(&self, thread: &Thread) -> Space {
        self.state
            .project(&thread.project_id)
            .map(|p| self.project_space(p))
            .unwrap_or(Space::Code)
    }

    /// The hidden project of `space` (the chat space or temporary chats).
    pub fn space_project(&self, space: Space) -> Option<&Project> {
        self.state
            .projects
            .iter()
            .find(|p| self.project_space(p) == space)
    }

    /// The code projects, by title.
    pub fn code_projects(&self) -> Vec<&Project> {
        let mut projects: Vec<&Project> = self
            .state
            .projects
            .iter()
            .filter(|p| self.project_space(p) == Space::Code)
            .collect();
        projects.sort_by_key(|p| p.title.to_lowercase());
        projects
    }

    /// The threads a space lists, the latest first; temporary chats and
    /// subagents are listed nowhere.
    pub fn listed(&self, chat: bool) -> Vec<Arc<Thread>> {
        let mut threads: Vec<_> = self
            .state
            .threads
            .iter()
            .filter(|t| t.archived_at.is_none() && !t.is_subagent())
            .filter(|t| {
                let space = self.space(t);
                space != Space::Temporary && space.is_chat() == chat
            })
            .cloned()
            .collect();
        threads.sort_by(|a, b| last_activity(b).cmp(last_activity(a)));
        threads
    }

    pub fn thread(&self, id: &str) -> Option<Arc<Thread>> {
        self.state.threads.iter().find(|t| t.id == id).cloned()
    }

    pub fn providers(&self) -> &[Value] {
        self.state.config["providers"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn settings(&self) -> &Value {
        &self.state.config["settings"]
    }

    /// The host and port the session talks to, for the status lines.
    pub fn host(&self) -> String {
        url_host(&self.server)
    }

    pub fn show_toast(&mut self, text: impl Into<String>) {
        self.serial += 1;
        self.toast = Some((text.into(), self.serial));
    }

    /// Whether the request `key` (or `command:{id}`) still waits.
    pub fn waiting(&self, key: &str) -> bool {
        self.pending.contains(key)
    }

    /// The live subagent below `parent` that waits on the user
    /// (`refresh_subagent_requests`'s walk, from the child up).
    pub fn pending_subagent(&self, parent: &str) -> Option<Arc<Thread>> {
        self.state
            .threads
            .iter()
            .filter(|t| t.is_subagent() && !t.is_provider_native_subagent() && t.child_run_live())
            .filter(|t| {
                t.pending_runtime_request
                    .as_ref()
                    .is_some_and(|r| r.kind != "auth_refresh")
            })
            .find(|child| {
                let mut seen = vec![child.id.clone()];
                let mut current = child.lineage.parent_thread_id.clone();
                while let Some(id) = current {
                    if id == parent {
                        return true;
                    }
                    if seen.contains(&id) {
                        return false;
                    }
                    seen.push(id.clone());
                    current = self
                        .thread(&id)
                        .filter(|t| t.is_subagent())
                        .and_then(|t| t.lineage.parent_thread_id.clone());
                }
                false
            })
            .cloned()
    }

    /// Keeps the temporary chats still to delete in the phone's settings,
    /// so a chat left when the app was killed goes on the next connection.
    fn keep_temporary(&mut self) {
        let mut ids: Vec<&String> = self.temporary.iter().collect();
        ids.sort();
        let ids: Vec<&String> = ids.into_iter().rev().take(50).collect();
        let value = json!(ids);
        if *self.prefs.get("temporaryChats") != value {
            self.prefs.set("temporaryChats", value);
        }
    }

    fn is_temporary(&self, id: &str) -> bool {
        self.thread(id)
            .is_some_and(|t| self.space(&t) == Space::Temporary)
    }

    /// Hands every message on its way back to its composer.
    fn give_back_sends(&mut self) {
        self.persisting.clear();
        for (_, (composer, text, attachments)) in std::mem::take(&mut self.sends) {
            self.give_back(composer, text, attachments);
        }
    }

    /// A message the server did not take: its text and attachments go back
    /// to its composer.
    fn give_back(&mut self, composer: String, text: String, attachments: Vec<Attached>) {
        // Added to what the composer holds since.
        if !attachments.is_empty() {
            self.attachments
                .entry(composer.clone())
                .or_default()
                .extend(attachments);
        }
        if !text.trim().is_empty() {
            self.unsent.entry(composer).or_default().push(text);
        }
    }

    /// Sends `action` once the server synchronized; false without a
    /// synchronized connection.
    pub fn send(&self, action: Action) -> bool {
        match &self.connection {
            Some(connection) if self.synced => {
                connection.send(action);
                true
            }
            _ => false,
        }
    }

    /// Loads the open thread's earlier turns above.
    pub fn load_earlier(&mut self, thread_id: &str) {
        let cursor = self.state.begin_history_load(thread_id);
        if let (Some(cursor), Some(connection)) = (cursor, &self.connection)
            && self.synced
        {
            connection.load_history(thread_id, &cursor, None);
        }
    }

    /// Connects to `server` with `token`; a launch from remembered
    /// credentials shows the unreachable page when it fails.
    pub fn connect(
        &mut self,
        hub: &Arc<Hub>,
        server: String,
        token: Zeroizing<String>,
        remembered: bool,
    ) {
        self.server = server;
        self.token = Some(token);
        self.established = false;
        self.launch = remembered.then_some(false);
        self.net = Net::Connecting;
        self.open(hub, remembered);
    }

    /// Opens a connection with the session's credentials.
    fn open(&mut self, hub: &Arc<Hub>, remembered: bool) {
        self.generation += 1;
        self.connection = None;
        self.synced = false;
        let Some(token) = self.token.clone() else {
            return;
        };
        let generation = self.generation;
        let (wake, woken) = mpsc::channel::<()>();
        let repaint = pluto_core::Repaint::new(move |_| {
            let _ = wake.send(());
        });
        match Connection::open(&self.server, token, repaint) {
            Ok(connection) => self.connection = Some(connection),
            Err(error) => {
                self.failed(hub, error, remembered);
                return;
            }
        }
        // A reconnected session follows the open thread again once the shell
        // lists it (commands sent before the server synchronized would end
        // the connection).
        self.resubscribe = self.selected.clone();
        let hub = Arc::downgrade(hub);
        std::thread::Builder::new()
            .name("pluto-session".into())
            .spawn(move || pump(hub, woken, generation, remembered))
            .ok();
    }

    /// Applies the connection's waiting events; true once it ended.
    fn drain(&mut self, hub: &Arc<Hub>, remembered: bool) -> bool {
        let mut events = Vec::new();
        if let Some(connection) = &self.connection {
            while let Ok(event) = connection.events.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            if let Event::Disconnected(error) = event {
                self.connection = None;
                self.synced = false;
                self.failed(hub, error, remembered);
                return true;
            }
            // The server's config comes once it synchronized: commands may
            // go out from then on.
            if matches!(event, Event::Data(Route::Config, _)) {
                self.synced = true;
            }
            self.apply(hub, event);
        }
        false
    }

    /// The connection ended or never started.
    fn failed(&mut self, hub: &Arc<Hub>, error: String, remembered: bool) {
        self.last_error = Some(error.clone());
        if !self.sends.is_empty() {
            self.give_back_sends();
            self.show_toast(
                "The connection dropped while sending: your message is back in the composer. Check the thread before sending it again.",
            );
        }
        // Whatever still waited will not hear back from this connection.
        for key in std::mem::take(&mut self.pending) {
            self.replies.insert(
                key,
                Err("The connection dropped before the server answered.".into()),
            );
        }
        if !self.established {
            // A rejected token is not an unreachable server.
            if remembered && !error.contains("401") {
                self.net = Net::Unreachable;
                self.launch = Some(true);
            } else {
                self.net = Net::Disconnected { error: Some(error) };
            }
            return;
        }
        let attempt = match self.net {
            Net::Reconnecting { attempt } => attempt + 1,
            _ => 1,
        };
        if attempt > ATTEMPTS {
            self.net = Net::Offline;
            return;
        }
        self.net = Net::Reconnecting { attempt };
        Hub::later(hub, RETRY_DELAY, move |app, hub| {
            if app.session.net == (Net::Reconnecting { attempt }) {
                app.session.open(hub, false);
            }
        });
    }

    /// Retry after the attempts ran out, or from the unreachable page.
    pub fn retry(&mut self, hub: &Arc<Hub>) {
        self.net = if self.established {
            Net::Reconnecting { attempt: 1 }
        } else {
            Net::Connecting
        };
        let remembered = !self.established;
        self.open(hub, remembered);
    }

    /// Leaves the server for the connection screen.
    pub fn disconnect(&mut self) {
        // The generation keeps counting: a late event of the old connection
        // stays out.
        let (prefs, generation) = (self.prefs.clone(), self.generation + 1);
        *self = Session::new(prefs);
        self.generation = generation;
    }

    /// Checks a session that may have died while the phone slept: the
    /// server must answer at once, else it reconnects. Its settings come
    /// back as `probe` once it answered.
    pub fn probe(&mut self, hub: &Arc<Hub>) {
        match self.net {
            Net::Offline => self.retry(hub),
            _ if self.net.online() => {
                self.send(Action::Ping);
                self.request("probe", "server.getSettings", json!({}));
            }
            _ => {}
        }
    }

    fn apply(&mut self, hub: &Arc<Hub>, event: Event) {
        match event {
            Event::Data(Route::Config, value) => {
                self.last_error = None;
                self.state.config_event(&value);
                if !self.net.online() {
                    let back = self.established;
                    self.net = if back { Net::Back } else { Net::Connected };
                    self.established = true;
                    // A new connection deletes again what the last one may
                    // not have.
                    self.deleted.clear();
                    // The tasks the last connection followed.
                    if self.tasks_streaming {
                        self.send(Action::Stream {
                            key: "scheduled-tasks".into(),
                            method: "scheduledTasks.subscribe",
                            payload: json!({}),
                        });
                    }
                    if back {
                        // "Reconnected" stays for a moment, then the bar
                        // leaves.
                        Hub::later(hub, Duration::from_millis(1400), |app, _| {
                            if app.session.net == Net::Back {
                                app.session.net = Net::Connected;
                            }
                        });
                    }
                }
            }
            Event::Data(Route::Shell, value) => {
                match self.state.shell(&value) {
                    Ok(()) => self.listed = true,
                    Err(error) => self.show_toast(error),
                }
                if let Some(id) = self.resubscribe.clone()
                    && self.thread(&id).is_some()
                {
                    self.resubscribe = None;
                    self.thread_error = None;
                    self.send(Action::Select(Some(id)));
                }
                if let Some(id) = self.selected.clone()
                    && self.is_temporary(&id)
                {
                    self.temporary.insert(id);
                    self.keep_temporary();
                }
                self.sweep_temporary();
            }
            Event::Data(Route::Thread(id), value) => {
                if let Err(error) = self.state.thread(&id, &value) {
                    self.show_toast(error);
                }
            }
            Event::Data(
                Route::History {
                    thread_id,
                    cursor,
                    complete,
                },
                value,
            ) => {
                if let Err(error) = self
                    .state
                    .history_page(&thread_id, &cursor, &value, complete)
                {
                    self.state.history_failed(&thread_id, &cursor, error);
                }
            }
            Event::Failed(
                Route::History {
                    thread_id, cursor, ..
                },
                message,
            ) => {
                self.state.history_failed(&thread_id, &cursor, message);
            }
            // A new thread can be asked for before the server created it:
            // it is followed again once the shell lists it.
            Event::Failed(Route::Thread(id), message)
                if self.selected.as_deref() == Some(id.as_str()) =>
            {
                self.thread_error = Some((id.clone(), message));
                self.resubscribe = Some(id);
            }
            Event::Data(Route::Rpc(key), value) if key.starts_with("asset:") => {
                self.fetch_asset(hub, &key["asset:".len()..], &value);
            }
            Event::Failed(Route::Rpc(key), _) if key.starts_with("asset:") => {
                // A failure is tried again a little later.
                self.assets.insert(
                    key["asset:".len()..].to_owned(),
                    Asset::Failed(std::time::Instant::now()),
                );
            }
            Event::Data(Route::Rpc(key), value) if key.starts_with("file-upload:") => {
                self.pending.remove(&key);
                self.upload_target(hub, &key, Ok(value));
            }
            Event::Failed(Route::Rpc(key), message) if key.starts_with("file-upload:") => {
                self.pending.remove(&key);
                self.upload_target(hub, &key, Err(message));
            }
            Event::Data(Route::Rpc(key), value) => self.rpc_result(key, Ok(value)),
            Event::Failed(Route::Rpc(key), message) => self.rpc_result(key, Err(message)),
            Event::Data(Route::Command(id), value) => {
                self.sends.remove(&id);
                let key = format!("command:{id}");
                if self.pending.remove(&key) {
                    self.replies.insert(key, Ok(value));
                }
            }
            Event::Failed(Route::Command(id), message) => {
                let key = format!("command:{id}");
                // A tracked command shows its failure where it was asked.
                if self.pending.remove(&key) {
                    self.replies.insert(key, Err(message));
                } else if let Some((composer, text, attachments)) = self.sends.remove(&id) {
                    self.give_back(composer, text, attachments);
                    self.show_toast(format!(
                        "Message not sent: {} Your text is back in the composer.",
                        sentence(&message)
                    ));
                } else {
                    self.show_toast(message);
                }
            }
            Event::Failed(_, message) => self.show_toast(message),
            Event::Data(..) | Event::Disconnected(_) => {}
        }
    }

    /// A project's favicon (`favicon.rs`): `assets.createUrl` with a
    /// `project-favicon` resource, once per project.
    pub fn favicon(&mut self, project_id: &str) -> Option<Arc<Vec<u8>>> {
        let project = self.state.project(project_id)?.clone();
        if project.project_icon.is_some() {
            return None;
        }
        let mut resource = json!({"_tag": "project-favicon", "cwd": project.workspace_root});
        if let Some(path) = &project.favicon_path {
            resource["path"] = json!(path);
        }
        self.asset(&format!("favicon:{}", project.id), resource)
    }

    /// A message's image attachment (`attachments.rs`).
    pub fn attachment(&mut self, id: &str) -> Option<Arc<Vec<u8>>> {
        self.asset(
            &format!("attachment:{id}"),
            json!({"_tag": "attachment", "attachmentId": id}),
        )
    }

    /// An image Codex generated for `thread`, by its path on the server's
    /// host (`media-file`).
    pub fn generated_image(&mut self, thread: &str, path: &str) -> Option<Arc<Vec<u8>>> {
        self.asset(
            &format!("generated:{path}"),
            json!({"_tag": "media-file", "threadId": thread, "path": path}),
        )
    }

    /// Asks again for the images Codex generated for `thread`, when it ran
    /// with Codex (the server lists them; an older one has no such list).
    pub fn watch_generated(&mut self, thread: &str) {
        let codex = self.state.detail.as_ref().is_some_and(|d| {
            d.id() == thread
                && d.provider_threads
                    .iter()
                    .any(|t| t["driver"] == "codex" && t["appThreadId"] == thread)
        });
        let key = format!("generated:{thread}");
        if !codex || !self.net.online() || self.waiting(&key) {
            return;
        }
        self.request(key, "generatedImages.list", json!({"threadId": thread}));
    }

    /// An image the server serves (`assets.createUrl`), asked for once.
    fn asset(&mut self, key: &str, resource: Value) -> Option<Arc<Vec<u8>>> {
        match self.assets.get(key) {
            Some(Asset::Ready(bytes)) => return Some(bytes.clone()),
            Some(Asset::Failed(at)) if at.elapsed() >= ASSET_RETRY => {}
            Some(_) => return None,
            None => {}
        }
        if !self.net.online() {
            return None;
        }
        self.assets.insert(key.to_owned(), Asset::Pending);
        self.send(Action::Request {
            key: format!("asset:{key}"),
            method: "assets.createUrl",
            payload: json!({"resource": resource}),
        });
        None
    }

    fn fetch_asset(&mut self, hub: &Arc<Hub>, key: &str, reply: &Value) {
        // Only from the server itself: an absolute URL elsewhere is refused.
        let url = reply["relativeUrl"].as_str().and_then(|relative| {
            let base = url::Url::parse(self.server.trim()).ok()?;
            base.join(relative)
                .ok()
                .filter(|url| url.origin() == base.origin())
        });
        // The server's marker for a project without a favicon keeps the
        // monogram for the session.
        let Some(url) = url.filter(|u| {
            u.path_segments()
                .and_then(|mut s| s.next_back())
                .is_some_and(|name| name != "project-favicon-missing")
        }) else {
            self.assets.insert(key.to_owned(), Asset::Missing);
            return;
        };
        let (key, generation) = (key.to_owned(), self.generation);
        let hub = Arc::downgrade(hub);
        std::thread::spawn(move || {
            let image = pluto_core::http::get(&url).and_then(|bytes| crate::images::raster(&bytes));
            let Some(hub) = hub.upgrade() else {
                return;
            };
            hub.update(|app, _| {
                // An image of an older connection or server stays out.
                if app.session.generation != generation {
                    return;
                }
                match image {
                    Some(bytes) => {
                        app.session
                            .assets
                            .insert(key, Asset::Ready(Arc::new(bytes)));
                    }
                    // A failed download is tried again a little later.
                    None => {
                        app.session
                            .assets
                            .insert(key, Asset::Failed(std::time::Instant::now()));
                    }
                }
            });
        });
    }

    /// Sends a command whose failure shows as a toast.
    pub fn dispatch(&mut self, command: Value) -> bool {
        let sent = self.send(Action::Dispatch(command));
        if !sent {
            self.show_toast(NOT_CONNECTED);
        }
        sent
    }

    /// Sends a command whose reply lands in `replies` under
    /// `command:{id}`; returns that key.
    pub fn dispatch_tracked(&mut self, command: Value) -> String {
        let key = format!(
            "command:{}",
            command["commandId"].as_str().unwrap_or_default()
        );
        self.replies.remove(&key);
        self.pending.insert(key.clone());
        if !self.send(Action::Dispatch(command)) {
            self.pending.remove(&key);
            self.replies.insert(key.clone(), Err(NOT_CONNECTED.into()));
        }
        key
    }

    /// Sends a message from `composer`: refused, it comes back there.
    pub fn send_message(&mut self, mut command: Value, composer: String, text: String) -> bool {
        let id = command["commandId"].as_str().unwrap_or_default().to_owned();
        let attachments = self.attachments.remove(&composer).unwrap_or_default();
        self.sends
            .insert(id.clone(), (composer.clone(), text, attachments));
        let uploads = take_uploads(&mut command["attachments"]);
        let sent = if uploads.is_empty() {
            self.dispatch(command)
        } else {
            self.persist(Held::Message(command), uploads, id.clone())
        };
        if !sent && let Some((_, _, attachments)) = self.sends.remove(&id) {
            self.attachments.insert(composer, attachments);
        }
        sent
    }

    /// Starts a thread from `composer`'s draft (`orchestration.launchThread`):
    /// refused, the text comes back there.
    pub fn launch(
        &mut self,
        thread_id: &str,
        input: Value,
        composer: String,
        text: String,
    ) -> bool {
        let key = format!("launch:{thread_id}");
        let attachments = self.attachments.remove(&composer).unwrap_or_default();
        self.sends
            .insert(key.clone(), (composer.clone(), text, attachments));
        let mut input = input;
        let uploads = take_uploads(&mut input["initialMessage"]["attachments"]);
        let refused = if uploads.is_empty() {
            self.request(key.clone(), "orchestration.launchThread", input);
            self.replies.contains_key(&key)
        } else {
            !self.persist(
                Held::Launch(thread_id.to_owned(), input),
                uploads,
                key.clone(),
            )
        };
        if refused && let Some((_, _, attachments)) = self.sends.remove(&key) {
            self.attachments.insert(composer, attachments);
        }
        !refused
    }

    /// Stores a message's images (`assets.persistChatAttachments`, as
    /// `startThreadTurn` does) before the message goes with what the server
    /// kept: a message cannot carry them inline. False when it could not ask.
    fn persist(&mut self, held: Held, uploads: Vec<Value>, sends_key: String) -> bool {
        let (thread, message) = match &held {
            Held::Message(command) => (command["threadId"].clone(), command["messageId"].clone()),
            Held::Launch(thread, input) => {
                (json!(thread), input["initialMessage"]["messageId"].clone())
            }
        };
        let key = format!("persist:{}", message.as_str().unwrap_or_default());
        self.request(
            key.clone(),
            "assets.persistChatAttachments",
            json!({"threadId": thread, "messageId": message, "attachments": uploads}),
        );
        if self.replies.remove(&key).is_some() {
            self.show_toast(NOT_CONNECTED);
            return false;
        }
        // The new thread's screen waits on its launch meanwhile.
        if let Held::Launch(thread, _) = &held {
            self.pending.insert(format!("launch:{thread}"));
        }
        self.persisting.insert(key, (held, sends_key));
        true
    }

    /// The images were stored (or refused): the message goes on with them.
    fn persisted(&mut self, held: Held, sends_key: String, result: Result<Value, String>) {
        let stored =
            result.map(|value| value["attachments"].as_array().cloned().unwrap_or_default());
        match (held, stored) {
            (Held::Message(mut command), Ok(stored)) => {
                add_attachments(&mut command["attachments"], stored);
                if !self.dispatch(command)
                    && let Some((composer, text, attachments)) = self.sends.remove(&sends_key)
                {
                    self.give_back(composer, text, attachments);
                }
            }
            (Held::Message(_), Err(error)) => {
                if let Some((composer, text, attachments)) = self.sends.remove(&sends_key) {
                    self.give_back(composer, text, attachments);
                }
                self.show_toast(format!(
                    "Message not sent: could not upload the images: {} Your text is back in the composer.",
                    sentence(&error)
                ));
            }
            (Held::Launch(_, mut input), Ok(stored)) => {
                add_attachments(&mut input["initialMessage"]["attachments"], stored);
                self.request(sends_key.clone(), "orchestration.launchThread", input);
                if let Some(Err(error)) = self.replies.remove(&sends_key) {
                    self.rpc_result(sends_key, Err(error));
                }
            }
            (Held::Launch(..), Err(error)) => {
                self.rpc_result(
                    sends_key,
                    Err(format!("could not upload the images: {error}")),
                );
            }
        }
    }

    /// A one-shot request; its reply lands in `replies` under `key`.
    pub fn request(&mut self, key: impl Into<String>, method: &'static str, payload: Value) {
        let key = key.into();
        self.replies.remove(&key);
        self.pending.insert(key.clone());
        let sent = self.send(Action::Request {
            key: key.clone(),
            method,
            payload,
        });
        if !sent {
            self.pending.remove(&key);
            if key.starts_with("mutation:") || key.starts_with("settings:") {
                self.show_toast(NOT_CONNECTED);
            }
            self.replies.insert(key, Err(NOT_CONNECTED.into()));
        }
    }

    /// Drops the replies under `prefix` (a closed picker's searches).
    pub fn forget(&mut self, prefix: &str) {
        self.replies.retain(|key, _| !key.starts_with(prefix));
    }

    /// A request whose reply is only checked for failure.
    pub fn mutate(&mut self, method: &'static str, payload: Value) {
        self.request(format!("mutation:{method}"), method, payload);
    }

    /// Follows the scheduled tasks once.
    pub fn watch_tasks(&mut self) {
        if self.tasks_streaming {
            return;
        }
        self.tasks_streaming = true;
        self.send(Action::Stream {
            key: "scheduled-tasks".into(),
            method: "scheduledTasks.subscribe",
            payload: json!({}),
        });
    }

    /// Opens `id` (or none), following its timeline.
    pub fn select(&mut self, id: Option<String>) {
        if self.selected == id {
            return;
        }
        self.state.switch_detail(id.as_deref());
        self.selected = id.clone();
        self.expansion = Expansion::default();
        self.thread_error = None;
        if let Some(id) = &id
            && self.is_temporary(id)
        {
            self.temporary.insert(id.clone());
            self.keep_temporary();
        }
        // Offline, the thread is followed once the server is back.
        let online = self.net.online();
        self.resubscribe = if online { None } else { id.clone() };
        if online {
            self.send(Action::Select(id));
        }
        self.sweep_temporary();
    }

    /// Follows the open thread again after it failed to load.
    pub fn follow_again(&mut self, id: &str) {
        self.thread_error = None;
        self.send(Action::Select(Some(id.to_owned())));
    }

    /// Marks a temporary chat started here, deleted once the phone leaves it.
    pub fn opened_temporary(&mut self, id: &str) {
        self.temporary.insert(id.to_owned());
        self.keep_temporary();
    }

    /// Deletes the temporary chats opened here that the phone left
    /// (`poll_chat_mode`); a chat open on the computer is not the phone's.
    fn sweep_temporary(&mut self) {
        if !self.net.online() {
            return;
        }
        let left: Vec<String> = self
            .temporary
            .iter()
            .filter(|id| self.selected.as_deref() != Some(id.as_str()))
            .filter(|id| !self.deleted.contains(*id) && self.thread(id).is_some())
            .cloned()
            .collect();
        for id in left {
            if self.send(Action::Dispatch(pluto_core::model::command(
                "thread.delete",
                &id,
            ))) {
                self.deleted.insert(id.clone());
                self.temporary.remove(&id);
                self.keep_temporary();
            }
        }
    }

    /// A `server.updateSettings` patch, applied locally at once.
    pub fn update_setting(&mut self, key: &str, value: Value) {
        if !self.state.config["settings"].is_object() {
            self.state.config["settings"] = json!({});
        }
        // The value before the first change still on its way, for a refusal.
        let before = self.state.config["settings"][key].clone();
        self.setting_backups.entry(key.to_owned()).or_insert(before);
        self.state.config["settings"][key] = value.clone();
        self.request(
            format!("settings:{key}"),
            "server.updateSettings",
            json!({ "patch": { key: value } }),
        );
    }

    fn rpc_result(&mut self, key: String, result: Result<Value, String>) {
        self.pending.remove(&key);
        if let Some((held, sends_key)) = self.persisting.remove(&key) {
            self.persisted(held, sends_key, result);
            return;
        }
        // A setting the server refused goes back to what it was; one it took
        // keeps its new value.
        if let Some(setting) = key.strip_prefix("settings:") {
            let before = self.setting_backups.remove(setting);
            if let (Err(_), Some(before)) = (&result, before) {
                self.state.config["settings"][setting] = before;
            }
        }
        if let Some(sent) = self.sends.remove(&key)
            && let Err(error) = &result
        {
            // The launch was refused: the draft comes back, and the screen of
            // a thread that will not exist goes home.
            let thread = key.trim_start_matches("launch:").to_owned();
            self.give_back(sent.0, sent.1, sent.2);
            if self.selected.as_deref() == Some(thread.as_str()) {
                self.selected = None;
                self.state.switch_detail(None);
                self.resubscribe = None;
            }
            self.show_toast(format!(
                "Could not start the thread: {} Your text is back in the composer.",
                sentence(error)
            ));
            self.replies.insert(key, result);
            return;
        }
        // A launch the server took: nothing waits on its reply.
        if key.starts_with("launch:") && result.is_ok() {
            self.sends.remove(&key);
            return;
        }
        if let Some(thread) = key.strip_prefix("generated:") {
            // A server without the list keeps the timeline as it is.
            if let Ok(value) = &result {
                let images = value["images"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|image| {
                        Some(pluto_core::generated_images::GeneratedImage {
                            path: image["path"].as_str()?.into(),
                            modified: std::time::UNIX_EPOCH
                                + Duration::from_millis(image["modifiedAt"].as_u64()?),
                            bytes: image["sizeBytes"].as_u64().unwrap_or_default(),
                        })
                    })
                    .collect();
                self.generated.insert(thread.to_owned(), images);
            }
            return;
        }
        if key == "scheduled-tasks" {
            self.tasks = Some(match result {
                Ok(value) => Ok(value["tasks"].as_array().cloned().unwrap_or_default()),
                Err(error) => {
                    self.tasks_streaming = false;
                    Err(error)
                }
            });
            return;
        }
        if let Err(error) = &result
            && (key.starts_with("mutation:") || key.starts_with("settings:"))
        {
            self.show_toast(error.clone());
        }
        self.replies.insert(key, result);
    }
}

/// The pump of one connection: woken by its events, it applies them until
/// the connection ends or a newer one replaced it.
fn pump(hub: Weak<Hub>, woken: mpsc::Receiver<()>, generation: u64, remembered: bool) {
    while woken.recv().is_ok() {
        let Some(hub) = hub.upgrade() else {
            return;
        };
        let mut ended = false;
        hub.update(|app, hub| {
            if app.session.generation != generation {
                ended = true;
                return;
            }
            ended = app.session.drain(hub, remembered);
        });
        if ended {
            return;
        }
    }
}

/// "192.168.1.20:3774" from "http://192.168.1.20:3774/".
pub fn url_host(server: &str) -> String {
    let rest = server
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(server);
    rest.trim_end_matches('/').to_owned()
}

/// When a thread was last used: its latest user message, else its update.
pub fn last_activity(thread: &Thread) -> &str {
    thread
        .latest_user_message_at
        .as_deref()
        .unwrap_or(&thread.updated_at)
}

/// A message as a sentence, for the toasts that go on after it.
fn sentence(text: &str) -> String {
    let text = text.trim();
    if text.ends_with(['.', '!', '?']) {
        text.to_owned()
    } else {
        format!("{text}.")
    }
}

/// A message held while its images are stored (`Session::persist`).
enum Held {
    Message(Value),
    /// A new thread's id and its `orchestration.launchThread` input.
    Launch(String, Value),
}

/// Takes the images to store (those carrying their bytes as a data URL)
/// out of `attachments`; the files, already uploaded, stay.
fn take_uploads(attachments: &mut Value) -> Vec<Value> {
    let Some(list) = attachments.as_array_mut() else {
        return Vec::new();
    };
    let (uploads, kept) = std::mem::take(list)
        .into_iter()
        .partition(|attachment| attachment.get("dataUrl").is_some());
    *list = kept;
    uploads
}

/// Appends the attachments the server stored.
fn add_attachments(attachments: &mut Value, stored: Vec<Value>) {
    match attachments.as_array_mut() {
        Some(list) => list.extend(stored),
        None => *attachments = Value::Array(stored),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_drop_the_scheme() {
        assert_eq!(url_host("http://192.168.1.20:3774/"), "192.168.1.20:3774");
        assert_eq!(url_host("pluto.example"), "pluto.example");
    }

    #[test]
    fn images_are_stored_before_their_message() {
        let mut attachments = json!([
            {"type": "image", "name": "a.png", "dataUrl": "data:image/png;base64,AQID"},
            {"type": "file", "id": "att-1", "name": "notes.pdf"},
        ]);
        let uploads = take_uploads(&mut attachments);
        assert_eq!(uploads.len(), 1);
        assert_eq!(
            attachments,
            json!([{"type": "file", "id": "att-1", "name": "notes.pdf"}])
        );
        add_attachments(
            &mut attachments,
            vec![json!({"type": "image", "id": "img-1"})],
        );
        assert_eq!(attachments[1]["id"], "img-1");

        // Offline, the message and its image stay in the composer.
        let mut session = Session::new(crate::prefs::Prefs::load(None));
        session
            .attach("chat", "a.png".into(), "image/png".into(), vec![1, 2, 3])
            .unwrap();
        let mut command = pluto_core::commands::message_dispatch("t", "hi", &json!({}), "auto");
        command["attachments"] = json!(session.wire_attachments("chat").unwrap());
        assert!(!session.send_message(command, "chat".into(), "hi".into()));
        assert_eq!(session.attachments["chat"].len(), 1);
        assert!(session.persisting.is_empty());
    }
}
