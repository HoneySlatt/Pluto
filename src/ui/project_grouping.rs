//! Logical project groups (`projectGrouping.ts` in client-runtime and
//! `sidebarProjectGrouping.ts`): projects whose repository identity matches
//! share one sidebar entry, by repository or by repository-relative path,
//! with per-project overrides (`sidebarProjectGroupingOverrides`).
//!
//! This client connects to one environment, so groups only form inside it
//! (a repository's root and its packages, or two checkouts of one remote).
use std::collections::HashMap;

use serde_json::Value;

use crate::model::Project;

/// A sidebar entry: its key, label and the physical projects it gathers.
#[derive(Clone, Debug)]
pub struct ProjectGroup {
    pub key: String,
    pub label: String,
    pub representative: Project,
    /// (physical key, project), in first-appearance order.
    pub members: Vec<(String, Project)>,
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

/// `derivePhysicalProjectKey`: the environment and the normalized path. It
/// also keys grouping overrides and the manual project order.
pub fn physical_key(environment_id: &str, project: &Project) -> String {
    format!(
        "{environment_id}:{}",
        normalize_path(&project.workspace_root)
    )
}

/// `legacyProjectCwdPreferenceKey`.
pub fn legacy_cwd_key(workspace_root: &str) -> String {
    format!("legacy-project-cwd:{}", normalize_path(workspace_root))
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

/// `deriveRepositoryRelativeProjectPath`.
fn repository_relative_path(project: &Project) -> Option<String> {
    let root = normalize_path(identity_field(project, "rootPath")?);
    let path = normalize_path(&project.workspace_root);
    if root.is_empty() || path.is_empty() {
        return None;
    }
    if path == root {
        return Some(String::new());
    }
    path.strip_prefix(&format!("{root}/")).map(str::to_owned)
}

/// `deriveLogicalProjectKey`.
fn logical_key(environment_id: &str, project: &Project, mode: &str) -> String {
    if mode != "separate"
        && let Some(canonical) = identity_field(project, "canonicalKey")
    {
        if mode == "repository" {
            return canonical.to_owned();
        }
        return match repository_relative_path(project) {
            Some(relative) if !relative.is_empty() => format!("{canonical}::{relative}"),
            _ => canonical.to_owned(),
        };
    }
    physical_key(environment_id, project)
}

/// `resolveProjectGroupingMode`: the project's override, else the default.
pub fn grouping_mode(
    environment_id: &str,
    project: &Project,
    default_mode: &str,
    overrides: &Value,
) -> String {
    overrides[physical_key(environment_id, project)]
        .as_str()
        .unwrap_or(default_mode)
        .to_owned()
}

fn freshness(project: &Project) -> i64 {
    crate::timeline::parse_time(&project.updated_at)
        .or_else(|| crate::timeline::parse_time(&project.created_at))
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
pub fn build_groups(
    environment_id: &str,
    projects: &[&Project],
    default_mode: &str,
    overrides: &Value,
) -> Vec<ProjectGroup> {
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
    let mut groups: HashMap<String, Vec<(String, Project)>> = HashMap::new();
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
        let mode = grouping_mode(environment_id, winner, default_mode, overrides);
        let key = logical_key(environment_id, identity, &mode);
        if !groups.contains_key(&key) {
            group_order.push(key.clone());
        }
        groups
            .entry(key)
            .or_default()
            .push((physical, winner.clone()));
    }
    group_order
        .into_iter()
        .map(|key| {
            let members = groups.remove(&key).unwrap_or_default();
            let representative = members[0].1.clone();
            let label = if members.len() > 1 {
                let refs: Vec<&Project> = members.iter().map(|(_, p)| p).collect();
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

/// `orderItemsByPreferredIds`: projects named in the saved order first (by
/// physical key or legacy cwd key), then the rest in their given order.
pub fn order_projects<'a>(
    environment_id: &str,
    projects: &[&'a Project],
    preferred: &[String],
) -> Vec<&'a Project> {
    if preferred.is_empty() {
        return projects.to_vec();
    }
    let mut emitted = vec![false; projects.len()];
    let mut ordered = Vec::new();
    for id in preferred {
        let found = projects.iter().enumerate().position(|(index, project)| {
            !emitted[index]
                && (physical_key(environment_id, project) == *id
                    || legacy_cwd_key(&project.workspace_root) == *id)
        });
        if let Some(index) = found {
            emitted[index] = true;
            ordered.push(projects[index]);
        }
    }
    ordered.extend(
        projects
            .iter()
            .enumerate()
            .filter(|(index, _)| !emitted[*index])
            .map(|(_, project)| *project),
    );
    ordered
}

/// `reorderProjects`: moves the dragged keys to the target's slot; None
/// when nothing changes.
pub fn reorder(order: &[String], dragged: &[String], target: &[String]) -> Option<Vec<String>> {
    if dragged.is_empty() || dragged.iter().all(|id| target.contains(id)) {
        return None;
    }
    let target_index = order.iter().position(|id| target.contains(id))?;
    let mut next = order.to_vec();
    let mut removed = Vec::new();
    let mut before_target: usize = 0;
    for index in (0..next.len()).rev() {
        if dragged.contains(&next[index]) {
            removed.insert(0, next.remove(index));
            if index < target_index {
                before_target += 1;
            }
        }
    }
    if removed.is_empty() {
        return None;
    }
    let insert = target_index - before_target.saturating_sub(1).min(target_index);
    for (offset, id) in removed.into_iter().enumerate() {
        next.insert(insert + offset, id);
    }
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let groups = build_groups("env", &projects, "repository", &json!({}));
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].members.len(), 2);
        assert_eq!(groups[0].label, "example/web-demo");
        let by_path = build_groups("env", &projects, "repository_path", &json!({}));
        assert_eq!(by_path.len(), 3);
        let overridden = build_groups(
            "env",
            &projects,
            "repository",
            &json!({ "env:/w/web-demo/packages/ui": "separate" }),
        );
        assert_eq!(overridden.len(), 3);
    }

    #[test]
    fn reorder_moves_dragged_projects_to_the_target() {
        let order: Vec<String> = ["a", "b", "c", "d"].map(String::from).to_vec();
        let moved = reorder(&order, &["d".into()], &["b".into()]).unwrap();
        assert_eq!(moved, ["a", "d", "b", "c"]);
        let down = reorder(&order, &["a".into()], &["c".into()]).unwrap();
        assert_eq!(down, ["b", "c", "a", "d"]);
        assert!(reorder(&order, &["a".into()], &["a".into()]).is_none());
    }
}
