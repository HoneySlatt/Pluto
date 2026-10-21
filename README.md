<img src="assets/icon.svg" width="64" height="64" alt="">

# T3-Code Rust

A native [egui](https://github.com/emilk/egui) client for the
[T3 Code](https://t3.codes) server, reproducing the interface of the upstream
web/Electron client (v0.0.45). The server stays upstream's: this client speaks
its WebSocket protocol (Effect RPC envelopes, orchestration protocol 1).

## Running

```sh
# NixOS: provides the Vulkan/Wayland/X11 libraries eframe loads at runtime.
nix-shell --run 'cargo run --release -- --server http://127.0.0.1:3773'
```

The connection screen asks for the server address and an access token. Issue a
token on the server machine with `t3 auth session issue`. `--token-file PATH`
reads it from a file at startup instead; it is kept in memory only.

## What is ported

| Area | State |
|---|---|
| Theme | Dark theme tokens, Geist fonts, Lucide 0.564 icons, pierre file-type icons; follows the environment theme (`t3 theme set`, published as `environmentThemesUpdated`), with `pierre-light` code under light themes |
| Sidebar | Wordmark, search filter, project scope filter, thread cards (project monogram, Approval/Input/Working/Failed/Done/Woke status, time, branch, provider glyph), active/pinned ordering, snoozed and settled shelves, settle/snooze/wake/un-settle actions, inline rename |
| Thread menu | The full context menu with Snooze, Auto-settle and Copy submenus, mark unread, filter by project, archive, delete with confirmation; the custom snooze dialog (date with the month calendar and segmented time, or a duration) from the menu and the card snooze button |
| Command palette | Ctrl+K actions and recent threads, project and thread search, new thread in project, add project with folder browsing, "Go to file" (Ctrl+P: `projects.searchEntries` with fuzzy highlights, opening the file in the right panel and revealing it in the tree), "Search project contents" (Ctrl+Shift+F: `projects.searchContents` with case, whole-word and regex options, results grouped by file with sticky headers, highlighted lines and match marks; opening a match centers and selects its line), settings results while typing (ranked like `filterCommandPaletteGroups`), and the pull requests, usage (Ctrl+U) and settings screens |
| Header | Breadcrumb, Add action, Open, git quick action from live VCS status, Initialize Git, panel toggles |
| Timeline | User bubbles, assistant markdown, settled-turn folds ("Worked for…"), tool rows and group summaries, changed-files card, working indicator, hover meta (time, copy) |
| Markdown | Paragraphs, headings, lists, inline code chips, links, quotes, tables, fenced code with the pierre-dark theme |
| Composer | Prompt editor, model picker, traits and runtime menus, send (`thread.turn.start`), stop, new-thread drafts (`bootstrap.createThread`), checkout strip |
| Requests | Approval banner (approve/decline) and user-input questions (options, multi-step answers) |
| Right panel | Per-thread panel with the "Open a surface" launcher (letter shortcuts), surface tabs and the "+" surface menu; the Files surface (project tree from `projects.listEntries`, search, read-only highlighted file view with line numbers, wrapping and the tree toggle); the Terminal surface (`terminal.attach` stream into a VT screen with scrollback, alternate screen and 16/256/truecolor SGR, Ghostty's default palette, keyboard and paste through `terminal.write`, resizing, new and close; a locally installed Symbols Nerd Font is used for prompt glyphs) |
| Diff panel | Working tree, branch and per-turn scopes (`review.getDiffPreview`, `orchestration.getTurnDiff`), unified or split view, change blocks realigned by similarity and word-alt emphasis like @pierre/diffs, the file tree, collapse, wrap and whitespace toggles, resizable panel |
| Settings | Settings sidebar and header; the General page (new-thread defaults, organization, behavior, projects & threads, confirmations, text generation, about, diagnostics, legacy features) saving server settings with `server.updateSettings`, with the model and traits pickers for the default and text generation models; the Source Control page (repository defaults, `server.discoverSourceControl` version control and hosting integrations with redacted accounts, text generation style, the writer model switch with its model picker); the Providers page (instance list with status, version and enable switch; the editor's display name with the redacted account, runtime fields, models with favorites, visibility and order; the health check interval); the Integrations page (agent browser access, the desktop-only browser defaults, device hub and hosts from `subscribeDeviceState`, read-only); the Storage page (worktree cleanup and retention rules); the SnapShots page (unavailable outside the desktop app, like the web client); the Connections page as a session without `access:write` sees it (administrative access notice, empty remote environments); the Archive page (unarchive, delete); the Keybindings page (shortcut recording and Save, Custom badges, Reset to default and Remove, conflict warnings, the when-clause builder with its expression field and condition tree, adding bindings and the header search); settings search (the sidebar field and "/" over the ported catalog, opening the page, centering the row and pulsing it) |
| Usage | Header with the metric and period groups and refresh; the Cost and Tokens views (`server.getUsageSummary` merged like upstream: provider summary, daily or hourly chart with hover readout, totals, model and period breakdowns); the Limits view (provider quota windows pooled per driver, share left, pace, reset countdown, `server.refreshProviders`) |
| Pull requests | The page with search, sort, filters (state, involvement, draft, review, checks, project) and provider menus; `pullRequests.list` grouped by involvement and sorted like upstream; rows with state, conflict, checks and review glyphs, line counts, author, repository, labels and update time; loading, empty and error states |
| State | Shell and thread subscriptions; thread events applied like upstream `threadReducer.ts` |

Not ported yet (buttons for these show a "not implemented" notice):

- settings: the Appearance page (built-in themes, contrast and the other
  appearance controls), adding or deleting provider instances, the provider
  accent color picker, provider environment variables and custom models,
  usage limit hubs, provider update installs and setup flows, settings scope
  pickers, the keybindings.json file action, project settings, device
  setup (enabling the hub or agent access, adding hosts), the Git fetch
  interval and Bitbucket credentials panels, and
  the Connections administration (pairing links, client sessions, network
  access, adding remote environments);
- usage: limit sources, reset credits, segment popovers, keyboard shortcuts,
  price overrides and the environment filter;
- the pull request detail and right panels, searching on the hosts (the
  search filters loaded rows), author and label filters, line-count backfill,
  paging past 99 rows per repository and avatars (initials are shown);
- the surfaces other than Diff, Files and Terminal (browser, pull requests,
  agents, device), file editing, previews and "Open in editor", and terminal
  splits, the bottom drawer, selection and copy, links and font settings;
- the branch diff's base-ref picker and expanding unchanged diff context;
- attachments, queued messages, project favicons, palette
  sub-actions (themes, git clone; "Go to file" and content search from a
  draft without a thread), and "New thread on <branch>"
  branch carry-over (it opens a draft in the thread's project on its current
  checkout).

Environment themes map their colors onto the app tokens like the web client's
`html[data-theme-id]` rule. Surfaces measured as white tints of the stock dark
palette become tints of the theme's text color over its canvas, which
approximates the upstream mixes. Built-in theme ids keep the stock palette, and
clearing the environment theme returns to it (upstream keeps the last theme).

The Usage page's metric and period live in memory (upstream: local storage).
Client settings persist in `$XDG_CONFIG_HOME/t3-code-rust/client-settings.json`
(upstream: local storage). The project scope and read/unread markers live in
memory; upstream persists them in its UI state store.

Known differences: syntect grammars tokenize differently from Shiki (Rust
keywords, operators and identifiers are recolored to Shiki's scopes; other
languages may still differ);
text is rasterized by epaint, not the browser, so a few text-sized boxes land
1px off (the card snooze menu, the header buttons next to Initialize Git).
`tabular-nums` is emulated with fixed digit cells, so Geist's tabular "1"
sits about 2px right, and "↻" (missing from Geist) comes from egui's fallback
font, smaller than the browser's. Scrollbars stay visible at rest; upstream
fades them in on hover or scroll. The usage chart readout has no backdrop blur.

## Development

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Visual parity is checked on identical synthetic data:

```sh
scripts/fixture-server.sh                      # disposable server on :3999
ELECTRON=/path/to/electron scripts/reference/shot.sh /tmp/ref.png /
cargo run --example snapshot -- http://127.0.0.1:3999 /tmp/t3-egui-fixture/bearer /tmp/egui.png
```

`scripts/gen-icons.mjs` and `scripts/gen-lang-icons.mjs` regenerate the icon
assets from the upstream packages.

Fonts: Geist (SIL OFL, `assets/fonts/OFL.txt`). Icons: Lucide (ISC,
`assets/icons/LUCIDE-LICENSE`); the `diffs-symbol-*` file status icons come
from @pierre/diffs (Apache-2.0, `assets/PIERRE-DIFFS-LICENSE`). Code theme:
pierre-dark (MIT, `assets/PIERRE-THEME-LICENSE`).
