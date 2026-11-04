# Backlog to the first release

The release is private (the repository stays private) and complete: every feature of T3 Code v0.1.0's client
is ported (the ACP registry stays out on purpose), the Android app gets its
background notifications, and the programs ship with their notices.

`docs/parity/` holds the source-based inventory this list comes from, one
report per area (chat, settings, pull requests, surfaces, shell), each row
with its upstream files, what is missing, where it goes in the port and its
size. About 330 gaps: some 35 large, 115 medium, 155 small. Each milestone
below ends with `scripts/check.sh`, the snapshot comparison on the fixture for
what it touched, and the README's parity sections brought up to date.

## 1. Bugs and wrong behavior

Done: drafts per thread, saved for the next run (`drafts.rs`); refused or
lost messages back in their composer; automatic reconnection; the thread's
error banner; the streaming modes and default keybindings of v0.1.0;
navigation back and forward, copy reference, the model picker, the
composer's controls, the drawer's terminal keys and project action
shortcuts; the Send shortcut, Time format and "Open preview automatically"
settings; provider instance mutations; the palette's keys and Project
settings; each PR host's mark; typed toasts; the connection card's name;
the README's gap lists replaced by `docs/parity/`. Moved to later
milestones: raw HTML in answers (2, upstream parses sanitized HTML there),
`.avif` and stale open files (4), `thread.undo` and `view.reopenClosed` (2),
the preview keys (4), the composer settings tied to the rich editor (2).

Still open:

- Settings saved but ignored: Collapse composer on scroll, proactive panels,
  Working shelf, the Browser rows (chat.md, shell.md, surfaces.md,
  pull-requests.md).
- Bound commands without a handler beyond those moved above, and the
  when-clause variables still faked (shell.md, surfaces.md).
- The Agents surface is a v0.0.45 leftover v0.1.0 removed (surfaces.md).

## 2. The chat, the composer and the sidebar

Done: the composer banner stack (offline, server update against the pinned
0.1.0, branch changed, child thread waiting, goal, background work, project
clone, usage limit, provider status); prompt stash, length limit, large
pastes, the typing guard, `@` thread mentions, background send; the send
button's disabled states and the Compact chip; questions' 200 ms advance and
Stop; the timeline minimap, Scroll to end, secret requests, the status line,
subagent threads, intent and failed pills, attribution, timestamps; the T3
tool table, icons, group summaries (`tool_presentation.rs`), thread-read
labels, created-thread cards, subagent notification cards; markdown
autolinks with e-mail, footnotes with back-references, alerts, thread links,
heading anchors, code block wrap, run and titles, table cell expansion; the
default sidebar's drag and drop (sections, sweep, auto-scroll, onto the
composer), multi-select, undo, draft rows, hover cards, indicators,
settle-and-advance, durations, colors, Woke; root coordinators and toasts.

Also done: worktree setup card, prompt links on `pluto://`, sending to
several models, video through ffmpeg, Mermaid (merman), the T3 tool table
with image icons, created-thread and subagent cards, checkpoint rollback,
the editor's Markdown rules (fences, lists, Tab, chip undo).

Still open (polish, after the later milestones if time allows):

- Sidebar list motion (FLIP glides and fades).
- The editor's styled rendering and the code block language picker.
- Worktree setup: Work locally and the background chip.
- Fan-out: stacked provider icons, Allow retry, failed-model restore.
- Video: fullscreen and the media menu.
- PR links in answers (with milestone 3), cloning from a prompt link.

## 3. Pull requests

Set aside on 2026-11-04 (the user does not use these): only the thread
links were ported (Linked pull requests surface, Link dialog, palette
entries). The items below stay unported unless asked for.

- The detail panel (summary, timeline, code with review threads, guide,
  actions, edits, labels, reviewers, reactions, viewed files) and the right
  panel: about 20 `pullRequests.*` methods.
- Host search with qualifiers and ranking, involvement on the hosts, paging,
  `listStats`, live refresh, `invalidate`, author and label filters, saved
  filters, stacks, row menu, checks popover, avatars.
- Thread PRs: link, unlink, watch, the palette entries, checkout
  (`git.resolvePullRequest`, `git.preparePullRequestThread`), the details
  card's PR row and git panel, link previews, the "#" exact lookup.
- Git progress in the button and `vcs.refreshStatus`.

## 4. Surfaces

- The browser through the server (`preview.*`, `subscribePreviewEvents`,
  `/api/preview-stream`), so agents and the user share tabs; the mini player.
- Local browser: dialogs, file inputs, downloads, popups, the More menu,
  zoom, device toolbar, annotations, screenshots and recording, profiles,
  the error page.
- File editing and saving, markdown checkboxes, review comments on diff and
  file lines, find in the diff.
- Media in place: HTML, PDF, video, audio, `.avif`.
- Diff panel: v0.1.0's scopes, open a file from it, its menu, per-file
  loading and truncation, refresh after workspace changes.
- Terminal: an emulator with mouse reporting, wide characters, OSC 8, Kitty
  keyboard, DA/DSR (`alacritty_terminal`), file links, Select all, Jump to
  latest, close confirmation, persisted groups.
- Panels: state per thread, tab menu, drag reorder, overflow, reopen closed,
  the narrow-window sheet, proactive panels.
- The device surface (H.264 frames).

## 5. Settings, accounts and environments

- Provider sign-in with its terminal, managed runtimes (Codex, Antigravity),
  ChatGPT accounts (a local callback listener replaces Electron's), updates,
  the Muse and Pi drivers' rows, the add-provider wizard's tiles.
- Connections: pairing links with QR, client sessions, environment version
  and self-update; one-time pairing tokens and the connect-agent approval
  screen in place of the pasted bearer token.
- Several environments: saved remotes, SSH, routing, the environment
  selectors, icons and badges, read-only sessions.
- Onboarding, with the Claude Code and Codex history import.
- Diagnostics; Open source licenses (with the desktop's crate notices).
- Themes: library, light and system modes, editor, import, contrast, glass,
  motion; SnapShots capture through the screenshot portal.
- The 15 or so new rows of General, Storage, Source Control and
  Integrations; Usage and Providers details (model dialog, share bars, reset
  credits, hubs, filters).
- Electron-only features mapped to Linux (settings.md's table): updates,
  deep links (a `.desktop` URL handler), open folder, SSH password prompt,
  browser profile import, system color scheme, CLI install.

## 6. The Android app

- Done: background notifications (a foreground service keeping the
  connection, with its setting), attachments (photos, camera, files).
- https servers, creating the chat space, chat projects' memories, unread and snoozed
  threads, the request text in Needs you, code block copy and colors,
  published pages, MCP apps and images, a list window for very long threads.

## 7. Performance

- Desktop: startup, idle CPU, a 1 000-message thread (scroll, streaming),
  memory over a long session, frame pacing, binary size; the bench and the
  eu-stack sampling recipe, a before/after table kept in the repository.
- Android: frame rate, latency and battery measured on the phone.

## 8. Release

- Version 0.1.0 everywhere, a CHANGELOG, the Android `versionCode`.
- A Nix package for the desktop (`packages.default`, `.desktop` entry,
  icon) and the crate notices shipped with it (cargo-about).
- The APK signed with a release key the user enters at signing time.
- The pinned pluto-server version and a clear message on a protocol
  mismatch.
- The repository stays private: no public install README or history
  scrub; the tag and a private release, after the user's go.
