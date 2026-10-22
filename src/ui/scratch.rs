//! Threads without a project (`useScratchProject.ts`): they live in the
//! environment's Scratch project, a plain folder the server owns
//! (`ServerConfig.scratchWorkspaceRoot`, shown as "No project").
//! `projects.ensureScratch` creates it on first use; once its project
//! reaches the shell, the draft is retargeted or a new thread starts in it.
use serde_json::json;

use crate::app::App;
use crate::model::Project;
use crate::ui::project_grouping::normalize_path;

const REQUEST: &str = "projects-ensure-scratch";

/// What to do once the Scratch project is known.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Then {
    /// Retarget the open draft (the hero's picker and its "or start without
    /// a project"), unless it moved to another project meanwhile.
    Retarget,
    /// Start a new thread in it (`startScratchThread`).
    NewThread,
}

pub(crate) struct Pending {
    then: Then,
    /// The draft's project when asked, to notice a later choice.
    draft_project: Option<String>,
    /// The id the server answered, while its project has not arrived.
    project_id: Option<String>,
}

impl App {
    /// The Scratch folder of the connected environment, when it offers one.
    pub(crate) fn scratch_root(&self) -> Option<&str> {
        self.connected()
            .then(|| self.state.config["scratchWorkspaceRoot"].as_str())
            .flatten()
            .filter(|root| !root.trim().is_empty())
    }

    /// `isScratchProject`: the project rooted at the Scratch folder.
    pub(crate) fn is_scratch(&self, project: &Project) -> bool {
        self.scratch_root()
            .is_some_and(|root| normalize_path(root) == normalize_path(&project.workspace_root))
    }

    /// Asks for the Scratch project (`openScratchProject`).
    pub(crate) fn open_scratch(&mut self, then: Then) {
        if self.scratch_root().is_none() || self.scratch.is_some() {
            return;
        }
        self.scratch = Some(Pending {
            then,
            draft_project: self.draft_project().map(|p| p.id),
            project_id: None,
        });
        self.send_request(REQUEST.into(), "projects.ensureScratch", json!({}));
    }

    pub(crate) fn start_scratch_thread(&mut self) {
        self.open_scratch(Then::NewThread);
    }

    /// The server's answer; `false` for other requests.
    pub(crate) fn scratch_result(
        &mut self,
        key: &str,
        result: &Result<serde_json::Value, String>,
    ) -> bool {
        if key != REQUEST {
            return false;
        }
        match result {
            Ok(value) => match (&mut self.scratch, value["projectId"].as_str()) {
                (Some(pending), Some(id)) => pending.project_id = Some(id.to_owned()),
                _ => self.scratch = None,
            },
            Err(error) => {
                self.scratch = None;
                self.status = Some(format!("Could not start without a project: {error}"));
            }
        }
        true
    }

    /// Acts once the Scratch project has reached the shell (`waitForProject`).
    pub(crate) fn poll_scratch(&mut self) {
        let Some(pending) = &self.scratch else {
            return;
        };
        let Some(id) = pending.project_id.clone() else {
            return;
        };
        if self.state.project(&id).is_none() {
            return;
        }
        let (then, asked_from) = (pending.then, pending.draft_project.clone());
        self.scratch = None;
        match then {
            Then::NewThread => self.start_draft(Some(id)),
            Then::Retarget => {
                // A stale continuation must not retarget a draft that moved.
                let current = self.draft_project().map(|p| p.id);
                if self.selected.is_none() && current == asked_from {
                    self.draft_project_id = Some(id);
                }
            }
        }
    }
}
