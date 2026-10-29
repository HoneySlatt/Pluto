//! Thread notifications (`ThreadNotificationCoordinator.tsx`,
//! `threadNotifications.ts`): when a thread finishes, fails, or needs input
//! or approval, a system notification, or a toast while Pluto has focus and
//! in-app notifications are on. Pluto, after Claude Code: a system
//! notification also comes while Pluto has focus on another thread, not only
//! while it is in the background. They go through `notify-send` (libnotify),
//! and clicking one opens its thread. Not ported: the sounds, the dock badge,
//! and closing the notifications when the window gets focus.

use std::sync::mpsc;

use eframe::egui;

use pluto_core::notifications::{Event, Scanner};

use crate::app::App;
use crate::ui::toasts;

/// The icon the notifications show: Pluto's mark, written once to the cache.
const ICON: &[u8] = include_bytes!("../../assets/icon.svg");

pub(crate) struct ThreadNotifications {
    scanner: Scanner,
    /// The thread list's revision last scanned; `None` before the first scan.
    revision: Option<u64>,
    /// Threads whose notification was clicked.
    opened: (mpsc::Sender<String>, mpsc::Receiver<String>),
}

impl Default for ThreadNotifications {
    fn default() -> Self {
        Self {
            scanner: Scanner::default(),
            revision: None,
            opened: mpsc::channel(),
        }
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
        let events = self.thread_notifications.scanner.scan(&self.state.threads);
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
