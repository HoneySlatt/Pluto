//! A thread (`#thread`): the header with the title and project · branch (or
//! the chat's day · model), the timeline `pluto_core::timeline` builds (user
//! bubbles with their context chips, work folds, answers in markdown, the
//! changed-files card, the live work and "Working for …"), the approval and
//! question cards (`requests::derive`, answered with
//! `runtime-request.respond`), a subagent's request below this thread, then
//! the composer and the workspace line. A user message shows its images and
//! files (`attachments.rs`) and its context chips, the referenced ones and
//! those the message carries without a link.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use pluto_core::requests::{self, DraftAnswer, PendingApproval, PendingUserInput};
use pluto_core::timeline::{self, Row, WorkEntry};
use serde_json::json;

use crate::badge::Badge;
use crate::compose::Target;
use crate::home::Composer;
use crate::icons::Icon;
use crate::markdown;
use crate::nav::{Page, Sheet, Ui};

/// `work_icon` (chat.rs).
fn work_icon(entry: &WorkEntry) -> &'static str {
    match entry.item_type.as_deref() {
        Some("command_execution") => return "terminal",
        Some("file_change") => return "square-pen",
        Some("web_search") => return "globe",
        Some("mcp_tool_call") => return "wrench",
        Some("dynamic_tool_call") => return "hammer",
        _ => {}
    }
    if entry.label.eq_ignore_ascii_case("read file") {
        return "eye";
    }
    if entry.command.is_some() {
        return "terminal";
    }
    if !entry.changed_files.is_empty() {
        return "square-pen";
    }
    match entry.tone.as_str() {
        "error" => "circle-alert",
        "thinking" => "brain",
        "info" => "check",
        "tool" => "wrench",
        _ => "zap",
    }
}

/// A work entry's code part: its command, else its first changed file or
/// detail line.
fn work_code(entry: &WorkEntry) -> Option<String> {
    entry
        .command
        .clone()
        .or_else(|| entry.changed_files.first().cloned())
        .or_else(|| {
            entry
                .detail
                .as_ref()
                .and_then(|d| d.lines().next().map(str::to_owned))
        })
        .filter(|s| !s.trim().is_empty())
}

/// Keeps the top-most timeline at its end unless the user scrolled away
/// (then the jump button shows), through every change of its content's
/// height (new rows, images, fonts, the keyboard); while earlier turns load
/// above, the rows in view stay where they are.
const STICK: &str = "requestAnimationFrame(() => {
  const all = document.querySelectorAll('.timeline'), t = all[all.length - 1];
  if (!t) return;
  t.dataset.earlier = window.plutoEarlier ? '1' : '0';
  if (!t.dataset.watch) {
    t.dataset.watch = '1';
    t.dataset.stick = '1';
    const wrap = t.parentElement;
    let fromEnd = 0;
    let set = -1;
    const sync = () => {
      // The script's own scrolling (then content still settling) leaves it
      // following.
      if (Math.abs(t.scrollTop - set) < 2) return;
      set = -1;
      const gap = t.scrollHeight - t.scrollTop - t.clientHeight;
      t.dataset.stick = gap < 80 ? '1' : '0';
      fromEnd = t.scrollHeight - t.scrollTop;
      wrap.classList.toggle('away', gap > 320);
    };
    t.addEventListener('scroll', sync, { passive: true });
    const keep = () => {
      if (t.dataset.stick !== '0') t.scrollTop = t.scrollHeight;
      else if (t.dataset.earlier === '1') t.scrollTop = t.scrollHeight - fromEnd;
      else return;
      set = t.scrollTop;
      fromEnd = t.scrollHeight - t.scrollTop;
    };
    t.plutoKeep = keep;
    const sizes = new ResizeObserver(keep);
    sizes.observe(t);
    if (t.firstElementChild) sizes.observe(t.firstElementChild);
    new MutationObserver(keep).observe(t, { childList: true, subtree: true, characterData: true });
    t.addEventListener('load', keep, true);
  }
  t.plutoKeep();
});";

/// The jump button: back to the end, following it again.
const TO_LATEST: &str = "(() => {
  const all = document.querySelectorAll('.timeline'), t = all[all.length - 1];
  if (!t) return;
  t.dataset.stick = '1';
  t.parentElement.classList.remove('away');
  t.scrollTo({ top: t.scrollHeight, behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' });
})();";

/// A row's key: its own id, so loading earlier turns keeps the rows below.
fn row_key(row: &Row, index: usize) -> String {
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
        Row::GeneratedImage { .. } => format!("image:{index}"),
    }
}

/// The rows with their unique keys.
fn keyed(rows: Vec<Row>) -> Vec<(String, Row)> {
    let mut seen = std::collections::HashSet::new();
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            let mut key = row_key(&row, index);
            if !seen.insert(key.clone()) {
                key = format!("{key}#{index}");
            }
            (key, row)
        })
        .collect()
}

/// "6m 40s" from seconds.
fn elapsed(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds >= 3600 {
        format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

#[component]
fn Work(entry: WorkEntry, #[props(default)] live: bool) -> Element {
    let failed = entry.tone == "error" || entry.status.as_deref() == Some("failed");
    let code = work_code(&entry);
    let label = entry.label.clone();
    rsx! {
        div { class: if live { "w live" } else { "w" },
            style: if failed { "color:var(--error)" },
            if live {
                Icon { name: "loader-circle", class: "spin" }
            } else {
                Icon { name: work_icon(&entry) }
            }
            span { "{label}" }
            if let Some(code) = code {
                code { "{code}" }
            }
        }
    }
}

#[component]
pub fn ThreadView(id: String, page: bool) -> Element {
    let ui = use_context::<Ui>();
    let session = ui.ctx.session;
    // The clock ticks only while the thread works ("Working for …").
    let mut tick = use_signal(|| 0u64);
    let ticking = id.clone();
    use_future(move || {
        let ticking = ticking.clone();
        async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if session
                    .peek()
                    .thread(&ticking)
                    .is_some_and(|t| t.is_running())
                {
                    tick += 1;
                }
            }
        }
    });
    // A thread opens at its end and follows it while it changes, until the
    // user scrolls up; back near the end, it follows again.
    use_effect(move || {
        let session = session.read();
        let _ = session.state.revision;
        let earlier = session
            .state
            .detail
            .as_ref()
            .is_some_and(|d| d.history.loading);
        drop(session);
        document::eval(&format!("window.plutoEarlier = {earlier}; {STICK}"));
    });
    let now = {
        let _ = tick.read();
        chrono::Utc::now()
    };
    let s = session.read();
    let thread = s.thread(&id);
    let detail = s.state.detail.clone().filter(|d| d.id() == id);
    let project = thread
        .as_ref()
        .and_then(|t| s.state.project(&t.project_id))
        .cloned();
    let chat = thread.as_ref().is_some_and(|t| s.space(t).is_chat());
    let title = thread
        .as_ref()
        .map(|t| t.title.clone())
        .unwrap_or_else(|| "New thread".into());
    let branch = thread.as_ref().and_then(|t| t.branch.clone());
    let worktree = thread.as_ref().is_some_and(|t| t.worktree_path.is_some());
    let model_label = thread
        .as_ref()
        .map(|t| crate::catalog::label(s.providers(), &t.model_selection).0)
        .unwrap_or_default();
    let day = thread
        .as_ref()
        .map(|t| crate::day_group(crate::session::last_activity(t), now))
        .unwrap_or("Today");
    let rows = match (&thread, &detail) {
        (Some(thread), Some(detail)) => keyed(timeline::rows(thread, detail, &s.expansion)),
        _ => Vec::new(),
    };
    let pending = detail
        .as_ref()
        .map(|d| requests::derive(d))
        .unwrap_or_default();
    let partial = detail.as_ref().is_some_and(|d| d.history.partial());
    let loading_history = detail.as_ref().is_some_and(|d| d.history.loading);
    let history_error = detail.as_ref().and_then(|d| d.history.error.clone());
    let online = s.net.online();
    let loaded = detail.is_some();
    // What the screen says while there is no timeline: the reason it could
    // not be followed, a thread the server no longer lists, or loading.
    let failure = s
        .thread_error
        .as_ref()
        .filter(|(failed, _)| *failed == id)
        .map(|(_, message)| message.clone());
    let launching = s.waiting(&format!("launch:{id}"));
    let gone = s.listed && thread.is_none() && !launching && failure.is_none();
    let subagent = s.pending_subagent(&id).map(|child| {
        let kind = child
            .pending_runtime_request
            .as_ref()
            .map(|r| r.kind.clone())
            .unwrap_or_default();
        let question = kind == "user_input";
        let icon = if question {
            "message-circle-question-mark"
        } else {
            "shield"
        };
        (child.id.clone(), child.title.clone(), question, icon)
    });
    drop(s);
    let thread_id = id.clone();
    // Earlier turns load above while the rows in view stay put.
    let earlier = move |_| {
        document::eval(
            "window.plutoEarlier = true; (() => { const all = document.querySelectorAll('.timeline'), t = all[all.length - 1]; if (t) t.dataset.earlier = '1'; })();",
        );
        let cursor = ui
            .ctx
            .session
            .clone()
            .write()
            .state
            .begin_history_load(&thread_id);
        if let (Some(cursor), Some(connection)) = (cursor, ui.ctx.link.0.peek().as_ref()) {
            connection.load_history(&thread_id, &cursor, None);
        }
    };
    let retry_id = id.clone();
    let menu_id = id.clone();
    let workspace_icon = if worktree { "folder-git-2" } else { "folder" };
    let now = now.timestamp();
    rsx! {
        section { class: if page { "page" } else { "view" },
            div { class: "glow dim" }
            header { class: "thead",
                if page {
                    button { class: "disc", "aria-label": "Back", onclick: move |_| ui.back(),
                        Icon { name: "chevron-left" }
                    }
                } else {
                    button { class: "disc", "aria-label": "Open threads", onclick: move |_| ui.open_drawer(),
                        Icon { name: "menu" }
                    }
                }
                div { class: "ttl",
                    b { "{title}" }
                    span {
                        if chat {
                            "{day} · {model_label}"
                        } else if let Some(project) = project.clone() {
                            Badge { project: project.clone() }
                            "{project.title}"
                            if let Some(branch) = branch.clone() {
                                span { class: "sep", "·" }
                                Icon { name: "git-branch" }
                                "{branch}"
                            }
                        }
                    }
                }
                button { class: "disc", "aria-label": "Thread actions", disabled: thread.is_none(), onclick: move |_| ui.open_sheet(Sheet::ThreadMenu(menu_id.clone())),
                    Icon { name: "ellipsis-vertical" }
                }
            }
            div { class: "tlwrap",
                div { class: "timeline",
                    // The rows in one box: its size tells the stick script
                    // every change of their height.
                    div { class: "tlin",
                    if gone {
                        div { class: "thread-state",
                            b { "This thread is not available" }
                            "It was deleted, or it belongs to another server."
                            button { class: "btn", onclick: move |_| ui.go_home(), "Back to the draft" }
                        }
                    } else if let Some(message) = failure.clone().filter(|_| !loaded) {
                        div { class: "thread-state",
                            b { "Could not open this thread" }
                            "{message}"
                            button { class: "btn", disabled: !online, onclick: move |_| ui.ctx.follow_again(&retry_id), "Retry" }
                        }
                    } else if !loaded {
                        div { class: "loading-note", if launching { "Starting…" } else { "Loading…" } }
                    }
                    if partial {
                        div { class: "earlier",
                            button { class: "textbtn", disabled: loading_history || !online, onclick: earlier,
                                if loading_history { "Loading earlier turns…" } else if history_error.is_some() { "Try loading earlier turns again" } else { "Load earlier turns" }
                            }
                            if let Some(error) = history_error {
                                span { class: "error", "{error}" }
                            }
                        }
                    }
                    for (key, row) in rows {
                        TimelineRow {
                            key: "{key}",
                            now: if matches!(row, Row::Working { .. }) { now } else { 0 },
                            row,
                            thread: id.clone(),
                        }
                    }
                    for approval in pending.approvals {
                        ApprovalCard { key: "{approval.request_id}", approval, thread: id.clone(), online }
                    }
                    for input in pending.user_inputs {
                        QuestionCard { key: "{input.request_id}", input, thread: id.clone(), online }
                    }
                    if let Some((child, child_title, question, icon)) = subagent {
                        div { class: "card ask-card subagent-card",
                            div { class: "ch",
                                Icon { name: icon }
                                span { if question { "A subagent needs your answer" } else { "A subagent needs your approval" } }
                            }
                            div { class: "why", "{child_title}" }
                            div { class: "actions",
                                button { class: "btn primary", onclick: move |_| ui.open_page(Page::Thread(child.clone())), "Open the subagent" }
                            }
                        }
                    }
                    }
                }
                button { class: "tolatest", "aria-label": "Jump to the latest message", onclick: move |_| { document::eval(TO_LATEST); },
                    Icon { name: "arrow-down" }
                }
            }
            Composer { target: Target::Thread(id.clone()), placeholder: "Send a follow-up" }
            div { class: "workspace",
                if !chat {
                    button { onclick: move |_| ui.ctx.toast("The workspace of a started thread is set on the computer"),
                        Icon { name: workspace_icon }
                        if worktree { "Worktree" } else { "Local checkout" }
                    }
                    span { class: "spacer" }
                    if let Some(branch) = branch {
                        button { onclick: move |_| ui.open_sheet(Sheet::Branch),
                            Icon { name: "git-branch" }
                            "{branch}"
                            Icon { name: "chevron-down" }
                        }
                    }
                }
            }
        }
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

/// A message's image (`attachments.rs`): fetched through the session, a
/// placeholder while it comes or when the server no longer has it.
#[component]
fn AttachmentImage(id: String, name: String) -> Element {
    let ctx = use_context::<crate::session::Ctx>();
    let asked = id.clone();
    use_effect(move || {
        ctx.attachment(&asked);
    });
    let data = ctx
        .session
        .read()
        .assets
        .get(&format!("attachment:{id}"))
        .cloned()
        .flatten();
    rsx! {
        div { class: "img",
            if let Some(data) = data {
                img { src: data, alt: "{name}" }
            } else {
                Icon { name: "image" }
            }
        }
    }
}

#[component]
fn TimelineRow(row: Row, thread: String, now: i64) -> Element {
    let ui = use_context::<Ui>();
    let mut session = ui.ctx.session;
    let mut toggle = move |key: String, turn: bool| {
        let mut session = session.write();
        let set = if turn {
            &mut session.expansion.turns
        } else {
            &mut session.expansion.groups
        };
        if !set.remove(&key) {
            set.insert(key);
        }
    };
    match row {
        Row::User(message) => {
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
            let images: Vec<(String, String)> = message
                .attachments
                .iter()
                .filter(|a| a["type"] == "image")
                .map(|a| {
                    (
                        a["id"].as_str().unwrap_or_default().to_owned(),
                        a["name"].as_str().unwrap_or("Image").to_owned(),
                    )
                })
                .collect();
            for file in message.attachments.iter().filter(|a| a["type"] != "image") {
                chips.push((
                    "file".into(),
                    file["name"].as_str().unwrap_or("File").to_owned(),
                ));
            }
            let nothing = text.is_empty() && chips.is_empty() && images.is_empty();
            let grid = if images.len() == 1 {
                "u-images one"
            } else {
                "u-images"
            };
            rsx! {
                if !images.is_empty() {
                    div { class: grid,
                        for (id, name) in images {
                            AttachmentImage { key: "{id}", id, name }
                        }
                    }
                }
                if nothing {
                    div { class: "u", style: "color:var(--muted)", "Message without text" }
                }
                if !chips.is_empty() {
                    div { class: "chipline",
                        for (kind, label) in chips {
                            span { class: "ctx", Icon { name: chip_icon(&kind) } "{label}" }
                        }
                    }
                }
                if !text.is_empty() {
                    div { class: "u", "{text}" }
                }
            }
        }
        Row::Assistant {
            message, changes, ..
        } => {
            let card = changes.map(|(run, files)| {
                let adds: u64 = files.iter().map(|f| f.additions).sum();
                let dels: u64 = files.iter().map(|f| f.deletions).sum();
                (run, files, adds, dels)
            });
            rsx! {
                div { class: "a", dangerous_inner_html: markdown::to_html(&message.text) }
                if let Some((run, files, adds, dels)) = card {
                    div { class: "card",
                        div { class: "ch",
                            span { "{files.len()} changed files" }
                            span { class: "stat plus", "+{adds}" }
                            span { class: "stat minus", "−{dels}" }
                            span { class: "spacer" }
                            button {
                                class: "textbtn",
                                onclick: {
                                    let (thread, run) = (thread.clone(), run.clone());
                                    move |_| ui.open_page(Page::Diff { thread: thread.clone(), run: Some(run.clone()), focus: None })
                                },
                                Icon { name: "file-diff" }
                                "Open diff"
                            }
                        }
                        for file in files {
                            button {
                                class: "frow",
                                onclick: {
                                    let (thread, run, path) = (thread.clone(), run.clone(), file.path.clone());
                                    move |_| ui.open_page(Page::Diff { thread: thread.clone(), run: Some(run.clone()), focus: Some(path.clone()) })
                                },
                                span { "{file.path}" }
                                span { class: "plus", "+{file.additions}" }
                                span { class: "minus", "−{file.deletions}" }
                            }
                        }
                    }
                }
            }
        }
        Row::Plan(plan) => rsx! {
            div { class: "a", dangerous_inner_html: markdown::to_html(&plan.plan_markdown) }
        },
        Row::TurnFold {
            run_id,
            label,
            expanded,
            ..
        } => rsx! {
            button { class: "fold", "aria-expanded": "{expanded}", onclick: move |_| toggle(run_id.clone(), true),
                "{label}"
                Icon { name: "chevron-right" }
            }
        },
        Row::WorkToggle {
            group_id,
            summary,
            expanded,
            has_failure,
        } => rsx! {
            button {
                class: "fold",
                "aria-expanded": "{expanded}",
                style: if has_failure { "color:var(--error)" },
                onclick: move |_| toggle(group_id.clone(), false),
                "{summary}"
                Icon { name: "chevron-right" }
            }
        },
        Row::AttemptFold {
            attempt_id,
            expanded,
        } => rsx! {
            button { class: "fold", "aria-expanded": "{expanded}", onclick: move |_| toggle(format!("attempt-fold:{attempt_id}"), false),
                "Superseded attempt · Cut off by a steer"
                Icon { name: "chevron-right" }
            }
        },
        Row::WorkDetails { entries, .. } | Row::Event { entries, .. } => rsx! {
            div { class: "works", style: "display:block",
                for entry in entries {
                    Work { key: "{entry.id}", entry }
                }
            }
        },
        Row::Work { entry, .. } => rsx! {
            div { class: "works", style: "display:block",
                Work { entry: *entry }
            }
        },
        Row::LiveWork {
            entry,
            thought,
            active,
            ..
        } => rsx! {
            div { class: "works", style: "display:block;margin-top:8px",
                if let Some(thought) = thought {
                    div { class: "w", Icon { name: "brain" } span { "{thought}" } }
                }
                Work { entry: *entry, live: active }
            }
        },
        Row::Working { started_at } => {
            let seconds = started_at
                .as_deref()
                .and_then(timeline::parse_time)
                .map(|at| now - at.timestamp());
            rsx! {
                div { class: "working-row",
                    span { class: "orb" }
                    if let Some(seconds) = seconds {
                        span { "Working for " span { class: "num", "{elapsed(seconds)}" } }
                    } else {
                        span { class: "dots", "Working" }
                    }
                }
            }
        }
        Row::Thinking { .. } => rsx! {
            div { class: "working-row", span { class: "orb" } span { class: "dots", "Thinking" } }
        },
        Row::Compaction { label, .. } => rsx! {
            div { class: "resolved", Icon { name: "info" } "{label}" }
        },
        Row::HtmlRender(_) | Row::McpApp(_) | Row::GeneratedImage { .. } => rsx! {
            div { class: "resolved", Icon { name: "info" } "Open this thread in Pluto on the computer to see this page" }
        },
    }
}

/// The reply to a card's last answer: waiting, or why it failed.
fn answer_state(ui: &Ui, key: &Option<String>) -> (bool, Option<String>) {
    let Some(key) = key else {
        return (false, None);
    };
    let session = ui.ctx.session.read();
    let error = match session.replies.get(key) {
        Some(Err(error)) => Some(error.clone()),
        _ => None,
    };
    (session.waiting(key), error)
}

#[component]
fn ApprovalCard(approval: PendingApproval, thread: String, online: bool) -> Element {
    let ui = use_context::<Ui>();
    let options = approval.options();
    // Decline and Approve side by side, the other options under them.
    let pair: Vec<_> = options
        .iter()
        .filter(|o| o.decision == "decline" || o.decision == "accept")
        .cloned()
        .collect();
    let rest: Vec<_> = options
        .iter()
        .filter(|o| o.decision != "decline" && o.decision != "accept")
        .cloned()
        .collect();
    let warnings: Vec<String> = options.iter().filter_map(|o| o.warning.clone()).collect();
    // The answer in flight: the card waits for the server, and shows why it
    // failed.
    let mut sent = use_signal(|| None::<String>);
    let (busy, error) = answer_state(&ui, &sent.read());
    let request_id = approval.request_id.clone();
    let respond = move |decision: String| {
        let mut command = pluto_core::model::command("runtime-request.respond", &thread);
        command["requestId"] = json!(request_id);
        command["decision"] = json!(decision);
        sent.set(Some(ui.ctx.dispatch_tracked(command)));
    };
    let label = format!("{} requested", approval.label());
    let enabled = online && !busy;
    let pair_class = if pair.len() == 2 {
        "actions two"
    } else {
        "actions"
    };
    rsx! {
        div { class: "card ask-card",
            div { class: "ch", Icon { name: "shield" } span { "{label}" } }
            if let Some(app) = approval.app_name.clone() {
                div { class: "app", "{app}" }
            }
            if let Some(detail) = approval.detail.clone() {
                div { class: "cmd", "{detail}" }
            }
            for warning in warnings {
                div { class: "warn", Icon { name: "triangle-alert" } span { "{warning}" } }
            }
            if let Some(error) = error {
                div { class: "err", role: "alert", "{error}" }
            }
            if !pair.is_empty() {
                div { class: pair_class,
                    for option in pair {
                        button {
                            class: if option.decision == "accept" { "btn primary" } else { "btn" },
                            disabled: !enabled,
                            onclick: {
                                let mut respond = respond.clone();
                                let decision = option.decision.clone();
                                move |_| respond(decision.clone())
                            },
                            "{option.label}"
                        }
                    }
                }
            }
            if !rest.is_empty() {
                div { class: "actions",
                    for option in rest {
                        button {
                            class: "btn",
                            disabled: !enabled,
                            onclick: {
                                let mut respond = respond.clone();
                                let decision = option.decision.clone();
                                move |_| respond(decision.clone())
                            },
                            "{option.label}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn QuestionCard(input: PendingUserInput, thread: String, online: bool) -> Element {
    let ui = use_context::<Ui>();
    let mut answers = use_signal(BTreeMap::<String, DraftAnswer>::new);
    // Picking an option clears the typed answer: its field remounts empty.
    let mut picks = use_signal(|| 0u64);
    let mut sent = use_signal(|| None::<String>);
    let (busy, error) = answer_state(&ui, &sent.read());
    let ready = requests::build_answers(&input.questions, &answers.read()).is_some();
    let questions = input.questions.clone();
    let submit = {
        let thread = thread.clone();
        let request = input.request_id.clone();
        move |_| {
            let Some(built) = requests::build_answers(&questions, &answers.peek()) else {
                return;
            };
            let mut command = pluto_core::model::command("runtime-request.respond", &thread);
            command["requestId"] = json!(request);
            command["answers"] = serde_json::Value::Object(built);
            sent.set(Some(ui.ctx.dispatch_tracked(command)));
        }
    };
    let dismiss = {
        let request = input.request_id.clone();
        move |_| {
            let mut command = pluto_core::model::command("thread.user-input.dismiss", &thread);
            command["requestId"] = json!(request);
            sent.set(Some(ui.ctx.dispatch_tracked(command)));
        }
    };
    let enabled = online && !busy;
    let generation = picks();
    rsx! {
        for question in input.questions.clone() {
            div { class: "card ask-card q", key: "{question.id}",
                div { class: "ch",
                    Icon { name: "message-circle-question-mark" }
                    span { if question.question.is_empty() { "{question.header}" } else { "{question.question}" } }
                }
                for option in question.options.clone() {
                    button {
                        class: "opt",
                        role: if question.multi_select { "checkbox" } else { "radio" },
                        "aria-checked": "{answers.read().get(&question.id).is_some_and(|d| d.selected.contains(&option.value))}",
                        disabled: !enabled,
                        onclick: {
                            let question = question.clone();
                            let value = option.value.clone();
                            move |_| {
                                let mut answers = answers.write();
                                let draft = answers.entry(question.id.clone()).or_default();
                                let typed = !draft.custom.is_empty();
                                requests::toggle_option(&question, draft, &value);
                                if typed {
                                    picks += 1;
                                }
                            }
                        },
                        span { class: "radio" }
                        span {
                            "{option.label}"
                            if let Some(description) = option.description.clone() {
                                small { "{description}" }
                            }
                        }
                    }
                }
                if question.allow_custom_answer {
                    div { class: "sfield",
                        // A key only remounts an element in a list: a list of one.
                        for generation in [generation] {
                            input {
                                key: "{generation}",
                                placeholder: "Your answer",
                                disabled: !enabled,
                                initial_value: "{answers.peek().get(&question.id).map(|d| d.custom.clone()).unwrap_or_default()}",
                                oninput: {
                                    let id = question.id.clone();
                                    move |e: FormEvent| {
                                        let mut answers = answers.write();
                                        let draft = answers.entry(id.clone()).or_default();
                                        draft.custom = e.value();
                                        draft.selected.clear();
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
        if let Some(error) = error {
            div { class: "ask-card", div { class: "err", role: "alert", "{error}" } }
        }
        div { class: "actions",
            class: if input.dismissible { "two" },
            if input.dismissible {
                button { class: "btn", disabled: !enabled, onclick: dismiss, "Dismiss" }
            }
            button { class: "btn primary", disabled: !enabled || !ready, onclick: submit,
                if busy { "Sending…" } else { "Submit" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_text_keeps_its_chips_apart() {
        let (text, chips) = user_parts("[Research](t3-context://v1/research/research_1) find it");
        assert_eq!(text, "find it");
        assert_eq!(chips, [("research".to_owned(), "Research".to_owned())]);
        assert_eq!(elapsed(400), "6m 40s");
    }

    #[test]
    fn rows_keep_unique_keys() {
        let rows = vec![
            Row::Working { started_at: None },
            Row::Working { started_at: None },
        ];
        let keys: Vec<String> = keyed(rows).into_iter().map(|(key, _)| key).collect();
        assert_eq!(keys, ["working", "working#1"]);
    }
}
