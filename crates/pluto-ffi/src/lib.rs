//! Pluto's core for the Android app, exposed to Kotlin through UniFFI: the
//! session with the server (`session`), what the composers send
//! (`compose`), and the data each screen shows (`views`), built from
//! `pluto_core` like the desktop's. Compose draws; this crate decides.
//!
//! The state lives in a [`Hub`] behind a lock. The connection's events
//! arrive on a pump thread; after each change the hub calls the interface's
//! [`Listener`], which reads the screens' data again.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

uniffi::setup_scaffolding!();

mod attachments;
mod badge;
mod catalog;
mod compose;
mod icons;
mod images;
mod markdown;
mod prefs;
mod session;
mod times;
mod views;
// The pages and sheets, each with its own `impl Core` block.
mod diff;
mod licenses;
mod memory;
mod projects;
mod settings;
mod sheets;
mod tasks;
mod usage;

pub use badge::Badge;
pub use compose::Target;
pub use markdown::{Block, BlockKind, Span};
pub use session::Net;
pub use views::*;

/// The interface, told that the screens' data changed.
#[uniffi::export(with_foreign)]
pub trait Listener: Send + Sync {
    fn changed(&self);
}

/// The app's state: the session and the composers' choices.
pub struct App {
    pub session: session::Session,
    pub drafts: compose::Drafts,
    /// The phone shows a 24-hour clock.
    pub clock_24h: bool,
    /// Answers typed into question cards, by request then question.
    pub answers: std::collections::HashMap<
        String,
        std::collections::BTreeMap<String, pluto_core::requests::DraftAnswer>,
    >,
}

pub struct Hub {
    app: Mutex<App>,
    listener: Arc<dyn Listener>,
}

impl Hub {
    fn lock(&self) -> MutexGuard<'_, App> {
        self.app.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    /// Changes the state, then tells the interface.
    pub fn update<R>(self: &Arc<Self>, change: impl FnOnce(&mut App, &Arc<Hub>) -> R) -> R {
        let result = {
            let mut app = self.lock();
            change(&mut app, self)
        };
        self.listener.changed();
        result
    }

    /// Runs `change` after `delay`, unless the app is gone.
    pub fn later(
        hub: &Arc<Hub>,
        delay: Duration,
        change: impl FnOnce(&mut App, &Arc<Hub>) + Send + 'static,
    ) {
        let hub = Arc::downgrade(hub);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            if let Some(hub) = hub.upgrade() {
                hub.update(change);
            }
        });
    }
}

/// The app's handle on its core.
#[derive(uniffi::Object)]
pub struct Core {
    hub: Arc<Hub>,
}

impl Core {
    fn read<R>(&self, read: impl FnOnce(&App) -> R) -> R {
        read(&self.hub.lock())
    }

    fn update<R>(&self, change: impl FnOnce(&mut App, &Arc<Hub>) -> R) -> R {
        self.hub.update(change)
    }
}

#[uniffi::export]
impl Core {
    /// `files_dir` keeps the phone's settings; `clock_24h` is the phone's
    /// clock format.
    #[uniffi::constructor]
    pub fn new(files_dir: String, clock_24h: bool, listener: Arc<dyn Listener>) -> Arc<Self> {
        let dir = (!files_dir.is_empty()).then(|| std::path::PathBuf::from(files_dir));
        let app = App {
            session: session::Session::new(prefs::Prefs::load(dir)),
            drafts: compose::Drafts::default(),
            clock_24h,
            answers: Default::default(),
        };
        Arc::new(Self {
            hub: Arc::new(Hub {
                app: Mutex::new(app),
                listener,
            }),
        })
    }
}

/// The core's version, for the About row.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// An icon's path data in its 24px box: the app's set, else the Lucide
/// catalog (project icons).
#[uniffi::export]
pub fn icon_paths(name: String) -> Vec<String> {
    let markup = icons::markup(&name);
    let markup = if markup.is_empty() {
        icons::lucide(&name)
    } else {
        markup
    };
    images::icon_paths(markup)
}

/// A provider's mark (`views` name it) as PNG, `px` on its longer side.
#[uniffi::export]
pub fn mark_png(name: String, px: u32) -> Option<Vec<u8>> {
    images::svg_png(images::mark(&name)?, px)
}

/// "6m 40s" from seconds ("Working for …").
#[uniffi::export]
pub fn elapsed(seconds: i64) -> String {
    times::elapsed(seconds)
}

/// "192.168.1.20:3774" from "http://192.168.1.20:3774/".
#[uniffi::export]
pub fn server_host(server: String) -> String {
    session::url_host(&server)
}

/// Whether `address` is one the app connects to (plain http only for
/// private addresses), with the reason when it is not.
#[uniffi::export]
pub fn check_address(address: String) -> Option<String> {
    pluto_core::protocol::socket_url(&address).err()
}
