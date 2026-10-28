//! Pending provider requests, ported from upstream
//! `client-runtime/state/threadRequests.ts` and `pendingUserInput.ts`.
use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::model::Detail;

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

/// Port of `derivePendingThreadRequests`: pending runtime requests, joined
/// to the request items that carry what they show.
pub fn derive(detail: &Detail) -> Pending {
    let mut pending = Pending::default();
    let item = |kind: &str, id: &str| {
        detail
            .turn_items
            .iter()
            .rev()
            .find(|i| i.kind == kind && i.str("requestId") == Some(id))
            .cloned()
    };
    for request in &detail.runtime_requests {
        if request.status != "pending" {
            continue;
        }
        let capability = request.response_capability["type"]
            .as_str()
            .unwrap_or_default();
        if request.kind == "user_input" {
            let Some(item) = item("user_input_request", &request.id) else {
                continue;
            };
            let questions = parse_questions(item.get("questions"));
            pending.user_inputs.push(PendingUserInput {
                request_id: request.id.clone(),
                created_at: request.created_at.clone(),
                questions,
                dismissible: item.str("responseMode") == Some("message") || capability == "message",
            });
            continue;
        }
        if request.kind == "auth_refresh" || request.kind == "dynamic_tool_call" {
            continue;
        }
        let item = item("approval_request", &request.id);
        let field = |key: &str| item.as_ref().and_then(|i| text(i.get(key)));
        let options = item
            .as_ref()
            .and_then(|i| i.get("options").as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|o| {
                Some(ApprovalOption {
                    decision: o["decision"].as_str()?.to_owned(),
                    label: o["label"].as_str()?.to_owned(),
                    warning: text(&o["warning"]),
                })
            })
            .collect();
        pending.approvals.push(PendingApproval {
            request_id: request.id.clone(),
            request_kind: request.kind.clone(),
            created_at: request.created_at.clone(),
            detail: field("prompt"),
            app_name: field("appName"),
            options,
        });
    }
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

    fn detail() -> Detail {
        let request = |id: &str, kind: &str, status: &str, at: &str| {
            json!({"id":id,"nodeId":"n","providerTurnId":null,"nativeRequestRef":null,
                "kind":kind,"status":status,"responseCapability":{"type":"live",
                "providerSessionId":"s"},"createdAt":at,"resolvedAt":null})
        };
        let item = |id: &str, kind: &str, request: &str, extra: Value| {
            let mut item = json!({"id":id,"threadId":"t","type":kind,"requestId":request,
                "ordinal":0,"status":"waiting","updatedAt":"1"});
            for (key, value) in extra.as_object().unwrap() {
                item[key] = value.clone();
            }
            item
        };
        serde_json::from_value(json!({
            "thread": {"id": "t"},
            "runtimeRequests": [
                request("a", "command", "resolved", "1"),
                request("b", "file-change", "pending", "3"),
                request("c", "user_input", "pending", "2"),
                request("d", "auth_refresh", "pending", "4"),
            ],
            "turnItems": [
                item("i1", "approval_request", "b", json!({"requestKind":"file-change","prompt":"Edit app.rs"})),
                item("i2", "user_input_request", "c", json!({"questions":[{"id":"q","header":"H",
                    "question":"Which?","options":[{"label":"A","description":"a"}]}]})),
            ],
        }))
        .unwrap()
    }

    #[test]
    fn pending_requests_join_their_items() {
        let pending = derive(&detail());
        assert_eq!(pending.approvals.len(), 1);
        assert_eq!(pending.approvals[0].label(), "File change approval");
        assert_eq!(pending.approvals[0].detail.as_deref(), Some("Edit app.rs"));
        assert_eq!(pending.user_inputs.len(), 1);
        assert_eq!(pending.user_inputs[0].questions[0].question, "Which?");
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
