//! Native subagent observability (`client-runtime/state/subagentRuntime.ts`):
//! a tolerant fold over a thread's `task.*` / `tool.progress` activities into
//! subagent state, and the panel model the Agents surface renders.
use std::collections::HashMap;

use serde_json::Value;

use crate::model::Activity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Running,
    Waiting,
    Idle,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl Status {
    fn parse(value: &Value) -> Option<Self> {
        Some(match value.as_str()? {
            "pending" => Status::Pending,
            "running" => Status::Running,
            "waiting" => Status::Waiting,
            "idle" => Status::Idle,
            "completed" => Status::Completed,
            "failed" => Status::Failed,
            "cancelled" => Status::Cancelled,
            "interrupted" => Status::Interrupted,
            _ => return None,
        })
    }

    pub fn terminal(self) -> bool {
        matches!(
            self,
            Status::Completed | Status::Failed | Status::Cancelled | Status::Interrupted
        )
    }

    /// The user may still need to care while it runs; waiting needs them.
    pub fn active(self) -> bool {
        matches!(self, Status::Pending | Status::Running | Status::Waiting)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Subagent,
    Batch,
    Workflow,
    WorkflowAgent,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub total_tokens: f64,
    pub tool_uses: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Agent {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub role: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub status: Status,
    pub activation_count: u32,
    pub usage: Option<Usage>,
    pub progress: Option<String>,
    pub last_tool_name: Option<String>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub parent_agent_id: Option<String>,
    pub agent_index: Option<f64>,
    pub phase_index: Option<f64>,
    pub phase_title: Option<String>,
    pub attempt: Option<f64>,
    pub workflow_name: Option<String>,
    pub phases: Vec<(f64, String)>,
    pub first_seen_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub updated_at: String,
}

const SUMMARY_CHAR_LIMIT: usize = 180;
const ROSTER_LIMIT: usize = 100;

fn bounded(value: &str) -> String {
    if value.chars().count() <= SUMMARY_CHAR_LIMIT {
        value.to_owned()
    } else {
        let mut out: String = value.chars().take(SUMMARY_CHAR_LIMIT - 1).collect();
        out.push('…');
        out
    }
}

fn text(value: &Value) -> Option<String> {
    let s = value.as_str()?.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

fn count(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
}

fn usage(value: &Value) -> Option<Usage> {
    Some(Usage {
        total_tokens: count(&value["totalTokens"])?,
        tool_uses: count(&value["toolUses"]),
    })
}

/// `mergeUsageMax`: field-wise maximum; a total-only frame keeps the rest.
fn merge_usage(current: Option<Usage>, incoming: Option<Usage>) -> Option<Usage> {
    match (current, incoming) {
        (current, None) => current,
        (None, incoming) => incoming,
        (Some(a), Some(b)) => Some(Usage {
            total_tokens: a.total_tokens.max(b.total_tokens),
            tool_uses: match (a.tool_uses, b.tool_uses) {
                (Some(x), Some(y)) => Some(x.max(y)),
                (x, y) => x.or(y),
            },
        }),
    }
}

/// `isBackgroundTaskActivity`: only rows stamped `agentKind: "agent"` join.
fn background(payload: &Value) -> bool {
    payload["agentKind"] != "agent"
}

fn kind_from(payload: &Value, id: &str) -> Kind {
    if payload["taskType"] == "subagent_batch" {
        Kind::Batch
    } else if text(&payload["taskType"]).as_deref() == Some("local_workflow") {
        Kind::Workflow
    } else if !payload["parentAgentId"].is_null() || id.contains(":wf:") {
        Kind::WorkflowAgent
    } else {
        Kind::Subagent
    }
}

fn get_or_create<'a>(
    agents: &'a mut Vec<Agent>,
    index: &mut HashMap<String, usize>,
    id: &str,
    payload: &Value,
    at: &str,
) -> &'a mut Agent {
    let position = *index.entry(id.to_owned()).or_insert_with(|| {
        agents.push(Agent {
            id: id.to_owned(),
            kind: kind_from(payload, id),
            title: text(&payload["title"])
                .or_else(|| text(&payload["detail"]))
                .unwrap_or_else(|| id.to_owned()),
            role: text(&payload["role"]),
            model: text(&payload["model"]),
            effort: text(&payload["effort"]),
            status: Status::Pending,
            activation_count: 0,
            usage: None,
            progress: None,
            last_tool_name: None,
            result: None,
            error: None,
            parent_agent_id: text(&payload["parentAgentId"]),
            agent_index: count(&payload["agentIndex"]),
            phase_index: count(&payload["phaseIndex"]),
            phase_title: text(&payload["phaseTitle"]),
            attempt: count(&payload["attempt"]),
            workflow_name: text(&payload["workflowName"]),
            phases: Vec::new(),
            first_seen_at: at.to_owned(),
            started_at: None,
            completed_at: None,
            updated_at: at.to_owned(),
        });
        agents.len() - 1
    });
    &mut agents[position]
}

/// `fillMetadata`: never downgrades known values.
fn fill_metadata(agent: &mut Agent, payload: &Value) {
    if payload["taskType"] == "subagent_batch" {
        agent.kind = Kind::Batch;
    }
    if let Some(title) = text(&payload["title"]) {
        agent.title = title;
    }
    if let Some(role) = text(&payload["role"]) {
        agent.role = Some(role);
    }
    if let Some(model) = text(&payload["model"]) {
        agent.model = Some(model);
    }
    if let Some(effort) = text(&payload["effort"]) {
        agent.effort = Some(effort);
    }
    if let Some(parent) = text(&payload["parentAgentId"]) {
        agent.parent_agent_id = Some(parent);
        if agent.kind == Kind::Subagent {
            agent.kind = Kind::WorkflowAgent;
        }
    }
    if let Some(name) = text(&payload["workflowName"]) {
        agent.workflow_name = Some(name);
    }
    if text(&payload["taskType"]).as_deref() == Some("local_workflow") {
        agent.kind = Kind::Workflow;
    }
    if let Some(index) = count(&payload["agentIndex"]) {
        agent.agent_index = Some(index);
    }
    if let Some(index) = count(&payload["phaseIndex"]) {
        agent.phase_index = Some(index);
    }
    if let Some(title) = text(&payload["phaseTitle"]) {
        agent.phase_title = Some(title);
    }
    if let Some(attempt) = count(&payload["attempt"]) {
        // A new attempt reactivates the slot: drop the old run's outcome.
        if agent.attempt.is_some_and(|previous| attempt > previous) {
            agent.result = None;
            agent.error = None;
            agent.completed_at = None;
        }
        agent.attempt = Some(attempt);
    }
    if let Some(entries) = payload["phases"].as_array() {
        let mut phases: Vec<(f64, String)> = entries
            .iter()
            .filter_map(|e| Some((count(&e["index"])?, text(&e["title"])?)))
            .collect();
        if !phases.is_empty() {
            phases.sort_by(|a, b| a.0.total_cmp(&b.0));
            agent.phases = phases;
        }
    }
}

/// `applyStatus`: duplicate terminal events are idempotent; leaving a
/// terminal or idle state for running is a reactivation.
fn apply_status(agent: &mut Agent, status: Status, at: &str) {
    let was_terminal = agent.status.terminal();
    if was_terminal && status.terminal() {
        return;
    }
    if (was_terminal || agent.status == Status::Idle)
        && matches!(status, Status::Running | Status::Pending)
    {
        agent.activation_count += 1;
        agent.result = None;
        agent.error = None;
        agent.completed_at = None;
        if status == Status::Running {
            agent.started_at = Some(at.to_owned());
        }
    }
    if status == Status::Running && agent.started_at.is_none() {
        agent.started_at = Some(at.to_owned());
    }
    if status.terminal() && agent.completed_at.is_none() {
        agent.completed_at = Some(at.to_owned());
    }
    agent.status = status;
}

/// `foldSubagentActivities`.
pub fn fold(activities: &[Activity], session_live: bool) -> Vec<Agent> {
    let mut agents: Vec<Agent> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for activity in activities {
        let payload = &activity.payload;
        if !payload.is_object() {
            continue;
        }
        let at = activity.created_at.as_str();
        let Some(task) = text(&payload["taskId"]) else {
            continue;
        };
        match activity.kind.as_str() {
            "task.started" => {
                if background(payload) {
                    continue;
                }
                let agent = get_or_create(&mut agents, &mut index, &task, payload, at);
                fill_metadata(agent, payload);
                if agent.activation_count == 0 && !agent.status.terminal() {
                    agent.activation_count = 1;
                    agent.started_at.get_or_insert_with(|| at.to_owned());
                    agent.status = Status::Running;
                } else if agent.status == Status::Idle {
                    apply_status(agent, Status::Running, at);
                }
                if let Some(detail) = text(&payload["detail"])
                    && agent.title == agent.id
                {
                    agent.title = detail;
                }
                agent.updated_at = at.to_owned();
            }
            "task.progress" => {
                let existed = index.contains_key(&task);
                if !existed && background(payload) {
                    continue;
                }
                let agent = get_or_create(&mut agents, &mut index, &task, payload, at);
                fill_metadata(agent, payload);
                if agent.activation_count == 0 {
                    agent.activation_count = 1;
                }
                if let Some(status) = Status::parse(&payload["status"]) {
                    apply_status(agent, status, at);
                } else if (payload["usageSnapshot"] != true || !existed)
                    && !agent.status.terminal()
                    && agent.status != Status::Idle
                {
                    apply_status(agent, Status::Running, at);
                }
                let summary = text(&payload["summary"]);
                if let Some(summary) = &summary {
                    agent.progress = Some(bounded(summary));
                }
                if let Some(tool) = text(&payload["lastToolName"]) {
                    agent.last_tool_name = Some(tool);
                }
                if let Some(error) = text(&payload["error"]) {
                    agent.error = Some(bounded(&error));
                }
                agent.usage = merge_usage(agent.usage.take(), usage(&payload["typedUsage"]));
                agent.updated_at = at.to_owned();
            }
            "task.updated" => {
                if !index.contains_key(&task) && background(payload) {
                    continue;
                }
                let agent = get_or_create(&mut agents, &mut index, &task, payload, at);
                fill_metadata(agent, payload);
                if let Some(detail) = text(&payload["detail"]) {
                    agent.progress = Some(bounded(&detail));
                }
                if agent.activation_count == 0 {
                    agent.activation_count = 1;
                }
                let was_terminal = agent.status.terminal();
                if let Some(status) = Status::parse(&payload["status"]) {
                    apply_status(agent, status, at);
                }
                if let Some(error) = text(&payload["error"]) {
                    agent.error = Some(bounded(&error));
                }
                if let Some(ended) = text(&payload["endedAt"])
                    && !was_terminal
                    && agent.status.terminal()
                {
                    agent.completed_at = Some(ended);
                }
                agent.updated_at = at.to_owned();
            }
            "task.completed" => {
                if !index.contains_key(&task) && background(payload) {
                    continue;
                }
                let agent = get_or_create(&mut agents, &mut index, &task, payload, at);
                fill_metadata(agent, payload);
                if agent.activation_count == 0 {
                    agent.activation_count = 1;
                }
                let summary = text(&payload["summary"]).or_else(|| text(&payload["detail"]));
                if agent.status.terminal() {
                    // Frozen status, but the completion still enriches.
                    if let Some(summary) = &summary {
                        if agent.status == Status::Failed {
                            agent.error.get_or_insert_with(|| bounded(summary));
                        } else {
                            agent.result.get_or_insert_with(|| bounded(summary));
                        }
                    }
                    agent.usage = merge_usage(agent.usage.take(), usage(&payload["typedUsage"]));
                    continue;
                }
                let status = match text(&payload["status"]).as_deref() {
                    Some("failed") => Status::Failed,
                    Some("stopped") => Status::Interrupted,
                    _ => Status::Completed,
                };
                apply_status(agent, status, at);
                if let Some(summary) = &summary {
                    if status == Status::Failed {
                        agent.error.get_or_insert_with(|| bounded(summary));
                    } else {
                        agent.result = Some(bounded(summary));
                    }
                }
                agent.usage = merge_usage(agent.usage.take(), usage(&payload["typedUsage"]));
                agent.updated_at = at.to_owned();
            }
            "tool.progress" => {
                let Some(&position) = index.get(&task) else {
                    continue;
                };
                let agent = &mut agents[position];
                if let Some(tool) = text(&payload["toolName"]) {
                    agent.last_tool_name = Some(tool);
                }
                agent.updated_at = at.to_owned();
            }
            _ => {}
        }
    }

    // A settled workflow settles members that never got their own terminal row.
    let settled_workflows: Vec<(String, Status, Option<String>, String)> = agents
        .iter()
        .filter(|a| a.kind == Kind::Workflow && a.status.terminal())
        .map(|a| {
            (
                a.id.clone(),
                a.status,
                a.completed_at.clone(),
                a.updated_at.clone(),
            )
        })
        .collect();
    for (id, status, completed, updated) in settled_workflows {
        for member in agents.iter_mut() {
            if member.parent_agent_id.as_deref() != Some(id.as_str())
                || member.status.terminal()
                || member.status == Status::Idle
            {
                continue;
            }
            member.status = if status == Status::Completed {
                Status::Completed
            } else {
                Status::Interrupted
            };
            member.completed_at = member
                .completed_at
                .clone()
                .or_else(|| completed.clone())
                .or_else(|| Some(updated.clone()));
            member.updated_at = updated.clone();
        }
    }
    // Session death orphans every live agent.
    if !session_live {
        for agent in agents.iter_mut().filter(|a| a.status.active()) {
            agent.status = Status::Interrupted;
            agent.completed_at = agent
                .completed_at
                .clone()
                .or_else(|| Some(agent.updated_at.clone()));
        }
    }
    if agents.len() > ROSTER_LIMIT {
        let rank = |a: &Agent| {
            if a.status.active() {
                0
            } else if a.status == Status::Idle {
                1
            } else {
                2
            }
        };
        agents.sort_by(|a, b| rank(a).cmp(&rank(b)).then(b.updated_at.cmp(&a.updated_at)));
        agents.truncate(ROSTER_LIMIT);
    }
    agents
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseState {
    Pending,
    Running,
    Done,
}

pub struct Phase {
    pub index: f64,
    pub title: String,
    pub members: Vec<Agent>,
    pub state: PhaseState,
    pub active: usize,
    pub settled: usize,
}

pub struct WorkflowGroup {
    pub workflow: Agent,
    pub phases: Vec<Phase>,
    pub unphased: Vec<Agent>,
}

#[derive(Default)]
pub struct PanelModel {
    pub workflows: Vec<WorkflowGroup>,
    pub direct: Vec<Agent>,
    pub running: usize,
    pub waiting: usize,
    pub idle: usize,
    pub settled: usize,
    pub total_tokens: f64,
    pub has_agents: bool,
}

/// `deriveAgentPanelModel`.
pub fn panel_model(agents: Vec<Agent>) -> PanelModel {
    if agents.is_empty() {
        return PanelModel::default();
    }
    let by_seen = |a: &Agent, b: &Agent| {
        a.first_seen_at
            .cmp(&b.first_seen_at)
            .then_with(|| a.id.cmp(&b.id))
    };
    let mut workflows: Vec<Agent> = agents
        .iter()
        .filter(|a| a.kind == Kind::Workflow)
        .cloned()
        .collect();
    workflows.sort_by(by_seen);
    let workflow_ids: Vec<&str> = workflows.iter().map(|w| w.id.as_str()).collect();
    let mut members: HashMap<String, Vec<Agent>> = HashMap::new();
    let mut direct: Vec<Agent> = Vec::new();
    for agent in &agents {
        if agent.kind == Kind::Workflow {
            continue;
        }
        match &agent.parent_agent_id {
            Some(parent) if workflow_ids.contains(&parent.as_str()) => {
                members
                    .entry(parent.clone())
                    .or_default()
                    .push(agent.clone());
            }
            _ => direct.push(agent.clone()),
        }
    }
    let by_index = |a: &Agent, b: &Agent| {
        a.agent_index
            .unwrap_or(0.0)
            .total_cmp(&b.agent_index.unwrap_or(0.0))
    };
    let groups = workflows
        .iter()
        .map(|workflow| {
            let workflow_members = members.get(&workflow.id).cloned().unwrap_or_default();
            let known: Vec<(f64, String)> = if !workflow.phases.is_empty() {
                workflow.phases.clone()
            } else {
                let mut derived: Vec<(f64, String)> = Vec::new();
                for member in &workflow_members {
                    if let Some(index) = member.phase_index
                        && !derived.iter().any(|(i, _)| *i == index)
                    {
                        let title = member
                            .phase_title
                            .clone()
                            .unwrap_or_else(|| format!("Phase {}", index + 1.0));
                        derived.push((index, title));
                    }
                }
                derived.sort_by(|a, b| a.0.total_cmp(&b.0));
                derived
            };
            let phases = known
                .iter()
                .map(|(index, title)| {
                    let mut phase_members: Vec<Agent> = workflow_members
                        .iter()
                        .filter(|m| m.phase_index == Some(*index))
                        .cloned()
                        .collect();
                    phase_members.sort_by(by_index);
                    let active = phase_members
                        .iter()
                        .filter(|m| m.status.active() || m.status == Status::Idle)
                        .count();
                    let settled = phase_members.iter().filter(|m| m.status.terminal()).count();
                    let state = if phase_members.is_empty() {
                        PhaseState::Pending
                    } else if active > 0 {
                        PhaseState::Running
                    } else if settled == phase_members.len() {
                        PhaseState::Done
                    } else {
                        PhaseState::Pending
                    };
                    Phase {
                        index: *index,
                        title: title.clone(),
                        members: phase_members,
                        state,
                        active,
                        settled,
                    }
                })
                .collect();
            let mut unphased: Vec<Agent> = workflow_members
                .iter()
                .filter(|m| {
                    m.phase_index
                        .is_none_or(|i| !known.iter().any(|(k, _)| *k == i))
                })
                .cloned()
                .collect();
            unphased.sort_by(by_index);
            WorkflowGroup {
                workflow: workflow.clone(),
                phases,
                unphased,
            }
        })
        .collect();
    let mut model = PanelModel {
        workflows: groups,
        has_agents: true,
        ..PanelModel::default()
    };
    for agent in &agents {
        // A coordinator with members is their container, not work of its own.
        if agent.kind == Kind::Workflow && members.get(&agent.id).is_some_and(|m| !m.is_empty()) {
            continue;
        }
        match agent.status {
            Status::Running | Status::Pending => model.running += 1,
            Status::Waiting => model.waiting += 1,
            Status::Idle => model.idle += 1,
            _ => model.settled += 1,
        }
        model.total_tokens += agent.usage.as_ref().map_or(0.0, |u| u.total_tokens);
    }
    direct.sort_by(by_seen);
    model.direct = direct;
    model
}

/// `formatSubagentModelLabel`: "claude-opus-4-20250514" reads "opus-4".
pub fn model_label(model: Option<&str>, effort: Option<&str>) -> Option<String> {
    let model = model?;
    let mut compact = model.strip_prefix("claude-").unwrap_or(model).to_owned();
    if let Some((head, tail)) = compact.rsplit_once('-')
        && tail.len() == 8
        && tail.chars().all(|c| c.is_ascii_digit())
    {
        compact = head.to_owned();
    }
    if let Some(head) = compact.strip_suffix("-latest") {
        compact = head.to_owned();
    }
    Some(match effort {
        Some(effort) => format!("{compact} · {effort}"),
        None => compact,
    })
}

/// `formatSubagentTokenCount`.
pub fn token_count(total: f64) -> String {
    if total < 1000.0 {
        format!("{total}")
    } else if total < 1_000_000.0 {
        let value = total / 1000.0;
        if value >= 100.0 {
            format!("{}k", value.round())
        } else {
            format!("{value:.1}k")
        }
    } else {
        format!("{:.1}M", total / 1_000_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn activity(kind: &str, at: &str, payload: Value) -> Activity {
        Activity {
            kind: kind.into(),
            created_at: at.into(),
            payload,
            ..Activity::default()
        }
    }

    #[test]
    fn agents_fold_from_task_activities() {
        let activities = vec![
            activity(
                "task.started",
                "1",
                json!({"taskId": "a", "agentKind": "agent", "title": "Explore", "model": "claude-opus-4-20250514"}),
            ),
            activity(
                "task.started",
                "2",
                json!({"taskId": "shell", "title": "Run"}),
            ),
            activity(
                "task.progress",
                "3",
                json!({"taskId": "a", "summary": "Reading files", "typedUsage": {"totalTokens": 1200}}),
            ),
            activity(
                "task.completed",
                "4",
                json!({"taskId": "a", "summary": "Done"}),
            ),
            activity(
                "task.completed",
                "5",
                json!({"taskId": "a", "summary": "Again"}),
            ),
        ];
        let agents = fold(&activities, true);
        assert_eq!(agents.len(), 1, "background tasks stay out");
        let agent = &agents[0];
        assert_eq!(agent.status, Status::Completed);
        assert_eq!(agent.result.as_deref(), Some("Done"));
        assert_eq!(agent.completed_at.as_deref(), Some("4"));
        assert_eq!(
            model_label(agent.model.as_deref(), None).as_deref(),
            Some("opus-4")
        );
        let model = panel_model(agents);
        assert_eq!((model.settled, model.total_tokens), (1, 1200.0));
        assert_eq!(token_count(1200.0), "1.2k");
        assert_eq!(token_count(250_000.0), "250k");
    }

    #[test]
    fn a_dead_session_interrupts_live_agents() {
        let activities = vec![activity(
            "task.started",
            "1",
            json!({"taskId": "a", "agentKind": "agent"}),
        )];
        assert_eq!(fold(&activities, false)[0].status, Status::Interrupted);
        assert_eq!(fold(&activities, true)[0].status, Status::Running);
    }
}
