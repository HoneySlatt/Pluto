//! Settling or snoozing the open thread moves on to the next one
//! (`settleAndAdvance.ts`, `Sidebar.tsx`'s `planForwardNavigation`): the
//! next thread of the sidebar that is neither settled nor snoozed, else a
//! new draft in its project. The target is chosen before the command, and
//! the move happens once the server took it and the shell shows the thread
//! parked, unless the user went elsewhere meanwhile. Lifecycle commands
//! that fail show a titled error (`stackedThreadToast`).

use std::time::{Duration, Instant};

use serde_json::Value;

use crate::app::App;
use crate::ui::toasts;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Park {
    Settle,
    Snooze,
}

/// Where the open thread goes once it is parked.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Next {
    Thread(String),
    Draft(String),
}

/// What waits on a command's receipt.
pub(crate) struct Continuation {
    /// The error toast's title when the server refuses it.
    pub failure: &'static str,
    pub advance: Option<(String, Park, Next)>,
    /// The notice waiting on it (`App::notice_busy`), freed either way.
    pub busy: Option<String>,
    /// What the undo notice offers once the server took it.
    pub undo: Option<crate::ui::undo::Undoable>,
}

/// A move waiting for the shell to show the thread parked.
pub(crate) struct Advance {
    thread: String,
    park: Park,
    next: Next,
    since: Instant,
}

impl App {
    /// Sends `command`; a refusal shows `failure` as its title.
    pub(crate) fn dispatch_titled(&mut self, command: Value, failure: &'static str) {
        self.dispatch_with(
            command,
            Continuation {
                failure,
                advance: None,
                busy: None,
                undo: None,
            },
        );
    }

    /// Unpins `thread`, offering to pin it back where it was.
    pub(crate) fn unpin(&mut self, thread: &crate::model::Thread) {
        let mut pin = crate::model::command("thread.pin", &thread.id);
        if let Some(key) = &thread.pin_order_key {
            pin["orderKey"] = serde_json::json!(key);
        }
        self.dispatch_with(
            crate::model::command("thread.unpin", &thread.id),
            Continuation {
                failure: "Failed to unpin thread",
                advance: None,
                busy: None,
                undo: Some(crate::ui::undo::Undoable {
                    action: "Unpinned",
                    kind: "pin",
                    thread: thread.id.clone(),
                    commands: vec![pin],
                    failure: "Failed to undo unpin",
                }),
            },
        );
    }

    /// Archives `thread`, offering to bring it back.
    pub(crate) fn archive(&mut self, thread_id: &str) {
        self.dispatch_with(
            crate::model::command("thread.archive", thread_id),
            Continuation {
                failure: "Failed to archive thread",
                advance: None,
                busy: None,
                undo: Some(crate::ui::undo::Undoable {
                    action: "Archived",
                    kind: "archive",
                    thread: thread_id.to_owned(),
                    commands: vec![crate::model::command("thread.unarchive", thread_id)],
                    failure: "Failed to undo archive",
                }),
            },
        );
    }

    /// Sends `command` for the notice `busy`, which shows its in-flight
    /// label until the server answers.
    pub(crate) fn dispatch_busy(&mut self, command: Value, busy: &str, failure: &'static str) {
        self.notice_busy.insert(busy.to_owned());
        self.dispatch_with(
            command,
            Continuation {
                failure,
                advance: None,
                busy: Some(busy.to_owned()),
                undo: None,
            },
        );
    }

    fn dispatch_with(&mut self, command: Value, continuation: Continuation) {
        if let Some(id) = command["commandId"].as_str() {
            self.pending_commands.insert(id.to_owned(), continuation);
        }
        self.dispatch(command);
    }

    /// Settles or snoozes `thread_id` (`command`), moving on when it is the
    /// open thread; `batch` are parked with it and are no target.
    pub(crate) fn park(&mut self, thread_id: &str, command: Value, park: Park, batch: &[String]) {
        let failure = match park {
            Park::Settle => "Failed to settle thread",
            Park::Snooze => "Failed to snooze thread",
        };
        let advance = self
            .next_after_park(thread_id, batch)
            .map(|next| (thread_id.to_owned(), park, next));
        // What puts it back: unsettle, then the pin and the snooze it had.
        let thread = self
            .state
            .threads
            .iter()
            .find(|t| t.id == thread_id)
            .cloned();
        let undo = thread.map(|thread| {
            let mut unpark = crate::model::command(
                match park {
                    Park::Settle => "thread.unsettle",
                    Park::Snooze => "thread.unsnooze",
                },
                thread_id,
            );
            unpark["reason"] = serde_json::json!("user");
            let mut commands = vec![unpark];
            if park == Park::Settle {
                if thread.pinned_at.is_some() {
                    let mut pin = crate::model::command("thread.pin", thread_id);
                    if let Some(key) = &thread.pin_order_key {
                        pin["orderKey"] = serde_json::json!(key);
                    }
                    commands.push(pin);
                }
                if let Some(until) = thread
                    .snoozed_until
                    .clone()
                    .filter(|_| crate::snooze::effective_snoozed(&thread, self.now))
                {
                    let mut snooze = crate::model::command("thread.snooze", thread_id);
                    snooze["snoozedUntil"] = serde_json::json!(until);
                    commands.push(snooze);
                }
            }
            crate::ui::undo::Undoable {
                action: match park {
                    Park::Settle => "Settled",
                    Park::Snooze => "Snoozed",
                },
                kind: match park {
                    Park::Settle => "settle",
                    Park::Snooze => "snooze",
                },
                thread: thread_id.to_owned(),
                commands,
                failure: match park {
                    Park::Settle => "Failed to undo settle",
                    Park::Snooze => "Failed to wake thread",
                },
            }
        });
        self.dispatch_with(
            command,
            Continuation {
                failure,
                advance,
                busy: None,
                undo,
            },
        );
    }

    /// `planForwardNavigation`: only for the open thread.
    fn next_after_park(&self, thread_id: &str, batch: &[String]) -> Option<Next> {
        if self.selected.as_deref() != Some(thread_id) {
            return None;
        }
        let order = self.ordered_sidebar_threads();
        let start = order
            .iter()
            .position(|id| id == thread_id)
            .map_or(0, |i| i + 1);
        let parked = |id: &String| {
            self.state
                .threads
                .iter()
                .find(|t| &t.id == id)
                .is_none_or(|t| {
                    t.settled_override.as_deref() == Some("settled")
                        || crate::snooze::effective_snoozed(t, self.now)
                })
        };
        let next = (0..order.len())
            .map(|i| &order[(start + i) % order.len()])
            .find(|id| *id != thread_id && !batch.contains(id) && !parked(id));
        if let Some(next) = next {
            return Some(Next::Thread(next.clone()));
        }
        let project = self
            .state
            .threads
            .iter()
            .find(|t| t.id == thread_id)?
            .project_id
            .clone();
        Some(Next::Draft(project))
    }

    /// A command's receipt (`ok`) or refusal (`message`): true when it was
    /// one of the commands followed here.
    pub(crate) fn command_settled(&mut self, id: &str, refusal: Option<&str>) -> bool {
        let Some(continuation) = self.pending_commands.remove(id) else {
            return false;
        };
        if let Some(busy) = &continuation.busy {
            self.notice_busy.remove(busy);
        }
        match refusal {
            Some(message) => {
                let description = if message.trim().is_empty() {
                    "An error occurred."
                } else {
                    message
                };
                self.toasts.add(
                    toasts::Toast::new(toasts::Kind::Error, continuation.failure)
                        .description(description),
                );
            }
            None => {
                if let Some(undoable) = continuation.undo {
                    self.undo.push(undoable);
                }
                if let Some((thread, park, next)) = continuation.advance {
                    self.advance = Some(Advance {
                        thread,
                        park,
                        next,
                        since: Instant::now(),
                    });
                }
            }
        }
        true
    }

    /// Moves on once the shell shows the parked thread (every frame); gives
    /// up when the user left it or the shell never catches up.
    pub(crate) fn poll_advance(&mut self) {
        let Some(advance) = &self.advance else {
            return;
        };
        if self.selected.as_deref() != Some(advance.thread.as_str())
            || advance.since.elapsed() > Duration::from_secs(5)
        {
            self.advance = None;
            return;
        }
        let parked = self
            .state
            .threads
            .iter()
            .find(|t| t.id == advance.thread)
            .is_some_and(|t| match advance.park {
                Park::Settle => t.settled_override.as_deref() == Some("settled"),
                Park::Snooze => crate::snooze::effective_snoozed(t, self.now),
            });
        if !parked {
            return;
        }
        let Some(advance) = self.advance.take() else {
            return;
        };
        match advance.next {
            Next::Thread(id) => self.select_thread(Some(id)),
            Next::Draft(project) => self.start_draft(Some(project)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::app::Options;
    use crate::model::Thread;
    use crate::protocol::{Action, Connection, Event, Route};

    #[test]
    fn settling_the_open_thread_moves_to_the_next_once_the_shell_shows_it() {
        let mut harness = egui_kittest::Harness::builder().build_eframe(|cc| {
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
        let ctx = harness.ctx.clone();
        let (connection, actions, events) = Connection::fake(crate::repaint(&ctx));
        let app = harness.state_mut();
        app.use_connection_for_tests(connection);
        let thread = |id: &str, settled: bool| {
            Arc::new(Thread {
                id: id.into(),
                project_id: "p".into(),
                title: id.into(),
                created_at: "2026-10-29T10:00:00.000Z".into(),
                updated_at: "2026-10-29T10:00:00.000Z".into(),
                settled_override: settled.then(|| "settled".into()),
                ..Default::default()
            })
        };
        app.state.threads = vec![thread("a", false), thread("b", true), thread("c", false)];
        app.select_thread(Some("a".into()));
        while actions.try_recv().is_ok() {}

        let settle = crate::model::command("thread.settle", "a");
        let id = settle["commandId"].as_str().unwrap().to_owned();
        app.park("a", settle, Park::Settle, &[]);
        assert!(matches!(actions.try_recv(), Ok(Action::Dispatch(_))));
        // Received, but the shell still shows it active: no move yet.
        events
            .send(Event::Data(Route::Command(id), serde_json::json!({})))
            .unwrap();
        app.poll_events();
        assert_eq!(app.selected.as_deref(), Some("a"));
        // Settled in the shell: the next unsettled thread opens.
        app.state.threads[0] = thread("a", true);
        app.poll_events();
        assert_eq!(app.selected.as_deref(), Some("c"));

        // A refused settle stays and says so.
        let settle = crate::model::command("thread.settle", "c");
        let id = settle["commandId"].as_str().unwrap().to_owned();
        app.park("c", settle, Park::Settle, &[]);
        events
            .send(Event::Failed(Route::Command(id), "Nope".into()))
            .unwrap();
        app.poll_events();
        assert_eq!(app.selected.as_deref(), Some("c"));
        assert!(app.advance.is_none());
    }
}
