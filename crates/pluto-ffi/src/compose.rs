//! What a composer sends, after the desktop's `composer.rs`: the model,
//! access and mode it resolves (the composer's pick, else the thread's,
//! else the project's then the server's defaults), and the commands for a
//! follow-up (`startThreadTurn`) or a new thread (`orchestration.launchThread`),
//! with Pluto's Research and Image chats (`chat_requests`).

use pluto_core::model::{ModelSelection, Project};
use pluto_core::spaces::Space;
use serde_json::{Value, json};

use crate::App;
use crate::catalog;
use crate::session::Session;

/// A composer: the Chat or Code draft, or a thread.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Target {
    Chat,
    Code,
    Thread { id: String },
}

impl Target {
    pub fn key(&self) -> String {
        match self {
            Target::Chat => "chat".into(),
            Target::Code => "code".into(),
            Target::Thread { id } => id.clone(),
        }
    }
}

/// The composers' choices, kept while the app runs (their texts live in
/// the interface).
#[derive(Default)]
pub struct Drafts {
    pub research: bool,
    pub image: bool,
    pub temporary: bool,
    /// The Code draft's project.
    pub project: Option<String>,
    /// The Code draft starts in a new worktree (else the checkout), off
    /// `base` when one was picked.
    pub worktree: Option<bool>,
    pub base: Option<String>,
    /// Picks by composer key.
    pub models: std::collections::HashMap<String, ModelSelection>,
    pub access: std::collections::HashMap<String, String>,
    pub plan: std::collections::HashMap<String, bool>,
}

/// The draft's project: the picked one, else the latest used code project.
pub fn code_project<'a>(session: &'a Session, picked: Option<&str>) -> Option<&'a Project> {
    let projects = session.code_projects();
    if let Some(project) = picked.and_then(|id| projects.iter().find(|p| p.id == id)) {
        return Some(project);
    }
    session
        .listed(false)
        .first()
        .and_then(|t| session.state.project(&t.project_id))
        .filter(|p| session.project_space(p) == Space::Code)
        .or_else(|| projects.first().copied())
}

/// The chat project a new chat goes to.
pub fn chat_project(session: &Session, temporary: bool) -> Option<&Project> {
    session.space_project(if temporary {
        Space::Temporary
    } else {
        Space::Chats
    })
}

/// `project_default_model`: the project's override, its own default, then
/// the server's.
pub fn project_default(session: &Session, project: &Project) -> Value {
    let settings = session.settings();
    settings["projectSettingsOverrides"][&project.id]
        .get("defaultModelSelection")
        .filter(|v| !v.is_null())
        .cloned()
        .or_else(|| {
            project
                .default_model_selection
                .clone()
                .filter(|v| !v.is_null())
        })
        .unwrap_or_else(|| settings["defaultModelSelection"].clone())
}

pub fn model(
    session: &Session,
    drafts: &Drafts,
    target: &Target,
    project: Option<&str>,
) -> ModelSelection {
    if let Some(picked) = drafts.models.get(&target.key()) {
        return picked.clone();
    }
    let chat = match target {
        Target::Thread { id } => {
            if let Some(thread) = session.thread(id) {
                return thread.model_selection.clone();
            }
            false
        }
        Target::Chat => true,
        Target::Code => false,
    };
    let default = match target {
        Target::Chat => chat_project(session, drafts.temporary)
            .or_else(|| chat_project(session, false))
            .map(|p| project_default(session, p)),
        _ => code_project(session, project).map(|p| project_default(session, p)),
    };
    default
        .as_ref()
        .and_then(catalog::selection)
        .or_else(|| catalog::selection(&session.settings()["defaultModelSelection"]))
        .unwrap_or_else(|| catalog::first_model(session.providers(), chat))
}

/// The runtime mode: chats ask before anything that would write or run.
pub fn access(
    session: &Session,
    drafts: &Drafts,
    target: &Target,
    project: Option<&str>,
) -> String {
    match target {
        Target::Chat => return "approval-required".into(),
        Target::Thread { id }
            if session
                .thread(id)
                .is_some_and(|t| session.space(&t).is_chat()) =>
        {
            return "approval-required".into();
        }
        _ => {}
    }
    if let Some(mode) = drafts.access.get(&target.key()) {
        return mode.clone();
    }
    if let Target::Thread { id } = target
        && let Some(thread) = session.thread(id)
    {
        return thread.runtime_mode.clone();
    }
    let settings = session.settings();
    code_project(session, project)
        .and_then(|p| settings["projectSettingsOverrides"][&p.id]["defaultRuntimeMode"].as_str())
        .or(settings["defaultRuntimeMode"].as_str())
        .unwrap_or("full-access")
        .to_owned()
}

pub fn plan(session: &Session, drafts: &Drafts, target: &Target) -> bool {
    if let Some(plan) = drafts.plan.get(&target.key()) {
        return *plan;
    }
    match target {
        Target::Thread { id } => session
            .thread(id)
            .is_some_and(|t| t.interaction_mode == "plan"),
        _ => false,
    }
}

/// `title_seed`: the text, cut at 50 characters.
fn title_seed(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= 50 {
        text.to_owned()
    } else {
        format!("{}...", chars[..50].iter().collect::<String>())
    }
}

/// Codex's default model, which makes a chat's images (`image_model`).
fn image_model(session: &Session) -> Option<Value> {
    let provider = session
        .providers()
        .iter()
        .find(|p| p["driver"] == "codex" && p["enabled"] != false && p["status"] == "ready")?;
    let models = provider["models"].as_array()?;
    let model = models
        .iter()
        .find(|m| m["isDefault"] == true)
        .or(models.first())?;
    Some(json!({"instanceId": provider["instanceId"], "model": model["slug"]}))
}

/// Sends `text` from `target`; returns the new thread's id for a draft.
pub fn send(app: &mut App, target: &Target, text: &str) -> Result<Option<String>, String> {
    let text = text.trim();
    let attached = app
        .session
        .attachments
        .get(&target.key())
        .is_some_and(|list| !list.is_empty());
    if text.is_empty() && !attached {
        return Ok(None);
    }
    let attachments = Value::Array(app.session.wire_attachments(&target.key())?);
    // A message of attachments alone takes its title from the first one.
    let seed = if text.is_empty() {
        app.session.attachments[&target.key()][0].name.clone()
    } else {
        title_seed(text)
    };
    let App {
        session, drafts, ..
    } = app;
    let project_pick = drafts.project.clone();
    let selection = model(session, drafts, target, project_pick.as_deref());
    let access = access(session, drafts, target, project_pick.as_deref());
    let interaction = if plan(session, drafts, target) {
        "plan"
    } else {
        "default"
    };
    let inline =
        session.state.config["environment"]["capabilities"]["inlineMessageContext"] == true;
    match target {
        Target::Thread { id } => {
            let thread = session.thread(id).ok_or("This thread is not listed yet")?;
            let mut commands = Vec::new();
            if access != thread.runtime_mode {
                let mut command = pluto_core::model::command("thread.runtime-mode.set", id);
                command["runtimeMode"] = json!(access);
                commands.push(command);
            }
            if interaction != thread.interaction_mode
                && !(interaction == "default" && thread.interaction_mode.is_empty())
            {
                let mut command = pluto_core::model::command("thread.interaction-mode.set", id);
                command["interactionMode"] = json!(interaction);
                commands.push(command);
            }
            let mut message = pluto_core::commands::message_dispatch(
                id,
                text,
                &catalog::wire(&selection),
                "auto",
            );
            message["titleSeed"] = json!(seed);
            message["attachments"] = attachments;
            if !session.net.online() {
                return Err(crate::session::NOT_CONNECTED.into());
            }
            for command in commands {
                session.dispatch(command);
            }
            // Refused, the text comes back to this composer.
            if !session.send_message(message, target.key(), text.to_owned()) {
                return Err(String::new());
            }
            Ok(None)
        }
        Target::Chat | Target::Code => {
            let chat = *target == Target::Chat;
            let project = if chat {
                chat_project(session, drafts.temporary)
                    .ok_or("Open Chat once in Pluto on the computer: it creates the chat space.")?
            } else {
                code_project(session, project_pick.as_deref())
                    .ok_or("Add a project in Pluto on the computer first.")?
            }
            .clone();
            let mut model_json = catalog::wire(&selection);
            let (text_out, records) = if chat && drafts.image && !drafts.temporary {
                model_json = image_model(session).ok_or("Images need a ready Codex provider.")?;
                pluto_core::chat_requests::image_message(text, &[])
            } else if chat && drafts.research && !drafts.temporary {
                pluto_core::chat_requests::research_message(text, &[])
            } else {
                (text.to_owned(), Vec::new())
            };
            let thread_id = uuid::Uuid::new_v4().to_string();
            let mut input = pluto_core::commands::launch_input(
                &thread_id,
                &project.id,
                &seed,
                &text_out,
                &model_json,
                &access,
                interaction,
            );
            pluto_core::context::apply(&mut input["initialMessage"], &text_out, &records, inline);
            input["initialMessage"]["attachments"] = attachments;
            if !chat
                && drafts
                    .worktree
                    .unwrap_or_else(|| default_worktree(session, &project))
            {
                let base = drafts.base.clone().unwrap_or_else(|| "main".into());
                input["workspaceStrategy"] = json!({"type": "worktree", "baseRef": base});
                let settings = session.settings();
                let from_origin = settings["projectSettingsOverrides"][&project.id]
                    ["newWorktreesStartFromOrigin"]
                    .as_bool()
                    .or(settings["newWorktreesStartFromOrigin"].as_bool());
                if from_origin == Some(true) {
                    input["workspaceStrategy"]["startFromOrigin"] = json!(true);
                }
            }
            let temporary = chat && drafts.temporary;
            if !session.launch(&thread_id, input, target.key(), text.to_owned()) {
                return Err(crate::session::NOT_CONNECTED.into());
            }
            // Deleted once the phone leaves it.
            if temporary {
                session.opened_temporary(&thread_id);
            }
            Ok(Some(thread_id))
        }
    }
}

/// `resolveNewThreadEnvMode`: whether new threads of `project` start in a
/// worktree.
pub fn default_worktree(session: &Session, project: &Project) -> bool {
    let settings = session.settings();
    settings["projectSettingsOverrides"][&project.id]["defaultThreadEnvMode"]
        .as_str()
        .or(project.default_thread_env_mode.as_deref())
        .or(settings["defaultThreadEnvMode"].as_str())
        == Some("worktree")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_stop_at_fifty_characters() {
        assert_eq!(title_seed("short"), "short");
        assert_eq!(
            title_seed(&"x".repeat(60)),
            format!("{}...", "x".repeat(50))
        );
    }
}
