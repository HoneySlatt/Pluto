//! Client state for orchestration protocol 2: the shell (projects and thread
//! summaries) and the selected thread's projection.
//!
//! Shell rows are presented like upstream `client-runtime/state/models.ts`
//! (`presentThreadShell`) and kept like `state/shellReducer.ts`; the open
//! thread's projection takes events like `state/orchestrationV2Projection.ts`,
//! with the turn item visibility of `shared/orchestrationV2Timeline.ts`.
//! Threads open on a bounded snapshot (the recent turns) and page in older
//! turns on demand, like `state/threads.ts` and `state/threadHistoryMerge.ts`;
//! the `throughEntryId` jump of find and citations is not ported.
use std::sync::Arc;

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelSelection {
    pub instance_id: String,
    pub model: String,
    pub options: Option<Value>,
}

/// `OrchestrationProjectShell`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub id: String,
    pub title: String,
    pub workspace_root: String,
    pub project_icon: Option<Value>,
    pub favicon_path: Option<String>,
    pub default_model_selection: Option<Value>,
    /// Where new threads start (`ThreadEnvMode`); `None` defers to t3.json,
    /// then the global setting.
    pub default_thread_env_mode: Option<String>,
    pub auto_pull: Option<bool>,
    /// `RepositoryIdentity` (canonical key, root path, names), kept as sent.
    pub repository_identity: Option<Value>,
    pub scripts: Vec<Value>,
    pub created_at: String,
    pub updated_at: String,
}

/// `OrchestrationV2AppThreadLineage`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Lineage {
    pub parent_thread_id: Option<String>,
    /// `fork` or `subagent`.
    pub relationship_to_parent: Option<String>,
    pub root_thread_id: String,
}

/// `OrchestrationV2PendingRuntimeRequestSummary`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PendingRequestSummary {
    pub id: String,
    pub kind: String,
    pub created_at: String,
}

/// `OrchestrationV2LatestVisibleMessageSummary`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct LatestVisibleMessage {
    pub id: String,
    pub role: String,
    pub text: String,
    pub updated_at: String,
}

/// `ThreadRunSummary`: the thread's latest run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunSummary {
    pub run_id: String,
    pub status: String,
    pub requested_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub assistant_message_id: Option<String>,
}

/// `ThreadRuntimeSummary`: what the thread's provider is doing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuntimeSummary {
    /// `idle` or a run status.
    pub status: String,
    pub active_run_id: Option<String>,
    /// The activity-owning run's work start; `None` when the server predates it.
    pub activity_started_at: Option<Option<String>>,
    pub provider_instance_id: String,
    pub last_error: Option<String>,
    pub last_error_class: Option<String>,
    pub usage_limit_reset_at: Option<String>,
    pub updated_at: String,
}

impl RuntimeSummary {
    /// `threadRuntimeIsActive`.
    pub fn is_active(&self) -> bool {
        run_status_is_active(&self.status)
    }

    /// `threadRuntimeCanArchive`: queued work may go, a working provider stays.
    pub fn can_archive(&self) -> bool {
        if self.status == "queued" {
            return self.active_run_id.is_none();
        }
        !matches!(self.status.as_str(), "preparing" | "starting" | "running")
    }
}

/// `threadRunStatusIsActive`.
pub fn run_status_is_active(status: &str) -> bool {
    matches!(
        status,
        "preparing" | "queued" | "starting" | "running" | "waiting"
    )
}

fn terminal_run_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "interrupted" | "failed" | "cancelled" | "rolled_back"
    )
}

/// Distinguishes an absent field (`None`) from a null one (`Some(None)`).
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

/// A shell thread (`EnvironmentThreadShell`), presented from the server's
/// `OrchestrationV2ThreadShell` by [`Thread::from_shell`].
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Thread {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub created_by: String,
    pub creation_source: String,
    pub provider_instance_id: String,
    pub model_selection: ModelSelection,
    pub runtime_mode: String,
    pub interaction_mode: String,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    pub lineage: Lineage,
    pub forked_from: Option<Value>,
    pub active_provider_thread_id: Option<String>,
    /// `idle` or the latest run's status.
    pub status: String,
    pub latest_run_id: Option<String>,
    pub active_run_id: Option<String>,
    #[serde(deserialize_with = "present")]
    pub activity_run_started_at: Option<Option<String>>,
    #[serde(deserialize_with = "present")]
    pub activity_run_status: Option<Option<String>>,
    pub pending_runtime_request: Option<PendingRequestSummary>,
    pub latest_visible_message: Option<LatestVisibleMessage>,
    pub latest_user_message_at: Option<String>,
    /// The last message the user wrote; `None` when the server predates it.
    #[serde(deserialize_with = "present")]
    pub latest_user_authored_message_at: Option<Option<String>>,
    pub has_actionable_proposed_plan: bool,
    /// `OrchestrationV2PendingBackgroundTask`s, kept as sent.
    pub pending_background_tasks: Vec<Value>,
    pub provider_instance_history: Vec<String>,
    pub goal: Option<Value>,
    pub item_count: u64,
    pub visible_item_count: u64,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub settled_override: Option<String>,
    pub settled_at: Option<String>,
    pub unsettled_at: Option<String>,
    pub snoozed_until: Option<String>,
    pub snoozed_at: Option<String>,
    pub limit_recovery: Option<Value>,
    pub pinned_at: Option<String>,
    pub auto_settle_disabled_at: Option<String>,
    pub pin_order_key: Option<String>,
    pub active_order_key: Option<String>,
    /// `ThreadPullRequestLink`s (`threadPullRequestsOf`).
    pub pull_requests: Vec<Value>,
    /// `ThreadLinkedPullRequest` the user linked, and the one the server
    /// found for the thread's branch.
    pub linked_pull_request: Option<Value>,
    pub branch_pull_request: Option<Value>,
    /// Server-tracked visited watermark; `None` when the server predates it.
    #[serde(deserialize_with = "present")]
    pub last_visited_at: Option<Option<String>>,
    pub title_regeneration: Option<Value>,
    pub deleted_at: Option<String>,
    #[serde(skip)]
    pub latest_run: Option<RunSummary>,
    #[serde(skip)]
    pub runtime: Option<RuntimeSummary>,
    #[serde(skip)]
    pub has_pending_approvals: bool,
    #[serde(skip)]
    pub has_pending_user_input: bool,
    /// What a live subagent below this thread waits on the user for
    /// (`approval` or `user_input`): subagent rows are hidden, so the threads
    /// above them carry it.
    #[serde(skip)]
    pub pending_subagent_request: Option<String>,
}

impl Thread {
    /// `presentThreadShell`.
    pub fn from_shell(raw: &Value) -> Result<Self, String> {
        let mut thread: Thread = decode(raw)?;
        let pull_requests_sent = raw.get("pullRequests").is_some_and(|v| !v.is_null());
        if !pull_requests_sent && let Some(linked) = thread.linked_pull_request.clone() {
            thread.pull_requests = vec![legacy_link(&linked)];
        }
        if thread.lineage.relationship_to_parent.as_deref() == Some("subagent") {
            thread.title = subagent_display_title(&thread.title);
        }
        thread.latest_run = thread.latest_run_id.clone().map(|run_id| {
            let status = if thread.status == "idle" {
                "completed".to_owned()
            } else {
                thread.status.clone()
            };
            let completed_at = match raw.get("latestRunCompletedAt") {
                None => (thread.status == "idle" || terminal_run_status(&thread.status))
                    .then(|| thread.updated_at.clone()),
                Some(value) => value.as_str().map(str::to_owned),
            };
            RunSummary {
                run_id,
                status,
                requested_at: raw["latestRunRequestedAt"].as_str().map(str::to_owned),
                started_at: raw["latestRunStartedAt"].as_str().map(str::to_owned),
                completed_at,
                assistant_message_id: None,
            }
        });
        thread.runtime = thread.shell_runtime(raw);
        let kind = thread
            .pending_runtime_request
            .as_ref()
            .map(|r| r.kind.as_str());
        thread.has_pending_approvals =
            kind.is_some_and(|k| k != "user_input" && k != "auth_refresh");
        thread.has_pending_user_input = kind == Some("user_input");
        Ok(thread)
    }

    /// `shellRuntime`: a post-settlement background roster that wakes the
    /// agent parks the runtime at idle, unless the latest run failed.
    fn shell_runtime(&self, raw: &Value) -> Option<RuntimeSummary> {
        let park_at_idle = self
            .pending_background_tasks
            .iter()
            .any(|task| task["kind"] != "command")
            && self.status != "failed";
        if self.latest_run_id.is_none() && self.active_provider_thread_id.is_none() && !park_at_idle
        {
            return None;
        }
        let status = if park_at_idle {
            "idle".to_owned()
        } else {
            self.activity_run_status
                .clone()
                .flatten()
                .unwrap_or_else(|| self.status.clone())
        };
        Some(RuntimeSummary {
            status,
            active_run_id: self.active_run_id.clone(),
            activity_started_at: self.activity_run_started_at.clone(),
            provider_instance_id: self.provider_instance_id.clone(),
            last_error: raw["lastError"].as_str().map(str::to_owned),
            last_error_class: raw["lastErrorClass"].as_str().map(str::to_owned),
            usage_limit_reset_at: raw["usageLimitResetAt"].as_str().map(str::to_owned),
            updated_at: self.updated_at.clone(),
        })
    }

    /// Whether a provider turn is under way (`threadRuntimeIsActive`).
    pub fn is_running(&self) -> bool {
        self.runtime.as_ref().is_some_and(RuntimeSummary::is_active)
    }

    /// `isLatestRunSettled`.
    pub fn latest_run_settled(&self) -> bool {
        let Some(run) = &self.latest_run else {
            return false;
        };
        !run_status_is_active(&run.status)
            && self.runtime.as_ref().and_then(|r| r.active_run_id.as_ref()) != Some(&run.run_id)
    }

    pub fn is_subagent(&self) -> bool {
        self.lineage.relationship_to_parent.as_deref() == Some("subagent")
    }

    /// `isProviderNativeSubagentThread`: the provider owns its conversation.
    pub fn is_provider_native_subagent(&self) -> bool {
        self.is_subagent() && self.creation_source == "provider"
    }

    /// `isChildRunLive` in `threadShell.ts`.
    fn child_run_live(&self) -> bool {
        match &self.activity_run_status {
            Some(status) => status.is_some(),
            None => matches!(
                self.status.as_str(),
                "preparing" | "starting" | "running" | "waiting"
            ),
        }
    }

    /// `resolveThreadWorkingStartedAt`: shell and detail timers count from
    /// the activity-owning run's work start.
    pub fn working_started_at(&self) -> Option<String> {
        let valid = |value: Option<&String>| value.filter(|v| parse_iso(v)).cloned();
        let runtime = self.runtime.as_ref();
        if let Some(started) = runtime.and_then(|r| r.activity_started_at.as_ref()) {
            return valid(started.as_ref());
        }
        let run = self.latest_run.as_ref()?;
        let owns = run.completed_at.is_none()
            && runtime.and_then(|r| r.active_run_id.as_deref()) == Some(run.run_id.as_str());
        if owns {
            valid(run.started_at.as_ref()).or_else(|| valid(run.requested_at.as_ref()))
        } else {
            None
        }
    }
}

fn parse_iso(value: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(value).is_ok()
}

/// `formatSubagentDisplayTitle`: Codex task paths read as names.
pub fn subagent_display_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let display = if lower.starts_with("subagent:") {
        title["subagent:".len()..].trim_start()
    } else {
        title
    };
    let Some(path) = display.strip_prefix("/root/") else {
        return display.to_owned();
    };
    let path = path.strip_suffix('/').unwrap_or(path);
    let Some(name) = path.rsplit('/').next().filter(|n| !n.is_empty()) else {
        return display.to_owned();
    };
    let words: Vec<String> = name
        .split(|c: char| c == '_' || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        })
        .collect();
    if words.is_empty() {
        display.to_owned()
    } else {
        words.join(" ")
    }
}

/// The link `threadPullRequestsOf` builds from a pre-links server's
/// `linkedPullRequest`.
fn legacy_link(linked: &Value) -> Value {
    let mut link = linked.clone();
    link["source"] = json!("manual");
    link["linkedAt"] = json!("1970-01-01T00:00:00.000Z");
    link["snapshot"] = Value::Null;
    link["stack"] = Value::Null;
    link
}

/// `OrchestrationV2Run`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Run {
    pub id: String,
    pub thread_id: String,
    pub ordinal: u64,
    pub provider_instance_id: String,
    pub model_selection: ModelSelection,
    pub provider_thread_id: Option<String>,
    pub user_message_id: String,
    pub root_node_id: Option<String>,
    pub active_attempt_id: Option<String>,
    pub status: String,
    pub queue_position: Option<u64>,
    pub queue_held: Option<bool>,
    pub requested_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub checkpoint_id: Option<String>,
    pub work_started_at: Option<String>,
    pub source_plan_ref: Option<Value>,
    pub restart_cancelled_background_work: Option<Value>,
}

/// `OrchestrationV2RunAttempt`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunAttempt {
    pub id: String,
    pub run_id: String,
    pub attempt_ordinal: u64,
    pub root_node_id: String,
    pub provider_instance_id: String,
    pub provider_thread_id: String,
    pub provider_turn_id: Option<String>,
    pub reason: String,
    pub status: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// `OrchestrationV2ExecutionNode`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Node {
    pub id: String,
    pub run_id: Option<String>,
    pub parent_node_id: Option<String>,
    pub root_node_id: String,
    pub kind: String,
    pub status: String,
    pub runtime_request_id: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// `OrchestrationV2Subagent`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Subagent {
    pub id: String,
    pub run_id: Option<String>,
    pub parent_node_id: String,
    pub origin: String,
    pub driver: String,
    pub provider_instance_id: String,
    pub child_thread_id: Option<String>,
    pub prompt: String,
    pub title: Option<String>,
    pub model: Option<String>,
    pub status: String,
    pub progress: Option<String>,
    /// `OrchestrationV2SubagentWorkflow`, kept as sent.
    pub workflow: Option<Value>,
    pub result: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub updated_at: String,
}

/// `OrchestrationV2RuntimeRequest`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RuntimeRequest {
    pub id: String,
    pub node_id: String,
    pub provider_turn_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub response_capability: Value,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub decision: Option<Value>,
    pub answers: Option<Value>,
}

/// `OrchestrationV2PlanArtifact`: a proposed plan or a todo list.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Plan {
    pub id: String,
    pub run_id: Option<String>,
    pub node_id: String,
    /// `draft`, `active`, `completed` or `superseded`.
    pub status: String,
    /// `proposed_plan` or `todo_list`.
    pub kind: String,
    pub markdown: String,
    pub steps: Vec<PlanStep>,
    pub explanation: Option<String>,
}

/// `OrchestrationV2PlanStep`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanStep {
    pub id: String,
    pub text: String,
    /// `pending`, `running` or `completed`.
    pub status: String,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CheckpointFile {
    pub path: String,
    pub kind: String,
    pub additions: u64,
    pub deletions: u64,
}

/// `OrchestrationV2Checkpoint`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Checkpoint {
    pub id: String,
    pub scope_id: String,
    pub run_id: Option<String>,
    pub node_id: String,
    pub ordinal_within_scope: u64,
    pub app_run_ordinal: Option<u64>,
    /// The hidden Git ref holding the workspace at this checkpoint.
    #[serde(rename = "ref")]
    pub git_ref: String,
    /// `ready`, `missing`, `error` or `stale`.
    pub status: String,
    pub files: Vec<CheckpointFile>,
    pub captured_at: String,
}

/// The turn item types this client knows; others are dropped like upstream's
/// forward-compatible decoding does.
const TURN_ITEM_TYPES: &[&str] = &[
    "notification",
    "user_message",
    "assistant_message",
    "reasoning",
    "proposed_plan",
    "todo_list",
    "user_input_request",
    "file_change",
    "command_execution",
    "file_search",
    "web_search",
    "approval_request",
    "checkpoint",
    "run_interrupt_request",
    "run_interrupt_result",
    "system_notice",
    "error",
    "compaction",
    "handoff",
    "fork",
    "thread_created",
    "secret_request",
    "subagent",
    "dynamic_tool",
];

/// `OrchestrationV2TurnItem`: the fields every item has, and its
/// type-specific ones kept as sent (`data`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TurnItem {
    pub id: String,
    pub thread_id: String,
    pub run_id: Option<String>,
    pub node_id: Option<String>,
    pub provider_thread_id: Option<String>,
    pub provider_turn_id: Option<String>,
    pub parent_item_id: Option<String>,
    pub ordinal: u64,
    pub status: String,
    pub title: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub updated_at: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub data: Map<String, Value>,
}

static NULL: Value = Value::Null;

impl TurnItem {
    pub fn get(&self, key: &str) -> &Value {
        self.data.get(key).unwrap_or(&NULL)
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key).as_str()
    }

    pub fn text(&self) -> &str {
        self.str("text").unwrap_or_default()
    }

    pub fn flag(&self, key: &str) -> bool {
        self.get(key).as_bool().unwrap_or(false)
    }

    pub fn is_known(&self) -> bool {
        TURN_ITEM_TYPES.contains(&self.kind.as_str())
    }
}

/// `OrchestrationV2ProjectedTurnItem`: a row of the thread's timeline.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct VisibleItem {
    pub position: u64,
    /// `local`, `inherited` (a fork's source rows) or `synthetic`.
    pub visibility: String,
    pub source_thread_id: String,
    pub source_item_id: String,
    pub item: Arc<TurnItem>,
}

/// The open thread: `OrchestrationV2ThreadProjection`. Entities the client
/// does not read field by field are kept as sent.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Detail {
    /// `OrchestrationV2AppThread`, kept as sent.
    pub thread: Value,
    pub runs: Vec<Run>,
    pub attempts: Vec<RunAttempt>,
    pub nodes: Vec<Node>,
    pub subagents: Vec<Subagent>,
    pub provider_sessions: Vec<Value>,
    pub provider_threads: Vec<Value>,
    pub provider_turns: Vec<Value>,
    pub runtime_requests: Vec<RuntimeRequest>,
    pub messages: Vec<Value>,
    pub plans: Vec<Plan>,
    pub turn_items: Vec<Arc<TurnItem>>,
    pub checkpoint_scopes: Vec<Value>,
    pub checkpoints: Vec<Checkpoint>,
    pub context_handoffs: Vec<Value>,
    pub context_transfers: Vec<Value>,
    #[serde(rename = "visibleTurnItems")]
    pub visible: Vec<VisibleItem>,
    pub updated_at: String,
    /// Progressive history of a bounded snapshot; empty for a full one.
    #[serde(skip)]
    pub history: History,
}

/// `ThreadHistoryMeta`: where older turns resume and whether there are any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    /// Opaque cursor of the next older page.
    pub cursor: Option<String>,
    pub has_more: bool,
    /// Max local turn ordinal of the full projection: a turn item update
    /// at or below it belongs to unloaded history.
    pub latest_local_ordinal: Option<u64>,
    pub loading: bool,
    pub error: Option<String>,
}

impl History {
    /// An incomplete progressive window (`partialTimeline`).
    pub fn partial(&self) -> bool {
        self.has_more || self.cursor.is_some()
    }
}

/// Runs whose items are new rather than unloaded history.
const UNFINISHED_RUN_STATUSES: [&str; 5] =
    ["preparing", "queued", "starting", "running", "waiting"];

impl Detail {
    pub fn id(&self) -> &str {
        self.thread["id"].as_str().unwrap_or_default()
    }

    fn decode(projection: &Value) -> Result<Self, String> {
        let mut detail: Detail = decode(projection)?;
        detail.turn_items.retain(|item| item.is_known());
        detail.visible.retain(|row| row.item.is_known());
        Ok(detail)
    }

    pub fn run(&self, id: &str) -> Option<&Run> {
        self.runs.iter().find(|run| run.id == id)
    }

    /// The thread as a shell row, for a thread the shell does not list (an
    /// archived one): its runtime comes from its runs.
    pub fn shell_thread(&self) -> Option<Thread> {
        let mut thread = Thread::from_shell(&self.thread).ok()?;
        let latest = self.runs.iter().max_by_key(|run| run.ordinal);
        let active = self
            .runs
            .iter()
            .filter(|run| run_status_is_active(&run.status))
            .max_by_key(|run| run.ordinal);
        thread.latest_run = latest.map(|run| RunSummary {
            run_id: run.id.clone(),
            status: run.status.clone(),
            requested_at: Some(run.requested_at.clone()),
            started_at: run.started_at.clone(),
            completed_at: run.completed_at.clone(),
            assistant_message_id: None,
        });
        thread.runtime = latest.map(|run| RuntimeSummary {
            status: active.map_or("idle", |run| run.status.as_str()).to_owned(),
            active_run_id: active.map(|run| run.id.clone()),
            activity_started_at: None,
            provider_instance_id: run.provider_instance_id.clone(),
            updated_at: thread.updated_at.clone(),
            ..Default::default()
        });
        Some(thread)
    }

    /// `applyOrchestrationV2ProjectionEvent` for one event of this thread.
    fn apply(&mut self, event: &Value) -> Result<(), String> {
        let payload = &event["payload"];
        let kind = event["type"].as_str().unwrap_or_default();
        let occurred_at = event["occurredAt"].as_str().unwrap_or_default().to_owned();
        match kind {
            "thread.pull-request-link-updated" => {
                let links = if self.thread["pullRequests"].is_array() {
                    self.thread["pullRequests"].clone()
                } else {
                    match self
                        .thread
                        .get("linkedPullRequest")
                        .filter(|v| !v.is_null())
                    {
                        Some(linked) => json!([legacy_link(linked)]),
                        None => json!([]),
                    }
                };
                let key = |link: &Value| {
                    (
                        link["host"].as_str().unwrap_or_default().to_lowercase(),
                        link["repository"]
                            .as_str()
                            .unwrap_or_default()
                            .to_lowercase(),
                        link["number"].as_u64(),
                    )
                };
                let links: Vec<Value> = links
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|link| {
                        if key(link) == key(payload) {
                            payload.clone()
                        } else {
                            link.clone()
                        }
                    })
                    .collect();
                self.thread["pullRequests"] = json!(links);
            }
            "thread.visited" | "thread.marked-unread" => {
                self.thread = payload.clone();
                return Ok(());
            }
            "thread.created"
            | "thread.archived"
            | "thread.unarchived"
            | "thread.deleted"
            | "thread.settled"
            | "thread.unsettled"
            | "thread.snoozed"
            | "thread.unsnoozed"
            | "thread.auto-settle-set"
            | "thread.pinned"
            | "thread.unpinned"
            | "thread.pin-reordered"
            | "thread.active-reordered"
            | "thread.metadata-updated"
            | "thread.pull-request-synced"
            | "thread.runtime-mode-updated"
            | "thread.interaction-mode-updated"
            | "thread.model-selection-updated"
            | "thread.provider-switched" => self.thread = payload.clone(),
            "run.created" | "run.updated" => {
                upsert(&mut self.runs, decode(payload)?, |r| &r.id);
                self.reconcile_visible();
            }
            "run.background-work-cancelled" => {
                if let Some(run) = self.runs.iter_mut().find(|r| r.id == payload["runId"]) {
                    run.restart_cancelled_background_work =
                        Some(payload["restartCancelledBackgroundWork"].clone());
                }
            }
            "run-attempt.created" | "run-attempt.updated" => {
                upsert(&mut self.attempts, decode(payload)?, |a| &a.id);
                self.reconcile_visible();
            }
            "node.updated" => upsert(&mut self.nodes, decode(payload)?, |n| &n.id),
            "subagent.updated" => upsert(&mut self.subagents, decode(payload)?, |s| &s.id),
            "provider-session.attached" | "provider-session.updated" => {
                upsert_value(&mut self.provider_sessions, payload.clone());
            }
            "provider-session.detached" => self
                .provider_sessions
                .retain(|s| s["id"] != payload["providerSessionId"]),
            "provider-thread.updated" => upsert_value(&mut self.provider_threads, payload.clone()),
            "provider-turn.updated" => {
                let mut turn = payload.clone();
                // Token usage stays until a later update carries its own.
                if turn.get("tokenUsage").is_none_or(Value::is_null)
                    && let Some(usage) = self
                        .provider_turns
                        .iter()
                        .find(|t| t["id"] == payload["id"])
                        .and_then(|t| t.get("tokenUsage"))
                        .filter(|u| !u.is_null())
                {
                    turn["tokenUsage"] = usage.clone();
                }
                upsert_value(&mut self.provider_turns, turn);
            }
            "runtime-request.updated" => {
                upsert(&mut self.runtime_requests, decode(payload)?, |r| &r.id);
            }
            "message.updated" => upsert_value(&mut self.messages, payload.clone()),
            "plan.updated" => upsert(&mut self.plans, decode(payload)?, |p| &p.id),
            "turn-item.updated" => {
                let item: TurnItem = decode(payload)?;
                if !item.is_known() {
                    return Ok(());
                }
                let partial = self.history.partial();
                // An update of an item outside the loaded window is
                // history, not a new row at the end.
                if partial
                    && !self.turn_items.iter().any(|i| i.id == item.id)
                    && self.drop_missing_partial_item(&item)
                {
                    return Ok(());
                }
                let ordinal = item.ordinal;
                let item = Arc::new(item);
                let was_interrupt = self
                    .turn_items
                    .iter()
                    .find(|i| i.id == item.id)
                    .is_some_and(|i| i.kind == "run_interrupt_request");
                upsert(&mut self.turn_items, item.clone(), |i| &i.id);
                // Only interrupt requests change another item's visibility.
                if item.kind == "run_interrupt_request" || was_interrupt {
                    self.reconcile_visible();
                }
                if self.item_visible(&item) {
                    self.upsert_visible(item);
                } else if let Some(index) = self
                    .visible
                    .iter()
                    .position(|row| row.source_item_id == item.id)
                {
                    self.visible.remove(index);
                    renumber(&mut self.visible);
                }
                if partial
                    && self
                        .history
                        .latest_local_ordinal
                        .is_none_or(|w| ordinal > w)
                {
                    self.history.latest_local_ordinal = Some(ordinal);
                }
            }
            "checkpoint-scope.created" => {
                upsert_value(&mut self.checkpoint_scopes, payload.clone());
            }
            "checkpoint.captured" => upsert(&mut self.checkpoints, decode(payload)?, |c| &c.id),
            "context-handoff.updated" => upsert_value(&mut self.context_handoffs, payload.clone()),
            "context-transfer.created" | "context-transfer.updated" => {
                upsert_value(&mut self.context_transfers, payload.clone());
            }
            // `checkpoint.rollback-requested` and event types this client
            // does not know change nothing but the projection's time.
            _ => {}
        }
        self.updated_at = occurred_at;
        Ok(())
    }

    /// `isOrchestrationV2TurnItemVisible`.
    fn item_visible(&self, item: &TurnItem) -> bool {
        let status = item
            .run_id
            .as_deref()
            .and_then(|id| self.run(id))
            .map(|run| run.status.as_str());
        if status == Some("rolled_back") {
            return false;
        }
        // A queued message whose run was cancelled never reached the provider.
        if status == Some("cancelled")
            && item.kind == "user_message"
            && item.str("inputIntent") == Some("queued_turn")
        {
            return false;
        }
        !self.superseded_interrupt(item)
    }

    /// `isOrchestrationV2SupersededInterrupt`: a plain steer's interrupt
    /// result stays hidden; a stop-then-steer pair stays visible.
    fn superseded_interrupt(&self, item: &TurnItem) -> bool {
        let (Some(run_id), Some(node_id)) = (&item.run_id, &item.node_id) else {
            return false;
        };
        if item.kind != "run_interrupt_result" {
            return false;
        }
        let superseded = self
            .attempts
            .iter()
            .any(|a| &a.run_id == run_id && &a.root_node_id == node_id && a.status == "superseded");
        superseded
            && !self.turn_items.iter().any(|candidate| {
                candidate.kind == "run_interrupt_request"
                    && candidate.run_id.as_ref() == Some(run_id)
            })
    }

    /// `activeVisibleTurnItems`: drops local rows that run or attempt state
    /// now hides.
    fn reconcile_visible(&mut self) {
        let before = self.visible.len();
        let rows = std::mem::take(&mut self.visible);
        self.visible = rows
            .into_iter()
            .filter(|row| row.visibility != "local" || self.item_visible(&row.item))
            .collect();
        if self.visible.len() != before {
            renumber(&mut self.visible);
        }
    }

    /// `shouldDropMissingPartialTurnItem`.
    fn drop_missing_partial_item(&self, item: &TurnItem) -> bool {
        if self.visible.iter().any(|row| row.source_item_id == item.id) {
            return false;
        }
        let Some(run_id) = &item.run_id else {
            return self.below_partial_window(item);
        };
        // A queued run keeps the ordinal it got when queued, so a steer sent
        // later can raise the watermark past it. Items of a run that has not
        // finished are new, not unloaded history.
        if self
            .runs
            .iter()
            .any(|run| &run.id == run_id && UNFINISHED_RUN_STATUSES.contains(&run.status.as_str()))
        {
            return false;
        }
        // Later items of a run already in the window can raise the watermark first.
        if self.visible.iter().any(|row| {
            row.visibility == "local"
                && row.item.run_id.as_ref() == Some(run_id)
                && row.item.ordinal <= item.ordinal
        }) {
            return false;
        }
        self.below_partial_window(item)
    }

    /// `isBelowPartialWindow`.
    fn below_partial_window(&self, item: &TurnItem) -> bool {
        if self
            .history
            .latest_local_ordinal
            .is_some_and(|w| item.ordinal <= w)
        {
            return true;
        }
        let oldest = self
            .visible
            .iter()
            .filter(|row| row.visibility == "local")
            .map(|row| row.item.ordinal)
            .min();
        oldest.is_some_and(|oldest| item.ordinal < oldest)
    }

    /// `mergeOlderHistoryIntoProjection`: an older page goes before the
    /// rows already loaded, deduplicated by source; live values win and
    /// hidden live items stay hidden.
    fn merge_older(&mut self, older: Vec<VisibleItem>) -> bool {
        let key = |row: &VisibleItem| (row.source_thread_id.clone(), row.source_item_id.clone());
        let current: std::collections::HashMap<_, _> =
            self.visible.iter().map(|row| (key(row), row)).collect();
        let thread_id = self.id().to_owned();
        let mut seen = std::collections::HashSet::new();
        let mut page = Vec::new();
        let mut added = Vec::new();
        for page_row in older {
            let row_key = key(&page_row);
            if seen.contains(&row_key) {
                continue;
            }
            let mut row = current.get(&row_key).map_or(page_row, |row| (*row).clone());
            let local = row.visibility == "local" || row.source_thread_id == thread_id;
            if !current.contains_key(&row_key) && local {
                let known = self.turn_items.iter().find(|i| i.id == row.source_item_id);
                // Cold snapshots retain interrupt requests outside the
                // window; paging may reveal those requests.
                if let Some(known) = known {
                    if known.kind != "run_interrupt_request" || !self.item_visible(known) {
                        continue;
                    }
                    row.item = known.clone();
                }
            }
            if local && !self.turn_items.iter().any(|i| i.id == row.source_item_id) {
                added.push(row.item.clone());
            }
            seen.insert(row_key);
            page.push(row);
        }
        if page.is_empty() {
            return false;
        }
        let rest = std::mem::take(&mut self.visible)
            .into_iter()
            .filter(|row| !seen.contains(&key(row)));
        page.extend(rest);
        self.visible = page;
        renumber(&mut self.visible);
        for item in added {
            if !self.turn_items.iter().any(|i| i.id == item.id) {
                self.turn_items.push(item);
            }
        }
        true
    }

    /// `upsertVisibleTurnItem`: a local row by ordinal, then id.
    fn upsert_visible(&mut self, item: Arc<TurnItem>) {
        let index = self
            .visible
            .iter()
            .position(|r| r.source_item_id == item.id);
        if index.is_none() && self.history.partial() && self.drop_missing_partial_item(&item) {
            return;
        }
        if let Some(index) = index {
            self.visible.remove(index);
        }
        let at = self
            .visible
            .iter()
            .position(|row| {
                row.visibility == "local"
                    && (row.item.ordinal > item.ordinal
                        || (row.item.ordinal == item.ordinal && row.item.id > item.id))
            })
            .unwrap_or(self.visible.len());
        self.visible.insert(
            at,
            VisibleItem {
                position: 0,
                visibility: "local".into(),
                source_thread_id: item.thread_id.clone(),
                source_item_id: item.id.clone(),
                item,
            },
        );
        renumber(&mut self.visible);
    }
}

fn renumber(rows: &mut [VisibleItem]) {
    for (position, row) in rows.iter_mut().enumerate() {
        row.position = position as u64;
    }
}

#[derive(Default)]
pub struct State {
    pub projects: Vec<Project>,
    /// Active shell threads. Shared so views can hold the list without
    /// copying it.
    pub threads: Vec<Arc<Thread>>,
    /// The open thread's projection. Shared so a frame can hold it without
    /// copying it.
    pub detail: Option<Arc<Detail>>,
    pub config: Value,
    /// Counts changes to `threads` and `detail`, for views that cache what
    /// they derive from them.
    pub revision: u64,
    /// Threads left in the last five minutes (projection, sequence, when
    /// left): one opened again shows at once while its new subscription
    /// catches up (`threadRetention.ts` `THREAD_SNAPSHOT_IDLE_TTL_MS`).
    recent: Vec<(Arc<Detail>, Option<u64>, std::time::Instant)>,
    shell_sequence: Option<u64>,
    thread_sequence: Option<u64>,
}

fn decode<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, String> {
    T::deserialize(value).map_err(|e| e.to_string())
}

impl State {
    pub fn project(&self, id: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn config_event(&mut self, item: &Value) {
        match item["type"].as_str() {
            Some("snapshot") => self.config = item["config"].clone(),
            Some("providerStatuses") => {
                self.config["providers"] = item["payload"]["providers"].clone()
            }
            Some("settingsUpdated") => {
                self.config["settings"] = item["payload"]["settings"].clone()
            }
            // Themes this environment publishes (`t3 theme set`).
            Some("environmentThemesUpdated") => {
                self.config["environmentThemes"] = item["payload"]["themes"].clone()
            }
            Some("keybindingsUpdated") => {
                if let Some(bindings) = item["payload"].get("keybindings") {
                    self.config["keybindings"] = bindings.clone();
                }
            }
            _ => {}
        }
    }

    /// Applies an `OrchestrationV2ShellStreamItem` (`shellReducer.ts`).
    pub fn shell(&mut self, item: &Value) -> Result<(), String> {
        match item["kind"].as_str() {
            Some("synchronized") => return Ok(()),
            Some("snapshot") => {
                let snapshot = &item["snapshot"];
                // An enrichment refresh only patches repository identities.
                if let Some(roots) = item.get("resolvedRepositoryIdentityRoots") {
                    let roots: Vec<&str> = roots
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect();
                    let next: Vec<Project> = decode(&snapshot["projects"])?;
                    for project in &mut self.projects {
                        let Some(candidate) = next.iter().find(|c| {
                            c.id == project.id && c.workspace_root == project.workspace_root
                        }) else {
                            continue;
                        };
                        if roots.contains(&project.workspace_root.as_str())
                            || (project.repository_identity.is_none()
                                && candidate.repository_identity.is_some())
                        {
                            project.repository_identity = candidate.repository_identity.clone();
                        }
                    }
                    self.revision += 1;
                    return Ok(());
                }
                // Decode both collections before replacing visible state.
                let mut projects: Vec<Project> = decode(&snapshot["projects"])?;
                for project in &mut projects {
                    retain_identity(&self.projects, project);
                }
                let threads = snapshot["threads"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(Thread::from_shell)
                    .collect::<Result<Vec<_>, _>>()?;
                self.projects = projects;
                self.threads = threads.into_iter().map(Arc::new).collect();
                self.shell_sequence = snapshot["snapshotSequence"].as_u64();
                self.refresh_subagent_requests();
                self.revision += 1;
                return Ok(());
            }
            _ => {}
        }
        let sequence = item["sequence"]
            .as_u64()
            .ok_or("Missing shell event sequence")?;
        if self.shell_sequence.is_some_and(|s| sequence <= s) {
            return Ok(());
        }
        match item["kind"].as_str() {
            Some("project.updated") => {
                let mut project: Project = decode(&item["project"])?;
                retain_identity(&self.projects, &mut project);
                upsert(&mut self.projects, project, |p| &p.id);
            }
            Some("project.removed") => self.projects.retain(|p| p.id != item["projectId"]),
            Some("thread.updated") => {
                let id = item["thread"]["id"].as_str().unwrap_or_default();
                if item["location"] == "active" {
                    upsert(
                        &mut self.threads,
                        Arc::new(Thread::from_shell(&item["thread"])?),
                        |t| &t.id,
                    );
                } else {
                    // The archive has its own subscription.
                    self.threads.retain(|t| t.id != id);
                }
                self.refresh_subagent_requests();
            }
            Some("thread.removed") => {
                self.recent
                    .retain(|(detail, ..)| detail.id() != item["threadId"]);
                self.threads.retain(|t| t.id != item["threadId"]);
                self.refresh_subagent_requests();
            }
            _ => {}
        }
        self.shell_sequence = Some(sequence);
        self.revision += 1;
        Ok(())
    }

    /// `pendingSubagentRequestsByThread`: every thread above a subagent that
    /// waits on the user carries what it waits on; an approval outranks a
    /// question.
    fn refresh_subagent_requests(&mut self) {
        let mut requests: std::collections::HashMap<String, &'static str> = Default::default();
        for child in &self.threads {
            let kind = child
                .pending_runtime_request
                .as_ref()
                .map(|r| r.kind.as_str());
            let request = match kind {
                None | Some("auth_refresh") => continue,
                Some("user_input") => "user_input",
                Some(_) => "approval",
            };
            if !child.is_subagent()
                || child.is_provider_native_subagent()
                || !child.child_run_live()
            {
                continue;
            }
            let mut seen = vec![child.id.clone()];
            let mut current = Some(child.clone());
            while let Some(thread) = current.filter(|t| t.is_subagent()) {
                let Some(parent) = thread.lineage.parent_thread_id.clone() else {
                    break;
                };
                if seen.contains(&parent) {
                    break;
                }
                seen.push(parent.clone());
                if requests.get(&parent) != Some(&"approval") {
                    requests.insert(parent.clone(), request);
                }
                current = self.threads.iter().find(|t| t.id == parent).cloned();
            }
        }
        for thread in &mut self.threads {
            let request = requests.get(&thread.id).map(|r| (*r).to_owned());
            if thread.pending_subagent_request != request {
                Arc::make_mut(thread).pending_subagent_request = request;
            }
        }
    }

    pub fn clear_detail(&mut self) {
        self.detail = None;
        self.thread_sequence = None;
        self.revision += 1;
    }

    /// Leaves the open thread for `id`, keeping the one left among the
    /// recent threads and showing `id` from them if it is there.
    pub fn switch_detail(&mut self, id: Option<&str>) {
        const IDLE_TTL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
        self.recent.retain(|(.., left)| left.elapsed() < IDLE_TTL);
        if let Some(detail) = self.detail.take() {
            self.recent.retain(|(kept, ..)| kept.id() != detail.id());
            let left = std::time::Instant::now();
            self.recent.push((detail, self.thread_sequence, left));
        }
        self.clear_detail();
        let found = id.and_then(|id| self.recent.iter().position(|(kept, ..)| kept.id() == id));
        if let Some(index) = found {
            let (detail, sequence, _) = self.recent.remove(index);
            self.detail = Some(detail);
            self.thread_sequence = sequence;
        }
    }

    /// Applies an `OrchestrationV2ThreadStreamItem` of the selected thread.
    pub fn thread(&mut self, selected: &str, item: &Value) -> Result<(), String> {
        match item["kind"].as_str() {
            Some("synchronized") => return Ok(()),
            Some("snapshot") => {
                let projection = &item["projection"];
                if projection["thread"]["id"] == selected {
                    let mut detail = Detail::decode(projection)?;
                    // A bounded snapshot carries its cursor; a full one
                    // replaces progressive state.
                    detail.history = History {
                        cursor: item["historyCursor"].as_str().map(str::to_owned),
                        has_more: item["hasMoreHistory"].as_bool().unwrap_or(false),
                        latest_local_ordinal: item["latestLocalTurnOrdinal"].as_u64(),
                        loading: false,
                        error: None,
                    };
                    self.detail = Some(Arc::new(detail));
                    self.revision += 1;
                    self.thread_sequence = item["snapshotSequence"].as_u64();
                }
                return Ok(());
            }
            _ => {}
        }
        let sequence = item["sequence"]
            .as_u64()
            .ok_or("Missing thread event sequence")?;
        if self.thread_sequence.is_some_and(|s| sequence <= s) {
            return Ok(());
        }
        self.thread_sequence = Some(sequence);
        let event = &item["event"];
        let Some(detail) = self.detail.as_mut().filter(|d| d.id() == selected) else {
            return Ok(());
        };
        if event["threadId"] != selected {
            return Ok(());
        }
        Arc::make_mut(detail).apply(event)?;
        self.revision += 1;
        Ok(())
    }

    /// Starts loading older turns of the open thread: the cursor to request,
    /// or `None` when there is nothing to load or a load is under way.
    pub fn begin_history_load(&mut self, thread_id: &str) -> Option<String> {
        let detail = self.detail.as_mut().filter(|d| d.id() == thread_id)?;
        let history = &detail.history;
        if !history.has_more || history.loading {
            return None;
        }
        let cursor = history.cursor.clone()?;
        let history = &mut Arc::make_mut(detail).history;
        history.loading = true;
        history.error = None;
        self.revision += 1;
        Some(cursor)
    }

    /// Merges an `OrchestrationV2ThreadHistoryPage`, unless a newer snapshot
    /// replaced the cursor that requested it.
    pub fn history_page(
        &mut self,
        thread_id: &str,
        cursor: &str,
        page: &Value,
    ) -> Result<(), String> {
        let Some(detail) = self
            .detail
            .as_mut()
            .filter(|d| d.id() == thread_id && d.history.cursor.as_deref() == Some(cursor))
        else {
            return Ok(());
        };
        let rows: Vec<VisibleItem> = decode(&page["items"])?;
        let detail = Arc::make_mut(detail);
        detail.merge_older(rows.into_iter().filter(|row| row.item.is_known()).collect());
        detail.history.cursor = page["nextCursor"].as_str().map(str::to_owned);
        detail.history.has_more = page["hasMoreHistory"].as_bool().unwrap_or(false);
        detail.history.loading = false;
        detail.history.error = None;
        self.revision += 1;
        Ok(())
    }

    pub fn history_failed(&mut self, thread_id: &str, cursor: &str, message: String) {
        let Some(detail) = self
            .detail
            .as_mut()
            .filter(|d| d.id() == thread_id && d.history.cursor.as_deref() == Some(cursor))
        else {
            return;
        };
        let history = &mut Arc::make_mut(detail).history;
        history.loading = false;
        history.error = Some(message);
        self.revision += 1;
    }
}

/// `retainRepositoryIdentity`: enrichment is asynchronous, so a project
/// update without an identity keeps the one already resolved for its root.
fn retain_identity(previous: &[Project], next: &mut Project) {
    if next.repository_identity.is_none()
        && let Some(prior) = previous.iter().find(|p| p.id == next.id)
        && prior.repository_identity.is_some()
        && prior.workspace_root == next.workspace_root
    {
        next.repository_identity = prior.repository_identity.clone();
    }
}

fn upsert<T>(items: &mut Vec<T>, value: T, id: impl Fn(&T) -> &str) {
    if let Some(index) = items.iter().position(|old| id(old) == id(&value)) {
        items[index] = value;
    } else {
        items.push(value);
    }
}

fn upsert_value(items: &mut Vec<Value>, value: Value) {
    if let Some(index) = items.iter().position(|old| old["id"] == value["id"]) {
        items[index] = value;
    } else {
        items.push(value);
    }
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// A V2 command for `thread_id` (`OrchestrationV2Command`).
pub fn command(kind: &str, thread_id: &str) -> Value {
    json!({"type":kind,"commandId":uuid::Uuid::new_v4().to_string(),"threadId":thread_id})
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: &str = "2026-01-01T00:00:00.000Z";

    fn item(id: &str, kind: &str, ordinal: u64, run: Option<&str>, extra: Value) -> Value {
        let mut item = json!({"id":id,"threadId":"t","runId":run,"nodeId":null,
            "providerThreadId":null,"providerTurnId":null,"nativeItemRef":null,
            "parentItemId":null,"ordinal":ordinal,"status":"completed","title":null,
            "startedAt":AT,"completedAt":AT,"updatedAt":AT,"type":kind});
        for (key, value) in extra.as_object().unwrap() {
            item[key] = value.clone();
        }
        item
    }

    fn run(id: &str, status: &str) -> Value {
        json!({"id":id,"threadId":"t","ordinal":1,"providerInstanceId":"codex",
            "modelSelection":{"instanceId":"codex","model":"gpt"},"providerThreadId":null,
            "userMessageId":"m","rootNodeId":null,"activeAttemptId":null,"status":status,
            "requestedAt":AT,"startedAt":null,"completedAt":null,"checkpointId":null,
            "contextHandoffId":null})
    }

    fn detail() -> State {
        let mut state = State::default();
        let snapshot = json!({"kind":"snapshot","snapshotSequence":1,"projection":{
            "thread":{"id":"t","projectId":"p","title":"T"},"runs":[],"attempts":[],"nodes":[],
            "subagents":[],"providerSessions":[],"providerThreads":[],"providerTurns":[],
            "runtimeRequests":[],"messages":[],"plans":[],"turnItems":[],"checkpointScopes":[],
            "checkpoints":[],"contextHandoffs":[],"contextTransfers":[],"visibleTurnItems":[],
            "updatedAt":AT}});
        state.thread("t", &snapshot).unwrap();
        state
    }

    fn event(sequence: u64, kind: &str, payload: Value) -> Value {
        json!({"kind":"event","sequence":sequence,"event":{"id":format!("e{sequence}"),
            "threadId":"t","type":kind,"occurredAt":AT,"payload":payload}})
    }

    fn visible(state: &State) -> Vec<String> {
        let detail = state.detail.as_ref().unwrap();
        detail.visible.iter().map(|r| r.item.id.clone()).collect()
    }

    /// A bounded snapshot whose window holds `c` (ordinal 3, run `r2`)
    /// while the full projection's newest local item has ordinal 3.
    fn bounded() -> State {
        let mut state = State::default();
        let text = json!({"messageId":"m","text":"hi","streaming":false});
        let c = item("c", "assistant_message", 3, Some("r2"), text);
        let snapshot = json!({"kind":"snapshot","snapshotSequence":1,"projection":{
            "thread":{"id":"t","projectId":"p","title":"T"},
            "runs":[run("r1", "completed"), run("r2", "completed")],"attempts":[],"nodes":[],
            "subagents":[],"providerSessions":[],"providerThreads":[],"providerTurns":[],
            "runtimeRequests":[],"messages":[],"plans":[],"turnItems":[c],"checkpointScopes":[],
            "checkpoints":[],"contextHandoffs":[],"contextTransfers":[],
            "visibleTurnItems":[{"position":0,"visibility":"local","sourceThreadId":"t",
                "sourceItemId":"c","item":c}],
            "updatedAt":AT},
            "historyCursor":"cursor-1","hasMoreHistory":true,"latestLocalTurnOrdinal":3});
        state.thread("t", &snapshot).unwrap();
        state
    }

    fn row(item: Value) -> Value {
        json!({"position":0,"visibility":"local","sourceThreadId":"t",
            "sourceItemId":item["id"],"item":item})
    }

    #[test]
    fn a_bounded_window_keeps_unloaded_history_out_of_its_end() {
        let mut state = bounded();
        let text = json!({"messageId":"m","text":"old","streaming":false});
        // An update of an older, finished run's item is unloaded history.
        state
            .thread(
                "t",
                &event(
                    2,
                    "turn-item.updated",
                    item("a", "assistant_message", 1, Some("r1"), text.clone()),
                ),
            )
            .unwrap();
        assert_eq!(visible(&state), ["c"]);
        // A new item lands at the end and raises the watermark.
        state
            .thread(
                "t",
                &event(
                    3,
                    "turn-item.updated",
                    item("d", "assistant_message", 4, None, text.clone()),
                ),
            )
            .unwrap();
        assert_eq!(visible(&state), ["c", "d"]);
        let detail = state.detail.as_ref().unwrap();
        assert_eq!(detail.history.latest_local_ordinal, Some(4));
        // A running run's item is new even below the watermark.
        state
            .thread("t", &event(4, "run.created", run("r3", "running")))
            .unwrap();
        state
            .thread(
                "t",
                &event(
                    5,
                    "turn-item.updated",
                    item("e", "assistant_message", 2, Some("r3"), text),
                ),
            )
            .unwrap();
        assert_eq!(visible(&state), ["e", "c", "d"]);
    }

    #[test]
    fn an_older_page_goes_before_the_loaded_window() {
        let mut state = bounded();
        assert_eq!(state.begin_history_load("t").as_deref(), Some("cursor-1"));
        // One load at a time.
        assert_eq!(state.begin_history_load("t"), None);
        let text = json!({"messageId":"m","text":"old","streaming":false});
        let page = json!({"snapshotSequence":1,"nextCursor":null,"hasMoreHistory":false,"items":[
            row(item("a", "user_message", 1, Some("r1"), json!({"messageId":"u","text":"q"}))),
            row(item("b", "assistant_message", 2, Some("r1"), text)),
            row(item("c", "assistant_message", 3, Some("r2"), json!({"messageId":"m","text":"stale"}))),
        ]});
        // A page for a replaced cursor changes nothing.
        state.history_page("t", "cursor-0", &page).unwrap();
        assert_eq!(visible(&state), ["c"]);
        state.history_page("t", "cursor-1", &page).unwrap();
        assert_eq!(visible(&state), ["a", "b", "c"]);
        let detail = state.detail.as_ref().unwrap();
        // The live row wins over the page's copy.
        assert_eq!(detail.visible[2].item.text(), "hi");
        assert_eq!(detail.visible[2].position, 2);
        assert_eq!(detail.turn_items.len(), 3);
        assert!(!detail.history.partial());
        assert!(!detail.history.loading);
    }

    #[test]
    fn a_failed_load_can_be_retried() {
        let mut state = bounded();
        let cursor = state.begin_history_load("t").unwrap();
        state.history_failed("t", &cursor, "The server answered 500.".into());
        let history = &state.detail.as_ref().unwrap().history;
        assert_eq!(history.error.as_deref(), Some("The server answered 500."));
        assert_eq!(state.begin_history_load("t").as_deref(), Some("cursor-1"));
        assert_eq!(state.detail.as_ref().unwrap().history.error, None);
    }

    #[test]
    fn turn_items_take_their_ordinal_place() {
        let mut state = detail();
        let text = json!({"messageId":"m","text":"hi","streaming":false});
        state
            .thread(
                "t",
                &event(
                    2,
                    "turn-item.updated",
                    item("b", "assistant_message", 2, None, text.clone()),
                ),
            )
            .unwrap();
        state
            .thread(
                "t",
                &event(
                    3,
                    "turn-item.updated",
                    item("a", "assistant_message", 1, None, text.clone()),
                ),
            )
            .unwrap();
        // A streamed update replaces the row in place.
        let more = json!({"messageId":"m","text":"hello","streaming":true});
        state
            .thread(
                "t",
                &event(
                    4,
                    "turn-item.updated",
                    item("b", "assistant_message", 2, None, more),
                ),
            )
            .unwrap();
        assert_eq!(visible(&state), ["a", "b"]);
        let detail = state.detail.as_ref().unwrap();
        assert_eq!(detail.visible[1].item.text(), "hello");
        assert_eq!(detail.visible[1].position, 1);
    }

    #[test]
    fn rolled_back_runs_hide_their_items() {
        let mut state = detail();
        state
            .thread("t", &event(2, "run.created", run("r", "running")))
            .unwrap();
        let text = json!({"messageId":"m","text":"hi","streaming":false});
        state
            .thread(
                "t",
                &event(
                    3,
                    "turn-item.updated",
                    item("a", "assistant_message", 1, Some("r"), text),
                ),
            )
            .unwrap();
        assert_eq!(visible(&state), ["a"]);
        state
            .thread("t", &event(4, "run.updated", run("r", "rolled_back")))
            .unwrap();
        assert!(visible(&state).is_empty());
    }

    #[test]
    fn unknown_items_and_stale_sequences_are_skipped() {
        let mut state = detail();
        state
            .thread(
                "t",
                &event(
                    3,
                    "turn-item.updated",
                    item("x", "hologram", 1, None, json!({})),
                ),
            )
            .unwrap();
        let text = json!({"messageId":"m","text":"late","streaming":false});
        state
            .thread(
                "t",
                &event(
                    2,
                    "turn-item.updated",
                    item("a", "assistant_message", 1, None, text),
                ),
            )
            .unwrap();
        assert!(visible(&state).is_empty());
    }

    #[test]
    fn reopened_threads_show_from_the_recent_ones() {
        let mut state = detail();
        state.switch_detail(Some("other"));
        assert!(state.detail.is_none());
        state.switch_detail(Some("t"));
        assert_eq!(state.detail.as_ref().unwrap().id(), "t");
        // Its events still apply after the kept sequence.
        let renamed = json!({"id":"t","projectId":"p","title":"New"});
        state
            .thread("t", &event(2, "thread.metadata-updated", renamed))
            .unwrap();
        assert_eq!(state.detail.as_ref().unwrap().thread["title"], "New");
    }

    fn shell_thread(id: &str, extra: Value) -> Value {
        let mut thread = json!({"id":id,"projectId":"p","title":"T","providerInstanceId":"codex",
            "modelSelection":{"instanceId":"codex","model":"gpt"},"runtimeMode":"full-access",
            "interactionMode":"default","branch":null,"worktreePath":null,
            "lineage":{"parentThreadId":null,"relationshipToParent":null,"rootThreadId":id},
            "forkedFrom":null,"activeProviderThreadId":null,"latestRunId":null,"activeRunId":null,
            "status":"idle","pendingRuntimeRequest":null,"latestVisibleMessage":null,
            "latestUserMessageAt":null,"hasActionableProposedPlan":false,"itemCount":0,
            "visibleItemCount":0,"createdAt":AT,"updatedAt":AT,"archivedAt":null,
            "settledOverride":null,"settledAt":null,"deletedAt":null});
        for (key, value) in extra.as_object().unwrap() {
            thread[key] = value.clone();
        }
        thread
    }

    #[test]
    fn shell_rows_present_runs_and_requests() {
        let running = shell_thread(
            "a",
            json!({"latestRunId":"r","activeRunId":"r","status":"running",
                "activeProviderThreadId":"pt","latestRunStartedAt":AT,
                "pendingRuntimeRequest":{"id":"q","kind":"command","createdAt":AT}}),
        );
        let thread = Thread::from_shell(&running).unwrap();
        assert!(thread.is_running());
        assert!(thread.has_pending_approvals);
        assert_eq!(thread.latest_run.as_ref().unwrap().completed_at, None);
        assert_eq!(thread.working_started_at().as_deref(), Some(AT));

        let idle = shell_thread("b", json!({"latestRunId":"r","status":"idle"}));
        let thread = Thread::from_shell(&idle).unwrap();
        assert!(!thread.is_running());
        let run = thread.latest_run.unwrap();
        assert_eq!(run.status, "completed");
        assert_eq!(run.completed_at.as_deref(), Some(AT));
    }

    #[test]
    fn subagent_requests_reach_their_ancestors() {
        let mut state = State::default();
        let child = shell_thread(
            "c",
            json!({"status":"running","creationSource":"mcp",
                "lineage":{"parentThreadId":"p1","relationshipToParent":"subagent","rootThreadId":"p1"},
                "pendingRuntimeRequest":{"id":"q","kind":"user_input","createdAt":AT}}),
        );
        let shell = json!({"kind":"snapshot","snapshot":{"snapshotSequence":1,"projects":[],
            "threads":[shell_thread("p1", json!({})), child],"archivedThreads":[]}});
        state.shell(&shell).unwrap();
        assert_eq!(
            state.threads[0].pending_subagent_request.as_deref(),
            Some("user_input")
        );
        // Archiving moves a row out of the shell.
        let archived = json!({"kind":"thread.updated","sequence":2,"location":"archive",
            "thread":shell_thread("p1", json!({}))});
        state.shell(&archived).unwrap();
        assert_eq!(state.threads.len(), 1);
    }

    #[test]
    fn subagent_titles_read_as_names() {
        assert_eq!(
            subagent_display_title("Subagent: /root/code_review/"),
            "Code Review"
        );
        assert_eq!(
            subagent_display_title("Explore the repo"),
            "Explore the repo"
        );
    }
}
