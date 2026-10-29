//! Thread notifications on the phone (`ThreadNotificationCoordinator.tsx`,
//! through `pluto_core::notifications`): while Pluto is in the background
//! and the setting is on, a thread that finishes, fails, or needs input or
//! approval waits here until the app's service posts it. In the foreground
//! the screens show it already.

use std::sync::Arc;

use pluto_core::model::Thread;
use pluto_core::notifications::Scanner;

use crate::Core;

/// The phone setting that turns them on.
pub const PREF: &str = "threadNotifications";

/// A notification to post.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ThreadNotice {
    pub thread_id: String,
    /// "Thread completed", "Input needed"…
    pub title: String,
    /// The thread's title.
    pub body: String,
    /// The thread waits on the user (input, approval), rather than done.
    pub attention: bool,
}

pub struct Notices {
    scanner: Scanner,
    waiting: Vec<ThreadNotice>,
    /// The app is on screen.
    foreground: bool,
}

impl Default for Notices {
    fn default() -> Self {
        Self {
            scanner: Scanner::default(),
            waiting: Vec::new(),
            foreground: true,
        }
    }
}

impl Notices {
    /// Keeps up with `threads`; their events wait for the service when the
    /// setting is `on` and the app is away.
    pub fn scan(&mut self, threads: &[Arc<Thread>], on: bool) {
        let events = self.scanner.scan(threads);
        if !on || self.foreground {
            return;
        }
        self.waiting
            .extend(events.into_iter().map(|event| ThreadNotice {
                attention: !event.completion && event.title != "Thread failed",
                thread_id: event.thread_id,
                title: event.title.to_owned(),
                body: event.body,
            }));
    }
}

#[uniffi::export]
impl Core {
    /// The notifications waiting to be posted, once.
    pub fn take_notices(&self) -> Vec<ThreadNotice> {
        std::mem::take(&mut self.hub.lock().session.notices.waiting)
    }

    /// The app went to the background or came back.
    pub fn set_foreground(&self, foreground: bool) {
        let mut app = self.hub.lock();
        app.session.notices.foreground = foreground;
        if foreground {
            app.session.notices.waiting.clear();
        }
    }

    pub fn notifications_enabled(&self) -> bool {
        self.read(|app| app.session.prefs.flag(PREF))
    }

    pub fn set_notifications(&self, on: bool) {
        self.update(|app, _| app.session.prefs.set(PREF, serde_json::json!(on)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pluto_core::model::RunSummary;

    fn thread(status: &str, completed_at: Option<&str>) -> Arc<Thread> {
        Arc::new(Thread {
            id: "a".into(),
            title: "Fix the build".into(),
            latest_run: Some(RunSummary {
                run_id: "r1".into(),
                status: status.into(),
                completed_at: completed_at.map(Into::into),
                ..Default::default()
            }),
            ..Default::default()
        })
    }

    #[test]
    fn only_the_background_with_the_setting_on_keeps_events() {
        let mut notices = Notices::default();
        notices.scan(&[thread("running", None)], true);
        // On screen, the finished run sets no notice.
        notices.scan(
            &[thread("completed", Some("2026-10-29T10:00:00.000Z"))],
            true,
        );
        assert!(notices.waiting.is_empty());
        notices.foreground = false;
        notices.scan(&[thread("running", None)], true);
        notices.scan(
            &[thread("completed", Some("2026-10-29T10:05:00.000Z"))],
            true,
        );
        assert_eq!(
            notices.waiting,
            [ThreadNotice {
                thread_id: "a".into(),
                title: "Thread completed".into(),
                body: "Fix the build".into(),
                attention: false,
            }]
        );
    }
}
