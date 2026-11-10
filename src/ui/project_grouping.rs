//! Logical project groups (`projectGrouping.ts` in client-runtime and
//! `sidebarProjectGrouping.ts`): projects whose repository identity matches
//! share one entry of the draft hero's project picker, newest first.
//!
//! This client connects to one environment, so groups only form inside it
//! (a repository's root and its packages, or two checkouts of one remote).
use std::collections::HashMap;

use crate::app::App;
use crate::model::{Project, Thread};
use crate::timeline::parse_time;

/// A project picker entry: its key, label and the physical projects it gathers.
#[derive(Clone, Debug)]
pub struct ProjectGroup {
    pub key: String,
    pub label: String,
    pub representative: Project,
    /// The projects it gathers, in first-appearance order.
    pub members: Vec<Project>,
}

/// `normalizeProjectPathForComparison` (POSIX paths).
pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    let stripped = trimmed.trim_end_matches(['/', '\\']);
    if stripped.is_empty() && !trimmed.is_empty() {
        trimmed[..1].to_owned()
    } else {
        stripped.to_owned()
    }
}

/// `derivePhysicalProjectKey`: the environment and the normalized path.
fn physical_key(environment_id: &str, project: &Project) -> String {
    format!(
        "{environment_id}:{}",
        normalize_path(&project.workspace_root)
    )
}

fn identity_field<'a>(project: &'a Project, field: &str) -> Option<&'a str> {
    project
        .repository_identity
        .as_ref()?
        .get(field)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// `deriveLogicalProjectKey` in the "repository" mode: the repository's
/// canonical key, else the project's own path.
fn logical_key(environment_id: &str, project: &Project) -> String {
    identity_field(project, "canonicalKey")
        .map_or_else(|| physical_key(environment_id, project), str::to_owned)
}

fn freshness(project: &Project) -> i64 {
    parse_time(&project.updated_at)
        .or_else(|| parse_time(&project.created_at))
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

/// `deriveProjectGroupLabel`.
fn group_label(representative: &Project, members: &[&Project]) -> String {
    let unique = |values: Vec<Option<&str>>| -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for value in values.into_iter().flatten() {
            let value = value.trim();
            if !value.is_empty() && !seen.iter().any(|s| s == value) {
                seen.push(value.to_owned());
            }
        }
        seen
    };
    let titles = unique(members.iter().map(|m| Some(m.title.as_str())).collect());
    let display_names = unique(
        members
            .iter()
            .map(|m| identity_field(m, "displayName"))
            .collect(),
    );
    let names = unique(members.iter().map(|m| identity_field(m, "name")).collect());
    if titles.len() == 1 && !display_names.contains(&titles[0]) && !names.contains(&titles[0]) {
        return titles[0].clone();
    }
    if display_names.len() == 1 {
        return display_names[0].clone();
    }
    if names.len() == 1 {
        return names[0].clone();
    }
    representative.title.clone()
}

/// `buildProjectGroups` for one environment: projects sharing a path keep
/// the freshest, then physical projects join by logical key.
pub fn build_groups(environment_id: &str, projects: &[&Project]) -> Vec<ProjectGroup> {
    let mut physical_order: Vec<String> = Vec::new();
    let mut by_physical: HashMap<String, Vec<&Project>> = HashMap::new();
    for project in projects {
        let key = physical_key(environment_id, project);
        if !by_physical.contains_key(&key) {
            physical_order.push(key.clone());
        }
        by_physical.entry(key).or_default().push(project);
    }
    let mut group_order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<Project>> = HashMap::new();
    for physical in physical_order {
        let candidates = &by_physical[&physical];
        let winner = candidates
            .iter()
            .copied()
            .fold(candidates[0], |current, candidate| {
                let delta = freshness(candidate) - freshness(current);
                if delta > 0 || (delta == 0 && candidate.id > current.id) {
                    candidate
                } else {
                    current
                }
            });
        // `selectProjectIdentitySource`: the freshest project with an identity.
        let identity = if winner.repository_identity.is_some() {
            winner
        } else {
            candidates
                .iter()
                .copied()
                .filter(|p| p.repository_identity.is_some())
                .max_by_key(|p| (freshness(p), p.id.clone()))
                .unwrap_or(winner)
        };
        let key = logical_key(environment_id, identity);
        if !groups.contains_key(&key) {
            group_order.push(key.clone());
        }
        groups.entry(key).or_default().push(winner.clone());
    }
    group_order
        .into_iter()
        .map(|key| {
            let members = groups.remove(&key).unwrap_or_default();
            let representative = members[0].clone();
            let label = if members.len() > 1 {
                let refs: Vec<&Project> = members.iter().collect();
                group_label(&representative, &refs)
            } else {
                representative.title.clone()
            };
            ProjectGroup {
                key,
                label,
                representative,
                members,
            }
        })
        .collect()
}

fn millis(iso: &str) -> Option<i64> {
    parse_time(iso).map(|d| d.timestamp_millis())
}

/// `getThreadSortTimestamp`: the latest user message, else the last update,
/// else the creation.
fn thread_timestamp(thread: &Thread) -> i64 {
    thread
        .latest_user_message_at
        .as_deref()
        .and_then(millis)
        .or(millis(&thread.updated_at))
        .or(millis(&thread.created_at))
        .unwrap_or(i64::MIN)
}

/// `sortProjectsForSidebar`: groups by their newest thread (or their
/// representative's stamp), ties by title then key.
fn sort_groups(groups: &mut [ProjectGroup], threads: &[&Thread]) {
    let thread_stamps: Vec<(&str, i64)> = threads
        .iter()
        .map(|t| (t.project_id.as_str(), thread_timestamp(t)))
        .collect();
    let stamp = |group: &ProjectGroup| -> i64 {
        let latest = thread_stamps
            .iter()
            .filter(|(project, _)| group.members.iter().any(|m| m.id == *project))
            .map(|(_, stamp)| *stamp)
            .max();
        if let Some(latest) = latest {
            return latest;
        }
        let project = &group.representative;
        millis(&project.updated_at)
            .or(millis(&project.created_at))
            .unwrap_or(i64::MIN)
    };
    let stamps: HashMap<String, i64> = groups.iter().map(|g| (g.key.clone(), stamp(g))).collect();
    groups.sort_by(|a, b| {
        stamps[&b.key]
            .cmp(&stamps[&a.key])
            .then_with(|| a.representative.title.cmp(&b.representative.title))
            .then_with(|| a.key.cmp(&b.key))
    });
}

impl App {
    fn environment_id(&self) -> String {
        self.state.config["environment"]["environmentId"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    /// The logical projects, the one with the newest thread first
    /// (`sortLogicalProjectsForSidebar`).
    pub(crate) fn sorted_project_groups(&self) -> Vec<ProjectGroup> {
        let projects: Vec<&Project> = self
            .state
            .projects
            .iter()
            .filter(|p| !self.is_chats(p))
            .collect();
        let threads: Vec<&Thread> = self
            .state
            .threads
            .iter()
            .filter(|t| t.archived_at.is_none() && !t.is_subagent() && !self.is_chat_thread(t))
            .map(std::sync::Arc::as_ref)
            .collect();
        let mut groups = build_groups(&self.environment_id(), &projects);
        sort_groups(&mut groups, &threads);
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn project(id: &str, title: &str, root: &str, identity: Option<Value>) -> Project {
        Project {
            id: id.into(),
            title: title.into(),
            workspace_root: root.into(),
            repository_identity: identity,
            ..Project::default()
        }
    }

    #[test]
    fn repository_mode_groups_a_repository_and_its_packages() {
        let identity = |root: &str| {
            Some(json!({
                "canonicalKey": "github.com/example/web-demo",
                "rootPath": root,
                "displayName": "example/web-demo",
                "name": "web-demo",
            }))
        };
        let web = project("a", "web-demo", "/w/web-demo", identity("/w/web-demo"));
        let ui = project(
            "b",
            "web-ui",
            "/w/web-demo/packages/ui",
            identity("/w/web-demo"),
        );
        let other = project("c", "other", "/w/other", None);
        let projects = [&web, &ui, &other];
        let groups = build_groups("env", &projects);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].members.len(), 2);
        assert_eq!(groups[0].label, "example/web-demo");
    }

    fn thread(id: &str, project: &str, created: &str, message: Option<&str>) -> Thread {
        Thread {
            id: id.into(),
            project_id: project.into(),
            created_at: created.into(),
            updated_at: created.into(),
            latest_user_message_at: message.map(str::to_owned),
            ..Thread::default()
        }
    }

    #[test]
    fn a_thread_is_stamped_by_its_latest_user_message_then_its_creation() {
        let with_message = thread(
            "a",
            "p",
            "2026-01-01T00:00:00Z",
            Some("2026-01-05T00:00:00Z"),
        );
        let without = thread("b", "p", "2026-01-03T00:00:00Z", None);
        assert!(thread_timestamp(&with_message) > thread_timestamp(&without));
    }

    #[test]
    fn projects_follow_their_newest_thread() {
        let p1 = project("p1", "Alpha", "/w/alpha", None);
        let p2 = project("p2", "Beta", "/w/beta", None);
        let t = thread("t", "p1", "2026-01-09T00:00:00Z", None);
        let mut groups = build_groups("env", &[&p2, &p1]);
        sort_groups(&mut groups, &[&t]);
        assert_eq!(groups[0].representative.id, "p1");
    }
}
