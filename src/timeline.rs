//! Timeline rows for the chat. Entries come from the thread's visible turn
//! items like upstream `session-logic.ts` (`deriveTimelineEntriesFromVisibleTurnItems`,
//! `projectedWorkEntry`); rows fold settled runs and group tool calls like
//! `MessagesTimeline.logic.ts` (`deriveTurnFolds`, `deriveMessagesTimelineRows`).
//! Pages published with `html_render` show where the call happened and stay
//! out of folds (`kind: "html-render"`).
//! Not ported from v0.1.0's rows yet: the live activity row, superseded
//! attempt folds, context compaction rows, MCP apps, and
//! the lifecycle cards (`V2LifecycleRow.tsx`: interrupts, forks, handoffs,
//! subagents, secret requests), which show as standalone work rows with
//! their upstream labels.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use serde_json::Value;

use crate::model::{CheckpointFile, Detail, RunSummary, Thread, TurnItem};

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
    Work(WorkEntry),
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
            Entry::Work(w) => &w.id,
        }
    }

    fn created_at(&self) -> &str {
        match self {
            Entry::Message(m) => &m.created_at,
            Entry::Plan(p) => &p.created_at,
            Entry::HtmlRender(h) => &h.created_at,
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
            Entry::Plan(_) | Entry::HtmlRender(_) => false,
        }
    }

    /// `responseBoundaryRunId`.
    fn boundary_run_id(&self) -> Option<&str> {
        match self {
            Entry::Message(m) => m.run_id.as_deref(),
            Entry::Work(w) => w.run_id.as_deref(),
            Entry::Plan(_) | Entry::HtmlRender(_) => None,
        }
    }

    /// `timelineEntryFoldRunId`; lifecycle items stay out of folds.
    fn fold_run_id(&self, runless: Option<&str>) -> Option<String> {
        match self {
            Entry::Work(w) if w.item_type.as_deref() == Some("system_notice") => None,
            Entry::Work(w) if w.source_kind == "event" => None,
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
    },
    /// Rows of an expanded tool group.
    WorkDetails {
        group_id: String,
        entries: Vec<WorkEntry>,
    },
    Working {
        started_at: Option<String>,
    },
    /// A proposed plan (`ProposedPlanCard`).
    Plan(Box<ProposedPlan>),
    /// A page an agent published (`HtmlRenderTimelineRow`).
    HtmlRender(Box<HtmlRenderEntry>),
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
            format!(
                "Secret requested: {}",
                item.str("label").unwrap_or_default()
            ),
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
            entry.label = or("Searched files".into());
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
                _ => entry.label = or(tool.unwrap_or_else(|| "Tool call".into())),
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
                out.push(Entry::Message(Message {
                    id: message_id.to_owned(),
                    role: if user { "user" } else { "assistant" }.into(),
                    text: item.text().to_owned(),
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Read,
    Edit,
    Command,
    Search,
    Other,
    Update,
}

fn action(entry: &WorkEntry) -> Action {
    let title = entry.label.trim().to_lowercase();
    if title == "read file" {
        Action::Read
    } else if entry.item_type.as_deref() == Some("file_change") || !entry.changed_files.is_empty() {
        Action::Edit
    } else if entry.item_type.as_deref() == Some("command_execution") || entry.command.is_some() {
        Action::Command
    } else if entry.item_type.as_deref() == Some("web_search") {
        Action::Search
    } else if is_tool_like(entry) {
        Action::Other
    } else {
        Action::Update
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// Port of `summarizeToolGroup`, e.g. "Ran 2 commands and changed 1 file".
pub fn summarize_tool_group(entries: &[WorkEntry]) -> String {
    let mut groups: Vec<(Action, Vec<&WorkEntry>)> = Vec::new();
    for entry in entries {
        let action = action(entry);
        match groups.iter_mut().find(|(a, _)| *a == action) {
            Some((_, list)) => list.push(entry),
            None => groups.push((action, vec![entry])),
        }
    }
    let labels: Vec<String> = groups
        .iter()
        .map(|(action, list)| {
            let count = if *action == Action::Edit {
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
            match action {
                Action::Read => format!("Read {}", plural(count, "file", "files")),
                Action::Edit => format!("Changed {}", plural(count, "file", "files")),
                Action::Command => format!("Ran {}", plural(count, "command", "commands")),
                Action::Search => format!("Searched the web {}", plural(count, "time", "times")),
                Action::Other => format!("Used {}", plural(count, "tool", "tools")),
                Action::Update => format!("Received {}", plural(count, "update", "updates")),
            }
        })
        .collect();
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
    if entry.source_kind == "event" {
        return entry.label.clone();
    }
    entry
        .command
        .clone()
        .or_else(|| entry.detail.clone())
        .or_else(|| match entry.changed_files.as_slice() {
            [] => None,
            [one] => Some(one.clone()),
            [first, rest @ ..] => Some(format!("{first} +{} more", rest.len())),
        })
        .unwrap_or_else(|| capitalize(entry.label.trim()))
}

fn single_tool_call_label(entry: &WorkEntry) -> String {
    if let Some(command) = entry
        .command
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        return command.to_owned();
    }
    capitalize(entry.label.trim())
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// Port of `@t3tools/shared/orchestrationTiming.formatDuration`.
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
) -> BTreeMap<String, Fold> {
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
                if w.status.as_deref() != Some("inProgress") && !indicates_failure(w));
            if !compaction && index > terminal_index && !foldable_trailing {
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
    let folds = turn_folds(
        &entries,
        &terminal_assistant_ids(&entries),
        latest,
        &unfolded,
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
    let folds = turn_folds(&entries, &terminal, latest, &unfolded);
    let mut collapsed: HashSet<String> = HashSet::new();
    for fold in folds.values() {
        if !expansion.turns.contains(&fold.run_id) {
            collapsed.extend(fold.hidden.iter().cloned());
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

    let mut out = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        if index == working_index {
            out.push(Row::Working {
                started_at: started_at.clone(),
            });
        }
        let entry = &entries[index];
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
        match entry {
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
                        || collapsed.contains(&next.id)
                        || folds.contains_key(&next.id)
                        || cursor == working_index
                    {
                        break;
                    }
                    group.push(next.clone());
                    cursor += 1;
                }
                let group_id = format!("work-group:{}", first.id);
                if group.len() == 1 && is_tool_like(&group[0]) {
                    let label = if action(&group[0]) == Action::Edit {
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
        }
        index += 1;
    }
    if is_working && working_index >= entries.len() {
        out.push(Row::Working { started_at });
    }
    out
}

/// Keep recovered images after their response and outside its work fold,
/// before the next prompt written after the file was saved.
pub(crate) fn insert_generated_images(
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
        assert!(matches!(rows.last(), Some(Row::Work { label, .. }) if label == "Run interrupted"));
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
}
