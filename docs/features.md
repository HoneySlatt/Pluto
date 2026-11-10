# Features

Every feature of Pluto today, by family, and what the owner decided for each: keep, rethink or cut
([ADR-0010](adr/0010-todays-features-triaged.md)). The marks are the start of the specification of
the Rust server and of the uwui client ([M1.3](PLAN.md#m13--the-triage)).

- **Keep**: the family and its form go to the specification. A row kept may name what it gains.
- **Rethink**: its need stays; its form is decided row by row below, each row with its problem,
  the rival that does it best, and whether it is core or optional
  ([ADR-0003](adr/0003-what-pluto-is-to-become.md)).
- **Cut**: it leaves the egui client in M1.4, and the specification.

The families were marked on 2026-11-08 ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md));
the terminal's mark was corrected and the rows of the rethought families marked on 2026-11-10
([ADR-0022](adr/0022-the-triage-row-by-row.md)). What they become is
[`specification.md`](specification.md). Paths are under `src/` unless named otherwise; sizes are
lines of Rust, rounded.

## The desktop

| # | Family | What it does today | Where | Size | Mark | Direction |
|---|---|---|---|---|---|---|
| 1 | Threads and the sidebar | Thread cards and their state, pinned and settled shelves, snooze, auto-settle, the thread's menu, ordering by drag | `ui/sidebar*.rs`, `ui/legacy_sidebar.rs`, `ui/thread_menu.rs`, `snooze.rs`, `ui/park.rs` | 7,400 | Rethink | The dashboard of ADR-0003 becomes the home screen; a compact list beside an open thread; the legacy sidebar removed ([rows](#1-threads-and-the-sidebar)) |
| 2 | The timeline and its Markdown | Folded turns, tool groups, thinking, Mermaid, citations, a subset of HTML, highlighted code | `ui/markdown.rs`, `ui/md_*.rs`, `ui/cite*.rs`, `ui/timeline_view.rs`, `highlight.rs`, `crates/pluto-core/src/timeline.rs` | 8,600 | Rethink | Too close to T3, too dense, too heavy, things missing: a summary per turn, details folded ([rows](#2-the-timeline-and-its-markdown)) |
| 3 | The composer and attachments | The prompt's editor, mentions, drafts per thread, prompt history, context chips, the queue, steering | `ui/composer*.rs`, `ui/attachments.rs`, `ui/mentions.rs`, `ui/drafts.rs`, `ui/prompt_*.rs`, `ui/queue.rs` | 8,800 | Keep | Local dictation added |
| 4 | Agents' requests | Approvals, the agent's questions, secrets entered privately | `ui/requests_panel.rs`, `ui/secret_request.rs`, `crates/pluto-core/src/requests.rs` | 1,000 | Keep | In the grid of agents' abilities ([ADR-0017](adr/0017-pluto-measured-against-the-market.md)) |
| 5 | Plans | The proposed plan's card: refine, implement, save, implement in a new thread | `ui/plan_card.rs` | 1,100 | Keep | Supplied to every agent (ADR-0017) |
| 6 | The diff and review | Working tree, branch and turn scopes, the changed files' card | `ui/diff_panel.rs`, `diff_review.rs`, `changed_files.rs`, `crates/pluto-core/src/diff.rs` | 3,500 | Rethink | The market's best review view, comments sent to an agent (ADR-0018) |
| 7 | The terminal | A real terminal per thread, in a drawer, on the server | `terminal.rs`, `ui/terminal_drawer.rs`, `ui/terminal_surface.rs` | 2,500 | Keep | Marked Rethink by mistake on 2026-11-08, corrected by the owner |
| 8 | Files | A tree, search, a highlighted view, small edits, media previews | `ui/files.rs`, `ui/file_*.rs`, `ui/project_file.rs` | 2,700 | Rethink | The editor's light mode, then the editor of Stage 7 ([ADR-0019](adr/0019-a-code-editor-in-pluto.md)) |
| 9 | The browser | A preview panel, server-side tabs that agents drive, a Device surface | `preview.rs`, `ui/preview.rs`, `server_browser.rs`, `ui/server_browser.rs` | 3,200 | Rethink | One browser on the server, watched in a tab, which also shows computer use; optional where Chromium is installed |
| 10 | Git and worktrees | A quick action, commit, push, pull, branches, stash, worktrees' setup, publishing, Bitbucket | `ui/git_actions.rs`, `branch_picker.rs`, `source_control.rs`, `stash.rs`, `worktree_setup.rs`, `publish.rs`, `bitbucket.rs` | 6,200 | Keep | Worktrees' size shown and cleaned, checkpoints at each turn (ADR-0017); GitHub, GitLab and Forgejo kept, Bitbucket and Azure DevOps cut |
| 11 | Pull requests | A page of pull requests, those linked to each thread and their state | `ui/pull_requests.rs`, `linked_pull_requests.rs`, `thread_pull_requests.rs`, `crates/pluto-core/src/change_request.rs` | 2,700 | Rethink | A state of the thread and of the dashboard; followed to the merge (ADR-0018) |
| 12 | Search | Find in a thread, search among threads, search in their content | `ui/thread_find.rs`, `thread_search.rs`, `content_search.rs` | 1,800 | Rethink | One search, everywhere, every machine, the phone too |
| 13 | The palette and menus | Ctrl+K, Ctrl+P to a file, context menus, `pluto://` links | `ui/palette.rs`, `command_menu.rs`, `context_menu.rs`, `ui/prompt_links.rs` | 3,100 | Keep | Duplicates merged |
| 14 | Keybindings | Bindings resolved by the server, a when-clause builder, the shortcuts' page | `ui/keybindings.rs`, `when_builder.rs`, `shortcuts.rs` | 2,800 | Rethink | Remappable by zone, no when-clause builder; the editor's Vim and Helix modes shared |
| 15 | Agents' pages and MCP apps | A page an agent publishes, shown in the thread; MCP applications' frames | `ui/html_render.rs`, `mcp_apps.rs`, `crates/pluto-core/src/html_render.rs`, `mcp_app.rs` | 1,300 | Rethink | Heavy, and not powerful enough: a preview in the thread, the page in a panel, in uwui's web view; living artifacts ([rows](#15-agents-pages-and-mcp-apps)) |
| 16 | Providers and models | Each provider's settings, instances, the model picker, custom models | `ui/providers.rs`, `providers/`, `pickers.rs`, `custom_model_editor.rs` | 6,700 | Rethink | One configuration for every agent (ADR-0018); one picker, local and remote equal, quotas beside |
| 17 | Local models | Ollama's driver in the fork; code through the OpenCode companion | the fork's `packages/provider-ollama` | none in the client | Rethink | Managed in Pluto, as ADR-0003; coded by Pluto's own agent ([ADR-0011](adr/0011-plutos-own-harness-minimal-and-measured.md)) |
| 18 | Usage and limits | Cost and token charts, limits, prices, the context meter | `ui/usage*.rs`, `context_meter.rs` | 4,400 | Rethink | Quotas in the dashboard, warned before they run out, a switch suggested; Pluto's own tokens shown (ADR-0017) |
| 19 | Settings | Pages, search, scope and inheritance, project overrides, defaults restored | `ui/settings*.rs`, `setting_inheritance.rs` | 7,200 | Rethink | Fewer, each useful, triaged row by row; inheritance by machine, project and thread kept |
| 20 | Themes and typography | T3's environment themes, the accent picker, fonts, Lucide icons | `theme.rs`, `ui/accent_picker.rs`, `typography.rs`, `lucide.rs` | 3,000 | Rethink | Pluto's own look of ADR-0003 and ADR-0016, dark and light; T3's environment themes cut |
| 21 | Notifications | A thread finished or waiting, toasts, the composer's notices | `ui/thread_notifications.rs`, `toasts.rs`, `composer_notices.rs`, `crates/pluto-core/src/notifications.rs` | 2,100 | Keep | The triggers' notices added |
| 22 | Chat mode | The chat space, its projects, research, image chats, answers' versions, temporary chats, export | `ui/chat_*.rs`, `crates/pluto-core/src/spaces.rs` | 3,500 | Keep | Every model equal; it grows into the owner's only chat ([ADR-0020](adr/0020-the-chat.md)) |
| 23 | Memory and instructions | What Pluto remembers between conversations; the owner's instructions | `ui/memory_settings.rs`, the fork's memory tools | 700 | Keep | In the one configuration; its tokens counted |
| 24 | Sandboxed Python | `code_run`: Python in bubblewrap, for the chat | the fork | none in the client | Rethink | Under ADR-0009's one sandbox |
| 25 | Design projects | The design surface and its home, the server's design charter | `ui/design.rs`, `ui/design_home.rs` | 2,000 | Rethink | A kind of project, its studio the artifacts panel ([rows](#25-design-projects)) |
| 26 | Scheduled tasks | An agent started at a time or at intervals, the list of tasks | `ui/scheduled_tasks.rs`, `ui/tasks.rs` | 1,500 | Keep | Event triggers added (ADR-0018) |
| 27 | Subagents and the agents panel | Subagents shown in their thread; an Agents panel that T3 v0.1.0 removed | `subagents.rs`, `ui/agents_panel.rs` | 1,300 | Rethink | Subagents under their parent, delegation alike; the panel replaced by the dashboard |
| 28 | Projects and scripts | The projects' page, a project's actions, scripts imported, the actions' editor, "open in" | `ui/projects_page.rs`, `project_*.rs`, `script_import.rs`, `action_editor.rs`, `open_in.rs` | 3,500 | Rethink | Too many pages and settings: one page per project, actions found by Pluto ([rows](#28-projects-and-scripts)) |
| 29 | Rewind, undo and fork | Rewind a thread to a message, undo, fork, answers' versions | `ui/rewind.rs`, `undo.rs`, `chat_versions.rs` | 1,300 | Keep | Files rewound too, for every agent, by Pluto's checkpoints |
| 30 | Media | The lightbox, video, Codex's generated images, tools' images | `ui/media.rs`, `video.rs`, `lightbox.rs`, `tool_images.rs`, `crates/pluto-core/src/generated_images.rs` | 1,900 | Keep | Decoded off the interface's thread, in a bounded cache |

## The phone

| # | Family | What it does today | Where | Mark | Direction |
|---|---|---|---|---|---|
| 31 | The Android application | Connection, threads, a thread, the diff, settings, projects, tasks, usage, personalisation, notifications by a foreground service | `android/app/src/main/kotlin/app/pluto/mobile/`, `crates/pluto-ffi` | Rethink | The chat as readily as the dashboard, code not neglected: start, review, follow and unblock; notifications through UnifiedPush; offline, and models on the phone itself; files, a terminal, git; the desktop's marks apply family by family (ADR-0018) |

## The rows

Each family marked Rethink, and the phone, has its rows below: one feature a user would recognise,
its place in the code, its problem today, the rival that does it best, whether it is core or
optional ([ADR-0003](adr/0003-what-pluto-is-to-become.md)), and the owner's mark. The rows were
listed from the code, `docs/parity/`, the README and the fork's `PLUTO.md` on 2026-11-10; the
rivals come from the vendors' documentation read that day (the Codex app, Claude's desktop
application, Cursor, Zed, Warp, T3 Code, LM Studio, Claude Design, Paseo), not from use. The marks
were given in the interview of 2026-11-10 ([ADR-0022](adr/0022-the-triage-row-by-row.md)).

Marks: **Keep**; **Cut**; **Rethink**, followed by how; **Added**, a feature the row's family gains.
*Problem today* holds only facts with a source (a parity gap, a debt of the PLAN, a figure, a limit
written in the code); `—` when none was found. Paths are under `src/` unless named; `server:` is the
fork. C is core, O optional.

### 0. The right panel

Not one of the 31 families: the panel that holds the surfaces (`ui/surfaces.rs`, 576 lines;
`ui/proactive_panels.rs`, 126). No rival was surveyed for it.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 0.1 | Open a surface: the launcher and its "+" menu, by letter | `ui/surfaces.rs` `surface_launcher` | Lists Agents, which T3 v0.1.0 removed, and Device, which only shows a notice | C | Rethink: the surfaces left (review, files, terminal, browser, artifacts) |
| 0.2 | Tab bar, one tab per surface, middle-click close | `panel_tab_bar` | No reorder or overflow | C | Keep |
| 0.3 | Tab menu: copy path, close others, to the right, all | `panel_tab_menu` | — | C | Keep |
| 0.4 | Reopen a closed tab (Mod+Shift+T) | `reopen_closed_view` | — | C | Keep |
| 0.5 | The panel's state kept per thread | `app.rs` `panels_to_json` | Its width is not saved | C | Keep, its width saved |
| 0.6 | Resize and maximize | `ui/diff_panel.rs` `resize_handle` | Below 720 px it covers the chat | C | Keep; a sheet in a narrow window |
| 0.7 | Proactive panels: the diff or the pull request opens after a turn | `ui/proactive_panels.rs` | — | C | Keep, as a setting: the review opens after a turn; a pull request shows in its thread's strip (11.8) |
| 0.8 | Pull request surface | `surfaces.rs:100` | Always unavailable | — | Cut: the pull request's strip in its thread (11) |
| 0.9 | Device surface | `surfaces.rs:69` | Opens a notice only | — | Cut: the emulator is an app of computer use's session (9) |
| 0.10 | Agents surface | `ui/agents_panel.rs` | A T3 v0.0.45 leftover | — | Cut: the dashboard and the lineage card (27) |
| 0.11 | Design surface | `ui/design.rs` | — | O | Rethink: the artifacts panel (15, 25) |

### 1. Threads and the sidebar

Rivals: T3 Code for the thread's states (pin, snooze, settle, archive, undo); the Codex app's
Activity view and Devin Desktop's Agent Command Center for an overview of live agents.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 1.0 | The dashboard as the home screen: a grid of thread cards | — | — | C | Added |
| 1.1 | Pinned, active, snoozed and settled shelves | `ui/sidebar.rs` `sections` | Their folding is not saved | C | Rethink: the dashboard's sections; folding saved |
| 1.2 | Order: working first, then latest activity | `sort_active` | Ignores the server's order key | C | Keep |
| 1.3 | Thread card: project, title, status or time | `thread_card` | — | C | Rethink: the dashboard's card: title, project, state and live line (what it does, for how long), agent, model, machine |
| 1.4 | Card footer: worktree, branch, terminals, pull request, providers | `thread_card` | The PR badge opens the system browser | C | Rethink: in the hover card and the thread's header, not on the card |
| 1.5 | Status labels and live durations | `pluto-core` `notifications.rs` | — | C | Keep |
| 1.6 | Pin and unpin | `pin_glyph` | — | C | Keep |
| 1.7 | Snooze presets and the custom snooze dialog | `snooze.rs`, `ui/snooze_dialog.rs` | The clock button's picker is not ported | C | Keep |
| 1.8 | Snooze rules: when a thread wakes | `snooze.rs` | — | C | Keep |
| 1.9 | Settle, un-settle, wake now, settle and go to the next | `ui/park.rs` | Hover only, no keyboard | C | Keep, from the keyboard too |
| 1.10 | The thread's menu | `ui/thread_menu.rs` | Two menus, one per sidebar; unpin and archive ask no confirmation | C | Rethink: one menu, confirmations as set |
| 1.11 | Rename in place | `rename_editor` | No warning on an empty title | C | Keep |
| 1.12 | Unsent drafts listed above the threads | `ui/sidebar_drafts.rs` | Discarding cannot be undone | C | Rethink: a draft mark on the card; never lost |
| 1.13 | Drag a thread between sections, or onto the composer | `ui/sidebar_drag.rs` | — | C | Keep |
| 1.14 | Sweep an action along the list | `ui/sidebar_sweep.rs` | — | C | Keep |
| 1.15 | Multi-select and its menu | `sidebar_selection_menu` | One selection set shared with the legacy sidebar | C | Keep |
| 1.16 | Undo notice, Mod+Z | `ui/undo.rs` | — | C | Keep |
| 1.17 | Hover card: branch, model, earlier agents, terminals, error, PRs | `ui/thread_hover.rs` | — | C | Keep; it takes the card footer (1.4) |
| 1.18 | Drop files on a thread to attach them | `row_file_drop` | — | C | Keep |
| 1.19 | Sidebar width, collapse, sheet | `sidebar_width` | Fixed limits | C | Rethink: a compact list of threads beside an open thread; a key returns to the dashboard |
| 1.20 | Sidebar footer and empty states | `sidebar_footer` | "No threads yet" also for an empty search | C | Rethink: the dashboard's strip and empty states |
| 1.21 | Legacy sidebar: project trees, sorts, paging | `ui/legacy_sidebar.rs` (2,433) | Duplicates the sidebar | — | Cut |
| 1.22 | Legacy sidebar: project menu, rename and grouping dialogs | `legacy_project_menu` | No "project not empty" step | — | Cut |
| 1.23 | Legacy sidebar: glyphs, archive on hover, Mod+digit chips | `legacy_thread_row` | The terminal glyph does nothing | — | Cut |
| 1.24 | Status line under the last message: snoozed, settled, woke | `ui/chat.rs` `thread_status_line` | — | C | Keep |
| 1.25 | Filter the list by project | `ui/project_scope.rs` | Not saved | C | Rethink: by project or machine, saved |
| 1.26 | Auto-settle after a merge or after N days | `ui/settings.rs` | — | C | Keep, both, per project (3 days by default) |
| 1.27 | On a card: approve, answer, stop, send a message, pin, settle, snooze | — | — | C | Added |
| 1.28 | The dashboard's strip: quotas, machines, next scheduled tasks | — | — | C | Added |

### 2. The timeline and its Markdown

The owner's reasons for rethinking it: too close to T3, too dense to follow, too heavy, and
things missing. Rival: Claude's desktop application (views by detail, tool calls summarised); T3
Code next.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 2.1 | Markdown: paragraphs, headings, lists, quotes, tables, images | `ui/markdown.rs` (4,003) | — | C | Keep; math (LaTeX) added |
| 2.2 | Code blocks: highlighting, language, copy, wrap, run in the terminal | `markdown.rs` `code_block`, `highlight.rs` | Tokens differ from Shiki's | C | Rethink: also open in the editor or save to a file, line numbers, long blocks folded; running asks first |
| 2.3 | Mermaid diagrams, drawn natively | `ui/md_mermaid.rs` | Layout differs slightly from mermaid.js | C | Keep |
| 2.4 | A safe subset of HTML in answers | `ui/md_html.rs` | — | C | Keep |
| 2.5 | Paths as file chips; links with the site's icon | `ui/md_links.rs` | Favicons fetched from Google's service | C | Rethink: a path opens at its line, in Pluto or the owner's editor; images and icons load by themselves only when safe, else on a click, never through Google ([ADR-0022](adr/0022-the-triage-row-by-row.md)) |
| 2.6 | Copy a selection or a message as Markdown | `ui/md_copy.rs` | — | C | Keep |
| 2.7 | Cite a selection into the composer | `ui/cite.rs`, `ui/citations.rs` | No notice when the quoted text changed | C | Keep; also into a side question |
| 2.8 | The owner's message | `chat.rs` `user_row` | Only terminal chips react to a click | C | Rethink: a light block above the answer; edit and resend, fork from here |
| 2.9 | The agent's answer | `assistant_row` | — | C | Rethink: full width, as a page |
| 2.10 | A finished turn folded behind "Worked for…" | `pluto-core` `timeline.rs`, `fold_row` | — | C | Rethink: a summary at the head of the turn (duration, files changed, commands, result), then the answer; details folded |
| 2.11 | Superseded attempts and compaction | `Row::AttemptFold` | — | C | Keep |
| 2.12 | Tool rows and groups | `summarize_tool_group` | — | C | Rethink: grouped by step; runs of one kind merged ("read 12 files"); edits and commands one per line |
| 2.13 | An opened tool call: command, arguments, output | `ui/inspector.rs` | No shell highlighting | C | Rethink: a command as in a terminal: exit code, duration, coloured output, folded, run again |
| 2.14 | The live work row and "Thinking" | `ui/live_activity.rs` | Cropped at 256 px | C | Rethink: one live line that stacks into the summary; thinking shown live in grey, then folded to one line |
| 2.15 | "Working for 1m 20s" | `working_row` | — | C | Keep, in the live line |
| 2.16 | Virtual layout and earlier turns | `ui/timeline_view.rs` | 5,000 messages: p99 8.5 ms a frame against 4.17 | C | Rethink: within the frame budget; a thread opens whole (ADR-0017) |
| 2.17 | Minimap of the owner's messages | `ui/minimap.rs` | — | C | Rethink: an outline of the thread (messages, turns' summaries, questions, approvals) and keys to the previous and next turn |
| 2.18 | "Scroll to end" pill | `scroll_to_end_pill` | — | C | Keep |
| 2.19 | Dividers: interrupted, handoff, fork, created thread | `ui/lifecycle.rs` | — | C | Keep |

### 6. The diff and review

Rival: the Codex app (scopes, comments sent back, a hunk staged or reverted, a review's findings as
comments); Claude's desktop application and T3 Code next.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 6.0 | A full-window review mode: files, diff, comments | — | — | C | Added; the panel stays for a quick look |
| 6.1 | Scopes: the thread, uncommitted, latest turn, a chosen turn | `ui/diff_panel.rs` `diff_scope_dropdown` | — | C | Keep |
| 6.2 | The base picker | `base_ref_popup` | — | C | Rethink: by default the thread's starting point; two turns compared through checkpoints |
| 6.3 | Unified or split | `diff_panel.rs:458` | — | C | Keep |
| 6.4 | Toolbar: tree, whitespace, wrap, collapse, refresh, counts | `diff_panel.rs:418` | No find | C | Rethink: find in the diff added, unfolding its file |
| 6.5 | The changed files' tree | `diff_file_tree` | Its state is not saved | C | Keep; files marked viewed |
| 6.6 | A file's header: open, copy path, menu | `diff_file_header` | — | C | Keep |
| 6.7 | Unchanged context expanded | `diff_gap` | Cannot be commented | C | Keep |
| 6.8 | Large diffs loaded per file | `lazy_file_body` | — | C | Keep |
| 6.9 | Comments on lines, sent with the next message | `ui/diff_review.rs` | Kept in memory only | C | Rethink: saved like a draft; sent in one batch to the thread's agent or another provider's |
| 6.10 | The changed files' card under an answer | `ui/changed_files.rs` | — | C | Rethink: merged into the turn's summary (2.10) |
| 6.11 | Patch parsing and word emphasis | `pluto-core` `diff.rs` | — | C | Keep |
| 6.12 | Highlighting off the interface's thread | `highlight.rs` | — | C | Keep |
| 6.13 | Refreshed after an agent's edit | `diff_panel.rs:387` | — | C | Keep |
| 6.14 | Empty, binary and failed states | `diff_panel.rs:672` | — | C | Keep |
| 6.15 | Mark a file viewed; it folds until it changes | — | — | C | Added |
| 6.16 | Keep or reject a hunk (revert, stage) | — | — | C | Added |
| 6.17 | Cross-agent review: another provider's agent reviews, its remarks arrive as line comments the owner keeps, edits or drops | — | — | C | Added |

### 8. Files

Rival: Zed (the agent edits the same buffers, review per hunk); Warp next.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 8.1 | The project's tree, search, refresh | `ui/files.rs` `file_tree` | No keyboard, no drag | C | Rethink: by keyboard; drag a file into the composer |
| 8.2 | The tree's file menu: open with, reveal | `ui/file_menu.rs` | — | C | Keep |
| 8.3 | Breadcrumb | `files.rs:337` | Static text | C | Rethink: each step clickable |
| 8.4 | Highlighted source with line numbers | `file_source` | — | C | Keep |
| 8.5 | "Open in" the owner's editor | `ui/open_in.rs` | — | C | Keep |
| 8.6 | Small edits, saved alone | `ui/file_editor.rs` | Files over 1 MB stay read-only | C | Rethink: through the editor's live merge, never over an agent's write; real editing in the owner's editor, then Stage 7 |
| 8.7 | Images and SVG | `ui/file_previews.rs` | AVIF routed but not decoded | C | Keep, AVIF decoded |
| 8.8 | Markdown rendered, task boxes saved | `rendered_markdown` | — | C | Keep |
| 8.9 | CSV and TSV as a table | `delimited_table` | — | C | Keep |
| 8.10 | Video | `workspace_video` | No menu | C | Keep |
| 8.11 | HTML and PDF sent to the browser | `open_file_in_preview` | Not shown in place | C | Rethink: shown in place |
| 8.12 | Reloaded after an agent's edit | `workspace_mutated` | — | C | Keep |
| 8.13 | The repository's `t3.json` | `ui/project_file.rs` | — | — | Cut: the project's own TOML (28); a `t3.json` found is offered for import once |
| 8.14 | Comment a file's line for the agent, batched like the review | — | — | C | Added |

### 9. The browser

Rival: the Codex app (a shared browser, comments on elements, computer use; not on Linux); Claude's
desktop application and Antigravity next. Optional, where Chromium is installed.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 9.1 | Back, forward, reload, address, loading bar | `ui/preview.rs` | No DevTools, zoom or clearing | O | Keep; DevTools added |
| 9.2 | The local dev servers to open | `no_preview` | — | O | Keep |
| 9.3 | A Chromium launched by the client | `preview.rs` (1,255) | Duplicates the server's browser; profile thrown away | — | Cut from the uwui client: one browser, on the server; the egui client keeps it while it has no web view, to draw pages, MCP apps and designs |
| 9.4 | The server's tabs, listed and restored | `server_browser.rs` | — | O | Keep |
| 9.5 | A server tab streamed: control taken or given, dialogs, the agent's pointer | `server_browser.rs` `stream` | No file chooser or downloads | O | Keep, uploads and downloads included |
| 9.6 | Load and crash notes | `preview.rs:859` | No retry | O | Keep |
| 9.7 | Device surface | `surfaces.rs:69` | Missing | — | Cut: computer use's isolated session holds the emulator |
| 9.8 | Device settings | `settings.rs:1624` | Switches only show a notice | — | Cut, with 9.7 |
| 9.9 | server: open, navigate, resize, set the appearance of a watched tab | `server: toolkits/preview` | — | O | Keep |
| 9.10 | server: the page's elements, diagnostics, screenshot | `preview_snapshot` | — | O | Keep |
| 9.11 | server: click, type, hover, select, drag, scroll, evaluate | `preview_*` | — | O | Keep |
| 9.12 | server: page dialogs | `preview_dialog` | — | O | Keep |
| 9.13 | server: record a tab's video | `preview_recording_*` | — | O | Keep, watched in Pluto |
| 9.14 | server: list and close tabs | `previewControls` | — | O | Keep |
| 9.15 | server: the pinned headless browser | `PreviewBrowser.ts` | Throwaway profile | O | Rethink: a profile kept per project, its logins kept |
| 9.16 | server: uploads and downloads | `ServerBrowserStream.ts` | Unused by the client | O | Keep, used |
| 9.17 | server: drive simulators and emulators | `toolkits/device` | No client surface | O | Rethink: through computer use's isolated session (ADR-0018) |
| 9.18 | Comment an element of the page for the agent | — | — | O | Added |

### 11. Pull requests

Rival: T3 Code (PRs linked to threads, an agent woken on failed checks); Claude's desktop
application next (CI fixed, merge when ready). The forges kept are GitHub, GitLab and Forgejo
(Codeberg); Bitbucket and Azure DevOps are cut.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 11.1 | A page of every PR of every host | `ui/pull_requests.rs` (1,377) | No detail; a row click shows a notice | C | Rethink: the PRs of Pluto's threads (open, in review, CI red), from the dashboard |
| 11.2 | Search among PRs | `matches_query` | Promises host qualifiers that match nothing | — | Cut |
| 11.3 | Filters: state, involvement, draft, review, checks, project, host | `pull_requests_menu` | Not saved | — | Cut |
| 11.4 | Host menu | `pull_requests.rs:1061` | — | C | Keep, for GitHub, GitLab and Forgejo |
| 11.5 | Refresh | `request_pull_requests` | Never invalidates the cache | C | Keep |
| 11.6 | A row: state, checks, review, counts, author, labels | `pull_request_row` | — | C | Keep |
| 11.7 | Loading, empty and error states | `pull_requests_empty` | Claims hosts were searched | C | Keep, true |
| 11.8 | The thread's linked PRs, stacks, watch, unlink | `ui/linked_pull_requests.rs` | — | C | Rethink: a strip in the thread: state, CI, reviewers' remarks, each forwarded to the agent; files in the review view |
| 11.9 | Link a PR by number, URL or checkout command | `link_pull_request_dialog` | — | C | Keep |
| 11.10 | Palette entries | `palette.rs:369` | — | C | Keep |
| 11.11 | Link, unlink, watch commands | `pull_request_command` | — | C | Keep |
| 11.12 | The thread's current PR state | `thread_pull_requests.rs` | Polls every minute | C | Rethink: on events; GitHub polled only while a PR is followed (ADR-0018) |
| 11.13 | PR references parsed | `pluto-core` `change_request.rs` | — | C | Keep |
| 11.14 | "#" in the composer lists the project's PRs | `ui/command_menu.rs` | — | C | Keep |
| 11.15 | server: link, unlink, list, watch, unwatch tools | `toolkits/pullRequests` | — | C | Keep |
| 11.16 | server: review guides of up to 20 PRs | `write_pull_request_guides` | Minutes and turns per guide | — | Cut |
| 11.17 | server: the hosts' PR interface | `pullRequests.*` | 28 methods unused | C | Rethink: GitHub, GitLab and Forgejo (Codeberg), and only what Pluto uses; Bitbucket and Azure DevOps cut |
| 11.18 | Followed to the merge: on a failed CI or a remark, Pluto offers to let the agent fix; it waits for the owner's go to push | — | — | C | Added |

### 12. Search

Rival: Cursor (find in a thread, a local index of transcripts); T3 Code next.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 12.0 | One search, in the palette: threads, messages, files, code, commands, settings, on every machine, by section | — | — | C | Added |
| 12.1 | Find in the thread (Mod+F) | `ui/thread_find.rs` | — | C | Keep; also in an open file |
| 12.2 | Search every thread's messages | `ui/thread_search.rs` | One machine only | C | Rethink: in the one search; archived threads and imported history too; a full-text index on each server, and search by meaning through a local embedding model |
| 12.3 | The sidebar's search | `sidebar.rs` `thread_list` | Duplicates the palette's | — | Cut |
| 12.4 | Search in the project's content (Ctrl+Shift+F) | `ui/content_search.rs` | Caps at 500 matches | C | Rethink: a section of the one search |

### 14. Keybindings

Rival: Zed (every action remappable, base keymaps, Vim and Helix modes).

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 14.1 | A key resolved to a command | `ui/shortcuts.rs` | Thirteen bound commands do nothing | C | Rethink: by zone (thread, composer, review, terminal…) |
| 14.2 | When-clauses evaluated against the window | `run_shortcuts` | — | — | Cut: zones replace them |
| 14.3 | Palette, usage, new thread keys | `run_command` | — | C | Keep |
| 14.4 | Panel, surface, terminal and diff keys | `run_command` | — | C | Keep |
| 14.5 | Previous, next, Mod+1…9, back and forward | `ordered_sidebar_threads` | Order can differ from the list | C | Keep, in the dashboard's order |
| 14.6 | Settle, pin, copy the thread's reference | `run_command` | — | C | Keep |
| 14.7 | Composer and model picker keys | `run_command` | — | C | Keep |
| 14.8 | A project action's key | `script.<id>.run` | — | C | Keep |
| 14.9 | The page: record, save, reset, remove | `ui/keybindings.rs` | "Open keybindings.json" shows a notice | C | Rethink: by zone, with search; the same in a readable TOML file |
| 14.10 | Conflict warnings | `conflict_labels` | — | C | Keep |
| 14.11 | Add a binding, search | `new_keybinding_control` | — | C | Keep |
| 14.12 | The when-clause builder | `ui/when_builder.rs` (1,013) | — | — | Cut |
| 14.13 | The table of defaults | `DEFAULTS` | — | C | Keep |
| 14.14 | A command's key shown beside it | `effective_shortcut_label` | The palette's labels are fixed | C | Keep |
| 14.15 | Digit chips while Mod is held | `legacy_jump_labels` | Legacy sidebar only | — | Cut |
| 14.16 | Reloaded, with a toast when the file has errors | `app.rs` `keybindings_reloaded` | — | C | Rethink: the TOML file watched, its errors shown |
| 14.17 | Modal navigation outside the editor (j/k, gg, /), off by default, on the editor's modal core | — | — | O | Added |
| 14.18 | A leader key (Space in modal mode) with short sequences | — | — | O | Added |

### 15. Agents' pages and MCP apps

The owner's reasons: heavy (a Chromium off screen draws each page) and not powerful enough. Rival: T3
Code (pages and MCP apps in the thread); Claude's artifacts (versions, publishing) next.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 15.1 | An agent's page shown in the thread | `ui/html_render.rs` | Drawn through an off-screen Chromium | C | Rethink: a preview in the thread, the page whole in the artifacts panel, in uwui's isolated web view (no network or files by default) |
| 15.2 | The page's reference, height and theme | `pluto-core` `html_render.rs` | — | C | Keep |
| 15.3 | Open in Design or in the system browser | `html_render.rs:13` | — | C | Rethink: open in the panel, the browser, or export |
| 15.4 | MCP apps in the thread, their JSON-RPC bridge | `ui/mcp_apps.rs` (906) | No full screen or downloads; the same machinery as 15.1 twice | C | Rethink: as 15.1, one machinery |
| 15.5 | An app's tool calls ask first when not read-only | `ask_mcp` | — | C | Keep |
| 15.6 | An app's context, size and closing | `mcp_notification` | — | C | Keep |
| 15.7 | An app's reference, permissions, CSP | `pluto-core` `mcp_app.rs` | — | C | Keep |
| 15.8 | Placed where the tool call happened | `timeline.rs:1238` | — | C | Keep |
| 15.9 | server: `html_render` publishes a page | `toolkits/html` | — | C | Keep |
| 15.10 | server: `html_preview` renders a page to an image | `toolkits/html` | — | O | Keep, where Chromium is installed |
| 15.11 | server: the pages' renderer | `HtmlRender.ts` | — | O | Keep, for 15.10 |
| 15.12 | Living artifacts: versions, replayed, exported, changed in the conversation (ADR-0020) | — | — | C | Added |

### 16. Providers and models

Rival: T3 Code (several accounts per provider, one prompt to several models); OpenCode and Zed
next.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 16.1 | The providers' page: instances and an editor | `ui/providers.rs` (2,534) | Rows overflow a fixed card | C | Rethink: each driver describes its fields; part of the one configuration (ADR-0018) |
| 16.2 | A provider's row: mark, version, status, switch | `provider_list_row` | The status's detail is dropped | C | Keep, the detail shown |
| 16.3 | The editor's header: name, accent, version, delete | `provider_editor` | — | C | Keep |
| 16.4 | Signed in as whom, or what is missing | `provider_status_line` | The detail is dropped | C | Keep |
| 16.5 | Runtime fields per driver | `driver_fields` | Written by hand, drifted from the fork | C | Rethink: described by the server's driver |
| 16.6 | Environment variables, secrets write-only | `ui/providers/environment.rs` | — | C | Keep; secrets in the keyring (ADR-0009) |
| 16.7 | Models: favourites, hidden, order | `provider_models` | No filter | C | Keep, a filter added |
| 16.8 | The custom model editor: names and option descriptors | `ui/custom_model_editor.rs` (1,223) | — | C | Rethink: an id and a name; options from the agent |
| 16.9 | Add an instance: driver, identity, settings | `ui/providers/add_instance.rs` | Offers v0.0.45's "coming soon" tiles | C | Rethink: the drivers the server offers |
| 16.10 | Usage limit hubs (CLIProxyAPI) | `providers.rs:973` | Cannot be added | — | Cut |
| 16.11 | The providers' health checked every N seconds | `interval_field` | — | — | Cut: status read when the picker or page opens and when a session starts |
| 16.12 | "Applying settings for…" above the page | `providers_scope_sentence` | Inert | C | Rethink: the layers icon (19) |
| 16.13 | The model picker | `ui/pickers.rs` (1,496) | No arrows, Enter or Escape | C | Rethink: one searchable list, favourites first, then by agent, local models equal, each agent's quota and state beside it, by keyboard |
| 16.14 | One prompt to several models, one thread each | `toggle_fanout` | — | C | Rethink: their threads side by side (ADR-0003) |
| 16.15 | The model's options: effort, fast mode | `traits_menu` | Some sections missing | C | Keep |
| 16.16 | Runtime modes: supervised to full access | `RUNTIME_MODES` | — | C | Keep |
| 16.17 | Why a provider is not ready, and the banner | `ui/thread_error.rs` | — | C | Keep |
| 16.18 | A started thread keeps its driver unless the context moves | `locked_driver` | — | C | Keep |
| 16.19 | server: the drivers' contract | `provider-core` | — | C | Keep, in Rust ([ADR-0006](adr/0006-agents-drivers.md)) |
| 16.20 | server: Codex | `CodexDriver.ts` | — | C | Keep |
| 16.21 | server: Claude | `ClaudeDriver.ts` | — | C | Keep |
| 16.22 | server: Cursor | `provider-cursor` | — | — | Cut (ADR-0003) |
| 16.23 | server: Grok | `provider-grok` | — | C | Keep |
| 16.24 | server: OpenCode | `provider-opencode` | — | C | Keep |
| 16.25 | server: Antigravity | `AntigravityDriver.ts` | — | C | Keep |
| 16.26 | server: Pi | `provider-pi` | The client cannot offer it | C | Keep, offered |
| 16.27 | server: Muse | `provider-muse` | The client cannot offer it | C | Keep, offered |
| 16.28 | server: Kimi Code | `provider-kimi-code` | No application text, rollback or fork | C | Keep |
| 16.29 | server: DeepSeek Harness | `provider-deepseek-harness` | No fork, rollback, plan mode or application text | C | Keep |
| 16.30 | server: Mistral Vibe | `provider-mistral-vibe` | Reports no limits | C | Keep |
| 16.31 | server: Z Code | `provider-zcode` | No real turn run yet | C | Keep |
| 16.32 | server: any agent of the ACP registry | `provider-acp-registry` | — | C | Keep |
| 16.33 | server: instances, their homes and defaults | `serverSettings.ts` | — | C | Keep |
| 16.34 | server: agents' runtimes installed and updated | `maintenance.ts` | The client only shows a notice | C | Rethink: Pluto says what is missing or old and proposes the command; it downloads nothing itself |
| 16.35 | server: providers' sign-in | `CodexChatGptAuth.ts` | No client interface | C | Rethink: the official sign-in opened in a terminal; "Sign in with ChatGPT" where allowed ([ADR-0020](adr/0020-the-chat.md)) |
| 16.36 | server: status, version and models probed | `snapshotProbe.ts` | — | C | Keep |
| 16.37 | server: titles, commit messages, branch names generated | `textGenerationOperations.ts` | — | C | Keep |
| 16.38 | server: a thread's MCP tools given to its agent | `McpProviderSessions.ts` | — | C | Rethink: the one configuration (MCP, skills, instructions), given to each agent in its own format |
| 16.39 | server: instructions given to each agent | `runtimeInstructions.ts` | — | C | Keep, measured against the tokens' budget (ADR-0017) |

### 17. Local models

Rival: LM Studio (a downloader, memory estimated before loading, served to coding agents); Ollama
and Jan next. Core; Ollama is installed by the owner.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 17.1 | An Ollama instance, for chats | `provider-ollama` | A code project's thread is refused | C | Rethink: chat and code, code through Pluto's agent (Stage 4) |
| 17.2 | Its models, thinking levels, vision, context | `modelCatalog.ts` | — | C | Keep |
| 17.3 | A local chat turn, streamed | `adapter.ts` | — | C | Keep |
| 17.4 | The conversation kept by the server, handed over | `transcript.ts` | — | C | Keep |
| 17.5 | Compaction, rollback, interrupt | `adapter.ts` | No steering, fork, approvals or plan | C | Keep; the rest supplied by Pluto (ADR-0017) |
| 17.6 | Web search (SearXNG or Brave) and fetch | `webTools.ts` | — | C | Rethink: a keyed engine by default; SearXNG as an option |
| 17.7 | Pluto's MCP tools for local models | `mcpClient.ts` | — | C | Keep |
| 17.8 | Derived `pluto/<model>:<k>k` models and an OpenCode companion for code | `codeModels.ts` | The companion is relabelled as Ollama | C | Rethink: the context set per model and sent with each request; the companion replaced by Pluto's agent |
| 17.9 | Titles and memory on the local model | `textGeneration.ts` | — | C | Keep |
| 17.10 | Its settings rows | `providers.rs` | Written by hand in three places | C | Rethink: described by the driver |
| 17.11 | Local models in the picker, chats only | `pickers.rs` `CHAT_ONLY_DRIVERS` | — | C | Rethink: equal everywhere |
| 17.12 | Local tokens on the usage page | — | Missing | C | Added |
| 17.13 | Installed models: size, loaded, GPU memory, unload, delete | — | — | C | Added |
| 17.14 | A model pulled by its name, on request | — | — | C | Added |
| 17.15 | A catalog of Ollama's library, with whether a model fits the GPU; network on request | — | — | O | Added |
| 17.16 | Any OpenAI-compatible endpoint (llama.cpp, LM Studio, vLLM) for chat and Pluto's agent | — | — | C | Added |

### 18. Usage and limits

Rival: T3 Code (one usage page across providers, several accounts pooled); Claude's desktop
application next. No rival warns before a quota runs out.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 18.1 | Cost, tokens, limits; periods; refresh | `ui/usage.rs` | No saved preferences | C | Keep |
| 18.2 | Cost and tokens per provider, by day | `ui/usage_summary.rs` | — | C | Keep; DeepSeek Harness and local models included |
| 18.3 | By model or by period | `breakdown_by_model` | No model detail | C | Keep |
| 18.4 | Quota windows per account: left, pace, reset | `limit_pools` | — | C | Keep |
| 18.5 | Notices for limits that failed | `limit_notices` | — | C | Keep |
| 18.6 | A window's popover | `segment_popover` | — | C | Keep |
| 18.7 | Prices per model, editable | `ui/usage_prices.rs` | — | C | Keep |
| 18.8 | The context meter, "Compact session" | `ui/context_meter.rs` | — | C | Keep |
| 18.9 | The composer's banner on a limit: resume or snooze until reset | `ui/composer_notices.rs` | — | C | Rethink: resume at the reset, or continue with another agent |
| 18.10 | Usage's keys | `shortcuts.rs` | — | C | Keep |
| 18.11 | server: the usage history, priced | `UsageService.ts` | — | C | Keep |
| 18.12 | server: each driver's history reader | `*Usage.ts` | None for Ollama and ACP | C | Keep; Ollama and ACP added |
| 18.13 | server: the scan cache | `usageScanCache.ts` | — | C | Keep |
| 18.14 | server: prices, overrides, mappings | `usagePricing.ts` | — | C | Keep |
| 18.15 | server: usage limit hubs | `UsageLimitSources.ts` | — | — | Cut, with 16.10 |
| 18.16 | server: each provider's quota read | `*UsageLimits.ts` | Vibe reports none | C | Keep, through each agent's own interface, never its stored credentials |
| 18.17 | server: Claude's banked resets claimed | `claudeResetCredits.ts` | Reads Claude Code's stored OAuth token, against [ADR-0009](adr/0009-secrets-privacy-and-the-sandbox.md) and Anthropic's terms | — | Cut |
| 18.18 | server: runs a limit stopped resumed at the reset | `UsageLimitRecoveryWorker.ts` | — | C | Keep, at the owner's choice (18.9) |
| 18.19 | server: windows and a task's cost over MCP | `toolkits/orchestrator` | — | C | Keep |
| 18.20 | A warning by pace ("Claude runs out in 40 min, before its reset") and at 95 %, with a suggested switch, never made alone | — | — | C | Added |

### 19. Settings

Rival: T3 Code (scope and inheritance shown per value); Zed next.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 19.1 | Thirteen pages, a header, back | `ui/settings.rs` (4,603) | Diagnostics and licences have no page, only buttons that show a notice | C | Rethink: a few pages by subject: General, Agents, Appearance, Keybindings, Machines and devices, Privacy, Storage |
| 19.2 | Search over the settings | `ui/settings_search.rs` | Some rows missing | C | Keep |
| 19.3 | "Applying settings for…" with project and machine pickers | `ui/settings_scope.rs` | One machine only | C | Rethink: machine, project and thread |
| 19.4 | Project overrides | `PROJECT_SCOPED_KEYS` | — | C | Keep, on the project's page (28) |
| 19.5 | The layers icon: where a value comes from, reset | `ui/setting_inheritance.rs` | — | C | Keep |
| 19.6 | Restore device defaults | `settings.rs:2531` | No confirmation | C | Rethink: lists what resets, confirms, keeps a copy to undo |
| 19.7 | Client settings in a JSON file | `save_client_settings` | — | C | Rethink: readable TOML per machine and project, versioned |
| 19.8 | New threads: model, permissions, workspace | `general_sections` | — | C | Keep |
| 19.9 | Organisation: grouping, working shelf, auto-settle | `general_sections` | The working shelf does nothing | C | Rethink: auto-settle kept (1.26); grouping and the working shelf cut |
| 19.10 | Behaviour: eleven switches | `general_sections` | "Collapse composer on scroll" does nothing | C | Rethink: collapse on scroll made to work; rich text, proactive panels and the send key kept; provider update checks cut |
| 19.11 | Start from origin, add-project folder | `general_sections` | — | C | Keep |
| 19.12 | Confirmations: unpin, archive, delete, quit | `general_sections` | — | C | Keep |
| 19.13 | The model for generated text | `general_sections` | — | C | Keep |
| 19.14 | About, diagnostics, licences, legacy fold | `general_sections` | Two buttons show a notice | C | Rethink: a Diagnostics and licences page (logs and crash reports kept on the machine); the legacy fold cut |
| 19.15 | Appearance | `appearance_sections` | — | C | Rethink: as family 20 |
| 19.16 | Keybindings page | `keybindings_list` | — | C | Rethink: as family 14 |
| 19.17 | SnapShots: capture a window into the draft | `snap_shot_sections` | Inert | C | Rethink: the capture of ADR-0020, through Wayland's portal |
| 19.18 | Personalisation: instructions and memories | `ui/memory_settings.rs` | — | C | Keep (family 23) |
| 19.19 | Scheduled tasks | `ui/scheduled_tasks.rs` | — | C | Keep (family 26) |
| 19.20 | Integrations: browser access, device support | `integrations_sections` | Inert rows | C | Rethink: browser and computer use are rights of the sandbox; device rows cut |
| 19.21 | Source control: repositories, discovery, writing style, Bitbucket | `source_control_sections` | — | C | Rethink: Bitbucket cut |
| 19.22 | Storage: worktree cleanup, retention | `storage_sections` | — | C | Rethink: worktrees' sizes, finished or merged ones cleaned, never with uncommitted work (ADR-0017) |
| 19.23 | Connections | `connections_sections` | A fixed notice | C | Rethink: Machines and devices: servers, paired devices, their rights, revoked at once |
| 19.24 | Archive: unarchive, delete | `archive_sections` | — | C | Keep |
| 19.25 | The project page | `ui/projects_page.rs` | — | C | Rethink: family 28 |
| 19.26 | server: settings saved, checked, announced | `serverSettings.ts` | — | C | Keep, versioned |
| 19.27 | Defaults read from `t3.json` | `project_file.rs` | — | — | Cut, with 8.13 |

### 20. Themes and typography

Rival: Zed (themes previewed live, light and dark apart, interface and code fonts). T3's
environment themes are T3 Chat, Grove, Ocean, Ember and Iris.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 20.1 | The theme the server names | `app.rs` `sync_theme` | No selector | — | Cut |
| 20.2 | T3's five themes | `assets/themes/built-in-dark.json` | Dark only | — | Cut |
| 20.3 | Themes a machine publishes | `server: environmentTheme.ts` | — | — | Cut |
| 20.4 | Pluto's own dark theme, its violet and glow | `theme.rs` `pluto_stock` | — | C | Rethink: Pluto's theme on uwui, dark and light, and high contrast |
| 20.5 | Dark only | `theme.rs` | — | C | Rethink: dark and light, following the system |
| 20.6 | Code colours, pierre-dark | `highlight.rs` | — | C | Rethink: Pluto's code theme, light and dark; a code theme imported (VS Code, Helix) for the editor |
| 20.7 | Diff colours: red and green, or blue and orange | `theme.rs` | — | C | Keep |
| 20.8 | A provider instance's accent colour | `ui/accent_picker.rs` | — | C | Keep |
| 20.9 | Interface and monospace fonts, sizes | `ui/typography.rs` | — | C | Rethink: two families, interface and code, with their sizes; Geist by default |
| 20.10 | Advanced: prompt, code and terminal fonts apart | `typography.rs` | — | — | Cut |
| 20.11 | Installed fonts through fontconfig, Geist behind | `theme.rs` `install_fonts` | Fonts held twice, 4.9 MiB | C | Rethink: loaded once |
| 20.12 | The Lucide catalog | `ui/lucide.rs` | — | C | Keep |
| 20.13 | Appearance rows T3 has: contrast, glass, motion | — | Missing | C | Rethink: Pluto's accent and density added; contrast by the high-contrast theme; motion and transparency follow the system |

### 24. Sandboxed Python

Rival: Claude (code execution and files, on every surface); Open WebUI next. Optional: it needs
bubblewrap and Python.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 24.1 | `code_run`: Python for a chat, with numpy, pandas, matplotlib | `server: toolkits/code` | — | O | Keep; charts and files shown in the chat and exportable |
| 24.2 | bubblewrap: no network, the system read-only | `sandboxCommand` | — | O | Rethink: under the one sandbox of [ADR-0009](adr/0009-secrets-privacy-and-the-sandbox.md); network opened per chat on request |
| 24.3 | Limits: 2 GiB, 300 s, 64 KiB of output | `handlers.ts` | — | O | Keep |
| 24.4 | A working folder per chat | `handlers.ts` | — | O | Keep |
| 24.5 | Chats only | `handlers.ts` | — | O | Keep |
| 24.6 | Its row in the timeline | `tool_presentation.rs` | — | O | Keep |

### 25. Design projects

The owner uses them for mockups of the owner's applications and for pages and sites. Rival:
Claude Design (a canvas, comments, several proposals, handed to Claude Code); Cursor's design mode
next. Optional, a kind of project.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 25.1 | The Design surface | `ui/design.rs` (1,030) | — | O | Rethink: the artifacts panel (15) is the studio |
| 25.2 | Versions of a design | `thread_pages` | — | O | Keep; two versions compared side by side |
| 25.3 | A project's HTML files as pages, reloaded | `changed_html_files` | — | O | Keep |
| 25.4 | Follow the newest page, or pin one | `design_surface` | — | O | Keep |
| 25.5 | Full, tablet and phone widths | `Width` | — | O | Keep |
| 25.6 | Comment on an element | `comment_overlay` | — | O | Rethink: batched like the review's comments |
| 25.7 | The studio: the page beside a narrow chat | `studio_active` | No versions without git | O | Keep |
| 25.8 | "New design": brief, references, kind, agent, folder | `ui/design_home.rs` (1,008) | — | O | Rethink: a kind of project in "add a project" (28), its brief the first message |
| 25.9 | Designs in `~/Designs`, each with git | `DESIGNS_ROOT` | — | O | Keep |
| 25.10 | A design session from a thread | `thread_menu.rs` | — | O | Keep |
| 25.11 | server: the design charter, per kind, and `DESIGN.md` | `designCharter.ts` | — | O | Rethink: in the one configuration |
| 25.12 | server: the charter given to each agent | `designProfile.ts` | Cursor, Muse and Pi get none | O | Rethink: every agent, through the one configuration |
| 25.13 | Several proposals from one or several models, one chosen | — | — | O | Added |
| 25.14 | Inspect an element: colours, spacing, fonts | — | — | O | Added |
| 25.15 | OpenDesign's catalog of design systems, downloaded on request | — | — | O | Added |
| 25.16 | A validated mockup opens a code thread in its target project | — | — | O | Added |

### 27. Subagents and the agents panel

Rival: Claude Code (a tasks pane: subagents and workflows, opened or stopped); the Codex app next.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 27.1 | A card per subagent in the thread | `ui/lifecycle.rs` `subagent_card` | No hover card | C | Keep |
| 27.2 | Neighbouring subagents folded into one row | `subagent_group` | — | C | Keep |
| 27.3 | The lineage: parent, forks, subagents, transfers | `ui/thread_details.rs` | — | C | Keep, as a lineage card with a workflow's phases |
| 27.4 | Stop a working child | `stop_of` | — | C | Keep |
| 27.5 | Merge a fork back | `merge_back_run` | — | C | Keep |
| 27.6 | "Subagent of" at the thread's head | `chat.rs` | — | C | Keep |
| 27.7 | The Agents surface | `ui/agents_panel.rs` (850), `subagents.rs` | Removed by T3 v0.1.0 | — | Cut |
| 27.8 | server: `delegate_task` | `toolkits/orchestrator` | Refused tools still listed to a child | C | Keep |
| 27.9 | server: a child's narrower credential | `McpSessionRegistry.ts` | A shell can read another token | C | Keep, under the sandbox |
| 27.10 | server: a child's status and cancellation | `task_status`, `task_cancel` | — | C | Keep |
| 27.11 | server: stop a workflow | `t3_workflow_stop` | — | C | Keep |
| 27.12 | server: capabilities and providers' windows | `orchestrator_capabilities` | — | C | Keep |
| 27.13 | server: a child's tokens and cost | `OrchestratorMcpService.ts` | Not added to the parent's | C | Rethink: shown on the child and summed into the parent |
| 27.14 | Children under their parent's card in the dashboard, foldable, delegation alike | — | — | C | Added |

### 28. Projects and scripts

The owner's reason: too many pages and settings. Rival: the Codex app (a project's environment in
files, setup scripts, actions in its bar); T3 Code next.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 28.1 | The project's settings page | `ui/projects_page.rs` | One project, not a group | C | Rethink: one page per project: its threads, worktrees and their size, PRs, settings, its MCP servers, skills and instructions, actions |
| 28.2 | Its icon: Lucide, emoji, monogram, favicon | `ui/project_icon.rs`, `favicon.rs` | — | C | Rethink: a coloured monogram by default, or one chosen |
| 28.3 | The icon chooser | `ui/icon_picker.rs` (679) | — | C | Rethink: a small grid with search |
| 28.4 | The project's defaults for new threads | `projects_page.rs` | — | C | Keep |
| 28.5 | Actions in the thread's header, run in the terminal | `ui/project_actions.rs` | — | C | Rethink: actions found by Pluto (package.json, Justfile, Makefile, Cargo, the flake's apps), in the palette and a button of the thread |
| 28.6 | The action editor | `ui/action_editor.rs` (1,157) | — | — | Cut: the owner's actions in the project's TOML or a small form |
| 28.7 | Actions imported from `t3.json` | `ui/script_import.rs` | — | — | Cut |
| 28.8 | `t3.json` read and checked | `ui/project_file.rs` | — | — | Cut |
| 28.9 | Open in the owner's editor | `ui/open_in.rs` | — | C | Keep |
| 28.10 | Project groups | `ui/project_grouping.rs` | One machine only | C | Rethink: one project is one repository, on every machine |
| 28.11 | The legacy project menu | `legacy_project_menu` | — | — | Cut |
| 28.12 | Add a project from a folder | `palette.rs` `View::AddProject` | No clone, no new project | C | Rethink: a folder, a URL cloned, a new empty project, on a chosen machine |

### 31. The Android application

Rivals: T3 Code's mobile application (live notifications, push, reviews offline); Paseo (open
source, an encrypted relay of its own) next; Claude's application for the chat. Optional, an
application of its own. Family numbers in brackets are the desktop's.

| Id | Feature | Where | Problem today | C/O | Mark |
|---|---|---|---|---|---|
| 31.1 | Connect: address and token, kept by the Keystore | `ui/Connect.kt`, `Secret.kt` | https not tried | O | Rethink: paired by a QR code from the desktop, its own key and rights; typed by hand as a fallback |
| 31.2 | Reconnect at launch, or "Can't reach your server" | `ui/Connect.kt`, `App.kt` | — | O | Rethink: every machine, by the fastest path (ADR-0018) |
| 31.3 | Reconnect, retry, offline bar | `App.kt` `NetBar`, `pluto-ffi` `session.rs` | The core wakes once a second | O | Rethink: threads and chats of the last 30 days kept, encrypted, under a cap (500 MB); messages written offline leave later |
| 31.4 | Android 17's local network permission | `LocalNetwork.kt` | — | O | Keep |
| 31.5 | Notifications through a kept connection | `Notifications.kt`, `ConnectionService.kt` | Battery not measured | O | Rethink: UnifiedPush; live notifications with answers and approvals (ADR-0018) |
| 31.6 | Chat and Code spaces, research, image, temporary chats | `ui/Home.kt` | The chat space cannot be created from the phone | O | Rethink: two tabs, Chat and Threads; reopens on the last |
| 31.7 | The home draft and its project, workspace, branch chips | `ui/Home.kt` | — | O | Rethink: a new chat from Chat, a new thread from Threads |
| 31.8 | The composer | `ui/Home.kt` `Composer` | — | O | Keep; dictation added (ADR-0018) |
| 31.9 | Photos, camera and files attached | `ui/Attachments.kt` | — | O | Keep |
| 31.10 | Full-screen image viewer | `ui/Viewer.kt` | — | O | Keep |
| 31.11 | The drawer: needs you, threads, settled, chats by day | `ui/Drawer.kt` | Snooze not wired | O | Rethink: the Threads tab is the dashboard, snooze included (1) |
| 31.12 | The drawer's search and pull to refresh | `ui/Drawer.kt` | — | O | Rethink: the one search (12) |
| 31.13 | A thread's actions: rename, pin, fork, settle, delete | `ui/ComposerSheets.kt` | — | O | Keep; snooze added |
| 31.14 | The thread and its timeline | `ui/Thread.kt` | Pages and MCP apps missing | O | Rethink: the desktop's rules (2) laid out for the phone; pages and artifacts as previews opened full screen |
| 31.15 | Approvals | `ApprovalCardView` | — | O | Keep; the fingerprint for sensitive ones (ADR-0018) |
| 31.16 | Questions | `QuestionCardView` | — | O | Keep |
| 31.17 | A subagent's request | `SubagentCard` | — | O | Keep |
| 31.18 | Changed files and the diff page | `ui/Diff.kt` | Not seen on a device | O | Rethink: review and comments (ADR-0018) |
| 31.19 | The model sheet | `ComposerSheets.kt` `ModelSheet` | The third copy of the providers' list | O | Rethink: the one picker (16) |
| 31.20 | Project, workspace and branch sheets | `ComposerSheets.kt` | — | O | Keep |
| 31.21 | Scheduled tasks | `ui/Tasks.kt` | — | O | Keep |
| 31.22 | The task editor | `ui/Tasks.kt` | — | O | Keep; event triggers added (26) |
| 31.23 | Usage | `ui/Usage.kt` | Not measured on a phone | O | Keep |
| 31.24 | Settings | `ui/Settings.kt` | No Kotlin test in CI | O | Rethink: the device's lock and rights added (ADR-0018) |
| 31.25 | Personalisation | `ui/Memory.kt` | Chat projects' memories missing | O | Keep |
| 31.26 | Projects and a project's page | `ui/Projects.kt` | — | O | Keep |
| 31.27 | Markdown in answers | `ui/Markdown.kt` | HTML stays text | O | Rethink: the desktop's rules (2) |
| 31.28 | Open source licences | `Settings.kt` `LicensesPage` | — | O | Keep |
| 31.29 | Theme, motion and the planet | `ui/Theme.kt`, `Widgets.kt` | — | O | Keep: Pluto's theme, dark and light |
| 31.30 | Project badges | `Widgets.kt` `ProjectBadge` | — | O | Keep |
| 31.31 | Providers' marks | `ui/Icons.kt` | — | O | Keep |
| 31.32 | Codex's generated images | `Thread.kt` `GeneratedImage` | Asked every 3 s while a run works | O | Rethink: on events, no polling |
| 31.33 | A demo session in debug builds | `MainActivity.kt` | — | O | Keep, debug builds only |

## Added on 2026-11-08

Features Pluto does not have today, decided on 2026-11-08:

- [ADR-0018](adr/0018-the-markets-features-and-the-triage.md): event triggers and pull requests
  followed to their merge; one configuration for every agent; side questions; local dictation;
  runs in the vendors' clouds, optional; computer use in an isolated session; on the phone,
  UnifiedPush, offline writing, models on the phone, live notifications, two panes on large
  screens, a fingerprint lock and rights per device.
- [ADR-0019](adr/0019-a-code-editor-in-pluto.md): the code editor.
- [ADR-0020](adr/0020-the-chat.md): one memory; files read and a library indexed locally; a
  document written together, living artifacts, exports; a global shortcut, a window shown to the
  model, actions on selected text, models compared side by side; local voice; images by vendors and
  locally; one history imported from the vendors; the chat's tasks and connectors; locked chats.
