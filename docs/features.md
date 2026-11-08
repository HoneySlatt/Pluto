# Features

Every feature of Pluto today, by family, and what the owner decided for each: keep, rethink or cut
([ADR-0010](adr/0010-todays-features-triaged.md)). The marks are the start of the specification of
the Rust server and of the uwui client ([M1.3](PLAN.md#m13--the-triage)).

- **Keep**: the family and its form go to the specification.
- **Rethink**: its need stays; its form is decided row by row below, each row with its problem,
  the rival that does it best, and whether it is core or optional
  ([ADR-0003](adr/0003-what-pluto-is-to-become.md)).
- **Cut**: it leaves the egui client in M1.4, and the specification.

The families were marked on 2026-11-08 ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md)).
Their rows are not yet listed: `docs/parity/`, the README, the fork's `PLUTO.md` and the code are
their sources. Paths are under `src/` unless named otherwise; sizes are lines of Rust, rounded.

## The desktop

| # | Family | What it does today | Where | Size | Mark | Direction |
|---|---|---|---|---|---|---|
| 1 | Threads and the sidebar | Thread cards and their state, pinned and settled shelves, snooze, auto-settle, the thread's menu, ordering by drag | `ui/sidebar*.rs`, `ui/legacy_sidebar.rs`, `ui/thread_menu.rs`, `snooze.rs`, `ui/park.rs` | 7,400 | Rethink | Merged with the dashboard of ADR-0003; one sidebar, the legacy one removed |
| 2 | The timeline and its Markdown | Folded turns, tool groups, thinking, Mermaid, citations, a subset of HTML, highlighted code | `ui/markdown.rs`, `ui/md_*.rs`, `ui/cite*.rs`, `ui/timeline_view.rs`, `highlight.rs`, `crates/pluto-core/src/timeline.rs` | 8,600 | Rethink | The owner's reasons, to ask with the rows (recommended: keep) |
| 3 | The composer and attachments | The prompt's editor, mentions, drafts per thread, prompt history, context chips, the queue, steering | `ui/composer*.rs`, `ui/attachments.rs`, `ui/mentions.rs`, `ui/drafts.rs`, `ui/prompt_*.rs`, `ui/queue.rs` | 8,800 | Keep | Local dictation added |
| 4 | Agents' requests | Approvals, the agent's questions, secrets entered privately | `ui/requests_panel.rs`, `ui/secret_request.rs`, `crates/pluto-core/src/requests.rs` | 1,000 | Keep | In the grid of agents' abilities ([ADR-0017](adr/0017-pluto-measured-against-the-market.md)) |
| 5 | Plans | The proposed plan's card: refine, implement, save, implement in a new thread | `ui/plan_card.rs` | 1,100 | Keep | Supplied to every agent (ADR-0017) |
| 6 | The diff and review | Working tree, branch and turn scopes, the changed files' card | `ui/diff_panel.rs`, `diff_review.rs`, `changed_files.rs`, `crates/pluto-core/src/diff.rs` | 3,500 | Rethink | The market's best review view, comments sent to an agent (ADR-0018) |
| 7 | The terminal | A real terminal per thread, in a drawer, on the server | `terminal.rs`, `ui/terminal_drawer.rs`, `ui/terminal_surface.rs` | 2,500 | Rethink | The owner's reasons, to ask with the rows (recommended: keep) |
| 8 | Files | A tree, search, a highlighted view, small edits, media previews | `ui/files.rs`, `ui/file_*.rs`, `ui/project_file.rs` | 2,700 | Rethink | The editor's light mode, then the editor of Stage 7 ([ADR-0019](adr/0019-a-code-editor-in-pluto.md)) |
| 9 | The browser | A preview panel, server-side tabs that agents drive, a Device surface | `preview.rs`, `ui/preview.rs`, `server_browser.rs`, `ui/server_browser.rs` | 3,200 | Rethink | One browser on the server, watched in a tab, which also shows computer use; optional where Chromium is installed |
| 10 | Git and worktrees | A quick action, commit, push, pull, branches, stash, worktrees' setup, publishing, Bitbucket | `ui/git_actions.rs`, `branch_picker.rs`, `source_control.rs`, `stash.rs`, `worktree_setup.rs`, `publish.rs`, `bitbucket.rs` | 6,200 | Keep | Worktrees' size shown and cleaned, checkpoints at each turn (ADR-0017); Bitbucket asked with the rows |
| 11 | Pull requests | A page of pull requests, those linked to each thread and their state | `ui/pull_requests.rs`, `linked_pull_requests.rs`, `thread_pull_requests.rs`, `crates/pluto-core/src/change_request.rs` | 2,700 | Rethink | A state of the thread and of the dashboard; followed to the merge (ADR-0018) |
| 12 | Search | Find in a thread, search among threads, search in their content | `ui/thread_find.rs`, `thread_search.rs`, `content_search.rs` | 1,800 | Rethink | One search, everywhere, every machine, the phone too |
| 13 | The palette and menus | Ctrl+K, Ctrl+P to a file, context menus, `pluto://` links | `ui/palette.rs`, `command_menu.rs`, `context_menu.rs`, `ui/prompt_links.rs` | 3,100 | Keep | Duplicates merged |
| 14 | Keybindings | Bindings resolved by the server, a when-clause builder, the shortcuts' page | `ui/keybindings.rs`, `when_builder.rs`, `shortcuts.rs` | 2,800 | Rethink | Remappable by zone, no when-clause builder; the editor's Vim and Helix modes shared |
| 15 | Agents' pages and MCP apps | A page an agent publishes, shown in the thread; MCP applications' frames | `ui/html_render.rs`, `mcp_apps.rs`, `crates/pluto-core/src/html_render.rs`, `mcp_app.rs` | 1,300 | Rethink | The owner's reasons, to ask with the rows (recommended: keep); for example shown by uwui's web view |
| 16 | Providers and models | Each provider's settings, instances, the model picker, custom models | `ui/providers.rs`, `providers/`, `pickers.rs`, `custom_model_editor.rs` | 6,700 | Rethink | One configuration for every agent (ADR-0018); one picker, local and remote equal, quotas beside |
| 17 | Local models | Ollama's driver in the fork; code through the OpenCode companion | the fork's `packages/provider-ollama` | none in the client | Rethink | Managed in Pluto, as ADR-0003; coded by Pluto's own agent ([ADR-0011](adr/0011-plutos-own-harness-minimal-and-measured.md)) |
| 18 | Usage and limits | Cost and token charts, limits, prices, the context meter | `ui/usage*.rs`, `context_meter.rs` | 4,400 | Rethink | Quotas in the dashboard, warned before they run out, a switch suggested; Pluto's own tokens shown (ADR-0017) |
| 19 | Settings | Pages, search, scope and inheritance, project overrides, defaults restored | `ui/settings*.rs`, `setting_inheritance.rs` | 7,200 | Rethink | Fewer, each useful, triaged row by row; inheritance by machine, project and thread kept |
| 20 | Themes and typography | T3's environment themes, the accent picker, fonts, Lucide icons | `theme.rs`, `ui/accent_picker.rs`, `typography.rs`, `lucide.rs` | 3,000 | Rethink | Pluto's own look of ADR-0003 and ADR-0016; T3's environment themes asked with the rows |
| 21 | Notifications | A thread finished or waiting, toasts, the composer's notices | `ui/thread_notifications.rs`, `toasts.rs`, `composer_notices.rs`, `crates/pluto-core/src/notifications.rs` | 2,100 | Keep | The triggers' notices added |
| 22 | Chat mode | The chat space, its projects, research, image chats, answers' versions, temporary chats, export | `ui/chat_*.rs`, `crates/pluto-core/src/spaces.rs` | 3,500 | Keep | Every model equal; it grows into the owner's only chat ([ADR-0020](adr/0020-the-chat.md)) |
| 23 | Memory and instructions | What Pluto remembers between conversations; the owner's instructions | `ui/memory_settings.rs`, the fork's memory tools | 700 | Keep | In the one configuration; its tokens counted |
| 24 | Sandboxed Python | `code_run`: Python in bubblewrap, for the chat | the fork | none in the client | Rethink | Under ADR-0009's one sandbox |
| 25 | Design projects | The design surface and its home, the server's design charter | `ui/design.rs`, `ui/design_home.rs` | 2,000 | Rethink | The owner's direction, to ask with the rows (the first answer was keep) |
| 26 | Scheduled tasks | An agent started at a time or at intervals, the list of tasks | `ui/scheduled_tasks.rs`, `ui/tasks.rs` | 1,500 | Keep | Event triggers added (ADR-0018) |
| 27 | Subagents and the agents panel | Subagents shown in their thread; an Agents panel that T3 v0.1.0 removed | `subagents.rs`, `ui/agents_panel.rs` | 1,300 | Rethink | Subagents under their parent, delegation alike; the panel replaced by the dashboard |
| 28 | Projects and scripts | The projects' page, a project's actions, scripts imported, the actions' editor, "open in" | `ui/projects_page.rs`, `project_*.rs`, `script_import.rs`, `action_editor.rs`, `open_in.rs` | 3,500 | Rethink | The owner's reasons, to ask with the rows (recommended: keep) |
| 29 | Rewind, undo and fork | Rewind a thread to a message, undo, fork, answers' versions | `ui/rewind.rs`, `undo.rs`, `chat_versions.rs` | 1,300 | Keep | Files rewound too, for every agent, by Pluto's checkpoints |
| 30 | Media | The lightbox, video, Codex's generated images, tools' images | `ui/media.rs`, `video.rs`, `lightbox.rs`, `tool_images.rs`, `crates/pluto-core/src/generated_images.rs` | 1,900 | Keep | Decoded off the interface's thread, in a bounded cache |

## The phone

| # | Family | What it does today | Where | Mark | Direction |
|---|---|---|---|---|---|
| 31 | The Android application | Connection, threads, a thread, the diff, settings, projects, tasks, usage, personalisation, notifications by a foreground service | `android/app/src/main/kotlin/app/pluto/mobile/`, `crates/pluto-ffi` | Rethink | The chat as readily as the dashboard, code not neglected: start, review, follow and unblock; notifications through UnifiedPush; offline, and models on the phone itself; files, a terminal, git; the desktop's marks apply family by family (ADR-0018) |

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
