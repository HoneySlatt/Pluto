//! Timeline rows for the chat. Entries come from the thread's visible turn
//! items like upstream `session-logic.ts` (`deriveTimelineEntriesFromVisibleTurnItems`,
//! `projectedWorkEntry`); rows fold settled runs and group tool calls like
//! `MessagesTimeline.logic.ts` (`deriveTurnFolds`, `deriveMessagesTimelineRows`).
//! Pages published with `html_render` show where the call happened and stay
//! out of folds (`kind: "html-render"`).
//! Lifecycle items (`V2LifecycleRow.tsx`) are event rows, drawn by
//! `ui/lifecycle.rs`; adjacent subagents of a provider turn share one, and
//! subagents fold with their turn once none of the batch works.
//! A working turn's trailing calls become one live row (`work-live`, drawn
//! by `ui/live_activity.rs`), or "Thinking" (`thinking`).
//! Compactions are dividers (`context-compaction`), and the output of an
//! attempt a steer cut off folds behind `attempt-fold`.
//! A secret request is a lifecycle event row, drawn by the client as its
//! card.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Value, json};

use crate::model::{CheckpointFile, Detail, RunAttempt, RunSummary, Thread, TurnItem};

/// `ChatMessage`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Message {
    pub id: String,
    /// `user` or `assistant`.
    pub role: String,
    pub text: String,
    /// `ChatAttachment`s (an open union, kept as sent).
    pub attachments: Vec<Value>,
    /// `OrchestrationMessageContext` (`{version, records}`), kept as sent.
    pub context: Value,
    pub run_id: Option<String>,
    pub streaming: bool,
    pub created_at: String,
    pub updated_at: String,
    /// `turn_start`, `queued_turn`, `steer` or `promoted_queued_to_steer`.
    pub input_intent: Option<String>,
    /// Who sent a user message the user did not type
    /// (`resolveUserMessagePresentation`): `automation`, `agent` or `t3code`;
    /// with the task or the sending thread.
    pub attribution: Option<String>,
    pub scheduled_task_id: Option<String>,
    pub sender_thread_id: Option<String>,
    /// The turn item's status (`failed`, `interrupted`, `completed`…).
    pub status: String,
}

/// `resolveUserMessagePresentation`: the sender of a user message, and its
/// text without an older scheduled message's prefix.
fn user_message_presentation(
    id: &str,
    text: &str,
    item: &TurnItem,
) -> (String, Option<String>, Option<String>) {
    if let Some(task) = item.str("scheduledTaskId") {
        return (
            text.to_owned(),
            Some("automation".into()),
            Some(task.to_owned()),
        );
    }
    let agent = item.str("createdBy") == Some("agent");
    // `LEGACY_AUTOMATION_PREFIX`: "[Triggered by schedule task: …]" and a blank line.
    let legacy = text
        .strip_prefix("[Triggered by schedule task: ")
        .and_then(|rest| {
            let end = rest.find(']')?;
            let after = &rest[end + 1..];
            after
                .strip_prefix("\r\n\r\n")
                .or_else(|| after.strip_prefix("\n\n"))
                .filter(|_| !rest[..end].contains('\n'))
        });
    let legacy_task = id
        .strip_prefix("scheduled-task-message:")
        .and_then(|rest| rest.rsplit_once(':'))
        .filter(|(_, mode)| matches!(*mode, "scheduled" | "manual"))
        .and_then(|(rest, _)| rest.rsplit_once(':'))
        .filter(|(_, n)| n.chars().all(|c| c.is_ascii_digit()))
        .map(|(task, _)| task.to_owned());
    if let Some(body) = legacy
        && (legacy_task.is_some() || agent)
    {
        return (body.to_owned(), Some("automation".into()), legacy_task);
    }
    let attribution = agent.then(|| {
        if item.str("creationSource") == Some("server") {
            "t3code".to_owned()
        } else {
            "agent".to_owned()
        }
    });
    (text.to_owned(), attribution, None)
}

/// `ProposedPlan`: a plan a plan-mode run proposed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProposedPlan {
    pub id: String,
    pub run_id: Option<String>,
    pub plan_markdown: String,
    /// `draft`, `active`, `completed` or `superseded`.
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// `WorkLogEntry`.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkEntry {
    pub id: String,
    pub created_at: String,
    pub run_id: Option<String>,
    pub label: String,
    /// `thinking`, `tool`, `info` or `error`.
    pub tone: String,
    pub detail: Option<String>,
    pub command: Option<String>,
    pub item_type: Option<String>,
    pub changed_files: Vec<String>,
    /// `toolLifecycleStatus`: `idle`, `inProgress`, `completed`, `failed`,
    /// `declined` or `stopped`.
    pub status: Option<String>,
    /// `sourceActivityKind` (`runtime.warning`, `runtime.error`,
    /// `context-compaction`), or `event` for a lifecycle item shown as work.
    pub source_kind: String,
    pub question: Option<QuestionAnswer>,
    pub item: Option<Arc<TurnItem>>,
}

#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Message(Message),
    Plan(ProposedPlan),
    HtmlRender(HtmlRenderEntry),
    McpApp(McpAppEntry),
    Work(WorkEntry),
}

/// An MCP app a tool call returned, where the call happened.
#[derive(Clone, Debug, PartialEq)]
pub struct McpAppEntry {
    pub id: String,
    pub run_id: Option<String>,
    pub created_at: String,
    /// The thread that produced the app, which its requests run against.
    pub source_thread_id: String,
    pub item: Arc<TurnItem>,
    pub app: crate::mcp_app::McpApp,
}

/// A page an agent published, where its call happened.
#[derive(Clone, Debug, PartialEq)]
pub struct HtmlRenderEntry {
    pub id: String,
    pub run_id: Option<String>,
    pub created_at: String,
    pub render: crate::html_render::HtmlRender,
}

impl Entry {
    fn id(&self) -> &str {
        match self {
            Entry::Message(m) => &m.id,
            Entry::Plan(p) => &p.id,
            Entry::HtmlRender(h) => &h.id,
            Entry::McpApp(a) => &a.id,
            Entry::Work(w) => &w.id,
        }
    }

    fn created_at(&self) -> &str {
        match self {
            Entry::Message(m) => &m.created_at,
            Entry::Plan(p) => &p.created_at,
            Entry::HtmlRender(h) => &h.created_at,
            Entry::McpApp(a) => &a.created_at,
            Entry::Work(w) => &w.created_at,
        }
    }

    /// `timelineEntryRunId`.
    fn run_id(&self) -> Option<&str> {
        match self {
            Entry::Message(m) if m.role == "assistant" => m.run_id.as_deref(),
            Entry::Message(_) => None,
            Entry::Plan(p) => p.run_id.as_deref(),
            Entry::HtmlRender(h) => h.run_id.as_deref(),
            Entry::McpApp(a) => a.run_id.as_deref(),
            Entry::Work(w) => w.run_id.as_deref(),
        }
    }

    /// `timelineEntryStartsResponse`: a prompt or a wake starts a response;
    /// a steer continues the current one.
    fn starts_response(&self) -> bool {
        match self {
            Entry::Message(m) => {
                m.role == "user"
                    && !matches!(
                        m.input_intent.as_deref(),
                        Some("steer" | "promoted_queued_to_steer")
                    )
            }
            Entry::Work(w) => w.item_type.as_deref() == Some("notification"),
            Entry::Plan(_) | Entry::HtmlRender(_) | Entry::McpApp(_) => false,
        }
    }

    /// `responseBoundaryRunId`.
    fn boundary_run_id(&self) -> Option<&str> {
        match self {
            Entry::Message(m) => m.run_id.as_deref(),
            Entry::Work(w) => w.run_id.as_deref(),
            Entry::Plan(_) | Entry::HtmlRender(_) | Entry::McpApp(_) => None,
        }
    }

    /// `timelineEntryFoldRunId`; lifecycle items stay out of folds, except
    /// linked resources and subagents.
    fn fold_run_id(&self, runless: Option<&str>) -> Option<String> {
        match self {
            Entry::Work(w) if w.item_type.as_deref() == Some("system_notice") => None,
            Entry::Work(w) if w.source_kind == "event" && !persistent_card(w) && !subagent(w) => {
                None
            }
            Entry::Message(m) if m.role == "assistant" => {
                m.run_id.as_deref().or(runless).map(str::to_owned)
            }
            Entry::Work(w) => w.run_id.as_deref().or(runless).map(str::to_owned),
            _ => None,
        }
    }

    /// `timelineEntryFailedItem`: a top-level failed provider error.
    fn failed_run(&self) -> Option<&str> {
        let Entry::Work(w) = self else {
            return None;
        };
        let item = w.item.as_ref()?;
        (item.kind == "error" && item.status == "failed" && item.parent_item_id.is_none())
            .then_some(item.run_id.as_deref())
            .flatten()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    User(Message),
    Assistant {
        message: Message,
        show_meta: bool,
        /// The run's checkpoint files (`turnDiffSummaryByAssistantMessageId`),
        /// with the run id.
        changes: Option<(String, Vec<CheckpointFile>)>,
    },
    TurnFold {
        run_id: String,
        label: String,
        expanded: bool,
        created_at: String,
    },
    /// A single tool call shown inline.
    Work {
        id: String,
        label: String,
        entry: Box<WorkEntry>,
    },
    /// A collapsed group of tool calls.
    WorkToggle {
        group_id: String,
        summary: String,
        expanded: bool,
        has_failure: bool,
        /// `toolGroupSummaryKind`, for its icon.
        kind: &'static str,
    },
    /// Rows of an expanded tool group.
    WorkDetails {
        group_id: String,
        entries: Vec<WorkEntry>,
    },
    Working {
        started_at: Option<String>,
    },
    /// The active run's trailing work as one row (`work-live`): the running
    /// tool, or the last one in past tense, with the latest thought above.
    LiveWork {
        group_id: String,
        entry: Box<WorkEntry>,
        entries: Vec<WorkEntry>,
        expanded: bool,
        active: bool,
        thought: Option<String>,
    },
    /// Output of an attempt a steer cut off, folded (`attempt-fold`).
    AttemptFold {
        attempt_id: String,
        expanded: bool,
    },
    /// A context compaction as a divider (`context-compaction`), sweeping
    /// while the active run compacts.
    Compaction {
        id: String,
        label: String,
        active: bool,
    },
    /// "Thinking" while the active turn shows nothing live (`thinking`);
    /// after a failed tool it opens that tool's group.
    Thinking {
        group_id: Option<String>,
        expanded: bool,
    },
    /// A proposed plan (`ProposedPlanCard`).
    Plan(Box<ProposedPlan>),
    /// A page an agent published (`HtmlRenderTimelineRow`).
    HtmlRender(Box<HtmlRenderEntry>),
    /// An MCP app (`McpAppTimelineRow`).
    McpApp(Box<McpAppEntry>),
    /// A lifecycle card (`V2LifecycleRow`): an interrupt, a handoff, a fork
    /// or a subagent; adjacent subagents of one provider turn share a row.
    Event {
        id: String,
        entries: Vec<WorkEntry>,
    },
    /// The worktree's setup card (`WorktreeSetupCard`); `embedded` once the
    /// agent's turn took over.
    WorktreeSetup {
        snapshot: Box<crate::worktree_setup::WorktreeSetup>,
        embedded: bool,
    },
    /// A local Codex image omitted by the server's native item projection.
    GeneratedImage {
        image: crate::generated_images::GeneratedImage,
    },
}

#[derive(Clone, Default, PartialEq)]
pub struct Expansion {
    /// Unfolded settled runs.
    pub turns: BTreeSet<String>,
    pub groups: BTreeSet<String>,
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Question/answer summary of an answered user-input request.
#[derive(Clone, Debug, PartialEq)]
pub struct QuestionAnswer {
    pub questions: Vec<String>,
    pub answers: Vec<String>,
}

fn answer_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(list) => list
            .iter()
            .map(answer_text)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(map) => map.get("answers").map(answer_text).unwrap_or_default(),
        _ => String::new(),
    }
}

/// A `user_input_request` item's `questionAnswer`, with option values shown
/// by their labels.
fn question_answer(item: &TurnItem) -> Option<QuestionAnswer> {
    let answer = item.get("questionAnswer");
    if !answer.is_object() {
        return None;
    }
    let questions = item
        .get("questions")
        .as_array()
        .cloned()
        .unwrap_or_default();
    let label = |qid: &str, text: String| {
        questions
            .iter()
            .find(|q| q["id"] == qid)
            .and_then(|q| q["options"].as_array())
            .and_then(|options| {
                options
                    .iter()
                    .find(|o| o["value"].as_str() == Some(text.as_str()))
            })
            .and_then(|o| o["label"].as_str().map(str::to_owned))
            .unwrap_or(text)
    };
    let answers = answer["answers"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(qid, value)| label(qid, answer_text(value)))
        .filter(|s| !s.is_empty())
        .collect();
    let mut texts: Vec<String> = questions
        .iter()
        .filter_map(|q| q["question"].as_str().map(str::to_owned))
        .collect();
    if texts.is_empty()
        && let Some(by_id) = answer["questionTextById"].as_object()
    {
        texts = by_id
            .values()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
    }
    Some(QuestionAnswer {
        questions: texts,
        answers,
    })
}

/// `projectedWorkEntryStatus`.
fn lifecycle_status(item: &TurnItem) -> &'static str {
    match item.status.as_str() {
        "pending" | "running" | "waiting" => "inProgress",
        "completed" => "completed",
        "idle" => "idle",
        "failed" => "failed",
        _ => "stopped",
    }
}

/// `projectedWorkEntryTone`.
fn tone(item: &TurnItem) -> &'static str {
    match item.kind.as_str() {
        "reasoning" => "thinking",
        "command_execution" | "file_change" | "file_search" | "web_search" | "dynamic_tool"
        | "subagent" | "thread_created" | "user_input_request" | "approval_request" => "tool",
        _ => "info",
    }
}

/// `formatTokens` (`shared/usageFormat.ts`).
fn format_tokens(value: u64) -> String {
    let value = value as f64;
    let trim = |v: f64| {
        let digits = if v >= 100.0 {
            0
        } else if v >= 10.0 {
            1
        } else {
            2
        };
        let text = format!("{v:.digits$}");
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            text
        }
    };
    if value >= 1e12 {
        format!("{}T", trim(value / 1e12))
    } else if value >= 1e9 {
        format!("{}B", trim(value / 1e9))
    } else if value >= 1e6 {
        format!("{}M", trim(value / 1e6))
    } else if value >= 1e3 {
        format!("{}K", trim(value / 1e3))
    } else {
        format!("{}", value.round() as u64)
    }
}

/// `providerErrorPresentation`.
fn provider_error(item: &TurnItem) -> (String, String) {
    let failure = item.get("failure");
    let message = failure["message"].as_str().unwrap_or_default().to_owned();
    let usage_limit = failure["class"] == "usage_limit";
    let retry = item.get("retry");
    if !retry.is_object() {
        let label = if usage_limit {
            "Usage limit reached".to_owned()
        } else {
            item.title
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .unwrap_or("Provider error")
                .to_owned()
        };
        return (label, message);
    }
    let attempt = retry["attempt"].as_u64().unwrap_or(0);
    let progress = match retry["maxAttempts"].as_u64() {
        Some(max) => format!("{attempt}/{max}"),
        None => attempt.to_string(),
    };
    let label = match item.status.as_str() {
        "running" => format!("Retrying provider ({progress})"),
        "completed" => format!("Provider recovered ({progress} retries)"),
        "failed" => format!(
            "{} after {progress} retries",
            if usage_limit {
                "Usage limit reached"
            } else {
                "Provider error"
            }
        ),
        _ => format!("Provider retry stopped ({progress})"),
    };
    let delay = retry["retryDelayMs"].as_u64().filter(|d| *d > 0);
    let retry_delay = match delay {
        Some(ms) if item.status == "running" && ms < 1_000 => format!(" Retrying in {ms}ms."),
        Some(ms) if item.status == "running" => {
            let seconds = format!("{:.1}", ms as f64 / 1_000.0);
            format!(" Retrying in {}s.", seconds.trim_end_matches(".0"))
        }
        _ => String::new(),
    };
    (label, format!("{message}{retry_delay}"))
}

/// `v2EventPresentation` labels for the lifecycle items upstream draws as
/// cards (`STANDALONE_V2_ITEM_TYPES`).
fn event_presentation(item: &TurnItem) -> (String, Option<String>) {
    let message = item.str("message").map(str::to_owned);
    match item.kind.as_str() {
        "run_interrupt_request" => ("Interrupt requested".into(), message),
        "run_interrupt_result" => ("Run interrupted".into(), message),
        "handoff" => {
            let from: Vec<&str> = item
                .get("fromProviderInstanceIds")
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let detail = item.str("summary").map(str::to_owned).unwrap_or_else(|| {
                format!(
                    "{} → {}",
                    from.join(", "),
                    item.str("toProviderInstanceId").unwrap_or_default()
                )
            });
            ("Context handoff".into(), Some(detail))
        }
        "fork" => {
            let label = if item.get("source")["type"] == "run" {
                "Forked from conversation"
            } else {
                "Conversation fork"
            };
            (label.into(), None)
        }
        "subagent" => (
            crate::model::subagent_display_title(item.title.as_deref().unwrap_or("Subagent")),
            item.str("progress")
                .or_else(|| item.str("result"))
                .map(str::to_owned),
        ),
        "secret_request" => (
            item.str("label").unwrap_or("Secret").to_owned(),
            item.str("reason").map(str::to_owned),
        ),
        _ => (item.kind.replace('_', " "), message),
    }
}

/// `projectedWorkEntry`.
fn work_entry(item: &Arc<TurnItem>, created_at: &str) -> WorkEntry {
    let title = item
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned);
    let mut entry = WorkEntry {
        id: item.id.clone(),
        created_at: created_at.to_owned(),
        run_id: item.run_id.clone(),
        label: String::new(),
        tone: tone(item).into(),
        detail: None,
        command: None,
        item_type: Some(item.kind.clone()),
        changed_files: Vec::new(),
        status: Some(lifecycle_status(item).into()),
        source_kind: String::new(),
        question: None,
        item: Some(item.clone()),
    };
    let or = |fallback: String| title.clone().unwrap_or(fallback);
    match item.kind.as_str() {
        "thread_created" => entry.label = "Created thread".into(),
        "compaction" => {
            entry.label = if item.status == "running" {
                "Compacting context".into()
            } else {
                match (
                    item.get("beforeTokenCount").as_u64(),
                    item.get("afterTokenCount").as_u64(),
                ) {
                    (Some(before), Some(after)) => format!(
                        "Context compacted {} → {} tokens",
                        format_tokens(before),
                        format_tokens(after)
                    ),
                    _ => "Context compacted".into(),
                }
            };
            entry.source_kind = "context-compaction".into();
            entry.detail = text(item.get("summary"));
        }
        "reasoning" => {
            entry.label = or("Thinking".into());
            entry.detail = text(item.get("text"));
        }
        "command_execution" => {
            entry.label = or("Ran command".into());
            entry.command = item.str("input").map(str::to_owned);
        }
        "file_change" => {
            let changes: Vec<String> = item
                .get("changes")
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c["path"].as_str().map(str::to_owned))
                .collect();
            let file = item.str("fileName").unwrap_or_default().to_owned();
            entry.label = or(if changes.len() > 1 {
                format!("Changed {} files", changes.len())
            } else {
                format!("Changed {file}")
            });
            entry.changed_files = if item.get("changes").is_array() {
                changes
            } else {
                vec![file]
            };
        }
        "file_search" => {
            entry.label = or(crate::tool_presentation::search_label(&Value::Object(
                item.data.clone(),
            ))
            .unwrap_or_else(|| "Searched files".into()));
            entry.detail = text(item.get("pattern"));
        }
        "web_search" => {
            entry.label = or("Searched the web".into());
            let patterns: Vec<&str> = item
                .get("patterns")
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            entry.detail = (!patterns.is_empty()).then(|| patterns.join(", "));
        }
        "checkpoint" => {
            entry.label = or("Checkpoint captured".into());
            entry.changed_files = item
                .get("files")
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f["path"].as_str().map(str::to_owned))
                .collect();
        }
        "system_notice" => {
            entry.label = item.str("message").unwrap_or_default().to_owned();
            entry.source_kind = "runtime.warning".into();
        }
        "error" => {
            let (label, detail) = provider_error(item);
            entry.label = label;
            entry.detail = Some(detail).filter(|d| !d.is_empty());
            let usage_limit = item.get("failure")["class"] == "usage_limit";
            if usage_limit && item.status != "completed" {
                entry.source_kind = "runtime.warning".into();
            } else if !item.get("retry").is_object() {
                entry.source_kind = "runtime.error".into();
            }
        }
        "dynamic_tool" => {
            let tool = item.str("toolName").map(str::to_owned);
            // Pluto's chat memory tools say what they remembered.
            let memory = tool.as_deref().and_then(|name| {
                [
                    ("memory_save", "Saved to memory"),
                    ("memory_update", "Updated a memory"),
                    ("memory_delete", "Deleted a memory"),
                ]
                .into_iter()
                .find(|(id, _)| crate::html_render::is_t3_tool(name, id))
            });
            let failed = |output: &Value| {
                output["isError"] == true
                    || output["is_error"] == true
                    || !output["error"].is_null()
            };
            match memory {
                Some((_, label)) if item.status == "completed" && !failed(item.get("output")) => {
                    entry.label = label.into();
                    entry.detail = text(&tool_arguments(item.get("input"))["text"]);
                }
                _ => {
                    use crate::tool_presentation::{Activity, classify, read_label, search_label};
                    let input = item.get("input");
                    let fallback = tool.clone().unwrap_or_else(|| "Tool call".into());
                    entry.label = or(
                        match classify(Some("dynamic_tool_call"), None, None, tool.as_deref()) {
                            Activity::Read => read_label(
                                crate::tool_presentation::collect_file_paths(
                                    &json!({"input": input}),
                                )
                                .first()
                                .map_or("", String::as_str),
                                0,
                            ),
                            Activity::Search => {
                                search_label(&json!({"input": input})).unwrap_or(fallback)
                            }
                            _ => fallback,
                        },
                    );
                }
            }
        }
        "approval_request" => {
            entry.label = or("Approval requested".into());
            entry.detail = item
                .str("prompt")
                .or_else(|| item.str("requestKind"))
                .map(str::to_owned);
        }
        "user_input_request" => {
            entry.question = question_answer(item);
            entry.label = or(if entry.question.is_some() {
                "Answered questions".into()
            } else {
                "Input requested".into()
            });
        }
        _ => entry.label = or(item.kind.replace('_', " ")),
    }
    entry
}

/// `t3ToolSummary.ts`'s `readInput`: a tool call's arguments, which some
/// adapters keep as JSON text and Cursor inside its MCP envelope.
fn tool_arguments(input: &Value) -> Value {
    let input = match input.as_str() {
        Some(raw) => serde_json::from_str(raw).unwrap_or(Value::Null),
        None => input.clone(),
    };
    if input["toolName"].is_string() {
        input["args"].clone()
    } else {
        input
    }
}

/// `turnItemIsWorkspacePreparation`.
fn workspace_preparation(item: &TurnItem) -> bool {
    (item.kind == "command_execution" && item.str("input") == Some("Preparing workspace"))
        || (item.kind == "error"
            && item.status == "cancelled"
            && item.get("failure")["code"] == "workspace_preparation_failed")
}

/// Item types upstream draws as lifecycle cards (`STANDALONE_V2_ITEM_TYPES`).
const STANDALONE: &[&str] = &[
    "fork",
    "handoff",
    "run_interrupt_request",
    "run_interrupt_result",
    "secret_request",
    "subagent",
];

/// `workEntryIndicatesToolSuccess`.
fn indicates_success(entry: &WorkEntry) -> bool {
    if !is_tool_like(entry)
        || indicates_failure(entry)
        || (entry.tone == "thinking" && entry.item_type.as_deref() != Some("reasoning"))
    {
        return false;
    }
    !matches!(
        entry.status.as_deref(),
        Some("failed" | "declined" | "inProgress" | "stopped" | "idle")
    )
}

/// `workEntryIsVisibleInGroup` for an expanded group: a thought with text,
/// a call still running, a stopped call, or one with a clear outcome.
fn visible_in_live_group(entry: &WorkEntry) -> bool {
    if entry.item_type.as_deref() == Some("reasoning") {
        return entry
            .detail
            .as_deref()
            .is_some_and(|d| !d.trim().is_empty());
    }
    let neutral = is_tool_like(entry) && !indicates_failure(entry) && !indicates_success(entry);
    matches!(entry.status.as_deref(), Some("inProgress" | "stopped"))
        || entry.source_kind == "task.progress"
        || !neutral
}

/// `workEntryIsActiveTurnActivity`.
fn active_activity(entry: &WorkEntry) -> bool {
    entry.status.as_deref() == Some("inProgress")
        || (entry.status.is_none() && (entry.source_kind == "task.progress" || is_tool_like(entry)))
}

/// `liveActivityToolStatus`: a settled call takes the live group's present
/// tense.
fn live_status(entry: &WorkEntry, present: bool) -> &str {
    match entry.status.as_deref() {
        Some(status @ ("failed" | "declined" | "stopped" | "idle")) => status,
        Some("inProgress") => "inProgress",
        _ if present => "inProgress",
        _ => "completed",
    }
}

/// `commandProgramName`: the program a command runs, past environment
/// assignments, `env` and a shell's `-c` script.
pub fn command_program(command: &str) -> Option<String> {
    let mut words = command.split_whitespace().peekable();
    while let Some(word) = words.peek() {
        let assignment = word.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        let base = word.rsplit('/').next().unwrap_or(word);
        if assignment || matches!(base, "env" | "sudo" | "exec") {
            words.next();
        } else {
            break;
        }
    }
    let program = words.next()?;
    let name = program
        .trim_matches(['"', '\''])
        .rsplit('/')
        .next()
        .unwrap_or(program);
    if matches!(name, "bash" | "sh" | "zsh" | "fish" | "dash") {
        let rest: Vec<&str> = words.collect();
        if let Some(at) = rest
            .iter()
            .position(|w| w.starts_with('-') && w.contains('c'))
        {
            let script = rest[at + 1..].join(" ");
            let script = script.trim().trim_matches(['"', '\'']);
            if let Some(inner) = command_program(script) {
                return Some(inner);
            }
        }
    }
    (!name.is_empty()).then(|| name.to_owned())
}

/// `liveWorkEntryLabel`: what the live row says the call does or did.
pub fn live_label(entry: &WorkEntry, active: bool) -> String {
    let status = live_status(entry, active);
    if entry.item_type.as_deref() == Some("reasoning") {
        let detail = entry
            .detail
            .as_deref()
            .map(|d| d.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|d| !d.is_empty());
        return detail.unwrap_or_else(|| {
            if status == "inProgress" {
                "Thinking".into()
            } else {
                "Thought".into()
            }
        });
    }
    if let Some((label, _)) = t3_presentation(entry, Some(status)) {
        return label;
    }
    if let Some(command) = entry
        .command
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        let verb = match status {
            "inProgress" => "Running",
            "failed" => "Failed",
            "declined" => "Declined",
            "stopped" => "Stopped",
            _ => "Ran",
        };
        let program = command_program(command).unwrap_or_else(|| "command".into());
        return format!("{verb} {program}");
    }
    display_label(entry)
}

/// `liveThoughtLine`: a bold-only opening line (a summary heading), else
/// the first sentence, as plain text.
pub fn live_thought_line(markdown: &str) -> String {
    let first = markdown
        .trim_start()
        .lines()
        .next()
        .unwrap_or_default()
        .trim_end();
    let heading = first
        .strip_prefix("**")
        .and_then(|rest| rest.strip_suffix("**"))
        .filter(|inner| !inner.is_empty() && !inner.contains('*'));
    let source = heading.unwrap_or(markdown);
    let mut text = String::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let unlisted = ["# ", "## ", "### ", "#### ", "- ", "* ", "+ "]
            .iter()
            .find_map(|prefix| trimmed.strip_prefix(prefix))
            .or_else(|| {
                let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
                (digits > 0)
                    .then(|| trimmed[digits..].strip_prefix(". "))
                    .flatten()
            })
            .unwrap_or(line);
        text.push_str(unlisted);
        text.push(' ');
    }
    // Links read as their labels; code ticks, bold and strikes drop.
    let mut plain = String::new();
    let mut rest = text.as_str();
    while let Some(open) = rest.find('[') {
        match (rest[open..].find("]("), rest[open..].find(')')) {
            (Some(close), Some(end)) if close < end => {
                let label_start = if rest[..open].ends_with('!') {
                    open - 1
                } else {
                    open
                };
                plain.push_str(&rest[..label_start]);
                plain.push_str(&rest[open + 1..open + close]);
                rest = &rest[open + end + 1..];
            }
            _ => {
                plain.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    plain.push_str(rest);
    let plain = plain.replace("**", "").replace("~~", "").replace('`', "");
    let plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if heading.is_some() {
        return plain;
    }
    // Cut after the first sentence end that a space follows.
    let chars: Vec<(usize, char)> = plain.char_indices().collect();
    for (i, (at, c)) in chars.iter().enumerate() {
        if matches!(c, '.' | '?' | '!') {
            let mut end = at + c.len_utf8();
            let mut next = i + 1;
            if let Some((q_at, q)) = chars.get(next)
                && matches!(q, '"' | '\'' | '”' | '’' | ')')
            {
                end = q_at + q.len_utf8();
                next += 1;
            }
            if chars.get(next).is_some_and(|(_, c)| c.is_whitespace()) {
                return plain[..end].to_owned();
            }
        }
    }
    plain
}

/// `resolveAttempt`: the attempt whose root node holds the item, walking up
/// its execution nodes.
fn resolve_attempt<'a>(detail: &'a Detail, item: &TurnItem) -> Option<&'a RunAttempt> {
    let run = item.run_id.as_deref()?;
    let by_root = |node: &str| {
        detail
            .attempts
            .iter()
            .find(|a| a.root_node_id == node && a.run_id == run)
    };
    let mut node_id = item.node_id.clone();
    let mut visited = HashSet::new();
    while let Some(id) = node_id {
        if !visited.insert(id.clone()) {
            break;
        }
        if let Some(attempt) = by_root(&id) {
            return Some(attempt);
        }
        let node = detail.nodes.iter().find(|n| n.id == id)?;
        if let Some(attempt) = by_root(&node.root_node_id) {
            return Some(attempt);
        }
        node_id = node.parent_node_id.clone();
    }
    None
}

/// Each entry's attempt and item, by entry id.
fn entry_attempts(detail: &Detail) -> HashMap<String, (&RunAttempt, &Arc<TurnItem>)> {
    detail
        .visible
        .iter()
        .filter_map(|row| {
            let item = &row.item;
            let attempt = resolve_attempt(detail, item)?;
            let id = match item.kind.as_str() {
                "user_message" | "assistant_message" => item.str("messageId")?.to_owned(),
                "proposed_plan" => item.str("planId")?.to_owned(),
                _ => item.id.clone(),
            };
            Some((id, (attempt, item)))
        })
        .collect()
}

type Attempts<'a> = HashMap<String, (&'a RunAttempt, &'a Arc<TurnItem>)>;

fn superseded<'a>(attempts: &Attempts<'a>, entry: &Entry) -> Option<&'a RunAttempt> {
    attempts
        .get(entry.id())
        .map(|(attempt, _)| *attempt)
        .filter(|a| a.status == "superseded")
}

/// `completeSupersededReplyEntryIds`: a superseded attempt's last output,
/// when it is an answer that completed before the steer replaced it.
fn complete_superseded_replies(entries: &[Entry], attempts: &Attempts) -> HashSet<String> {
    let mut last: HashMap<&str, &Entry> = HashMap::new();
    for entry in entries {
        let Some(attempt) = superseded(attempts, entry) else {
            continue;
        };
        let output = match entry {
            Entry::Message(m) => m.role != "user",
            Entry::Work(w) => !matches!(
                w.item_type.as_deref(),
                Some("system_notice" | "notification")
            ),
            _ => false,
        };
        if output {
            last.insert(attempt.id.as_str(), entry);
        }
    }
    last.into_values()
        .filter(|entry| {
            let Entry::Message(message) = entry else {
                return false;
            };
            let Some((attempt, item)) = attempts.get(entry.id()) else {
                return false;
            };
            let completed = item.completed_at.as_deref().and_then(parse_time);
            let superseded_at = attempt.completed_at.as_deref().and_then(parse_time);
            !message.streaming
                && item.kind == "assistant_message"
                && item.status == "completed"
                && matches!((completed, superseded_at), (Some(done), Some(cut)) if done < cut)
        })
        .map(|entry| entry.id().to_owned())
        .collect()
}

/// `SupersededAttemptFold`.
struct AttemptFold {
    attempt_id: String,
    hidden: HashSet<String>,
}

/// `deriveSupersededAttemptFolds`: the output of an attempt a steer cut off,
/// anchored at its first entry. Prompts, published pages, linked resources,
/// working children and finished answers stay out.
fn attempt_folds(
    entries: &[Entry],
    attempts: &Attempts,
    failed: &HashSet<String>,
    complete: &HashSet<String>,
) -> BTreeMap<String, AttemptFold> {
    let live = live_subagent_ids(entries);
    let complete_attempts: HashSet<&str> = complete
        .iter()
        .filter_map(|id| attempts.get(id).map(|(a, _)| a.id.as_str()))
        .collect();
    let mut order: Vec<String> = Vec::new();
    let mut by_attempt: HashMap<String, Vec<&Entry>> = HashMap::new();
    for entry in entries {
        let Some(attempt) = superseded(attempts, entry) else {
            continue;
        };
        let kept = complete_attempts.contains(attempt.id.as_str())
            || failed.contains(&attempt.run_id)
            || matches!(entry, Entry::Message(m) if m.role == "user")
            || matches!(entry, Entry::HtmlRender(_) | Entry::McpApp(_))
            || matches!(entry, Entry::Work(w) if persistent_card(w)
                || w.item_type.as_deref() == Some("system_notice"))
            || live.contains(entry.id());
        if kept {
            continue;
        }
        by_attempt
            .entry(attempt.id.clone())
            .or_insert_with(|| {
                order.push(attempt.id.clone());
                Vec::new()
            })
            .push(entry);
    }
    order
        .into_iter()
        .filter_map(|attempt_id| {
            let group = by_attempt.remove(&attempt_id)?;
            let anchor = group.first()?.id().to_owned();
            Some((
                anchor,
                AttemptFold {
                    attempt_id,
                    hidden: group.iter().map(|e| e.id().to_owned()).collect(),
                },
            ))
        })
        .collect()
}

/// `PERSISTENT_RESOURCE_V2_ITEM_TYPES`: linked resources stay visible when
/// their turn folds.
fn persistent_card(entry: &WorkEntry) -> bool {
    entry.source_kind == "event"
        && matches!(
            entry.item_type.as_deref(),
            Some("fork" | "thread_created" | "secret_request")
        )
}

fn subagent(entry: &WorkEntry) -> bool {
    entry.source_kind == "event" && entry.item_type.as_deref() == Some("subagent")
}

/// The lifecycle items drawn as cards (`isV2LifecycleItem`).
pub fn is_card(entry: &WorkEntry) -> bool {
    entry.source_kind == "event"
        && matches!(
            entry.item_type.as_deref(),
            Some("run_interrupt_result" | "handoff" | "fork" | "subagent" | "secret_request")
        )
}

fn provider_turn(entry: &WorkEntry) -> Option<&str> {
    entry.item.as_ref()?.provider_turn_id.as_deref()
}

/// `liveSubagentCardEntryIds`: a batch of adjacent subagent cards from one
/// provider turn stays out of its fold while any of them still works.
fn live_subagent_ids(entries: &[Entry]) -> HashSet<String> {
    let mut visible = HashSet::new();
    let mut batch: Vec<&WorkEntry> = Vec::new();
    let mut flush = |batch: &mut Vec<&WorkEntry>| {
        let live = batch.iter().any(|w| {
            matches!(
                w.item.as_ref().map(|i| i.status.as_str()),
                Some("pending" | "running" | "waiting")
            )
        });
        if live {
            visible.extend(batch.iter().map(|w| w.id.clone()));
        }
        batch.clear();
    };
    for entry in entries {
        let Entry::Work(w) = entry else {
            flush(&mut batch);
            continue;
        };
        if !subagent(w) {
            flush(&mut batch);
            continue;
        }
        if batch
            .last()
            .is_some_and(|p| p.run_id != w.run_id || provider_turn(p) != provider_turn(w))
        {
            flush(&mut batch);
        }
        batch.push(w);
    }
    flush(&mut batch);
    visible
}

/// `deriveTimelineEntriesFromVisibleTurnItems`, in the order the server
/// committed. A run's interrupt request is bookkeeping its result repeats.
fn entries(detail: &Detail) -> Vec<Entry> {
    let folded_answers: HashSet<String> = detail
        .visible
        .iter()
        .filter(|row| row.item.kind == "user_input_request")
        .filter_map(|row| row.item.get("questionAnswer")["requestId"].as_str())
        .map(|id| format!("async-answer:{id}"))
        .collect();
    let plans: HashMap<&str, &str> = detail
        .plans
        .iter()
        .filter(|p| p.kind == "proposed_plan")
        .map(|p| (p.id.as_str(), p.status.as_str()))
        .collect();
    let mut out = Vec::new();
    for row in &detail.visible {
        let item = &row.item;
        if workspace_preparation(item) {
            continue;
        }
        // Task progress belongs in the composer, not between messages.
        if item.kind == "todo_list" || item.kind == "checkpoint" {
            continue;
        }
        let message_id = item.str("messageId").unwrap_or_default();
        if item.kind == "user_message" && folded_answers.contains(message_id) {
            continue;
        }
        let created_at = item
            .started_at
            .clone()
            .unwrap_or_else(|| item.updated_at.clone());
        if let Some(render) = crate::html_render::from_tool_item(item) {
            out.push(Entry::HtmlRender(HtmlRenderEntry {
                id: item.id.clone(),
                run_id: item.run_id.clone(),
                created_at,
                render,
            }));
            continue;
        }
        if let Some(app) = crate::mcp_app::from_tool_item(item) {
            out.push(Entry::McpApp(McpAppEntry {
                id: item.id.clone(),
                run_id: item.run_id.clone(),
                created_at,
                source_thread_id: row.source_thread_id.clone(),
                item: item.clone(),
                app,
            }));
            continue;
        }
        match item.kind.as_str() {
            "notification" => out.push(Entry::Work(WorkEntry {
                id: item.id.clone(),
                created_at,
                run_id: item.run_id.clone(),
                label: item.str("summary").unwrap_or_default().to_owned(),
                tone: "info".into(),
                detail: None,
                command: None,
                item_type: Some(item.kind.clone()),
                changed_files: Vec::new(),
                status: None,
                source_kind: String::new(),
                question: None,
                item: Some(item.clone()),
            })),
            "user_message" | "assistant_message" => {
                let user = item.kind == "user_message";
                let (text, attribution, scheduled_task_id) = if user {
                    user_message_presentation(message_id, item.text(), item)
                } else {
                    (item.text().to_owned(), None, None)
                };
                out.push(Entry::Message(Message {
                    id: message_id.to_owned(),
                    role: if user { "user" } else { "assistant" }.into(),
                    text,
                    attachments: item
                        .get("attachments")
                        .as_array()
                        .cloned()
                        .unwrap_or_default(),
                    context: if user {
                        item.get("context").clone()
                    } else {
                        Value::Null
                    },
                    run_id: item.run_id.clone(),
                    streaming: !user && item.flag("streaming"),
                    created_at,
                    updated_at: item.updated_at.clone(),
                    input_intent: if user {
                        item.str("inputIntent").map(str::to_owned)
                    } else {
                        None
                    },
                    attribution,
                    scheduled_task_id,
                    sender_thread_id: item.str("senderThreadId").map(str::to_owned),
                    status: item.status.clone(),
                }));
            }
            "proposed_plan" => {
                let plan_id = item.str("planId").unwrap_or_default();
                out.push(Entry::Plan(ProposedPlan {
                    id: plan_id.to_owned(),
                    run_id: item.run_id.clone(),
                    plan_markdown: item.str("markdown").unwrap_or_default().to_owned(),
                    status: plans.get(plan_id).copied().unwrap_or("active").to_owned(),
                    created_at,
                    updated_at: item.updated_at.clone(),
                }));
            }
            kind if STANDALONE.contains(&kind) => {
                if kind == "run_interrupt_request" {
                    continue;
                }
                let (label, detail) = event_presentation(item);
                let mut entry = work_entry(item, &created_at);
                entry.label = label;
                entry.detail = detail;
                entry.command = None;
                entry.source_kind = "event".into();
                if kind == "run_interrupt_result" {
                    entry.tone = "error".into();
                }
                out.push(Entry::Work(entry));
            }
            _ => out.push(Entry::Work(work_entry(item, &created_at))),
        }
    }
    out
}

/// `planItemTime`: when the plan's latest item changed.
fn plan_item_time(detail: &Detail, plan_id: &str) -> String {
    detail
        .turn_items
        .iter()
        .rev()
        .find(|i| {
            matches!(i.kind.as_str(), "proposed_plan" | "todo_list")
                && i.str("planId") == Some(plan_id)
        })
        .map_or_else(|| detail.updated_at.clone(), |i| i.updated_at.clone())
}

/// `findLatestProposedPlan`: the latest run's newest proposed plan, else
/// the thread's.
pub fn latest_proposed_plan(detail: &Detail, latest_run: Option<&str>) -> Option<ProposedPlan> {
    let plans: Vec<_> = detail
        .plans
        .iter()
        .filter(|p| p.kind == "proposed_plan")
        .collect();
    let of_run: Vec<_> = match latest_run {
        Some(run) => plans
            .iter()
            .filter(|p| p.run_id.as_deref() == Some(run))
            .copied()
            .collect(),
        None => plans.clone(),
    };
    let candidates = if of_run.is_empty() { plans } else { of_run };
    let plan = candidates.into_iter().max_by(|a, b| {
        plan_item_time(detail, &a.id)
            .cmp(&plan_item_time(detail, &b.id))
            .then_with(|| a.id.cmp(&b.id))
    })?;
    let at = plan_item_time(detail, &plan.id);
    Some(ProposedPlan {
        id: plan.id.clone(),
        run_id: plan.run_id.clone(),
        plan_markdown: plan.markdown.clone(),
        status: plan.status.clone(),
        created_at: at.clone(),
        updated_at: at,
    })
}

/// `workLogEntryIsToolLike`.
pub fn is_tool_like(entry: &WorkEntry) -> bool {
    entry.tone == "tool"
        || entry.tone == "thinking"
        || entry.tone == "error"
        || entry.command.is_some()
        || entry
            .item
            .as_ref()
            .is_some_and(|item| item.kind == "approval_request")
}

/// `toolOutputIndicatesFailure` (`shared/toolOutput.ts`).
fn output_indicates_failure(text: &str) -> bool {
    let lower = text.to_lowercase();
    const PHRASES: &[&str] = &[
        "file not found",
        "no files found",
        "enoent",
        "no such file",
        "commandnotfoundexception",
        "command not found",
        "is not recognized as the name of a cmdlet",
        "a parameter cannot be found that matches parameter name",
    ];
    if PHRASES.iter().any(|p| lower.contains(p))
        || (lower.contains("cannot find path") && lower.contains("because it does not exist"))
        || (lower.contains("is not recognized") && lower.contains("the term '"))
    {
        return true;
    }
    // `exit code N` with N > 0, as the patterns around it spell it.
    lower.match_indices("exit code").any(|(at, _)| {
        let rest = lower[at + "exit code".len()..].trim_start_matches([':', ' ', '\t']);
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u64>().is_ok_and(|code| code > 0)
    })
}

/// `workEntryDisplayIndicatesToolFailure`: the rendered output, not the
/// user's command, says the call failed. T3 MCP tool results are not read.
pub fn indicates_failure(entry: &WorkEntry) -> bool {
    if entry.tone == "error" || matches!(entry.status.as_deref(), Some("failed" | "declined")) {
        return true;
    }
    if !is_tool_like(entry) {
        return false;
    }
    if let Some(item) = entry
        .item
        .as_ref()
        .filter(|i| i.kind == "command_execution")
    {
        if item.flag("outputIndicatesFailure")
            || item.get("exitCode").as_i64().is_some_and(|code| code != 0)
        {
            return true;
        }
        if let Some(output) = item.str("output") {
            let end = output
                .char_indices()
                .nth(32_768)
                .map_or(output.len(), |(i, _)| i);
            if output_indicates_failure(&output[..end]) {
                return true;
            }
        }
    }
    entry
        .detail
        .as_deref()
        .is_some_and(|d| !d.is_empty() && output_indicates_failure(d))
}

/// `workEntryToolName`: structured identity first.
fn tool_name(entry: &WorkEntry) -> Option<String> {
    let item = entry.item.as_ref()?;
    if item.kind == "dynamic_tool"
        && let Some(name) = item.str("toolName")
    {
        return Some(name.to_owned());
    }
    if item.kind != "dynamic_tool" {
        if let (Some(server), Some(tool)) = (item.str("server"), item.str("tool")) {
            return Some(format!("{server}.{tool}"));
        }
        if let Some(name) = item.str("toolName") {
            return Some(name.to_owned());
        }
    }
    let title = tool_title(entry);
    if title
        .as_deref()
        .and_then(crate::tool_presentation::t3_tool)
        .is_some()
    {
        return title;
    }
    Some(entry.label.clone())
}

/// `toolTitle`: the call's heading, by item type.
fn tool_title(entry: &WorkEntry) -> Option<String> {
    let item = entry.item.as_ref()?;
    let title = item
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned);
    let fallback = match item.kind.as_str() {
        "command_execution" => "Command",
        "file_change" => "File change",
        "file_search" => "File search",
        "web_search" => "Web search",
        "dynamic_tool" => item.str("toolName").unwrap_or("Tool"),
        _ => return None,
    };
    Some(title.unwrap_or_else(|| fallback.to_owned()))
}

/// `toolData`: a dynamic tool's input and output, else the item.
fn tool_data(entry: &WorkEntry) -> Value {
    match &entry.item {
        Some(item) if item.kind == "dynamic_tool" => {
            json!({"input": item.get("input"), "output": item.get("output")})
        }
        Some(item) => Value::Object(item.data.clone()),
        None => Value::Null,
    }
}

/// `workEntryToolOutput`.
fn tool_output(entry: &WorkEntry) -> Value {
    let data = tool_data(entry);
    ["output", "result", "rawOutput", "content"]
        .iter()
        .map(|key| &data[*key])
        .find(|v| !v.is_null())
        .cloned()
        .unwrap_or(Value::Null)
}

/// `resolveWorkEntryToolPresentation`: a T3 tool's tensed label, and its
/// definition.
pub fn t3_presentation(
    entry: &WorkEntry,
    fallback_status: Option<&str>,
) -> Option<(String, &'static crate::tool_presentation::T3Tool)> {
    use crate::tool_presentation::{t3_label, t3_result_failed, t3_tool};
    let tool = t3_tool(&tool_name(entry)?)?;
    let status = if t3_result_failed(&tool_output(entry)) {
        "failed"
    } else {
        fallback_status
            .filter(|_| entry.status.is_none())
            .or(entry.status.as_deref())
            .unwrap_or("")
    };
    let data = tool_data(entry);
    let input = ["arguments", "input", "rawInput"]
        .iter()
        .map(|key| &data[*key])
        .find(|v| !v.is_null())
        .unwrap_or(&Value::Null);
    Some((t3_label(tool, status, input), tool))
}

/// `workLogEntryIsLocalCodeSearch`.
fn local_code_search(entry: &WorkEntry) -> bool {
    match entry.item_type.as_deref() {
        Some("file_search") => true,
        Some("web_search") => {
            let heading = tool_title(entry).unwrap_or_else(|| entry.label.clone());
            heading
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word.eq_ignore_ascii_case("grep"))
        }
        _ => false,
    }
}

/// `toolGroupAction`: `thread-create`, a PR action (`link-pr`,
/// `unlink-pr`, `list-prs`, `watch-pr`, `unwatch-pr`), `browser`, `device`,
/// `read`, `edit`, `command`, `search`, `code-search`, `other` or `update`.
pub fn group_action(entry: &WorkEntry) -> &'static str {
    use crate::tool_presentation::{Activity, classify};
    let kind = entry.item_type.as_deref();
    if kind == Some("thread_created") {
        return "thread-create";
    }
    if let Some((_, tool)) = t3_presentation(entry, None) {
        if matches!(
            tool.summary,
            "link-pr" | "unlink-pr" | "list-prs" | "watch-pr" | "unwatch-pr"
        ) {
            return tool.summary;
        }
        if matches!(tool.icon, "browser" | "device") {
            return tool.icon;
        }
    }
    let item = entry.item.as_ref();
    if item.is_some_and(|i| i.str("viewedImagePath").is_some()) {
        return "read";
    }
    if matches!(kind, Some("approval_request" | "user_input_request")) {
        return if is_tool_like(entry) {
            "other"
        } else {
            "update"
        };
    }
    let data = tool_data(entry);
    let tool_name = match item {
        Some(i) if i.kind == "dynamic_tool" => i.str("toolName").map(str::to_owned),
        _ => data["toolName"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| tool_title(entry)),
    };
    let classified = classify(
        match kind {
            Some(k @ ("command_execution" | "file_change" | "web_search")) => Some(k),
            Some("dynamic_tool") => Some("dynamic_tool_call"),
            _ => None,
        },
        None,
        data["kind"].as_str(),
        tool_name.as_deref(),
    );
    match classified {
        Activity::Read => return "read",
        Activity::FileChange => return "edit",
        _ if kind == Some("file_change") => return "edit",
        Activity::Command => return "command",
        _ if kind == Some("command_execution") || entry.command.is_some() => return "command",
        Activity::Search => {
            return if kind == Some("web_search") && !local_code_search(entry) {
                "search"
            } else {
                "code-search"
            };
        }
        _ => {}
    }
    if local_code_search(entry) {
        "code-search"
    } else if kind == Some("web_search") {
        "search"
    } else if !entry.changed_files.is_empty() {
        "edit"
    } else if is_tool_like(entry) {
        "other"
    } else {
        "update"
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// `t3ToolSummaryCall`.
fn t3_summary_call(entry: &WorkEntry) -> crate::tool_presentation::SummaryCall {
    let data = tool_data(entry);
    let input = ["arguments", "input", "rawInput"]
        .iter()
        .map(|key| &data[*key])
        .find(|v| !v.is_null())
        .cloned()
        .unwrap_or(Value::Null);
    let failed =
        matches!(entry.status.as_deref(), Some("failed" | "declined")) || entry.tone == "error";
    let outcome = if failed {
        "failed"
    } else if entry.status.as_deref() == Some("completed") {
        "completed"
    } else {
        "unfinished"
    };
    crate::tool_presentation::SummaryCall {
        input,
        output: tool_output(entry),
        outcome,
    }
}

/// `toolGroupActionLabel`.
fn group_action_label(action: &str, count: usize) -> String {
    let pull_requests = plural(count, "pull request", "pull requests");
    let times = plural(count, "time", "times");
    match action {
        "link-pr" => format!("Linked {pull_requests}"),
        "unlink-pr" => format!("Unlinked {pull_requests}"),
        "watch-pr" => format!("Watching {pull_requests}"),
        "unwatch-pr" => format!("Stopped watching {pull_requests}"),
        "list-prs" if count == 1 => "Checked linked pull requests".into(),
        "list-prs" => format!("Checked linked pull requests {count} times"),
        "read" => format!("Read {}", plural(count, "file", "files")),
        "edit" => format!("Changed {}", plural(count, "file", "files")),
        "command" => format!("Ran {}", plural(count, "command", "commands")),
        "thread-create" => format!("Created {}", plural(count, "thread", "threads")),
        "device" => format!("Used device controls {times}"),
        "browser" => format!("Used browser {times}"),
        "search" => format!("Searched the web {times}"),
        "code-search" => format!("Searched code {times}"),
        "other" => format!("Used {}", plural(count, "tool", "tools")),
        _ => format!("Received {}", plural(count, "update", "updates")),
    }
}

/// `summaryActionPriority`: effects first, then lookups, then the rest.
fn summary_priority(action: &str) -> u8 {
    match action {
        "command" | "edit" | "delegate" | "task-cancel" | "thread-create" | "thread-send"
        | "thread-interrupt" | "workflow-stop" | "schedule-create" | "schedule-update"
        | "schedule-delete" | "schedule-run" | "thread-configure" | "thread-fork"
        | "thread-merge" | "thread-organize" | "thread-update" | "queue-edit" | "queue-cancel"
        | "queue-reorder" | "queue-steer" | "question-respond" | "worktree-handoff"
        | "project-create" | "project-update" | "project-delete" | "project-clone"
        | "environment-update" | "attachment-prepare" | "attachment-discard"
        | "attachment-send" => 0,
        "other" | "update" => 2,
        _ => 1,
    }
}

/// Port of `summarizeToolGroup`: at most two categories, by priority,
/// then how many other calls there were ("Ran 2 commands and changed 1
/// file"). T3 calls group by their summary action (`summarizeT3ToolCalls`).
pub fn summarize_tool_group(entries: &[WorkEntry]) -> String {
    let tools: Vec<&WorkEntry> = entries
        .iter()
        .filter(|e| e.item_type.as_deref() != Some("reasoning"))
        .collect();
    if !entries.is_empty() && tools.is_empty() {
        return if entries.len() == 1 {
            "Thought".into()
        } else {
            format!("Thought (×{})", entries.len())
        };
    }
    let mut groups: Vec<(&'static str, &'static str, Vec<&WorkEntry>)> = Vec::new();
    // Calls from a named source (`toolSource`, not T3's) say who ran them.
    let mut sources: Vec<(String, String, String)> = Vec::new();
    let mut sourced = 0;
    for entry in &tools {
        let t3 = t3_presentation(entry, None);
        let source = entry
            .item
            .as_ref()
            .map(|i| i.get("toolSource"))
            .filter(|s| s["key"].is_string() && s["name"].is_string());
        if let (Some(source), None) = (source, &t3) {
            sourced += 1;
            let key = source["key"].as_str().unwrap_or_default().to_owned();
            if !sources.iter().any(|(k, ..)| *k == key) {
                sources.push((
                    key,
                    source["name"].as_str().unwrap_or_default().to_owned(),
                    source["kind"].as_str().unwrap_or_default().to_owned(),
                ));
            }
            continue;
        }
        let action = group_action(entry);
        let key = t3.map_or(action, |(_, tool)| tool.summary);
        match groups.iter_mut().find(|(k, _, _)| *k == key) {
            Some((_, _, list)) => list.push(entry),
            None => groups.push((key, action, vec![entry])),
        }
    }
    let mut selected: Vec<(usize, u8, usize, String)> = groups
        .iter()
        .enumerate()
        .map(|(index, (key, action, list))| {
            let count = if *action == "edit" {
                let mut files = HashSet::new();
                let mut without = 0;
                for entry in list {
                    if entry.changed_files.is_empty() {
                        without += 1;
                    }
                    files.extend(entry.changed_files.iter());
                }
                files.len() + without
            } else {
                list.len()
            };
            let label = if t3_presentation(list[0], None).is_some() {
                let calls: Vec<crate::tool_presentation::SummaryCall> =
                    list.iter().map(|e| t3_summary_call(e)).collect();
                crate::tool_presentation::summarize_t3_calls(key, &calls).0
            } else {
                group_action_label(action, count)
            };
            (index, summary_priority(key), list.len(), label)
        })
        .collect();
    selected.sort_by_key(|(index, priority, _, _)| (*priority, *index));
    selected.truncate(2);
    selected.sort_by_key(|(index, ..)| *index);
    let mut labels: Vec<String> = selected.iter().map(|(.., label)| label.clone()).collect();
    if !sources.is_empty() {
        let mut names: Vec<String> = Vec::new();
        for (_, name, _) in &sources {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        let joined = match names.len() {
            1 => names[0].clone(),
            2 => names.join(" and "),
            n => format!("{}, and {}", names[..n - 1].join(", "), names[n - 1]),
        };
        let integrations = sources.iter().all(|(.., kind)| kind == "integration");
        let suffix = match (integrations, names.len()) {
            (true, 1) => " integration",
            (true, _) => " integrations",
            _ => "",
        };
        labels.insert(0, format!("Used {joined}{suffix}"));
    }
    let remaining =
        tools.len() - sourced - selected.iter().map(|(_, _, count, _)| count).sum::<usize>();
    if remaining > 0 {
        labels.push(format!(
            "Performed {}",
            plural(remaining, "other action", "other actions")
        ));
    }
    let sentence: Vec<String> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            if i == 0 {
                label.clone()
            } else {
                let mut chars = label.chars();
                chars
                    .next()
                    .map(|c| c.to_lowercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default()
            }
        })
        .collect();
    match sentence.len() {
        0 => String::new(),
        1 => sentence[0].clone(),
        2 => sentence.join(" and "),
        n => format!("{}, and {}", sentence[..n - 1].join(", "), sentence[n - 1]),
    }
}

/// `toolGroupSummaryKind`: what the group toggle's icon stands for.
pub fn group_summary_kind(entries: &[WorkEntry]) -> &'static str {
    let tools: Vec<&WorkEntry> = entries
        .iter()
        .filter(|e| e.item_type.as_deref() != Some("reasoning"))
        .collect();
    if !entries.is_empty() && tools.is_empty() {
        return "reasoning";
    }
    if !tools.is_empty()
        && tools
            .iter()
            .all(|e| t3_presentation(e, None).is_some_and(|(_, t)| t.icon == "pull-request"))
    {
        return "pull-request";
    }
    let actions: HashSet<&str> = tools.iter().map(|e| group_action(e)).collect();
    if actions.len() != 1 {
        return "mixed";
    }
    let action = actions.into_iter().next().unwrap_or("mixed");
    if action != "other" {
        return action;
    }
    let kinds: HashSet<&str> = tools
        .iter()
        .map(|e| match (e.item_type.as_deref(), e.tone.as_str()) {
            (Some("dynamic_tool"), _) => "dynamic-tool",
            (Some("subagent"), _) | (_, "thinking") => "agent-tool",
            (_, "tool") => "tone-tool",
            _ => "other",
        })
        .collect();
    if kinds.len() == 1 {
        kinds.into_iter().next().unwrap_or("mixed")
    } else {
        "mixed"
    }
}

/// `workEntryIconName`. `t3-code` stands for upstream's wordmark, which
/// Pluto draws as its own mark; `browser-app` is upstream's
/// `BrowserAppIcon`.
pub fn work_icon(entry: &WorkEntry) -> &'static str {
    let item = entry.item.as_deref();
    if let Some(item) = item.filter(|i| i.kind == "notification") {
        if item.str("outcome") == Some("failed") {
            return "circle-alert";
        }
        let source = item.get("source");
        return match source["kind"].as_str() {
            Some("subagent" | "delegated_task") => "bot",
            Some("background_task") if source["work"] == "subagent" => "bot",
            Some("command" | "background_command") => "terminal",
            Some("monitor") => "eye",
            Some("system") => "t3-code",
            _ => "zap",
        };
    }
    if matches!(
        entry.item_type.as_deref(),
        Some("user_input_request" | "approval_request")
    ) {
        return "message-circle";
    }
    if let Some(item) = item {
        let surface = item
            .str("toolSurface")
            .filter(|s| matches!(*s, "browser" | "computer"));
        let source = item.get("toolSource")["kind"].as_str();
        let dynamic_surface = (item.kind == "dynamic_tool")
            .then(|| {
                crate::tool_presentation::dynamic_tool_surface(
                    item.str("toolName"),
                    item.get("input"),
                )
            })
            .flatten();
        if surface.is_some()
            || matches!(source, Some("browser" | "computer"))
            || dynamic_surface.is_some()
        {
            return "browser-app";
        }
        if item.kind == "dynamic_tool" && crate::tool_presentation::is_js_repl(item.str("toolName"))
        {
            return "code-xml";
        }
    }
    if let Some((_, tool)) = t3_presentation(entry, None) {
        return summary_icon(tool.icon);
    }
    let action = group_action(entry);
    if action != "other" {
        return summary_icon(action);
    }
    match entry.item_type.as_deref() {
        Some("dynamic_tool") => return "wrench",
        Some("subagent") => return "bot",
        _ => {}
    }
    match entry.tone.as_str() {
        "error" => "circle-alert",
        "thinking" => "brain",
        "info" => "check",
        _ => "zap",
    }
}

/// `toolGroupSummaryIconName`, for a summary kind, a group action or a T3
/// definition's icon.
pub fn summary_icon(kind: &str) -> &'static str {
    match kind {
        "pull-request" | "link-pr" | "unlink-pr" | "list-prs" | "watch-pr" | "unwatch-pr" => {
            "git-pull-request-arrow"
        }
        "read" => "eye",
        "edit" => "square-pen",
        "command" => "terminal",
        "thread-create" | "t3-code" => "t3-code",
        "browser" => "browser-app",
        "device" => "smartphone",
        "search" => "globe",
        "code-search" => "search",
        "other" => "wrench",
        "reasoning" => "brain",
        "agent-tool" => "bot",
        "tone-tool" => "zap",
        _ => "hammer",
    }
}

/// `threadReadTargetId`: the thread a `t3_thread_read` call reads.
pub fn thread_read_target(entry: &WorkEntry) -> Option<String> {
    let item = entry.item.as_ref().filter(|i| i.kind == "dynamic_tool")?;
    let tool = crate::tool_presentation::t3_tool(item.str("toolName")?)?;
    (tool.summary == "thread-read")
        .then(|| item.get("input")["threadId"].as_str())
        .flatten()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

/// `threadReadLabelPrefix`: "Read a T3 thread" names its thread instead
/// (`Read thread`), in the label's tense.
pub fn thread_read_prefix(label: &str) -> Option<String> {
    label
        .strip_suffix(" a T3 thread")
        .map(|prefix| format!("{prefix} thread"))
}

/// `normalizeCompactToolLabel`.
fn compact_tool_label(value: &str) -> &str {
    let trimmed = value.trim_end();
    for suffix in [" completed", " complete"] {
        if trimmed.len() >= suffix.len()
            && trimmed[trimmed.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
        {
            return trimmed[..trimmed.len() - suffix.len()].trim();
        }
    }
    trimmed.trim()
}

/// `workEntryDisplayLabel`: what a standalone row shows.
pub fn display_label(entry: &WorkEntry) -> String {
    if let Some(question) = &entry.question {
        let preview: Vec<String> = question
            .questions
            .iter()
            .map(|q| q.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|q| !q.is_empty())
            .collect();
        if !preview.is_empty() {
            return preview.join(" · ");
        }
    }
    let kind = entry.item_type.as_deref();
    if entry.source_kind == "event" || kind == Some("system_notice") {
        return entry.label.clone();
    }
    if kind == Some("reasoning") || entry.tone == "thinking" {
        let thought = entry
            .detail
            .as_deref()
            .map(|d| d.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|d| !d.is_empty());
        return thought.unwrap_or_else(|| entry.label.clone());
    }
    if let Some((label, _)) = t3_presentation(entry, None) {
        return label;
    }
    if let Some(command) = entry.command.as_deref().filter(|c| !c.trim().is_empty()) {
        return command.to_owned();
    }
    let action = group_action(entry);
    if matches!(action, "code-search" | "search") {
        // Adapters title file searches with their target; the item keeps
        // only the pattern.
        let label = if kind == Some("file_search") {
            Some(entry.label.clone())
        } else {
            crate::tool_presentation::search_label(&tool_data(entry))
        };
        if let Some(label) = label {
            return label;
        }
    }
    if action == "read" {
        let paths = read_paths(entry);
        if let Some(first) = paths.first() {
            return crate::tool_presentation::read_label(first, paths.len() - 1);
        }
    }
    let item = entry.item.as_ref();
    let provider_retry = item.is_some_and(|i| i.kind == "error" && i.get("retry").is_object());
    if let Some(title) = item.filter(|i| i.kind == "dynamic_tool").and_then(|i| {
        crate::tool_presentation::dynamic_tool_title(
            i.str("toolName"),
            i.get("input"),
            i.title.as_deref(),
        )
    }) {
        return title;
    }
    let detail = entry
        .detail
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty());
    let search_output = matches!(action, "code-search" | "search")
        && detail.is_some_and(|d| d.contains(['\r', '\n']));
    if let Some(detail) = detail
        && !provider_retry
        && action != "read"
        && !search_output
    {
        return detail.to_owned();
    }
    match entry.changed_files.as_slice() {
        [] => {}
        [one] => return one.clone(),
        [first, rest @ ..] => return format!("{first} +{} more", rest.len()),
    }
    if action == "read" && item.is_none_or(|i| i.str("viewedImagePath").is_none()) {
        return "Read file".into();
    }
    let heading = tool_title(entry)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| entry.label.clone());
    capitalize(compact_tool_label(&heading))
}

/// `workEntryReadRawPaths`.
fn read_paths(entry: &WorkEntry) -> Vec<String> {
    if !entry.changed_files.is_empty() {
        return entry.changed_files.clone();
    }
    if let Some(item) = entry.item.as_ref().filter(|i| i.kind == "dynamic_tool") {
        let paths =
            crate::tool_presentation::collect_file_paths(&json!({"input": item.get("input")}));
        if !paths.is_empty() {
            return paths;
        }
    }
    crate::tool_presentation::collect_file_paths(&tool_data(entry))
}

/// `singleToolCallLabel`.
fn single_tool_call_label(entry: &WorkEntry) -> String {
    if entry.item_type.as_deref() == Some("reasoning") {
        let thought = entry
            .detail
            .as_deref()
            .map(|d| d.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|d| !d.is_empty());
        return thought.unwrap_or_else(|| "Thought".into());
    }
    if let Some((label, _)) = t3_presentation(entry, Some("completed")) {
        return label;
    }
    let item = entry.item.as_ref();
    if let Some(title) = item.filter(|i| i.kind == "dynamic_tool").and_then(|i| {
        crate::tool_presentation::dynamic_tool_title(
            i.str("toolName"),
            i.get("input"),
            i.title.as_deref(),
        )
    }) {
        return title;
    }
    // A lone web search keeps its heading; the query stays in its detail.
    if entry.item_type.as_deref() == Some("web_search") {
        return tool_title(entry).unwrap_or_else(|| "Web search".into());
    }
    display_label(entry)
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// Port of `@t3tools/shared/orchestrationTiming.formatDuration`.
/// `deriveProviderSubagentStatus` and `formatProviderSubagentStatus`: what
/// a provider-native subagent is doing, from its runless root turn node
/// ("Working 12s", "Completed in 3m 4s", "Starting" before it exists),
/// and whether it is still active.
pub fn provider_subagent_status(
    detail: &crate::model::Detail,
    now: chrono::DateTime<chrono::Utc>,
) -> (String, bool) {
    let Some(node) = detail
        .nodes
        .iter()
        .rev()
        .find(|n| n.kind == "root_turn" && n.run_id.is_none())
    else {
        return ("Starting".into(), true);
    };
    let label = match node.status.as_str() {
        "idle" => "Idle",
        "pending" | "running" => "Working",
        "waiting" => "Waiting",
        "completed" => "Completed",
        "interrupted" => "Interrupted",
        "failed" => "Failed",
        "cancelled" | "rolled_back" => "Cancelled",
        other => other,
    }
    .to_owned();
    let active = matches!(node.status.as_str(), "pending" | "running" | "waiting");
    let elapsed = |end: chrono::DateTime<chrono::Utc>| {
        node.started_at
            .as_deref()
            .and_then(parse_time)
            .map(|start| format_duration(((end - start).num_seconds().max(1)) * 1000))
    };
    let text = if active {
        match elapsed(now) {
            Some(duration) => format!("{label} {duration}"),
            None => label,
        }
    } else if node.status == "completed" {
        match node
            .completed_at
            .as_deref()
            .and_then(parse_time)
            .and_then(elapsed)
        {
            Some(duration) => format!("Completed in {duration}"),
            None => label,
        }
    } else {
        label
    };
    (text, active)
}

pub fn format_duration(ms: i64) -> String {
    if ms < 0 {
        return "0ms".into();
    }
    if ms < 1_000 {
        return format!("{}ms", ms.max(1));
    }
    if ms < 10_000 {
        let tenths = (ms as f64 / 100.0).round() / 10.0;
        return if tenths >= 10.0 {
            "10s".into()
        } else {
            format!("{tenths:.1}s")
        };
    }
    if ms < 60_000 {
        return format!("{}s", (ms as f64 / 1_000.0).round() as i64);
    }
    let total = (ms as f64 / 1_000.0).round() as i64;
    let (hours, minutes, seconds) = (total / 3_600, (total % 3_600) / 60, total % 60);
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds > 0 {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
}

pub fn parse_time(iso: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
}

fn elapsed_ms(start: &str, end: &str) -> Option<i64> {
    let (start, end) = (parse_time(start)?, parse_time(end)?);
    let ms = (end - start).num_milliseconds();
    (ms >= 0).then_some(ms)
}

/// `deriveUnsettledRunId`.
fn unsettled_run_id(latest: Option<&RunSummary>, running: Option<&str>) -> Option<String> {
    if let Some(running) = running {
        return Some(running.to_owned());
    }
    let latest = latest?;
    let settled = latest.completed_at.is_some()
        && !matches!(latest.status.as_str(), "running" | "starting" | "waiting");
    (!settled).then(|| latest.run_id.clone())
}

/// `deriveTerminalAssistantMessageIds`: each response's last answer.
fn terminal_assistant_ids(entries: &[Entry]) -> HashSet<String> {
    let mut last: BTreeMap<String, String> = BTreeMap::new();
    let mut unkeyed = 0;
    for entry in entries {
        let Entry::Message(message) = entry else {
            continue;
        };
        if message.role == "user" {
            unkeyed += 1;
            continue;
        }
        let key = match &message.run_id {
            Some(run) => format!("turn:{run}"),
            None => format!("unkeyed:{unkeyed}"),
        };
        last.insert(key, message.id.clone());
    }
    last.into_values().collect()
}

/// `failedTimelineRunIds`.
fn failed_run_ids(entries: &[Entry], latest: Option<&RunSummary>) -> HashSet<String> {
    let mut failed: HashSet<String> = entries
        .iter()
        .filter_map(Entry::failed_run)
        .map(str::to_owned)
        .collect();
    if let Some(latest) = latest.filter(|l| l.status == "failed") {
        failed.insert(latest.run_id.clone());
    }
    failed
}

/// `lastResponseBoundaryIndex`.
fn last_boundary(entries: &[Entry]) -> Option<usize> {
    entries.iter().rposition(Entry::starts_response)
}

/// `deriveActiveVisualResponseRunIds`: the unsettled run, and while working
/// every run since the prompt that started the response.
fn active_response_runs(
    entries: &[Entry],
    unsettled: Option<&str>,
    working: bool,
) -> HashSet<String> {
    let mut runs = HashSet::new();
    let Some(unsettled) = unsettled else {
        return runs;
    };
    runs.insert(unsettled.to_owned());
    if working {
        let from = last_boundary(entries).map_or(0, |i| i + 1);
        runs.extend(
            entries[from..]
                .iter()
                .filter_map(|e| e.run_id().map(str::to_owned)),
        );
    }
    runs
}

struct Fold {
    run_id: String,
    created_at: String,
    hidden: HashSet<String>,
    label: String,
}

/// `deriveTurnFolds`: settled runs fold their work behind "Worked for ...".
/// A response without a run (one imported from protocol 1) folds under a key
/// of its own prompt.
fn turn_folds(
    entries: &[Entry],
    terminal: &HashSet<String>,
    latest: Option<&RunSummary>,
    unfolded: &HashSet<String>,
    complete_replies: &HashSet<String>,
) -> BTreeMap<String, Fold> {
    let live_subagents = live_subagent_ids(entries);
    let mut interrupted: HashSet<String> = HashSet::new();
    let mut prompt_index: HashMap<String, usize> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        if entry.starts_response()
            && let Some(run) = entry.boundary_run_id()
        {
            prompt_index.entry(run.to_owned()).or_insert(index);
        }
        if let Entry::Work(w) = entry
            && w.item
                .as_ref()
                .is_some_and(|i| i.kind == "run_interrupt_result")
            && let Some(run) = &w.run_id
        {
            interrupted.insert(run.clone());
        }
    }
    struct Group<'a> {
        entries: Vec<&'a Entry>,
        terminal: Option<&'a Message>,
        streaming: bool,
        start: Option<String>,
        anchor: String,
    }
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Group> = HashMap::new();
    let mut runless_failed: HashSet<String> = HashSet::new();
    let mut runless: Option<String> = None;
    let mut response: Option<String> = None;
    let mut pending: Option<(String, String)> = None;
    for (index, entry) in entries.iter().enumerate() {
        if entry.starts_response() {
            pending = entries
                .get(index + 1)
                .map(|next| (entry.created_at().to_owned(), next.id().to_owned()));
            let boundary = entry.boundary_run_id();
            runless = boundary
                .is_none()
                .then(|| format!("runless:{}", entry.id()));
            response = boundary.map(str::to_owned).or_else(|| runless.clone());
            continue;
        }
        let Some(own) = entry.fold_run_id(runless.as_deref()) else {
            continue;
        };
        // A resumed subagent's item keeps the place of the turn that started it.
        let run = match &response {
            Some(response) if prompt_index.get(&own).is_some_and(|&i| i > index) => {
                response.clone()
            }
            _ => own,
        };
        if Some(&run) == runless.as_ref() && entry.failed_run().is_some() {
            runless_failed.insert(run.clone());
        }
        let group = groups.entry(run.clone()).or_insert_with(|| {
            order.push(run.clone());
            let (start, anchor) = match pending.take() {
                Some((start, anchor)) => (Some(start), anchor),
                None => (None, entry.id().to_owned()),
            };
            Group {
                entries: Vec::new(),
                terminal: None,
                streaming: false,
                start,
                anchor,
            }
        });
        group.entries.push(entry);
        if let Entry::Message(m) = entry {
            if terminal.contains(&m.id) {
                group.terminal = Some(m);
            }
            if m.streaming {
                group.streaming = true;
            }
        }
    }
    let mut folds = BTreeMap::new();
    for run in order {
        let group = &groups[&run];
        if unfolded.contains(&run)
            || interrupted.contains(&run)
            || runless_failed.contains(&run)
            || group.streaming
        {
            continue;
        }
        let terminal_index = group
            .terminal
            .and_then(|t| group.entries.iter().position(|e| e.id() == t.id))
            .unwrap_or(group.entries.len());
        let mut hidden = HashSet::new();
        for (index, entry) in group.entries.iter().enumerate() {
            if Some(entry.id()) == group.terminal.map(|t| t.id.as_str()) {
                continue;
            }
            let compaction =
                matches!(entry, Entry::Work(w) if w.source_kind == "context-compaction");
            let foldable_trailing = matches!(entry, Entry::Work(w)
                if w.source_kind != "event" && w.status.as_deref() != Some("inProgress")
                    && !indicates_failure(w));
            if !compaction && index > terminal_index && !foldable_trailing {
                continue;
            }
            // Linked resources and still-working children outlive the fold.
            if matches!(entry, Entry::Work(w) if persistent_card(w))
                || live_subagents.contains(entry.id())
                || complete_replies.contains(entry.id())
            {
                continue;
            }
            if matches!(entry, Entry::Work(w) if w.item_type.as_deref() == Some("notification")) {
                continue;
            }
            hidden.insert(entry.id().to_owned());
        }
        // A lone compaction row stays visible on its own.
        let hides_work = group.entries.iter().any(|e| {
            hidden.contains(e.id())
                && !matches!(e, Entry::Work(w) if w.source_kind == "context-compaction")
        });
        if !hides_work {
            continue;
        }
        let (Some(first), Some(last)) = (group.entries.first(), group.entries.last()) else {
            continue;
        };
        let last_end = match last {
            Entry::Message(m) => m.updated_at.clone(),
            other => other.created_at().to_owned(),
        };
        let latest_for_run = latest.filter(|l| l.run_id == run);
        let elapsed = match latest_for_run {
            Some(RunSummary {
                started_at: Some(start),
                completed_at: Some(end),
                ..
            }) => elapsed_ms(start, end),
            _ => {
                let end = group
                    .terminal
                    .map(|t| t.updated_at.clone())
                    .filter(|t| *t > last_end)
                    .unwrap_or(last_end);
                elapsed_ms(group.start.as_deref().unwrap_or(first.created_at()), &end)
            }
        };
        let duration = elapsed.map(format_duration);
        let interrupted = latest_for_run.is_some_and(|l| l.status == "interrupted");
        let label = match (interrupted, duration) {
            (true, Some(d)) => format!("You stopped after {d}"),
            (true, None) => "You stopped this response".into(),
            (false, Some(d)) => format!("Worked for {d}"),
            (false, None) => "Worked".into(),
        };
        folds.insert(
            group.anchor.clone(),
            Fold {
                run_id: run.clone(),
                created_at: group
                    .start
                    .clone()
                    .unwrap_or_else(|| first.created_at().to_owned()),
                hidden,
                label,
            },
        );
    }
    folds
}

/// `deriveThreadCheckpointSummaries` keyed by the run's last answer: the
/// files a run changed, shown under that answer.
fn changes_by_message(detail: &Detail) -> HashMap<String, (String, Vec<CheckpointFile>)> {
    let mut out = HashMap::new();
    for checkpoint in &detail.checkpoints {
        let (Some(run), Some(_)) = (&checkpoint.run_id, checkpoint.app_run_ordinal) else {
            continue;
        };
        let answer = detail
            .messages
            .iter()
            .rev()
            .find(|m| m["runId"] == run.as_str() && m["role"] == "assistant")
            .and_then(|m| m["id"].as_str());
        if let Some(answer) = answer
            && !checkpoint.files.is_empty()
        {
            out.insert(answer.to_owned(), (run.clone(), checkpoint.files.clone()));
        }
    }
    out
}

/// The fold that hides `entry_id` while folded (a citation reveal opens it).
/// The role of the timeline message `message_id`, when it is loaded.
pub fn message_role(detail: &Detail, message_id: &str) -> Option<String> {
    entries(detail).into_iter().find_map(|entry| match entry {
        Entry::Message(message) if message.id == message_id => Some(message.role),
        _ => None,
    })
}

pub fn fold_containing(thread: &Thread, detail: &Detail, entry_id: &str) -> Option<String> {
    let entries = entries(detail);
    let runtime = thread.runtime.as_ref();
    let working = runtime.is_some_and(|r| matches!(r.status.as_str(), "running" | "waiting"));
    let running = runtime
        .filter(|r| r.is_active())
        .and_then(|r| r.active_run_id.as_deref());
    let latest = thread.latest_run.as_ref();
    let unsettled = unsettled_run_id(latest, running);
    let unfolded: HashSet<String> = active_response_runs(&entries, unsettled.as_deref(), working)
        .union(&failed_run_ids(&entries, latest))
        .cloned()
        .collect();
    let complete = complete_superseded_replies(&entries, &entry_attempts(detail));
    let folds = turn_folds(
        &entries,
        &terminal_assistant_ids(&entries),
        latest,
        &unfolded,
        &complete,
    );
    folds
        .into_values()
        .find(|fold| fold.hidden.contains(entry_id))
        .map(|fold| fold.run_id)
}

/// `deriveMessagesTimelineRows` for `thread` (the shell row: its latest run
/// and runtime) and its projection.
pub fn rows(thread: &Thread, detail: &Detail, expansion: &Expansion) -> Vec<Row> {
    let entries = entries(detail);
    let runtime = thread.runtime.as_ref();
    let is_working = runtime.is_some_and(|r| matches!(r.status.as_str(), "running" | "waiting"));
    let running_run = runtime
        .filter(|r| r.is_active())
        .and_then(|r| r.active_run_id.as_deref());
    let latest = thread.latest_run.as_ref();
    let terminal = terminal_assistant_ids(&entries);
    let unsettled = unsettled_run_id(latest, running_run);
    let failed = failed_run_ids(&entries, latest);
    let active_runs = active_response_runs(&entries, unsettled.as_deref(), is_working);
    let unfolded: HashSet<String> = active_runs.union(&failed).cloned().collect();
    let attempts = entry_attempts(detail);
    let complete = complete_superseded_replies(&entries, &attempts);
    let attempt_folds = attempt_folds(&entries, &attempts, &failed, &complete);
    let folds = turn_folds(&entries, &terminal, latest, &unfolded, &complete);
    let mut collapsed: HashSet<String> = HashSet::new();
    for fold in folds.values() {
        if !expansion.turns.contains(&fold.run_id) {
            collapsed.extend(fold.hidden.iter().cloned());
        }
    }
    let attempt_key = |id: &str| format!("attempt-fold:{id}");
    let mut cut_off: HashSet<String> = HashSet::new();
    for fold in attempt_folds.values() {
        if !expansion.groups.contains(&attempt_key(&fold.attempt_id)) {
            cut_off.extend(fold.hidden.iter().cloned());
        }
    }
    let changes = changes_by_message(detail);
    // A steer continues the current turn: the working header stays under
    // the prompt that started it.
    let working_index = if is_working {
        last_boundary(&entries).map_or(0, |i| i + 1)
    } else {
        usize::MAX
    };
    let started_at = thread.working_started_at();
    let breaks_group = |entry: &WorkEntry| {
        entry.tone == "error"
            || entry.question.is_some()
            || entry.source_kind == "event"
            || entry.source_kind == "runtime.error"
            || entry.source_kind == "context-compaction"
            || matches!(
                entry.item_type.as_deref(),
                Some("system_notice" | "notification")
            )
            || entry.run_id.as_ref().is_some_and(|r| failed.contains(r))
    };

    // `activeToolEntries`: the active run's trailing work becomes one live
    // row that keeps the latest tool once everything settles.
    let mut active: Vec<&WorkEntry> = Vec::new();
    if is_working && unsettled.is_some() {
        for entry in entries.iter().skip(working_index).rev() {
            let Entry::Work(w) = entry else {
                break;
            };
            if w.tone == "error"
                || w.source_kind == "runtime.error"
                || w.source_kind == "event"
                || w.source_kind == "context-compaction"
                || matches!(
                    w.item_type.as_deref(),
                    Some("system_notice" | "notification")
                )
                || !w.run_id.as_ref().is_some_and(|r| active_runs.contains(r))
                || collapsed.contains(&w.id)
                || cut_off.contains(&w.id)
                || folds.contains_key(&w.id)
                || attempt_folds.contains_key(&w.id)
            {
                break;
            }
            active.push(w);
        }
        active.reverse();
    }
    let visible: Vec<&WorkEntry> = active
        .iter()
        .copied()
        .filter(|e| visible_in_live_group(e))
        .collect();
    let latest_visible = visible.last().copied();
    let latest_running = visible.iter().rev().copied().find(|e| active_activity(e));
    let keeps_live = latest_running.is_some() || latest_visible.is_some_and(indicates_success);
    let latest_failed = latest_running.is_none()
        && latest_visible
            .is_some_and(|e| e.status.as_deref() != Some("declined") && indicates_failure(e));
    let anchor_group = active.first().map(|a| format!("work-group:{}", a.id));
    let live_row = match (&anchor_group, latest_visible) {
        (Some(group_id), Some(latest)) if !latest_failed => Some(Row::LiveWork {
            group_id: group_id.clone(),
            entry: Box::new(latest_running.unwrap_or(latest).clone()),
            entries: visible.iter().map(|e| (*e).clone()).collect(),
            expanded: expansion.groups.contains(group_id),
            active: keeps_live,
            thought: visible
                .iter()
                .rev()
                .find(|e| {
                    e.item_type.as_deref() == Some("reasoning")
                        && e.detail.as_deref().is_some_and(|d| !d.trim().is_empty())
                })
                .and_then(|e| e.detail.clone()),
        }),
        _ => None,
    };
    let placement = latest_visible.map(|e| e.id.clone());
    let live_ids: HashSet<String> = if live_row.is_some() || latest_failed {
        active.iter().map(|e| e.id.clone()).collect()
    } else {
        HashSet::new()
    };
    // `workEntryIsInActiveRun`.
    let in_active_run = |w: &WorkEntry| {
        is_working
            && w.status.as_deref() == Some("inProgress")
            && w.run_id.is_some()
            && w.run_id == unsettled
    };
    let mut compacting = false;
    let mut has_activity = false;

    let mut out = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        if index == working_index {
            out.push(Row::Working {
                started_at: started_at.clone(),
            });
        }
        let entry = &entries[index];
        if placement.as_deref() == Some(entry.id())
            && let Some(row) = &live_row
        {
            out.push(row.clone());
            if let Row::LiveWork {
                group_id,
                entries,
                expanded,
                active,
                ..
            } = row
            {
                has_activity |= *active;
                if *expanded {
                    out.push(Row::WorkDetails {
                        group_id: group_id.clone(),
                        entries: entries.clone(),
                    });
                }
            }
        }
        if live_ids.contains(entry.id()) {
            index += 1;
            continue;
        }
        if let Some(fold) = folds.get(entry.id()) {
            out.push(Row::TurnFold {
                run_id: fold.run_id.clone(),
                label: fold.label.clone(),
                expanded: expansion.turns.contains(&fold.run_id),
                created_at: fold.created_at.clone(),
            });
        }
        if collapsed.contains(entry.id()) {
            index += 1;
            continue;
        }
        if let Some(fold) = attempt_folds.get(entry.id()) {
            out.push(Row::AttemptFold {
                attempt_id: fold.attempt_id.clone(),
                expanded: expansion.groups.contains(&attempt_key(&fold.attempt_id)),
            });
        }
        if cut_off.contains(entry.id()) {
            index += 1;
            continue;
        }
        match entry {
            Entry::Work(first) if first.source_kind == "context-compaction" => {
                let active = in_active_run(first);
                compacting |= active;
                out.push(Row::Compaction {
                    id: first.id.clone(),
                    label: first.label.clone(),
                    active,
                });
            }
            Entry::Work(first) if is_card(first) => {
                // Adjacent subagents of one provider turn share a row.
                if subagent(first)
                    && let Some(Row::Event { entries, .. }) = out.last_mut()
                    && entries.last().is_some_and(|p| {
                        subagent(p)
                            && p.run_id == first.run_id
                            && provider_turn(p) == provider_turn(first)
                    })
                {
                    entries.push(first.clone());
                } else {
                    out.push(Row::Event {
                        id: first.id.clone(),
                        entries: vec![first.clone()],
                    });
                }
            }
            Entry::Work(first) => {
                if breaks_group(first) {
                    out.push(Row::Work {
                        id: first.id.clone(),
                        label: display_label(first),
                        entry: Box::new(first.clone()),
                    });
                    index += 1;
                    continue;
                }
                let mut group = vec![first.clone()];
                let mut cursor = index + 1;
                while let Some(Entry::Work(next)) = entries.get(cursor) {
                    if breaks_group(next)
                        || live_ids.contains(&next.id)
                        || collapsed.contains(&next.id)
                        || cut_off.contains(&next.id)
                        || folds.contains_key(&next.id)
                        || attempt_folds.contains_key(&next.id)
                        || cursor == working_index
                    {
                        break;
                    }
                    group.push(next.clone());
                    cursor += 1;
                }
                let group_id = format!("work-group:{}", first.id);
                if group.len() == 1 && is_tool_like(&group[0]) {
                    let label = if group_action(&group[0]) == "edit" {
                        summarize_tool_group(&group)
                    } else {
                        single_tool_call_label(&group[0])
                    };
                    out.push(Row::Work {
                        id: first.id.clone(),
                        label,
                        entry: Box::new(group[0].clone()),
                    });
                } else {
                    let expanded = expansion.groups.contains(&group_id);
                    let summary = if group.len() == 1 && !is_tool_like(&group[0]) {
                        group[0].label.clone()
                    } else {
                        summarize_tool_group(&group)
                    };
                    out.push(Row::WorkToggle {
                        group_id: group_id.clone(),
                        summary,
                        expanded,
                        kind: group_summary_kind(&group),
                        has_failure: group
                            .iter()
                            .rev()
                            .find(|e| is_tool_like(e))
                            .is_some_and(indicates_failure),
                    });
                    if expanded {
                        out.push(Row::WorkDetails {
                            group_id,
                            entries: group,
                        });
                    }
                }
                index = cursor;
                continue;
            }
            Entry::Message(message) if message.role == "user" => {
                out.push(Row::User(message.clone()))
            }
            Entry::Message(message) => {
                let in_progress = message
                    .run_id
                    .as_ref()
                    .is_some_and(|r| active_runs.contains(r));
                out.push(Row::Assistant {
                    message: message.clone(),
                    show_meta: terminal.contains(&message.id) && !in_progress,
                    changes: changes.get(&message.id).cloned(),
                });
            }
            Entry::Plan(plan) => out.push(Row::Plan(Box::new(plan.clone()))),
            Entry::HtmlRender(page) => out.push(Row::HtmlRender(Box::new(page.clone()))),
            Entry::McpApp(app) => out.push(Row::McpApp(Box::new(app.clone()))),
        }
        index += 1;
    }
    if is_working && working_index >= entries.len() {
        out.push(Row::Working { started_at });
    }
    if is_working && !compacting && (!has_activity || latest_failed) {
        // A failed latest tool hands the row back to thinking; its group
        // stays one click away.
        let group_id = anchor_group.filter(|_| latest_failed);
        let expanded = group_id
            .as_ref()
            .is_some_and(|g| expansion.groups.contains(g));
        out.push(Row::Thinking {
            group_id: group_id.clone(),
            expanded,
        });
        if let Some(group_id) = group_id.filter(|_| expanded) {
            out.push(Row::WorkDetails {
                group_id,
                entries: visible.iter().map(|e| (*e).clone()).collect(),
            });
        }
    }
    attach_created_thread_summaries(out, &entries)
}

/// `attachCreatedThreadSummaries`: the threads a run created show again
/// as cards (`source_kind` `summary`) under its final answer; their own
/// rows go when the run folds or sit after that answer.
fn attach_created_thread_summaries(rows: Vec<Row>, entries: &[Entry]) -> Vec<Row> {
    let mut terminal: HashMap<&str, usize> = HashMap::new();
    let mut collapsed: HashSet<&str> = HashSet::new();
    for (index, row) in rows.iter().enumerate() {
        match row {
            Row::Assistant {
                message,
                show_meta: true,
                ..
            } => {
                if let Some(run) = message.run_id.as_deref() {
                    terminal.insert(run, index);
                }
            }
            Row::TurnFold {
                run_id,
                expanded: false,
                ..
            } => {
                collapsed.insert(run_id);
            }
            _ => {}
        }
    }
    let mut created: HashMap<String, Vec<WorkEntry>> = HashMap::new();
    for entry in entries {
        if let Entry::Work(work) = entry
            && work.item_type.as_deref() == Some("thread_created")
            && let Some(run) = &work.run_id
        {
            created.entry(run.clone()).or_default().push(work.clone());
        }
    }
    if created.is_empty() {
        return rows;
    }
    let mut out = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        if let Row::Work { entry, .. } = row
            && entry.item_type.as_deref() == Some("thread_created")
            && let Some(run) = entry.run_id.as_deref()
            && let Some(&end) = terminal.get(run)
            && (collapsed.contains(run) || index > end)
        {
            continue;
        }
        out.push(row.clone());
        if let Row::Assistant {
            message,
            show_meta: true,
            ..
        } = row
            && let Some(list) = message.run_id.as_ref().and_then(|run| created.get(run))
        {
            for entry in list {
                out.push(Row::Event {
                    id: format!("summary:{}", entry.id),
                    entries: vec![WorkEntry {
                        source_kind: "summary".into(),
                        ..entry.clone()
                    }],
                });
            }
        }
    }
    out
}

/// Keep recovered images after their response and outside its work fold,
/// before the next prompt written after the file was saved.
pub fn insert_generated_images(
    rows: &mut Vec<Row>,
    images: &[crate::generated_images::GeneratedImage],
) {
    for image in images {
        let saved_at: chrono::DateTime<chrono::Utc> = image.modified.into();
        let next_prompt = rows.iter().position(|row| {
            matches!(row, Row::User(message)
                if parse_time(&message.created_at).is_some_and(|at| at > saved_at))
        });
        rows.insert(
            next_prompt.unwrap_or(rows.len()),
            Row::GeneratedImage {
                image: image.clone(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RunSummary, VisibleItem};
    use serde_json::json;

    fn item(id: &str, kind: &str, at: &str, run: Option<&str>, extra: Value) -> VisibleItem {
        let mut raw = json!({"id":id,"threadId":"t","runId":run,"ordinal":0,"status":"completed",
            "title":null,"startedAt":at,"completedAt":at,"updatedAt":at,"type":kind});
        for (key, value) in extra.as_object().unwrap() {
            raw[key] = value.clone();
        }
        let item: TurnItem = serde_json::from_value(raw).unwrap();
        VisibleItem {
            visibility: "local".into(),
            source_thread_id: "t".into(),
            source_item_id: id.into(),
            item: Arc::new(item),
            ..Default::default()
        }
    }

    #[test]
    fn tool_calls_read_like_upstream() {
        let at = "2026-10-01T00:00:00.000Z";
        let entry = |id: &str, kind: &str, extra: Value| {
            work_entry(&item(id, kind, at, None, extra).item, at)
        };
        let delegate = entry(
            "a",
            "dynamic_tool",
            json!({"toolName": "mcp__t3-code__delegate_task", "input": {}}),
        );
        assert_eq!(display_label(&delegate), "Delegated a child task");
        assert_eq!(group_action(&delegate), "other");
        let link = entry(
            "b",
            "dynamic_tool",
            json!({"toolName": "t3-code.link_pull_request",
            "input": {"url": "https://github.com/a/b/pull/7"}}),
        );
        assert_eq!(display_label(&link), "Linked PR #7");
        assert_eq!(group_action(&link), "link-pr");
        let grep = entry(
            "c",
            "dynamic_tool",
            json!({"toolName": "Grep",
            "input": {"pattern": "todo", "path": "/src/app"}}),
        );
        assert_eq!(grep.label, "Searched todo in app");
        assert_eq!(group_action(&grep), "code-search");
        let read = entry(
            "d",
            "dynamic_tool",
            json!({"toolName": "Read", "input": {"file_path": "/a/b.rs"}}),
        );
        assert_eq!(display_label(&read), "Read /a/b.rs");
        let command = entry("e", "command_execution", json!({"input": "ls"}));
        let commands: Vec<WorkEntry> = (0..3).map(|_| command.clone()).collect();
        let mut group = commands.clone();
        group.extend([grep.clone(), read.clone(), delegate.clone()]);
        // Two categories by priority, then the rest.
        assert_eq!(
            summarize_tool_group(&group),
            "Ran 3 commands, delegated 1 task, and performed 2 other actions"
        );
        assert_eq!(group_summary_kind(&commands), "command");
        let linear = entry(
            "g",
            "dynamic_tool",
            json!({"toolName": "linear.search",
            "toolSource": {"key": "linear", "name": "Linear", "kind": "integration"}}),
        );
        assert_eq!(
            summarize_tool_group(&[linear, command.clone()]),
            "Used Linear integration and ran 1 command"
        );
        let read_thread = entry(
            "f",
            "dynamic_tool",
            json!({"toolName": "t3_thread_read",
            "input": {"threadId": " other "}}),
        );
        assert_eq!(thread_read_target(&read_thread).as_deref(), Some("other"));
        assert_eq!(display_label(&read_thread), "Read a T3 thread");
        assert_eq!(
            thread_read_prefix("Reading a T3 thread").as_deref(),
            Some("Reading thread")
        );
        assert_eq!(group_summary_kind(&[link.clone(), link]), "pull-request");
    }

    #[test]
    fn user_messages_say_who_sent_them() {
        let presentation = |id: &str, text: &str, extra: Value| {
            let row = item(id, "user_message", "2026-10-01T00:00:00.000Z", None, extra);
            user_message_presentation(id, text, &row.item)
        };
        assert_eq!(
            presentation("m", "hi", json!({"scheduledTaskId": "task"})),
            ("hi".into(), Some("automation".into()), Some("task".into()))
        );
        assert_eq!(
            presentation(
                "scheduled-task-message:task-1:3:manual",
                "[Triggered by schedule task: Daily]\n\nRun it",
                json!({})
            ),
            (
                "Run it".into(),
                Some("automation".into()),
                Some("task-1".into())
            )
        );
        assert_eq!(
            presentation(
                "m",
                "go",
                json!({"createdBy": "agent", "creationSource": "server"})
            )
            .1,
            Some("t3code".into())
        );
        assert_eq!(
            presentation("m", "go", json!({"createdBy": "agent"})).1,
            Some("agent".into())
        );
        assert_eq!(
            presentation("m", "go", json!({"createdBy": "user"})).1,
            None
        );
    }

    #[test]
    fn memory_calls_say_what_they_remembered() {
        let call = |tool: &str, input: Value, output: Value| {
            let row = item(
                "m",
                "dynamic_tool",
                "2026-10-27T00:00:00.000Z",
                None,
                json!({"toolName": tool, "input": input, "output": output}),
            );
            let entry = work_entry(&row.item, "");
            (entry.label, entry.detail)
        };
        let saved = (
            "Saved to memory".to_owned(),
            Some("Lives in Lyon.".to_owned()),
        );
        assert_eq!(
            call(
                "mcp__t3-code__memory_save",
                json!({"text": "Lives in Lyon."}),
                json!({})
            ),
            saved
        );
        // Kept as JSON text, or in Cursor's envelope.
        assert_eq!(
            call(
                "t3-code.memory_save",
                json!(r#"{"text":"Lives in Lyon."}"#),
                json!({})
            ),
            saved
        );
        assert_eq!(
            call(
                "memory_save",
                json!({"toolName": "memory_save", "args": {"text": "Lives in Lyon."}}),
                json!({})
            ),
            saved
        );
        // A refused call keeps the tool's name.
        let refused = call(
            "memory_save",
            json!({"text": "x"}),
            json!({"isError": true}),
        );
        assert_eq!(refused.0, "memory_save");
    }

    fn settled() -> (Thread, Detail) {
        let visible = vec![
            item(
                "u",
                "user_message",
                "2026-01-01T00:00:00.000Z",
                Some("run"),
                json!({"messageId":"u","text":"go","inputIntent":"turn_start","attachments":[]}),
            ),
            item(
                "c1",
                "command_execution",
                "2026-01-01T00:01:00.000Z",
                Some("run"),
                json!({"input":"cargo build"}),
            ),
            item(
                "c2",
                "command_execution",
                "2026-01-01T00:02:00.000Z",
                Some("run"),
                json!({"input":"cargo test"}),
            ),
            item(
                "f1",
                "file_change",
                "2026-01-01T00:03:00.000Z",
                Some("run"),
                json!({"fileName":"src/app.rs"}),
            ),
            item(
                "a",
                "assistant_message",
                "2026-01-01T00:04:00.000Z",
                Some("run"),
                json!({"messageId":"a","text":"done","streaming":false}),
            ),
        ];
        let detail = Detail {
            visible,
            ..Default::default()
        };
        let thread = Thread {
            id: "t".into(),
            latest_run: Some(RunSummary {
                run_id: "run".into(),
                status: "completed".into(),
                completed_at: Some("2026-01-01T00:04:00.000Z".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        (thread, detail)
    }

    #[test]
    fn settled_runs_fold_work_behind_worked_for() {
        let (thread, detail) = settled();
        let rows = rows(&thread, &detail, &Expansion::default());
        assert!(matches!(&rows[0], Row::User(_)));
        assert!(matches!(&rows[1], Row::TurnFold { label, .. } if label == "Worked for 4m"));
        assert!(matches!(
            &rows[2],
            Row::Assistant {
                show_meta: true,
                ..
            }
        ));
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn created_threads_show_again_under_the_answer() {
        let (thread, mut detail) = settled();
        detail.visible.insert(
            2,
            item(
                "c",
                "thread_created",
                "2026-01-01T00:02:00.000Z",
                Some("run"),
                json!({"targetThreadId": "child", "title": "Child"}),
            ),
        );
        let rows = rows(&thread, &detail, &Expansion::default());
        // Folded: only the summary under the final answer remains.
        assert!(matches!(
            &rows[2],
            Row::Assistant {
                show_meta: true,
                ..
            }
        ));
        assert!(matches!(&rows[3], Row::Event { id, entries }
            if id == "summary:c" && entries[0].source_kind == "summary"));
        assert_eq!(rows.len(), 4);
    }

    #[test]
    fn published_pages_stay_out_of_folds() {
        let (thread, mut detail) = settled();
        detail.visible.insert(
            3,
            item(
                "h",
                "dynamic_tool",
                "2026-01-01T00:02:30.000Z",
                Some("run"),
                json!({"toolName": "mcp__t3-code__html_render", "input": {},
                    "output": {"htmlRender": {"attachmentId": "p-html", "title": "Chart", "height": 400}}}),
            ),
        );
        let rows = rows(&thread, &detail, &Expansion::default());
        assert!(matches!(&rows[1], Row::TurnFold { .. }));
        assert!(matches!(&rows[2], Row::HtmlRender(page) if page.render.attachment_id == "p-html"));
        assert!(matches!(&rows[3], Row::Assistant { .. }));
        assert_eq!(rows.len(), 4);
    }

    #[test]
    fn expanded_fold_summarizes_the_tool_group() {
        let (thread, detail) = settled();
        let mut expansion = Expansion::default();
        expansion.turns.insert("run".into());
        let rows = rows(&thread, &detail, &expansion);
        assert!(matches!(&rows[2], Row::WorkToggle { summary, .. }
            if summary == "Ran 2 commands and changed 1 file"));
    }

    #[test]
    fn runless_responses_fold_under_their_prompt() {
        let (mut thread, mut detail) = settled();
        thread.latest_run = None;
        detail.visible = detail
            .visible
            .into_iter()
            .map(|mut row| {
                Arc::make_mut(&mut row.item).run_id = None;
                row
            })
            .collect();
        let rows = rows(&thread, &detail, &Expansion::default());
        assert!(matches!(&rows[1], Row::TurnFold { run_id, .. } if run_id == "runless:u"));
    }

    #[test]
    fn recovered_images_stay_after_the_answer_and_outside_work_folds() {
        let (thread, mut detail) = settled();
        detail.visible.push(item(
            "u2",
            "user_message",
            "2026-01-01T00:05:00.000Z",
            Some("next-run"),
            json!({"messageId":"u2","text":"Next prompt","inputIntent":"turn_start"}),
        ));
        let image = crate::generated_images::GeneratedImage {
            path: "/fixture/native/exec-image.png".into(),
            modified: parse_time("2026-01-01T00:03:00.000Z").unwrap().into(),
            bytes: 128,
        };
        let mut shown = rows(&thread, &detail, &Expansion::default());
        insert_generated_images(&mut shown, &[image]);
        let picture = shown
            .iter()
            .position(|r| matches!(r, Row::GeneratedImage { .. }))
            .unwrap();
        assert!(matches!(&shown[picture - 1], Row::Assistant { message, .. } if message.id == "a"));
        assert!(matches!(&shown[picture + 1], Row::User(message) if message.id == "u2"));
        assert!(shown.iter().any(|r| matches!(
            r,
            Row::TurnFold {
                expanded: false,
                ..
            }
        )));
    }

    #[test]
    fn interrupted_runs_stay_unfolded() {
        let (thread, mut detail) = settled();
        detail.visible.push(item(
            "i",
            "run_interrupt_result",
            "2026-01-01T00:05:00.000Z",
            Some("run"),
            json!({"message":"Stopped by you"}),
        ));
        let rows = rows(&thread, &detail, &Expansion::default());
        assert!(!rows.iter().any(|r| matches!(r, Row::TurnFold { .. })));
        assert!(matches!(rows.last(), Some(Row::Event { entries, .. })
            if entries[0].item_type.as_deref() == Some("run_interrupt_result")));
    }

    #[test]
    fn a_working_turn_shows_its_trailing_calls_as_one_live_row() {
        let (mut thread, mut detail) = settled();
        // The run still works: no answer yet, the last call running.
        detail.visible.truncate(4);
        Arc::make_mut(&mut detail.visible[3].item).status = "running".into();
        thread.latest_run = Some(RunSummary {
            run_id: "run".into(),
            status: "running".into(),
            ..Default::default()
        });
        thread.runtime = Some(crate::model::RuntimeSummary {
            status: "running".into(),
            active_run_id: Some("run".into()),
            ..Default::default()
        });
        let rows = rows(&thread, &detail, &Expansion::default());
        let live: Vec<&Row> = rows
            .iter()
            .filter(|r| matches!(r, Row::LiveWork { .. }))
            .collect();
        assert_eq!(live.len(), 1, "one live row");
        let Row::LiveWork {
            entry,
            entries,
            active,
            ..
        } = live[0]
        else {
            unreachable!()
        };
        assert!(*active);
        assert_eq!(entry.id, "f1");
        assert_eq!(entries.len(), 3);
        // The calls it stands for are not drawn again, and nothing thinks.
        assert!(
            !rows
                .iter()
                .any(|r| matches!(r, Row::WorkToggle { .. } | Row::Work { .. }))
        );
        assert!(!rows.iter().any(|r| matches!(r, Row::Thinking { .. })));
    }

    #[test]
    fn a_steer_folds_the_attempt_it_cut_off() {
        let (thread, mut detail) = settled();
        // c1 and c2 belong to the first attempt, which the steer replaced.
        for (index, node) in [(1, "n1"), (2, "n1"), (3, "n2"), (4, "n2")] {
            Arc::make_mut(&mut detail.visible[index].item).node_id = Some(node.into());
        }
        let attempt = |id: &str, node: &str, status: &str| RunAttempt {
            id: id.into(),
            run_id: "run".into(),
            root_node_id: node.into(),
            status: status.into(),
            completed_at: Some("2026-01-01T00:02:30.000Z".into()),
            ..Default::default()
        };
        detail.attempts = vec![
            attempt("a1", "n1", "superseded"),
            attempt("a2", "n2", "completed"),
        ];
        let mut expansion = Expansion::default();
        expansion.turns.insert("run".into());
        let folded = rows(&thread, &detail, &expansion);
        let fold = folded
            .iter()
            .position(|r| matches!(r, Row::AttemptFold { attempt_id, expanded: false } if attempt_id == "a1"))
            .expect("an attempt fold");
        // Its calls are hidden; the replacement attempt's work stays.
        let ids: Vec<String> = folded[fold + 1..]
            .iter()
            .filter_map(|r| match r {
                Row::Work { id, .. } => Some(id.clone()),
                Row::WorkToggle { group_id, .. } => Some(group_id.clone()),
                _ => None,
            })
            .collect();
        assert!(
            !ids.iter().any(|id| id.contains("c1") || id.contains("c2")),
            "{ids:?}"
        );
        expansion.groups.insert("attempt-fold:a1".into());
        let open = rows(&thread, &detail, &expansion);
        assert!(
            open.iter()
                .any(|r| matches!(r, Row::AttemptFold { expanded: true, .. }))
        );
    }

    #[test]
    fn mcp_apps_show_where_their_call_happened() {
        let (thread, mut detail) = settled();
        detail.visible.insert(
            3,
            item(
                "app",
                "dynamic_tool",
                "2026-01-01T00:02:30.000Z",
                Some("run"),
                json!({"toolName": "weather.forecast", "output": {"t3McpApp": {
                    "attachmentId": "att", "server": "weather", "tool": "forecast",
                    "resourceUri": "ui://weather/forecast"}}}),
            ),
        );
        let rows = rows(&thread, &detail, &Expansion::default());
        // Folded turns keep the app out, like a published page.
        assert!(rows.iter().any(|r| matches!(r, Row::TurnFold { .. })));
        assert!(
            rows.iter()
                .any(|r| matches!(r, Row::McpApp(app) if app.app.server == "weather"))
        );
    }

    #[test]
    fn a_lone_compaction_is_a_divider_outside_the_fold() {
        let (thread, mut detail) = settled();
        detail.visible.insert(
            1,
            item(
                "k",
                "compaction",
                "2026-01-01T00:00:30.000Z",
                Some("run"),
                json!({"beforeTokenCount": 180_000, "afterTokenCount": 12_000}),
            ),
        );
        let mut expansion = Expansion::default();
        expansion.turns.insert("run".into());
        let rows = rows(&thread, &detail, &expansion);
        assert!(
            rows.iter()
                .any(|r| matches!(r, Row::Compaction { label, active: false, .. }
            if label == "Context compacted 180K → 12K tokens"))
        );
    }

    #[test]
    fn live_labels_name_the_program_and_the_first_sentence() {
        assert_eq!(
            command_program("cargo test --all").as_deref(),
            Some("cargo")
        );
        assert_eq!(
            command_program("RUST_LOG=debug /usr/bin/env bash -lc \"npm run build\"").as_deref(),
            Some("npm")
        );
        assert_eq!(
            live_thought_line("I checked `src/app.rs`. Then the [docs](x)."),
            "I checked src/app.rs."
        );
        assert_eq!(
            live_thought_line("**Planning the port**\n\nFirst the rows."),
            "Planning the port"
        );
    }

    #[test]
    fn subagents_of_a_turn_share_a_row_and_fold_once_settled() {
        let with_agents = |status: &str| {
            let (thread, mut detail) = settled();
            for (index, id) in ["s1", "s2"].into_iter().enumerate() {
                let mut agent = item(
                    id,
                    "subagent",
                    &format!("2026-01-01T00:03:3{index}.000Z"),
                    Some("run"),
                    json!({"subagentId": id, "providerTurnId": "pt", "title": "Explore"}),
                );
                Arc::make_mut(&mut agent.item).status = if index == 0 {
                    status.into()
                } else {
                    "completed".into()
                };
                detail.visible.insert(4, agent);
            }
            rows(&thread, &detail, &Expansion::default())
        };
        let event = |rows: &[Row]| {
            rows.iter().find_map(|r| match r {
                Row::Event { entries, .. } => Some(entries.len()),
                _ => None,
            })
        };
        // A working child keeps its batch out of the fold, as one row.
        assert_eq!(event(&with_agents("running")), Some(2));
        // Settled children fold with their turn.
        let rows = with_agents("completed");
        assert!(rows.iter().any(|r| matches!(r, Row::TurnFold { .. })));
        assert_eq!(event(&rows), None);
    }

    #[test]
    fn failures_read_from_exit_codes_and_output() {
        assert!(output_indicates_failure("<exited with exit code 2>"));
        assert!(!output_indicates_failure("exit code 0"));
        assert!(output_indicates_failure("zsh: command not found: foo"));
    }

    #[test]
    fn durations_match_upstream_formatting() {
        assert_eq!(format_duration(1), "1ms");
        assert_eq!(format_duration(4_250), "4.3s");
        assert_eq!(format_duration(42_000), "42s");
        assert_eq!(format_duration(3_723_000), "1h 2m 3s");
        assert_eq!(format_tokens(12_400), "12.4K");
        assert_eq!(format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn provider_subagent_status_reads_its_root_turn() {
        let now = parse_time("2026-10-29T10:01:05.000Z").unwrap();
        let mut detail = crate::model::Detail::default();
        assert_eq!(
            provider_subagent_status(&detail, now),
            ("Starting".into(), true)
        );
        let node = |status: &str, completed: Option<&str>| crate::model::Node {
            id: "n".into(),
            root_node_id: "n".into(),
            kind: "root_turn".into(),
            status: status.into(),
            started_at: Some("2026-10-29T10:00:00.000Z".into()),
            completed_at: completed.map(Into::into),
            ..Default::default()
        };
        detail.nodes = vec![node("running", None)];
        assert_eq!(
            provider_subagent_status(&detail, now),
            ("Working 1m 5s".into(), true)
        );
        detail.nodes = vec![node("completed", Some("2026-10-29T10:00:30.000Z"))];
        assert_eq!(
            provider_subagent_status(&detail, now),
            ("Completed in 30s".into(), false)
        );
        detail.nodes = vec![node("rolled_back", None)];
        assert_eq!(
            provider_subagent_status(&detail, now),
            ("Cancelled".into(), false)
        );
    }
}
