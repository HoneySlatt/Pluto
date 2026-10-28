//! The changed files of a thread (`#diff`): a turn's checkpoint diff
//! (`orchestration.getTurnDiff`, parsed by `pluto_core::diff`), or every turn
//! of the thread from the thread actions. Unified with one line-number
//! gutter, a chip per file to jump to it, files collapsed or expanded after
//! "Default diff file state", and a wrap-lines toggle.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use pluto_core::diff::{FileDiff, LineKind};
use serde_json::json;

use crate::icons::Icon;
use crate::nav::Ui;

#[component]
pub fn DiffPage(thread: String, run: Option<String>, focus: Option<String>) -> Element {
    let ui = use_context::<Ui>();
    let mut wrap = use_signal(|| false);
    let collapsed = ui.ctx.session.read().prefs.flag("diffFilesCollapsed");
    let ignore_ws = ui.ctx.session.read().prefs.flag("diffIgnoreWhitespace");
    let mut open = use_signal(BTreeSet::<String>::new);
    // The turn range: the run's checkpoint, else every turn of the thread;
    // asked for once the thread's projection has its checkpoints.
    let request = use_memo({
        let (thread, run) = (thread.clone(), run.clone());
        move || {
            let session = ui.ctx.session.read();
            let detail = session.state.detail.clone().filter(|d| d.id() == thread)?;
            let turns: Vec<(String, u64)> = detail
                .checkpoints
                .iter()
                .filter_map(|c| Some((c.run_id.clone()?, c.app_run_ordinal?)))
                .collect();
            let (from, to) = match &run {
                Some(run) => turns
                    .iter()
                    .find(|(r, _)| r == run)
                    .map(|(_, turn)| (turn.saturating_sub(1), *turn))?,
                None => (0, turns.iter().map(|(_, turn)| *turn).max()?),
            };
            Some((format!("diff:{thread}:{from}:{to}:{ignore_ws}"), from, to))
        }
    });
    let mut asked = use_signal(|| None::<String>);
    use_effect({
        let thread = thread.clone();
        move || {
            if let Some((key, from, to)) = request()
                && asked.peek().as_deref() != Some(key.as_str())
            {
                asked.set(Some(key.clone()));
                let mut payload =
                    json!({"threadId": thread, "fromTurnCount": from, "toTurnCount": to});
                if ignore_ws {
                    payload["ignoreWhitespace"] = json!(true);
                }
                ui.ctx.request(key, "orchestration.getTurnDiff", payload);
            }
        }
    });
    let key = request().map(|(key, _, _)| key);
    let loaded = ui
        .ctx
        .session
        .read()
        .state
        .detail
        .as_ref()
        .is_some_and(|d| d.id() == thread);
    let files: Option<Result<Vec<FileDiff>, String>> = key.as_ref().and_then(|key| {
        ui.ctx.session.read().replies.get(key).map(|r| {
            r.as_ref()
                .map(|v| pluto_core::diff::parse(v["diff"].as_str().unwrap_or_default()))
                .map_err(Clone::clone)
        })
    });
    // Files start after the setting; a picked file opens and scrolls in.
    use_effect({
        let (focus, files) = (focus.clone(), files.clone());
        move || {
            if let Some(Ok(files)) = &files {
                let mut set = open.write();
                if set.is_empty() {
                    if !collapsed {
                        set.extend(files.iter().map(|f| f.path.clone()));
                    }
                    if let Some(focus) = &focus {
                        set.insert(focus.clone());
                        let index = files.iter().position(|f| &f.path == focus).unwrap_or(0);
                        document::eval(&format!(
                            "setTimeout(() => {{ const b = document.querySelector('.diffscroll'), f = document.getElementById('f-{index}'); if (b && f) b.scrollTop = f.offsetTop - b.offsetTop; }}, 300);"
                        ));
                    }
                }
            }
        }
    });
    let branch = ui
        .ctx
        .session
        .read()
        .thread(&thread)
        .and_then(|t| t.branch.clone());
    let (count, adds, dels) = match &files {
        Some(Ok(files)) => (
            files.len(),
            files.iter().map(|f| f.additions).sum::<u32>(),
            files.iter().map(|f| f.deletions).sum::<u32>(),
        ),
        _ => (0, 0, 0),
    };
    let wrapped = wrap();
    rsx! {
        section { class: "page",
            header { class: "thead",
                button { class: "disc", "aria-label": "Back", onclick: move |_| ui.back(),
                    Icon { name: "chevron-left" }
                }
                div { class: "ttl",
                    b { "{count} changed files" }
                    span {
                        span { class: "plus", "+{adds}" }
                        span { class: "minus", "−{dels}" }
                        if let Some(branch) = branch {
                            span { class: "sep", "·" }
                            Icon { name: "git-branch" }
                            "{branch}"
                        }
                    }
                }
                button {
                    class: if wrapped { "disc on" } else { "disc" },
                    "aria-label": "Wrap lines",
                    "aria-pressed": "{wrapped}",
                    onclick: move |_| wrap.toggle(),
                    Icon { name: "text-wrap" }
                }
            }
            match files {
                None if key.is_none() && loaded => rsx! { div { class: "empty-page", "No changes in this thread yet." } },
                None => rsx! { div { class: "empty-page", "Loading the diff…" } },
                Some(Err(error)) => rsx! { div { class: "empty-page", b { "Couldn’t load the diff" } "{error}" } },
                Some(Ok(files)) if files.is_empty() => rsx! { div { class: "empty-page", "No changes in this turn." } },
                Some(Ok(files)) => rsx! {
                    div { class: "fchips",
                        for (index, file) in files.iter().enumerate() {
                            button {
                                key: "{file.path}",
                                onclick: {
                                    let path = file.path.clone();
                                    move |_| {
                                        open.write().insert(path.clone());
                                        document::eval(&format!(
                                            "setTimeout(() => {{ const b = document.querySelector('.diffscroll'), f = document.getElementById('f-{index}'); if (b && f) b.scrollTo({{ top: f.offsetTop - b.offsetTop, behavior: 'smooth' }}); }}, 30);"
                                        ));
                                    }
                                },
                                "{file.path.rsplit('/').next().unwrap_or_default()}"
                                span { class: "plus", "+{file.additions}" }
                                span { class: "minus", "−{file.deletions}" }
                            }
                        }
                    }
                    div { class: if wrapped { "diffscroll wrap" } else { "diffscroll" },
                        for (index, file) in files.into_iter().enumerate() {
                            FileView { key: "{file.path}", index, expanded: open.read().contains(&file.path), file, open }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn FileView(
    index: usize,
    file: FileDiff,
    expanded: bool,
    open: Signal<BTreeSet<String>>,
) -> Element {
    let (dir, name) = match file.path.rfind('/') {
        Some(at) => (file.path[..=at].to_owned(), file.path[at + 1..].to_owned()),
        None => (String::new(), file.path.clone()),
    };
    let path = file.path.clone();
    rsx! {
        section { class: "dfile", id: "f-{index}",
            button {
                class: "dfh",
                "aria-expanded": "{expanded}",
                onclick: move |_| {
                    let mut set = open.write();
                    if !set.remove(&path) {
                        set.insert(path.clone());
                    }
                },
                Icon { name: "chevron-down", class: "chev" }
                span { class: "path", "{dir}" b { "{name}" } }
                span { class: "plus", "+{file.additions}" }
                span { class: "minus", "−{file.deletions}" }
            }
            if expanded {
                div { class: "hunks",
                    div {
                        if file.binary {
                            div { class: "hk", style: "color:var(--muted);background:none", "Binary file" }
                        }
                        for hunk in file.hunks.iter() {
                            div { class: "hk", "@@ -{hunk.old_start} +{hunk.new_start} @@" }
                            for line in hunk.lines() {
                                div {
                                    class: match line.kind {
                                        LineKind::Addition => "ln add",
                                        LineKind::Deletion => "ln del",
                                        LineKind::Context => "ln keep",
                                    },
                                    span { class: "no", "{line.number()}" }
                                    span { class: "sg",
                                        match line.kind {
                                            LineKind::Addition => "+",
                                            LineKind::Deletion => "−",
                                            LineKind::Context => "",
                                        }
                                    }
                                    code { if line.text.is_empty() { " " } else { "{line.text}" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
