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

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_util::StreamExt;
use pluto_core::model::{Project, State, Thread};
use pluto_core::protocol::{Action, Connection, Event, Route};
use pluto_core::spaces::{Space, space_of};
use pluto_core::timeline::Expansion;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::prefs::Prefs;

/// Reconnection attempts before the session gives up (`#lost`).
pub const ATTEMPTS: u8 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(1500);

#[derive(Clone, Debug, PartialEq)]
pub enum Net {
    /// No session yet: the connection screen, with the last failure.
    Disconnected(Option<String>),
    /// The first connection of this server is on its way.
    Connecting,
    Connected,
    /// Lost after it was established: this attempt is on its way.
    Reconnecting(u8),
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
            Net::Connected | Net::Reconnecting(_) | Net::Offline | Net::Back
        )
    }

    /// Whether commands can go out: Send, Stop and the request cards wait
    /// for the server otherwise.
    pub fn online(&self) -> bool {
        matches!(self, Net::Connected | Net::Back)
    }
}

pub struct Session {
    pub state: State,
    pub net: Net,
    /// The server's address and the token, kept in memory for reconnecting.
    pub server: String,
    token: Option<Zeroizing<String>>,
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
    sends: HashMap<String, (String, String)>,
    /// Texts given back to their composer (by composer key).
    pub unsent: HashMap<String, String>,
    /// The scheduled tasks (`scheduledTasks.subscribe`), once listed.
    pub tasks: Option<Result<Vec<Value>, String>>,
    tasks_streaming: bool,
    pub prefs: Prefs,
    /// A short message at the top of the screen, with its serial.
    pub toast: Option<(String, u64)>,
    serial: u64,
    /// The open thread to follow again once the shell lists it.
    resubscribe: Option<String>,
    /// Why the open thread could not be followed, by thread id.
    pub thread_error: Option<(String, String)>,
    /// Images by key (`favicon:{project}`, `attachment:{id}`): a data URL
    /// once fetched, `None` while asked or when there is none.
    pub assets: HashMap<String, Option<String>>,
    /// Temporary chats opened here, and those already sent `thread.delete`
    /// on this connection.
    temporary: HashSet<String>,
    deleted: HashSet<String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            state: State::default(),
            net: Net::Disconnected(None),
            server: String::new(),
            token: None,
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
            unsent: HashMap::new(),
            tasks: None,
            tasks_streaming: false,
            prefs: Prefs::load(),
            toast: None,
            serial: 0,
            resubscribe: None,
            thread_error: None,
            assets: HashMap::new(),
            temporary: HashSet::new(),
            deleted: HashSet::new(),
        }
    }
}

impl Session {
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
    pub fn listed(&self, chat: bool) -> Vec<std::sync::Arc<Thread>> {
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

    pub fn thread(&self, id: &str) -> Option<std::sync::Arc<Thread>> {
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
    pub fn pending_subagent(&self, parent: &str) -> Option<std::sync::Arc<Thread>> {
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

    fn is_temporary(&self, id: &str) -> bool {
        self.thread(id)
            .is_some_and(|t| self.space(&t) == Space::Temporary)
    }

    /// Hands every message on its way back to its composer.
    fn give_back_sends(&mut self) {
        for (_, (composer, text)) in std::mem::take(&mut self.sends) {
            self.unsent.entry(composer).or_insert(text);
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

/// The connection, shared by the screens that send through it.
#[derive(Clone, Copy, Default)]
pub struct Link(pub Signal<Option<Rc<Connection>>>);

impl Link {
    /// Sends `action` once the server synchronized; false without a
    /// synchronized connection.
    pub fn send(&self, action: Action) -> bool {
        match self.0.peek().as_ref() {
            Some(connection) => {
                connection.send(action);
                true
            }
            None => false,
        }
    }
}

const NOT_CONNECTED: &str = "Not connected to the server: nothing was sent.";

/// The session and its connection, as the screens use them.
#[derive(Clone, Copy)]
pub struct Ctx {
    pub session: Signal<Session>,
    pub link: Link,
}

impl Ctx {
    /// Connects to `server` with `token`; a launch from remembered
    /// credentials shows the unreachable page when it fails.
    pub fn connect(mut self, server: String, token: Zeroizing<String>, remembered: bool) {
        {
            let mut session = self.session.write();
            session.server = server;
            session.token = Some(token);
            session.established = false;
            session.launch = remembered.then_some(false);
            session.net = Net::Connecting;
        }
        self.open(remembered);
    }

    /// Opens a connection with the session's credentials.
    fn open(mut self, remembered: bool) {
        let (server, token, generation) = {
            let mut session = self.session.write();
            session.generation += 1;
            let Some(token) = session.token.clone() else {
                return;
            };
            (session.server.clone(), token, session.generation)
        };
        let (wake, mut woken) = futures_channel::mpsc::unbounded::<()>();
        let repaint = pluto_core::Repaint::new(move |_| {
            let _ = wake.unbounded_send(());
        });
        let connection = match Connection::open(&server, token, repaint) {
            Ok(connection) => Rc::new(connection),
            Err(error) => {
                self.failed(error, remembered);
                return;
            }
        };
        // A reconnected session follows the open thread again once the shell
        // lists it (commands sent before the server synchronized would end
        // the connection).
        {
            let mut session = self.session.write();
            session.resubscribe = session.selected.clone();
        }
        // The session outlives the screen that started it (the connection
        // screen unmounts once connected).
        dioxus::core::spawn_forever(async move {
            while woken.next().await.is_some() {
                if self.session.peek().generation != generation {
                    return;
                }
                let mut events = Vec::new();
                while let Ok(event) = connection.events.try_recv() {
                    events.push(event);
                }
                for event in events {
                    if let Event::Disconnected(error) = event {
                        self.link.0.set(None);
                        self.failed(error, remembered);
                        return;
                    }
                    // The server's config comes once it synchronized: the
                    // link opens to the screens.
                    if matches!(event, Event::Data(Route::Config, _))
                        && self.link.0.peek().is_none()
                    {
                        self.link.0.set(Some(connection.clone()));
                    }
                    self.apply(event);
                }
            }
        });
    }

    /// The connection ended or never started.
    fn failed(mut self, error: String, remembered: bool) {
        {
            let mut session = self.session.write();
            if !session.sends.is_empty() {
                session.give_back_sends();
                session.show_toast(
                    "The connection dropped while sending: your message is back in the composer. Check the thread before sending it again.",
                );
            }
            // Whatever still waited will not hear back from this connection.
            for key in std::mem::take(&mut session.pending) {
                session.replies.insert(
                    key,
                    Err("The connection dropped before the server answered.".into()),
                );
            }
        }
        let (established, net) = {
            let session = self.session.peek();
            (session.established, session.net.clone())
        };
        if !established {
            let mut session = self.session.write();
            // A rejected token is not an unreachable server.
            if remembered && !error.contains("401") {
                session.net = Net::Unreachable;
                session.launch = Some(true);
            } else {
                session.net = Net::Disconnected(Some(error));
            }
            return;
        }
        let attempt = match net {
            Net::Reconnecting(n) => n + 1,
            _ => 1,
        };
        if attempt > ATTEMPTS {
            self.session.write().net = Net::Offline;
            return;
        }
        self.session.write().net = Net::Reconnecting(attempt);
        // The session outlives the screen that started it (the connection
        // screen unmounts once connected).
        dioxus::core::spawn_forever(async move {
            tokio::time::sleep(RETRY_DELAY).await;
            if self.session.peek().net == Net::Reconnecting(attempt) {
                self.open(false);
            }
        });
    }

    /// Retry after the attempts ran out, or from the unreachable page.
    pub fn retry(mut self) {
        let established = self.session.peek().established;
        self.session.write().net = if established {
            Net::Reconnecting(1)
        } else {
            Net::Connecting
        };
        self.open(!established);
    }

    /// Leaves the server for the connection screen.
    pub fn disconnect(mut self) {
        let mut session = self.session.write();
        session.generation += 1;
        session.token = None;
        session.established = false;
        session.launch = None;
        session.net = Net::Disconnected(None);
        session.listed = false;
        session.tasks = None;
        session.tasks_streaming = false;
        session.selected = None;
        session.state = State::default();
        session.expansion = Expansion::default();
        session.replies.clear();
        session.pending.clear();
        session.sends.clear();
        session.unsent.clear();
        session.resubscribe = None;
        session.thread_error = None;
        session.assets.clear();
        session.temporary.clear();
        session.deleted.clear();
        session.toast = None;
        drop(session);
        self.link.0.set(None);
    }

    /// Checks a session that may have died while the phone slept: the
    /// server must answer at once, else it reconnects. Its settings come
    /// back as `probe` once it answered.
    pub fn probe(self) {
        let net = self.session.peek().net.clone();
        match net {
            Net::Offline => self.retry(),
            _ if net.online() => {
                self.link.send(Action::Ping);
                self.request("probe", "server.getSettings", json!({}));
            }
            _ => {}
        }
    }

    fn apply(mut self, event: Event) {
        let mut session = self.session.write();
        match event {
            Event::Data(Route::Config, value) => {
                session.state.config_event(&value);
                if !session.net.online() {
                    let back = session.established;
                    session.net = if back { Net::Back } else { Net::Connected };
                    session.established = true;
                    // A new connection deletes again what the last one may
                    // not have.
                    session.deleted.clear();
                    // The tasks the last connection followed.
                    if session.tasks_streaming {
                        self.link.send(Action::Stream {
                            key: "scheduled-tasks".into(),
                            method: "scheduledTasks.subscribe",
                            payload: json!({}),
                        });
                    }
                    if back {
                        drop(session);
                        self.settle_back();
                    }
                }
            }
            Event::Data(Route::Shell, value) => {
                match session.state.shell(&value) {
                    Ok(()) => session.listed = true,
                    Err(error) => session.show_toast(error),
                }
                if let Some(id) = session.resubscribe.clone()
                    && session.thread(&id).is_some()
                {
                    session.resubscribe = None;
                    session.thread_error = None;
                    self.link.send(Action::Select(Some(id)));
                }
                if let Some(id) = session.selected.clone()
                    && session.is_temporary(&id)
                {
                    session.temporary.insert(id);
                }
                drop(session);
                self.sweep_temporary();
            }
            Event::Data(Route::Thread(id), value) => {
                if let Err(error) = session.state.thread(&id, &value) {
                    session.show_toast(error);
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
                if let Err(error) = session
                    .state
                    .history_page(&thread_id, &cursor, &value, complete)
                {
                    session.state.history_failed(&thread_id, &cursor, error);
                }
            }
            Event::Failed(
                Route::History {
                    thread_id, cursor, ..
                },
                message,
            ) => {
                session.state.history_failed(&thread_id, &cursor, message);
            }
            // A new thread can be asked for before the server created it:
            // it is followed again once the shell lists it.
            Event::Failed(Route::Thread(id), message)
                if session.selected.as_deref() == Some(id.as_str()) =>
            {
                session.thread_error = Some((id.clone(), message));
                session.resubscribe = Some(id);
            }
            Event::Data(Route::Rpc(key), value) if key.starts_with("asset:") => {
                let (server, generation) = (session.server.clone(), session.generation);
                drop(session);
                self.fetch_asset(&key["asset:".len()..], &server, &value, generation);
            }
            Event::Failed(Route::Rpc(key), _) if key.starts_with("asset:") => {
                // A failure is tried again on the next showing.
                session.assets.remove(&key["asset:".len()..]);
            }
            Event::Data(Route::Rpc(key), value) => rpc_result(&mut session, key, Ok(value)),
            Event::Failed(Route::Rpc(key), message) => rpc_result(&mut session, key, Err(message)),
            Event::Data(Route::Command(id), value) => {
                session.sends.remove(&id);
                let key = format!("command:{id}");
                if session.pending.remove(&key) {
                    session.replies.insert(key, Ok(value));
                }
            }
            Event::Failed(Route::Command(id), message) => {
                let key = format!("command:{id}");
                // A tracked command shows its failure where it was asked.
                if session.pending.remove(&key) {
                    session.replies.insert(key, Err(message));
                } else if let Some((composer, text)) = session.sends.remove(&id) {
                    session.unsent.entry(composer).or_insert(text);
                    session.show_toast(format!(
                        "Message not sent: {} Your text is back in the composer.",
                        sentence(&message)
                    ));
                } else {
                    session.show_toast(message);
                }
            }
            Event::Failed(_, message) => session.show_toast(message),
            Event::Data(..) | Event::Disconnected(_) => {}
        }
    }

    /// A project's favicon (`favicon.rs`): `assets.createUrl` with a
    /// `project-favicon` resource, once per project.
    pub fn favicon(self, project: &Project) -> Option<String> {
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
    pub fn attachment(self, id: &str) -> Option<String> {
        self.asset(
            &format!("attachment:{id}"),
            json!({"_tag": "attachment", "attachmentId": id}),
        )
    }

    /// An image the server serves (`assets.createUrl`), asked for once. The
    /// WebView may not load plain http from the network, so the image is
    /// fetched here and shown as a data URL.
    fn asset(mut self, key: &str, resource: Value) -> Option<String> {
        if let Some(found) = self.session.peek().assets.get(key) {
            return found.clone();
        }
        if !self.session.peek().net.online() {
            return None;
        }
        self.session.write().assets.insert(key.to_owned(), None);
        self.link.send(Action::Request {
            key: format!("asset:{key}"),
            method: "assets.createUrl",
            payload: json!({"resource": resource}),
        });
        None
    }

    fn fetch_asset(mut self, key: &str, server: &str, reply: &Value, generation: u64) {
        let url = reply["relativeUrl"]
            .as_str()
            .and_then(|relative| url::Url::parse(server.trim()).ok()?.join(relative).ok());
        // The server's marker for a project without a favicon keeps the
        // monogram; it stays `None` for the session.
        let Some(url) = url.filter(|u| {
            u.path_segments()
                .and_then(|mut s| s.next_back())
                .is_some_and(|name| name != "project-favicon-missing")
        }) else {
            return;
        };
        let (done, fetched) = futures_channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = done.send(pluto_core::http::get(&url));
        });
        let key = key.to_owned();
        dioxus::core::spawn_forever(async move {
            let data = fetched
                .await
                .ok()
                .flatten()
                .and_then(|bytes| crate::badge::data_url(&bytes));
            let mut session = self.session.write();
            // An image of an older connection or server stays out.
            if session.generation != generation {
                return;
            }
            match data {
                Some(data) => {
                    session.assets.insert(key, Some(data));
                }
                // A failed download is tried again on the next showing.
                None => {
                    session.assets.remove(&key);
                }
            }
        });
    }

    /// "Reconnected" stays for a moment, then the bar leaves.
    fn settle_back(mut self) {
        // The session outlives the screen that started it (the connection
        // screen unmounts once connected).
        dioxus::core::spawn_forever(async move {
            tokio::time::sleep(Duration::from_millis(1400)).await;
            if self.session.peek().net == Net::Back {
                self.session.write().net = Net::Connected;
            }
        });
    }

    /// Sends a command whose failure shows as a toast.
    pub fn dispatch(self, command: Value) -> bool {
        let sent = self.link.send(Action::Dispatch(command));
        if !sent {
            self.toast(NOT_CONNECTED);
        }
        sent
    }

    /// Sends a command whose reply lands in `replies` under
    /// `command:{id}`; returns that key.
    pub fn dispatch_tracked(mut self, command: Value) -> String {
        let key = format!(
            "command:{}",
            command["commandId"].as_str().unwrap_or_default()
        );
        {
            let mut session = self.session.write();
            session.replies.remove(&key);
            session.pending.insert(key.clone());
        }
        if !self.link.send(Action::Dispatch(command)) {
            let mut session = self.session.write();
            session.pending.remove(&key);
            session
                .replies
                .insert(key.clone(), Err(NOT_CONNECTED.into()));
        }
        key
    }

    /// Sends a message from `composer`: refused, it comes back there.
    pub fn send_message(mut self, command: Value, composer: String, text: String) -> bool {
        let id = command["commandId"].as_str().unwrap_or_default().to_owned();
        self.session
            .write()
            .sends
            .insert(id.clone(), (composer, text));
        let sent = self.dispatch(command);
        if !sent {
            self.session.write().sends.remove(&id);
        }
        sent
    }

    /// Starts a thread from `composer`'s draft (`orchestration.launchThread`):
    /// refused, the text comes back there.
    pub fn launch(mut self, thread_id: &str, input: Value, composer: String, text: String) -> bool {
        let key = format!("launch:{thread_id}");
        self.session
            .write()
            .sends
            .insert(key.clone(), (composer, text));
        self.request(key.clone(), "orchestration.launchThread", input);
        let refused = self.session.peek().replies.contains_key(&key);
        if refused {
            self.session.write().sends.remove(&key);
        }
        !refused
    }

    /// A one-shot request; its reply lands in `replies` under `key`.
    pub fn request(mut self, key: impl Into<String>, method: &'static str, payload: Value) {
        let key = key.into();
        {
            let mut session = self.session.write();
            session.replies.remove(&key);
            session.pending.insert(key.clone());
        }
        let sent = self.link.send(Action::Request {
            key: key.clone(),
            method,
            payload,
        });
        if !sent {
            let mut session = self.session.write();
            session.pending.remove(&key);
            if key.starts_with("mutation:") || key.starts_with("settings:") {
                session.show_toast(NOT_CONNECTED);
            }
            session.replies.insert(key, Err(NOT_CONNECTED.into()));
        }
    }

    /// Drops the replies under `prefix` (a closed picker's searches).
    pub fn forget(mut self, prefix: &str) {
        self.session
            .write()
            .replies
            .retain(|key, _| !key.starts_with(prefix));
    }

    /// A request whose reply is only checked for failure.
    pub fn mutate(self, method: &'static str, payload: Value) {
        self.request(format!("mutation:{method}"), method, payload);
    }

    /// Follows the scheduled tasks once.
    pub fn watch_tasks(mut self) {
        if self.session.peek().tasks_streaming {
            return;
        }
        self.session.write().tasks_streaming = true;
        self.link.send(Action::Stream {
            key: "scheduled-tasks".into(),
            method: "scheduledTasks.subscribe",
            payload: json!({}),
        });
    }

    /// Opens `id` (or none), following its timeline.
    pub fn select(mut self, id: Option<String>) {
        let mut session = self.session.write();
        if session.selected == id {
            return;
        }
        session.state.switch_detail(id.as_deref());
        session.selected = id.clone();
        session.expansion = Expansion::default();
        session.thread_error = None;
        if let Some(id) = &id
            && session.is_temporary(id)
        {
            session.temporary.insert(id.clone());
        }
        // Offline, the thread is followed once the server is back.
        let online = session.net.online();
        session.resubscribe = if online { None } else { id.clone() };
        drop(session);
        if online {
            self.link.send(Action::Select(id));
        }
        self.sweep_temporary();
    }

    /// Follows the open thread again after it failed to load.
    pub fn follow_again(mut self, id: &str) {
        self.session.write().thread_error = None;
        self.link.send(Action::Select(Some(id.to_owned())));
    }

    /// Marks a temporary chat started here, deleted once the phone leaves it.
    pub fn opened_temporary(mut self, id: &str) {
        self.session.write().temporary.insert(id.to_owned());
    }

    /// Deletes the temporary chats opened here that the phone left
    /// (`poll_chat_mode`); a chat open on the computer is not the phone's.
    fn sweep_temporary(mut self) {
        let left: Vec<String> = {
            let session = self.session.peek();
            if !session.net.online() {
                return;
            }
            session
                .temporary
                .iter()
                .filter(|id| session.selected.as_deref() != Some(id.as_str()))
                .filter(|id| !session.deleted.contains(*id) && session.thread(id).is_some())
                .cloned()
                .collect()
        };
        for id in left {
            if self.link.send(Action::Dispatch(pluto_core::model::command(
                "thread.delete",
                &id,
            ))) {
                let mut session = self.session.write();
                session.deleted.insert(id.clone());
                session.temporary.remove(&id);
            }
        }
    }

    /// A `server.updateSettings` patch, applied locally at once.
    pub fn update_setting(mut self, key: &str, value: Value) {
        {
            let mut session = self.session.write();
            if !session.state.config["settings"].is_object() {
                session.state.config["settings"] = json!({});
            }
            session.state.config["settings"][key] = value.clone();
        }
        self.request(
            format!("settings:{key}"),
            "server.updateSettings",
            json!({ "patch": { key: value } }),
        );
    }

    pub fn toast(mut self, text: impl Into<String>) {
        self.session.write().show_toast(text);
    }
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

fn rpc_result(session: &mut Session, key: String, result: Result<Value, String>) {
    session.pending.remove(&key);
    if let Some(sent) = session.sends.remove(&key)
        && let Err(error) = &result
    {
        // The launch was refused: the draft comes back, and the screen of a
        // thread that will not exist goes home.
        let thread = key.trim_start_matches("launch:").to_owned();
        session.unsent.entry(sent.0).or_insert(sent.1);
        if session.selected.as_deref() == Some(thread.as_str()) {
            session.selected = None;
            session.state.switch_detail(None);
            session.resubscribe = None;
        }
        session.show_toast(format!(
            "Could not start the thread: {} Your text is back in the composer.",
            sentence(error)
        ));
        session.replies.insert(key, result);
        return;
    }
    if key == "scheduled-tasks" {
        session.tasks = Some(match result {
            Ok(value) => Ok(value["tasks"].as_array().cloned().unwrap_or_default()),
            Err(error) => {
                session.tasks_streaming = false;
                Err(error)
            }
        });
        return;
    }
    if let Err(error) = &result
        && (key.starts_with("mutation:") || key.starts_with("settings:"))
    {
        session.show_toast(error.clone());
    }
    session.replies.insert(key, result);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_drop_the_scheme() {
        assert_eq!(url_host("http://192.168.1.20:3774/"), "192.168.1.20:3774");
        assert_eq!(url_host("pluto.example"), "pluto.example");
    }
}
