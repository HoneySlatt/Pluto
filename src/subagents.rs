//! Subagent observability (`client-runtime/state/subagentRuntime.ts`): the
//! thread's projected subagents and their workflow members as a roster
//! (`projectedSubagentsToRuntime`), and the panel model the Agents surface
//! renders.
use std::collections::HashMap;

use serde_json::Value;

use crate::model::Subagent;

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

fn count(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
}

/// `workflowUsage`.
fn workflow_usage(source: &Value) -> Option<Usage> {
    Some(Usage {
        total_tokens: count(&source["totalTokens"])?,
        tool_uses: count(&source["toolCalls"]),
    })
}

fn iso_from_millis(value: &Value) -> Option<String> {
    let ms = value.as_i64()?;
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// `workflowMembersToRuntime`: a coordinator's workflow roster as member
/// rows, keyed by the coordinator and the spawn ordinal.
fn workflow_members(coordinator: &Agent, workflow: &Value) -> Vec<Agent> {
    workflow["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|member| {
            let state = member["state"].as_str().unwrap_or("queued");
            let status = if matches!(state, "queued" | "running") && coordinator.status.terminal() {
                coordinator.status
            } else {
                Status::parse(&Value::from(if state == "queued" {
                    "pending"
                } else {
                    state
                }))
                .unwrap_or(Status::Pending)
            };
            let failed = status == Status::Failed;
            let started_at = iso_from_millis(&member["startedAt"]);
            let completed_at = if status.terminal() {
                match (member["startedAt"].as_i64(), member["durationMs"].as_i64()) {
                    (Some(start), Some(duration)) => {
                        iso_from_millis(&Value::from(start + duration))
                    }
                    _ => coordinator.completed_at.clone(),
                }
            } else {
                None
            };
            let result = member["result"].as_str().map(str::to_owned);
            let first_seen_at = iso_from_millis(&member["queuedAt"])
                .or_else(|| started_at.clone())
                .unwrap_or_else(|| coordinator.first_seen_at.clone());
            Agent {
                id: format!(
                    "{}:agent:{}",
                    coordinator.id,
                    member["index"].as_u64().unwrap_or(0)
                ),
                kind: Kind::WorkflowAgent,
                title: member["label"].as_str().unwrap_or_default().to_owned(),
                model: member["model"].as_str().map(str::to_owned),
                status,
                activation_count: member["attempt"].as_u64().unwrap_or(1) as u32,
                usage: workflow_usage(member),
                // A member keeps its prompt as the question half of its
                // conversation.
                progress: member["prompt"].as_str().map(str::to_owned),
                result: if failed { None } else { result.clone() },
                error: if failed { result } else { None },
                parent_agent_id: Some(coordinator.id.clone()),
                agent_index: count(&member["index"]),
                phase_index: count(&member["phaseIndex"]),
                phase_title: member["phaseTitle"].as_str().map(str::to_owned),
                attempt: count(&member["attempt"]),
                phases: Vec::new(),
                updated_at: completed_at
                    .clone()
                    .or_else(|| started_at.clone())
                    .unwrap_or_else(|| coordinator.first_seen_at.clone()),
                first_seen_at,
                started_at,
                completed_at,
                ..coordinator.clone()
            }
        })
        .collect()
}

/// `projectedSubagentsToRuntime`.
pub fn from_projection(subagents: &[Subagent]) -> Vec<Agent> {
    let mut agents = Vec::new();
    for subagent in subagents {
        let workflow = subagent.workflow.as_ref().filter(|w| w.is_object());
        let title = subagent.title.clone().unwrap_or_else(|| {
            if subagent.prompt.chars().count() > 80 {
                format!(
                    "{}...",
                    subagent.prompt.chars().take(77).collect::<String>()
                )
            } else {
                subagent.prompt.clone()
            }
        });
        let status =
            Status::parse(&Value::from(subagent.status.as_str())).unwrap_or(Status::Pending);
        let phases = workflow
            .and_then(|w| w["phases"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|p| Some((count(&p["index"])?, p["title"].as_str()?.to_owned())))
            .collect();
        let coordinator = Agent {
            id: subagent.id.clone(),
            kind: if workflow.is_some() {
                Kind::Workflow
            } else {
                Kind::Subagent
            },
            title,
            role: None,
            model: subagent.model.clone(),
            effort: None,
            status,
            activation_count: 1,
            usage: workflow.and_then(workflow_usage),
            progress: subagent.progress.clone(),
            last_tool_name: None,
            result: subagent.result.clone(),
            error: (status == Status::Failed)
                .then(|| subagent.result.clone())
                .flatten(),
            parent_agent_id: None,
            agent_index: None,
            phase_index: None,
            phase_title: None,
            attempt: None,
            workflow_name: workflow.and_then(|w| w["name"].as_str().map(str::to_owned)),
            phases,
            first_seen_at: subagent
                .started_at
                .clone()
                .unwrap_or_else(|| subagent.updated_at.clone()),
            started_at: subagent.started_at.clone(),
            completed_at: subagent.completed_at.clone(),
            updated_at: subagent.updated_at.clone(),
        };
        let members = workflow.map(|w| workflow_members(&coordinator, w));
        agents.push(coordinator);
        agents.extend(members.into_iter().flatten());
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

    #[test]
    fn projected_subagents_become_the_roster() {
        let subagents: Vec<Subagent> = serde_json::from_value(json!([
            {"id": "a", "parentNodeId": "n", "origin": "provider_native", "driver": "claude",
             "providerInstanceId": "claude", "prompt": "Explore the repository", "title": "Explore",
             "model": "claude-opus-4-20250514", "status": "completed", "result": "Done",
             "startedAt": "1", "completedAt": "4", "updatedAt": "4"},
            {"id": "w", "parentNodeId": "n", "origin": "app_owned", "driver": "claude",
             "providerInstanceId": "claude", "prompt": "Review", "title": null, "model": null,
             "status": "running", "result": null, "startedAt": "2", "completedAt": null,
             "updatedAt": "5", "workflow": {"name": "review", "phases": [{"index": 0, "title": "Read"}],
             "agents": [{"index": 0, "label": "Reader", "state": "completed", "phaseIndex": 0,
             "totalTokens": 1200}], "totalTokens": 1200}},
        ]))
        .unwrap();
        let agents = from_projection(&subagents);
        assert_eq!(agents.len(), 3);
        assert_eq!(agents[0].status, Status::Completed);
        assert_eq!(
            model_label(agents[0].model.as_deref(), None).as_deref(),
            Some("opus-4")
        );
        assert_eq!(agents[1].kind, Kind::Workflow);
        assert_eq!(agents[2].id, "w:agent:0");
        assert_eq!(agents[2].parent_agent_id.as_deref(), Some("w"));
        let model = panel_model(agents);
        assert_eq!(model.workflows.len(), 1);
        assert_eq!(token_count(1200.0), "1.2k");
        assert_eq!(token_count(250_000.0), "250k");
    }
}
