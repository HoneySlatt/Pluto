//! What is on screen, layer over layer: the home draft or a thread, the
//! pages pushed from the right, the drawer and a bottom sheet. Each layer
//! opened pushes a WebView history entry, so Android's back gesture (the
//! WebView going back) closes the top layer; the app's own close buttons go
//! back through the history too. A closed page or sheet stays on screen for
//! its exit animation.

use std::collections::HashMap;

use dioxus::prelude::*;
use pluto_core::model::ModelSelection;

use crate::session::Ctx;

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Settings,
    /// A thread opened from a page (a project's threads).
    Thread(String),
    /// The changed files of a thread, from `focus` when a file was picked.
    Diff {
        thread: String,
        run: Option<String>,
        focus: Option<String>,
    },
    Projects,
    Project(String),
    Tasks,
    /// The task editor: an existing task's id, or a new task.
    TaskEdit(Option<String>),
    Usage,
    /// Personalization: custom instructions and memory.
    Memory,
    Licenses,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Sheet {
    Model,
    ThreadMenu(String),
    Attach,
    Project,
    Workspace,
    Branch,
    /// A Settings choice by key.
    Setting(&'static str),
    /// A project's New threads default (`model`, `access`, `workspace`).
    ProjectSetting(String, &'static str),
    RemoveProject(String),
    DeleteThread(String),
    Rename(String),
    TaskMenu(String),
    /// A task editor field (`project`, `workspace`, `model`).
    TaskField(&'static str),
    /// Add a memory (`None`) or edit one.
    MemoryEdit(Option<String>),
    MemoryMenu(String),
    ClearMemories,
    /// A license's text by key (`licenses::title`).
    License(String),
}

#[derive(Clone, Debug, PartialEq)]
enum Layer {
    Drawer,
    /// The base view changed; going back opens `prev` again.
    View(Option<String>),
    /// A page; a thread page restores the selection it replaced.
    Page(Option<Option<String>>),
    Sheet,
}

/// How long a closed page or sheet stays for its exit (`main.css`).
const EXIT: std::time::Duration = std::time::Duration::from_millis(240);

#[derive(Default)]
pub struct Nav {
    pub drawer: bool,
    pub pages: Vec<Page>,
    pub sheet: Option<Sheet>,
    /// The page and the sheet on their way out, with their exit's serial.
    pub leaving_page: Option<(Page, u64)>,
    pub leaving_sheet: Option<(Sheet, u64)>,
    /// The selection a leaving thread page gives back after its exit.
    restore: Option<(u64, Option<String>)>,
    exits: u64,
    layers: Vec<Layer>,
    /// A reset went back over every layer at once: its popstate is not a
    /// layer closing.
    resetting: bool,
}

/// The composers' drafts and choices, kept while the app runs.
#[derive(Default)]
pub struct Drafts {
    /// Text by composer: `chat`, `code` or a thread id.
    pub text: HashMap<String, String>,
    /// Sends so far: a composer remounts empty after one.
    pub cleared: u64,
    pub research: bool,
    pub image: bool,
    pub temporary: bool,
    /// The Code draft's project.
    pub project: Option<String>,
    /// The Code draft starts in a new worktree (else the checkout), off
    /// `base` when one was picked.
    pub worktree: Option<bool>,
    pub base: Option<String>,
    /// Picks by composer key.
    pub models: HashMap<String, ModelSelection>,
    pub access: HashMap<String, String>,
    pub plan: HashMap<String, bool>,
    /// The scheduled task editor's draft.
    pub task: Option<crate::tasks::TaskDraft>,
    pub personalization: crate::memory::Personalization,
}

/// The screens' handle on the session, the layers and the drafts.
#[derive(Clone, Copy)]
pub struct Ui {
    pub ctx: Ctx,
    pub nav: Signal<Nav>,
    pub drafts: Signal<Drafts>,
}

fn push_history() {
    document::eval("history.pushState({ pluto: true }, '');");
}

impl Ui {
    /// Closes the top layer, the way the back gesture does.
    pub fn back(self) {
        document::eval("history.back();");
    }

    /// The history went back: the top layer closes.
    pub fn popped(mut self) {
        if std::mem::take(&mut self.nav.write().resetting) {
            return;
        }
        let layer = self.nav.write().layers.pop();
        match layer {
            Some(Layer::Drawer) => self.nav.write().drawer = false,
            Some(Layer::View(prev)) => self.ctx.select(prev),
            Some(Layer::Page(prev)) => {
                let serial = {
                    let mut nav = self.nav.write();
                    nav.exits += 1;
                    let serial = nav.exits;
                    nav.leaving_page = nav.pages.pop().map(|page| (page, serial));
                    // A thread page keeps its thread while it slides out.
                    nav.restore = prev.map(|prev| (serial, prev));
                    serial
                };
                self.finish_exit(serial);
            }
            Some(Layer::Sheet) => {
                let serial = {
                    let mut nav = self.nav.write();
                    nav.exits += 1;
                    let serial = nav.exits;
                    nav.leaving_sheet = nav.sheet.take().map(|sheet| (sheet, serial));
                    serial
                };
                self.finish_exit(serial);
            }
            None => {}
        }
    }

    /// Drops the leaving page or sheet once its exit played, and gives a
    /// thread page's replaced selection back.
    fn finish_exit(mut self, serial: u64) {
        dioxus::core::spawn_forever(async move {
            tokio::time::sleep(EXIT).await;
            let mut nav = self.nav.write();
            if nav.leaving_page.as_ref().is_some_and(|(_, s)| *s == serial) {
                nav.leaving_page = None;
            }
            if nav
                .leaving_sheet
                .as_ref()
                .is_some_and(|(_, s)| *s == serial)
            {
                nav.leaving_sheet = None;
            }
            let restore = nav.restore.take_if(|(s, _)| *s == serial);
            drop(nav);
            if let Some((_, prev)) = restore {
                self.ctx.select(prev);
            }
        });
    }

    fn top(&self) -> Option<Layer> {
        self.nav.peek().layers.last().cloned()
    }

    /// Replaces the drawer or a sheet on top with `layer`, else pushes it.
    fn place(mut self, layer: Layer) {
        let mut nav = self.nav.write();
        match nav.layers.last() {
            Some(Layer::Drawer) => {
                nav.drawer = false;
                *nav.layers.last_mut().unwrap() = layer;
            }
            Some(Layer::Sheet) => {
                nav.sheet = None;
                *nav.layers.last_mut().unwrap() = layer;
            }
            _ => {
                nav.layers.push(layer);
                drop(nav);
                push_history();
            }
        }
    }

    /// The drawer opens over the base views only: a page over them would
    /// stay in front of the thread it opens.
    pub fn open_drawer(mut self) {
        let nav = self.nav.peek();
        if nav.drawer || !nav.pages.is_empty() || nav.sheet.is_some() {
            return;
        }
        drop(nav);
        let mut nav = self.nav.write();
        nav.drawer = true;
        nav.layers.push(Layer::Drawer);
        drop(nav);
        push_history();
    }

    /// A page still leaving gives its place, and its selection, at once.
    fn settle(mut self) {
        let restore = {
            let mut nav = self.nav.write();
            nav.leaving_page = None;
            nav.restore.take()
        };
        if let Some((_, prev)) = restore {
            self.ctx.select(prev);
        }
    }

    /// Opens `id` as the base view (from the drawer, the home chips).
    pub fn open_thread(self, id: String) {
        self.settle();
        let prev = self.ctx.session.peek().selected.clone();
        if prev.as_deref() == Some(id.as_str()) {
            if self.top() == Some(Layer::Drawer) {
                self.back();
            }
            return;
        }
        self.place(Layer::View(prev));
        self.ctx.select(Some(id));
    }

    /// The home draft of the current space.
    pub fn go_home(self) {
        self.settle();
        let prev = self.ctx.session.peek().selected.clone();
        if prev.is_none() {
            if self.top() == Some(Layer::Drawer) {
                self.back();
            }
            return;
        }
        self.place(Layer::View(prev));
        self.ctx.select(None);
    }

    pub fn open_page(mut self, page: Page) {
        self.settle();
        let prev = match &page {
            Page::Thread(id) => {
                let prev = self.ctx.session.peek().selected.clone();
                self.ctx.select(Some(id.clone()));
                Some(prev)
            }
            _ => None,
        };
        self.place(Layer::Page(prev));
        self.nav.write().pages.push(page);
    }

    pub fn open_sheet(mut self, sheet: Sheet) {
        let mut nav = self.nav.write();
        nav.leaving_sheet = None;
        if nav.sheet.is_some() {
            nav.sheet = Some(sheet);
            return;
        }
        nav.sheet = Some(sheet);
        nav.layers.push(Layer::Sheet);
        drop(nav);
        push_history();
    }

    /// Leaves the server: the layers, the drafts and the session's data of
    /// that server go; the next server starts clean.
    pub fn disconnect(mut self) {
        self.reset();
        self.drafts.set(Drafts::default());
        self.ctx.disconnect();
    }

    /// Leaves every layer at once (Disconnect, a project's New thread).
    pub fn reset(mut self) {
        let mut nav = self.nav.write();
        let depth = nav.layers.len();
        *nav = Nav::default();
        if depth > 0 {
            nav.resetting = true;
            drop(nav);
            document::eval(&format!("history.go(-{depth});"));
        }
    }
}
