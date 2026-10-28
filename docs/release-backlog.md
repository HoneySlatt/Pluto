# Backlog to the first release

The release is public and complete: every feature of T3 Code v0.1.0's client
is ported (the ACP registry stays out on purpose), the Android app gets its
background notifications, and the programs ship with their notices.

`docs/parity/` holds the source-based inventory this list comes from, one
report per area (chat, settings, pull requests, surfaces, shell), each row
with its upstream files, what is missing, where it goes in the port and its
size. About 330 gaps: some 35 large, 115 medium, 155 small. Each milestone
below ends with `scripts/check.sh`, the snapshot comparison on the fixture for
what it touched, and the README's parity sections brought up to date.

## 1. Bugs and wrong behavior

- Composer drafts are global: the text, images and files follow the user to
  the next thread, and nothing survives a restart (chat.md).
- A refused send clears the composer; the thread's `lastError` is never shown
  (chat.md).
- Any socket error drops back to the Connect screen: port the Android app's
  reconnection (attempts, the bar, the thread and streams followed again,
  the silence check of `pluto-core`).
- Response streaming still offers v0.0.45's `token`, which v0.1.0 rejects;
  five v0.1.0 keybinding defaults are missing (settings.md).
- About 20 bound commands do nothing (`thread.undo`, navigation back and
  forward, `view.reopenClosed`, `thread.copyReference`, `thread.stop`,
  `modelPicker.toggle`, `preview.*`, `terminal.*`, `composer.*`,
  `script.*.run`); four when-clause variables are faked (shell.md,
  surfaces.md, pull-requests.md).
- Settings saved but ignored: `timestampFormat`, Send shortcut, Rich text
  composer, Collapse composer on scroll, proactive panels, Working shelf,
  "Open preview automatically", the Browser rows (chat.md, shell.md,
  surfaces.md, pull-requests.md).
- Provider instances are saved by rewriting the whole map
  (`mutateProviderInstance` upstream): concurrent edits are lost; the status
  detail is computed and dropped (settings.md).
- Raw HTML in markdown prints as text; `.avif` opens the image preview and
  fails; open files stay stale after an agent edits them (chat.md,
  surfaces.md).
- Every PR host shows the GitHub icon; palette: Escape in Files/Content
  closes it, the first ArrowDown after typing skips a row; Project settings
  wrongly says "not implemented" (pull-requests.md, shell.md).
- The connect card says "T3 CODE (ALPHA)"; the Agents surface is a v0.0.45
  leftover v0.1.0 removed (settings.md, surfaces.md).
- Around 56 `self.status` toasts are untyped (no success, warning or error).
- README and module docs that claim or deny the wrong things (every report's
  last section).

## 2. The chat, the composer and the sidebar

- Edit from here with checkpoint rollback ("Revert files too").
- The composer banner stack: offline, server update, branch changed, child
  thread waiting, goal, background work Stop, project clone, usage limit,
  provider status.
- The rich prompt editor (formatting, code blocks, list continuation, chip
  undo), prompt stash, prompt length limit and large-paste folding, `@`
  thread mentions, prompt links, sending to several models, background send.
- Timeline: minimap, Scroll to end, worktree setup card, secret requests,
  the status line (Wake now, Un-settle), subagent threads, From queue / Steer
  pills, attribution, failed pills, the rest of the tool presentation names,
  markdown (autolinks, footnotes, GitHub alerts, Mermaid, thread and PR
  links, heading anchors, code block wrap and run), video.
- Default sidebar: drag and drop, multi-select, undo, draft rows, hover
  cards, indicators, settle-and-advance, durations and v0.1.0 status colors,
  Woke from the server.
- Root coordinators: keybindings reload, migration, slow request,
  permission notice, version mismatch, quit hold; toasts with copy, details,
  several actions, tied to their thread.

## 3. Pull requests

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

- Background notifications (a foreground service keeping the connection,
  with its setting), attachments (photos, camera, files), https servers,
  creating the chat space, chat projects' memories, unread and snoozed
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
- An install README for strangers; a final check of the history for
  personal data; the tag and the GitHub release, after the user's go.
