//! A thread's current pull request (`threadPullRequests.ts` in
//! packages/shared): the one link a single-slot surface such as a sidebar
//! row shows, preferring open work and the top of a stack, and its state
//! glyph (`PULL_REQUEST_STATE_PRESENTATION`).
use std::collections::{HashMap, HashSet};

use eframe::egui::Color32;
use serde_json::Value;

use crate::theme;

/// `threadPullRequestKeyOf` (host and repository compared lowercase).
fn key_of(link: &Value) -> String {
    format!(
        "{}/{}#{}",
        link["host"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_lowercase(),
        link["repository"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_lowercase(),
        link["number"].as_u64().unwrap_or(0)
    )
}

fn repo_of(link: &Value) -> String {
    format!(
        "{}/{}",
        link["host"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_lowercase(),
        link["repository"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_lowercase()
    )
}

/// `visibleThreadPullRequests`: dismissed stack members stay hidden.
fn visible(links: &[Value]) -> Vec<&Value> {
    links
        .iter()
        .filter(|l| l["source"] != "stack-dismissed")
        .collect()
}

/// `isOpen`: unsynced links count as open.
fn is_open(link: &Value) -> bool {
    link["snapshot"].is_null() || link["snapshot"]["state"] == "open"
}

fn millis(value: &Value) -> i64 {
    value
        .as_str()
        .and_then(crate::timeline::parse_time)
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

/// `latestUpdatedAt`.
fn latest_updated_at(link: &Value) -> i64 {
    let updated = &link["snapshot"]["updatedAt"];
    millis(if updated.is_string() {
        updated
    } else {
        &link["linkedAt"]
    })
}

/// `resolveThreadPullRequestChains`: native stacks in layer order, then
/// chains derived from head and base branches, bottom to top.
fn chains<'a>(links: &[&'a Value]) -> Vec<Vec<&'a Value>> {
    let mut chains: Vec<Vec<&Value>> = Vec::new();
    let mut placed: HashSet<String> = HashSet::new();
    let mut stack_order: Vec<String> = Vec::new();
    let mut stacks: HashMap<String, Vec<&Value>> = HashMap::new();
    for link in links {
        let Some(id) = link["stack"]["id"].as_str() else {
            continue;
        };
        let key = format!("{}#stack:{id}", repo_of(link));
        if !stacks.contains_key(&key) {
            stack_order.push(key.clone());
        }
        stacks.entry(key).or_default().push(link);
    }
    for key in stack_order {
        let mut members = stacks.remove(&key).unwrap_or_default();
        let order: HashMap<u64, usize> = members[0]["stack"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(i, layer)| layer["number"].as_u64().map(|n| (n, i)))
            .collect();
        members.sort_by_key(|m| {
            order
                .get(&m["number"].as_u64().unwrap_or(0))
                .copied()
                .unwrap_or(0)
        });
        for member in &members {
            placed.insert(key_of(member));
        }
        chains.push(members);
    }
    let remaining: Vec<&Value> = links
        .iter()
        .copied()
        .filter(|l| !placed.contains(&key_of(l)))
        .collect();
    let branch_key = |link: &Value, branch: &Value| -> Option<String> {
        branch.as_str().map(|b| format!("{}:{b}", repo_of(link)))
    };
    // Reused head names cannot identify a parent.
    let mut by_head: HashMap<String, Option<&Value>> = HashMap::new();
    for link in &remaining {
        if let Some(key) = branch_key(link, &link["snapshot"]["headBranch"]) {
            let ambiguous = by_head.contains_key(&key);
            by_head.insert(key, if ambiguous { None } else { Some(link) });
        }
    }
    let parent_of = |link: &Value| -> Option<&Value> {
        branch_key(link, &link["snapshot"]["baseBranch"])
            .and_then(|key| by_head.get(&key).copied().flatten())
    };
    let mut has_child: HashSet<String> = HashSet::new();
    for link in &remaining {
        if let Some(parent) = parent_of(link)
            && key_of(parent) != key_of(link)
        {
            has_child.insert(key_of(parent));
        }
    }
    for top in &remaining {
        if has_child.contains(&key_of(top)) {
            continue;
        }
        let mut layers: Vec<&Value> = Vec::new();
        let mut cursor = Some(*top);
        while let Some(link) = cursor {
            if !placed.insert(key_of(link)) {
                break;
            }
            layers.insert(0, link);
            cursor = parent_of(link);
        }
        if !layers.is_empty() {
            chains.push(layers);
        }
    }
    // Cycles have no top; keep their links visible one by one.
    for link in remaining {
        if placed.insert(key_of(link)) {
            chains.push(vec![link]);
        }
    }
    chains
}

/// One line of a thread's pull-request list (`PullRequestListLine`).
pub(crate) struct ListLine<'a> {
    pub link: &'a Value,
    /// 0 for a pull request on the base branch; each layer above steps in.
    pub depth: usize,
    /// On the bottom layer of a multi-layer stack: whether the host stacks
    /// them natively, and how many layers.
    pub stack: Option<(bool, usize)>,
}

/// The visible links (`visibleThreadPullRequests`).
pub(crate) fn visible_links(links: &[Value]) -> Vec<&Value> {
    visible(links)
}

/// `pullRequestListLines`: chains newest first by their most recent layer,
/// each read bottom to top.
pub(crate) fn list_lines<'a>(links: &[&'a Value]) -> Vec<ListLine<'a>> {
    let mut chains = chains(links);
    let activity = |chain: &Vec<&Value>| chain.iter().map(|l| latest_updated_at(l)).max();
    chains.sort_by_key(|chain| std::cmp::Reverse(activity(chain)));
    chains
        .into_iter()
        .flat_map(|chain| {
            let size = chain.len();
            let native = chain[0]["stack"].is_object();
            chain
                .into_iter()
                .enumerate()
                .map(move |(depth, link)| ListLine {
                    link,
                    depth,
                    stack: (depth == 0 && size > 1).then_some((native, size)),
                })
        })
        .collect()
}

/// `resolveThreadCurrentPullRequestLink`.
pub fn current_link(links: &[Value]) -> Option<&Value> {
    let visible = visible(links);
    if visible.is_empty() {
        return None;
    }
    let open: Vec<&Value> = visible.iter().copied().filter(|l| is_open(l)).collect();
    if open.len() == 1 {
        return Some(open[0]);
    }
    let chains = chains(&visible);
    if open.len() > 1 {
        // The newest open chain's highest open layer.
        return chains
            .iter()
            .map(|chain| {
                chain
                    .iter()
                    .rev()
                    .copied()
                    .filter(|l| is_open(l))
                    .collect::<Vec<_>>()
            })
            .filter(|layers| !layers.is_empty())
            .max_by_key(|layers| {
                layers
                    .iter()
                    .map(|l| millis(&l["linkedAt"]))
                    .max()
                    .unwrap_or(0)
            })
            .map(|layers| layers[0]);
    }
    if chains.len() == 1 {
        return chains[0].last().copied();
    }
    visible.into_iter().max_by_key(|l| latest_updated_at(l))
}

/// The state glyph and tone of a synced link (`resolvePullRequestState`),
/// with its tooltip (`prStatusIndicator`): "PR #12 - Open: title".
pub fn status(link: &Value) -> Option<(&'static str, Color32, String)> {
    let snapshot = &link["snapshot"];
    if snapshot.is_null() {
        return None;
    }
    let (icon, tone, label) = match (snapshot["state"].as_str(), snapshot["isDraft"] == true) {
        (Some("merged"), _) => (
            "git-merge",
            theme::alpha(Color32::from_rgb(0xc4, 0xb4, 0xff), 0.9),
            "Merged",
        ),
        (Some("closed"), _) => (
            "git-pull-request-closed",
            theme::alpha(Color32::from_rgb(0xff, 0xa2, 0xa2), 0.9),
            "Closed",
        ),
        (_, true) => (
            "git-pull-request-draft",
            theme::alpha(Color32::from_rgb(0x9f, 0x9f, 0xa9), 0.8),
            "Draft",
        ),
        _ => (
            "git-pull-request-arrow",
            theme::alpha(theme::EMERALD_300, 0.9),
            "Open",
        ),
    };
    let url = link["url"].as_str().unwrap_or_default();
    // `resolveChangeRequestPresentation`: GitLab says MR, the rest PR.
    let short = if url.contains("/-/merge_requests/") {
        "MR"
    } else {
        "PR"
    };
    let tooltip = format!(
        "{short} #{} - {label}: {}",
        link["number"].as_u64().unwrap_or(0),
        snapshot["title"].as_str().unwrap_or_default()
    );
    Some((icon, tone, tooltip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn link(number: u64, state: &str, head: &str, base: &str, linked: &str) -> Value {
        json!({
            "host": "github.com", "repository": "example/repo", "number": number,
            "url": format!("https://github.com/example/repo/pull/{number}"),
            "source": "manual", "linkedAt": linked, "stack": null,
            "snapshot": {
                "state": state, "title": format!("PR {number}"), "headBranch": head,
                "baseBranch": base, "isDraft": false, "updatedAt": linked,
                "syncedAt": linked,
            },
        })
    }

    #[test]
    fn prefers_the_single_open_link_then_the_top_of_a_stack() {
        let merged = link(1, "merged", "a", "main", "2026-01-01T00:00:00Z");
        let open = link(2, "open", "b", "main", "2026-01-02T00:00:00Z");
        assert_eq!(current_link(&[merged.clone(), open]).unwrap()["number"], 2);
        let bottom = link(3, "open", "c", "main", "2026-01-03T00:00:00Z");
        let top = link(4, "open", "d", "c", "2026-01-04T00:00:00Z");
        assert_eq!(current_link(&[bottom, top]).unwrap()["number"], 4);
        let closed = link(5, "closed", "e", "main", "2026-01-05T00:00:00Z");
        assert_eq!(current_link(&[merged, closed]).unwrap()["number"], 5);
    }
}
