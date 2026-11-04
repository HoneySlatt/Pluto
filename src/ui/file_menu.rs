//! A workspace file's right-click actions (`fileContextMenu.ts`,
//! `diffFileActions.ts`, `fileExplorerLabel.ts`): Open in the default app,
//! Reveal in the file manager, Open with an installed editor, all through
//! `shell.openInEditor` on the server's machine. Diff paths are
//! repository-relative and resolve through the workspace
//! (`resolveDiffPathForWorkspace`).
use serde_json::json;

use crate::app::App;
use crate::ui::context_menu::Item;
use crate::ui::toasts;

const KEY: &str = "file-menu:";

/// `resolveDiffPathForWorkspace`: a repository-relative path as a path in
/// the workspace, `None` when it lies outside it.
pub(crate) fn workspace_path(
    path: &str,
    workspace: &str,
    repository: Option<&str>,
) -> Option<String> {
    if path.starts_with('/') || path.chars().nth(1) == Some(':') {
        return None;
    }
    let segments: Vec<&str> = path
        .split(['/', '\\'])
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    if segments.is_empty() || segments.contains(&"..") {
        return None;
    }
    let inside: Vec<&str> = match repository {
        Some(root) if root.trim_end_matches('/') != workspace.trim_end_matches('/') => {
            let prefix = format!("{}/", root.trim_end_matches('/'));
            match workspace.strip_prefix(&prefix) {
                Some(rest) => rest.split('/').filter(|s| !s.is_empty()).collect(),
                None => return Some(segments.join("/")),
            }
        }
        _ => Vec::new(),
    };
    if inside.is_empty() {
        return Some(segments.join("/"));
    }
    if segments.len() <= inside.len() || segments[..inside.len()] != inside[..] {
        return None;
    }
    Some(segments[inside.len()..].join("/"))
}

impl App {
    /// `buildFileContextMenuItems` for what the server advertises; empty
    /// when nothing can act.
    pub(crate) fn file_menu_items(&self) -> Vec<Item> {
        let config = &self.state.config;
        let editors = self.available_editors();
        let mut items = Vec::new();
        if editors.contains(&"file-manager") {
            items.push(Item::new("open", "Open").icon("pencil"));
            if config["shellRevealInFileManager"] == true {
                let label = match config["shellRevealInFileManagerKind"].as_str().or(
                    match config["environment"]["platform"]["os"].as_str() {
                        Some("darwin") => Some("finder"),
                        Some("windows") => Some("file-explorer"),
                        _ => None,
                    },
                ) {
                    Some("finder") => "Reveal in Finder",
                    Some("file-explorer") => "Reveal in File Explorer",
                    _ => "Reveal in Files",
                };
                items.push(Item::new("reveal-in-folder", label).icon("folder-tree"));
            }
        }
        let children: Vec<Item> = editors
            .iter()
            .filter(|id| **id != "file-manager")
            .map(|id| Item::new(format!("editor:{id}"), crate::ui::open_in::editor_label(id)))
            .collect();
        if !children.is_empty() {
            items.push(Item {
                children,
                ..Item::new("open-with", "Open with")
            });
        }
        items
    }

    /// Runs a file menu action on `absolute`; false for ids it does not own.
    pub(crate) fn activate_file_menu(&mut self, action: &str, absolute: &str) -> bool {
        let reveal = action == "reveal-in-folder";
        let editor = match action {
            "open" | "reveal-in-folder" => "file-manager",
            other => match other.strip_prefix("editor:") {
                Some(editor) => editor,
                None => return false,
            },
        };
        let mut input = json!({"cwd": absolute, "editor": editor});
        if reveal {
            input["reveal"] = json!(true);
        }
        self.send_request(
            format!("{KEY}{action}\0{absolute}"),
            "shell.openInEditor",
            input,
        );
        true
    }

    /// The action's failure as a toast naming the path.
    pub(crate) fn file_menu_result(
        &mut self,
        key: &str,
        result: &Result<serde_json::Value, String>,
    ) -> bool {
        let Some(rest) = key.strip_prefix(KEY) else {
            return false;
        };
        if result.is_err() {
            let (action, path) = rest.split_once('\0').unwrap_or((rest, ""));
            let title = match action {
                "open" => "Could not open file".to_owned(),
                "reveal-in-folder" => "Unable to reveal file".to_owned(),
                other => format!(
                    "Could not open in {}",
                    crate::ui::open_in::editor_label(other.trim_start_matches("editor:"))
                ),
            };
            self.toasts
                .add(toasts::Toast::new(toasts::Kind::Error, title).description(path));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::workspace_path;

    #[test]
    fn diff_paths_resolve_through_the_workspace() {
        assert_eq!(
            workspace_path("src/a.rs", "/r", Some("/r")).as_deref(),
            Some("src/a.rs")
        );
        assert_eq!(
            workspace_path("app/src/a.rs", "/r/app", Some("/r")).as_deref(),
            Some("src/a.rs")
        );
        assert_eq!(workspace_path("lib/a.rs", "/r/app", Some("/r")), None);
        assert_eq!(workspace_path("../x", "/r", None), None);
    }
}
