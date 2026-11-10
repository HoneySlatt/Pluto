# Specification

What Pluto's Rust server and its client on uwui must do, the phone included. It is written from
the owner's triage of today's features ([`features.md`](features.md),
[ADR-0010](adr/0010-todays-features-triaged.md), [ADR-0018](adr/0018-the-markets-features-and-the-triage.md),
[ADR-0022](adr/0022-the-triage-row-by-row.md)) and from the decisions of
[ADR-0003](adr/0003-what-pluto-is-to-become.md) and [ADR-0017](adr/0017-pluto-measured-against-the-market.md)
to [ADR-0020](adr/0020-the-chat.md). The owner accepted it in
[ADR-0023](adr/0023-the-specification-accepted.md).

How to read it:
- It says *what* each part does, not how it is built. The how is decided by the ADRs and by each
  stage's milestones.
- Numbers in brackets are rows of [`features.md`](features.md) (`1.27`) or families (`family 3`).
  Every row kept, rethought or added is named here; [section 7](#7-the-families-and-where-they-go)
  maps each family to its section.
- Each feature is **core**, always present, or **optional**, present only when the owner turns it
  on or installs what it needs (ADR-0003). Optional is said; core is the default.
- The stage that builds each part is the [ROADMAP](ROADMAP.md#4-stages)'s. The client's sections
  are built at Stage 6, the editor at Stage 7.
- The budgets each part must meet are in the [quality bar](ROADMAP.md#quality-bar).

## 1. What holds everywhere

- **Every agent equal.** Every first-rank agent has steering, interrupt, resume, checkpoints,
  approvals, the plan and its usage, native or supplied by Pluto. A grid of tests on recorded
  sessions says, per agent and ability, native, supplied or impossible (ADR-0017).
- **Nothing at rest.** No part runs, polls or draws while there is nothing to do. State is read when
  it is used or pushed by events: providers' status when the picker opens or a session starts
  (16.11), a pull request's state on events, its forge polled only while it is followed (11.12),
  generated images on events (31.32), the dashboard's quotas and machines on events, their last
  values kept at rest (1.28).
- **Instant.** No common action shows a loading state; a thread visited before shows whole in the
  frame after the click (ADR-0017).
- **Nothing is lost.** Drafts, pending review comments, threads, worktrees with uncommitted work and
  history survive a crash, a cut, an update or a migration. Every format is versioned and
  migrated; writes are atomic; a migration backs up first ([ADR-0008](adr/0008-storage-and-the-history-moved.md)).
- **Private.** Pluto needs no account and no cloud. Only the agents' own traffic leaves, and Pluto
  shows which (ADR-0017), with one exception: images and link icons in answers load by themselves
  when they are safe, from the hosts of the answer's cited sources or tool results, without a query
  string, never to a loopback or private address, under the thread's sandbox network rule; any
  other loads on a click; never through a third party (2.5, ADR-0022). Pluto downloads nothing of
  its own accord: models, catalogs and agents' programs only on the owner's request. It never reads
  an agent's stored credentials ([ADR-0009](adr/0009-secrets-privacy-and-the-sandbox.md)).
- **Keyboard first**, the mouse complete; WCAG 2.2 AA with a screen reader; reduced motion and
  transparency followed.
- **Tokens.** Everything Pluto adds to an agent (tools, instructions, memory) is measured and can be
  turned off per thread; a turn costs at most 5 % more tokens than the agent alone (ADR-0017).
- **Configuration** lives in the interface and in readable TOML files, per machine and per project,
  which the owner's NixOS can write; a value is inherited from default to machine, project and
  thread, and the layers icon says where it comes from (19.3 to 19.7); the server's settings are
  checked, saved and announced to every client (19.26). A value a read-only file fixes, as NixOS
  writes them, shows locked, with its file named.

## 2. The server

One Rust server per machine that codes ([ADR-0004](adr/0004-plutos-own-server-in-rust.md)). It is a
service: closing a window stops nothing, and an update or a restart never stops a running agent.

### 2.1 Threads, storage and history

- Projects, threads, turns, messages and their attachments in SQLite, versioned, within 25 MiB a
  day of heavy use (ADR-0008, ADR-0017).
- A thread's states: pinned, snoozed until a time, settled, archived; auto-settle after its pull
  request merges or after N days, set per project (1.6 to 1.9, 1.26).
- A readable export of any thread; the fork's history imported and verified (Stage 3); the owner's
  exports from ChatGPT, Claude, Gemini and others imported into one history (ADR-0020).
- A full-text index of messages, answers, archived threads and imported history on each server and,
  optionally, an index by meaning built with a local embedding model the owner downloads, both
  queried by the one search (12.2).
  Tool outputs and thinking are not indexed.
- Rewind, undo and fork (family 29): a thread rewound to a message rewinds its files too, through
  Pluto's checkpoints, for every agent; a fork from any message, to the same agent or another.

### 2.2 Agents

- Drivers ([ADR-0006](adr/0006-agents-drivers.md)): Claude Code, Codex, OpenCode, Kimi Code,
  Mistral Vibe, Z Code, Grok, Antigravity, Pi, Muse, DeepSeek Harness, any agent of the ACP
  registry, Ollama, and Pluto's own agent (Stage 4) (16.19 to 16.21, 16.23 to 16.32). Cursor is not driven.
- Several instances of one agent, each with its name, colour, home and account; a thread may move
  to a compatible account or, with its context handed over, to another agent (16.3, 16.18, 16.33).
- Each driver describes its own settings, and the client draws them (16.1, 16.5, 17.10).
- **The one configuration**: MCP servers, skills and instructions declared once, per machine,
  project or thread, given to each agent in its own format when a session starts, without touching
  the agent's own files; what an agent cannot receive is shown as absent (16.38, ADR-0018). The
  design charter is part of it (25.11, 25.12).
- Instructions Pluto adds to an agent, measured against the tokens' budget (16.39).
- Titles, commit messages, pull request texts and branch names generated by a model the owner
  chooses (16.37, 19.13).
- Agents' programs: Pluto says which are missing, or older or newer than the versions its drivers
  were tested with (ADR-0017 pins them), opens their official sign-in in a
  terminal, and proposes the command to install or update them; it downloads nothing itself
  (16.34, 16.35). "Sign in with ChatGPT" where OpenAI allows it (ADR-0020).
- Status, version and models probed when used (16.36, 16.11).
- Runs in the vendors' clouds: optional, off by default, turned on per provider (ADR-0018).
- Sessions kept ready for the chat: how many and how long is decided at Stage 2, measured (ADR-0020).

### 2.3 Local models

Core; Ollama is installed by the owner (family 17).
- Ollama for chat and code, code through Pluto's own agent from Stage 4 (17.1 to 17.5, 17.7, 17.9).
- Any OpenAI-compatible endpoint (llama.cpp, LM Studio, vLLM) for chat and Pluto's agent, without
  model management (17.16).
- Managed from Pluto: installed models with their size, whether loaded, their GPU memory, unload
  and delete; a model pulled by name with its progress; optionally a catalog of Ollama's library
  saying whether a model fits the GPU, fetched on request (17.13 to 17.15).
- A context length set per model and sent with each request, proposed from the GPU's memory; no
  derived models (17.8).
- Web search for local models and Pluto's agent through a keyed engine (Brave, Kagi, Exa), SearXNG
  as an option; pages fetched never from loopback or private addresses (17.6, ADR-0020).
- Their tokens in the usage history (17.12, 18.12).

### 2.4 Git, worktrees, review and pull requests

- A worktree per thread, set up by the project; commit, push and pull; branches; stash; publishing
  (family 10). Each worktree's size is known; those of finished or merged threads may be deleted,
  never one with uncommitted work, and its branch and commits are kept; a new worktree costs only
  the files git tracks (ADR-0017, 19.22).
- A checkpoint at each turn, for every agent (ADR-0017).
- Diffs for four scopes: the thread since its starting point, uncommitted, the latest turn, a chosen
  turn; two turns compared through checkpoints; another base on request (6.1, 6.2). Patches
  parsed with word emphasis, large ones per file (6.8, 6.11).
- A hunk staged or reverted alone (6.16).
- Review comments kept like a draft and sent in one batch to the thread's agent or another
  provider's; a cross-agent review returns its remarks as line comments (6.9, 6.17).
- Forges: GitHub, GitLab and Forgejo (Codeberg) (11.17). Pull requests linked to threads, with
  their state, checks and reviewers' remarks; linked by number, URL or checkout command (11.9,
  11.11, 11.13).
- A pull request followed to its merge: on a failed CI or a remark, the owner is offered to let the
  agent fix it; the agent works in the locked sandbox and waits for the owner's go before each push
  (11.18, ADR-0018).

### 2.5 Terminals and files

- Real terminals per thread, kept across the client's restarts (family 7).
- A project's files listed, searched, read and lightly edited; an edit never overwrites an agent's
  concurrent write (8.1 to 8.12). From Stage 7, the editor's core runs here: open files merged
  live, language servers, search ([ADR-0019](adr/0019-a-code-editor-in-pluto.md)).
- Projects: one project is one repository, on every machine where it is checked out (28.10); added
  from a folder, a URL cloned, or a new empty repository, on a chosen machine (28.12); a project's
  own TOML file holds its settings and its own actions; a `t3.json` found is offered for import once
  (8.13).
- A project's actions found in its files (package.json, Justfile, Makefile, Cargo, the flake's
  apps), run in the thread's terminal (28.5).

### 2.6 The browser and computer use

Optional: present where Chromium is installed (family 9).
- One headless browser on the server; tabs per thread, restored after a restart when they are
  next shown, frozen while no one watches them and no agent drives them, streamed to every
  client watching, with control taken or given, dialogs, uploads and downloads, and the agent's
  pointer shown (9.4 to 9.6, 9.16).
- A browser profile kept per project, its logins kept, stored encrypted under the keyring; reaching
  it is a right of the sandbox, closed when the sandbox is locked (9.15).
- Agents' tools: open, navigate, resize, appearance, snapshot of the page's elements, click, type,
  hover, select, drag, scroll, evaluate, dialogs, recordings, list and close (9.9 to 9.14).
- Computer use in an isolated session, a headless compositor or a virtual machine holding only the
  applications opened for it, emulators included; the owner's own desktop only with the owner's
  leave for one task, never an unattended one, with an emergency stop (9.17, ADR-0018).

### 2.7 Pluto's MCP tools for agents

Offered to every agent through the one configuration, each right decided by the sandbox:
- `html_render` and, where Chromium is installed, `html_preview` (15.9 to 15.11);
- the browser's tools (section 2.6);
- delegation: `delegate_task`, a child's status and cancellation, stopping a workflow, the
  providers' capabilities and windows; a child gets a narrower credential (27.8 to 27.12);
- pull requests: link, unlink, list, watch, unwatch (11.15);
- memory, earlier chats, `code_run`, scheduled tasks, a secret asked of the owner (families 22 to
  26).

### 2.8 Scheduled tasks and triggers

An agent started at a time, at intervals, or on the owner's own events: a CI that fails, a comment
on a pull request, a webhook. Triggered runs use the locked sandbox. Events come through the
owner's relay, or from the forge (GitHub, GitLab, Forgejo) polled only while a pull request is
followed. The chat has its own
tasks, such as a morning summary (family 26, ADR-0018, ADR-0020).

### 2.9 Usage and quotas

- Each driver's usage history, cached, priced at list prices with the owner's overrides; local
  models and ACP agents included (18.11 to 18.14).
- Each account's quota windows, read through each agent's own interface, never its stored
  credentials (18.16).
- A warning by pace ("at this pace Claude runs out in 40 minutes, before its reset") and at 95 %,
  with a suggested switch to another agent, never made alone (18.20).
- A run stopped by a limit resumed at its reset or continued with another agent, as the owner
  chooses (18.9, 18.18).
- A delegated task's tokens and estimated cost, summed into its parent's (27.13); windows and costs
  over MCP (18.19).

### 2.10 The chat's services

Families 22 to 24 and [ADR-0020](adr/0020-the-chat.md):
- each vendor reached by the path its terms allow; the table of these paths kept up to date;
- one memory of the owner, visible, editable, exported, and one per project; code agents receive it
  only on request; additions shown and undone in one gesture (family 23);
- files read (PDF, Office, spreadsheets, images, code); optionally, a large library indexed locally
  with a local embedding model, only the useful passages sent, cited;
- web search with sources always cited; deep research with every model;
- voice recognised and spoken locally, optional, since it needs speech models downloaded; a vendor's
  real-time voice through a key, optional;
- images generated by vendors' permitted paths and, optionally, by a local model; a part of an image
  selected and changed, versions kept;
- `code_run`: Python for a chat, under the one sandbox, without network unless the owner opens it
  for a chat, reading only the chat's files, a working folder per chat, its charts and files
  returned (24.1 to 24.6). Optional: it needs bubblewrap and Python;
- locked chats, hidden from the list.

### 2.11 Machines, devices and notifications

- Several servers shown as one; devices paired with their own revocable key and rights (read-only,
  no push, no approval), revoked at once ([ADR-0007](adr/0007-several-servers-and-a-relay.md),
  ADR-0018).
- A device reaches a machine by the fastest path that answers: the local network, iroh directly,
  the owner's relay (optional, a binary of its own), Tailscale where installed.
- Notifications of a thread finished, failing or waiting, of the triggers, through UnifiedPush to
  the phone, their content encrypted end to end (family 21, ADR-0018).
- Logs and crash reports kept on the machine, exported on request (19.14).

### 2.12 Secrets, sandbox and privacy

[ADR-0009](adr/0009-secrets-privacy-and-the-sandbox.md): secrets entered by the owner live in the
system's keyring (16.6); agents run in one sandbox adjusted per project or thread and locked for
unattended runs; reaching the browser, computer use and uwui's applications are rights of that
sandbox (19.20); secrets agents meet are masked on screen, in exports and notifications.
Actions kept for a person are never offered to agents, whatever consent they hold: approving a
request, changing a sandbox, revealing a masked secret, pairing a device
([ADR-0016](adr/0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)).

## 3. The protocol

Pluto's own typed, versioned protocol, described once in a shared crate
([ADR-0005](adr/0005-plutos-own-typed-protocol.md)). Events are pushed; a dropped connection
recovers within a second on the local network, losing nothing; the client never polls.

## 4. The desktop client

Pluto's own look on uwui (ADR-0003, ADR-0016); built at Stage 6.

### 4.1 The dashboard

The home screen (family 1, ADR-0022):
- a grid of thread cards, the threads that work first, then the latest; filtered by project or
  machine, the filter kept (1.0, 1.2, 1.25);
- a card shows the thread's title and project, its state and its live line (what it does, for how long), its agent, model
  and machine (1.3, 1.5); a draft carries a mark (1.12); a parent's children fold under it, a
  delegation to another provider alike (27.14);
- on a card: approve, answer, stop, send a message, pin, settle, snooze, from the keyboard too
  (1.9, 1.27);
- sections for pinned, live, snoozed and settled threads, their folding kept (1.1);
- multi-select and its menu, a sweep along the list, a thread dragged between sections or onto a
  composer, files dropped on a thread, the undo notice (1.13 to 1.16, 1.18);
- a hover card with branch, worktree, terminals, pull requests, model, earlier agents and error
  (1.4, 1.17);
- the thread's one menu: rename in place, pin, settle, snooze, regenerate the title, mark unread,
  copy, archive, delete, each confirmation as set (1.10, 1.11);
- the snooze's presets and dialog (1.7, 1.8);
- a strip at the top, updated on events and still at rest: each quota as a gauge, each machine's
  load and memory, the next scheduled
  tasks; a click opens their page (1.28);
- opening a thread puts it over the dashboard, with a compact list of threads beside it; a key
  returns (1.19, 1.20).

### 4.2 A thread

**The timeline** (family 2):
- answers as pages, full width; the owner's message in a light block above (2.8, 2.9);
- a finished turn begins with its summary: duration, files changed (unfolding to their list, a file
  opening the review there), commands run, result; then the answer; tools and thinking folded
  (2.10, 6.10);
- a running turn streams its text and shows its actions on one live line, with "working for…"
  (2.14, 2.15); thinking live in grey, then folded to "thought 40 s";
- tools grouped by step, runs of one kind merged, edits and commands one per line (2.12); an opened
  command as in a terminal: exit code, duration, coloured output, folded, run again (2.13);
- Markdown with tables, images, Mermaid, math, a safe subset of HTML (2.1, 2.3, 2.4); code blocks
  with highlighting, language, copy, line numbers, folding, wrap, open in an editor or saved, run in
  the terminal after confirmation (2.2);
- a cited path opens at its line, in Pluto or the owner's editor (2.5);
- a message copied as text or Markdown; edited and resent; forked from, to the same agent or another;
  a passage quoted into the composer or a side question (2.6, 2.7);
- an outline of the thread (messages, turns' summaries, questions, approvals) and keys to the
  previous and next turn; "scroll to end" (2.17, 2.18);
- dividers for interruptions, handoffs, forks, created threads, superseded attempts and compaction;
  the status line under the last message (2.11, 2.19, 1.24);
- a thread of 5,000 messages within the frame budget; a thread opens whole (2.16);
- subagents as cards that open their thread, neighbours folded; "subagent of" at the head; a
  lineage card with parent, forks, children, transfers and a workflow's phases, merging a fork and
  stopping a child (27.1 to 27.6);
- media: the lightbox, video, generated and tools' images, decoded off the interface's thread in a
  bounded cache (family 30).

**The composer** (family 3): the prompt's editor, mentions, a draft per thread, prompt history,
context chips, the queue and steering; local dictation, optional (ADR-0018); side questions that neither turn
the agent from its work nor fill its context.

**Requests and plans** (families 4, 5): approvals, questions and secrets entered privately; the
proposed plan refined, implemented, saved, or implemented in a new thread; both supplied to every
agent (ADR-0017).

**Rewind** (family 29): a thread rewound to a message, files included; undo; answers' versions.

**Notifications** (family 21): a thread finished or waiting, toasts, the composer's notices, the
triggers' notices; a tray icon while the window is closed.

### 4.3 The panel and its surfaces

A panel beside the thread, its tabs and their menu, a closed tab reopened, its state and width kept
per thread, maximized, a sheet in a narrow window; the review opened after a turn
when the setting says so (0.1 to 0.7).

- **Review** (family 6): unified or split; a tree of the changed files, each marked viewed until it
  changes; find in the diff; unchanged context expanded; whitespace, wrap, collapse; a hunk kept or
  rejected; comments on lines kept like a draft and sent in one batch; "Have it reviewed by…"
  another provider's agent (6.1 to 6.17). A full-window review mode beside the panel (6.0).
- **Files** (family 8): the project's tree by mouse and keyboard, search, a file dragged into the
  composer, a clickable breadcrumb; highlighted source; small edits until the editor (Stage 7);
  images, SVG and AVIF, Markdown rendered with its task boxes, CSV and TSV tables, video, HTML and PDF
  in place; a line commented for the agent; "open in" the owner's editor (8.1 to 8.12, 8.14).
- **Terminal** (family 7), as today: a drawer and tabs, splits and groups, links, selection added to
  the chat.
- **Browser** (family 9), optional: the server's tabs, back, forward, reload, address, DevTools, the
  local dev servers to open, control taken or given, an element commented for the agent, recordings
  watched; computer use's session watched in the same way (9.1, 9.2, 9.4 to 9.6, 9.9 to 9.18).
- **Artifacts** (families 15 and 25): agents' pages and MCP apps open whole here, in uwui's isolated
  web view, without network or files by default; a preview stays in the thread. Versions, replayed,
  exported, changed in the conversation; open in the system browser (0.11, 15.1 to 15.8, 15.12). An
  MCP app's tool calls that are not read-only, and its messages, ask first (15.5).

### 4.4 Pull requests

Family 11:
- a strip in the thread with the state, the checks and the reviewers' remarks, each forwarded to the
  agent, its files in the review (11.8);
- a page of the pull requests of Pluto's threads (open, in review, CI red), from the dashboard,
  with its hosts, refresh and states (11.1, 11.4 to 11.7);
- linked from the palette or a dialog; "#" in the composer (11.9, 11.10, 11.14);
- the offer to let an agent fix a failed CI or answer a remark (11.18).

### 4.5 Search, the palette and keys

- **One search**, in the palette: threads, messages, files, code, commands and settings, on every
  machine, by section, by words and, optionally, by meaning; find in the open thread or file (12.0 to 12.2, 12.4).
- **The palette and menus** (family 13): Ctrl+K, Ctrl+P to a file, context menus, `pluto://` links,
  their duplicates merged.
- **Keys** (family 14): every command bound per zone (thread, composer, review, terminal…),
  remapped on a page with search, recording, conflicts and defaults, and in a readable TOML file
  watched for changes; a command's key shown beside it; optionally, a modal navigation (j/k, gg, /)
  on the editor's modal core, with a leader key, built with that core at Stage 7 (14.1, 14.3 to
  14.11, 14.13, 14.14, 14.16 to 14.18).

### 4.6 Agents, models and usage

- **The picker** (family 16): one searchable list, favourites first, then by agent, local models
  equal, each agent's quota and state beside it, by keyboard (16.13, 17.11); the model's options and
  runtime modes (16.15, 16.16); one prompt to several models, their threads side by side (16.14).
- **An agent's page**: its instances, status and why it is not ready, version, accent colour,
  environment variables, models (favourites, hidden, order, filter), custom models as an id and a
  name, an instance added (16.1 to 16.9, 16.12, 16.17, 20.8).
- **Local models**: the installed list, pull, delete, unload, the catalog (17.13 to 17.15).
- **Usage** (family 18): cost at list prices, tokens per model, quota windows per account with their
  pace and reset, prices edited; periods and keys; the context meter and "compact"; a limit's banner
  offering to resume or switch (18.1 to 18.10).

### 4.7 Settings and appearance

- **Settings** (family 19): a few pages by subject (General, Agents, Appearance, Keybindings,
  Machines and devices, Privacy, Storage), their search, the layers icon, restoring defaults with a
  list, a confirmation and a copy to undo; new threads' defaults, confirmations, start from origin,
  the composer collapsed on scroll, rich text, the send key, proactive panels; Diagnostics and licences;
  Archive; personalisation; scheduled tasks (19.1 to 19.24).
- **Appearance** (family 20): Pluto's theme, dark and light, following the system, and high
  contrast; an accent colour, a density, the diffs' colours; two font families (interface, code)
  and their sizes, Geist by default, loaded once; Pluto's code theme and an imported one for the
  editor; the Lucide catalog (20.4 to 20.9, 20.11 to 20.13).
- **The capture** of a window or an area, through Wayland's portal, at the owner's request
  (19.17, ADR-0020).

### 4.8 Projects and design

- **A project's page** (family 28): its threads, worktrees and their size, pull requests, settings
  and overrides, MCP servers, skills and instructions, actions; its icon, a coloured monogram or one
  chosen from a small grid; the new threads' defaults; "open in" (28.1 to 28.5, 28.9, 19.4, 19.25).
- **Design projects** (family 25), optional, a kind of project, added with a brief sent as the
  first message: the artifacts panel as the studio beside a narrow chat; the project's HTML files as
  pages, reloaded; following the newest page or one pinned; full, tablet and phone widths; versions
  compared side by side; several proposals from one or several models; an element inspected or
  commented, comments batched; `DESIGN.md` and OpenDesign's catalog on request; a design session
  from a thread; a validated mockup handed to a code thread (25.1 to 25.16).

### 4.9 The chat

Family 22 and [ADR-0020](adr/0020-the-chat.md): the chat space and its projects, every model equal;
research, images, answers' versions, temporary chats, exports (PDF, Word, Markdown, a web page); a
document written together, the model's edits shown as diffs; living artifacts, small applications among them (section 4.3); sources cited;
a global shortcut opening a small chat window; text selected anywhere translated, explained or
rephrased; one question to several models side by side; voice; locked chats; the chat's tasks and
connectors.

### 4.10 The editor

Optional, at Stage 7, as [ADR-0019](adr/0019-a-code-editor-in-pluto.md) decides: on uwui's
code-editor component, its core in the server, Vim and Helix engines held to the real programs, the
owner's Neovim imported, a live merge with the agents' edits, language servers, huge files, a
terminal mode.

## 5. The phone

Optional, an application of its own; in Kotlin until uwui runs Android applications
([ADR-0002](adr/0002-the-android-app-until-uwui.md), family 31, ADR-0018):
- two tabs, **Chat** and **Threads** (the dashboard: what needs the owner first, then the live
  threads), reopening on the last (31.6, 31.7, 31.11); the one search (31.12); two panes on tablets
  and foldables;
- paired by a QR code shown on the desktop, with its own key and rights; an address typed by hand as
  a fallback; every machine, by the fastest path; the local network's permission asked (31.1, 31.2,
  31.4);
- offline: threads and chats opened in the last 30 days kept, encrypted, under a cap the owner sets
  (500 MB by default), older ones
  fetched on demand; messages and tasks written offline leave when the network returns (31.3);
- notifications through UnifiedPush; live notifications with answers, approvals and dictation; a
  widget (31.5);
- the thread under the desktop's rules, laid out for the phone; pages and artifacts as previews
  opened full screen; the desktop's Markdown (31.14, 31.27);
- the composer with attachments and dictation; the image viewer; a thread's actions, snooze
  included; approvals, sensitive ones behind the fingerprint; questions; a subagent's request
  (31.8 to 31.10, 31.13, 31.15 to 31.17);
- review and comments on diffs; the one picker; project, workspace and branch sheets (31.18 to
  31.20);
- scheduled tasks and their editor with event triggers; usage; personalisation; projects (31.21 to
  31.23, 31.25, 31.26);
- settings with the device's lock and rights; licences; Pluto's theme, the planet; badges and marks
  (31.24, 31.28 to 31.31);
- files browsed, searched, read and corrected; a terminal with a bar of keys; git and pull requests
  behind the fingerprint; local models on the phone, offline (ADR-0018);
- a demo session in debug builds only (31.33).

## 6. What leaves

Cut by the triage, from the egui client and the Android app in [M1.4](PLAN.md#m14--the-cuts) where
they are there, and never built again (ADR-0022): 0.8 to 0.10, 1.21 to 1.23, 8.13, 9.3 (in the uwui
client; the egui client keeps it while it has no web view), 9.7, 9.8, 11.2, 11.3, 11.16, 12.3,
14.2, 14.12, 14.15, 16.10, 16.11, 16.22, 18.15, 18.17, 19.27, 20.1 to 20.3, 20.10, 27.7, 28.6 to 28.8,
28.11; Bitbucket and Azure DevOps (family 10); project grouping, the working
shelf, provider update checks and the fold of legacy features (19.9, 19.10, 19.14); Google's
favicon service.

## 7. The families and where they go

| # | Family | Mark | Here | Stages |
|---|---|---|---|---|
| 1 | Threads and the sidebar | Rethink | 2.1, 4.1 | 2; 6 |
| 2 | The timeline and its Markdown | Rethink | 4.2 | 6 |
| 3 | The composer and attachments | Keep | 4.2 | 2; 6; dictation from 4 |
| 4 | Agents' requests | Keep | 4.2, 1 | 2; 6 |
| 5 | Plans | Keep | 4.2, 1 | 2 to 3; 6 |
| 6 | The diff and review | Rethink | 2.4, 4.3 | 2; 6 |
| 7 | The terminal | Keep | 2.5, 4.3 | 2; 6 |
| 8 | Files | Rethink | 2.5, 4.3 | 2; 6; editor 7 |
| 9 | The browser | Rethink | 2.6, 4.3 | 3; 6 |
| 10 | Git and worktrees | Keep | 2.4 | 2; 6 |
| 11 | Pull requests | Rethink | 2.4, 2.8, 4.4 | 3; 5 |
| 12 | Search | Rethink | 2.1, 4.5 | 2 to 3; 6 |
| 13 | The palette and menus | Keep | 4.5 | 6 |
| 14 | Keybindings | Rethink | 4.5 | 6; modal navigation 7 |
| 15 | Agents' pages and MCP apps | Rethink | 2.7, 4.3 | 3; 6 |
| 16 | Providers and models | Rethink | 2.2, 4.6 | 2 to 3; 6 |
| 17 | Local models | Rethink | 2.3, 4.6 | 3; 4; 6 |
| 18 | Usage and limits | Rethink | 2.9, 4.6 | 3; 5; 6 |
| 19 | Settings | Rethink | 1, 4.7 | 2; 6 |
| 20 | Themes and typography | Rethink | 4.7 | 6 |
| 21 | Notifications | Keep | 2.11, 4.2 | 2; 5; 6 |
| 22 | Chat mode | Keep | 2.10, 4.9 | 3; 6 |
| 23 | Memory and instructions | Keep | 2.2, 2.10 | 3; 6 |
| 24 | Sandboxed Python | Rethink | 2.10 | 3; 6 |
| 25 | Design projects | Rethink | 2.2, 4.8 | 3; 6 |
| 26 | Scheduled tasks | Keep | 2.8 | 3; triggers 5; 6 |
| 27 | Subagents and the agents panel | Rethink | 2.7, 2.9, 4.1, 4.2 | 3; 5; 6 |
| 28 | Projects and scripts | Rethink | 2.5, 4.8 | 2; 6 |
| 29 | Rewind, undo and fork | Keep | 2.1, 4.2 | 2 to 3; 6 |
| 30 | Media | Keep | 4.2 | 6 |
| 31 | The Android application | Rethink | 5 | 2 (protocol); 5; 6 |
| 0 | The right panel | — | 4.3 | 6 |
