//! Thread notifications (`ThreadNotificationCoordinator.tsx`,
//! `threadNotifications.ts`): when a thread finishes, fails, or needs input
//! or approval, a system notification, or a toast while Pluto has focus and
//! in-app notifications are on. Pluto, after Claude Code: a system
//! notification also comes while Pluto has focus on another thread, not only
//! while it is in the background. They go through `notify-send` (libnotify),
//! and clicking one opens its thread. Not ported: the sounds, the dock badge,
//! and closing the notifications when the window gets focus.

use std::collections::HashMap;
use std::sync::{Arc, mpsc};

use eframe::egui;

use crate::app::App;
use crate::model::Thread;
use crate::ui::sidebar::{Status, status};
use crate::ui::toasts;

/// The icon the notifications show: Pluto's mark, written once to the cache.
const ICON: &[u8] = include_bytes!("../../assets/icon.svg");

#[derive(Clone)]
struct Note {
    thread: Arc<Thread>,
    /// The run and status that need the user, once notified.
    attention: Option<String>,
    /// When the latest completed run ended.
    completion: Option<i64>,
}

pub(crate) struct ThreadNotifications {
    previous: HashMap<String, Note>,
    /// The thread list's revision last scanned; `None` before the first scan.
    revision: Option<u64>,
    /// Threads whose notification was clicked.
    opened: (mpsc::Sender<String>, mpsc::Receiver<String>),
}

impl Default for ThreadNotifications {
    fn default() -> Self {
        Self {
            previous: HashMap::new(),
            revision: None,
            opened: mpsc::channel(),
        }
    }
}

/// Something a thread did that the user should hear about.
#[derive(Debug, PartialEq)]
pub(crate) struct Event {
    pub thread_id: String,
    pub completion: bool,
    pub title: &'static str,
    pub body: String,
}

impl ThreadNotifications {
    /// The events in `threads` since the last scan. A thread seen for the
    /// first time only sets its baseline.
    pub(crate) fn scan(&mut self, threads: &[Arc<Thread>]) -> Vec<Event> {
        let mut next = HashMap::new();
        let mut events = Vec::new();
        for thread in threads {
            // Subagents notify through the thread above them.
            if thread.lineage.relationship_to_parent.as_deref() == Some("subagent") {
                continue;
            }
            let prior = self.previous.get(&thread.id);
            // The same thread cannot produce a new notification.
            if let Some(prior) = prior.filter(|prior| Arc::ptr_eq(&prior.thread, thread)) {
                next.insert(thread.id.clone(), prior.clone());
                continue;
            }
            let run = thread.latest_run.as_ref();
            let mut state = status(thread);
            if state == Status::Ready && run.is_some_and(|run| run.status == "failed") {
                state = Status::Failed;
            }
            let attention = match state {
                Status::Input => Some("input"),
                Status::Approval => Some("approval"),
                Status::Failed => Some("failed"),
                Status::Limited => Some("limited"),
                _ => None,
            }
            .map(|kind| {
                let run_id = run.map(|run| run.run_id.as_str()).unwrap_or_default();
                format!("{run_id}:{kind}")
            });
            // Commands left running (a dev server) read as ready; subagents and monitors wait.
            let completed_at = run
                .filter(|run| run.status == "completed")
                .and_then(|run| run.completed_at.as_deref())
                .and_then(crate::timeline::parse_time)
                .map(|at| at.timestamp_millis());
            let completion = if state == Status::Ready && completed_at.is_some() {
                completed_at
            } else {
                prior.and_then(|prior| prior.completion)
            };
            next.insert(
                thread.id.clone(),
                Note {
                    thread: thread.clone(),
                    attention: attention.clone(),
                    completion,
                },
            );
            let Some(prior) = prior else { continue };
            if thread.archived_at.is_some() {
                continue;
            }
            let title = if attention.is_some() && attention != prior.attention {
                match state {
                    Status::Approval => "Approval needed",
                    Status::Limited => "Usage limit reached",
                    Status::Failed => "Thread failed",
                    _ => "Input needed",
                }
            } else if completion.is_some_and(|at| prior.completion.is_none_or(|before| at > before))
            {
                "Thread completed"
            } else {
                continue;
            };
            events.push(Event {
                thread_id: thread.id.clone(),
                completion: title == "Thread completed",
                title,
                body: thread.title.clone(),
            });
        }
        self.previous = next;
        events
    }
}

/// Shows `event` as a system notification; a click sends its thread to `opened`.
fn notify_desktop(event: Event, opened: mpsc::Sender<String>, ctx: egui::Context) {
    let icon = icon_path();
    std::thread::spawn(move || {
        let mut command = std::process::Command::new("notify-send");
        command.args(["--app-name=Pluto", "--action=default=Open thread"]);
        if let Some(icon) = &icon {
            command.arg(format!("--icon={}", icon.display()));
        }
        command.args(["--", event.title, &event.body]);
        let Ok(output) = command
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
        else {
            return;
        };
        if String::from_utf8_lossy(&output.stdout).trim() == "default" {
            let _ = opened.send(event.thread_id);
            ctx.request_repaint();
        }
    });
}

/// Pluto's mark in the cache directory, written on first use.
fn icon_path() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::Path::new(&home).join(".cache"))
        })?
        .join("pluto");
    let path = dir.join("notification-icon.svg");
    if !path.exists() {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&path, ICON).ok()?;
    }
    Some(path)
}

impl App {
    /// Notifies the thread events since the last frame, and opens the
    /// threads whose notification was clicked.
    pub(crate) fn thread_notifications(&mut self, ctx: &egui::Context) {
        while let Ok(thread_id) = self.thread_notifications.opened.1.try_recv() {
            self.settings_page = None;
            self.select_thread(Some(thread_id));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        let revision = self.state.revision;
        if self.thread_notifications.revision == Some(revision) {
            return;
        }
        self.thread_notifications.revision = Some(revision);
        let events = self.thread_notifications.scan(&self.state.threads);
        if events.is_empty() {
            return;
        }
        let mode = self.client_setting("notificationMode");
        let desktop = matches!(
            mode.as_str(),
            Some("notifications" | "notifications-and-sound")
        );
        let in_app = self.client_setting("inAppNotificationsEnabled") == true;
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        for event in events {
            let viewing =
                self.settings_page.is_none() && self.selected.as_deref() == Some(&event.thread_id);
            if focused && viewing {
                continue;
            }
            if in_app && focused {
                let kind = if event.completion {
                    toasts::Kind::Success
                } else if event.title == "Thread failed" {
                    toasts::Kind::Error
                } else {
                    toasts::Kind::Info
                };
                let mut toast = toasts::Toast::new(kind, event.title).description(event.body);
                toast.action = Some((
                    "Open thread".into(),
                    toasts::Action::OpenThread(event.thread_id),
                ));
                self.toasts.add(toast);
            } else if desktop {
                notify_desktop(
                    event,
                    self.thread_notifications.opened.0.clone(),
                    ctx.clone(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RunSummary;

    fn thread(id: &str, run: Option<(&str, &str, Option<&str>)>) -> Arc<Thread> {
        Arc::new(Thread {
            id: id.into(),
            title: format!("Thread {id}"),
            created_at: "2026-10-28T10:00:00.000Z".into(),
            updated_at: "2026-10-28T10:00:00.000Z".into(),
            latest_run: run.map(|(run_id, status, completed_at)| RunSummary {
                run_id: run_id.into(),
                status: status.into(),
                completed_at: completed_at.map(Into::into),
                ..Default::default()
            }),
            ..Default::default()
        })
    }

    #[test]
    fn a_finished_run_notifies_once_and_a_new_thread_only_sets_its_baseline() {
        let mut notes = ThreadNotifications::default();
        let running = thread("a", Some(("r1", "running", None)));
        assert!(notes.scan(&[running]).is_empty());
        let done = thread(
            "a",
            Some(("r1", "completed", Some("2026-10-28T10:05:00.000Z"))),
        );
        let events = notes.scan(std::slice::from_ref(&done));
        assert_eq!(
            events,
            [Event {
                thread_id: "a".into(),
                completion: true,
                title: "Thread completed",
                body: "Thread a".into(),
            }]
        );
        // The same run seen again, as a new object after a reload, stays quiet.
        let reloaded = thread(
            "a",
            Some(("r1", "completed", Some("2026-10-28T10:05:00.000Z"))),
        );
        assert!(notes.scan(&[reloaded]).is_empty());
        // A thread that arrives already finished does not notify.
        let other = thread(
            "b",
            Some(("r2", "completed", Some("2026-10-28T10:06:00.000Z"))),
        );
        assert!(notes.scan(&[done, other]).is_empty());
    }

    #[test]
    fn failures_and_subagents() {
        let mut notes = ThreadNotifications::default();
        let mut child = (*thread("c", Some(("r3", "running", None)))).clone();
        child.lineage.relationship_to_parent = Some("subagent".into());
        let running = thread("a", Some(("r1", "running", None)));
        assert!(notes.scan(&[running, Arc::new(child.clone())]).is_empty());
        let failed = thread("a", Some(("r1", "failed", None)));
        child.latest_run = Some(RunSummary {
            run_id: "r3".into(),
            status: "completed".into(),
            completed_at: Some("2026-10-28T10:07:00.000Z".into()),
            ..Default::default()
        });
        let events = notes.scan(&[failed, Arc::new(child)]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Thread failed");
    }
}
