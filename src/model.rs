//! Client state: the shell projection (projects and thread summaries) and the
//! selected thread detail. Events are applied to raw JSON exactly like the
//! upstream reducer (`client-runtime/state/threadReducer.ts`), then decoded
//! into typed views for rendering.
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelSelection {
    pub instance_id: String,
    pub model: String,
    pub options: Option<Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub id: String,
    pub title: String,
    pub workspace_root: String,
    pub project_icon: Option<Value>,
    pub favicon_path: Option<String>,
    /// `RepositoryIdentity` (canonical key, root path, names), kept as sent.
    pub repository_identity: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct LatestTurn {
    pub turn_id: String,
    pub state: String,
    pub requested_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub assistant_message_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Session {
    pub status: String,
    pub provider_name: Option<String>,
    pub provider_instance_id: Option<String>,
    pub active_turn_id: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

/// Thread shell summary, or the shell part of a thread detail.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Thread {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub model_selection: ModelSelection,
    pub runtime_mode: String,
    pub interaction_mode: String,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    pub latest_turn: Option<LatestTurn>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub settled_override: Option<String>,
    pub settled_at: Option<String>,
    pub unsettled_at: Option<String>,
    pub snoozed_until: Option<String>,
    pub snoozed_at: Option<String>,
    pub auto_settle_disabled_at: Option<String>,
    pub pinned_at: Option<String>,
    pub pin_order_key: Option<String>,
    pub active_order_key: Option<String>,
    pub session: Option<Session>,
    pub latest_user_message_at: Option<String>,
    pub has_pending_approvals: bool,
    pub has_pending_user_input: bool,
    pub background_liveness: Option<String>,
    pub messages: Vec<Message>,
    pub activities: Vec<Activity>,
    pub checkpoints: Vec<Checkpoint>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub text: String,
    /// `ChatAttachment`s (an open union, kept as sent).
    pub attachments: Vec<Value>,
    pub turn_id: Option<String>,
    pub streaming: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Activity {
    pub id: String,
    pub tone: String,
    pub kind: String,
    pub summary: String,
    pub payload: Value,
    pub turn_id: Option<String>,
    pub sequence: Option<u64>,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CheckpointFile {
    pub path: String,
    pub kind: String,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Checkpoint {
    pub turn_id: String,
    pub checkpoint_turn_count: u64,
    pub status: String,
    pub files: Vec<CheckpointFile>,
    pub assistant_message_id: Option<String>,
    pub completed_at: String,
}

impl Thread {
    pub fn is_running(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|s| s.status == "running" || s.status == "starting")
    }
}

#[derive(Default)]
pub struct State {
    pub projects: Vec<Project>,
    pub threads: Vec<Thread>,
    pub detail: Option<Thread>,
    pub config: Value,
    shell_raw: Vec<Value>,
    detail_raw: Option<Value>,
    shell_sequence: Option<u64>,
    thread_sequence: Option<u64>,
}

fn decode<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, String> {
    serde_json::from_value(value.clone()).map_err(|e| e.to_string())
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

    pub fn shell(&mut self, item: &Value) -> Result<(), String> {
        if item["kind"] == "snapshot" {
            let snapshot = &item["snapshot"];
            // Decode both collections before replacing visible state.
            let projects = decode(&snapshot["projects"])?;
            let raw: Vec<Value> = snapshot["threads"].as_array().cloned().unwrap_or_default();
            let threads = raw.iter().map(decode).collect::<Result<Vec<Thread>, _>>()?;
            self.projects = projects;
            self.shell_raw = raw;
            self.threads = threads;
            self.shell_sequence = snapshot["snapshotSequence"].as_u64();
            return Ok(());
        }
        if item["kind"] == "synchronized" {
            return Ok(());
        }
        let sequence = item["sequence"]
            .as_u64()
            .ok_or("Missing shell event sequence")?;
        if self.shell_sequence.is_some_and(|s| sequence <= s) {
            return Ok(());
        }
        match item["kind"].as_str() {
            Some("project-upserted") => {
                let project: Project = decode(&item["project"])?;
                upsert(&mut self.projects, project, |p| &p.id);
            }
            Some("project-removed") => self.projects.retain(|p| p.id != item["projectId"]),
            Some("thread-upserted") => {
                let thread: Thread = decode(&item["thread"])?;
                if let Some(index) = self.threads.iter().position(|t| t.id == thread.id) {
                    self.shell_raw[index] = item["thread"].clone();
                    self.threads[index] = thread;
                } else {
                    self.shell_raw.push(item["thread"].clone());
                    self.threads.push(thread);
                }
            }
            Some("thread-removed") => {
                if let Some(index) = self.threads.iter().position(|t| t.id == item["threadId"]) {
                    self.threads.remove(index);
                    self.shell_raw.remove(index);
                }
            }
            _ => return Err("Unsupported shell event; reconnect with a compatible server".into()),
        }
        self.shell_sequence = Some(sequence);
        Ok(())
    }

    pub fn clear_detail(&mut self) {
        self.detail = None;
        self.detail_raw = None;
        self.thread_sequence = None;
    }

    pub fn thread(&mut self, selected: &str, item: &Value) -> Result<(), String> {
        if item["kind"] == "snapshot" {
            let raw = item["snapshot"]["thread"].clone();
            if raw["id"] == selected {
                self.detail = Some(decode(&raw)?);
                self.detail_raw = Some(raw);
                self.thread_sequence = item["snapshot"]["snapshotSequence"].as_u64();
            }
            return Ok(());
        }
        if item["kind"] == "synchronized" {
            return Ok(());
        }
        let event = &item["event"];
        let sequence = event["sequence"]
            .as_u64()
            .ok_or("Missing thread event sequence")?;
        if self.thread_sequence.is_some_and(|s| sequence <= s) {
            return Ok(());
        }
        self.thread_sequence = Some(sequence);
        let Some(raw) = self.detail_raw.as_mut().filter(|t| t["id"] == selected) else {
            return Ok(());
        };
        if event["payload"]["threadId"] != selected {
            return Ok(());
        }
        match apply_thread_event(raw, event) {
            Applied::Deleted => self.clear_detail(),
            Applied::Updated => self.detail = Some(decode(raw)?),
            Applied::Unchanged => {}
        }
        Ok(())
    }
}

enum Applied {
    Updated,
    Deleted,
    Unchanged,
}

fn set_present(thread: &mut Value, payload: &Value, keys: &[&str]) {
    for key in keys {
        if let Some(value) = payload.get(*key) {
            thread[*key] = value.clone();
        }
    }
}

fn settled_turn_state(status: &str) -> Option<&'static str> {
    match status {
        "idle" | "ready" => Some("completed"),
        "error" => Some("error"),
        "interrupted" | "stopped" => Some("interrupted"),
        _ => None,
    }
}

fn checkpoint_turn_state(status: &str) -> &'static str {
    if status == "error" {
        "error"
    } else {
        "completed"
    }
}

/// Port of `applyThreadDetailEvent` for the events that change what the chat renders.
fn apply_thread_event(thread: &mut Value, event: &Value) -> Applied {
    let payload = &event["payload"];
    let occurred_at = event["occurredAt"].clone();
    let updated_at = payload
        .get("updatedAt")
        .cloned()
        .unwrap_or(occurred_at.clone());
    match event["type"].as_str().unwrap_or_default() {
        "thread.deleted" => return Applied::Deleted,
        "thread.archived" => {
            thread["archivedAt"] = payload["archivedAt"].clone();
            thread["updatedAt"] = updated_at;
        }
        "thread.unarchived" => {
            thread["archivedAt"] = Value::Null;
            thread["updatedAt"] = updated_at;
        }
        "thread.settled" => {
            thread["settledOverride"] = json!("settled");
            thread["settledAt"] = payload["settledAt"].clone();
            thread["unsettledAt"] = Value::Null;
            thread["activeOrderKey"] = Value::Null;
            thread["updatedAt"] = updated_at;
        }
        "thread.unsettled" => {
            let pinned_active = thread["settledOverride"] == "active";
            thread["settledOverride"] = if payload["reason"] == "user" {
                json!("active")
            } else {
                Value::Null
            };
            thread["settledAt"] = Value::Null;
            if !pinned_active {
                thread["unsettledAt"] = updated_at.clone();
            }
            thread["updatedAt"] = updated_at;
        }
        "thread.snoozed" => {
            set_present(thread, payload, &["snoozedUntil", "snoozedAt"]);
            thread["updatedAt"] = updated_at;
        }
        "thread.unsnoozed" => {
            thread["snoozedUntil"] = Value::Null;
            thread["snoozedAt"] = Value::Null;
            thread["updatedAt"] = updated_at;
        }
        "thread.auto-settle-set" => {
            thread["autoSettleDisabledAt"] = payload["autoSettleDisabledAt"].clone();
            thread["updatedAt"] = updated_at;
        }
        "thread.pinned" => {
            set_present(thread, payload, &["pinnedAt", "pinOrderKey"]);
            thread["updatedAt"] = updated_at;
        }
        "thread.unpinned" => {
            thread["pinnedAt"] = Value::Null;
            thread["pinOrderKey"] = Value::Null;
            thread["updatedAt"] = updated_at;
        }
        "thread.meta-updated" => {
            set_present(
                thread,
                payload,
                &[
                    "title",
                    "titleState",
                    "titleRegeneration",
                    "modelSelection",
                    "branch",
                    "worktreePath",
                    "linkedPullRequest",
                    "branchPullRequest",
                    "activeOrderKey",
                ],
            );
            thread["updatedAt"] = updated_at;
        }
        "thread.runtime-mode-set" => {
            thread["runtimeMode"] = payload["runtimeMode"].clone();
            thread["updatedAt"] = updated_at;
        }
        "thread.interaction-mode-set" => {
            thread["interactionMode"] = payload["interactionMode"].clone();
            thread["updatedAt"] = updated_at;
        }
        "thread.turn-start-requested" => {
            set_present(thread, payload, &["modelSelection"]);
            thread["runtimeMode"] = payload["runtimeMode"].clone();
            thread["interactionMode"] = payload["interactionMode"].clone();
            thread["updatedAt"] = occurred_at;
        }
        "thread.turn-interrupt-requested" => {
            let turn_id = &payload["turnId"];
            if turn_id.is_null() || thread["latestTurn"]["turnId"] != *turn_id {
                return Applied::Unchanged;
            }
            let latest = &mut thread["latestTurn"];
            latest["state"] = json!("interrupted");
            if latest["startedAt"].is_null() {
                latest["startedAt"] = payload["createdAt"].clone();
            }
            if latest["completedAt"].is_null() {
                latest["completedAt"] = payload["createdAt"].clone();
            }
            thread["updatedAt"] = occurred_at;
        }
        "thread.message-sent" => apply_message(thread, payload, occurred_at),
        "thread.session-set" => {
            let session = &payload["session"];
            let status = session["status"].as_str().unwrap_or_default();
            let latest = thread["latestTurn"].clone();
            if status == "running" && !session["activeTurnId"].is_null() {
                let same = latest["turnId"] == session["activeTurnId"];
                thread["latestTurn"] = json!({
                    "turnId": session["activeTurnId"],
                    "state": "running",
                    "requestedAt": if same { latest["requestedAt"].clone() } else { session["updatedAt"].clone() },
                    "startedAt": if same && !latest["startedAt"].is_null() { latest["startedAt"].clone() } else { session["updatedAt"].clone() },
                    "completedAt": null,
                    "assistantMessageId": if same { latest["assistantMessageId"].clone() } else { Value::Null },
                });
            } else if latest["state"] == "running"
                && let Some(state) = settled_turn_state(status)
            {
                thread["latestTurn"]["state"] = json!(state);
                thread["latestTurn"]["completedAt"] = session["updatedAt"].clone();
            }
            thread["session"] = session.clone();
            thread["updatedAt"] = occurred_at;
        }
        "thread.session-stop-requested" => {
            if thread["session"].is_null() {
                return Applied::Unchanged;
            }
            thread["session"]["status"] = json!("stopped");
            thread["session"]["activeTurnId"] = Value::Null;
            thread["session"]["updatedAt"] = payload["createdAt"].clone();
            thread["updatedAt"] = occurred_at;
        }
        "thread.turn-diff-completed" => {
            let checkpoint = json!({
                "turnId": payload["turnId"],
                "checkpointTurnCount": payload["checkpointTurnCount"],
                "checkpointRef": payload["checkpointRef"],
                "status": payload["status"],
                "files": payload["files"],
                "assistantMessageId": payload["assistantMessageId"],
                "completedAt": payload["completedAt"],
            });
            let checkpoints = array_mut(thread, "checkpoints");
            if checkpoints
                .iter()
                .any(|c| c["turnId"] == payload["turnId"] && c["status"] != "missing")
                && payload["status"] == "missing"
            {
                return Applied::Unchanged;
            }
            checkpoints.retain(|c| c["turnId"] != payload["turnId"]);
            checkpoints.push(checkpoint);
            checkpoints.sort_by_key(|c| c["checkpointTurnCount"].as_u64().unwrap_or(u64::MAX));
            let running = thread["session"]["status"] == "running"
                && thread["session"]["activeTurnId"] == payload["turnId"];
            let latest = thread["latestTurn"].clone();
            if !running && (latest.is_null() || latest["turnId"] == payload["turnId"]) {
                let state = if latest["state"] == "interrupted" {
                    "interrupted"
                } else {
                    checkpoint_turn_state(payload["status"].as_str().unwrap_or_default())
                };
                let or_completed = |v: &Value| {
                    if v.is_null() {
                        payload["completedAt"].clone()
                    } else {
                        v.clone()
                    }
                };
                thread["latestTurn"] = json!({
                    "turnId": payload["turnId"],
                    "state": state,
                    "requestedAt": or_completed(&latest["requestedAt"]),
                    "startedAt": or_completed(&latest["startedAt"]),
                    "completedAt": payload["completedAt"],
                    "assistantMessageId": payload["assistantMessageId"],
                });
            }
            thread["updatedAt"] = occurred_at;
        }
        "thread.reverted" => {
            let turn_count = payload["turnCount"].as_u64().unwrap_or(0);
            let checkpoints = array_mut(thread, "checkpoints");
            checkpoints.retain(|c| {
                c["checkpointTurnCount"]
                    .as_u64()
                    .is_some_and(|n| n <= turn_count)
            });
            let retained: Vec<Value> = checkpoints.iter().map(|c| c["turnId"].clone()).collect();
            let latest = checkpoints.last().cloned();
            array_mut(thread, "messages").retain(|m| {
                m["role"] == "system" || (!m["turnId"].is_null() && retained.contains(&m["turnId"]))
            });
            array_mut(thread, "activities")
                .retain(|a| a["turnId"].is_null() || retained.contains(&a["turnId"]));
            thread["latestTurn"] = match latest {
                None => Value::Null,
                Some(c) => json!({
                    "turnId": c["turnId"],
                    "state": checkpoint_turn_state(c["status"].as_str().unwrap_or_default()),
                    "requestedAt": c["completedAt"],
                    "startedAt": c["completedAt"],
                    "completedAt": c["completedAt"],
                    "assistantMessageId": c["assistantMessageId"],
                }),
            };
            thread["updatedAt"] = occurred_at;
        }
        "thread.activity-appended" => {
            let activity = payload["activity"].clone();
            let activities = array_mut(thread, "activities");
            activities.retain(|a| a["id"] != activity["id"]);
            activities.push(activity);
            activities.sort_by(|a, b| {
                let seq = |v: &Value| v["sequence"].as_u64().unwrap_or(u64::MAX);
                seq(a)
                    .cmp(&seq(b))
                    .then_with(|| a["createdAt"].as_str().cmp(&b["createdAt"].as_str()))
                    .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
            });
            thread["updatedAt"] = occurred_at;
        }
        _ => return Applied::Unchanged,
    }
    Applied::Updated
}

fn apply_message(thread: &mut Value, payload: &Value, occurred_at: Value) {
    let streaming = payload["streaming"].as_bool().unwrap_or(false);
    let text = payload["text"].as_str().unwrap_or_default();
    let messages = array_mut(thread, "messages");
    if let Some(entry) = messages
        .iter_mut()
        .find(|m| m["id"] == payload["messageId"])
    {
        let previous = entry["text"].as_str().unwrap_or_default().to_owned();
        entry["text"] = if streaming {
            json!(format!("{previous}{text}"))
        } else if text.is_empty() {
            json!(previous)
        } else {
            json!(text)
        };
        entry["streaming"] = json!(streaming);
        if let Some(turn) = payload.get("turnId") {
            entry["turnId"] = turn.clone();
        }
        if !streaming {
            entry["updatedAt"] = payload["updatedAt"].clone();
        }
        if let Some(attachments) = payload.get("attachments") {
            entry["attachments"] = attachments.clone();
        }
    } else {
        messages.push(json!({
            "id": payload["messageId"],
            "role": payload["role"],
            "text": text,
            "turnId": payload.get("turnId").cloned().unwrap_or(Value::Null),
            "streaming": streaming,
            "attachments": payload.get("attachments").cloned().unwrap_or(json!([])),
            "createdAt": payload["createdAt"],
            "updatedAt": payload["updatedAt"],
        }));
    }
    let turn_id = payload.get("turnId").cloned().unwrap_or(Value::Null);
    let latest = thread["latestTurn"].clone();
    if payload["role"] == "assistant"
        && !turn_id.is_null()
        && (latest.is_null() || latest["turnId"] == turn_id)
    {
        let still_running = thread["session"]["status"] == "running"
            && thread["session"]["activeTurnId"] == turn_id;
        let settles = !streaming && !still_running;
        let same = latest["turnId"] == turn_id;
        let state = if !settles {
            "running"
        } else if latest["state"] == "interrupted" {
            "interrupted"
        } else if latest["state"] == "error" {
            "error"
        } else {
            "completed"
        };
        thread["latestTurn"] = json!({
            "turnId": turn_id,
            "state": state,
            "requestedAt": if same { latest["requestedAt"].clone() } else { payload["createdAt"].clone() },
            "startedAt": if same && !latest["startedAt"].is_null() { latest["startedAt"].clone() } else { payload["createdAt"].clone() },
            "completedAt": if settles { payload["updatedAt"].clone() } else if same { latest["completedAt"].clone() } else { Value::Null },
            "assistantMessageId": payload["messageId"],
        });
    }
    if payload["role"] == "assistant" && !turn_id.is_null() {
        for checkpoint in array_mut(thread, "checkpoints") {
            if checkpoint["turnId"] == turn_id {
                checkpoint["assistantMessageId"] = payload["messageId"].clone();
            }
        }
    }
    thread["updatedAt"] = occurred_at;
}

fn array_mut<'a>(thread: &'a mut Value, key: &str) -> &'a mut Vec<Value> {
    if !thread[key].is_array() {
        thread[key] = json!([]);
    }
    thread[key].as_array_mut().expect("array")
}

fn upsert<T>(items: &mut Vec<T>, value: T, id: impl Fn(&T) -> &str) {
    if let Some(index) = items.iter().position(|old| id(old) == id(&value)) {
        items[index] = value;
    } else {
        items.push(value);
    }
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn command(kind: &str, thread_id: &str) -> Value {
    json!({"type":kind,"commandId":uuid::Uuid::new_v4().to_string(),"threadId":thread_id,"createdAt":now_iso()})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail() -> State {
        let mut state = State::default();
        let snapshot = json!({"kind":"snapshot","snapshot":{"snapshotSequence":1,"thread":{
            "id":"t","projectId":"p","title":"T","messages":[],"activities":[],"checkpoints":[],
            "latestTurn":null,"session":null}}});
        state.thread("t", &snapshot).unwrap();
        state
    }

    fn event(sequence: u64, kind: &str, payload: Value) -> Value {
        json!({"kind":"event","event":{"sequence":sequence,"type":kind,"occurredAt":"2026-01-01T00:00:00.000Z","payload":payload}})
    }

    #[test]
    fn streaming_deltas_append_and_settle_the_turn() {
        let mut state = detail();
        let delta = |seq, text: &str, streaming| {
            event(
                seq,
                "thread.message-sent",
                json!({"threadId":"t","messageId":"m","role":"assistant",
                "text":text,"turnId":"u","streaming":streaming,
                "createdAt":"2026-01-01T00:00:00.000Z","updatedAt":"2026-01-01T00:00:05.000Z"}),
            )
        };
        state.thread("t", &delta(2, "Hel", true)).unwrap();
        state.thread("t", &delta(3, "lo", true)).unwrap();
        let thread = state.detail.as_ref().unwrap();
        assert_eq!(thread.messages[0].text, "Hello");
        assert_eq!(thread.latest_turn.as_ref().unwrap().state, "running");
        state.thread("t", &delta(4, "", false)).unwrap();
        let thread = state.detail.as_ref().unwrap();
        assert_eq!(thread.messages[0].text, "Hello");
        assert_eq!(thread.latest_turn.as_ref().unwrap().state, "completed");
    }

    #[test]
    fn stale_sequences_are_ignored() {
        let mut state = detail();
        let rename = |seq, title: &str| {
            event(
                seq,
                "thread.meta-updated",
                json!({"threadId":"t","title":title,"updatedAt":"2026-01-01T00:00:00.000Z"}),
            )
        };
        state.thread("t", &rename(3, "New")).unwrap();
        state.thread("t", &rename(2, "Old")).unwrap();
        assert_eq!(state.detail.as_ref().unwrap().title, "New");
    }
}
