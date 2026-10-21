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
| Thread menu | The full context menu with Snooze, Auto-settle and Copy submenus, mark unread, filter by project, archive, delete with confirmation |
| Command palette | Ctrl+K actions and recent threads, project and thread search, new thread in project, add project with folder browsing |
| Header | Breadcrumb, Add action, Open, git quick action from live VCS status, Initialize Git, panel toggles |
| Timeline | User bubbles, assistant markdown, settled-turn folds ("Worked for…"), tool rows and group summaries, changed-files card, working indicator, hover meta (time, copy) |
| Markdown | Paragraphs, headings, lists, inline code chips, links, quotes, tables, fenced code with the pierre-dark theme |
| Composer | Prompt editor, model picker, traits and runtime menus, send (`thread.turn.start`), stop, new-thread drafts (`bootstrap.createThread`), checkout strip |
| Requests | Approval banner (approve/decline) and user-input questions (options, multi-step answers) |
| Right panel | Per-thread panel with the "Open a surface" launcher (letter shortcuts), the Diff tab and the "+" surface menu |
| Diff panel | Working tree, branch and per-turn scopes (`review.getDiffPreview`, `orchestration.getTurnDiff`), unified or split view, change blocks realigned by similarity and word-alt emphasis like @pierre/diffs, the file tree, collapse, wrap and whitespace toggles, resizable panel |
| Settings | Settings sidebar and header; the General page (new-thread defaults, organization, behavior, projects & threads, confirmations, text generation, about, diagnostics, legacy features) saving server settings with `server.updateSettings`; the Source Control page (repository defaults, `server.discoverSourceControl` version control and hosting integrations with redacted accounts, text generation style); the Integrations page (agent browser access, the desktop-only browser defaults, device hub and hosts from `subscribeDeviceState`, read-only); the Storage page (worktree cleanup and retention rules); the SnapShots page (unavailable outside the desktop app, like the web client); the Connections page as a session without `access:write` sees it (administrative access notice, empty remote environments); the Archive page (unarchive, delete); the Keybindings page (read-only list) |
| Usage | Header with the metric and period groups and refresh; the Cost and Tokens views (`server.getUsageSummary` merged like upstream: provider summary, daily or hourly chart with hover readout, totals, model and period breakdowns); the Limits view (provider quota windows pooled per driver, share left, pace, reset countdown, `server.refreshProviders`) |
| State | Shell and thread subscriptions; thread events applied like upstream `threadReducer.ts` |

Not ported yet (buttons for these show a "not implemented" notice):

- settings: the Appearance page (built-in themes, contrast and the other
  appearance controls) and the Providers page, settings search and scope
  pickers, the default and source control writer model pickers, keybinding
  editing, project settings, device setup (enabling the hub or agent access,
  adding hosts), the Git fetch interval and Bitbucket credentials panels, and
  the Connections administration (pairing links, client sessions, network
  access, adding remote environments);
- usage: limit sources, reset credits, segment popovers, keyboard shortcuts,
  price overrides and the environment filter;
- the pull requests page and the surfaces other than Diff (browser, terminal,
  files, pull requests, agents, device);
- the branch diff's base-ref picker and expanding unchanged diff context;
- attachments, queued messages, custom snooze times, project favicons, palette
  sub-actions (themes, go to file, git clone), and "New thread on <branch>"
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
