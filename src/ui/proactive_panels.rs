//! Proactive panels (`ChatView.tsx` and `ChatView.logic.ts`
//! `shouldOpenProactivePullRequest`, `shouldOpenProactiveTurnDiff`,
//! `resolveProactiveTurnDiffAction`, `rightPanelStore.openProactive`), with
//! the "Proactive panels" setting on: the Linked pull requests surface opens
//! when the thread's links change, and the Diff (Changes) opens when a turn
//! watched here completes with 3 files or 50 changed lines, unless the user
//! changed the panel meanwhile.
use std::collections::HashMap;

use crate::app::{App, PanelTab};
use crate::ui::thread_pull_requests::visible_links;

/// What was seen of a thread: its running turn and the panel then, and the
/// linked pull requests' key.
#[derive(Default)]
pub(crate) struct Observation {
    running: Option<(String, String)>,
    links: Option<String>,
}

#[derive(Default)]
pub(crate) struct Proactive {
    threads: HashMap<String, Observation>,
}

impl App {
    fn panel_signature(&self) -> String {
        let panel = self.panel();
        format!("{}:{}:{}", panel.open, panel.active, panel.tabs.len())
    }

    /// Opens what a settled turn or new links warrant, once per change.
    pub(crate) fn proactive_panels(&mut self) {
        if self.client_setting("proactivePanelsEnabled") != true {
            return;
        }
        let (Some(thread), Some(detail)) = (self.current_thread(), self.current_detail()) else {
            return;
        };
        let signature = self.panel_signature();
        // `proactivePullRequestsKey`.
        let links = if self.links_pull_requests() {
            let mut keys: Vec<String> = visible_links(&thread.pull_requests)
                .iter()
                .map(|l| format!("{}/{}#{}", l["host"], l["repository"], l["number"]))
                .collect();
            keys.sort();
            (!keys.is_empty()).then(|| keys.join(","))
        } else {
            None
        };
        let running = detail
            .runs
            .iter()
            .rev()
            .find(|r| r.status == "running")
            .map(|r| r.id.clone());
        let latest = detail.runs.iter().max_by_key(|r| r.ordinal).cloned();
        let observed = self.proactive.threads.entry(thread.id.clone()).or_default();
        let open_links = links.is_some() && links != observed.links;
        observed.links = links.clone();
        let mut completed = None;
        match (&running, observed.running.take()) {
            (Some(run), previous) => {
                // The panel as it was when the turn started.
                let since = previous
                    .filter(|(id, _)| id == run)
                    .map_or(signature.clone(), |(_, s)| s);
                observed.running = Some((run.clone(), since));
            }
            (None, Some((run, since))) => completed = Some((run, since)),
            (None, None) => {}
        }
        if open_links {
            self.show_linked_pull_requests();
            return;
        }
        let Some((run, since)) = completed else {
            return;
        };
        // `shouldOpenProactiveTurnDiff`: the watched turn completed, and
        // nothing about the panel changed by hand since it started.
        if links.is_some()
            || since != signature
            || latest.is_none_or(|l| l.id != run || l.status != "completed")
        {
            return;
        }
        // `resolveProactiveTurnDiffAction`.
        let active = self
            .panel()
            .open
            .then(|| self.panel().active_tab().cloned())
            .flatten();
        if matches!(active, Some(PanelTab::Diff | PanelTab::PullRequests))
            || !self.workspace_is_repo()
        {
            return;
        }
        let checkpoint = detail
            .checkpoints
            .iter()
            .find(|c| c.run_id.as_deref() == Some(run.as_str()));
        // A checkpoint not captured yet: look again next frame ("defer").
        let Some(checkpoint) = checkpoint.filter(|c| c.status != "missing") else {
            if let Some(observed) = self.proactive.threads.get_mut(&thread.id) {
                observed.running = Some((run, since));
            }
            return;
        };
        let lines: u64 = checkpoint
            .files
            .iter()
            .map(|f| f.additions + f.deletions)
            .sum();
        if checkpoint.status != "ready" || !(checkpoint.files.len() >= 3 || lines >= 50) {
            return;
        }
        self.diff_scopes
            .insert(thread.id.clone(), crate::ui::diff_panel::DiffScope::Branch);
        if let Some(panel) = self.panel_mut() {
            panel.show(PanelTab::Diff, |t| *t == PanelTab::Diff);
            panel.open = true;
        }
    }
}
