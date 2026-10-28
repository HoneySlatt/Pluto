//! Pluto on Android: the desktop client's Code | Chat split on a phone. It
//! connects to a Pluto server (its address and a bearer token from
//! `pluto-server auth session issue`), lists the threads of the chosen
//! space, opens one to read its timeline (built by `pluto_core::timeline`,
//! like the desktop's), and sends messages to it.

mod markdown;
mod secret;
mod session;

use dioxus::prelude::*;
use pluto_core::timeline::{self, Row};
use session::{Link, Session, Status};
use zeroize::Zeroizing;

static STYLE: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let session = use_context_provider(|| Signal::new(Session::default()));
    let link = use_context_provider(|| Link(Signal::new(None)));
    // A remembered server connects at once.
    use_hook(move || {
        if let Some((server, token)) = secret::load() {
            session::connect(session, link, server, token);
        }
    });
    // Android's back gesture walks the WebView's history: opening a thread
    // pushes an entry, and going back to it closes the thread.
    use_hook(move || {
        spawn(async move {
            let mut back =
                document::eval("window.addEventListener('popstate', () => dioxus.send(true));");
            while back.recv::<bool>().await.is_ok() {
                session::select(session, &link, None);
            }
        });
    });
    let connected = session.read().status == Status::Connected;
    let open = session.read().selected.clone();
    rsx! {
        document::Stylesheet { href: STYLE }
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content",
        }
        if !connected {
            Connect {}
        } else if let Some(id) = open {
            ThreadView { id }
        } else {
            ThreadList {}
        }
    }
}

/// The connection card (`pair.tsx`'s, with the address and token fields of
/// the desktop's connection screen).
#[component]
fn Connect() -> Element {
    let session = use_context::<Signal<Session>>();
    let link = use_context::<Link>();
    let mut server = use_signal(|| secret::load_server().unwrap_or_else(|| "http://".into()));
    let mut token = use_signal(String::new);
    let mut remember = use_signal(|| true);
    let status = session.read().status.clone();
    let busy = status == Status::Connecting;
    let error = match &status {
        Status::Disconnected(Some(error)) => Some(error.clone()),
        _ => None,
    };
    let submit = move |_| {
        let address = server.read().trim().to_owned();
        let secret = Zeroizing::new(token.read().trim().to_owned());
        if address.is_empty() || secret.is_empty() {
            return;
        }
        if *remember.read() {
            if let Err(error) = secret::save(&address, &secret) {
                let mut session = session;
                session.write().notice = Some(error);
            }
        } else {
            secret::forget();
        }
        token.set(String::new());
        session::connect(session, link, address, secret);
    };
    rsx! {
        div { class: "screen",
            div { class: "connect",
                div { class: "eyebrow", "PLUTO" }
                h1 { "Connect to your server" }
                p { "The server's address and a session token." }
                label { "Server" }
                input {
                    r#type: "url",
                    autocapitalize: "off",
                    autocorrect: "off",
                    value: "{server}",
                    oninput: move |e| server.set(e.value()),
                }
                label { "Token" }
                input {
                    r#type: "password",
                    autocomplete: "off",
                    value: "{token}",
                    oninput: move |e| token.set(e.value()),
                }
                label { class: "remember",
                    input {
                        r#type: "checkbox",
                        checked: *remember.read(),
                        onchange: move |e| remember.set(e.checked()),
                    }
                    "Remember on this phone (encrypted)"
                }
                button {
                    class: "primary",
                    disabled: busy,
                    onclick: submit,
                    if busy { "Connecting…" } else { "Connect" }
                }
                if let Some(error) = error {
                    div { class: "error", "{error}" }
                }
            }
        }
    }
}

/// A listed thread: its id, title, whether it works, and when it was used.
type ListRow = (String, String, bool, String);

/// The threads of the chosen space: Code by project, Chat by day.
#[component]
fn ThreadList() -> Element {
    let mut session = use_context::<Signal<Session>>();
    let link = use_context::<Link>();
    let (chat, groups, notice, listed) = {
        let session = session.read();
        let now = chrono_now();
        let mut groups: Vec<(String, Vec<ListRow>)> = Vec::new();
        for thread in session.listed() {
            let group = if session.chat {
                day_group(session::last_activity(&thread), now).to_owned()
            } else {
                session
                    .state
                    .project(&thread.project_id)
                    .map(|p| p.title.clone())
                    .unwrap_or_default()
            };
            let row = (
                thread.id.clone(),
                thread.title.clone(),
                thread.is_running(),
                relative_time(session::last_activity(&thread), now),
            );
            match groups.iter_mut().find(|(name, _)| *name == group) {
                Some((_, rows)) => rows.push(row),
                None => groups.push((group, vec![row])),
            }
        }
        (session.chat, groups, session.notice.clone(), session.listed)
    };
    rsx! {
        div { class: "screen",
            div { class: "header",
                div { class: "switch",
                    button {
                        class: if !chat { "on" },
                        onclick: move |_| session.write().chat = false,
                        "Code"
                    }
                    button {
                        class: if chat { "on" },
                        onclick: move |_| session.write().chat = true,
                        "Chat"
                    }
                }
            }
            if let Some(notice) = notice {
                div { class: "notice", "{notice}" }
            }
            div { class: "list",
                if !listed {
                    div { class: "empty", "Loading…" }
                } else if groups.is_empty() {
                    div { class: "empty",
                        if chat { "No chats yet" } else { "No threads yet" }
                    }
                }
                for (name, rows) in groups {
                    div { class: "group", key: "{name}", "{name}" }
                    for (id, title, running, when) in rows {
                        button {
                            class: "row",
                            key: "{id}",
                            onclick: move |_| {
                                // The back gesture returns here.
                                document::eval("history.pushState({ thread: true }, '');");
                                session::select(session, &link, Some(id.clone()));
                            },
                            if running {
                                span { class: "dot" }
                            }
                            span { class: "name", "{title}" }
                            span { class: "when", "{when}" }
                        }
                    }
                }
            }
        }
    }
}

/// One thread: its timeline and the composer.
#[component]
fn ThreadView(id: String) -> Element {
    let mut session = use_context::<Signal<Session>>();
    let link = use_context::<Link>();
    let mut draft = use_signal(String::new);
    let (title, rows, loaded, notice) = {
        let session = session.read();
        let thread = session.thread(&id);
        let title = thread.as_ref().map(|t| t.title.clone()).unwrap_or_default();
        let detail = session.state.detail.clone().filter(|d| d.id() == id);
        let rows = match (&thread, &detail) {
            (Some(thread), Some(detail)) => timeline::rows(thread, detail, &session.expansion),
            _ => Vec::new(),
        };
        (title, rows, detail.is_some(), session.notice.clone())
    };
    // Back through the history, like the gesture.
    let back = move |_| {
        document::eval("history.back();");
    };
    let send = move |_| {
        let text = draft.read().clone();
        if !text.trim().is_empty() {
            session::send_message(session, &link, &text);
            draft.set(String::new());
        }
    };
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
    rsx! {
        div { class: "screen",
            div { class: "header",
                button { class: "back", onclick: back, "‹" }
                div { class: "title", "{title}" }
            }
            if let Some(notice) = notice {
                div { class: "notice", "{notice}" }
            }
            div { class: "timeline",
                if !loaded {
                    div { class: "empty", "Loading…" }
                }
                for (index, row) in rows.into_iter().enumerate() {
                    match row {
                        Row::User(message) => rsx! {
                            div { class: "user", key: "{index}", "{message.text}" }
                        },
                        Row::Assistant { message, .. } => rsx! {
                            div {
                                class: "assistant",
                                key: "{index}",
                                dangerous_inner_html: markdown::to_html(&message.text),
                            }
                        },
                        Row::Plan(plan) => rsx! {
                            div {
                                class: "assistant",
                                key: "{index}",
                                dangerous_inner_html: markdown::to_html(&plan.plan_markdown),
                            }
                        },
                        Row::TurnFold { run_id, label, expanded, .. } => rsx! {
                            button {
                                class: "work",
                                key: "{index}",
                                onclick: move |_| toggle(run_id.clone(), true),
                                if expanded { "▾ {label}" } else { "▸ {label}" }
                            }
                        },
                        Row::WorkToggle { group_id, summary, expanded, has_failure } => rsx! {
                            button {
                                class: if has_failure { "work failed" } else { "work" },
                                key: "{index}",
                                onclick: move |_| toggle(group_id.clone(), false),
                                if expanded { "▾ {summary}" } else { "▸ {summary}" }
                            }
                        },
                        Row::AttemptFold { attempt_id, expanded } => rsx! {
                            button {
                                class: "work",
                                key: "{index}",
                                onclick: move |_| toggle(format!("attempt-fold:{attempt_id}"), false),
                                if expanded {
                                    "▾ Superseded attempt · Cut off by a steer"
                                } else {
                                    "▸ Superseded attempt · Cut off by a steer"
                                }
                            }
                        },
                        Row::Work { label, entry, .. } => rsx! {
                            div { class: work_class(&entry), key: "{index}", "{label}" }
                        },
                        Row::WorkDetails { entries, .. } | Row::Event { entries, .. } => rsx! {
                            for entry in entries {
                                div { class: work_class(&entry), key: "{entry.id}", "{entry.label}" }
                            }
                        },
                        Row::LiveWork { entry, thought, .. } => rsx! {
                            div { key: "{index}",
                                if let Some(thought) = thought {
                                    div { class: "work", "{thought}" }
                                }
                                div { class: work_class(&entry), "{entry.label}" }
                            }
                        },
                        Row::Working { .. } => rsx! {
                            div { class: "working", key: "{index}", "Working" }
                        },
                        Row::Thinking { .. } => rsx! {
                            div { class: "working", key: "{index}", "Thinking" }
                        },
                        Row::Compaction { label, .. } => rsx! {
                            div { class: "work", key: "{index}", "{label}" }
                        },
                        Row::HtmlRender(_) | Row::McpApp(_) | Row::GeneratedImage { .. } => rsx! {
                            div { class: "work", key: "{index}", "Open on the desktop to see this page" }
                        },
                    }
                }
            }
            div { class: "composer",
                textarea {
                    rows: "1",
                    placeholder: "Ask for changes or send a follow-up",
                    value: "{draft}",
                    oninput: move |e| draft.set(e.value()),
                }
                button {
                    class: "send",
                    disabled: draft.read().trim().is_empty(),
                    onclick: send,
                    "↑"
                }
            }
        }
    }
}

fn work_class(entry: &timeline::WorkEntry) -> &'static str {
    if entry.tone == "error" || entry.status.as_deref() == Some("failed") {
        "work failed"
    } else {
        "work"
    }
}

fn chrono_now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

/// `formatRelativeTime`'s compact form: now, 5m, 3h, 2d.
fn relative_time(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(at) = timeline::parse_time(iso) else {
        return String::new();
    };
    let minutes = (now - at).num_minutes().max(0);
    match minutes {
        0 => "now".into(),
        1..=59 => format!("{minutes}m"),
        60..=1439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}

/// The desktop's chat day groups (`chat_mode`'s `day_group`).
fn day_group(iso: &str, now: chrono::DateTime<chrono::Utc>) -> &'static str {
    let Some(at) = timeline::parse_time(iso) else {
        return "Older";
    };
    let local = |t: chrono::DateTime<chrono::Utc>| t.with_timezone(&chrono::Local).date_naive();
    match (local(now) - local(at)).num_days() {
        ..=0 => "Today",
        1 => "Yesterday",
        2..=7 => "Previous 7 days",
        8..=30 => "Previous 30 days",
        _ => "Older",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_compactly() {
        let now = chrono::Utc::now();
        let ago = |minutes| {
            (now - chrono::Duration::minutes(minutes))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        };
        assert_eq!(relative_time(&ago(0), now), "now");
        assert_eq!(relative_time(&ago(5), now), "5m");
        assert_eq!(relative_time(&ago(180), now), "3h");
        assert_eq!(relative_time(&ago(2 * 1440), now), "2d");
        assert_eq!(day_group(&ago(0), now), "Today");
        assert_eq!(day_group(&ago(60 * 24 * 40), now), "Older");
    }
}
