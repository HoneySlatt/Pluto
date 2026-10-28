//! The phone's session with a Pluto server: the core connection, the state
//! its events fill, one-shot replies by key, the scheduled tasks stream, and
//! the connection's health. A session lost after it was established
//! reconnects on its own, three times, before it shows "Can't reach the
//! server" with Retry; a remembered server that does not answer at launch
//! shows "Can't reach your server".

use std::collections::HashMap;
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
    /// The scheduled tasks (`scheduledTasks.subscribe`), once listed.
    pub tasks: Option<Result<Vec<Value>, String>>,
    tasks_streaming: bool,
    pub prefs: Prefs,
    /// A short message at the top of the screen, with its serial.
    pub toast: Option<(String, u64)>,
    serial: u64,
    /// The open thread to follow again once the shell lists it.
    resubscribe: Option<String>,
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
            tasks: None,
            tasks_streaming: false,
            prefs: Prefs::load(),
            toast: None,
            serial: 0,
            resubscribe: None,
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
    pub fn send(&self, action: Action) {
        if let Some(connection) = self.0.peek().as_ref() {
            connection.send(action);
        }
    }
}

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
        self.link.0.set(Some(connection.clone()));
        // A reconnected session follows the open thread again.
        if let Some(id) = self.session.peek().selected.clone() {
            connection.send(Action::Select(Some(id)));
        }
        spawn(async move {
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
                    self.apply(event);
                }
            }
        });
    }

    /// The connection ended or never started.
    fn failed(mut self, error: String, remembered: bool) {
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
        spawn(async move {
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
        drop(session);
        self.link.0.set(None);
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
                    self.link.send(Action::Select(Some(id)));
                }
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
            Event::Failed(Route::Thread(id), _)
                if session.selected.as_deref() == Some(id.as_str()) =>
            {
                session.resubscribe = Some(id);
            }
            Event::Data(Route::Rpc(key), value) => rpc_result(&mut session, key, Ok(value)),
            Event::Failed(Route::Rpc(key), message) => rpc_result(&mut session, key, Err(message)),
            Event::Failed(Route::Command(_), message) => session.show_toast(message),
            Event::Failed(_, message) => session.show_toast(message),
            Event::Data(..) | Event::Disconnected(_) => {}
        }
    }

    /// "Reconnected" stays for a moment, then the bar leaves.
    fn settle_back(mut self) {
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(1400)).await;
            if self.session.peek().net == Net::Back {
                self.session.write().net = Net::Connected;
            }
        });
    }

    pub fn dispatch(self, command: Value) {
        self.link.send(Action::Dispatch(command));
    }

    /// A one-shot request; its reply lands in `replies` under `key`.
    pub fn request(mut self, key: impl Into<String>, method: &'static str, payload: Value) {
        let key = key.into();
        self.session.write().replies.remove(&key);
        self.link.send(Action::Request {
            key,
            method,
            payload,
        });
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
        drop(session);
        self.link.send(Action::Select(id));
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

fn rpc_result(session: &mut Session, key: String, result: Result<Value, String>) {
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
    if let Err(error) = &result {
        if let Some(what) = key.strip_prefix("launch:").map(|_| "start the thread") {
            session.show_toast(format!("Could not {what}: {error}"));
        } else if key.starts_with("mutation:") || key.starts_with("settings:") {
            session.show_toast(error.clone());
        }
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
