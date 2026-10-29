//! Which thread events deserve a notification (`threadNotifications.ts`):
//! a thread finished, failed, hit a usage limit, or needs input or approval.
//! Shared by the desktop's notifications and the phone's background service.

use std::collections::HashMap;
use std::sync::Arc;

use crate::model::Thread;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    Approval,
    Input,
    Working,
    /// Stopped, with background work that will wake it (runtime `idle`).
    Waiting,
    Failed,
    /// Failed on a usage limit.
    Limited,
    Ready,
}

/// `resolveSidebarThreadStatus`.
pub fn status(thread: &Thread) -> Status {
    let subagent = thread.pending_subagent_request.as_deref();
    let runtime = thread.runtime.as_ref();
    if thread.has_pending_approvals || subagent == Some("approval") {
        Status::Approval
    } else if thread.has_pending_user_input || subagent == Some("user_input") {
        Status::Input
    } else if runtime.is_some_and(|r| r.is_active()) {
        Status::Working
    } else if runtime.is_some_and(|r| r.status == "idle") {
        Status::Waiting
    } else if let Some(runtime) = runtime.filter(|r| r.status == "failed") {
        if runtime.last_error_class.as_deref() == Some("usage_limit") {
            Status::Limited
        } else {
            Status::Failed
        }
    } else {
        Status::Ready
    }
}

#[derive(Clone)]
struct Note {
    thread: Arc<Thread>,
    /// The run and status that need the user, once notified.
    attention: Option<String>,
    /// When the latest completed run ended.
    completion: Option<i64>,
}

/// What the last scan saw of each thread.
#[derive(Default)]
pub struct Scanner {
    previous: HashMap<String, Note>,
}

/// Something a thread did that the user should hear about.
#[derive(Debug, PartialEq)]
pub struct Event {
    pub thread_id: String,
    pub completion: bool,
    pub title: &'static str,
    pub body: String,
}

impl Scanner {
    /// The events in `threads` since the last scan. A thread seen for the
    /// first time only sets its baseline.
    pub fn scan(&mut self, threads: &[Arc<Thread>]) -> Vec<Event> {
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
        let mut notes = Scanner::default();
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
        let mut notes = Scanner::default();
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
