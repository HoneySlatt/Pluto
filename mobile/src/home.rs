//! The home draft (`#home`): round buttons around the Chat | Code tabs, the
//! hero (planet, glow, stars, the draft's title), the chips row and the
//! composer. Chat's chips are Research and Image (one at a time) and
//! Continue; its right disc turns a temporary chat on. Code's are the
//! workspace, the branch and Continue; its right disc picks the project.

use dioxus::prelude::*;

use crate::badge::Badge;
use crate::compose::{self, Target};
use crate::icons::Icon;
use crate::nav::{Sheet, Ui};
use crate::{Planet, Stars, catalog};

#[component]
pub fn Home() -> Element {
    let ui = use_context::<Ui>();
    let mut session = ui.ctx.session;
    let mut drafts = ui.drafts;
    let chat = session.read().chat;
    let temporary = drafts.read().temporary;
    let research = drafts.read().research;
    let image = drafts.read().image;
    let project = {
        let session = session.read();
        compose::code_project(&session, drafts.read().project.as_deref()).cloned()
    };
    // Continue: the space's latest thread (in the draft's project for Code).
    let latest = {
        let session = session.read();
        session
            .listed(chat)
            .into_iter()
            .find(|t| chat || project.as_ref().is_some_and(|p| p.id == t.project_id))
            .map(|t| (t.id.clone(), t.title.clone()))
    };
    let worktree = {
        let session = session.read();
        drafts.read().worktree.unwrap_or_else(|| {
            project
                .as_ref()
                .is_some_and(|p| compose::default_worktree(&session, p))
        })
    };
    let base = drafts.read().base.clone().unwrap_or_else(|| "main".into());
    let placeholder = if !chat {
        "Ask anything, @ files, / commands"
    } else if research {
        "What should I research?"
    } else if image {
        "Describe an image"
    } else {
        "Ask anything"
    };
    let target = if chat { Target::Chat } else { Target::Code };
    let short = |title: &str| {
        let chars: Vec<char> = title.chars().collect();
        if chars.len() > 26 {
            format!("{}…", chars[..25].iter().collect::<String>())
        } else {
            title.to_owned()
        }
    };
    let workspace_icon = if worktree { "folder-git-2" } else { "folder" };
    rsx! {
        section { class: "view",
            div { class: "glow" }
            header { class: "topbar",
                button { class: "disc", "aria-label": "Open threads", onclick: move |_| ui.open_drawer(),
                    Icon { name: "menu" }
                }
                nav { class: "tabs", role: "tablist",
                    button {
                        class: "tab",
                        role: "tab",
                        "aria-selected": "{chat}",
                        onclick: move |_| {
                            session.write().chat = true;
                        },
                        "Chat"
                    }
                    button {
                        class: "tab",
                        role: "tab",
                        "aria-selected": "{!chat}",
                        onclick: move |_| {
                            session.write().chat = false;
                            drafts.write().temporary = false;
                        },
                        "Code"
                    }
                }
                if chat {
                    button {
                        class: if temporary { "disc on" } else { "disc" },
                        "aria-label": "Temporary chat",
                        "aria-pressed": "{temporary}",
                        onclick: move |_| {
                            let mut drafts = drafts.write();
                            drafts.temporary = !drafts.temporary;
                        },
                        Icon { name: "message-square-dashed" }
                    }
                } else {
                    button {
                        class: "disc",
                        "aria-label": "Project",
                        onclick: move |_| ui.open_sheet(Sheet::Project),
                        if let Some(project) = project.clone() {
                            Badge { project }
                        } else {
                            Icon { name: "folder" }
                        }
                    }
                }
            }
            div { class: "hero",
                Stars {}
                Planet {}
                if chat && temporary {
                    h1 {
                        "Temporary chat"
                        br {}
                        span { style: "font-size:15px;color:var(--muted)", "Not saved, no memory, deleted when you leave." }
                    }
                } else if chat {
                    h1 { "How can I help?" }
                } else if let Some(project) = project.clone() {
                    h1 { "What should we build in " b { "{project.title}" } "?" }
                } else {
                    h1 { "Add a project in Pluto on the computer to start coding here." }
                }
            }
            div { class: "chips",
                if chat {
                    if !temporary {
                        button {
                            class: if research { "chip on" } else { "chip" },
                            "aria-pressed": "{research}",
                            onclick: move |_| {
                                let mut drafts = drafts.write();
                                drafts.research = !drafts.research;
                                if drafts.research {
                                    drafts.image = false;
                                }
                            },
                            Icon { name: "globe" }
                            "Research"
                        }
                        button {
                            class: if image { "chip on" } else { "chip" },
                            "aria-pressed": "{image}",
                            onclick: move |_| {
                                let mut drafts = drafts.write();
                                drafts.image = !drafts.image;
                                if drafts.image {
                                    drafts.research = false;
                                }
                            },
                            Icon { name: "image" }
                            "Image"
                        }
                    }
                } else if project.is_some() {
                    button { class: "chip", onclick: move |_| ui.open_sheet(Sheet::Workspace),
                        Icon { name: workspace_icon }
                        if worktree { "New worktree" } else { "Local checkout" }
                    }
                    if worktree {
                        button { class: "chip", onclick: move |_| ui.open_sheet(Sheet::Branch),
                            Icon { name: "git-branch" }
                            "From {base}"
                        }
                    }
                }
                if let Some((id, title)) = latest {
                    button { class: "chip", onclick: move |_| ui.open_thread(id.clone()),
                        Icon { name: "corner" }
                        span { class: "sub", "Continue" }
                        "{short(&title)}"
                    }
                }
            }
            Composer { target, placeholder }
            div { class: "workspace" }
        }
    }
}

/// The composer card (Grok's pill, Pluto's model picker and gradient send):
/// attach, the model, Code's access and mode, then Send, or Stop while the
/// thread works and the field is empty.
#[component]
pub fn Composer(target: Target, placeholder: &'static str) -> Element {
    let ui = use_context::<Ui>();
    let mut drafts = ui.drafts;
    let key = target.key();
    let text = drafts.read().text.get(&key).cloned().unwrap_or_default();
    // The field is not driven by the draft (fast typing outran the round
    // trip); sending remounts it empty.
    let cleared = drafts.read().cleared;
    let (model_label, driver, access, plan, running, online, code, run) = {
        let session = ui.ctx.session.read();
        let project = drafts.read().project.clone();
        let selection = compose::model(&session, &ui, &target, project.as_deref());
        let (label, driver) = catalog::label(session.providers(), &selection);
        let access = compose::access(&session, &ui, &target, project.as_deref());
        let plan = compose::plan(&session, &ui, &target);
        let thread = match &target {
            Target::Thread(id) => session.thread(id),
            _ => None,
        };
        let code = match &target {
            Target::Chat => false,
            Target::Code => true,
            Target::Thread(_) => thread.as_ref().is_some_and(|t| !session.space(t).is_chat()),
        };
        let running = thread.as_ref().is_some_and(|t| t.is_running());
        let run = thread.and_then(|t| t.active_run_id.clone());
        (
            label,
            driver,
            access,
            plan,
            running,
            session.net.online(),
            code,
            run,
        )
    };
    let empty = text.trim().is_empty();
    let access_icon = catalog::access(&access).2;
    let mark = catalog::mark(&driver);
    let send_target = target.clone();
    let send = move |_| {
        let text = drafts
            .peek()
            .text
            .get(&send_target.key())
            .cloned()
            .unwrap_or_default();
        match compose::send(ui, &send_target, &text) {
            Ok(created) => {
                {
                    let mut drafts = drafts.write();
                    drafts.text.remove(&send_target.key());
                    drafts.cleared += 1;
                    if created.is_some() {
                        drafts.research = false;
                        drafts.image = false;
                    }
                }
                if let Some(id) = created {
                    ui.open_thread(id);
                }
            }
            Err(error) => ui.ctx.toast(error),
        }
    };
    let stop_target = target.clone();
    let stop = move |_| {
        if let (Target::Thread(id), Some(run)) = (&stop_target, run.clone()) {
            let mut command = pluto_core::model::command("run.interrupt", id);
            command["runId"] = serde_json::json!(run);
            command["holdQueue"] = serde_json::json!(true);
            ui.ctx.dispatch(command);
        }
    };
    let input_key = key.clone();
    rsx! {
        div { class: "composer",
            // A key only remounts an element in a list: a list of one.
            for generation in [cleared] {
                textarea {
                    key: "{input_key}-{generation}",
                    rows: "1",
                    "aria-label": "Message",
                    placeholder,
                    initial_value: "{text}",
                    oninput: {
                        let input_key = input_key.clone();
                        move |e: FormEvent| {
                            drafts.write().text.insert(input_key.clone(), e.value());
                        }
                    },
                }
            }
            div { class: "tools",
                button { class: "tool round", "aria-label": "Attach", onclick: move |_| ui.open_sheet(Sheet::Attach),
                    Icon { name: "plus" }
                }
                button { class: "tool", "aria-label": "Model: {model_label}", onclick: move |_| ui.open_sheet(Sheet::Model),
                    if let Some(mark) = mark {
                        img { src: mark, alt: "" }
                    }
                    "{model_label}"
                    Icon { name: "chevron-down", class: "chev" }
                }
                if code {
                    button { class: "tool quiet", "aria-label": "Access and mode", onclick: move |_| ui.open_sheet(Sheet::Model),
                        Icon { name: access_icon }
                        if plan {
                            span { "Plan" }
                        }
                    }
                }
                span { class: "spacer" }
                if running && empty {
                    button { class: "send stop", "aria-label": "Stop", disabled: !online, onclick: stop,
                        Icon { name: "stop" }
                    }
                } else {
                    button { class: "send", "aria-label": "Send", disabled: empty || !online, onclick: send,
                        Icon { name: "arrow-up" }
                    }
                }
            }
        }
    }
}
