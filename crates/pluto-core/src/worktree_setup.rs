//! A worktree's setup (`packages/contracts/src/worktreeSetup.ts`,
//! `client-runtime/src/worktreeSetup.ts`, `MessagesTimeline.logic.ts`'s
//! setup row): the snapshot `subscribeWorktreeSetup` streams, when the
//! timeline shows it, and where its card goes.

use serde::Deserialize;

use crate::timeline::Row;

/// `WorktreeSetupSnapshot`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct WorktreeSetup {
    pub thread_id: String,
    /// `running`, `done`, `failed` or `cancelled`.
    pub phase: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub branch: Option<String>,
    pub base_ref: Option<String>,
    pub worktree_path: Option<String>,
    pub setup_script: Option<SetupScript>,
    pub stages: Vec<Stage>,
    pub error: Option<String>,
    pub sequence: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SetupScript {
    pub name: String,
    pub command: String,
    pub terminal_id: Option<String>,
}

/// `WorktreeSetupStage`: `id` is `fetch`, `checkout`, `submodules`,
/// `setup-script` or `agent`; `status` `pending`, `running`, `done`,
/// `skipped`, `warning` or `failed`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Stage {
    pub id: String,
    pub status: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub percent: Option<f64>,
    pub detail: Option<String>,
    pub tail: Vec<String>,
}

/// `worktreeSetupStageLabel`.
pub fn stage_label(id: &str) -> &'static str {
    match id {
        "fetch" => "Fetch base branch",
        "checkout" => "Check out files",
        "submodules" => "Init submodules",
        "setup-script" => "Run setup script",
        _ => "Start agent",
    }
}

impl WorktreeSetup {
    /// `worktreeSetupAgentStarted`.
    pub fn agent_started(&self) -> bool {
        self.stages
            .iter()
            .any(|s| s.id == "agent" && s.status == "done")
    }

    pub fn any_failed(&self) -> bool {
        self.stages.iter().any(|s| s.status == "failed")
    }
}

/// `resolveVisibleWorktreeSetup`: a running setup always shows; after a
/// follow-up message nothing does; a failed or cancelled one stays; a clean
/// finish goes once the turn is live, unless a stage failed.
pub fn visible(setup: &WorktreeSetup, turn_started: bool, follow_up_sent: bool) -> bool {
    if setup.phase == "running" {
        return true;
    }
    if follow_up_sent {
        return false;
    }
    if setup.phase != "done" || !turn_started {
        return true;
    }
    setup.any_failed()
}

/// The setup row's place: under the working row while it owns that slot
/// (inserting one after the first user message if needed), else after the
/// first user message, with no thinking row under it. A running setup
/// handed off to a live turn gets no row; the working row speaks for it.
pub fn insert_row(rows: &mut Vec<Row>, setup: &WorktreeSetup, run_started: bool) {
    let handed_off = setup.agent_started() && run_started;
    let owns_working = !handed_off && matches!(setup.phase.as_str(), "running" | "done");
    if handed_off && setup.phase == "running" {
        return;
    }
    let row = Row::WorktreeSetup {
        snapshot: Box::new(setup.clone()),
        embedded: handed_off,
    };
    // A setup that owns the working slot shows no activity row of its own.
    if owns_working {
        rows.retain(|r| !matches!(r, Row::Thinking { .. }));
    }
    let working = owns_working
        .then(|| rows.iter().position(|r| matches!(r, Row::Working { .. })))
        .flatten();
    if let Some(index) = working {
        rows.insert(index + 1, row);
        return;
    }
    let at = rows
        .iter()
        .position(|r| matches!(r, Row::User(_)))
        .map_or(rows.len(), |i| i + 1);
    if owns_working {
        rows.insert(
            at,
            Row::Working {
                started_at: Some(setup.started_at.clone()),
            },
        );
        rows.insert(at + 1, row);
    } else {
        rows.insert(at, row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(phase: &str, stages: &[(&str, &str)]) -> WorktreeSetup {
        WorktreeSetup {
            phase: phase.into(),
            started_at: "2026-10-29T10:00:00.000Z".into(),
            stages: stages
                .iter()
                .map(|(id, status)| Stage {
                    id: (*id).into(),
                    status: (*status).into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn shows_like_the_first_turn_wants() {
        assert!(visible(&setup("running", &[]), true, true));
        assert!(!visible(&setup("failed", &[]), false, true));
        assert!(visible(&setup("cancelled", &[]), true, false));
        assert!(visible(&setup("done", &[]), false, false));
        assert!(!visible(&setup("done", &[("agent", "done")]), true, false));
        assert!(visible(
            &setup("done", &[("setup-script", "failed")]),
            true,
            false
        ));
    }

    #[test]
    fn the_card_follows_the_working_row() {
        let user = Row::User(Default::default());
        let mut rows = vec![user.clone()];
        insert_row(&mut rows, &setup("running", &[]), false);
        assert!(matches!(rows[1], Row::Working { .. }));
        assert!(matches!(
            rows[2],
            Row::WorktreeSetup {
                embedded: false,
                ..
            }
        ));
        // Handed off while the script still runs: the working row speaks.
        let mut rows = vec![user.clone()];
        insert_row(&mut rows, &setup("running", &[("agent", "done")]), true);
        assert_eq!(rows.len(), 1);
        let mut rows = vec![user];
        insert_row(&mut rows, &setup("failed", &[]), false);
        assert!(matches!(rows[1], Row::WorktreeSetup { .. }));
    }
}
