//! The drawer (`#drawer`, Grok's second screen): the connection header, the
//! menu (Projects or Memory, Scheduled tasks, Usage), then Code's "Needs
//! you" (threads waiting on an approval or an answer), Threads and Settled,
//! or Chat's chats by day; Search, Settings and New at the bottom. The list
//! pulls down to refresh (the gesture lives in main.rs).

use dioxus::prelude::*;

use crate::badge::Badge;
use crate::icons::Icon;
use crate::nav::{Page, Ui};
use crate::session::{Net, last_activity};
use crate::{chat_time, day_group, relative_time};

/// The pull to refresh: the shell is live, so a connected list is current
/// once the server answers a probe; an offline one reconnects. The spinner
/// ends with the outcome (`plutoRefreshed`).
pub fn refresh(ui: Ui) {
    let online = ui.ctx.session.peek().net.online();
    if online {
        ui.ctx.probe();
    } else {
        ui.ctx.retry();
    }
    dioxus::core::spawn_forever(async move {
        // The server answers the probe, or misses its six-second deadline;
        // a reconnection tries three times.
        for _ in 0..60 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let net = ui.ctx.session.peek().net.clone();
            if (online && !net.online()) || (!online && matches!(net, Net::Offline)) {
                break;
            }
            if !online && net.online() {
                break;
            }
            if online && ui.ctx.session.peek().replies.contains_key("probe") {
                break;
            }
        }
        document::eval("window.plutoRefreshed && window.plutoRefreshed()");
        let net = ui.ctx.session.peek().net.clone();
        ui.ctx.toast(if net.online() {
            "Threads up to date"
        } else {
            "Can’t reach the server"
        });
    });
}

#[derive(Clone, PartialEq)]
struct RowData {
    id: String,
    title: String,
    project: Option<pluto_core::model::Project>,
    branch: Option<String>,
    status: Option<&'static str>,
    when: String,
}

#[component]
pub fn Drawer() -> Element {
    let ui = use_context::<Ui>();
    // The list lives while the drawer shows (and through its closing).
    let mut shown = use_signal(|| false);
    use_effect(move || {
        let open = ui.nav.read().drawer;
        if open {
            shown.set(true);
            return;
        }
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(320)).await;
            if !ui.nav.peek().drawer {
                shown.set(false);
            }
        });
    });
    // The times move on while it is open.
    let mut minute = use_signal(|| 0u64);
    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            if ui.nav.peek().drawer {
                minute += 1;
            }
        }
    });
    let _ = minute.read();
    if !shown() {
        return rsx! {
            aside { class: "drawer", "aria-label": "Threads" }
        };
    }
    let mut query = use_signal(String::new);
    let mut open_attention = use_signal(|| true);
    let mut open_threads = use_signal(|| true);
    let mut open_settled = use_signal(|| false);
    let session = ui.ctx.session.read();
    let chat = session.chat;
    let now = chrono::Utc::now();
    let time_format = session
        .prefs
        .get("timestampFormat")
        .as_str()
        .unwrap_or("locale")
        .to_owned();
    let q = query.read().trim().to_lowercase();
    let selected = session.selected.clone();
    let threads = session.listed(chat);
    let matches = |t: &pluto_core::model::Thread| {
        q.is_empty()
            || t.title.to_lowercase().contains(&q)
            || session
                .state
                .project(&t.project_id)
                .is_some_and(|p| p.title.to_lowercase().contains(&q))
    };
    let row = |t: &pluto_core::model::Thread| RowData {
        id: t.id.clone(),
        title: t.title.clone(),
        project: session.state.project(&t.project_id).cloned(),
        branch: t.branch.clone(),
        // A subagent below waiting on the user shows on its parent.
        status: if t.has_pending_approvals
            || t.pending_subagent_request.as_deref() == Some("approval")
        {
            Some("approval")
        } else if t.has_pending_user_input
            || t.pending_subagent_request.as_deref() == Some("user_input")
        {
            Some("input")
        } else if t.is_running() {
            Some("working")
        } else {
            None
        },
        when: if chat {
            chat_time(last_activity(t), now, &time_format)
        } else {
            relative_time(last_activity(t), now)
        },
    };
    let visible: Vec<_> = threads.iter().filter(|t| matches(t)).collect();
    let settled = |t: &pluto_core::model::Thread| t.settled_override.as_deref() == Some("settled");
    let waits = |t: &pluto_core::model::Thread| {
        t.has_pending_approvals || t.has_pending_user_input || t.pending_subagent_request.is_some()
    };
    let attention: Vec<RowData> = visible
        .iter()
        .filter(|t| !settled(t) && waits(t))
        .map(|t| row(t))
        .collect();
    let mut active: Vec<_> = visible
        .iter()
        .filter(|t| !settled(t) && !waits(t))
        .collect();
    active.sort_by_key(|t| t.pinned_at.is_none());
    let active: Vec<RowData> = active.into_iter().map(|t| row(t)).collect();
    let settled_rows: Vec<RowData> = visible
        .iter()
        .filter(|t| settled(t))
        .map(|t| row(t))
        .collect();
    let mut days: Vec<(&'static str, Vec<RowData>)> = Vec::new();
    if chat {
        for t in &visible {
            let day = day_group(last_activity(t), now);
            let data = row(t);
            match days.iter_mut().find(|(d, _)| *d == day) {
                Some((_, rows)) => rows.push(data),
                None => days.push((day, vec![data])),
            }
        }
    }
    let projects = session.code_projects().len();
    let tasks = session
        .tasks
        .as_ref()
        .and_then(|t| t.as_ref().ok())
        .map(|t| t.len());
    let net = session.net.clone();
    let host = session.host();
    let listed = session.listed;
    let empty = visible.is_empty();
    drop(session);
    let (dot, state) = match net {
        Net::Reconnecting(_) => ("online warn", "Reconnecting…"),
        Net::Offline => ("online bad", "Offline"),
        _ => ("online", "Connected"),
    };
    let thread_row = move |data: RowData| {
        let current = selected.as_deref() == Some(data.id.as_str());
        let id = data.id.clone();
        rsx! {
            div {
                key: "{data.id}",
                class: if current { "trow current" } else { "trow" },
                role: "button",
                tabindex: "0",
                "aria-current": if current { "true" },
                onclick: {
                    let id = id.clone();
                    move |_| ui.open_thread(id.clone())
                },
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Enter {
                        ui.open_thread(id.clone());
                    }
                },
                div { class: "body",
                    div { class: "title", "{data.title}" }
                    div { class: "meta",
                        if !chat {
                            if let Some(project) = data.project.clone() {
                                Badge { project: project.clone() }
                                span { "{project.title}" }
                            }
                            if let Some(branch) = data.branch.clone() {
                                span { class: "sep", "·" }
                                span { class: "branch", Icon { name: "git-branch" } "{branch}" }
                            }
                            span { class: "sep", "·" }
                        }
                        match data.status {
                            Some("working") => rsx! { span { class: "pill working", Icon { name: "loader-circle", class: "spin" } "Working" } },
                            Some("approval") => rsx! { span { class: "pill approval", Icon { name: "shield" } "Approval" } },
                            Some("input") => rsx! { span { class: "pill input", Icon { name: "message-circle-question-mark" } "Input" } },
                            _ => rsx! { span { class: "when", "{data.when}" } },
                        }
                    }
                }
                button {
                    class: "kebab",
                    "aria-label": "Actions for {data.title}",
                    onclick: {
                        let id = data.id.clone();
                        move |e: MouseEvent| {
                            e.stop_propagation();
                            ui.open_sheet(crate::nav::Sheet::ThreadMenu(id.clone()));
                        }
                    },
                    Icon { name: "ellipsis-vertical" }
                }
            }
        }
    };
    rsx! {
        aside { class: "drawer", "aria-label": "Threads", role: "dialog", "aria-modal": "true", tabindex: "-1",
            header { class: "dhead",
                crate::Avatar {}
                div { class: "who",
                    b { "Pluto" }
                    span { i { class: dot } "{host} · {state}" }
                }
                button { class: "disc", "aria-label": "Close", onclick: move |_| ui.back(),
                    Icon { name: "chevrons-right" }
                }
            }
            div { class: "ptr", "aria-hidden": "true", Icon { name: "refresh-cw" } }
            div { class: "dscroll",
                if q.is_empty() {
                    div { class: "menu",
                        if chat {
                            button { onclick: move |_| ui.open_page(Page::Memory),
                                Icon { name: "brain" }
                                "Memory"
                            }
                        } else {
                            button { onclick: move |_| ui.open_page(Page::Projects),
                                Icon { name: "folder-closed" }
                                "Projects"
                                span { class: "count", "{projects}" }
                            }
                        }
                        button { onclick: move |_| { ui.ctx.watch_tasks(); ui.open_page(Page::Tasks) },
                            Icon { name: "calendar-clock" }
                            "Scheduled tasks"
                            if let Some(tasks) = tasks {
                                span { class: "count", "{tasks}" }
                            }
                        }
                        button { onclick: move |_| ui.open_page(Page::Usage),
                            Icon { name: "chart-no-axes-column" }
                            "Usage"
                        }
                    }
                }
                if !listed {
                    div { class: "empty-note", "Loading…" }
                } else if chat {
                    for (day, rows) in days {
                        div { class: "section static", key: "{day}", "{day}" }
                        for data in rows {
                            {thread_row(data)}
                        }
                    }
                } else {
                    if !attention.is_empty() {
                        button {
                            class: "section",
                            "aria-expanded": "{open_attention}",
                            onclick: move |_| open_attention.toggle(),
                            "Needs you"
                            Icon { name: "chevron-up" }
                        }
                        if open_attention() {
                            for data in attention {
                                AttentionRow { key: "{data.id}", data }
                            }
                        }
                    }
                    button {
                        class: "section",
                        "aria-expanded": "{open_threads}",
                        onclick: move |_| open_threads.toggle(),
                        "Threads"
                        Icon { name: "chevron-up" }
                    }
                    if open_threads() {
                        for data in active {
                            {thread_row(data)}
                        }
                    }
                    if !settled_rows.is_empty() {
                        button {
                            class: "section",
                            "aria-expanded": "{open_settled}",
                            onclick: move |_| open_settled.toggle(),
                            "Settled ({settled_rows.len()})"
                            Icon { name: "chevron-up" }
                        }
                        if open_settled() {
                            for data in settled_rows {
                                {thread_row(data)}
                            }
                        }
                    }
                }
                if listed && empty {
                    if q.is_empty() {
                        div { class: "empty-note", if chat { "No chats yet." } else { "No threads yet." } }
                    } else {
                        div { class: "empty-note", "No threads match “{query}”." }
                    }
                }
            }
            footer { class: "dfoot",
                label { class: "search",
                    Icon { name: "search" }
                    input {
                        r#type: "search",
                        placeholder: "Search",
                        "aria-label": "Search threads",
                        initial_value: "{query}",
                        oninput: move |e| query.set(e.value()),
                    }
                }
                button { class: "disc", "aria-label": "Settings", onclick: move |_| ui.open_page(Page::Settings),
                    Icon { name: "settings" }
                }
                button { class: "disc", "aria-label": if chat { "New chat" } else { "New thread" }, onclick: move |_| ui.go_home(),
                    Icon { name: "square-pen" }
                }
            }
        }
    }
}

/// A "Needs you" row (Grok's "Bots" block): the project disc with its pip,
/// the title, the status and what is asked.
#[component]
fn AttentionRow(data: RowData) -> Element {
    let ui = use_context::<Ui>();
    let approval = data.status == Some("approval");
    let id = data.id.clone();
    rsx! {
        div {
            class: "trow arow",
            role: "button",
            tabindex: "0",
            onclick: {
                let id = id.clone();
                move |_| ui.open_thread(id.clone())
            },
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Enter {
                    ui.open_thread(id.clone());
                }
            },
            span {
                class: "adisc",
                style: if approval { "background:rgb(255 185 0 / .1)" } else { "background:rgb(81 162 255 / .1)" },
                if let Some(project) = data.project.clone() {
                    Badge { project }
                }
                span {
                    class: "pip",
                    style: if approval { "background:var(--warning);color:var(--accent-fg)" } else { "background:var(--info);color:var(--accent-fg)" },
                    if approval { "!" } else { "?" }
                }
            }
            div { class: "body",
                div { class: "line1",
                    span { class: "title", "{data.title}" }
                    span { class: if approval { "pill approval" } else { "pill input" },
                        if approval { "Approval" } else { "Input" }
                    }
                }
                div { class: "ask",
                    if approval { "Waiting for your approval" } else { "Waiting for your answer" }
                }
            }
        }
    }
}
