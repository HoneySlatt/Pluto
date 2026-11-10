//! The header's git actions (`GitActionsControl.tsx`,
//! `GitActionsControl.logic.ts`): the quick action the status suggests
//! (commit, push, create or view the pull request, pull), the menu of the
//! others, the "Commit changes" dialog (branch, files to include, an
//! optional message) and the confirmation before acting on the default
//! branch. Actions run as `git.runStackedAction` streams whose progress
//! events drive a loading toast, settled with the server's result toast
//! and its follow-up action. "Publish repository" (a repository without a
//! remote) is not ported.
use eframe::egui::{
    self, Color32, CornerRadius, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::context_menu::{self, Item};
use crate::ui::toasts::{self, Kind, Toast};
use crate::ui::widgets;

/// `getChangeRequestTerminology`: GitLab says "MR", the rest "PR".
#[derive(Clone, Copy)]
pub(crate) struct Terminology {
    pub short: &'static str,
    pub singular: &'static str,
}

pub(crate) fn terminology(status: Option<&Value>) -> Terminology {
    match status.and_then(|s| s["sourceControlProvider"]["kind"].as_str()) {
        Some("gitlab") => Terminology {
            short: "MR",
            singular: "merge request",
        },
        Some("unknown") => Terminology {
            short: "change request",
            singular: "change request",
        },
        _ => Terminology {
            short: "PR",
            singular: "pull request",
        },
    }
}

/// The provider's mark (`getSourceControlPresentation(...).Icon`).
fn source_control_icon(status: Option<&Value>) -> &'static str {
    match status.and_then(|s| s["sourceControlProvider"]["kind"].as_str()) {
        Some("gitlab") => "sc-gitlab",
        Some("forgejo") => "sc-forgejo",
        Some("unknown") => "git-pull-request-arrow",
        _ => "sc-github",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum QuickKind {
    RunAction(&'static str),
    RunPull,
    OpenPr,
    OpenPublish,
    ShowHint(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QuickAction {
    pub label: String,
    pub disabled: bool,
    pub kind: QuickKind,
}

/// The status fields the decisions read.
struct Facts {
    has_branch: bool,
    has_changes: bool,
    open_pr: bool,
    ahead: bool,
    behind: bool,
    default_delta: bool,
    has_upstream: bool,
}

fn facts(status: &Value) -> Facts {
    let count = |key: &str| status[key].as_u64().unwrap_or(0);
    Facts {
        has_branch: !status["refName"].is_null(),
        has_changes: status["hasWorkingTreeChanges"] == true,
        open_pr: status["pr"]["state"] == "open",
        ahead: count("aheadCount") > 0,
        behind: count("behindCount") > 0,
        default_delta: status["aheadOfDefaultCount"]
            .as_u64()
            .unwrap_or_else(|| count("aheadCount"))
            > 0,
        has_upstream: status["hasUpstream"] == true,
    }
}

/// `resolveQuickAction`.
pub(crate) fn resolve_quick_action(
    status: Option<&Value>,
    busy: bool,
    is_default: bool,
    has_remote: bool,
) -> QuickAction {
    let hint = |label: &str, hint: &str| QuickAction {
        label: label.into(),
        disabled: true,
        kind: QuickKind::ShowHint(hint.into()),
    };
    let run = |label: String, action: &'static str| QuickAction {
        label,
        disabled: false,
        kind: QuickKind::RunAction(action),
    };
    let open = |kind: QuickKind, label: String| QuickAction {
        label,
        disabled: false,
        kind,
    };
    if busy {
        return hint("Commit", "Git action in progress.");
    }
    let Some(status) = status else {
        return hint("Commit", "Git status is unavailable.");
    };
    let f = facts(status);
    let t = terminology(Some(status));
    if !f.has_branch {
        return hint(
            "Commit",
            &format!(
                "Create and checkout a ref before pushing or opening a {}.",
                t.singular
            ),
        );
    }
    if f.has_changes {
        if !f.has_upstream && !has_remote {
            return run("Commit".into(), "commit");
        }
        if f.open_pr || is_default {
            return run("Commit & push".into(), "commit_push");
        }
        return run(format!("Commit, push & {}", t.short), "commit_push_pr");
    }
    if !f.has_upstream {
        if !has_remote {
            if f.open_pr && !f.ahead {
                return open(QuickKind::OpenPr, format!("View {}", t.short));
            }
            return open(QuickKind::OpenPublish, "Publish repository".into());
        }
        if !f.ahead {
            if f.open_pr {
                return open(QuickKind::OpenPr, format!("View {}", t.short));
            }
            return hint("Push", "No local commits to push.");
        }
        if f.open_pr || is_default {
            return run(
                "Push".into(),
                if is_default { "commit_push" } else { "push" },
            );
        }
        return run(format!("Push & create {}", t.short), "create_pr");
    }
    if f.ahead && f.behind {
        return hint(
            "Sync ref",
            "Branch has diverged from upstream. Rebase/merge first.",
        );
    }
    if f.behind {
        return open(QuickKind::RunPull, "Pull".into());
    }
    if f.ahead {
        if f.open_pr || is_default {
            return run(
                "Push".into(),
                if is_default { "commit_push" } else { "push" },
            );
        }
        return run(format!("Push & create {}", t.short), "create_pr");
    }
    if f.open_pr && f.has_upstream {
        return open(QuickKind::OpenPr, format!("View {}", t.short));
    }
    if f.default_delta && !is_default {
        return run(format!("Create {}", t.short), "create_pr");
    }
    hint("Commit", "Branch is up to date. No action needed.")
}

/// `GitQuickActionIcon`.
pub(crate) fn quick_action_icon(action: &QuickAction, status: Option<&Value>) -> &'static str {
    match &action.kind {
        QuickKind::OpenPr => source_control_icon(status),
        QuickKind::OpenPublish => "cloud-upload",
        QuickKind::RunPull => "cloud-download",
        QuickKind::RunAction("commit") => "git-commit-horizontal",
        QuickKind::RunAction("push" | "commit_push") => "cloud-upload",
        QuickKind::RunAction(_) => source_control_icon(status),
        QuickKind::ShowHint(_) if action.label == "Commit" => "git-commit-horizontal",
        QuickKind::ShowHint(_) if action.label == "Push" => "cloud-upload",
        QuickKind::ShowHint(_) => "info",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MenuEntry {
    pub id: &'static str,
    pub label: String,
    pub disabled: bool,
    pub icon: &'static str,
}

/// `buildMenuItems`.
pub(crate) fn build_menu_items(
    status: Option<&Value>,
    busy: bool,
    has_remote: bool,
) -> Vec<MenuEntry> {
    let Some(status) = status else {
        return Vec::new();
    };
    let f = facts(status);
    let t = terminology(Some(status));
    let can_push_without_upstream = has_remote && !f.has_upstream;
    let can_commit = !busy && f.has_changes;
    let can_push = !busy
        && f.has_branch
        && !f.behind
        && f.ahead
        && (f.has_upstream || can_push_without_upstream);
    let can_create_pr = !busy
        && f.has_branch
        && !f.has_changes
        && !f.open_pr
        && f.default_delta
        && !f.behind
        && (f.has_upstream || can_push_without_upstream);
    let commit = MenuEntry {
        id: "commit",
        label: "Commit".into(),
        disabled: !can_commit,
        icon: "git-commit-horizontal",
    };
    if !has_remote {
        return vec![commit];
    }
    let pr = if f.open_pr {
        MenuEntry {
            id: "pr",
            label: format!("View {}", t.short),
            disabled: busy,
            icon: source_control_icon(Some(status)),
        }
    } else {
        MenuEntry {
            id: "pr",
            label: format!("Create {}", t.short),
            disabled: !can_create_pr,
            icon: source_control_icon(Some(status)),
        }
    };
    vec![
        commit,
        MenuEntry {
            id: "push",
            label: "Push".into(),
            disabled: !can_push,
            icon: "cloud-upload",
        },
        pr,
    ]
}

/// `getMenuActionDisabledReason` (`GitActionsControl.tsx`).
pub(crate) fn menu_disabled_reason(
    entry: &MenuEntry,
    status: Option<&Value>,
    busy: bool,
    has_remote: bool,
) -> Option<String> {
    if !entry.disabled {
        return None;
    }
    if busy {
        return Some("Git action in progress.".into());
    }
    let Some(status) = status else {
        return Some("Git status is unavailable.".into());
    };
    let f = facts(status);
    let t = terminology(Some(status)).singular;
    let no_remote = !f.has_upstream && !has_remote;
    let reason = match entry.id {
        "commit" if !f.has_changes => "Worktree is clean. Make changes before committing.".into(),
        "commit" => "Commit is currently unavailable.".into(),
        "push" if !f.has_branch => "Detached HEAD: check out a branch before pushing.".into(),
        "push" if f.has_changes => "Commit or stash local changes before pushing.".into(),
        "push" if f.behind => "Branch is behind upstream. Pull/rebase before pushing.".into(),
        "push" if no_remote => "Add an \"origin\" remote before pushing.".into(),
        "push" if !f.ahead => "No local commits to push.".into(),
        "push" => "Push is currently unavailable.".into(),
        _ if f.open_pr => format!("View {t} is currently unavailable."),
        _ if !f.has_branch => format!("Detached HEAD: check out a branch before creating a {t}."),
        _ if f.has_changes => format!("Commit local changes before creating a {t}."),
        _ if no_remote => format!("Add an \"origin\" remote before creating a {t}."),
        _ if !f.ahead => format!("No local commits to include in a {t}."),
        _ if f.behind => format!("Branch is behind upstream. Pull/rebase before creating a {t}."),
        _ => format!("Create {t} is currently unavailable."),
    };
    Some(reason)
}

/// `buildGitActionProgressStages`.
pub(crate) fn progress_stages(
    action: &str,
    custom_message: bool,
    has_changes: bool,
    feature_branch: bool,
    push_before_pr: bool,
    t: Terminology,
) -> Vec<String> {
    let branch: Vec<String> = if feature_branch {
        vec!["Preparing feature ref...".into()]
    } else {
        Vec::new()
    };
    let push = "Pushing...".to_owned();
    let pr = vec![
        format!("Preparing {}...", t.short),
        format!("Generating {} content...", t.short),
        format!("Creating {}...", t.singular),
    ];
    if action == "push" {
        return vec![push];
    }
    if action == "create_pr" {
        return if push_before_pr {
            [vec![push], pr].concat()
        } else {
            pr
        };
    }
    let commit: Vec<String> = if action != "commit" && !has_changes {
        Vec::new()
    } else if custom_message {
        vec!["Committing...".into()]
    } else {
        vec![
            "Generating commit message...".into(),
            "Committing...".into(),
        ]
    };
    match action {
        "commit" => [branch, commit].concat(),
        "commit_push" => [branch, commit, vec![push]].concat(),
        _ => [branch, commit, vec![push], pr].concat(),
    }
}

/// `resolveDefaultBranchActionDialogCopy`: (title, description, continue).
pub(crate) fn default_branch_copy(
    action: &str,
    branch: &str,
    includes_commit: bool,
    t: Terminology,
) -> (String, String, String) {
    let suffix = format!(
        " on \"{branch}\". You can continue on this ref or create a feature ref and run the same action there."
    );
    if action == "push" || action == "commit_push" {
        if includes_commit {
            return (
                "Commit & push to default ref?".into(),
                format!("This action will commit and push changes{suffix}"),
                format!("Commit & push to {branch}"),
            );
        }
        return (
            "Push to default ref?".into(),
            format!("This action will push local commits{suffix}"),
            format!("Push to {branch}"),
        );
    }
    if includes_commit {
        return (
            format!("Commit, push & create {} from default ref?", t.short),
            format!(
                "This action will commit, push, and create a {}{suffix}",
                t.singular
            ),
            format!("Commit, push & create {}", t.short),
        );
    }
    (
        format!("Push & create {} from default ref?", t.short),
        format!(
            "This action will push local commits and create a {}{suffix}",
            t.singular
        ),
        format!("Push & create {}", t.short),
    )
}

/// A request to run, as the dialogs and toasts carry it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Run {
    pub action: String,
    pub message: Option<String>,
    pub files: Option<Vec<String>>,
    pub feature_branch: bool,
    pub skip_default_prompt: bool,
}

/// A running stacked action and what its toast shows.
pub(crate) struct Running {
    key: String,
    action_id: String,
    toast: u64,
    title: String,
    phase: String,
    started: Option<f64>,
    hook_started: Option<f64>,
    output: Option<String>,
}

pub(crate) struct CommitDialog {
    message: String,
    excluded: std::collections::BTreeSet<String>,
    editing: bool,
}

pub(crate) struct PendingDefault {
    run: Run,
    branch: String,
    includes_commit: bool,
}

#[derive(Default)]
pub(crate) struct GitActions {
    running: Option<Running>,
    pulling: Option<u64>,
    menu: Option<context_menu::State>,
    commit: Option<CommitDialog>,
    pending_default: Option<PendingDefault>,
}

const PULL_REQUEST: &str = "git-pull";

impl App {
    /// The open workspace's status, when it is a repository.
    fn git_status(&self) -> Option<&Value> {
        let cwd = self.workspace_cwd()?;
        self.vcs
            .as_ref()
            .filter(|(c, status)| *c == cwd && status["isRepo"] == true)
            .map(|(_, status)| status)
    }

    fn git_busy(&self) -> bool {
        self.git.running.is_some() || self.git.pulling.is_some()
    }

    /// The header's quick action, label and icon.
    pub(crate) fn git_quick_action(&self) -> (QuickAction, &'static str) {
        let status = self.git_status();
        let is_default = status.is_some_and(|s| s["isDefaultRef"] == true);
        let has_remote = status.is_some_and(|s| s["hasPrimaryRemote"] == true);
        let action = resolve_quick_action(status, self.git_busy(), is_default, has_remote);
        let icon = quick_action_icon(&action, status);
        (action, icon)
    }

    /// `runQuickAction`.
    pub(crate) fn run_quick_git_action(&mut self) {
        let (action, _) = self.git_quick_action();
        match action.kind {
            QuickKind::OpenPr => self.open_existing_pr(),
            QuickKind::OpenPublish => self.open_publish_dialog(),
            QuickKind::RunPull => self.git_pull(),
            QuickKind::ShowHint(hint) => {
                self.toasts
                    .add(Toast::new(Kind::Info, action.label).description(hint));
            }
            QuickKind::RunAction(name) => self.run_git_action(Run {
                action: name.into(),
                ..Default::default()
            }),
        }
    }

    pub(crate) fn toggle_git_menu(&mut self, at: Pos2) {
        self.git.menu = match self.git.menu {
            Some(_) => None,
            None => Some(context_menu::State::dropdown(at).aligned_end()),
        };
    }

    fn open_existing_pr(&mut self) {
        let url = self
            .git_status()
            .filter(|s| s["pr"]["state"] == "open")
            .and_then(|s| s["pr"]["url"].as_str())
            .map(str::to_owned);
        match url {
            Some(url) => widgets::open_external(&url),
            None => {
                self.toasts
                    .add(Toast::new(Kind::Error, "No open pull request found."));
            }
        }
    }

    fn git_pull(&mut self) {
        let Some(cwd) = self.workspace_cwd() else {
            return;
        };
        let toast = self.toasts.add(Toast::new(Kind::Loading, "Pulling..."));
        self.git.pulling = Some(toast);
        self.send_request(PULL_REQUEST.into(), "vcs.pull", json!({ "cwd": cwd }));
    }

    /// `runGitActionWithToast`: asks first on the default branch, then
    /// streams the action under a loading toast.
    pub(crate) fn run_git_action(&mut self, run: Run) {
        let Some(cwd) = self.workspace_cwd() else {
            return;
        };
        let status = self.git_status().cloned();
        let branch = status
            .as_ref()
            .and_then(|s| s["refName"].as_str())
            .map(str::to_owned);
        let is_default =
            !run.feature_branch && status.as_ref().is_some_and(|s| s["isDefaultRef"] == true);
        let has_changes = status
            .as_ref()
            .is_some_and(|s| s["hasWorkingTreeChanges"] == true);
        let can_commit = matches!(
            run.action.as_str(),
            "commit" | "commit_push" | "commit_push_pr"
        );
        let includes_commit =
            can_commit && (run.action == "commit" || has_changes || run.feature_branch);
        let needs_confirm = is_default
            && matches!(
                run.action.as_str(),
                "push" | "create_pr" | "commit_push" | "commit_push_pr"
            );
        if !run.skip_default_prompt
            && needs_confirm
            && let Some(branch) = branch
        {
            self.git.pending_default = Some(PendingDefault {
                run,
                branch,
                includes_commit,
            });
            return;
        }
        let t = terminology(status.as_ref());
        let push_before_pr = run.action == "create_pr"
            && status.as_ref().is_some_and(|s| {
                s["hasUpstream"] != true || s["aheadCount"].as_u64().unwrap_or(0) > 0
            });
        let stages = progress_stages(
            &run.action,
            run.message.as_deref().is_some_and(|m| !m.trim().is_empty()),
            has_changes,
            run.feature_branch,
            push_before_pr,
            t,
        );
        let title = stages
            .first()
            .cloned()
            .unwrap_or_else(|| "Running git action...".into());
        let toast = self
            .toasts
            .add(Toast::new(Kind::Loading, title.clone()).description("Waiting for Git..."));
        let action_id = uuid::Uuid::new_v4().to_string();
        let key = format!("git-action:{action_id}");
        let mut payload = json!({"actionId": action_id, "cwd": cwd, "action": run.action});
        if let Some(message) = run
            .message
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            payload["commitMessage"] = json!(message);
        }
        if run.feature_branch {
            payload["featureBranch"] = json!(true);
        }
        if let Some(files) = &run.files {
            payload["filePaths"] = json!(files);
        }
        // A pull request it opens is linked to the thread it ran beside.
        if let Some(thread) = &self.selected {
            payload["threadId"] = json!(thread);
        }
        self.git.running = Some(Running {
            key: key.clone(),
            action_id,
            toast,
            title: title.clone(),
            phase: title,
            started: None,
            hook_started: None,
            output: None,
        });
        self.send(crate::protocol::Action::Stream {
            key,
            method: "git.runStackedAction",
            payload,
        });
    }

    /// Stream values, results and failures of git requests; `false` for
    /// other keys.
    pub(crate) fn git_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        if key == PULL_REQUEST {
            let Some(toast) = self.git.pulling.take() else {
                return true;
            };
            let settled = match result {
                Ok(value) => {
                    let pulled = value["status"] == "pulled";
                    let reference = value["refName"].as_str().unwrap_or("HEAD");
                    Toast::new(
                        Kind::Success,
                        if pulled {
                            "Pulled"
                        } else {
                            "Already up to date"
                        },
                    )
                    .description(if pulled {
                        format!(
                            "Updated {reference} from {}",
                            value["upstreamRef"].as_str().unwrap_or("upstream")
                        )
                    } else {
                        format!("{reference} is already synchronized.")
                    })
                }
                Err(error) => Toast::new(Kind::Error, "Pull failed").description(error.clone()),
            };
            self.toasts.update(toast, settled);
            return true;
        }
        let Some(running) = &mut self.git.running else {
            return key.starts_with("git-action:");
        };
        if running.key != key {
            return key.starts_with("git-action:");
        }
        let now = self
            .egui_ctx
            .as_ref()
            .map_or(0.0, |ctx| ctx.input(|i| i.time));
        let event = match result {
            Err(error) => {
                let toast = running.toast;
                self.git.running = None;
                self.toasts.update(
                    toast,
                    Toast::new(Kind::Error, "Action failed").description(error.clone()),
                );
                return true;
            }
            Ok(event) => event,
        };
        if event.is_null() || event["actionId"] != running.action_id.as_str() {
            return true;
        }
        match event["kind"].as_str() {
            Some("action_started") => {
                running.started = Some(now);
                running.hook_started = None;
                running.output = None;
            }
            Some("phase_started") => {
                let label = event["label"].as_str().unwrap_or_default().to_owned();
                running.title = label.clone();
                running.phase = label;
                running.started = Some(now);
                running.hook_started = None;
                running.output = None;
            }
            Some("hook_started") => {
                running.title = format!(
                    "Running {}...",
                    event["hookName"].as_str().unwrap_or("hook")
                );
                running.hook_started = Some(now);
                running.output = None;
            }
            Some("hook_output") => {
                running.output = event["text"].as_str().map(str::to_owned);
            }
            Some("hook_finished") => {
                running.title = running.phase.clone();
                running.hook_started = None;
                running.output = None;
            }
            Some("action_finished") => {
                let toast = running.toast;
                self.git.running = None;
                self.finish_git_action(toast, &event["result"]);
                return true;
            }
            Some("action_failed") => {
                let toast = running.toast;
                self.git.running = None;
                self.toasts.update(
                    toast,
                    Toast::new(Kind::Error, "Action failed")
                        .description(event["message"].as_str().unwrap_or("An error occurred.")),
                );
                return true;
            }
            _ => {}
        }
        self.refresh_git_toast(now);
        true
    }

    /// The loading toast's title and its last hook line or elapsed time.
    fn refresh_git_toast(&mut self, now: f64) {
        let Some(running) = &self.git.running else {
            return;
        };
        let description = running.output.clone().or_else(|| {
            running.hook_started.or(running.started).map(|start| {
                let seconds = (now - start).max(0.0) as u64;
                if seconds < 60 {
                    format!("Running for {seconds}s")
                } else {
                    format!("Running for {}m {}s", seconds / 60, seconds % 60)
                }
            })
        });
        let mut toast = Toast::new(Kind::Loading, running.title.clone());
        toast.description = description;
        self.toasts.update(running.toast, toast);
    }

    /// The server's result toast and its follow-up; a new feature branch
    /// becomes the thread's branch (`syncThreadBranchAfterGitAction`).
    fn finish_git_action(&mut self, toast: u64, result: &Value) {
        if result["branch"]["status"] == "created"
            && let (Some(name), Some(thread)) =
                (result["branch"]["name"].as_str(), self.selected.clone())
        {
            let mut command = crate::model::command("thread.metadata.update", &thread);
            command["branch"] = json!(name);
            self.dispatch(command);
        }
        let notice = &result["toast"];
        let mut settled = Toast::new(
            Kind::Success,
            notice["title"].as_str().unwrap_or("Done").to_owned(),
        );
        settled.description = notice["description"].as_str().map(str::to_owned);
        settled.timeout = Some(10.0);
        let cta = &notice["cta"];
        settled.action = match cta["kind"].as_str() {
            Some("run_action") => cta["action"]["kind"].as_str().map(|action| {
                (
                    cta["label"].as_str().unwrap_or("Continue").to_owned(),
                    toasts::Action::GitAction(action.to_owned()),
                )
            }),
            Some("open_pr") => cta["url"].as_str().map(|url| {
                (
                    cta["label"].as_str().unwrap_or("View").to_owned(),
                    toasts::Action::OpenUrl(url.to_owned()),
                )
            }),
            _ => None,
        };
        self.toasts.update(toast, settled);
    }

    /// The running action's elapsed time ticks in its toast.
    pub(crate) fn tick_git_toast(&mut self, ctx: &egui::Context) {
        if self.git.running.is_some() {
            let now = ctx.input(|i| i.time);
            self.refresh_git_toast(now);
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }

    /// The menu (`MenuPopup align="end"`): Commit, Push and the change
    /// request, the disabled ones dimmed.
    pub(crate) fn git_menu(&mut self, ui: &mut Ui) {
        let Some(state) = self.git.menu.take() else {
            return;
        };
        let status = self.git_status().cloned();
        let has_remote = status
            .as_ref()
            .is_some_and(|s| s["hasPrimaryRemote"] == true);
        let busy = self.git_busy();
        let entries = build_menu_items(status.as_ref(), busy, has_remote);
        let mut items: Vec<Item> = entries
            .iter()
            .map(|entry| {
                let mut item = Item::new(entry.id, entry.label.clone()).icon(entry.icon);
                item.disabled = entry.disabled;
                item.tooltip = menu_disabled_reason(entry, status.as_ref(), busy, has_remote);
                item
            })
            .collect();
        // `canPublishRepository`: a repository without a primary remote.
        if status.is_some() && !has_remote {
            let mut publish = Item::new("publish", "Publish repository...").icon("cloud-upload");
            publish.disabled = busy;
            items.push(publish);
        }
        let note = |id: &str, text: &str| {
            let mut item = Item::new(id, text);
            item.note = Some(theme::warning());
            item.disabled = true;
            item
        };
        if let Some(status) = &status {
            let count = |key: &str| status[key].as_u64().unwrap_or(0);
            if status["refName"].is_null() {
                items.push(note(
                    "detached",
                    "Detached HEAD: create and check out a branch to enable push and pull request actions.",
                ));
            } else if status["hasWorkingTreeChanges"] != true
                && count("behindCount") > 0
                && count("aheadCount") == 0
            {
                items.push(note("behind", "Behind upstream. Pull/rebase first."));
            }
        }
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        self.git.menu = open;
        let open_pr = status.as_ref().is_some_and(|s| s["pr"]["state"] == "open");
        match clicked.as_deref() {
            Some("commit") => {
                self.git.commit = Some(CommitDialog {
                    message: String::new(),
                    excluded: Default::default(),
                    editing: false,
                });
            }
            Some("push") => self.run_git_action(Run {
                action: "push".into(),
                ..Default::default()
            }),
            Some("pr") if open_pr => self.open_existing_pr(),
            Some("pr") => self.run_git_action(Run {
                action: "create_pr".into(),
                ..Default::default()
            }),
            Some("publish") => self.open_publish_dialog(),
            _ => {}
        }
    }

    /// The "Commit changes" dialog and the default-branch confirmation.
    pub(crate) fn git_dialogs(&mut self, ui: &mut Ui, screen: Rect) {
        self.commit_dialog(ui, screen);
        self.default_branch_dialog(ui, screen);
    }

    fn commit_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(mut dialog) = self.git.commit.take() else {
            return;
        };
        let status = self.git_status().cloned().unwrap_or(Value::Null);
        let files: Vec<(String, u64, u64)> = status["workingTree"]["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| {
                Some((
                    f["path"].as_str()?.to_owned(),
                    f["insertions"].as_u64().unwrap_or(0),
                    f["deletions"].as_u64().unwrap_or(0),
                ))
            })
            .collect();
        let selected: Vec<&(String, u64, u64)> = files
            .iter()
            .filter(|(path, ..)| !dialog.excluded.contains(path))
            .collect();
        let all_selected = dialog.excluded.is_empty();
        let none_selected = selected.is_empty();
        let mut outcome = None;
        let mut cancelled = false;
        let mut open_file = None;
        egui::Area::new(egui::Id::new("git-commit-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, backdrop_response) =
                    ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(backdrop, 0.0, Color32::from_black_alpha(170));
                let width = 512.0;
                let list_height = if files.is_empty() {
                    20.0
                } else {
                    176.0 + 8.0 + 20.0
                };
                let summary_height = 12.0 + 20.0 + 4.0 + 24.0 + 4.0 + list_height + 12.0;
                let description = widgets::galley(
                    ui,
                    "Review and confirm your commit. Leave the message blank to auto-generate one.",
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                    width - 48.0,
                );
                let description_height = description.size().y;
                let height = 24.0
                    + 20.0
                    + 8.0
                    + description_height
                    + 12.0
                    + 4.0
                    + summary_height
                    + 16.0
                    + 20.0
                    + 4.0
                    + 64.0
                    + 4.0
                    + 16.0
                    + 28.0
                    + 16.0;
                let popup = Rect::from_center_size(backdrop.center(), Vec2::new(width, height));
                let popup_response =
                    ui.interact(popup, egui::Id::new("git-commit-popup"), Sense::click());
                paint_dialog(ui, popup);
                let close = dialog_close(ui, popup, "commit-close");
                let left = popup.left() + 24.0;
                let inner = width - 48.0;
                let mut y = popup.top() + 24.0;
                widgets::text(
                    ui,
                    Pos2::new(left, y),
                    "Commit changes",
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                y += 28.0;
                ui.painter()
                    .galley(Pos2::new(left, y), description, theme::muted_foreground());
                y += description_height + 16.0;
                // The summary card (`rounded-xl bg-white/[0.035] p-3`).
                let card =
                    Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner, summary_height));
                ui.painter().rect(
                    card,
                    CornerRadius::same(12),
                    theme::overlay(0.035),
                    Stroke::new(1.0, theme::overlay(0.05)),
                    StrokeKind::Inside,
                );
                let cx = card.left() + 12.0;
                let cw = card.width() - 24.0;
                let mut cy = card.top() + 12.0;
                widgets::text(
                    ui,
                    Pos2::new(cx, cy),
                    "Branch",
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                );
                let branch_x =
                    cx + widgets::text_width(ui, "Branch", font(14.0, Weight::Regular)) + 8.0;
                widgets::text(
                    ui,
                    Pos2::new(branch_x, cy),
                    status["refName"].as_str().unwrap_or("(detached HEAD)"),
                    font(14.0, Weight::Medium),
                    theme::foreground(),
                    20.0,
                );
                if status["isDefaultRef"] == true {
                    let label = "Default branch";
                    let w = widgets::text_width(ui, label, font(14.0, Weight::Regular));
                    widgets::text(
                        ui,
                        Pos2::new(cx + cw - w, cy),
                        label,
                        font(14.0, Weight::Regular),
                        theme::warning_foreground(),
                        20.0,
                    );
                }
                cy += 24.0;
                // Files header: the select-all box while editing, the count
                // when some are left out, and Edit/Done.
                let mut fx = cx;
                if dialog.editing && !files.is_empty() {
                    let state = if all_selected {
                        Some(true)
                    } else if none_selected {
                        Some(false)
                    } else {
                        None
                    };
                    if checkbox(
                        ui,
                        Rect::from_min_size(Pos2::new(fx, cy + 2.0), Vec2::splat(16.0)),
                        "commit-all",
                        state,
                    ) {
                        dialog.excluded = if all_selected {
                            files.iter().map(|(p, ..)| p.clone()).collect()
                        } else {
                            Default::default()
                        };
                    }
                    fx += 24.0;
                }
                widgets::text(
                    ui,
                    Pos2::new(fx, cy),
                    "Files",
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                );
                if !all_selected && !dialog.editing {
                    let x =
                        fx + widgets::text_width(ui, "Files", font(14.0, Weight::Regular)) + 8.0;
                    widgets::text(
                        ui,
                        Pos2::new(x, cy),
                        &format!("({} of {})", selected.len(), files.len()),
                        font(14.0, Weight::Regular),
                        theme::muted_foreground(),
                        20.0,
                    );
                }
                if !files.is_empty() {
                    let label = if dialog.editing { "Done" } else { "Edit" };
                    let w = widgets::text_width(ui, label, font(12.0, Weight::Medium)) + 16.0;
                    let button =
                        Rect::from_min_size(Pos2::new(cx + cw - w, cy - 2.0), Vec2::new(w, 24.0));
                    let response =
                        ui.interact(button, egui::Id::new("commit-edit"), Sense::click());
                    if response.hovered() {
                        ui.painter()
                            .rect_filled(button, CornerRadius::same(6), theme::accent());
                    }
                    widgets::text_in(
                        ui,
                        Rect::from_center_size(button.center(), Vec2::new(w - 16.0 + 1.0, 24.0)),
                        label,
                        font(12.0, Weight::Medium),
                        theme::foreground(),
                    );
                    if response.clicked() {
                        dialog.editing = !dialog.editing;
                    }
                }
                cy += 28.0;
                if files.is_empty() {
                    widgets::text(
                        ui,
                        Pos2::new(cx, cy),
                        "none",
                        font(14.0, Weight::Medium),
                        theme::foreground(),
                        20.0,
                    );
                } else {
                    let list = Rect::from_min_size(Pos2::new(cx, cy), Vec2::new(cw, 176.0));
                    ui.painter().rect(
                        list,
                        CornerRadius::same(8),
                        theme::overlay(0.025),
                        Stroke::new(1.0, theme::overlay(0.05)),
                        StrokeKind::Inside,
                    );
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list.shrink(4.0)));
                    egui::ScrollArea::vertical()
                        .id_salt("commit-files")
                        .auto_shrink([false, false])
                        .show(&mut child, |ui| {
                            for (path, insertions, deletions) in &files {
                                let excluded = dialog.excluded.contains(path);
                                let (row, _) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), 28.0),
                                    Sense::hover(),
                                );
                                let row_response = ui.interact(
                                    row,
                                    egui::Id::new(("commit-file", path)),
                                    Sense::click(),
                                );
                                if row_response.hovered() {
                                    ui.painter().rect_filled(
                                        row,
                                        CornerRadius::same(6),
                                        theme::alpha(theme::accent(), 0.5),
                                    );
                                }
                                let mut x = row.left() + 8.0;
                                if dialog.editing {
                                    if checkbox(
                                        ui,
                                        Rect::from_min_size(
                                            Pos2::new(x, row.center().y - 8.0),
                                            Vec2::splat(16.0),
                                        ),
                                        ("commit-file-check", path.as_str()),
                                        Some(!excluded),
                                    ) {
                                        if excluded {
                                            dialog.excluded.remove(path);
                                        } else {
                                            dialog.excluded.insert(path.clone());
                                        }
                                    }
                                    x += 24.0;
                                }
                                let counts = if excluded {
                                    vec![("Excluded".to_owned(), theme::muted_foreground())]
                                } else {
                                    vec![
                                        (
                                            format!("+{insertions}"),
                                            theme::diff_addition_foreground(),
                                        ),
                                        (" / ".to_owned(), theme::muted_foreground()),
                                        (
                                            format!("-{deletions}"),
                                            theme::diff_deletion_foreground(),
                                        ),
                                    ]
                                };
                                let mono = theme::mono(13.0);
                                let counts_width: f32 = counts
                                    .iter()
                                    .map(|(t, _)| widgets::text_width(ui, t, mono.clone()))
                                    .sum();
                                let mut cx = row.right() - 8.0 - counts_width;
                                for (text, color) in &counts {
                                    let w = widgets::text_width(ui, text, mono.clone());
                                    widgets::text_in(
                                        ui,
                                        Rect::from_min_size(
                                            Pos2::new(cx, row.top()),
                                            Vec2::new(w + 1.0, row.height()),
                                        ),
                                        text,
                                        mono.clone(),
                                        *color,
                                    );
                                    cx += w;
                                }
                                // `StartTruncatedPath`: the end of a long path stays.
                                let available = row.right() - 8.0 - counts_width - 12.0 - x;
                                let shown = start_truncated(ui, path, &mono, available);
                                widgets::text_in(
                                    ui,
                                    Rect::from_min_max(
                                        Pos2::new(x, row.top()),
                                        Pos2::new(x + available, row.bottom()),
                                    ),
                                    &shown,
                                    mono,
                                    if excluded {
                                        theme::muted_foreground()
                                    } else {
                                        theme::foreground()
                                    },
                                );
                                if row_response.clicked() {
                                    open_file = Some(path.clone());
                                }
                            }
                        });
                    cy += 176.0 + 8.0;
                    let mono = theme::mono(13.0);
                    let added: u64 = selected.iter().map(|(_, i, _)| i).sum();
                    let removed: u64 = selected.iter().map(|(_, _, d)| d).sum();
                    let parts = [
                        (format!("+{added}"), theme::diff_addition_foreground()),
                        (" / ".to_owned(), theme::muted_foreground()),
                        (format!("-{removed}"), theme::diff_deletion_foreground()),
                    ];
                    let total: f32 = parts
                        .iter()
                        .map(|(t, _)| widgets::text_width(ui, t, mono.clone()))
                        .sum();
                    let mut x = cx + cw - total;
                    for (text, color) in &parts {
                        let w = widgets::text_width(ui, text, mono.clone());
                        widgets::text_in(
                            ui,
                            Rect::from_min_size(Pos2::new(x, cy), Vec2::new(w + 1.0, 20.0)),
                            text,
                            mono.clone(),
                            *color,
                        );
                        x += w;
                    }
                }
                y = card.bottom() + 16.0;
                widgets::text(
                    ui,
                    Pos2::new(left, y),
                    "Commit message (optional)",
                    font(14.0, Weight::Medium),
                    theme::foreground(),
                    20.0,
                );
                y += 24.0;
                let field = Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner, 64.0));
                ui.painter().rect(
                    field,
                    CornerRadius::same(8),
                    theme::overlay(0.03),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let mut editor = ui.new_child(
                    egui::UiBuilder::new().max_rect(field.shrink2(Vec2::new(10.0, 6.0))),
                );
                editor.add(
                    egui::TextEdit::multiline(&mut dialog.message)
                        .id(egui::Id::new("commit-message"))
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .font(font(14.0, Weight::Regular))
                        .hint_text(
                            egui::RichText::new("Leave empty to auto-generate")
                                .font(font(14.0, Weight::Regular))
                                .color(theme::muted_foreground()),
                        )
                        .desired_width(field.width() - 20.0)
                        .desired_rows(2),
                );
                // Footer: Cancel, Commit on new branch, Commit.
                let footer_y = popup.bottom() - 16.0 - 28.0;
                let mut right = popup.right() - 24.0;
                let commit_width =
                    widgets::text_width(ui, "Commit", font(14.0, Weight::Medium)) + 24.0;
                let commit_rect = Rect::from_min_size(
                    Pos2::new(right - commit_width, footer_y),
                    Vec2::new(commit_width, 28.0),
                );
                if dialog_button(
                    ui,
                    commit_rect,
                    "commit-run",
                    "Commit",
                    true,
                    !none_selected,
                ) {
                    outcome = Some(false);
                }
                right = commit_rect.left() - 8.0;
                let branch_label = "Commit on new branch";
                let branch_width =
                    widgets::text_width(ui, branch_label, font(14.0, Weight::Medium)) + 24.0;
                let branch_rect = Rect::from_min_size(
                    Pos2::new(right - branch_width, footer_y),
                    Vec2::new(branch_width, 28.0),
                );
                if dialog_button(
                    ui,
                    branch_rect,
                    "commit-branch",
                    branch_label,
                    false,
                    !none_selected,
                ) {
                    outcome = Some(true);
                }
                right = branch_rect.left() - 8.0;
                let cancel_width =
                    widgets::text_width(ui, "Cancel", font(14.0, Weight::Medium)) + 24.0;
                let cancel_rect = Rect::from_min_size(
                    Pos2::new(right - cancel_width, footer_y),
                    Vec2::new(cancel_width, 28.0),
                );
                let cancel = dialog_button(ui, cancel_rect, "commit-cancel", "Cancel", false, true);
                let escape = ui.input(|i| i.key_pressed(Key::Escape));
                let outside = backdrop_response.clicked() && !popup_response.hovered();
                if cancel || escape || outside || close {
                    cancelled = true;
                }
            });
        if let Some(path) = open_file
            && let (Some(cwd), Some(editor)) = (self.workspace_cwd(), self.preferred_editor())
        {
            self.open_in_editor(&format!("{}/{path}", cwd.trim_end_matches('/')), editor);
        }
        match outcome {
            Some(feature_branch) => {
                let message = dialog.message.trim().to_owned();
                self.run_git_action(Run {
                    action: "commit".into(),
                    message: (!message.is_empty()).then_some(message),
                    files: (!all_selected)
                        .then(|| selected.iter().map(|(p, ..)| p.clone()).collect()),
                    feature_branch,
                    skip_default_prompt: feature_branch,
                });
            }
            None if cancelled => {}
            None => self.git.commit = Some(dialog),
        }
    }

    fn default_branch_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(pending) = self.git.pending_default.take() else {
            return;
        };
        let t = terminology(self.git_status());
        let (title, description, continue_label) = default_branch_copy(
            &pending.run.action,
            &pending.branch,
            pending.includes_commit,
            t,
        );
        let mut decision: Option<Option<bool>> = None;
        egui::Area::new(egui::Id::new("git-default-branch-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, backdrop_response) =
                    ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(backdrop, 0.0, Color32::from_black_alpha(170));
                let width = 576.0; // max-w-xl
                let body = widgets::galley(
                    ui,
                    &description,
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                    width - 48.0,
                );
                let heading = widgets::galley(
                    ui,
                    &title,
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                    width - 48.0,
                );
                let height =
                    24.0 + heading.size().y + 8.0 + body.size().y + 24.0 + 16.0 + 28.0 + 16.0;
                let popup = Rect::from_center_size(backdrop.center(), Vec2::new(width, height));
                let popup_response =
                    ui.interact(popup, egui::Id::new("git-default-popup"), Sense::click());
                paint_dialog(ui, popup);
                let close = dialog_close(ui, popup, "default-close");
                let heading_height = heading.size().y;
                ui.painter().galley(
                    popup.min + Vec2::new(24.0, 24.0),
                    heading,
                    theme::foreground(),
                );
                ui.painter().galley(
                    popup.min + Vec2::new(24.0, 24.0 + heading_height + 8.0),
                    body,
                    theme::muted_foreground(),
                );
                let footer_y = popup.bottom() - 16.0 - 28.0;
                let feature_label = "Check out feature branch & continue";
                let feature_width =
                    widgets::text_width(ui, feature_label, font(14.0, Weight::Medium)) + 24.0;
                let feature_rect = Rect::from_min_size(
                    Pos2::new(popup.right() - 24.0 - feature_width, footer_y),
                    Vec2::new(feature_width, 28.0),
                );
                if dialog_button(
                    ui,
                    feature_rect,
                    "default-feature",
                    feature_label,
                    true,
                    true,
                ) {
                    decision = Some(Some(true));
                }
                let continue_width =
                    widgets::text_width(ui, &continue_label, font(14.0, Weight::Medium)) + 24.0;
                let continue_rect = Rect::from_min_size(
                    Pos2::new(feature_rect.left() - 8.0 - continue_width, footer_y),
                    Vec2::new(continue_width, 28.0),
                );
                if dialog_button(
                    ui,
                    continue_rect,
                    "default-continue",
                    &continue_label,
                    false,
                    true,
                ) {
                    decision = Some(Some(false));
                }
                let abort_width =
                    widgets::text_width(ui, "Abort", font(14.0, Weight::Medium)) + 24.0;
                let abort_rect = Rect::from_min_size(
                    Pos2::new(popup.left() + 24.0, footer_y),
                    Vec2::new(abort_width, 28.0),
                );
                let abort = dialog_button(ui, abort_rect, "default-abort", "Abort", false, true);
                let outside = backdrop_response.clicked() && !popup_response.hovered();
                if abort || outside || close || ui.input(|i| i.key_pressed(Key::Escape)) {
                    decision = Some(None);
                }
            });
        match decision {
            None => self.git.pending_default = Some(pending),
            Some(None) => {}
            Some(Some(feature_branch)) => self.run_git_action(Run {
                feature_branch,
                skip_default_prompt: true,
                ..pending.run
            }),
        }
    }
}

/// `DialogPopup`: rounded-2xl, the popover surface, a soft shadow.
fn paint_dialog(ui: &Ui, rect: Rect) {
    ui.painter().add(
        egui::epaint::Shadow {
            offset: [0, 16],
            blur: 40,
            spread: 0,
            color: Color32::from_black_alpha(90),
        }
        .as_shape(rect, CornerRadius::same(16)),
    );
    ui.painter().rect(
        rect,
        CornerRadius::same(16),
        theme::popover(),
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );
}

/// `DialogClose` (`absolute end-2 top-2`, a ghost icon button).
fn dialog_close(ui: &mut Ui, popup: Rect, id: &str) -> bool {
    let rect = Rect::from_min_size(
        Pos2::new(popup.right() - 8.0 - 28.0, popup.top() + 8.0),
        Vec2::splat(28.0),
    );
    widgets::icon_button(ui, rect, id, "x", 16.0, theme::muted_foreground(), "Close").clicked()
}

/// A dialog footer button (`size="sm"`): filled when `primary`, else
/// outlined; dimmed and inert when disabled.
fn dialog_button(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    label: &str,
    primary: bool,
    enabled: bool,
) -> bool {
    let response = ui.interact(
        rect,
        egui::Id::new(id),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = enabled && response.hovered();
    let (fill, text) = if primary {
        (
            if hovered {
                theme::primary_hover()
            } else {
                theme::primary()
            },
            theme::primary_foreground(),
        )
    } else {
        (
            if hovered {
                theme::overlay(0.06)
            } else {
                theme::control_surface()
            },
            theme::foreground(),
        )
    };
    let alpha = if enabled { 1.0 } else { 0.64 };
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        theme::alpha(fill, alpha),
        if primary {
            Stroke::NONE
        } else {
            Stroke::new(1.0, theme::alpha(theme::input(), alpha))
        },
        StrokeKind::Inside,
    );
    let width = widgets::text_width(ui, label, font(14.0, Weight::Medium));
    widgets::text_in(
        ui,
        Rect::from_center_size(rect.center(), Vec2::new(width + 1.0, rect.height())),
        label,
        font(14.0, Weight::Medium),
        theme::alpha(text, alpha),
    );
    response.clicked()
}

/// `Checkbox`: checked (primary with a tick), unchecked, or indeterminate
/// (None); returns a click.
fn checkbox(
    ui: &mut Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    state: Option<bool>,
) -> bool {
    let response = ui.interact(rect, egui::Id::new(("checkbox", id)), Sense::click());
    let radius = CornerRadius::same(4);
    match state {
        Some(true) => {
            ui.painter().rect_filled(rect, radius, theme::primary());
            theme::paint_icon(
                ui,
                "check",
                rect.shrink(2.0),
                12.0,
                theme::primary_foreground(),
            );
        }
        Some(false) => {
            ui.painter().rect(
                rect,
                radius,
                theme::alpha(theme::input(), 0.32),
                Stroke::new(1.0, theme::input()),
                StrokeKind::Inside,
            );
        }
        None => {
            ui.painter().rect(
                rect,
                radius,
                theme::alpha(theme::input(), 0.32),
                Stroke::new(1.0, theme::input()),
                StrokeKind::Inside,
            );
            ui.painter().rect_filled(
                Rect::from_center_size(rect.center(), Vec2::new(8.0, 1.5)),
                0.0,
                theme::foreground(),
            );
        }
    }
    response.clicked()
}

/// `StartTruncatedPath`: "…" then the end of a path that does not fit.
fn start_truncated(ui: &Ui, path: &str, font: &egui::FontId, width: f32) -> String {
    if widgets::text_width(ui, path, font.clone()) <= width {
        return path.to_owned();
    }
    let chars: Vec<char> = path.chars().collect();
    for start in 1..chars.len() {
        let shown: String = std::iter::once('…')
            .chain(chars[start..].iter().copied())
            .collect();
        if widgets::text_width(ui, &shown, font.clone()) <= width {
            return shown;
        }
    }
    "…".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(fields: Value) -> Value {
        let mut base = json!({"isRepo": true, "refName": "feature", "hasWorkingTreeChanges": false,
            "hasUpstream": true, "aheadCount": 0, "behindCount": 0, "pr": null});
        for (key, value) in fields.as_object().unwrap() {
            base[key] = value.clone();
        }
        base
    }

    #[test]
    fn disabled_items_say_why() {
        let reasons = |s: Value| -> Vec<Option<String>> {
            build_menu_items(Some(&s), false, true)
                .iter()
                .map(|entry| menu_disabled_reason(entry, Some(&s), false, true))
                .collect()
        };
        assert_eq!(
            reasons(status(json!({}))),
            vec![
                Some("Worktree is clean. Make changes before committing.".into()),
                Some("No local commits to push.".into()),
                Some("No local commits to include in a pull request.".into()),
            ]
        );
        assert_eq!(
            reasons(status(json!({"refName": null, "aheadCount": 1})))[1..],
            [
                Some("Detached HEAD: check out a branch before pushing.".into()),
                Some("Detached HEAD: check out a branch before creating a pull request.".into()),
            ]
        );
        assert_eq!(
            reasons(status(json!({"behindCount": 1, "aheadCount": 1})))[1],
            Some("Branch is behind upstream. Pull/rebase before pushing.".into())
        );
        let entry = &build_menu_items(Some(&status(json!({}))), true, true)[0];
        assert_eq!(
            menu_disabled_reason(entry, None, true, true).as_deref(),
            Some("Git action in progress.")
        );
    }

    #[test]
    fn quick_actions_follow_the_status() {
        let quick = |s: Value, default: bool| resolve_quick_action(Some(&s), false, default, true);
        assert_eq!(
            quick(status(json!({"hasWorkingTreeChanges": true})), false).kind,
            QuickKind::RunAction("commit_push_pr")
        );
        assert_eq!(
            quick(status(json!({"hasWorkingTreeChanges": true})), true).label,
            "Commit & push"
        );
        assert_eq!(
            quick(status(json!({"behindCount": 2})), false).kind,
            QuickKind::RunPull
        );
        assert_eq!(
            quick(status(json!({"aheadCount": 1})), false).label,
            "Push & create PR"
        );
        assert!(quick(status(json!({"aheadCount": 1, "behindCount": 1})), false).disabled);
        let gitlab = status(json!({"aheadCount": 1, "sourceControlProvider": {"kind": "gitlab"}}));
        assert_eq!(quick(gitlab, false).label, "Push & create MR");
        assert_eq!(
            resolve_quick_action(None, false, false, true).kind,
            QuickKind::ShowHint("Git status is unavailable.".into())
        );
    }

    #[test]
    fn menu_items_enable_what_the_status_allows() {
        let items = build_menu_items(Some(&status(json!({"aheadCount": 2}))), false, true);
        let enabled: Vec<_> = items.iter().filter(|i| !i.disabled).map(|i| i.id).collect();
        assert_eq!(enabled, ["push", "pr"]);
        let local = build_menu_items(
            Some(&status(json!({"hasWorkingTreeChanges": true}))),
            false,
            false,
        );
        assert_eq!(local.len(), 1);
        assert!(!local[0].disabled);
    }

    #[test]
    fn progress_stages_list_each_step() {
        let t = terminology(None);
        assert_eq!(
            progress_stages("commit_push_pr", false, true, false, false, t),
            [
                "Generating commit message...",
                "Committing...",
                "Pushing...",
                "Preparing PR...",
                "Generating PR content...",
                "Creating pull request..."
            ]
        );
        assert_eq!(
            progress_stages("push", false, false, false, false, t),
            ["Pushing..."]
        );
        let (title, _, label) = default_branch_copy("commit_push", "main", true, t);
        assert_eq!(title, "Commit & push to default ref?");
        assert_eq!(label, "Commit & push to main");
    }
}
