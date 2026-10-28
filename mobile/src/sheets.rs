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
    // The open sheet, else the one leaving (its exit, `nav.rs`).
    let (sheet, leaving) = {
        let nav = ui.nav.read();
        match (nav.sheet.clone(), nav.leaving_sheet.clone()) {
            (Some(sheet), _) => (sheet, false),
            (None, Some((sheet, _))) => (sheet, true),
            (None, None) => return rsx! {},
        }
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
        Sheet::MemoryEdit(id) => (
            if id.is_some() {
                "Edit memory"
            } else {
                "Add a memory"
            }
            .to_owned(),
            rsx! { crate::memory::MemoryEditSheet { id } },
        ),
        Sheet::MemoryMenu(id) => (
            "Memory".to_owned(),
            rsx! { crate::memory::MemoryMenu { id } },
        ),
        Sheet::ClearMemories => (
            "Delete all memories".to_owned(),
            rsx! { crate::memory::ClearMemories {} },
        ),
        Sheet::License(key) => (
            crate::licenses::title(&key),
            rsx! { crate::licenses::LicenseSheet { license: key } },
        ),
        Sheet::TaskField(field) => (
            crate::tasks::field_title(field).to_owned(),
            rsx! { crate::tasks::TaskField { field } },
        ),
    };
    rsx! {
        div {
            class: if leaving { "sheet leave" } else { "sheet on" },
            role: "dialog",
            "aria-modal": "true",
            "aria-label": if title.is_empty() { "Choose a ref" } else { "{title}" },
            tabindex: "-1",
            div { class: "grab" }
            if !title.is_empty() {
                h2 { "{title}" }
            }
            div { class: "sbody", {body} }
        }
    }
}

/// A tracked command or request asked from a sheet: the key to set once
/// sent, whether it still waits, and why it failed; `on_ok` runs once the
/// server accepted it.
pub fn use_outcome(
    ui: Ui,
    mut on_ok: impl FnMut() + 'static,
) -> (Signal<Option<String>>, bool, Option<String>) {
    let mut sent = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    use_effect(move || {
        let Some(key) = sent.read().clone() else {
            return;
        };
        let Some(result) = ui.ctx.session.read().replies.get(&key).cloned() else {
            return;
        };
        ui.ctx.forget(&key);
        sent.set(None);
        match result {
            Ok(_) => on_ok(),
            Err(message) => error.set(Some(message)),
        }
    });
    let busy = sent
        .read()
        .as_ref()
        .is_some_and(|key| ui.ctx.session.read().waiting(key));
    (sent, busy, error())
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
            div { class: "choices", role: "radiogroup",
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
            div { class: "choices", role: "radiogroup",
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
    let mut done = use_signal(|| "");
    let deleted = id.clone();
    let (mut sent, busy, error) = use_outcome(ui, move || {
        if *done.peek() == "Deleted" {
            leave_deleted(ui, &deleted);
        }
        ui.back();
        ui.ctx.toast(*done.peek());
    });
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
                button { class: "srow", disabled: busy, onclick: move |_| {
                        let target = uuid::Uuid::new_v4().to_string();
                        done.set("Forked");
                        sent.set(Some(ui.ctx.dispatch_tracked(json!({
                            "type": "thread.fork",
                            "commandId": uuid::Uuid::new_v4().to_string(),
                            "createdBy": "user",
                            "creationSource": "web",
                            "sourceThreadId": i3,
                            "targetThreadId": target,
                            "sourcePoint": {"type": "run", "runId": run},
                            "title": format!("{title} fork"),
                        }))));
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
        button { class: "srow danger", disabled: busy, onclick: move |_| {
                if confirm {
                    ui.open_sheet(Sheet::DeleteThread(i7.clone()));
                } else {
                    done.set("Deleted");
                    sent.set(Some(ui.ctx.dispatch_tracked(pluto_core::model::command("thread.delete", &i7))));
                }
            },
            Icon { name: "trash-2" } span { class: "grow", "Delete" }
        }
        if busy {
            p { class: "note", role: "status", "Waiting for the server…" }
        }
        if let Some(error) = error {
            p { class: "err", role: "alert", "{error}" }
        }
    }
}

/// A deleted thread that was open leaves for the draft.
fn leave_deleted(ui: Ui, id: &str) {
    if ui.ctx.session.peek().selected.as_deref() == Some(id) {
        ui.ctx.select(None);
    }
}

#[component]
fn DeleteThread(id: String) -> Element {
    let ui = use_context::<Ui>();
    let deleted = id.clone();
    let (mut sent, busy, error) = use_outcome(ui, move || {
        leave_deleted(ui, &deleted);
        ui.back();
        ui.ctx.toast("Deleted");
    });
    rsx! {
        p { class: "confirm", "This deletes the thread and its chat history." }
        if let Some(error) = error {
            p { class: "err", role: "alert", "{error}" }
        }
        div { class: "actions two",
            button { class: "btn", onclick: move |_| ui.back(), "Cancel" }
            button { class: "btn danger", disabled: busy, onclick: move |_| {
                    sent.set(Some(ui.ctx.dispatch_tracked(pluto_core::model::command("thread.delete", &id))));
                },
                if busy { "Deleting…" } else { "Delete" }
            }
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
    let (mut sent, busy, error) = use_outcome(ui, move || ui.back());
    rsx! {
        label { class: "sfield",
            input { initial_value: "{title}", "aria-label": "Title", oninput: move |e| title.set(e.value()) }
        }
        if let Some(error) = error {
            p { class: "err", role: "alert", "{error}" }
        }
        div { class: "actions two",
            button { class: "btn", onclick: move |_| ui.back(), "Cancel" }
            button { class: "btn primary", disabled: busy || title.read().trim().is_empty(), onclick: move |_| {
                    let mut command = pluto_core::model::command("thread.metadata.update", &id);
                    command["title"] = json!(title.peek().trim());
                    sent.set(Some(ui.ctx.dispatch_tracked(command)));
                },
                if busy { "Saving…" } else { "Save" }
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

/// `local_name`: a remote ref without its remote.
fn local_name(remote: &str) -> &str {
    match remote.find('/') {
        Some(index) if index > 0 && index + 1 < remote.len() => &remote[index + 1..],
        _ => remote,
    }
}

/// `selection_target` (`branch_picker.rs`): where a picked ref is checked
/// out, the thread's worktree afterwards, and whether a worktree that has it
/// already is reused.
fn selection_target(
    project_cwd: &str,
    worktree: Option<&str>,
    r: &Value,
) -> (String, Option<String>, bool) {
    if let Some(path) = r["worktreePath"].as_str() {
        let next = (path != project_cwd).then(|| path.to_owned());
        return (path.to_owned(), next, true);
    }
    let next = worktree
        .filter(|_| r["isDefault"] != true)
        .map(str::to_owned);
    (
        next.clone().unwrap_or_else(|| project_cwd.to_owned()),
        next,
        false,
    )
}

/// `setThreadBranch`: the thread records its branch and worktree; a new
/// worktree detaches the provider sessions of the old one.
fn set_thread_branch(ui: Ui, id: &str, branch: &str, worktree: Option<String>) {
    let detach: Vec<String> = {
        let session = ui.ctx.session.peek();
        let moved = session
            .thread(id)
            .is_some_and(|t| t.worktree_path != worktree);
        session
            .state
            .detail
            .as_ref()
            .filter(|d| moved && d.id() == id)
            .map(|d| {
                d.provider_sessions
                    .iter()
                    .filter_map(|s| s["id"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let base = uuid::Uuid::new_v4().to_string();
    for session_id in detach {
        ui.ctx.dispatch(json!({
            "type": "provider-session.detach",
            "commandId": format!("{base}:detach:{session_id}"),
            "threadId": id,
            "providerSessionId": session_id,
            "reason": "client-requested",
        }));
    }
    let mut command = pluto_core::model::command("thread.metadata.update", id);
    command["branch"] = json!(branch);
    command["worktreePath"] = json!(worktree);
    ui.ctx.dispatch(command);
}

/// A ref switch on its way: its request key, the branch it shows, and the
/// thread's worktree afterwards.
#[derive(Clone, PartialEq)]
struct Switch {
    key: String,
    shown: String,
    remote: bool,
    create: bool,
    worktree: Option<String>,
}

/// The ref picker: the draft's base for a new worktree, or the thread's
/// branch (`vcs.switchRef`, or `vcs.createRef` for a new one; the thread
/// records it once the checkout succeeded).
#[component]
fn BranchSheet() -> Element {
    let ui = use_context::<Ui>();
    let mut query = use_signal(String::new);
    // The query the list shows, a moment after the typing stops.
    let mut searched = use_signal(String::new);
    let mut switch = use_signal(|| None::<Switch>);
    let mut failure = use_signal(|| None::<String>);
    let (cwd, root, thread, worktree) = {
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
                    thread.worktree_path.clone(),
                )
            }
            None => {
                let root = compose::code_project(&session, ui.drafts.read().project.as_deref())
                    .map(|p| p.workspace_root.clone())
                    .unwrap_or_default();
                (root.clone(), root, None, None)
            }
        }
    };
    // The picker's searches leave with it.
    use_drop(move || ui.ctx.forget("refs:"));
    use_effect(move || {
        let q = query.read().trim().to_owned();
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(220)).await;
            if query.peek().trim() == q && *searched.peek() != q {
                searched.set(q);
            }
        });
    });
    let q = searched.read().clone();
    let key = format!("refs:{cwd}\0{q}");
    use_effect({
        let cwd = cwd.clone();
        move || {
            let q = searched.read().clone();
            let key = format!("refs:{cwd}\0{q}");
            if ui.ctx.session.peek().replies.contains_key(&key) {
                return;
            }
            let mut payload = json!({"cwd": cwd, "limit": 100});
            if !q.is_empty() {
                payload["query"] = json!(q.split_whitespace().collect::<Vec<_>>().join("-"));
            }
            ui.ctx.request(key, "vcs.listRefs", payload);
        }
    });
    // The switch's answer: the thread records the branch, or the sheet says
    // why the checkout failed.
    use_effect({
        let thread = thread.clone();
        move || {
            let Some(pending) = switch.read().clone() else {
                return;
            };
            let Some(result) = ui.ctx.session.read().replies.get(&pending.key).cloned() else {
                return;
            };
            ui.ctx.forget(&pending.key);
            switch.set(None);
            match result {
                Ok(value) => {
                    let returned = value["refName"].as_str().map(str::to_owned);
                    let branch = if pending.create || pending.remote {
                        returned.unwrap_or(pending.shown)
                    } else {
                        pending.shown
                    };
                    if let Some(id) = &thread {
                        set_thread_branch(ui, id, &branch, pending.worktree);
                    }
                    ui.back();
                }
                Err(error) => failure.set(Some(format!(
                    "{} {error}",
                    if pending.create {
                        "Failed to create and switch ref."
                    } else {
                        "Failed to switch ref."
                    }
                ))),
            }
        }
    });
    let refs: Option<Result<Vec<Value>, String>> =
        ui.ctx.session.read().replies.get(&key).map(|r| {
            r.as_ref()
                .map(|v| v["refs"].as_array().cloned().unwrap_or_default())
                .map_err(Clone::clone)
        });
    let typed = query.read().trim().to_owned();
    let new_name: String = typed.split_whitespace().collect::<Vec<_>>().join("-");
    let create = thread.is_some()
        && !new_name.is_empty()
        && refs
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .is_some_and(|refs| !refs.iter().any(|r| r["name"] == new_name.as_str()));
    let busy = switch.read().is_some();
    let pick = {
        let (cwd, root, thread, worktree) =
            (cwd.clone(), root.clone(), thread.clone(), worktree.clone());
        move |r: Value, created: bool| {
            let name = r["name"].as_str().unwrap_or_default().to_owned();
            let Some(id) = &thread else {
                ui.drafts.clone().write().base = Some(name);
                ui.back();
                return;
            };
            failure.set(None);
            let key = format!("branch-action:{}", uuid::Uuid::new_v4());
            if created {
                ui.ctx.request(
                    key.clone(),
                    "vcs.createRef",
                    json!({"cwd": cwd, "refName": name, "switchRef": true}),
                );
                switch.set(Some(Switch {
                    key,
                    shown: name,
                    remote: false,
                    create: true,
                    worktree: worktree.clone(),
                }));
                return;
            }
            let (target, next, reuse) = selection_target(&root, worktree.as_deref(), &r);
            // A worktree that has the ref checked out already is reused.
            if reuse {
                set_thread_branch(ui, id, &name, next);
                ui.back();
                return;
            }
            let remote = r["isRemote"] == true;
            let shown = if remote { local_name(&name) } else { &name }.to_owned();
            ui.ctx.request(
                key.clone(),
                "vcs.switchRef",
                json!({"cwd": target, "refName": name}),
            );
            switch.set(Some(Switch {
                key,
                shown,
                remote,
                create: false,
                worktree: next,
            }));
        }
    };
    rsx! {
        label { class: "sfield",
            Icon { name: "search" }
            input { placeholder: "Search refs...", "aria-label": "Search refs", initial_value: "{query}", oninput: move |e| query.set(e.value()) }
        }
        if busy {
            p { class: "note", role: "status", "Switching…" }
        }
        if let Some(error) = failure() {
            p { class: "err", role: "alert", "{error}" }
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
                        disabled: busy,
                        onclick: {
                            let mut pick = pick.clone();
                            let r = r.clone();
                            move |_| pick(r.clone(), false)
                        },
                        Icon { name: "git-branch" }
                        span { class: "grow", "{r[\"name\"].as_str().unwrap_or_default()}" }
                        if let Some(tag) = ref_tag(&r, &root) {
                            span { class: "tag", "{tag}" }
                        }
                    }
                }
                if create {
                    button { class: "srow", disabled: busy, onclick: {
                            let mut pick = pick.clone();
                            let name = new_name.clone();
                            move |_| pick(json!({"name": name}), true)
                        },
                        Icon { name: "plus" }
                        span { class: "grow", "Create new ref “{new_name}”" }
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `branch_picker.rs`'s cases.
    #[test]
    fn a_ref_checked_out_elsewhere_reuses_its_worktree() {
        assert_eq!(local_name("origin/feature/x"), "feature/x");
        assert_eq!(local_name("main"), "main");
        let elsewhere = json!({"name": "dev", "worktreePath": "/repo-dev"});
        assert_eq!(
            selection_target("/repo", None, &elsewhere),
            ("/repo-dev".into(), Some("/repo-dev".into()), true)
        );
        let default = json!({"name": "main", "isDefault": true});
        assert_eq!(
            selection_target("/repo", Some("/wt"), &default),
            ("/repo".into(), None, false)
        );
        let plain = json!({"name": "x"});
        assert_eq!(
            selection_target("/repo", Some("/wt"), &plain),
            ("/wt".into(), Some("/wt".into()), false)
        );
    }
}
