//! Pending provider requests, ported from upstream
//! `client-runtime/pendingRequests.ts` and `pendingUserInput.ts`.
use std::collections::{BTreeMap, HashSet};

use serde_json::{Value, json};

use crate::model::Activity;

#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalOption {
    pub decision: String,
    pub label: String,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingApproval {
    pub request_id: String,
    pub request_kind: String,
    pub created_at: String,
    pub detail: Option<String>,
    pub app_name: Option<String>,
    pub options: Vec<ApprovalOption>,
}

impl PendingApproval {
    /// `ComposerPendingApprovalPanel` fallback label.
    pub fn label(&self) -> &'static str {
        match self.request_kind.as_str() {
            "mcp-elicitation" => "App access approval",
            "command" => "Command approval",
            "file-read" => "File read approval",
            "permission" => "App permission approval",
            _ => "File change approval",
        }
    }

    /// Provider options, or the upstream defaults.
    pub fn options(&self) -> Vec<ApprovalOption> {
        if !self.options.is_empty() {
            return self.options.clone();
        }
        [
            ("cancel", "Cancel"),
            ("decline", "Decline"),
            ("acceptForSession", "Always allow this session"),
            ("accept", "Approve"),
        ]
        .into_iter()
        .map(|(decision, label)| ApprovalOption {
            decision: decision.into(),
            label: label.into(),
            warning: None,
        })
        .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuestionOption {
    pub label: String,
    pub value: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
    pub allow_custom_answer: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingUserInput {
    pub request_id: String,
    pub created_at: String,
    pub questions: Vec<Question>,
    pub dismissible: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pending {
    pub approvals: Vec<PendingApproval>,
    pub user_inputs: Vec<PendingUserInput>,
}

fn request_kind_from_type(request_type: &str) -> Option<&'static str> {
    Some(match request_type {
        "command_execution_approval" | "exec_command_approval" | "dynamic_tool_call" => "command",
        "file_read_approval" => "file-read",
        "file_change_approval" | "apply_patch_approval" => "file-change",
        "mcp_elicitation_approval" => "mcp-elicitation",
        "permission_approval" => "permission",
        _ => return None,
    })
}

fn text(value: &Value) -> Option<String> {
    value.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
}

fn parse_questions(value: &Value) -> Vec<Question> {
    let Some(list) = value.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|q| {
            let options: Vec<QuestionOption> = q["options"]
                .as_array()?
                .iter()
                .filter_map(|o| {
                    let label = o["label"].as_str()?.to_owned();
                    Some(QuestionOption {
                        value: o["value"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or(label.clone()),
                        label,
                        description: text(&o["description"]),
                    })
                })
                .collect();
            let allow_custom_answer = q["allowCustomAnswer"].as_bool() != Some(false);
            if options.is_empty() && !allow_custom_answer {
                return None;
            }
            Some(Question {
                id: q["id"].as_str()?.to_owned(),
                header: q["header"].as_str()?.to_owned(),
                question: q["question"].as_str()?.to_owned(),
                options,
                multi_select: q["multiSelect"] == true,
                allow_custom_answer,
            })
        })
        .collect()
}

const STALE_APPROVAL: [&str; 4] = [
    "stale pending approval request",
    "unknown pending approval request",
    "unknown pending permission request",
    "unknown pending codex approval request",
];
const STALE_USER_INPUT: [&str; 4] = [
    "stale pending user-input request",
    "unknown pending user-input request",
    "unknown pending user input request",
    "unknown pending codex user input request",
];

fn is_stale(payload: &Value, fragments: &[&str]) -> bool {
    let detail = payload["detail"]
        .as_str()
        .unwrap_or_default()
        .to_lowercase();
    fragments.iter().any(|f| detail.contains(f))
}

/// Port of `derivePendingRequests`: terminal events stay final regardless of order.
pub fn derive(activities: &[Activity]) -> Pending {
    let mut approvals: BTreeMap<String, PendingApproval> = BTreeMap::new();
    let mut inputs: BTreeMap<String, PendingUserInput> = BTreeMap::new();
    let mut closed_approvals = HashSet::new();
    let mut closed_inputs = HashSet::new();
    for activity in activities {
        let payload = &activity.payload;
        let Some(request_id) = payload["requestId"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        let request_id = request_id.to_owned();
        match activity.kind.as_str() {
            "approval.requested" => {
                let request_type = payload["requestType"].as_str().unwrap_or_default();
                if closed_approvals.contains(&request_id)
                    || request_type == "tool_user_input"
                    || request_type == "auth_tokens_refresh"
                {
                    continue;
                }
                let kind = payload["requestKind"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| request_kind_from_type(request_type).map(str::to_owned))
                    .unwrap_or_else(|| "command".into());
                let options = payload["options"]
                    .as_array()
                    .map(|list| {
                        list.iter()
                            .filter_map(|o| {
                                Some(ApprovalOption {
                                    decision: o["decision"].as_str()?.to_owned(),
                                    label: o["label"].as_str()?.to_owned(),
                                    warning: text(&o["warning"]),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                approvals.insert(
                    request_id.clone(),
                    PendingApproval {
                        request_id,
                        request_kind: kind,
                        created_at: activity.created_at.clone(),
                        detail: text(&payload["detail"]),
                        app_name: text(&payload["appName"]),
                        options,
                    },
                );
            }
            "user-input.requested" => {
                if closed_inputs.contains(&request_id) {
                    continue;
                }
                let questions = parse_questions(&payload["questions"]);
                if questions.is_empty() {
                    continue;
                }
                inputs.insert(
                    request_id.clone(),
                    PendingUserInput {
                        request_id,
                        created_at: activity.created_at.clone(),
                        questions,
                        dismissible: payload["responseMode"] == "message",
                    },
                );
            }
            "approval.resolved" => {
                approvals.remove(&request_id);
                closed_approvals.insert(request_id);
            }
            "provider.approval.respond.failed" if is_stale(payload, &STALE_APPROVAL) => {
                approvals.remove(&request_id);
                closed_approvals.insert(request_id);
            }
            "user-input.resolved" => {
                inputs.remove(&request_id);
                closed_inputs.insert(request_id);
            }
            "provider.user-input.respond.failed" if is_stale(payload, &STALE_USER_INPUT) => {
                inputs.remove(&request_id);
                closed_inputs.insert(request_id);
            }
            _ => {}
        }
    }
    let mut pending = Pending {
        approvals: approvals.into_values().collect(),
        user_inputs: inputs.into_values().collect(),
    };
    pending
        .approvals
        .sort_by(|a, b| a.created_at.cmp(&b.created_at));
    pending
        .user_inputs
        .sort_by(|a, b| a.created_at.cmp(&b.created_at));
    pending
}

/// Draft answer for one question (`PendingUserInputDraftAnswer`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftAnswer {
    pub selected: Vec<String>,
    pub custom: String,
}

/// `resolvePendingUserInputAnswer`: a custom answer outranks selected options.
pub fn resolve_answer(question: &Question, draft: Option<&DraftAnswer>) -> Option<Value> {
    let custom = draft.map(|d| d.custom.trim()).unwrap_or_default();
    if question.allow_custom_answer && !custom.is_empty() {
        return Some(json!(custom));
    }
    let selected: Vec<&String> = draft
        .map(|d| {
            d.selected
                .iter()
                .filter(|v| question.options.iter().any(|o| &o.value == *v))
                .collect()
        })
        .unwrap_or_default();
    if question.multi_select {
        (!selected.is_empty()).then(|| json!(selected))
    } else {
        selected.first().map(|v| json!(v))
    }
}

/// `togglePendingUserInputOptionSelection`.
pub fn toggle_option(question: &Question, draft: &mut DraftAnswer, value: &str) {
    draft.custom.clear();
    if question.multi_select {
        if let Some(index) = draft.selected.iter().position(|v| v == value) {
            draft.selected.remove(index);
        } else {
            draft.selected.push(value.to_owned());
        }
    } else {
        draft.selected = vec![value.to_owned()];
    }
}

/// `buildPendingUserInputAnswers`: every question needs an answer.
pub fn build_answers(
    questions: &[Question],
    drafts: &BTreeMap<String, DraftAnswer>,
) -> Option<serde_json::Map<String, Value>> {
    let mut answers = serde_json::Map::new();
    for question in questions {
        answers.insert(
            question.id.clone(),
            resolve_answer(question, drafts.get(&question.id))?,
        );
    }
    Some(answers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(kind: &str, payload: Value, at: &str) -> Activity {
        Activity {
            id: format!("{kind}-{at}"),
            kind: kind.into(),
            payload,
            created_at: at.into(),
            ..Default::default()
        }
    }

    #[test]
    fn resolved_requests_stay_closed_even_out_of_order() {
        let activities = vec![
            activity("approval.resolved", json!({"requestId": "a"}), "1"),
            activity(
                "approval.requested",
                json!({"requestId": "a", "requestType": "exec_command_approval", "detail": "ls"}),
                "2",
            ),
            activity(
                "approval.requested",
                json!({"requestId": "b", "requestKind": "file-change"}),
                "3",
            ),
        ];
        let pending = derive(&activities);
        assert_eq!(pending.approvals.len(), 1);
        assert_eq!(pending.approvals[0].label(), "File change approval");
    }

    #[test]
    fn answers_prefer_custom_text() {
        let question = Question {
            id: "q".into(),
            header: "H".into(),
            question: "Q?".into(),
            options: vec![QuestionOption {
                label: "A".into(),
                value: "A".into(),
                description: None,
            }],
            multi_select: false,
            allow_custom_answer: true,
        };
        let mut draft = DraftAnswer::default();
        toggle_option(&question, &mut draft, "A");
        assert_eq!(resolve_answer(&question, Some(&draft)), Some(json!("A")));
        draft.custom = "Other".into();
        assert_eq!(
            resolve_answer(&question, Some(&draft)),
            Some(json!("Other"))
        );
    }
}
