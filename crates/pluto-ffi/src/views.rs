//! What each screen shows, built from the session (the Dioxus app's
//! components computed the same before drawing): the shell's connection
//! state and toast, the home draft, a composer, the drawer's lists and a
//! thread with its timeline (`pluto_core::timeline`) and request cards
//! (`pluto_core::requests`). The actions the screens take are here too.

use std::sync::Arc;

use pluto_core::requests::{self, PendingApproval, PendingUserInput};
use pluto_core::timeline::{self, Row, WorkEntry};
use serde_json::json;
use zeroize::Zeroizing;

use crate::badge::{Badge, badge};
use crate::compose::{self, Target};
use crate::markdown::{self, Block};
use crate::session::{Net, Session, last_activity};
use crate::times::{chat_time, day_group, relative_time};
use crate::{App, Core, catalog};

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Toast {
    pub text: String,
    pub serial: u64,
}

/// The app's frame: which screen, and the connection's state.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ShellView {
    pub net: Net,
    pub host: String,
    /// A launch from the remembered server: `Some(true)` once it was
    /// unreachable.
    pub launch: Option<bool>,
    pub chat: bool,
    pub selected: Option<String>,
    pub toast: Option<Toast>,
    pub attempts: u8,
    /// Why the last connection failed.
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Latest {
    pub id: String,
    pub title: String,
}

/// The home draft (`#home`).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct HomeView {
    pub chat: bool,
    pub temporary: bool,
    pub research: bool,
    pub image: bool,
    /// The Code draft's project.
    pub project: Option<Badge>,
    /// Continue: the space's latest thread.
    pub latest: Option<Latest>,
    pub worktree: bool,
    pub base: String,
}

/// A composer's controls.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ComposerView {
    pub model_label: String,
    /// The model's provider mark (`mark_png`).
    pub mark: Option<String>,
    pub access_icon: String,
    pub plan: bool,
    /// The thread works: an empty field shows Stop.
    pub running: bool,
    pub online: bool,
    /// Code's access and mode button shows.
    pub code: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SendOutcome {
    /// Why nothing went (empty: refused without a message).
    pub error: Option<String>,
    /// The new thread of a draft.
    pub created: Option<String>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Dot {
    Ok,
    Warn,
    Bad,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ThreadRow {
    pub id: String,
    pub title: String,
    pub project: Option<Badge>,
    pub branch: Option<String>,
    /// `working`, `approval` or `input`.
    pub status: Option<String>,
    pub when: String,
    /// A run finished since the thread was last read (`.trow.unread`).
    pub unread: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DayGroup {
    pub label: String,
    pub rows: Vec<ThreadRow>,
}

/// The drawer (`#drawer`).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DrawerView {
    pub chat: bool,
    pub host: String,
    pub state: String,
    pub dot: Dot,
    pub projects: u32,
    pub tasks: Option<u32>,
    pub listed: bool,
    pub selected: Option<String>,
    /// Code: "Needs you", Threads and Settled.
    pub attention: Vec<ThreadRow>,
    pub active: Vec<ThreadRow>,
    pub settled: Vec<ThreadRow>,
    /// Chat: the chats by day.
    pub days: Vec<DayGroup>,
    pub empty: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Chip {
    pub icon: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ImageRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FileStat {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Changes {
    pub run: String,
    pub files: Vec<FileStat>,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Work {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub code: Option<String>,
    pub failed: bool,
    pub live: bool,
}

/// A timeline row, with its stable key.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Item {
    User {
        key: String,
        text: String,
        chips: Vec<Chip>,
        images: Vec<ImageRef>,
    },
    /// An answer or a plan: its markdown comes from `answer_blocks(key)`,
    /// asked again only when `hash` changes (the answer still streaming).
    Answer {
        key: String,
        hash: u64,
        changes: Option<Changes>,
    },
    /// A fold or a tool group's toggle; `toggle` flips it.
    Fold {
        key: String,
        label: String,
        expanded: bool,
        toggle: String,
        turn: bool,
        failure: bool,
    },
    Works {
        key: String,
        thought: Option<String>,
        entries: Vec<Work>,
        live: bool,
    },
    /// "Working for …" from `started_at` (Unix seconds), else "Working…".
    Working {
        key: String,
        started_at: Option<i64>,
    },
    Thinking {
        key: String,
    },
    Note {
        key: String,
        text: String,
    },
    /// An image Codex generated, by its path on the server's host
    /// (`generated_image`).
    Image {
        key: String,
        path: String,
    },
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ApprovalChoice {
    pub decision: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ApprovalCard {
    pub request_id: String,
    pub label: String,
    pub app: Option<String>,
    pub detail: Option<String>,
    pub warnings: Vec<String>,
    /// Decline and Approve side by side, the other options under them.
    pub pair: Vec<ApprovalChoice>,
    pub rest: Vec<ApprovalChoice>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AnswerOption {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
    pub checked: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct QuestionView {
    pub id: String,
    pub title: String,
    pub multi: bool,
    pub options: Vec<AnswerOption>,
    pub custom_allowed: bool,
    pub custom: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct QuestionCard {
    pub request_id: String,
    pub questions: Vec<QuestionView>,
    pub dismissible: bool,
    /// Every question has an answer.
    pub ready: bool,
    /// How many questions have one.
    pub answered: u32,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SubagentCard {
    pub id: String,
    pub title: String,
    pub question: bool,
}

/// A thread (`#thread`).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ThreadView {
    pub listed: bool,
    pub title: String,
    pub chat: bool,
    /// The chat's day group and model, for its header.
    pub day: String,
    pub model_label: String,
    pub project: Option<Badge>,
    pub branch: Option<String>,
    pub worktree: bool,
    pub items: Vec<Item>,
    pub approvals: Vec<ApprovalCard>,
    pub questions: Vec<QuestionCard>,
    pub subagent: Option<SubagentCard>,
    pub loaded: bool,
    /// Why it could not be followed.
    pub failure: Option<String>,
    pub launching: bool,
    /// The server no longer lists it.
    pub gone: bool,
    pub partial: bool,
    pub loading_history: bool,
    pub history_error: Option<String>,
    pub online: bool,
    pub running: bool,
}

/// A tracked command or request: still waiting, or its failure.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ReplyState {
    pub waiting: bool,
    pub done: bool,
    pub error: Option<String>,
}

fn thread_row(
    session: &Session,
    t: &pluto_core::model::Thread,
    chat: bool,
    now: chrono::DateTime<chrono::Utc>,
    clock_24h: bool,
) -> ThreadRow {
    let format = session
        .prefs
        .get("timestampFormat")
        .as_str()
        .unwrap_or("locale")
        .to_owned();
    ThreadRow {
        id: t.id.clone(),
        title: t.title.clone(),
        project: session.state.project(&t.project_id).map(badge),
        branch: t.branch.clone(),
        // A subagent below waiting on the user shows on its parent.
        status: if t.has_pending_approvals
            || t.pending_subagent_request.as_deref() == Some("approval")
        {
            Some("approval".into())
        } else if t.has_pending_user_input
            || t.pending_subagent_request.as_deref() == Some("user_input")
        {
            Some("input".into())
        } else if t.is_running() {
            Some("working".into())
        } else {
            None
        },
        when: if chat {
            chat_time(last_activity(t), now, &format, clock_24h)
        } else {
            relative_time(last_activity(t), now)
        },
        unread: unread(t),
    }
}

/// `hasUnseenCompletion` with the server's visit watermark: a run that
/// finished after the last visit; never-visited threads count as read.
fn unread(thread: &pluto_core::model::Thread) -> bool {
    let completed = thread
        .latest_run
        .as_ref()
        .and_then(|r| r.completed_at.as_deref())
        .and_then(timeline::parse_time);
    match (completed, &thread.last_visited_at) {
        (Some(completed), Some(Some(visited))) => {
            timeline::parse_time(visited).is_none_or(|visited| completed > visited)
        }
        _ => false,
    }
}

/// `work_icon` (pluto-core), with catalog icons for the desktop's own
/// glyphs.
fn work_icon(entry: &WorkEntry) -> &'static str {
    mobile_icon(pluto_core::timeline::work_icon(entry))
}

fn mobile_icon(icon: &'static str) -> &'static str {
    match icon {
        "browser-app" => "app-window",
        "t3-code" => "orbit",
        icon => icon,
    }
}

fn work(entry: &WorkEntry, live: bool) -> Work {
    // Its command, else its first changed file or detail line.
    let code = entry
        .command
        .clone()
        .or_else(|| entry.changed_files.first().cloned())
        .or_else(|| {
            entry
                .detail
                .as_ref()
                .and_then(|d| d.lines().next().map(str::to_owned))
        })
        .filter(|s| !s.trim().is_empty());
    Work {
        id: entry.id.clone(),
        label: entry.label.clone(),
        icon: work_icon(entry).into(),
        code,
        failed: entry.tone == "error" || entry.status.as_deref() == Some("failed"),
        live,
    }
}

/// A row's key: its own id, so loading earlier turns keeps the rows below.
fn row_key(row: &Row) -> String {
    match row {
        Row::User(message) => format!("u:{}", message.id),
        Row::Assistant { message, .. } => format!("a:{}", message.id),
        Row::TurnFold { run_id, .. } => format!("fold:{run_id}"),
        Row::Work { id, .. } => format!("work:{id}"),
        Row::WorkToggle { group_id, .. } => format!("toggle:{group_id}"),
        Row::WorkDetails { group_id, .. } => format!("details:{group_id}"),
        Row::Working { .. } => "working".into(),
        Row::LiveWork { group_id, .. } => format!("live:{group_id}"),
        Row::AttemptFold { attempt_id, .. } => format!("attempt:{attempt_id}"),
        Row::Compaction { id, .. } => format!("compaction:{id}"),
        Row::Thinking { .. } => "thinking".into(),
        Row::Plan(plan) => format!("plan:{}", plan.id),
        Row::HtmlRender(entry) => format!("html:{}", entry.id),
        Row::McpApp(entry) => format!("mcp:{}", entry.id),
        Row::Event { id, .. } => format!("event:{id}"),
        Row::GeneratedImage { image } => format!("image:{}", image.path.display()),
    }
}

/// The user message's text without its context references, and their chips.
fn user_parts(text: &str) -> (String, Vec<(String, String)>) {
    let references = pluto_core::context::collect(text);
    let mut out = String::new();
    let mut last = 0;
    let mut chips = Vec::new();
    for reference in references {
        out.push_str(&text[last..reference.range.start]);
        last = reference.range.end;
        chips.push((reference.kind.clone(), reference.label.clone()));
    }
    out.push_str(&text[last..]);
    (out.trim().to_owned(), chips)
}

fn chip_icon(kind: &str) -> &'static str {
    match kind {
        "research" => "globe",
        "image-request" | "image" => "image",
        "terminal" => "terminal",
        "file" | "mention" => "file",
        _ => "paperclip",
    }
}

fn user_item(key: String, message: &timeline::Message) -> Item {
    let (text, mut chips) = user_parts(&message.text);
    // Context the message carries without a link in its text.
    let records = message.context["records"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let linked = pluto_core::context::referenced(&message.text, &records);
    for record in records.iter().filter(|r| !linked.contains(r)) {
        let kind = pluto_core::context::display_kind(
            record["kind"].as_str().unwrap_or_default(),
            Some(record),
        );
        let label = record["label"]
            .as_str()
            .or(record["title"].as_str())
            .unwrap_or("Context")
            .to_owned();
        chips.push((kind, label));
    }
    let images = message
        .attachments
        .iter()
        .filter(|a| a["type"] == "image")
        .map(|a| ImageRef {
            id: a["id"].as_str().unwrap_or_default().to_owned(),
            name: a["name"].as_str().unwrap_or("Image").to_owned(),
        })
        .collect();
    for file in message.attachments.iter().filter(|a| a["type"] != "image") {
        chips.push((
            "file".into(),
            file["name"].as_str().unwrap_or("File").to_owned(),
        ));
    }
    Item::User {
        key,
        text,
        chips: chips
            .into_iter()
            .map(|(kind, label)| Chip {
                icon: chip_icon(&kind).into(),
                label,
            })
            .collect(),
        images,
    }
}

/// The answers' markdown by row key: its hash, its text, and its blocks
/// once asked for. A thread's view sends the hash alone; Compose asks for
/// the blocks of an answer whose hash changed.
type Answers = std::collections::HashMap<String, (u64, String, Option<Arc<Vec<Block>>>)>;

fn answers() -> std::sync::MutexGuard<'static, Answers> {
    static ANSWERS: std::sync::OnceLock<std::sync::Mutex<Answers>> = std::sync::OnceLock::new();
    ANSWERS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// Leaving the server forgets its answers.
pub fn reset_answers() {
    answers().clear();
}

/// Records an answer's text under its row key; its hash.
fn remember_answer(key: &str, text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    let hash = hasher.finish();
    let mut answers = answers();
    if answers.get(key).is_none_or(|(known, ..)| *known != hash) {
        // Many threads later, the cache starts over.
        if answers.len() > 4000 {
            answers.clear();
        }
        answers.insert(key.to_owned(), (hash, text.to_owned(), None));
    }
    hash
}

fn items(rows: Vec<Row>) -> Vec<Item> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (index, row) in rows.into_iter().enumerate() {
        let mut key = row_key(&row);
        if !seen.insert(key.clone()) {
            key = format!("{key}#{index}");
        }
        let fold =
            |label: String, expanded: bool, toggle: String, turn: bool, failure: bool| Item::Fold {
                key: key.clone(),
                label,
                expanded,
                toggle,
                turn,
                failure,
            };
        out.push(match row {
            Row::User(message) => user_item(key, &message),
            Row::Assistant {
                message, changes, ..
            } => Item::Answer {
                hash: remember_answer(&key, &message.text),
                changes: changes.map(|(run, files)| Changes {
                    run,
                    additions: files.iter().map(|f| f.additions).sum(),
                    deletions: files.iter().map(|f| f.deletions).sum(),
                    files: files
                        .into_iter()
                        .map(|f| FileStat {
                            path: f.path,
                            additions: f.additions,
                            deletions: f.deletions,
                        })
                        .collect(),
                }),
                key,
            },
            Row::Plan(plan) => Item::Answer {
                hash: remember_answer(&key, &plan.plan_markdown),
                key,
                changes: None,
            },
            Row::TurnFold {
                run_id,
                label,
                expanded,
                ..
            } => fold(label, expanded, run_id, true, false),
            Row::WorkToggle {
                group_id,
                summary,
                expanded,
                has_failure,
                ..
            } => fold(summary, expanded, group_id, false, has_failure),
            Row::AttemptFold {
                attempt_id,
                expanded,
            } => fold(
                "Superseded attempt · Cut off by a steer".into(),
                expanded,
                format!("attempt-fold:{attempt_id}"),
                false,
                false,
            ),
            Row::WorkDetails { entries, .. } | Row::Event { entries, .. } => Item::Works {
                key,
                thought: None,
                entries: entries.iter().map(|e| work(e, false)).collect(),
                live: false,
            },
            Row::Work { entry, .. } => Item::Works {
                key,
                thought: None,
                entries: vec![work(&entry, false)],
                live: false,
            },
            Row::LiveWork {
                entry,
                thought,
                active,
                ..
            } => Item::Works {
                key,
                thought,
                entries: vec![work(&entry, active)],
                live: true,
            },
            Row::Working { started_at } => Item::Working {
                key,
                started_at: started_at
                    .as_deref()
                    .and_then(timeline::parse_time)
                    .map(|at| at.timestamp()),
            },
            Row::Thinking { .. } => Item::Thinking { key },
            Row::Compaction { label, .. } => Item::Note { key, text: label },
            Row::GeneratedImage { image } => Item::Image {
                key,
                path: image.path.display().to_string(),
            },
            Row::HtmlRender(_) | Row::McpApp(_) => Item::Note {
                key,
                text: "Open this thread in Pluto on the computer to see this page".into(),
            },
        });
    }
    out
}

fn approval_card(approval: &PendingApproval) -> ApprovalCard {
    let options = approval.options();
    let choice = |o: &requests::ApprovalOption| ApprovalChoice {
        decision: o.decision.clone(),
        label: o.label.clone(),
    };
    ApprovalCard {
        request_id: approval.request_id.clone(),
        label: format!("{} requested", approval.label()),
        app: approval.app_name.clone(),
        detail: approval.detail.clone(),
        warnings: options.iter().filter_map(|o| o.warning.clone()).collect(),
        pair: options
            .iter()
            .filter(|o| o.decision == "decline" || o.decision == "accept")
            .map(choice)
            .collect(),
        rest: options
            .iter()
            .filter(|o| o.decision != "decline" && o.decision != "accept")
            .map(choice)
            .collect(),
    }
}

fn question_card(app: &App, input: &PendingUserInput) -> QuestionCard {
    let drafts = app.answers.get(&input.request_id);
    let empty = Default::default();
    let drafts = drafts.unwrap_or(&empty);
    QuestionCard {
        request_id: input.request_id.clone(),
        dismissible: input.dismissible,
        ready: requests::build_answers(&input.questions, drafts).is_some(),
        answered: input
            .questions
            .iter()
            .filter(|q| requests::resolve_answer(q, drafts.get(&q.id)).is_some())
            .count() as u32,
        questions: input
            .questions
            .iter()
            .map(|q| {
                let draft = drafts.get(&q.id);
                QuestionView {
                    id: q.id.clone(),
                    title: if q.question.is_empty() {
                        q.header.clone()
                    } else {
                        q.question.clone()
                    },
                    multi: q.multi_select,
                    options: q
                        .options
                        .iter()
                        .map(|o| AnswerOption {
                            value: o.value.clone(),
                            label: o.label.clone(),
                            description: o.description.clone(),
                            checked: draft.is_some_and(|d| d.selected.contains(&o.value)),
                        })
                        .collect(),
                    custom_allowed: q.allow_custom_answer,
                    custom: draft.map(|d| d.custom.clone()).unwrap_or_default(),
                }
            })
            .collect(),
    }
}

/// The open thread's pending user input `request`.
fn pending_input(app: &App, thread: &str, request: &str) -> Option<PendingUserInput> {
    let detail = app
        .session
        .state
        .detail
        .as_ref()
        .filter(|d| d.id() == thread)?;
    requests::derive(detail)
        .user_inputs
        .into_iter()
        .find(|i| i.request_id == request)
}

#[uniffi::export]
impl Core {
    pub fn shell(&self) -> ShellView {
        self.read(|app| {
            let s = &app.session;
            ShellView {
                net: s.net.clone(),
                host: s.host(),
                launch: s.launch,
                chat: s.chat,
                selected: s.selected.clone(),
                toast: s.toast.clone().map(|(text, serial)| Toast { text, serial }),
                attempts: crate::session::ATTEMPTS,
                error: s.last_error.clone(),
            }
        })
    }

    pub fn dismiss_toast(&self, serial: u64) {
        self.update(|app, _| {
            if app
                .session
                .toast
                .as_ref()
                .is_some_and(|(_, s)| *s == serial)
            {
                app.session.toast = None;
            }
        });
    }

    pub fn toast(&self, text: String) {
        self.update(|app, _| app.session.show_toast(text));
    }

    /// Connects to `server` with `token` (`remembered`: a launch from the
    /// saved credentials).
    pub fn connect(&self, server: String, token: String, remembered: bool) {
        self.update(|app, hub| {
            app.session
                .connect(hub, server, Zeroizing::new(token), remembered)
        });
    }

    pub fn retry(&self) {
        self.update(|app, hub| app.session.retry(hub));
    }

    /// Leaves the server: its data and the drafts go.
    pub fn disconnect(&self) {
        self.update(|app, _| {
            app.session.disconnect();
            app.drafts = Default::default();
            app.answers.clear();
            crate::usage::reset();
            crate::memory::reset();
            crate::tasks::reset();
            crate::diff::reset();
            reset_answers();
        });
    }

    pub fn probe(&self) {
        self.update(|app, hub| app.session.probe(hub));
    }

    /// Whether the session is online, and whether the probe was answered.
    pub fn probed(&self) -> bool {
        self.read(|app| app.session.replies.contains_key("probe"))
    }

    pub fn online(&self) -> bool {
        self.read(|app| app.session.net.online())
    }

    pub fn select(&self, id: Option<String>) {
        self.update(|app, _| app.session.select(id));
    }

    pub fn set_chat(&self, chat: bool) {
        self.update(|app, _| {
            app.session.chat = chat;
            if !chat {
                app.drafts.temporary = false;
            }
        });
    }

    pub fn set_temporary(&self, on: bool) {
        self.update(|app, _| app.drafts.temporary = on);
    }

    /// Research and Image: one at a time.
    pub fn set_research(&self, on: bool) {
        self.update(|app, _| {
            app.drafts.research = on;
            if on {
                app.drafts.image = false;
            }
        });
    }

    pub fn set_image(&self, on: bool) {
        self.update(|app, _| {
            app.drafts.image = on;
            if on {
                app.drafts.research = false;
            }
        });
    }

    pub fn home(&self) -> HomeView {
        self.read(|app| {
            let (s, d) = (&app.session, &app.drafts);
            let chat = s.chat;
            let project = compose::code_project(s, d.project.as_deref()).cloned();
            let latest = s
                .listed(chat)
                .into_iter()
                .find(|t| chat || project.as_ref().is_some_and(|p| p.id == t.project_id))
                .map(|t| Latest {
                    id: t.id.clone(),
                    title: t.title.clone(),
                });
            let worktree = d.worktree.unwrap_or_else(|| {
                project
                    .as_ref()
                    .is_some_and(|p| compose::default_worktree(s, p))
            });
            HomeView {
                chat,
                temporary: d.temporary,
                research: d.research,
                image: d.image,
                project: project.as_ref().map(badge),
                latest,
                worktree,
                base: d.base.clone().unwrap_or_else(|| "main".into()),
            }
        })
    }

    pub fn composer(&self, target: Target) -> ComposerView {
        self.read(|app| {
            let (s, d) = (&app.session, &app.drafts);
            let project = d.project.as_deref();
            let selection = compose::model(s, d, &target, project);
            let (model_label, driver) = catalog::label(s.providers(), &selection);
            let access = compose::access(s, d, &target, project);
            let thread = match &target {
                Target::Thread { id } => s.thread(id),
                _ => None,
            };
            ComposerView {
                model_label,
                mark: catalog::mark(&driver).map(str::to_owned),
                access_icon: catalog::access(&access).2.into(),
                plan: compose::plan(s, d, &target),
                running: thread.as_ref().is_some_and(|t| t.is_running()),
                online: s.net.online(),
                code: match &target {
                    Target::Chat => false,
                    Target::Code => true,
                    Target::Thread { .. } => thread.as_ref().is_some_and(|t| !s.space(t).is_chat()),
                },
            }
        })
    }

    /// Sends `text` from `target`.
    pub fn send(&self, target: Target, text: String) -> SendOutcome {
        self.update(|app, _| match compose::send(app, &target, &text) {
            Ok(created) => {
                if created.is_some() {
                    app.drafts.research = false;
                    app.drafts.image = false;
                }
                SendOutcome {
                    error: None,
                    created,
                }
            }
            Err(error) => SendOutcome {
                error: Some(error),
                created: None,
            },
        })
    }

    /// A message the server refused, back for its composer.
    pub fn take_unsent(&self, composer: String) -> Option<String> {
        // Nothing changes for the other screens: no notification.
        // Every message given back, oldest first, a blank line apart.
        self.hub
            .lock()
            .session
            .unsent
            .remove(&composer)
            .map(|texts| texts.join("\n\n"))
    }

    /// Stops the thread's run, holding its queue.
    pub fn stop(&self, thread: String) {
        self.update(|app, _| {
            let Some(run) = app
                .session
                .thread(&thread)
                .and_then(|t| t.active_run_id.clone())
            else {
                return;
            };
            let mut command = pluto_core::model::command("run.interrupt", &thread);
            command["runId"] = json!(run);
            command["holdQueue"] = json!(true);
            app.session.dispatch(command);
        });
    }

    pub fn drawer(&self, query: String) -> DrawerView {
        self.read(|app| {
            let s = &app.session;
            let chat = s.chat;
            let now = chrono::Utc::now();
            let q = query.trim().to_lowercase();
            let threads = s.listed(chat);
            let visible: Vec<&Arc<pluto_core::model::Thread>> = threads
                .iter()
                .filter(|t| {
                    q.is_empty()
                        || t.title.to_lowercase().contains(&q)
                        || s.state
                            .project(&t.project_id)
                            .is_some_and(|p| p.title.to_lowercase().contains(&q))
                })
                .collect();
            let row = |t: &pluto_core::model::Thread| thread_row(s, t, chat, now, app.clock_24h);
            let settled =
                |t: &pluto_core::model::Thread| t.settled_override.as_deref() == Some("settled");
            let waits = |t: &pluto_core::model::Thread| {
                t.has_pending_approvals
                    || t.has_pending_user_input
                    || t.pending_subagent_request.is_some()
            };
            let mut active: Vec<_> = visible
                .iter()
                .filter(|t| !settled(t) && !waits(t))
                .collect();
            active.sort_by_key(|t| t.pinned_at.is_none());
            let mut days: Vec<DayGroup> = Vec::new();
            if chat {
                for t in &visible {
                    let day = day_group(last_activity(t), now);
                    let data = row(t);
                    match days.iter_mut().find(|d| d.label == day) {
                        Some(group) => group.rows.push(data),
                        None => days.push(DayGroup {
                            label: day.into(),
                            rows: vec![data],
                        }),
                    }
                }
            }
            let (dot, state) = match s.net {
                Net::Reconnecting { .. } => (Dot::Warn, "Reconnecting…"),
                Net::Offline => (Dot::Bad, "Offline"),
                _ => (Dot::Ok, "Connected"),
            };
            DrawerView {
                chat,
                host: s.host(),
                state: state.into(),
                dot,
                projects: s.code_projects().len() as u32,
                tasks: s
                    .tasks
                    .as_ref()
                    .and_then(|t| t.as_ref().ok())
                    .map(|t| t.len() as u32),
                listed: s.listed,
                selected: s.selected.clone(),
                attention: visible
                    .iter()
                    .filter(|t| !settled(t) && waits(t))
                    .map(|t| row(t))
                    .collect(),
                active: active.into_iter().map(|t| row(t)).collect(),
                settled: visible
                    .iter()
                    .filter(|t| settled(t))
                    .map(|t| row(t))
                    .collect(),
                days,
                empty: visible.is_empty(),
            }
        })
    }

    pub fn watch_tasks(&self) {
        self.update(|app, _| app.session.watch_tasks());
    }

    pub fn thread(&self, id: String) -> ThreadView {
        self.read(|app| {
            let s = &app.session;
            let now = chrono::Utc::now();
            let thread = s.thread(&id);
            let detail = s.state.detail.clone().filter(|d| d.id() == id);
            let failure = s
                .thread_error
                .as_ref()
                .filter(|(failed, _)| *failed == id)
                .map(|(_, message)| message.clone());
            let launching = s.waiting(&format!("launch:{id}"));
            let pending = detail
                .as_ref()
                .map(|d| requests::derive(d))
                .unwrap_or_default();
            ThreadView {
                listed: thread.is_some(),
                title: thread
                    .as_ref()
                    .map(|t| t.title.clone())
                    .unwrap_or_else(|| "New thread".into()),
                chat: thread.as_ref().is_some_and(|t| s.space(t).is_chat()),
                day: thread
                    .as_ref()
                    .map(|t| day_group(last_activity(t), now))
                    .unwrap_or("Today")
                    .into(),
                model_label: thread
                    .as_ref()
                    .map(|t| catalog::label(s.providers(), &t.model_selection).0)
                    .unwrap_or_default(),
                project: thread
                    .as_ref()
                    .and_then(|t| s.state.project(&t.project_id))
                    .map(badge),
                branch: thread.as_ref().and_then(|t| t.branch.clone()),
                worktree: thread.as_ref().is_some_and(|t| t.worktree_path.is_some()),
                items: match (&thread, &detail) {
                    (Some(thread), Some(detail)) => {
                        let mut rows = timeline::rows(thread, detail, &s.expansion);
                        if let Some(images) = s.generated.get(&id) {
                            timeline::insert_generated_images(&mut rows, images);
                        }
                        items(rows)
                    }
                    _ => Vec::new(),
                },
                approvals: pending.approvals.iter().map(approval_card).collect(),
                questions: pending
                    .user_inputs
                    .iter()
                    .map(|i| question_card(app, i))
                    .collect(),
                subagent: s.pending_subagent(&id).map(|child| SubagentCard {
                    question: child
                        .pending_runtime_request
                        .as_ref()
                        .is_some_and(|r| r.kind == "user_input"),
                    id: child.id.clone(),
                    title: child.title.clone(),
                }),
                loaded: detail.is_some(),
                gone: s.listed && thread.is_none() && !launching && failure.is_none(),
                failure,
                launching,
                partial: detail.as_ref().is_some_and(|d| d.history.partial()),
                loading_history: detail.as_ref().is_some_and(|d| d.history.loading),
                history_error: detail.as_ref().and_then(|d| d.history.error.clone()),
                online: s.net.online(),
                running: thread.as_ref().is_some_and(|t| t.is_running()),
            }
        })
    }

    /// Unfolds or folds a turn (`turn`) or a tool group.
    pub fn toggle(&self, key: String, turn: bool) {
        self.update(|app, _| {
            let expansion = &mut app.session.expansion;
            let set = if turn {
                &mut expansion.turns
            } else {
                &mut expansion.groups
            };
            if !set.remove(&key) {
                set.insert(key);
            }
        });
    }

    /// Reading a thread clears its unread state: the server takes the
    /// thread's time as the visit (`thread.visit`), once per time and not
    /// while a turn streams.
    pub fn visit(&self, thread: String) {
        let visit = self.read(|app| {
            let s = &app.session;
            let t = s.thread(&thread)?;
            let tracked = t.last_visited_at.is_some();
            let seen =
                t.last_visited_at.clone().flatten().as_deref() == Some(t.updated_at.as_str());
            (tracked && !seen && !t.is_running() && s.net.online()).then(|| t.updated_at.clone())
        });
        if let Some(at) = visit {
            self.update(|app, _| {
                if app.session.visited.as_ref() == Some(&(thread.clone(), at.clone())) {
                    return;
                }
                app.session.visited = Some((thread.clone(), at.clone()));
                let mut command = pluto_core::model::command("thread.visit", &thread);
                command["visitedAt"] = json!(at);
                app.session
                    .send(pluto_core::protocol::Action::Dispatch(command));
            });
        }
    }

    pub fn load_earlier(&self, thread: String) {
        self.update(|app, _| app.session.load_earlier(&thread));
    }

    pub fn follow_again(&self, thread: String) {
        self.update(|app, _| app.session.follow_again(&thread));
    }

    /// A project's favicon, asked for on first use (PNG, JPEG, …).
    pub fn favicon(&self, project: String) -> Option<Vec<u8>> {
        self.hub
            .lock()
            .session
            .favicon(&project)
            .map(|bytes| bytes.to_vec())
    }

    /// An image Codex generated for `thread`, asked for on first use.
    pub fn generated_image(&self, thread: String, path: String) -> Option<Vec<u8>> {
        self.hub
            .lock()
            .session
            .generated_image(&thread, &path)
            .map(|bytes| bytes.to_vec())
    }

    /// Asks the server again for the images Codex generated for `thread`.
    pub fn watch_generated(&self, thread: String) {
        self.update(|app, _| app.session.watch_generated(&thread));
    }

    /// An answer's markdown image (`classifyMarkdownImageSource`): a file
    /// of the open thread's workspace through the server, a URL fetched
    /// here, or a data URL; asked for on first use.
    pub fn markdown_image(&self, src: String) -> Option<Vec<u8>> {
        use pluto_core::media::{ImageSource, classify};
        let mut app = self.hub.lock();
        let session = &mut app.session;
        let thread = session.selected.clone()?;
        let root = session.thread(&thread).and_then(|t| {
            t.worktree_path.clone().or_else(|| {
                session
                    .state
                    .project(&t.project_id)
                    .map(|p| p.workspace_root.clone())
            })
        });
        match classify(&src, root.as_deref()) {
            ImageSource::File(path) => session.generated_image(&thread, &path).map(|b| b.to_vec()),
            ImageSource::Direct(url) if url.starts_with("data:") => {
                use base64::Engine;
                let (head, data) = url.split_once(',')?;
                if !head.contains(";base64") || data.len() > 14_000_000 {
                    return None;
                }
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data.trim())
                    .ok()?;
                crate::images::raster(&bytes)
            }
            ImageSource::Direct(url) => {
                let url = if url.starts_with("//") {
                    format!("https:{url}")
                } else {
                    url
                };
                let parsed = url::Url::parse(&url)
                    .ok()
                    .filter(|u| matches!(u.scheme(), "http" | "https"))?;
                let key = format!("direct:{url}");
                if let Some(bytes) = session.ready_asset(&key) {
                    return Some(bytes.to_vec());
                }
                if session.start_asset(&key) {
                    let hub = std::sync::Arc::downgrade(&self.hub);
                    std::thread::spawn(move || {
                        let image =
                            pluto_core::http::get(&parsed).and_then(|b| crate::images::raster(&b));
                        if let Some(hub) = hub.upgrade() {
                            hub.update(|app, _| app.session.finish_asset(key, image));
                        }
                    });
                }
                None
            }
            ImageSource::Blocked => None,
        }
    }

    /// A message's image, asked for on first use.
    pub fn attachment(&self, id: String) -> Option<Vec<u8>> {
        self.hub
            .lock()
            .session
            .attachment(&id)
            .map(|bytes| bytes.to_vec())
    }

    /// Answers an approval; returns the key to follow (`reply_state`).
    pub fn respond(&self, thread: String, request: String, decision: String) -> String {
        self.update(|app, _| {
            let mut command = pluto_core::model::command("runtime-request.respond", &thread);
            command["requestId"] = json!(request);
            command["decision"] = json!(decision);
            app.session.dispatch_tracked(command)
        })
    }

    /// Picks (or unpicks) a question's option; a typed answer clears.
    pub fn pick_answer(&self, thread: String, request: String, question: String, value: String) {
        self.update(|app, _| {
            let Some(input) = pending_input(app, &thread, &request) else {
                return;
            };
            let Some(question) = input.questions.iter().find(|q| q.id == question) else {
                return;
            };
            let draft = app
                .answers
                .entry(request)
                .or_default()
                .entry(question.id.clone())
                .or_default();
            draft.custom.clear();
            requests::toggle_option(question, draft, &value);
        });
    }

    /// A typed answer replaces the picked options.
    pub fn type_answer(&self, request: String, question: String, text: String) {
        // The field keeps its own text: the card is built again only when
        // the answer starts or stops being empty (Submit depends on it).
        let changed = {
            let mut app = self.hub.lock();
            let draft = app
                .answers
                .entry(request)
                .or_default()
                .entry(question)
                .or_default();
            let changed = draft.custom.trim().is_empty() != text.trim().is_empty()
                || !draft.selected.is_empty();
            draft.custom = text;
            draft.selected.clear();
            changed
        };
        if changed {
            self.update(|_, _| {});
        }
    }

    /// An answer's markdown blocks (`Item::Answer`).
    pub fn answer_blocks(&self, key: String) -> Vec<Block> {
        let mut answers = answers();
        let Some((_, text, blocks)) = answers.get_mut(&key) else {
            return Vec::new();
        };
        blocks
            .get_or_insert_with(|| Arc::new(markdown::blocks(text)))
            .as_ref()
            .clone()
    }

    /// Submits a question card's answers; the key to follow, once ready.
    pub fn submit_answers(&self, thread: String, request: String) -> Option<String> {
        self.update(|app, _| {
            let input = pending_input(app, &thread, &request)?;
            let empty = Default::default();
            let drafts = app.answers.get(&request).unwrap_or(&empty);
            let built = requests::build_answers(&input.questions, drafts)?;
            let mut command = pluto_core::model::command("runtime-request.respond", &thread);
            command["requestId"] = json!(request);
            command["answers"] = serde_json::Value::Object(built);
            Some(app.session.dispatch_tracked(command))
        })
    }

    pub fn dismiss_question(&self, thread: String, request: String) -> String {
        self.update(|app, _| {
            let mut command = pluto_core::model::command("thread.user-input.dismiss", &thread);
            command["requestId"] = json!(request);
            app.session.dispatch_tracked(command)
        })
    }

    pub fn reply_state(&self, key: String) -> ReplyState {
        self.read(|app| {
            let reply = app.session.replies.get(&key);
            ReplyState {
                waiting: app.session.waiting(&key),
                done: reply.is_some(),
                error: reply.and_then(|r| r.as_ref().err().cloned()),
            }
        })
    }

    /// Loads a session without a server, for looking at the screens in a
    /// debug build: `{"config", "shell", "thread", "detail"}` as the server
    /// streams them, `thread` opened with `detail`.
    pub fn load_demo(&self, json: String) -> Option<String> {
        let demo: serde_json::Value = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(error) => return Some(error.to_string()),
        };
        self.update(|app, _| {
            let s = &mut app.session;
            // Over a connected session, its images still come from the server.
            if !s.net.in_session() {
                if s.server.is_empty() {
                    s.server = "http://demo.invalid:3773".into();
                }
                s.net = Net::Connected;
                s.state.config_event(&demo["config"]);
            }
            if let Err(error) = s.state.shell(&demo["shell"]) {
                return Some(error);
            }
            s.listed = true;
            if let Some(id) = demo["thread"].as_str() {
                s.selected = Some(id.to_owned());
                s.state.switch_detail(Some(id));
                if let Err(error) = s.state.thread(id, &demo["detail"]) {
                    return Some(error);
                }
            }
            None
        })
    }

    /// Drops the replies under `prefix`.
    pub fn forget(&self, prefix: String) {
        self.update(|app, _| app.session.forget(&prefix));
    }

    /// A one-shot request (`payload` as JSON); its reply arrives under
    /// `key` (`reply_json`).
    pub fn request(&self, key: String, method: String, payload: String) {
        let payload = serde_json::from_str(&payload).unwrap_or_else(|_| json!({}));
        self.update(|app, _| app.session.request(key, intern(&method), payload));
    }

    /// A request whose reply is only checked for failure (a toast).
    pub fn mutate(&self, method: String, payload: String) {
        let payload = serde_json::from_str(&payload).unwrap_or_else(|_| json!({}));
        self.update(|app, _| app.session.mutate(intern(&method), payload));
    }

    /// The reply to `key`: its JSON, or its failure; `None` while none came.
    pub fn reply_json(&self, key: String) -> Option<JsonReply> {
        self.read(|app| {
            app.session.replies.get(&key).map(|reply| match reply {
                Ok(value) => JsonReply {
                    ok: Some(value.to_string()),
                    error: None,
                },
                Err(error) => JsonReply {
                    ok: None,
                    error: Some(error.clone()),
                },
            })
        })
    }

    /// A command for `thread` with its id and time (`pluto_core::model::command`),
    /// as JSON to complete before `dispatch`.
    pub fn command_json(&self, kind: String, thread: String) -> String {
        pluto_core::model::command(&kind, &thread).to_string()
    }

    /// Sends a command (JSON); its failure shows as a toast.
    pub fn dispatch(&self, command: String) -> bool {
        let Ok(command) = serde_json::from_str(&command) else {
            return false;
        };
        self.update(|app, _| app.session.dispatch(command))
    }

    /// Sends a command (JSON) whose reply `reply_state` follows; the key.
    pub fn dispatch_tracked(&self, command: String) -> String {
        let command = serde_json::from_str(&command).unwrap_or_else(|_| json!({}));
        self.update(|app, _| app.session.dispatch_tracked(command))
    }

    /// The server's settings (`config.settings`) as JSON.
    pub fn settings_json(&self) -> String {
        self.read(|app| app.session.settings().to_string())
    }

    /// The server's config (`providers`, `environment`, …) as JSON.
    pub fn config_json(&self) -> String {
        self.read(|app| app.session.state.config.to_string())
    }

    /// A `server.updateSettings` patch (`value` as JSON), applied locally at
    /// once.
    pub fn update_setting(&self, key: String, value: String) {
        let value = serde_json::from_str(&value).unwrap_or(serde_json::Value::Null);
        self.update(|app, _| app.session.update_setting(&key, value));
    }

    /// The phone's own settings (`prefs`) as JSON.
    pub fn pref_json(&self, key: String) -> String {
        self.read(|app| app.session.prefs.get(&key).to_string())
    }

    pub fn set_pref(&self, key: String, value: String) {
        let value = serde_json::from_str(&value).unwrap_or(serde_json::Value::Null);
        self.update(|app, _| app.session.prefs.set(&key, value));
    }

    /// The scheduled tasks once listed, as a JSON array, or their failure.
    pub fn tasks_json(&self) -> Option<JsonReply> {
        self.read(|app| {
            app.session.tasks.as_ref().map(|tasks| match tasks {
                Ok(list) => JsonReply {
                    ok: Some(serde_json::Value::Array(list.clone()).to_string()),
                    error: None,
                },
                Err(error) => JsonReply {
                    ok: None,
                    error: Some(error.clone()),
                },
            })
        })
    }

    /// The code projects, by title, as badges.
    pub fn code_projects(&self) -> Vec<Badge> {
        self.read(|app| app.session.code_projects().into_iter().map(badge).collect())
    }
}

/// A reply as JSON, or its failure.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct JsonReply {
    pub ok: Option<String>,
    pub error: Option<String>,
}

/// A method name the protocol keeps for the connection's life (it wants a
/// `&'static str`): leaked once per distinct name Kotlin asks for, a fixed
/// handful in practice.
pub fn intern(name: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(found) = names.iter().find(|n| **n == name) {
        return found;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.push(leaked);
    leaked
}
