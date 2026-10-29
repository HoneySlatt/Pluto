//! The changed files of a thread (`#diff`): a turn's checkpoint diff
//! (`orchestration.getTurnDiff`, parsed by `pluto_core::diff`), or every
//! turn of the thread from the thread actions. Unified with one line-number
//! gutter; each changed pair marks the words that changed.

use std::sync::{Arc, Mutex};

use pluto_core::diff::{FileDiff, LineKind};
use serde_json::json;

use crate::{App, Core};

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
}

/// A changed span of a line, in UTF-16 units (Kotlin's string indices).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DiffSpan {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// The unified gutter's number: the new side unless deleted.
    pub number: u32,
    pub text: String,
    pub emphasis: Vec<DiffSpan>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DiffHunk {
    /// `@@ -old +new @@`.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DiffFile {
    pub path: String,
    /// The folder part (`src/`), then the file's name.
    pub dir: String,
    pub name: String,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum DiffState {
    /// The thread has no checkpoint yet.
    NoChanges,
    Loading,
    Failed {
        error: String,
    },
    /// The turn changed nothing.
    Empty,
    Files {
        files: Vec<DiffFile>,
    },
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DiffView {
    /// The request's key, once the thread's checkpoints tell the range.
    pub key: Option<String>,
    pub branch: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    /// "Default diff file state" (Settings): files start collapsed.
    pub collapsed: bool,
    pub state: DiffState,
}

/// The last parsed reply: the diff does not change once it arrived, and the
/// page reads it again on every event of the thread.
static PARSED: Mutex<Option<(String, Arc<Vec<DiffFile>>)>> = Mutex::new(None);

/// Leaving the server forgets its last diff.
pub fn reset() {
    *PARSED.lock().unwrap_or_else(|p| p.into_inner()) = None;
}

fn utf16(text: &str, byte: usize) -> u32 {
    text.get(..byte.min(text.len()))
        .map(|s| s.encode_utf16().count() as u32)
        .unwrap_or(0)
}

fn file(diff: FileDiff) -> DiffFile {
    let (dir, name) = match diff.path.rfind('/') {
        Some(at) => (diff.path[..=at].to_owned(), diff.path[at + 1..].to_owned()),
        None => (String::new(), diff.path.clone()),
    };
    DiffFile {
        hunks: diff
            .hunks
            .iter()
            .map(|hunk| DiffHunk {
                header: format!("@@ -{} +{} @@", hunk.old_start, hunk.new_start),
                lines: hunk
                    .lines()
                    .map(|line| DiffLine {
                        kind: match line.kind {
                            LineKind::Context => DiffLineKind::Context,
                            LineKind::Addition => DiffLineKind::Addition,
                            LineKind::Deletion => DiffLineKind::Deletion,
                        },
                        number: line.number(),
                        emphasis: line
                            .emphasis
                            .iter()
                            .map(|(start, end)| DiffSpan {
                                start: utf16(&line.text, *start),
                                end: utf16(&line.text, *end),
                            })
                            .filter(|span| span.end > span.start)
                            .collect(),
                        text: line.text.clone(),
                    })
                    .collect(),
            })
            .collect(),
        path: diff.path,
        dir,
        name,
        additions: diff.additions,
        deletions: diff.deletions,
        binary: diff.binary,
    }
}

/// The turn range of `run`'s checkpoint, else every turn of the thread,
/// as the request's key and payload.
fn request(app: &App, thread: &str, run: Option<&str>) -> Option<(String, serde_json::Value)> {
    let s = &app.session;
    let ignore_ws = s.prefs.flag("diffIgnoreWhitespace");
    let detail = s.state.detail.as_ref().filter(|d| d.id() == thread)?;
    let turns: Vec<(String, u64)> = detail
        .checkpoints
        .iter()
        .filter_map(|c| Some((c.run_id.clone()?, c.app_run_ordinal?)))
        .collect();
    let (from, to) = match run {
        Some(run) => turns
            .iter()
            .find(|(r, _)| r == run)
            .map(|(_, turn)| (turn.saturating_sub(1), *turn))?,
        None => (0, turns.iter().map(|(_, turn)| *turn).max()?),
    };
    let mut payload = json!({"threadId": thread, "fromTurnCount": from, "toTurnCount": to});
    if ignore_ws {
        payload["ignoreWhitespace"] = json!(true);
    }
    Some((format!("diff:{thread}:{from}:{to}:{ignore_ws}"), payload))
}

fn parsed(key: &str, patch: &str) -> Arc<Vec<DiffFile>> {
    let mut cache = PARSED.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((cached, files)) = cache.as_ref()
        && cached == key
    {
        return files.clone();
    }
    let files = Arc::new(
        pluto_core::diff::parse(patch)
            .into_iter()
            .map(file)
            .collect::<Vec<_>>(),
    );
    *cache = Some((key.to_owned(), files.clone()));
    files
}

#[uniffi::export]
impl Core {
    /// The diff page of `thread` (`run`'s turn, else every turn).
    pub fn diff(&self, thread: String, run: Option<String>) -> DiffView {
        self.read(|app| {
            let s = &app.session;
            let key = request(app, &thread, run.as_deref()).map(|(key, _)| key);
            let loaded = s.state.detail.as_ref().is_some_and(|d| d.id() == thread);
            let state = match key
                .as_ref()
                .and_then(|key| s.replies.get(key).map(|r| (key, r)))
            {
                None if key.is_none() && loaded => DiffState::NoChanges,
                None => DiffState::Loading,
                Some((_, Err(error))) => DiffState::Failed {
                    error: error.clone(),
                },
                Some((key, Ok(value))) => {
                    let files = parsed(key, value["diff"].as_str().unwrap_or_default());
                    if files.is_empty() {
                        DiffState::Empty
                    } else {
                        DiffState::Files {
                            files: files.as_ref().clone(),
                        }
                    }
                }
            };
            let (additions, deletions) = match &state {
                DiffState::Files { files } => (
                    files.iter().map(|f| f.additions).sum(),
                    files.iter().map(|f| f.deletions).sum(),
                ),
                _ => (0, 0),
            };
            DiffView {
                key,
                branch: s.thread(&thread).and_then(|t| t.branch.clone()),
                additions,
                deletions,
                collapsed: s.prefs.flag("diffFilesCollapsed"),
                state,
            }
        })
    }

    /// Asks for the diff once its range is known (and not already asked).
    pub fn ask_diff(&self, thread: String, run: Option<String>) {
        let ask = self.read(|app| {
            request(app, &thread, run.as_deref()).filter(|(key, _)| {
                !app.session.replies.contains_key(key) && !app.session.waiting(key)
            })
        });
        if let Some((key, payload)) = ask {
            self.update(|app, _| {
                app.session
                    .request(key, "orchestration.getTurnDiff", payload)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_split_their_path_and_mark_in_utf16() {
        let patch = "diff --git a/src/é.rs b/src/é.rs\n--- a/src/é.rs\n+++ b/src/é.rs\n@@ -1,1 +1,1 @@\n-let é = 1;\n+let é = 2;\n";
        let files: Vec<DiffFile> = pluto_core::diff::parse(patch)
            .into_iter()
            .map(file)
            .collect();
        assert_eq!(files.len(), 1);
        assert_eq!(
            (files[0].dir.as_str(), files[0].name.as_str()),
            ("src/", "é.rs")
        );
        let lines = &files[0].hunks[0].lines;
        assert_eq!(lines[0].kind, DiffLineKind::Deletion);
        assert_eq!(files[0].hunks[0].header, "@@ -1 +1 @@");
        // "let é = " is 8 UTF-16 units: the changed digit follows.
        assert!(
            lines[1].emphasis.iter().all(|s| s.start >= 8),
            "{:?}",
            lines[1].emphasis
        );
        assert_eq!(utf16("é1", 2), 1);
    }
}
