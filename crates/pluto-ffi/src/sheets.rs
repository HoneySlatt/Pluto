//! The composer's sheets (the Dioxus app's `sheets.rs`): the model (effort,
//! Code's mode and access, the models by provider), a thread's actions, the
//! draft's project and workspace, the ref picker ("Search refs...",
//! `vcs.listRefs`, `branch_picker.rs`), renaming and deleting a thread. A
//! sheet acts for the shown thread, else the space's draft.

use pluto_core::model::ModelSelection;
use serde_json::{Value, json};

use crate::badge::{Badge, badge};
use crate::compose::{self, Target};
use crate::session::Session;
use crate::{App, Core, catalog};

/// The composer the sheet acts for: the shown thread, else the draft.
fn target(session: &Session) -> Target {
    match session.selected.clone() {
        Some(id) => Target::Thread { id },
        None if session.chat => Target::Chat,
        None => Target::Code,
    }
}

/// A value and its label.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Labeled {
    pub value: String,
    pub label: String,
}

/// The model's effort trait: its id, its options and the chosen one.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EffortView {
    pub id: String,
    pub options: Vec<Labeled>,
    pub chosen: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ModelChoice {
    pub slug: String,
    pub label: String,
    pub checked: bool,
}

/// A provider instance and its models (`catalog::providers`).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ProviderModels {
    pub instance_id: String,
    pub name: String,
    /// Its mark (`mark_png`).
    pub mark: Option<String>,
    pub models: Vec<ModelChoice>,
}

/// The model sheet.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ModelSheetView {
    /// A chat: no mode nor access.
    pub chat: bool,
    pub effort: Option<EffortView>,
    pub plan: bool,
    pub access: String,
    /// The access's description (`.seg-note`).
    pub access_note: String,
    pub accesses: Vec<Labeled>,
    pub providers: Vec<ProviderModels>,
}

/// A thread's actions sheet.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ThreadMenuView {
    pub title: String,
    pub chat: bool,
    pub pinned: bool,
    pub settled: bool,
    /// Fork thread shows for a code thread that ran.
    pub can_fork: bool,
    /// `confirmThreadDelete`: Delete asks first.
    pub confirm_delete: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ProjectChoice {
    pub badge: Badge,
    pub checked: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RefRow {
    pub name: String,
    /// `current`, `worktree`, `remote` or `default`.
    pub tag: Option<String>,
}

/// The ref picker's list for a query.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RefsView {
    /// The refs once listed.
    pub refs: Option<Vec<RefRow>>,
    /// Why they could not be listed.
    pub error: Option<String>,
    /// A thread's picker offers to create this ref.
    pub create: Option<String>,
}

/// A ref switch on its way: its request key, the branch it shows, and the
/// thread's worktree afterwards.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RefSwitch {
    pub key: String,
    pub thread: String,
    pub shown: String,
    pub remote: bool,
    pub create: bool,
    pub worktree: Option<String>,
}

/// What picking a ref did: closed the sheet at once (`done`), or started
/// a switch to follow (`finish_ref_switch` once its reply came).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RefPick {
    pub done: bool,
    pub switch: Option<RefSwitch>,
}

/// Where the ref picker lists: the shown thread's checkout, else the Code
/// draft's project; its project root, thread and worktree.
struct Picker {
    cwd: String,
    root: String,
    thread: Option<String>,
    worktree: Option<String>,
}

fn picker(app: &App) -> Picker {
    let session = &app.session;
    match session.selected.clone().and_then(|id| session.thread(&id)) {
        Some(thread) => {
            let root = session
                .state
                .project(&thread.project_id)
                .map(|p| p.workspace_root.clone())
                .unwrap_or_default();
            Picker {
                cwd: thread.worktree_path.clone().unwrap_or_else(|| root.clone()),
                root,
                thread: Some(thread.id.clone()),
                worktree: thread.worktree_path.clone(),
            }
        }
        None => {
            let root = compose::code_project(session, app.drafts.project.as_deref())
                .map(|p| p.workspace_root.clone())
                .unwrap_or_default();
            Picker {
                cwd: root.clone(),
                root,
                thread: None,
                worktree: None,
            }
        }
    }
}

fn refs_key(cwd: &str, query: &str) -> String {
    format!("refs:{cwd}\0{query}")
}

/// A typed ref name: its words joined by dashes.
fn ref_name(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join("-")
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
fn set_thread_branch(session: &mut Session, id: &str, branch: &str, worktree: Option<String>) {
    let moved = session
        .thread(id)
        .is_some_and(|t| t.worktree_path != worktree);
    let detach: Vec<String> = session
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
        .unwrap_or_default();
    let base = uuid::Uuid::new_v4().to_string();
    for session_id in detach {
        session.dispatch(json!({
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
    session.dispatch(command);
}

/// The thread's current model selection for the sheet's composer.
fn selection(app: &App, target: &Target) -> ModelSelection {
    compose::model(
        &app.session,
        &app.drafts,
        target,
        app.drafts.project.as_deref(),
    )
}

#[uniffi::export]
impl Core {
    pub fn model_sheet(&self) -> ModelSheetView {
        self.read(|app| {
            let s = &app.session;
            let target = target(s);
            let project = app.drafts.project.as_deref();
            let chat = match &target {
                Target::Chat => true,
                Target::Code => false,
                Target::Thread { id } => s.thread(id).is_some_and(|t| s.space(&t).is_chat()),
            };
            let current = selection(app, &target);
            let access = compose::access(s, &app.drafts, &target, project);
            ModelSheetView {
                chat,
                effort: catalog::effort(s.providers(), &current).map(|(id, options, chosen)| {
                    EffortView {
                        id,
                        options: options
                            .into_iter()
                            .map(|(value, label)| Labeled { value, label })
                            .collect(),
                        chosen,
                    }
                }),
                plan: compose::plan(s, &app.drafts, &target),
                access_note: catalog::access(&access).3.into(),
                access,
                accesses: catalog::ACCESS
                    .iter()
                    .map(|(value, label, _, _)| Labeled {
                        value: (*value).into(),
                        label: (*label).into(),
                    })
                    .collect(),
                providers: catalog::providers(s.providers(), chat)
                    .into_iter()
                    .map(|p| ProviderModels {
                        mark: catalog::mark(&p.driver).map(str::to_owned),
                        models: p
                            .models
                            .iter()
                            .map(|(slug, label)| ModelChoice {
                                checked: current.instance_id == p.instance_id
                                    && current.model == *slug,
                                slug: slug.clone(),
                                label: label.clone(),
                            })
                            .collect(),
                        instance_id: p.instance_id,
                        name: p.name,
                    })
                    .collect(),
            }
        })
    }

    /// Picks the composer's model (without options).
    pub fn pick_model(&self, instance_id: String, slug: String) {
        self.update(|app, _| {
            let key = target(&app.session).key();
            app.drafts.models.insert(
                key,
                ModelSelection {
                    instance_id,
                    model: slug,
                    options: None,
                },
            );
        });
    }

    /// Sets the model's effort trait `id` to `value`.
    pub fn pick_effort(&self, id: String, value: String) {
        self.update(|app, _| {
            let target = target(&app.session);
            let next = catalog::with_option(&selection(app, &target), &id, &value);
            app.drafts.models.insert(target.key(), next);
        });
    }

    /// Build (`false`) or Plan (`true`).
    pub fn pick_plan(&self, plan: bool) {
        self.update(|app, _| {
            let key = target(&app.session).key();
            app.drafts.plan.insert(key, plan);
        });
    }

    pub fn pick_access(&self, access: String) {
        self.update(|app, _| {
            let key = target(&app.session).key();
            app.drafts.access.insert(key, access);
        });
    }

    /// A thread's actions, `None` once it is no longer listed.
    pub fn thread_menu(&self, id: String) -> Option<ThreadMenuView> {
        self.read(|app| {
            let s = &app.session;
            let thread = s.thread(&id)?;
            let chat = s.space(&thread).is_chat();
            Some(ThreadMenuView {
                title: thread.title.clone(),
                chat,
                pinned: thread.pinned_at.is_some(),
                settled: thread.settled_override.as_deref() == Some("settled"),
                can_fork: !chat && thread.latest_run_id.is_some(),
                confirm_delete: s.prefs.flag("confirmThreadDelete"),
            })
        })
    }

    /// `thread.pin`, `thread.unpin`, `thread.settle` or `thread.unsettle`.
    pub fn thread_command(&self, id: String, kind: String) {
        self.update(|app, _| {
            let mut command = pluto_core::model::command(&kind, &id);
            if kind == "thread.unsettle" {
                command["reason"] = json!("user");
            }
            app.session.dispatch(command);
        });
    }

    /// Forks a code thread from its latest run; the key to follow.
    pub fn fork_thread(&self, id: String) -> Option<String> {
        self.update(|app, _| {
            let thread = app.session.thread(&id)?;
            let run = thread.latest_run_id.clone()?;
            Some(app.session.dispatch_tracked(json!({
                "type": "thread.fork",
                "commandId": uuid::Uuid::new_v4().to_string(),
                "createdBy": "user",
                "creationSource": "web",
                "sourceThreadId": id,
                "targetThreadId": uuid::Uuid::new_v4().to_string(),
                "sourcePoint": {"type": "run", "runId": run},
                "title": format!("{} fork", thread.title),
            })))
        })
    }

    /// Deletes a thread; the key to follow.
    pub fn delete_thread(&self, id: String) -> String {
        self.update(|app, _| {
            app.session
                .dispatch_tracked(pluto_core::model::command("thread.delete", &id))
        })
    }

    /// A deleted thread that was open leaves for the draft.
    pub fn leave_deleted(&self, id: String) {
        self.update(|app, _| {
            if app.session.selected.as_deref() == Some(id.as_str()) {
                app.session.select(None);
            }
        });
    }

    /// Renames a thread; the key to follow.
    pub fn rename_thread(&self, id: String, title: String) -> String {
        self.update(|app, _| {
            let mut command = pluto_core::model::command("thread.metadata.update", &id);
            command["title"] = json!(title.trim());
            app.session.dispatch_tracked(command)
        })
    }

    /// A thread's title, empty once it is no longer listed.
    pub fn thread_title(&self, id: String) -> String {
        self.read(|app| {
            app.session
                .thread(&id)
                .map(|t| t.title.clone())
                .unwrap_or_default()
        })
    }

    /// The code projects, the Code draft's checked.
    pub fn project_choices(&self) -> Vec<ProjectChoice> {
        self.read(|app| {
            let s = &app.session;
            let current =
                compose::code_project(s, app.drafts.project.as_deref()).map(|p| p.id.clone());
            s.code_projects()
                .into_iter()
                .map(|p| ProjectChoice {
                    checked: current.as_deref() == Some(p.id.as_str()),
                    badge: badge(p),
                })
                .collect()
        })
    }

    /// The Code draft's project; its workspace and base start over.
    pub fn pick_project(&self, id: String) {
        self.update(|app, _| {
            app.drafts.project = Some(id);
            app.drafts.worktree = None;
            app.drafts.base = None;
        });
    }

    /// Whether the Code draft starts in a new worktree.
    pub fn draft_worktree(&self) -> bool {
        self.read(|app| {
            let s = &app.session;
            let project = compose::code_project(s, app.drafts.project.as_deref());
            app.drafts
                .worktree
                .unwrap_or_else(|| project.is_some_and(|p| compose::default_worktree(s, p)))
        })
    }

    pub fn pick_worktree(&self, worktree: bool) {
        self.update(|app, _| app.drafts.worktree = Some(worktree));
    }

    /// The refs matching `query` where the picker lists, asked for once.
    pub fn refs(&self, query: String) -> RefsView {
        // Asking changes nothing on screen yet: no notification.
        let mut app = self.hub.lock();
        let picker = picker(&app);
        let query = query.trim().to_owned();
        let key = refs_key(&picker.cwd, &query);
        if !app.session.replies.contains_key(&key) && !app.session.waiting(&key) {
            let mut payload = json!({"cwd": picker.cwd, "limit": 100});
            if !query.is_empty() {
                payload["query"] = json!(ref_name(&query));
            }
            app.session.request(key.clone(), "vcs.listRefs", payload);
        }
        let reply = app.session.replies.get(&key);
        let list: Option<Vec<Value>> = reply
            .and_then(|r| r.as_ref().ok())
            .map(|v| v["refs"].as_array().cloned().unwrap_or_default());
        let name = ref_name(&query);
        let create = (picker.thread.is_some()
            && !name.is_empty()
            && list
                .as_ref()
                .is_some_and(|refs| !refs.iter().any(|r| r["name"] == name.as_str())))
        .then_some(name);
        RefsView {
            refs: list.map(|refs| {
                refs.iter()
                    .map(|r| RefRow {
                        name: r["name"].as_str().unwrap_or_default().to_owned(),
                        tag: ref_tag(r, &picker.root).map(str::to_owned),
                    })
                    .collect()
            }),
            error: reply.and_then(|r| r.as_ref().err().cloned()),
            create,
        }
    }

    /// Picks the ref `name` (`create`: a new one): the draft's base, or the
    /// thread's branch (`vcs.switchRef`, or `vcs.createRef`).
    pub fn pick_ref(&self, name: String, create: bool) -> RefPick {
        self.update(|app, _| {
            let picker = picker(app);
            let Some(thread) = picker.thread.clone() else {
                app.drafts.base = Some(name);
                return RefPick {
                    done: true,
                    switch: None,
                };
            };
            let key = format!("branch-action:{}", uuid::Uuid::new_v4());
            if create {
                app.session.request(
                    key.clone(),
                    "vcs.createRef",
                    json!({"cwd": picker.cwd, "refName": name, "switchRef": true}),
                );
                return RefPick {
                    done: false,
                    switch: Some(RefSwitch {
                        key,
                        thread,
                        shown: name,
                        remote: false,
                        create: true,
                        worktree: picker.worktree,
                    }),
                };
            }
            // The ref as listed (its worktree, whether it is remote).
            let listed = app
                .session
                .replies
                .iter()
                .filter(|(k, _)| k.starts_with(&format!("refs:{}\0", picker.cwd)))
                .filter_map(|(_, r)| r.as_ref().ok())
                .filter_map(|v| v["refs"].as_array())
                .flatten()
                .find(|r| r["name"] == name.as_str())
                .cloned()
                .unwrap_or_else(|| json!({"name": name}));
            let (target, next, reuse) =
                selection_target(&picker.root, picker.worktree.as_deref(), &listed);
            // A worktree that has the ref checked out already is reused.
            if reuse {
                set_thread_branch(&mut app.session, &thread, &name, next);
                return RefPick {
                    done: true,
                    switch: None,
                };
            }
            let remote = listed["isRemote"] == true;
            let shown = if remote { local_name(&name) } else { &name }.to_owned();
            app.session.request(
                key.clone(),
                "vcs.switchRef",
                json!({"cwd": target, "refName": name}),
            );
            RefPick {
                done: false,
                switch: Some(RefSwitch {
                    key,
                    thread,
                    shown,
                    remote,
                    create: false,
                    worktree: next,
                }),
            }
        })
    }

    /// The switch's answer, once it came: the thread records the branch
    /// (`None`), or why the checkout failed.
    pub fn finish_ref_switch(&self, switch: RefSwitch) -> Option<String> {
        self.update(|app, _| {
            let result = app.session.replies.remove(&switch.key)?;
            match result {
                Ok(value) => {
                    let returned = value["refName"].as_str().map(str::to_owned);
                    let branch = if switch.create || switch.remote {
                        returned.unwrap_or(switch.shown)
                    } else {
                        switch.shown
                    };
                    set_thread_branch(&mut app.session, &switch.thread, &branch, switch.worktree);
                    None
                }
                Err(error) => Some(format!(
                    "{} {error}",
                    if switch.create {
                        "Failed to create and switch ref."
                    } else {
                        "Failed to switch ref."
                    }
                )),
            }
        })
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

    #[test]
    fn refs_take_their_tags_and_typed_names_their_dashes() {
        assert_eq!(ref_name("  fix the  parser "), "fix-the-parser");
        assert_eq!(ref_tag(&json!({"current": true}), "/r"), Some("current"));
        assert_eq!(
            ref_tag(&json!({"worktreePath": "/r"}), "/r"),
            None,
            "the project's own checkout is no other worktree"
        );
        assert_eq!(ref_tag(&json!({"isRemote": true}), "/r"), Some("remote"));
        assert_eq!(ref_tag(&json!({"isDefault": true}), "/r"), Some("default"));
    }
}
