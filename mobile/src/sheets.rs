//! The bottom sheets: the model (effort, Code's mode and access, the models
//! by provider), a thread's actions, attach, the draft's project, workspace
//! and branch ("Search refs...", `vcs.listRefs`), the Settings and project
//! choices, the confirmations and the task actions.

use dioxus::prelude::*;
use pluto_core::model::ModelSelection;
use serde_json::{Value, json};

use crate::badge::Badge;
use crate::catalog;
use crate::compose::{self, Target};
use crate::icons::Icon;
use crate::nav::{Page, Sheet, Ui};

/// The composer the sheet acts for: the shown thread, else the draft.
fn target(ui: &Ui) -> Target {
    let session = ui.ctx.session.peek();
    match session.selected.clone() {
        Some(id) => Target::Thread(id),
        None if session.chat => Target::Chat,
        None => Target::Code,
    }
}

#[component]
pub fn SheetView() -> Element {
    let ui = use_context::<Ui>();
    let Some(sheet) = ui.nav.read().sheet.clone() else {
        return rsx! {};
    };
    let (title, body) = match sheet {
        Sheet::Model => ("Model".to_owned(), rsx! { ModelSheet {} }),
        Sheet::ThreadMenu(id) => {
            let title = ui
                .ctx
                .session
                .read()
                .thread(&id)
                .map(|t| t.title.clone())
                .unwrap_or_default();
            (title, rsx! { ThreadMenu { id } })
        }
        Sheet::Attach => ("Attach".to_owned(), rsx! { AttachSheet {} }),
        Sheet::Project => ("Project".to_owned(), rsx! { ProjectSheet {} }),
        Sheet::Workspace => ("Workspace".to_owned(), rsx! { WorkspaceSheet {} }),
        Sheet::Branch => (String::new(), rsx! { BranchSheet {} }),
        Sheet::Setting(key) => (
            crate::pages::choice_title(key).to_owned(),
            rsx! { crate::pages::SettingSheet { setting: key } },
        ),
        Sheet::ProjectSetting(project, field) => (
            crate::pages::project_choice_title(field).to_owned(),
            rsx! { crate::pages::ProjectSettingSheet { project, field } },
        ),
        Sheet::RemoveProject(project) => (
            "Remove project".to_owned(),
            rsx! { crate::pages::RemoveProject { project } },
        ),
        Sheet::DeleteThread(id) => ("Delete thread".to_owned(), rsx! { DeleteThread { id } }),
        Sheet::Rename(id) => ("Rename thread".to_owned(), rsx! { Rename { id } }),
        Sheet::TaskMenu(id) => {
            let title = ui
                .ctx
                .session
                .read()
                .tasks
                .as_ref()
                .and_then(|t| t.as_ref().ok())
                .and_then(|t| t.iter().find(|t| t["id"] == id.as_str()))
                .and_then(|t| t["title"].as_str().map(str::to_owned))
                .unwrap_or_default();
            (title, rsx! { crate::tasks::TaskMenu { id } })
        }
        Sheet::TaskField(field) => (
            crate::tasks::field_title(field).to_owned(),
            rsx! { crate::tasks::TaskField { field } },
        ),
    };
    rsx! {
        div { class: "sheet on", role: "dialog", "aria-modal": "true",
            div { class: "grab" }
            if !title.is_empty() {
                h2 { "{title}" }
            }
            div { class: "sbody", {body} }
        }
    }
}

/// A checked row of a choice sheet.
#[component]
pub fn Choice(
    label: String,
    #[props(default)] detail: String,
    checked: bool,
    #[props(default)] mark: Option<Asset>,
    onpick: EventHandler<()>,
) -> Element {
    rsx! {
        button { class: "srow", role: "menuitemradio", "aria-checked": "{checked}", onclick: move |_| onpick.call(()),
            if let Some(mark) = mark {
                img { src: mark, alt: "" }
            }
            span { class: "grow",
                "{label}"
                if !detail.is_empty() {
                    small { "{detail}" }
                }
            }
            Icon { name: "check", class: "check" }
        }
    }
}

/// The models by provider, the current one checked; `onpick` receives the
/// selection.
#[component]
pub fn ModelList(
    chat: bool,
    current: ModelSelection,
    onpick: EventHandler<ModelSelection>,
) -> Element {
    let ui = use_context::<Ui>();
    let providers = catalog::providers(ui.ctx.session.read().providers(), chat);
    if providers.is_empty() {
        return rsx! { p { class: "note", "No provider is ready on the server." } };
    }
    rsx! {
        for provider in providers {
            div { class: "slabel", key: "{provider.instance_id}", "{provider.name}" }
            for (slug, label) in provider.models.clone() {
                Choice {
                    key: "{provider.instance_id}/{slug}",
                    label,
                    checked: current.instance_id == provider.instance_id && current.model == slug,
                    mark: catalog::mark(&provider.driver),
                    onpick: {
                        let instance = provider.instance_id.clone();
                        move |_| onpick.call(ModelSelection { instance_id: instance.clone(), model: slug.clone(), options: None })
                    },
                }
            }
        }
    }
}

#[component]
fn ModelSheet() -> Element {
    let ui = use_context::<Ui>();
    let target = target(&ui);
    let key = target.key();
    let (selection, access, plan, chat) = {
        let session = ui.ctx.session.read();
        let project = ui.drafts.read().project.clone();
        let chat = match &target {
            Target::Chat => true,
            Target::Code => false,
            Target::Thread(id) => session
                .thread(id)
                .is_some_and(|t| session.space(&t).is_chat()),
        };
        (
            compose::model(&session, &ui, &target, project.as_deref()),
            compose::access(&session, &ui, &target, project.as_deref()),
            compose::plan(&session, &ui, &target),
            chat,
        )
    };
    let effort = catalog::effort(ui.ctx.session.read().providers(), &selection);
    let note = catalog::access(&access).3;
    let mut drafts = ui.drafts;
    let (k1, k2, k3, k4) = (key.clone(), key.clone(), key.clone(), key);
    let current = selection.clone();
    rsx! {
        if let Some((id, options, chosen)) = effort {
            div { class: "slabel", "Effort" }
            div { class: "seg", role: "radiogroup",
                for (value, label) in options {
                    button {
                        role: "radio",
                        "aria-checked": "{value == chosen}",
                        onclick: {
                            let (id, key, selection) = (id.clone(), k1.clone(), selection.clone());
                            move |_| {
                                let next = catalog::with_option(&selection, &id, &value);
                                drafts.write().models.insert(key.clone(), next);
                            }
                        },
                        "{label}"
                    }
                }
            }
        }
        if !chat {
            div { class: "slabel", "Mode" }
            div { class: "seg", role: "radiogroup",
                for (label, on) in [("Build", false), ("Plan", true)] {
                    button {
                        role: "radio",
                        "aria-checked": "{plan == on}",
                        onclick: {
                            let key = k2.clone();
                            move |_| { drafts.write().plan.insert(key.clone(), on); }
                        },
                        "{label}"
                    }
                }
            }
            div { class: "slabel", "Access" }
            div { class: "seg", role: "radiogroup",
                for (value, label, _, _) in catalog::ACCESS {
                    button {
                        role: "radio",
                        "aria-checked": "{access == value}",
                        onclick: {
                            let key = k3.clone();
                            move |_| { drafts.write().access.insert(key.clone(), value.to_owned()); }
                        },
                        "{label}"
                    }
                }
            }
            div { class: "seg-note", "{note}" }
        }
        ModelList {
            chat,
            current,
            onpick: move |picked: ModelSelection| {
                drafts.write().models.insert(k4.clone(), picked);
                ui.back();
            },
        }
    }
}

#[component]
fn ThreadMenu(id: String) -> Element {
    let ui = use_context::<Ui>();
    let session = ui.ctx.session.read();
    let Some(thread) = session.thread(&id) else {
        return rsx! { p { class: "note", "This thread is no longer listed." } };
    };
    let chat = session.space(&thread).is_chat();
    let pinned = thread.pinned_at.is_some();
    let settled = thread.settled_override.as_deref() == Some("settled");
    let latest_run = thread.latest_run_id.clone();
    let title = thread.title.clone();
    let confirm = session.prefs.flag("confirmThreadDelete");
    drop(session);
    let run_command = move |kind: &str, id: &str| {
        let mut command = pluto_core::model::command(kind, id);
        if kind == "thread.unsettle" {
            command["reason"] = json!("user");
        }
        ui.ctx.dispatch(command);
        ui.back();
    };
    let (i1, i2, i3, i4, i5, i6, i7) = (
        id.clone(),
        id.clone(),
        id.clone(),
        id.clone(),
        id.clone(),
        id.clone(),
        id.clone(),
    );
    rsx! {
        button { class: "srow", onclick: move |_| ui.open_sheet(Sheet::Rename(i1.clone())),
            Icon { name: "pencil" } span { class: "grow", "Rename" }
        }
        button { class: "srow", onclick: move |_| run_command(if pinned { "thread.unpin" } else { "thread.pin" }, &i2),
            Icon { name: "pin" } span { class: "grow", if pinned { "Unpin" } else { "Pin" } }
        }
        if !chat {
            if let Some(run) = latest_run {
                button { class: "srow", onclick: move |_| {
                        let target = uuid::Uuid::new_v4().to_string();
                        ui.ctx.dispatch(json!({
                            "type": "thread.fork",
                            "commandId": uuid::Uuid::new_v4().to_string(),
                            "createdBy": "user",
                            "creationSource": "web",
                            "sourceThreadId": i3,
                            "targetThreadId": target,
                            "sourcePoint": {"type": "run", "runId": run},
                            "title": format!("{title} fork"),
                        }));
                        ui.back();
                        ui.ctx.toast("Forked");
                    },
                    Icon { name: "git-fork" } span { class: "grow", "Fork thread" }
                }
            }
            button { class: "srow", onclick: move |_| {
                    // The diff reads the thread's checkpoints: it opens first.
                    if ui.ctx.session.peek().selected.as_deref() != Some(i4.as_str()) {
                        ui.ctx.select(Some(i4.clone()));
                    }
                    ui.open_page(Page::Diff { thread: i4.clone(), run: None, focus: None })
                },
                Icon { name: "file-diff" } span { class: "grow", "Changed files" }
            }
        }
        button { class: "srow", onclick: move |_| {
                let id = i5.clone();
                spawn(async move {
                    let _ = document::eval(&format!("navigator.clipboard.writeText({})", json!(id))).await;
                });
                ui.back();
                ui.ctx.toast("Thread ID copied");
            },
            Icon { name: "copy" } span { class: "grow", "Copy thread ID" }
        }
        button { class: "srow", onclick: move |_| run_command(if settled { "thread.unsettle" } else { "thread.settle" }, &i6),
            Icon { name: "archive" } span { class: "grow", if settled { "Unsettle" } else { "Settle" } }
        }
        button { class: "srow danger", onclick: move |_| {
                if confirm {
                    ui.open_sheet(Sheet::DeleteThread(i7.clone()));
                } else {
                    delete_thread(ui, &i7);
                }
            },
            Icon { name: "trash-2" } span { class: "grow", "Delete" }
        }
    }
}

fn delete_thread(ui: Ui, id: &str) {
    if ui.ctx.session.peek().selected.as_deref() == Some(id) {
        ui.ctx.select(None);
    }
    ui.ctx
        .dispatch(pluto_core::model::command("thread.delete", id));
    ui.back();
    ui.ctx.toast("Deleted");
}

#[component]
fn DeleteThread(id: String) -> Element {
    let ui = use_context::<Ui>();
    rsx! {
        p { class: "confirm", "This deletes the thread and its chat history." }
        div { class: "actions two",
            button { class: "btn", onclick: move |_| ui.back(), "Cancel" }
            button { class: "btn danger", onclick: move |_| delete_thread(ui, &id), "Delete" }
        }
    }
}

#[component]
fn Rename(id: String) -> Element {
    let ui = use_context::<Ui>();
    let current = ui
        .ctx
        .session
        .read()
        .thread(&id)
        .map(|t| t.title.clone())
        .unwrap_or_default();
    let mut title = use_signal(|| current);
    rsx! {
        label { class: "sfield",
            input { initial_value: "{title}", "aria-label": "Title", oninput: move |e| title.set(e.value()) }
        }
        div { class: "actions two",
            button { class: "btn", onclick: move |_| ui.back(), "Cancel" }
            button { class: "btn primary", disabled: title.read().trim().is_empty(), onclick: move |_| {
                    let mut command = pluto_core::model::command("thread.metadata.update", &id);
                    command["title"] = json!(title.peek().trim());
                    ui.ctx.dispatch(command);
                    ui.back();
                },
                "Save"
            }
        }
    }
}

#[component]
fn AttachSheet() -> Element {
    let ui = use_context::<Ui>();
    let note = move |_| {
        ui.ctx
            .toast("Attachments from the phone are not available yet")
    };
    rsx! {
        div { class: "tiles",
            button { onclick: note, Icon { name: "image" } "Photos" }
            button { onclick: note, Icon { name: "camera" } "Camera" }
            button { onclick: note, Icon { name: "file" } "Files" }
        }
    }
}

#[component]
fn ProjectSheet() -> Element {
    let ui = use_context::<Ui>();
    let mut drafts = ui.drafts;
    let (projects, current) = {
        let session = ui.ctx.session.read();
        let current =
            compose::code_project(&session, drafts.read().project.as_deref()).map(|p| p.id.clone());
        (
            session
                .code_projects()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
            current,
        )
    };
    rsx! {
        for project in projects {
            button {
                key: "{project.id}",
                class: "srow",
                role: "menuitemradio",
                "aria-checked": "{current.as_deref() == Some(project.id.as_str())}",
                onclick: {
                    let id = project.id.clone();
                    move |_| {
                        let mut drafts = drafts.write();
                        drafts.project = Some(id.clone());
                        drafts.worktree = None;
                        drafts.base = None;
                        drop(drafts);
                        ui.back();
                    }
                },
                Badge { project: project.clone() }
                span { class: "grow", "{project.title}" }
                Icon { name: "check", class: "check" }
            }
        }
    }
}

#[component]
fn WorkspaceSheet() -> Element {
    let ui = use_context::<Ui>();
    let mut drafts = ui.drafts;
    let worktree = {
        let session = ui.ctx.session.read();
        let project = compose::code_project(&session, drafts.read().project.as_deref()).cloned();
        drafts
            .read()
            .worktree
            .unwrap_or_else(|| project.is_some_and(|p| compose::default_worktree(&session, &p)))
    };
    rsx! {
        for (label, icon, detail, value) in [
            ("Local checkout", "folder", "Work in the project’s own folder", false),
            ("New worktree", "folder-git-2", "A fresh branch in its own folder", true),
        ] {
            button {
                class: "srow",
                role: "menuitemradio",
                "aria-checked": "{worktree == value}",
                onclick: move |_| {
                    drafts.write().worktree = Some(value);
                    ui.back();
                },
                Icon { name: icon }
                span { class: "grow", "{label}" small { "{detail}" } }
                Icon { name: "check", class: "check" }
            }
        }
    }
}

/// `BranchPickerRefItem`'s tag.
fn ref_tag(r: &Value, root: &str) -> Option<&'static str> {
    if r["current"] == true {
        Some("current")
    } else if r["worktreePath"].as_str().is_some_and(|p| p != root) {
        Some("worktree")
    } else if r["isRemote"] == true {
        Some("remote")
    } else if r["isDefault"] == true {
        Some("default")
    } else {
        None
    }
}

/// The ref picker: the draft's base for a new worktree, or the thread's
/// branch (`vcs.switchRef`, or `vcs.createRef` for a new one).
#[component]
fn BranchSheet() -> Element {
    let ui = use_context::<Ui>();
    let mut query = use_signal(String::new);
    let (cwd, root, thread) = {
        let session = ui.ctx.session.read();
        match session.selected.clone().and_then(|id| session.thread(&id)) {
            Some(thread) => {
                let root = session
                    .state
                    .project(&thread.project_id)
                    .map(|p| p.workspace_root.clone())
                    .unwrap_or_default();
                (
                    thread.worktree_path.clone().unwrap_or_else(|| root.clone()),
                    root,
                    Some(thread.id.clone()),
                )
            }
            None => {
                let root = compose::code_project(&session, ui.drafts.read().project.as_deref())
                    .map(|p| p.workspace_root.clone())
                    .unwrap_or_default();
                (root.clone(), root, None)
            }
        }
    };
    let q = query.read().trim().to_owned();
    let key = format!("refs:{cwd}\0{q}");
    use_effect({
        let (key, cwd) = (key.clone(), cwd.clone());
        move || {
            let q = query.read().trim().to_owned();
            let mut payload = json!({"cwd": cwd, "limit": 100});
            if !q.is_empty() {
                payload["query"] = json!(q.split_whitespace().collect::<Vec<_>>().join("-"));
            }
            let _ = &key;
            ui.ctx
                .request(format!("refs:{cwd}\0{q}"), "vcs.listRefs", payload);
        }
    });
    let refs: Option<Result<Vec<Value>, String>> =
        ui.ctx.session.read().replies.get(&key).map(|r| {
            r.as_ref()
                .map(|v| v["refs"].as_array().cloned().unwrap_or_default())
                .map_err(Clone::clone)
        });
    let new_name: String = q.split_whitespace().collect::<Vec<_>>().join("-");
    let create = thread.is_some()
        && !new_name.is_empty()
        && refs
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .is_some_and(|refs| !refs.iter().any(|r| r["name"] == new_name.as_str()));
    let pick = {
        let (cwd, thread) = (cwd.clone(), thread.clone());
        move |name: String, created: bool| {
            match &thread {
                None => ui.drafts.clone().write().base = Some(name),
                Some(id) => {
                    let (method, payload) = if created {
                        (
                            "vcs.createRef",
                            json!({"cwd": cwd, "refName": name, "switchRef": true}),
                        )
                    } else {
                        ("vcs.switchRef", json!({"cwd": cwd, "refName": name}))
                    };
                    ui.ctx.mutate(method, payload);
                    let mut command = pluto_core::model::command("thread.metadata.update", id);
                    command["branch"] = json!(name);
                    ui.ctx.dispatch(command);
                }
            }
            ui.back();
        }
    };
    rsx! {
        label { class: "sfield",
            Icon { name: "search" }
            input { placeholder: "Search refs...", "aria-label": "Search refs", initial_value: "{query}", oninput: move |e| query.set(e.value()) }
        }
        match refs {
            None => rsx! { p { class: "note", "Loading refs..." } },
            Some(Err(error)) => rsx! { p { class: "note", "{error}" } },
            Some(Ok(list)) => rsx! {
                if list.is_empty() && !create {
                    p { class: "note", "No refs found." }
                }
                for r in list {
                    button {
                        key: "{r[\"name\"]}",
                        class: "srow",
                        onclick: {
                            let pick = pick.clone();
                            let name = r["name"].as_str().unwrap_or_default().to_owned();
                            move |_| pick(name.clone(), false)
                        },
                        Icon { name: "git-branch" }
                        span { class: "grow", "{r[\"name\"].as_str().unwrap_or_default()}" }
                        if let Some(tag) = ref_tag(&r, &root) {
                            span { class: "tag", "{tag}" }
                        }
                    }
                }
                if create {
                    button { class: "srow", onclick: {
                            let pick = pick.clone();
                            let name = new_name.clone();
                            move |_| pick(name.clone(), true)
                        },
                        Icon { name: "plus" }
                        span { class: "grow", "Create new ref “{new_name}”" }
                    }
                }
            },
        }
    }
}
