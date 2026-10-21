//! The repository's checked-in `t3.json` (`T3ProjectFile` in
//! `contracts/t3ProjectFile.ts`, decoded like `parseT3ProjectFile` with
//! `fromLenientJson`, read like `useMemberProjectFiles` in
//! `settings/SettingsScopeContext.tsx`): read with `projects.readFile` at
//! the workspace root once per session; comments and trailing commas are
//! tolerated, and a missing, truncated or invalid file counts as absent.

use serde_json::{Value, json};

use crate::app::App;

pub(crate) const FILE_NAME: &str = "t3.json";
const PATH_MAX_LENGTH: usize = 512;
const MAX_SCRIPTS: usize = 50;
const SCRIPT_ICONS: [&str; 6] = ["play", "test", "lint", "configure", "build", "debug"];

/// `parseLenientJsonGetter`: drops `//` and `/* */` comments and trailing
/// commas before `}` or `]`, leaving string literals intact.
fn strip_jsonc(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '"' => {
                out.push('"');
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        out.push(chars[i]);
                        i += 1;
                    }
                    out.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    out.push('"');
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                let end = (i + 2..chars.len().saturating_sub(1))
                    .find(|&j| chars[j] == '*' && chars[j + 1] == '/');
                match end {
                    Some(j) => i = j + 2,
                    // An unclosed comment stays, and fails to parse.
                    None => {
                        out.extend(&chars[i..]);
                        i = chars.len();
                    }
                }
            }
            ',' => {
                let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
                if !matches!(next, Some('}' | ']')) {
                    out.push(',');
                }
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// A string that is non-empty once trimmed (`trimmedNonEmpty`).
fn trimmed_non_empty(value: &Value, max_length: Option<usize>) -> bool {
    value.as_str().is_some_and(|s| {
        !s.trim().is_empty() && max_length.is_none_or(|max| s.chars().count() <= max)
    })
}

fn optional(file: &Value, key: &str, valid: impl Fn(&Value) -> bool) -> bool {
    file.get(key).is_none_or(valid)
}

fn one_of(value: &Value, allowed: &[&str]) -> bool {
    value.as_str().is_some_and(|s| allowed.contains(&s))
}

/// `T3ProjectFileScript`.
fn valid_script(script: &Value) -> bool {
    script.is_object()
        && trimmed_non_empty(&script["name"], None)
        && trimmed_non_empty(&script["command"], None)
        && optional(script, "icon", |v| one_of(v, &SCRIPT_ICONS))
        && ["runOnWorktreeCreate", "async", "autoOpenPreview"]
            .iter()
            .all(|key| optional(script, key, Value::is_boolean))
        && optional(script, "previewUrl", |v| trimmed_non_empty(v, None))
}

/// `parseT3ProjectFile`: the decoded file, or None when it is not valid.
pub(crate) fn parse(contents: &str) -> Option<Value> {
    let file: Value = serde_json::from_str(&strip_jsonc(contents)).ok()?;
    let valid = file.is_object()
        && optional(&file, "$schema", Value::is_string)
        && optional(&file, "iconPath", |v| {
            trimmed_non_empty(v, Some(PATH_MAX_LENGTH))
        })
        && optional(&file, "defaultThreadEnvMode", |v| {
            one_of(v, &["local", "worktree"])
        })
        && optional(&file, "worktreeSubmodules", |v| {
            one_of(v, &["recursive", "top-level", "none"])
        })
        && optional(&file, "scripts", |v| {
            v.as_array()
                .is_some_and(|s| s.len() <= MAX_SCRIPTS && s.iter().all(valid_script))
        });
    valid.then_some(file)
}

/// `PROJECT_FILE_BACKED_SETTINGS`: the t3.json field and built-in value of
/// a file-backed setting.
pub(crate) fn file_backed(key: &str) -> Option<(&'static str, &'static str)> {
    match key {
        "defaultThreadEnvMode" => Some(("defaultThreadEnvMode", "local")),
        "worktreeSubmodules" => Some(("worktreeSubmodules", "recursive")),
        _ => None,
    }
}

impl App {
    /// Asks for the workspace's t3.json (once).
    pub(crate) fn request_project_file(&mut self, cwd: &str) {
        self.files_request(
            format!("files-read:{cwd}\0{FILE_NAME}"),
            "projects.readFile",
            json!({"cwd": cwd, "relativePath": FILE_NAME}),
        );
    }

    /// The workspace's decoded t3.json: None until its read settles, then
    /// the file or None (missing, truncated or invalid).
    pub(crate) fn project_file(&self, cwd: &str) -> Option<Option<Value>> {
        let read = self.files.reads.get(&format!("{cwd}\0{FILE_NAME}"))?;
        Some(read.as_ref().ok().and_then(|data| {
            if data["truncated"] == true {
                return None;
            }
            parse(data["contents"].as_str()?)
        }))
    }
}

impl App {
    /// `T3ProjectFileState`'s "invalid": the file was read but fails to
    /// decode.
    pub(crate) fn project_file_invalid(&self, cwd: &str) -> bool {
        matches!(
            self.files.reads.get(&format!("{cwd}\0{FILE_NAME}")),
            Some(Ok(data)) if data["truncated"] != true
                && data["contents"].as_str().is_some_and(|c| parse(c).is_none())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lenient_json_keeps_strings() {
        let file = parse(
            r#"{
                // where threads start
                "defaultThreadEnvMode": "worktree", /* team default */
                "scripts": [{"name": "a,]", "command": "echo // not a comment",},],
            }"#,
        )
        .expect("valid");
        assert_eq!(file["defaultThreadEnvMode"], "worktree");
        assert_eq!(file["scripts"][0]["name"], "a,]");
        assert_eq!(file["scripts"][0]["command"], "echo // not a comment");
    }

    #[test]
    fn invalid_files_count_as_absent() {
        assert!(parse(r#"{"defaultThreadEnvMode": "elsewhere"}"#).is_none());
        assert!(parse(r#"{"worktreeSubmodules": null}"#).is_none());
        assert!(parse(r#"{"scripts": [{"name": " ", "command": "x"}]}"#).is_none());
        assert!(
            parse(r#"{"scripts": [{"name": "a", "command": "x", "icon": "rocket"}]}"#).is_none()
        );
        assert!(parse("[]").is_none());
        assert!(parse("{").is_none());
        assert!(parse(r#"{"unknown": 1}"#).is_some());
    }
}
