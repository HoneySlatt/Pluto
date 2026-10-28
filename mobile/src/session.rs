//! The phone's session with a Pluto server: the core connection, the state
//! its events fill, and what the screens choose (the space, the open
//! thread, its folds).

use std::rc::Rc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use pluto_core::model::{State, Thread};
use pluto_core::protocol::{Action, Connection, Event, Route};
use pluto_core::spaces::{Space, space_of};
use pluto_core::timeline::Expansion;
use serde_json::Value;
use zeroize::Zeroizing;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Disconnected(Option<String>),
    Connecting,
    Connected,
}

pub struct Session {
    pub state: State,
    pub status: Status,
    /// The Chat space (else Code).
    pub chat: bool,
    pub selected: Option<String>,
    pub expansion: Expansion,
    /// The last failure a command or request reported.
    pub notice: Option<String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            state: State::default(),
            status: Status::Disconnected(None),
            chat: false,
            selected: None,
            expansion: Expansion::default(),
            notice: None,
        }
    }
}

impl Session {
    fn scratch_root(&self) -> Option<&str> {
        self.state.config["scratchWorkspaceRoot"].as_str()
    }

    pub fn space(&self, thread: &Thread) -> Space {
        self.state
            .project(&thread.project_id)
            .map(|p| space_of(self.scratch_root(), &p.workspace_root))
            .unwrap_or(Space::Code)
    }

    /// The threads the current space lists, the latest first; temporary
    /// chats and subagents are listed nowhere.
    pub fn listed(&self) -> Vec<std::sync::Arc<Thread>> {
        let mut threads: Vec<_> = self
            .state
            .threads
            .iter()
            .filter(|t| t.archived_at.is_none() && !t.is_subagent())
            .filter(|t| {
                let space = self.space(t);
                space != Space::Temporary && space.is_chat() == self.chat
            })
            .cloned()
            .collect();
        threads.sort_by(|a, b| last_activity(b).cmp(last_activity(a)));
        threads
    }

    pub fn thread(&self, id: &str) -> Option<std::sync::Arc<Thread>> {
        self.state.threads.iter().find(|t| t.id == id).cloned()
    }
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

/// Opens the connection and applies its events to `session` as they come.
pub fn connect(
    mut session: Signal<Session>,
    mut link: Link,
    server: String,
    token: Zeroizing<String>,
) {
    let (wake, mut woken) = futures_channel::mpsc::unbounded::<()>();
    let repaint = pluto_core::Repaint::new(move |_| {
        let _ = wake.unbounded_send(());
    });
    session.write().status = Status::Connecting;
    let connection = match Connection::open(&server, token, repaint) {
        Ok(connection) => Rc::new(connection),
        Err(error) => {
            session.write().status = Status::Disconnected(Some(error));
            return;
        }
    };
    link.0.set(Some(connection.clone()));
    spawn(async move {
        while woken.next().await.is_some() {
            let mut events = Vec::new();
            while let Ok(event) = connection.events.try_recv() {
                events.push(event);
            }
            if events.is_empty() {
                continue;
            }
            let mut session = session.write();
            for event in events {
                if apply(&mut session, event) {
                    link.0.set(None);
                    return;
                }
            }
        }
    });
}

/// Applies one event; true once the connection is gone.
fn apply(session: &mut Session, event: Event) -> bool {
    match event {
        Event::Data(Route::Config, value) => {
            session.state.config_event(&value);
            session.status = Status::Connected;
        }
        Event::Data(Route::Shell, value) => {
            if let Err(error) = session.state.shell(&value) {
                session.notice = Some(error);
            }
        }
        Event::Data(Route::Thread(id), value) => {
            if let Err(error) = session.state.thread(&id, &value) {
                session.notice = Some(error);
            }
        }
        Event::Data(Route::Rpc(key), value) => rpc_result(session, &key, Ok(value)),
        Event::Failed(Route::Rpc(key), message) => rpc_result(session, &key, Err(message)),
        Event::Failed(_, message) => session.notice = Some(message),
        Event::Disconnected(message) => {
            session.status = Status::Disconnected(Some(message));
            return true;
        }
        Event::Data(..) => {}
    }
    false
}

fn rpc_result(session: &mut Session, key: &str, result: Result<Value, String>) {
    if key.starts_with("launch:")
        && let Err(error) = result
    {
        session.notice = Some(format!("Could not start the thread: {error}"));
    }
}

/// Opens `id` (or goes back to the list), following its timeline.
pub fn select(mut session: Signal<Session>, link: &Link, id: Option<String>) {
    let mut session = session.write();
    if session.selected == id {
        return;
    }
    session.state.switch_detail(id.as_deref());
    session.selected = id.clone();
    session.expansion = Expansion::default();
    link.send(Action::Select(id));
}

/// Sends `text` to the open thread (`startThreadTurn`): it starts a turn,
/// or follows the running one the way the server decides.
pub fn send_message(mut session: Signal<Session>, link: &Link, text: &str) {
    let text = text.trim();
    let Some(thread) = session
        .peek()
        .selected
        .as_deref()
        .and_then(|id| session.peek().thread(id))
    else {
        return;
    };
    if text.is_empty() {
        return;
    }
    let model = serde_json::to_value(&thread.model_selection).unwrap_or(Value::Null);
    let command = pluto_core::commands::message_dispatch(&thread.id, text, &model, "auto");
    session.write().notice = None;
    link.send(Action::Dispatch(command));
}
