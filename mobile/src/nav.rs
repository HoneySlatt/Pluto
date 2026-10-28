//! What is on screen, layer over layer: the home draft or a thread, the
//! pages pushed from the right, the drawer and a bottom sheet. Each layer
//! opened pushes a WebView history entry, so Android's back gesture (the
//! WebView going back) closes the top layer; the app's own close buttons go
//! back through the history too.

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

#[derive(Default)]
pub struct Nav {
    pub drawer: bool,
    pub pages: Vec<Page>,
    pub sheet: Option<Sheet>,
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
                self.nav.write().pages.pop();
                if let Some(prev) = prev {
                    self.ctx.select(prev);
                }
            }
            Some(Layer::Sheet) => self.nav.write().sheet = None,
            None => {}
        }
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

    pub fn open_drawer(mut self) {
        if self.nav.peek().drawer {
            return;
        }
        let mut nav = self.nav.write();
        nav.drawer = true;
        nav.layers.push(Layer::Drawer);
        drop(nav);
        push_history();
    }

    /// Opens `id` as the base view (from the drawer, the home chips).
    pub fn open_thread(self, id: String) {
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
        if nav.sheet.is_some() {
            nav.sheet = Some(sheet);
            return;
        }
        nav.sheet = Some(sheet);
        nav.layers.push(Layer::Sheet);
        drop(nav);
        push_history();
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
